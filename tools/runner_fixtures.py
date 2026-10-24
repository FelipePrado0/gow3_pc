"""One-off: runs the Python runner's own modules (scripts/*.py) over synthetic inputs and stores what they
write in launcher-rust/fixtures/runner/<case>/{input,expected}/, for the Rust runner's byte-for-byte parity tests.

Run from anywhere: python tools/runner_fixtures.py   (needs Pillow and iced_x86)

Every case folder holds input/ (what the module reads) and expected/ (what it wrote). A game folder keeps its
own layout inside input/ (eboot.bin, sce_module/*.prx, sce_sys/param.sfo, dvdroot_ps4/...), so a test can
copy input/ as it is. Text outputs are written with LF; no expected file holds the temporary folder (the
generator stops when one does).

  prepare   input: a synthetic eboot.bin SELF (two PT_LOAD, dynamic, library blob with imports and the four
            relocation kinds, TLS) with sce_module/libc.prx (its _init_env is RET), sce_sys/param.sfo and a
            dvdroot_ps4 folder. expected: boot.bin, eboot.elf, entry.bin, analysis.json (scripts/prepare.py).
  link      input: the same eboot.bin, libc.prx and libSceFios2.prx, and boot.bin from tests/test_probe.package
            (a freshly prepared G3PROBE2 image with the eboot's imports). expected: boot-libc.bin and
            libc-link.json (link_libc.py), boot-linked.bin and link.json (link_modules.py).
  isa       input: boot-linked.bin, a G3PROBE5 image (tests/test_isa_rewrite.boot_linked) whose one executable
            segment is isa_rewrite.fixture_code(). expected: isa-rewrite.bin (equal to tests/data/isa_fixture.bin)
            and sites.json, one entry per site: offset, length, kind and mode names, width, registers
            (dst, src1, src2, base, index; 255 is none), mem_slot, scale, mem_flags, disp, original and
            relocated bytes as hex.
  content   input: param.sfo with the four user-defined parameters. expected: full/ and trial/ with
            content.bin and content-profile.json (content_profile.py).
  patches   input: eboot.elf holding the signatures of tests/test_patches.SignatureTests in its one PT_LOAD,
            eboot.bin and param.sfo (CUSA01623, 01.02: the game folder patches.py looks the profile up in),
            patches.xml (SignatureTests.XML with a TitleID so the mod file merges in) and mod.xml
            (ModPatchTests.MOD). patches.py main() runs with --mod-patches mod.xml --extra "Found;Call;Missing;Data"
            and SPARTAN_PATCHES_ONLY=1. expected: patches-merged.xml, patches.bin, patch_status.json (Missing is
            skipped: its second signature is absent).
  graphics  input: spartanport.ini, graphics_state.json ({}) and edit.json, the keys written between the
            launches. graphics_options.prepare runs three times, as in
            test_unconfirmed_restart_is_tried_once_then_reverted; expected/<good|tried|reverted>/ holds the ini,
            the state file and environment.json after each launch.
  dxt       input: gradient, edges, alpha, noise and flat, 64 x 64 RGBA PNGs (noise from a fixed seed), and
            odd, a 5 x 3 RGBA gradient whose blocks extend past the picture (Pillow's padding).
            expected: <name>.dxt1 and <name>.dxt5, the 4-row strips of glyphs.encode_region concatenated with
            each DDS header (128 bytes) stripped (RGB for DXT1, RGBA for DXT5).
  icon      expected: window_icon.bmp, launcher/spartanport.png as run.py makes it (RGBA, 64 x 64 LANCZOS, BMP).
  glyphs    input: <texture>.png for each lyre and gold texture of glyphs.TEXTURES, a synthetic level texture
            (1024 px, the glow mask 512) with four discs in the lyre's corner: marble with a bright PlayStation
            shape, gold with a dark engraved one, domes with a raised ring (normal map), black with a bright
            shape (glow mask); flat elsewhere. expected: <style>/<texture>.png for the xbox, nintendo and
            keyboard styles with the default maps, the Python's own drawing: glyphs.render at the sizes of
            tests/test_glyphs.wad (inline 64, the select icon 64 x 40, QTE 128, atlas 256) and the controller
            diagram at 512 x 256, glyphs.lyre_paint of the input for the lyre and gold textures. The bytes
            written from those pictures: encode/<style>/<texture>.<fourcc>, glyphs.encode of the PNG (its mip
            chain: the DXT1 QTE cross in the 7 levels of tests/test_glyphs.wad, the DXT5 select key down to
            1 x 1); encode_region/xbox/TXEACA5E71.dxt1, a DDS header and the glow mask's whole chain of 0xee
            bytes after glyphs.encode_region of the painted PNG over the input (the lyre's corner)."""
import contextlib
import io
import json
import math
import os
import random
import shutil
import struct
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / 'launcher-rust' / 'fixtures' / 'runner'
WORK = Path(tempfile.mkdtemp(prefix='runner_fixtures_'))

sys.path.insert(0, str(ROOT / 'tests'))
sys.path.insert(0, str(ROOT / 'scripts'))
import content_profile  # noqa: E402
import glyphs  # noqa: E402
import graphics_options  # noqa: E402
import isa_rewrite  # noqa: E402
import link_libc  # noqa: E402
import link_modules  # noqa: E402
import patches  # noqa: E402
import prepare as prepare_module  # noqa: E402
from PIL import Image  # noqa: E402
from test_isa_rewrite import boot_linked  # noqa: E402
from test_patches import ModPatchTests, SignatureTests  # noqa: E402
from test_probe import package  # noqa: E402

FS_LOAD = bytes.fromhex('64488b042500000000')  # mov rax, fs:[0]: what the linkers rewrite to GS
PARAMS = {'USER_DEFINED_PARAM_1': 13, 'USER_DEFINED_PARAM_2': 0, 'USER_DEFINED_PARAM_3': 7,
          'USER_DEFINED_PARAM_4': 0xffffffff}
EDIT = {'render_resolution': '4K', 'engine_fps': '240', 'restart_unconfirmed': '1'}


# -- Synthetic files ----------------------------------------------------------------------------------------

def sfo_bytes(values):
    """A param.sfo (PSF 1.1): strings (0x204) and unsigned integers (0x404), keys sorted."""
    keys, data, entries = b'', b'', []
    for key, value in sorted(values.items()):
        if isinstance(value, int):
            fmt, raw, capacity = 0x404, struct.pack('<I', value), 4
        else:
            raw = value.encode('utf-8') + b'\0'
            fmt, capacity = 0x204, (len(raw) + 3) // 4 * 4
        entries.append((len(keys), fmt, len(raw), capacity, len(data)))
        keys += key.encode() + b'\0'
        data += raw.ljust(capacity, b'\0')
    key_table = 20 + 16 * len(entries)
    data_table = (key_table + len(keys) + 3) // 4 * 4
    head = struct.pack('<4sIIII', b'\0PSF', 0x101, key_table, data_table, len(entries))
    return (head + b''.join(struct.pack('<HHIII', *e) for e in entries)
            + keys.ljust(data_table - key_table, b'\0') + data)


class Strings:
    """A string table: an empty string at offset 0, then each text once."""

    def __init__(self):
        self.data, self.at = bytearray(b'\0'), {}

    def add(self, text):
        if text not in self.at:
            self.at[text] = len(self.data)
            self.data += text.encode() + b'\0'
        return self.at[text]


def symbol(strings, name, info, section=0, value=0, size=0):
    return struct.pack('<IBBHQQ', strings.add(name) if name else 0, info, 0, section, value, size)


def relocation(target, kind, sym=0, addend=0):
    return struct.pack('<QQq', target, (sym << 32) | kind, addend)


def identifier(ident, version, name_offset):
    """A library or module dynamic tag value: id in the top 16 bits, version, name offset."""
    return (ident << 48) | (version << 32) | name_offset


def library_blob(strings, symbols, rela, plt):
    """The 0x61000000 segment (strings, symbols, relocation tables) and the dynamic tags that locate them."""
    tags, blob = {}, b''
    for names, content in ((('str', 0x61000035, 0x61000037), bytes(strings.data)),
                           (('sym', 0x61000039, 0x6100003f), b''.join(symbols)),
                           (('rela', 0x61000029, 0x6100002d), b''.join(rela)),
                           (('plt', 0x6100002f, 0x61000031), b''.join(plt))):
        tags[names[1]], tags[names[2]] = len(blob), len(content)
        blob += content.ljust((len(content) + 7) // 8 * 8, b'\0')
    return blob, [(tag, value) for tag, value in tags.items()]


def dynamic_bytes(tags):
    return b''.join(struct.pack('<QQ', *t) for t in tags + [(0, 0)])


def make_self(segments, e_type, entry=0):
    """A SELF around an ELF with these program headers. A segment is a dict: type, flags, vaddr, memsz, align,
    data (its file bytes), covered (stored in the SELF; the others are metadata the unpacker reports as
    unavailable) and, for a segment inside another one's bytes, inside (its index) and at (offset in it)."""
    cursor = 0x1000
    for s in segments:
        if 'inside' in s:
            s['offset'] = segments[s['inside']]['offset'] + s['at']
        else:
            s['offset'] = cursor
            cursor = (cursor + len(s['data']) + 0xff) // 0x100 * 0x100
    phdrs = b''.join(struct.pack('<IIQQQQQQ', s['type'], s['flags'], s['offset'], s['vaddr'], s['vaddr'],
                                 len(s['data']), s['memsz'], s['align']) for s in segments)
    elf_head = struct.pack('<16sHHIQQQIHHHHHH', b'\x7fELF\x02\x01\x01\x09', e_type, 62, 1, entry, 64, 0, 0,
                           64, 56, len(segments), 0, 0, 0) + phdrs
    stored = [(i, s) for i, s in enumerate(segments) if s.get('covered') and s['data']]
    base = 32 + 32 * len(stored)
    position = (base + len(elf_head) + 0xff) // 0x100 * 0x100
    table, payload = b'', b''
    for i, s in stored:
        table += struct.pack('<QQQQ', 0x800 | (i << 20), position + len(payload), len(s['data']), len(s['data']))
        payload += s['data']
    head = (b'O\x15=\x1d' + bytes(20) + struct.pack('<H', len(stored))).ljust(32, b'\0')
    return (head + table + elf_head).ljust(position, b'\0') + payload


def load(vaddr, flags, data, memsz, **extra):
    return dict(type=1, flags=flags, vaddr=vaddr, memsz=memsz, align=0x4000, data=data, covered=True, **extra)


def code_page(size, pieces):
    page = bytearray(size)
    for at, code in pieces:
        page[at:at + len(code)] = code
    return bytes(page)


def eboot_parts():
    """(SELF bytes, import names, code): the synthetic eboot of the prepare and link cases. It imports _init_env
    (a function the libc exports), an unknown function and data object, and defines one symbol of its own."""
    nid = prepare_module.nid
    strings = Strings()
    needed, libc = strings.add('libc.prx'), strings.add('libc')
    names = [nid('_init_env') + '#q#q', 'AAAAAAAAAAA#q#q']
    symbols = [symbol(strings, '', 0),
               symbol(strings, names[0], 0x12),                       # import, function
               symbol(strings, names[1], 0x11),                       # import, object
               symbol(strings, 'BBBBBBBBBBB#q#q', 0x12, 1, 0x40, 8),  # defined here
               symbol(strings, 'CCCCCCCCCCC#q#q', 0x11, 0xfff1, 0x1234)]  # absolute
    rela = [relocation(0x800, 8, 0, 0x40), relocation(0x808, 6, 3, 0x10), relocation(0x810, 1, 4, 4)]
    plt = [relocation(0x818, 7, 1), relocation(0x820, 6, 2), relocation(0x828, 7, 1)]
    blob, tags = library_blob(strings, symbols, rela, plt)
    tags = [(1, needed), (0x61000013, identifier(42, 1, libc)), (0x6100000d, identifier(42, 1, libc))] + tags
    code = code_page(0x800, [(0x10, bytes.fromhex('31c0c3')), (0x20, FS_LOAD + b'\x90'), (0x40, FS_LOAD + b'\xc3')])
    data = bytes(i & 255 for i in range(0x800))
    segments = [load(0, 5, code, 0x800), load(0x800, 6, data, 0x800),
                dict(type=2, flags=4, vaddr=0, memsz=0, align=8, data=dynamic_bytes(tags), covered=True),
                dict(type=0x61000000, flags=4, vaddr=0, memsz=0, align=8, data=blob, covered=True),
                dict(type=0x61000001, flags=4, vaddr=0x80, memsz=0x40, align=8, data=b''),
                dict(type=7, flags=4, vaddr=0xc00, memsz=0x30, align=16, data=data[0x400:0x410],
                     inside=1, at=0x400)]
    return make_self(segments, 0xfe10, 0x10), names, code


def libc_self():
    """A libc.prx: _init_env is RET, one more function, one object, a TLS template, base-relative, TLS, import
    and defined-symbol relocations and an init function (dynamic tag 12)."""
    nid = prepare_module.nid
    strings = Strings()
    name = strings.add('libc')
    symbols = [symbol(strings, '', 3),
               symbol(strings, nid('_init_env') + '#C#A', 0x12, 1, 0x10, 1),
               symbol(strings, 'DDDDDDDDDDD#C#A', 0x12),
               symbol(strings, nid('_ZNSt8ios_base4InitC1Ev') + '#C#A', 0x12, 1, 0x20, 3),
               symbol(strings, 'EEEEEEEEEEE#C#A', 0x11, 1, 0x600, 8)]
    rela = [relocation(0x400, 8, 0, 0x10), relocation(0x408, 16), relocation(0x410, 1, 1), relocation(0x418, 6, 2)]
    plt = [relocation(0x420, 7, 2)]
    blob, tags = library_blob(strings, symbols, rela, plt)
    tags = [(12, 0x10), (0x61000013, identifier(2, 1, name)), (0x6100000d, identifier(0, 1, name))] + tags
    code = code_page(0x800, [(0x10, b'\xc3'), (0x20, bytes.fromhex('31c0c3')), (0x30, FS_LOAD + b'\xc3')])
    segments = [load(0, 5, code, 0x1000),
                dict(type=2, flags=4, vaddr=0, memsz=0, align=8, data=dynamic_bytes(tags), covered=True),
                dict(type=0x61000000, flags=4, vaddr=0, memsz=0, align=8, data=blob, covered=True),
                dict(type=7, flags=4, vaddr=0x500, memsz=0x30, align=16, data=bytes(range(16)),
                     inside=0, at=0x500)]
    return make_self(segments, 0xfe18)


def fios_self():
    """A libSceFios2.prx: one export, a weak undefined module_start, an import served by libc's same-NID export
    (library libSceLibcInternal), a defined-symbol and a base-relative relocation; no TLS."""
    strings = Strings()
    fios, internal = strings.add('libSceFios2'), strings.add('libSceLibcInternal')
    symbols = [symbol(strings, '', 0),
               symbol(strings, 'Ov1r5aKFXlE#D#B', 0x12, 1, 0x10, 1),
               symbol(strings, 'module_start', 0x20),
               symbol(strings, prepare_module.nid('_init_env') + '#E#F', 0x12)]
    rela = [relocation(0x200, 8, 0, 0x20), relocation(0x208, 1, 2), relocation(0x210, 7, 3), relocation(0x218, 6, 1)]
    blob, tags = library_blob(strings, symbols, rela, [])
    tags = [(0x61000013, identifier(3, 1, fios)), (0x61000013, identifier(4, 1, internal)),
            (0x6100000d, identifier(1, 1, fios)), (0x6100000d, identifier(5, 1, internal))] + tags
    code = code_page(0x400, [(0x10, b'\xc3'), (0x30, FS_LOAD + b'\xc3')])
    segments = [load(0, 5, code, 0x800),
                dict(type=2, flags=4, vaddr=0, memsz=0, align=8, data=dynamic_bytes(tags), covered=True),
                dict(type=0x61000000, flags=4, vaddr=0, memsz=0, align=8, data=blob, covered=True)]
    return make_self(segments, 0xfe18)


# -- Helpers ------------------------------------------------------------------------------------------------

def put(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return path


def text(value):
    return json.dumps(value, indent=2, ensure_ascii=False) + '\n'


def lf(path):
    """A text file as the module wrote it, with LF: Windows text-mode writes give CRLF, the Rust runner writes \\n."""
    return path.read_bytes().replace(b'\r\n', b'\n')


def quiet(function, *args):
    """function(*args) without its progress lines on the console."""
    with contextlib.redirect_stdout(io.StringIO()):
        return function(*args)


def case_folder(name):
    folder = FIXTURES / name
    shutil.rmtree(folder, ignore_errors=True)
    return folder / 'input', folder / 'expected'


def stage(game, files):
    """Writes {relative path: bytes} under game."""
    for relative, data in files.items():
        put(game / relative, data)
    return game


# -- Cases --------------------------------------------------------------------------------------------------

EBOOT, IMPORT_NAMES, EBOOT_CODE = eboot_parts()
LIBC, FIOS = libc_self(), fios_self()
PARAM_SFO = sfo_bytes({'TITLE_ID': 'CUSA01623', 'TITLE': 'ゴッド・オブ・ウォーIII リマスタード', 'APP_VER': '01.02',
                       'CONTENT_ID': 'EP9000-CUSA01623_00-GOW3REMASTERED00', **PARAMS})
GAME_FILES = {'eboot.bin': EBOOT, 'sce_module/libc.prx': LIBC, 'sce_sys/param.sfo': PARAM_SFO,
              'dvdroot_ps4/PART1.WAD': b'WAD3' + bytes(range(28)), 'dvdroot_ps4/sound/music.bin': bytes(100)}


def case_prepare():
    inputs, expected = case_folder('prepare')
    stage(inputs, GAME_FILES)
    out = WORK / 'prepare-out'
    quiet(prepare_module.prepare, inputs, out)
    for name in ('boot.bin', 'eboot.elf', 'entry.bin'):
        put(expected / name, (out / name).read_bytes())
    put(expected / 'analysis.json', lf(out / 'analysis.json'))
    return out


def case_link(prepared):
    inputs, expected = case_folder('link')
    stage(inputs, {'eboot.bin': EBOOT, 'sce_module/libc.prx': LIBC, 'sce_module/libSceFios2.prx': FIOS})
    relocs = [(0x800, 0, 0x40, 0), (0x818, 1, 0, 0), (0x820, 2, 1, 0)]
    boot = package(EBOOT_CODE, relocs, IMPORT_NAMES, capabilities=1)
    put(inputs / 'boot.bin', boot)
    for linker, outputs in ((link_libc, ('boot-libc.bin', 'libc-link.json')),
                            (link_modules, ('boot-linked.bin', 'link.json'))):
        out = WORK / f'{linker.__name__}-out'
        put(out / 'boot.bin', boot)
        quiet(linker.link, inputs, out)
        for name in outputs:
            put(expected / name, lf(out / name) if name.endswith('.json') else (out / name).read_bytes())


def site_entry(site):
    kinds = {value: name[5:] for name, value in vars(isa_rewrite).items() if name.startswith('KIND_')}
    modes = {value: name[5:] for name, value in vars(isa_rewrite).items() if name.startswith('MODE_')}
    return dict(offset=site.offset, length=site.length, kind=kinds[site.kind], width=site.width,
                mode=modes[site.mode],
                registers=dict(dst=site.dst, src1=site.src1, src2=site.src2, base=site.base, index=site.index),
                mem_slot=site.mem_slot, scale=site.scale, mem_flags=site.mem_flags, disp=site.disp,
                original=site.original.hex(), relocated=site.relocated.hex())


def case_isa():
    inputs, expected = case_folder('isa')
    code = isa_rewrite.fixture_code()
    boot = put(inputs / 'boot-linked.bin', boot_linked([(code, len(code), 5)]))
    out = WORK / 'isa-out'
    out.mkdir()
    if quiet(isa_rewrite.main, [str(boot), '--out', str(out)]) != 0:
        raise SystemExit('isa: the rewrite table could not be made')
    table = (out / 'isa-rewrite.bin').read_bytes()
    if table != (ROOT / 'tests/data/isa_fixture.bin').read_bytes():
        raise SystemExit('isa: the table differs from tests/data/isa_fixture.bin')
    put(expected / 'isa-rewrite.bin', table)
    put(expected / 'sites.json', text([site_entry(s) for s in isa_rewrite.scan_segment(code, 0)]).encode())


def case_content():
    inputs, expected = case_folder('content')
    sfo = put(inputs / 'param.sfo', sfo_bytes({'TITLE_ID': 'CUSA01623', 'APP_VER': '01.02', **PARAMS}))
    game = stage(WORK / 'content-game', {'sce_sys/param.sfo': sfo.read_bytes()})
    for sku in ('full', 'trial'):
        out = WORK / f'content-{sku}'
        quiet(content_profile.prepare, game, out, sku)
        put(expected / sku / 'content.bin', (out / 'content.bin').read_bytes())
        put(expected / sku / 'content-profile.json', lf(out / 'content-profile.json'))


def case_patches():
    inputs, expected = case_folder('patches')
    tests = SignatureTests('test_sites_found_anywhere_and_rel32_recomputed')
    tests.setUp()  # the image with the signatures planted; its temporary folder goes at once
    image = tests.image
    tests.doCleanups()
    # One PT_LOAD at file offset 0x100, virtual address 0, as patches.eboot_segments/eboot_image read it.
    head = struct.pack('<16sHHIQQQIHHHHHH', b'\x7fELF\x02\x01\x01\x09', 2, 62, 1, 0, 64, 0, 0, 64, 56, 1, 0, 0, 0)
    head += struct.pack('<IIQQQQQQ', 1, 5, 0x100, 0, 0, len(image), len(image), 0x1000)
    put(inputs / 'eboot.elf', head.ljust(0x100, b'\0') + image)
    put(inputs / 'eboot.bin', b'synthetic eboot.bin of the patches case\n')
    put(inputs / 'param.sfo', sfo_bytes({'TITLE_ID': 'CUSA01623', 'APP_VER': '01.02', 'TITLE': 'God of War III'}))
    xml = SignatureTests.XML.replace('<Patch>', '<Patch>\n  <TitleID><ID>CUSA01623</ID></TitleID>', 1)
    put(inputs / 'patches.xml', xml.encode())
    put(inputs / 'mod.xml', ModPatchTests.MOD.encode())

    game = stage(WORK / 'patches-game', {'eboot.bin': (inputs / 'eboot.bin').read_bytes(),
                                         'sce_sys/param.sfo': (inputs / 'param.sfo').read_bytes()})
    out = WORK / 'patches-out'
    put(out / 'eboot.elf', (inputs / 'eboot.elf').read_bytes())
    saved = patches.GAMES, sys.argv, os.environ.get('SPARTAN_PATCHES_ONLY')
    patches.GAMES = [({'CUSA01623'}, WORK / 'patches.xml', '01.02', 'God of War III')]
    put(WORK / 'patches.xml', xml.encode())
    os.environ['SPARTAN_PATCHES_ONLY'] = '1'
    sys.argv = ['patches.py', '--out', str(out), '--game-dir', str(game), '--mod-patches', str(inputs / 'mod.xml'),
                '--extra', 'Found;Call;Missing;Data']
    try:
        quiet(patches.main)
    finally:
        patches.GAMES, sys.argv = saved[0], saved[1]
        if saved[2] is None:
            del os.environ['SPARTAN_PATCHES_ONLY']
        else:
            os.environ['SPARTAN_PATCHES_ONLY'] = saved[2]
    put(expected / 'patches-merged.xml', lf(out / 'patches-merged.xml'))
    put(expected / 'patches.bin', (out / 'patches.bin').read_bytes())
    put(expected / 'patch_status.json', lf(out / 'patch_status.json'))


def case_graphics():
    inputs, expected = case_folder('graphics')
    put(inputs / 'spartanport.ini', b'render_resolution=1440p\nengine_fps=120\n')
    put(inputs / 'graphics_state.json', b'{}\n')
    put(inputs / 'edit.json', text(EDIT).encode())
    work = WORK / 'graphics'
    work.mkdir()
    ini, state = work / 'spartanport.ini', work / 'graphics_state.json'
    shutil.copyfile(inputs / 'spartanport.ini', ini)
    shutil.copyfile(inputs / 'graphics_state.json', state)
    for step in ('good', 'tried', 'reverted'):
        if step == 'tried':
            graphics_options.write_keys(ini, EDIT)  # the in-game menu asking for a restart
        options, _ = graphics_options.prepare(ini, state)
        put(expected / step / 'spartanport.ini', lf(ini))
        put(expected / step / 'graphics_state.json', lf(state))
        put(expected / step / 'environment.json', text(graphics_options.environment(options)).encode())


def test_images():
    """The five 64 x 64 RGBA test images."""
    size = 64
    noise = random.Random(20260101)
    drawings = {
        'gradient': lambda x, y: (x * 4, y * 4, (x + y) * 2, 255),
        'edges': lambda x, y: ((255, 255, 255, 255) if ((x + 1) // 5 + (y + 3) // 7 + (x == y) * 2) % 2 == 0
                               else (20, 30, 40, 255)),
        'alpha': lambda x, y: (230, 90 + y * 2, 40, x * 4),
        'noise': lambda x, y: tuple(noise.randrange(256) for _ in range(4)),
        'flat': lambda x, y: (200, 100, 50, 255),
    }
    images = {}
    for name, draw in drawings.items():
        image = Image.new('RGBA', (size, size))
        image.putdata([draw(x, y) for y in range(size) for x in range(size)])
        images[name] = image
    return images


def odd_image():
    """A 5 x 3 RGBA gradient: both of its blocks reach past the bottom edge, the second past the right one."""
    image = Image.new('RGBA', (5, 3))
    image.putdata([(40 + x * 50, 200 - y * 70, 30 + x * y * 20, 255 - x * 45 - y * 30)
                   for y in range(3) for x in range(5)])
    return image


def strips(image, fourcc):
    """glyphs.encode_region's 4-row strips of the whole picture, DDS headers stripped, concatenated."""
    image = image.convert('RGBA' if fourcc == 'DXT5' else 'RGB')
    width, height = image.size
    encoded = b''
    for y in range(0, height, 4):
        strip = io.BytesIO()
        image.crop((0, y, width, min(y + 4, height))).save(strip, 'DDS', pixel_format=fourcc)
        encoded += strip.getvalue()[128:]
    return encoded


def encode_region_blocks(image, fourcc):
    """The same bytes through glyphs.encode_region itself, over a texture holding just the top level
    (partial blocks included)."""
    block = 16 if fourcc == 'DXT5' else 8
    width, height = image.size
    columns, rows = -(-width // 4), -(-height // 4)
    data = bytearray(128 + columns * rows * block)
    struct.pack_into('<II', data, 12, height, width)
    glyphs.encode_region(data, 0, len(data), image, fourcc, (0, 0, width, height))
    return bytes(data[128:])


def case_dxt():
    inputs, expected = case_folder('dxt')
    for name, image in {**test_images(), 'odd': odd_image()}.items():
        inputs.mkdir(parents=True, exist_ok=True)
        image.save(inputs / f'{name}.png')
        for fourcc in ('DXT1', 'DXT5'):
            blocks = strips(image, fourcc)
            if blocks != encode_region_blocks(image, fourcc):
                raise SystemExit(f'dxt: {name} {fourcc} differs from glyphs.encode_region')
            put(expected / f'{name}.{fourcc.lower()}', blocks)


def case_icon():
    _, expected = case_folder('icon')
    expected.mkdir(parents=True)
    with Image.open(ROOT / 'launcher' / 'spartanport.png') as icon:  # run.py: the window icon
        icon.convert('RGBA').resize((64, 64), Image.LANCZOS).save(expected / 'window_icon.bmp')


GLYPH_STYLES = ('xbox', 'nintendo', 'keyboard')
# tests/test_glyphs.wad()'s sizes, the select icon's included; the controller diagram at half its picture.
GLYPH_SIZES = {'inline': (64, 64), 'qte': (128, 128), 'atlas': (256, 256), 'controller': (512, 256)}
GLYPH_SIZE_OF = {'TXD43D1443': (64, 40)}
LYRE_SIZE_OF = {'glow': 512}  # the other kinds: 1024
# (style, texture, fourcc, mip levels or None for the chain down to 1 x 1) for glyphs.encode
GLYPH_ENCODES = (('xbox', 'TX7FF61BF7', 'DXT1', 7), ('keyboard', 'TXD43D1443', 'DXT5', None))
GLYPH_REGION = ('xbox', 'TXEACA5E71')  # the glow mask (DXT1, 512 px) for glyphs.encode_region


def mip_chain(width, height, fourcc, levels=None):
    """The bytes of the DXT mip levels of a width x height texture: `levels` of them, or down to 1 x 1."""
    length, level = 0, 0
    while levels is None or level < levels:
        w, h = max(1, width >> level), max(1, height >> level)
        length += -(-w // 4) * -(-h // 4) * (16 if fourcc == 'DXT5' else 8)
        if (w, h) == (1, 1):
            break
        level += 1
    return length


def lyre_input(form, kind, size):
    """A synthetic level texture for glyphs.lyre_paint: four discs in the lyre's corner, each with its
    PlayStation shape; flat outside the corner."""
    from PIL import ImageDraw
    shrink = 2048 // size
    x0, y0, x1, y1 = (v // shrink for v in glyphs.LYRE_CORNER)
    radius = glyphs.LYRE_RADIUS // shrink
    width, height = x1 - x0, y1 - y0
    normal = kind == 'normal'
    if normal:
        corner = Image.new('RGBA', (width, height), (128, 128, 255, 128))
    elif kind in ('glow', 'decal'):
        corner = Image.new('RGB', (width, height), (0, 0, 0))
    else:
        base = (64, 58, 52) if kind == 'diffuse' else (40, 40, 40)
        corner = Image.new('RGB', (width, height))
        corner.putdata([tuple(max(0, min(255, c + int(18 * math.sin(x / 9.0) + 14 * math.cos(y / 13.0 + x / 31.0))))
                              for c in base) for y in range(height) for x in range(width)])
    draw = ImageDraw.Draw(corner)
    line = max(2, radius // 6)
    for button, (cx, cy) in glyphs.LYRE_DISCS.items():
        cx, cy = cx // shrink, cy // shrink
        r = radius * 0.55
        if normal:
            pixels = corner.load()
            for y in range(cy - radius, cy + radius):
                for x in range(cx - radius, cx + radius):
                    dx, dy = x + 0.5 - cx, y + 0.5 - cy
                    distance = math.hypot(dx, dy)
                    if distance < radius:
                        tilt = 50 * distance / radius + (25 if abs(distance - r) < line else 0)
                        pixels[x, y] = (128, 128 + int(tilt * dy / max(distance, 1)), 255,
                                        128 - int(tilt * dx / max(distance, 1)))
            continue
        if form == 'gold':
            disc, mark = ((156, 116, 60) if kind == 'diffuse' else (120, 120, 120)), (52, 36, 20)
        elif kind == 'diffuse':
            disc, mark = (40, 37, 34), (150, 138, 118)
        elif kind == 'specular':
            disc, mark = (26, 26, 26), (130, 130, 130)
        else:
            disc, mark = (0, 0, 0), (210, 190, 160)
        draw.ellipse((cx - radius, cy - radius, cx + radius, cy + radius), fill=disc)
        for ring in range(radius // 3, radius, max(2, radius // 8)):  # a little texture on the disc
            shade = tuple(max(0, c - 6) for c in disc)
            draw.ellipse((cx - ring, cy - ring, cx + ring, cy + ring), outline=shade, width=1)
        if button == 'cross':
            draw.line([(cx - r, cy - r), (cx + r, cy + r)], fill=mark, width=line)
            draw.line([(cx - r, cy + r), (cx + r, cy - r)], fill=mark, width=line)
        elif button == 'circle':
            draw.ellipse((cx - r, cy - r, cx + r, cy + r), outline=mark, width=line)
        elif button == 'square':
            draw.rectangle((cx - r * 0.8, cy - r * 0.8, cx + r * 0.8, cy + r * 0.8), outline=mark, width=line)
        else:
            draw.polygon([(cx, cy - r), (cx - r, cy + r * 0.7), (cx + r, cy + r * 0.7)], outline=mark, width=line)
    image = Image.new(corner.mode, (size, size), corner.getpixel((0, height - 1)))
    image.paste(corner, (x0, y0))
    return image


def case_glyphs():
    inputs, expected = case_folder('glyphs')
    inputs.mkdir(parents=True)
    lyre = {}
    for name, (form, kind) in glyphs.TEXTURES.items():
        if form in glyphs.LYRE_FORMS:
            lyre[name] = lyre_input(form, kind, LYRE_SIZE_OF.get(kind, 1024))
            lyre[name].save(inputs / f'{name}.png')
    for style in GLYPH_STYLES:
        drawn = dict(glyphs.textures(style))
        if style in ('xbox', 'nintendo'):
            drawn[glyphs.CONTROLLER_TEXTURE] = ('controller', None)
        (expected / style).mkdir(parents=True)
        for name, (form, buttons) in drawn.items():
            if form in glyphs.LYRE_FORMS:
                picture = glyphs.lyre_paint(lyre[name], form, buttons, style, None, '')
            else:
                width, height = GLYPH_SIZE_OF.get(name, GLYPH_SIZES[form])
                picture = glyphs.render(form, buttons, style, None, '', width, height, '')
            picture.save(expected / style / f'{name}.png')
    for style, name, fourcc, levels in GLYPH_ENCODES:
        with Image.open(expected / style / f'{name}.png') as picture:
            length = mip_chain(picture.width, picture.height, fourcc, levels)
            put(expected / 'encode' / style / f'{name}.{fourcc.lower()}', glyphs.encode(picture, fourcc, length))
    style, name = GLYPH_REGION
    with Image.open(inputs / f'{name}.png') as original, Image.open(expected / style / f'{name}.png') as painted:
        width, height = original.size
        data = bytearray(b'DDS ' + bytes(8) + struct.pack('<II', height, width) + bytes(108))
        data += b'\xee' * mip_chain(width, height, 'DXT1')
        region = tuple(v // (2048 // width) for v in glyphs.LYRE_CORNER)
        glyphs.encode_region(data, 0, len(data), painted, 'DXT1', region, original)
        put(expected / 'encode_region' / style / f'{name}.dxt1', bytes(data))


def check_no_temp_folder():
    for path in FIXTURES.rglob('*'):
        if path.is_file():
            data = path.read_bytes()
            for spelling in {str(WORK), str(WORK).replace('\\', '/'), WORK.name}:
                if spelling.encode() in data:
                    raise SystemExit(f'{path.relative_to(ROOT)} holds the temporary folder {spelling}')


if __name__ == '__main__':
    try:
        FIXTURES.mkdir(parents=True, exist_ok=True)
        prepared = case_prepare()
        for case in (lambda: case_link(prepared), case_isa, case_content, case_patches, case_graphics, case_dxt,
                     case_icon, case_glyphs):
            case()
        check_no_temp_folder()
        for folder in sorted(p for p in FIXTURES.iterdir() if p.is_dir()):
            print('wrote', folder.relative_to(ROOT))
    finally:
        shutil.rmtree(WORK, ignore_errors=True)
