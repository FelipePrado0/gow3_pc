from paths import ROOT
import os
import struct
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

from patches import (compile_patches, external_patches, external_selection, compile_external,
                     game_profile, selected_patches, patch_requirements, GAMES, merge_patch_files,
                     resolution_names, default_enabled)

GOW3 = ROOT / 'patches/God_of_War_III_Remastered.xml'
ULTRAWIDE = ROOT / 'extras/mods/Ultrawide/patches/Ultrawide.xml'
GOW3_IDS = {'CUSA01623'}
SEGMENTS = [(0, 0x6000000)]


EXTERNAL = """<?xml version="1.0"?>
<Patch>
  <TitleID><ID>CUSA01623</ID></TitleID>
  <Metadata Title="God of War III" Name="On" Author="x" PatchVer="1.0" AppVer="01.02" AppElf="eboot.bin" isEnabled="true">
    <PatchList><Line Type="bytes" Address="0x00401000" Value="9090"/></PatchList>
  </Metadata>
  <Metadata Title="God of War III" Name="Off" Author="x" PatchVer="1.0" AppVer="01.02" AppElf="eboot.bin">
    <PatchList><Line Type="bytes32" Address="0x00402000" Value="0x12345678"/></PatchList>
  </Metadata>
  <Metadata Title="God of War III" Name="Mask" Author="x" PatchVer="1.0" AppVer="01.02" AppElf="eboot.bin" isEnabled="true">
    <PatchList><Line Type="mask" Value="90 ?? 90" Offset="0"/></PatchList>
  </Metadata>
  <Metadata Title="God of War III" Name="Old" Author="x" PatchVer="1.0" AppVer="01.00" AppElf="eboot.bin" isEnabled="true">
    <PatchList><Line Type="bytes" Address="0x00403000" Value="90"/></PatchList>
  </Metadata>
</Patch>"""


class ExternalPatchTests(unittest.TestCase):
    def test_selection_and_unsupported_lines(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            (folder / 'extra.xml').write_text(EXTERNAL)
            (folder / 'other.xml').write_text(EXTERNAL.replace('CUSA01623', 'CUSA99999'))
            (folder / 'broken.xml').write_text('<Patch>')
            found = external_patches(folder, '01.02', GOW3, GOW3_IDS)
            self.assertEqual([key for key, _, _ in found],
                             ['extra.xml/On', 'extra.xml/Off', 'extra.xml/Mask'])
            # The file's isEnabled; the mask patch is skipped as a whole.
            writes = compile_external(external_selection(found, None), SEGMENTS)
            self.assertEqual(writes, [(0x1000, bytes.fromhex('9090'))])
            config = folder / 'patches.json'
            config.write_text('{"enabled": ["extra.xml/Off"], "disabled": ["extra.xml/On"]}')
            writes = compile_external(external_selection(found, config), SEGMENTS)
            self.assertEqual(writes, [(0x2000, (0x12345678).to_bytes(4, 'little'))])

    def test_built_in_file_is_not_external(self):
        self.assertEqual(external_patches(GOW3.parent, '01.02', GOW3, GOW3_IDS), [])


class GameProfileTests(unittest.TestCase):
    def test_profiles_by_title_id(self):
        self.assertEqual(game_profile('CUSA01623')[1:3], (GOW3, '01.02'))
        self.assertIsNone(game_profile('CUSA99999'))
        self.assertIsNone(game_profile(None))

    def test_unavailable_frame_rate_patch_falls_back_to_the_highest_lower_one(self):
        from graphics_options import restrict_to_available
        available = {'Bug Fix - Texture Corruption Fix', 'Frame Rate Patch - 120 FPS'}
        self.assertEqual(restrict_to_available(['Bug Fix - Texture Corruption Fix', 'Frame Rate Patch - 240 FPS'],
                                               available),
                         ['Bug Fix - Texture Corruption Fix', 'Frame Rate Patch - 120 FPS'])
        self.assertEqual(restrict_to_available(['Resolution Patch - 8K'], available), [])

    def test_gow3_defaults_and_extras(self):
        self.assertEqual(selected_patches(GOW3, '01.02', ''),
                         ['Bug Fix - Texture Corruption Fix', 'Skip Any Video With X Button',
                          'Bug Fix - Startup Black Screen'])
        self.assertEqual(selected_patches(GOW3, '01.02', 'Resolution Patch - 720p; Skip Any Video With X Button'),
                         ['Skip Any Video With X Button', 'Bug Fix - Startup Black Screen', 'Resolution Patch - 720p'])
        self.assertEqual(selected_patches(GOW3, '01.00', ''), [])
        # The launcher's selection is exact: unchecked defaults stay off, except the fixes that are
        # always on (a list saved before they existed must still get them).
        self.assertEqual(selected_patches(GOW3, '01.02', 'Skip Any Video With X Button', only=True),
                         ['Skip Any Video With X Button', 'Bug Fix - Startup Black Screen'])
        self.assertEqual(selected_patches(GOW3, '01.02', '', only=True), ['Bug Fix - Startup Black Screen'])
        self.assertEqual(selected_patches(GOW3, '01.00', '', only=True), [])

    def test_gow3_resolution_patch_replaces_texture_fix(self):
        self.assertEqual(selected_patches(GOW3, '01.02', 'Resolution Patch - 4K'),
                         ['Skip Any Video With X Button', 'Bug Fix - Startup Black Screen', 'Resolution Patch - 4K'])
        with self.assertRaises(ValueError):
            selected_patches(GOW3, '01.02', 'Resolution Patch - 4K;Resolution Patch - 720p')

    def test_profile_by_title_for_any_region(self):
        from unittest import mock
        import patches
        self.assertEqual(game_profile('CUSA01715')[1], GOW3)
        self.assertEqual(game_profile('CUSA99999', 'God of War® III Remastered')[1], GOW3)
        self.assertIsNone(game_profile('CUSA99999', 'Another Game'))
        with mock.patch.object(patches, 'game_sfo', return_value={'TITLE_ID': 'CUSA77777', 'TITLE': 'God of War III Remastered'}):
            self.assertTrue(patches.game_patchable('folder'))
        self.assertFalse(patches.game_patchable(None))
        with tempfile.TemporaryDirectory() as folder:
            self.assertFalse(patches.game_patchable(folder))  # no param.sfo: unknown game


class SignatureTests(unittest.TestCase):
    XML = """<?xml version="1.0"?>
<Patch>
  <Metadata Name="Found" AppVer="01.02">
    <PatchList><Line Type="bytes" Address="0x401004" Value="9090" Pattern="11 22 ?? 44 55 66" At="1"/></PatchList>
  </Metadata>
  <Metadata Name="Call" AppVer="01.02">
    <PatchList><Line Type="bytes" Address="0x401010" Value="e8{rel:there:0}" Pattern="aa bb cc dd ee" At="0"/></PatchList>
    <Target Name="there" Address="0x401100" Pattern="e9 ?? ?? ?? ?? 77 88" At="1" Ref="1"/>
  </Metadata>
  <Metadata Name="Missing" AppVer="01.02">
    <PatchList>
      <Line Type="bytes" Address="0x401004" Value="9090" Pattern="11 22 ?? 44 55 66" At="1"/>
      <Line Type="bytes" Address="0x401020" Value="90" Pattern="de ad be ef" At="0"/>
    </PatchList>
  </Metadata>
  <Metadata Name="Data" AppVer="01.02">
    <PatchList><Line Type="bytes32" Address="0x402000" Target="variable" Value="0x2"/></PatchList>
    <Target Name="variable" Address="0x402000" Pattern="44 89 35 ?? ?? ?? ?? c6" At="3" Ref="1"/>
  </Metadata>
</Patch>"""

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.xml = Path(directory.name) / 'test.xml'
        self.xml.write_text(self.XML)
        image = bytearray(0x400)
        image[0x40:0x46] = bytes.fromhex('112233445566')
        image[0x80:0x85] = bytes.fromhex('aabbccddee')
        image[0x200:0x207] = bytes.fromhex('e9') + (0x300 - 0x205).to_bytes(4, 'little', signed=True) + bytes.fromhex('7788')
        # mov [rip+disp], r14d storing to 0x3f0, then the c6 of the next instruction.
        image[0x280:0x288] = bytes.fromhex('448935') + (0x3f0 - 0x287).to_bytes(4, 'little', signed=True) + b'\xc6'
        self.image = bytes(image)

    def test_sites_found_anywhere_and_rel32_recomputed(self):
        skipped = {}
        writes = compile_patches(self.xml, ['Found', 'Call', 'Missing'], None, [(0, 0x400)], self.image, skipped)
        call = (0x300 - (0x80 + 5)).to_bytes(4, 'little', signed=True)
        self.assertEqual(writes, [(0x41, b'\x90\x90'), (0x80, b'\xe8' + call)])
        # One site of the patch is missing: the whole patch is left out, with the reason.
        self.assertEqual(skipped, {'Missing': 'signature not found'})

    def test_ambiguous_signature_is_skipped(self):
        image = bytearray(self.image)
        image[0x100:0x106] = image[0x40:0x46]
        skipped = {}
        self.assertEqual(compile_patches(self.xml, ['Found'], None, [(0, 0x400)], bytes(image), skipped), [])
        self.assertEqual(skipped, {'Found': 'signature found more than once'})

    def test_without_the_image_the_reference_addresses_are_used(self):
        writes = compile_patches(self.xml, ['Found', 'Call'], '01.02', [(0, 0x10000)])
        self.assertEqual(writes[0], (0x1004, b'\x90\x90'))
        self.assertEqual(writes[1][0], 0x1010)

    def test_data_line_writes_at_its_target(self):
        # The variable has no bytes of its own to find: the code that stores to it points at it.
        self.assertEqual(compile_patches(self.xml, ['Data'], None, [(0, 0x400)], self.image, {}),
                         [(0x3f0, (2).to_bytes(4, 'little'))])
        self.assertEqual(compile_patches(self.xml, ['Data'], '01.02', [(0, 0x10000)]),
                         [(0x2000, (2).to_bytes(4, 'little'))])
        image = bytearray(self.image)
        image[0x280:0x288] = bytes(8)  # store not found: the whole patch is left out
        skipped = {}
        self.assertEqual(compile_patches(self.xml, ['Data'], None, [(0, 0x400)], bytes(image), skipped), [])
        self.assertEqual(skipped, {'Data': 'signature not found'})

    def test_signatures_match_the_full_update_eboot(self):
        # Without that eboot.bin here: the original bytes others published for it must be the
        # fixed bytes of each signature at its site, with one shift per code region.
        import json
        import re
        known = json.loads((ROOT / 'tests/data/eboot_6e72_sites.json').read_text())
        fixed = lambda pattern, at, n: [None if b == '??' else int(b, 16) for b in pattern.split()][at:at + n]
        shifts = {}
        metas = {m.get('Name'): list(m.iter('Line')) for m in ET.parse(GOW3).getroot().iter('Metadata')}
        for name, sites in known['patches'].items():
            for index, address, original in sites:
                line = metas[name][index]
                ours = fixed(line.get('Pattern'), int(line.get('At')), len(original) // 2)
                self.assertTrue(all(b is None or b == o for b, o in zip(ours, bytes.fromhex(original))),
                                f'{name} line {index}')
                reference = int(line.get('Address'), 0)
                shifts.setdefault(reference >> 16, set()).add(int(address, 0) - reference)
        self.assertTrue(all(len(s) == 1 for s in shifts.values()), shifts)
        header = (ROOT / 'src/cheat_hook.h').read_text()
        hooks = re.findall(r'\{(0x[0-9a-f]+),GOW3_CHEAT_\w+,\d+,\d+,\s*\{([^}]*)\},.*?\{"([^"]+)",(\d+)\}\}',
                           header, re.S)
        for name, (address, original) in known['cheats'].items():
            # The cheats' region moved -0x10 (Rage of Sparta has two sites with the same bytes).
            match = [h for h in hooks if bytes(int(b, 0) for b in h[1].split(',')).hex() == original
                     and int(h[0], 0) - 0x10 == int(address, 0)]
            self.assertEqual(len(match), 1, name)
            _, _, pattern, at = match[0]
            self.assertEqual(fixed(pattern, int(at), len(original) // 2), list(bytes.fromhex(original)), name)

    @unittest.skipUnless(os.environ.get('GOW3_GAME'), 'GOW3_GAME (game folder) not set')
    def test_gow3_signatures_in_the_reference_and_a_moved_build(self):
        from prepare import parse_self
        from patches import eboot_image, eboot_segments
        elf, *_ = parse_self((Path(os.environ['GOW3_GAME']) / 'eboot.bin').read_bytes())
        image = eboot_image(bytes(elf))
        names = [m.get('Name') for m in ET.parse(GOW3).getroot().iter('Metadata')]
        segments = eboot_segments(bytes(elf))  # as patches.py: .bss included (memsz)
        # The reference build: the signatures give exactly the Address writes.
        for name in names:
            self.assertEqual(compile_patches(GOW3, [name], None, segments, image, {}),
                             compile_patches(GOW3, [name], '01.02', segments), name)
        # A build whose code moved like the full 01.02 update's: +0x120 from 0x1f0000 to 0x210000,
        # data unchanged (the bytes taken out keep the data at its address).
        moved = image[:0x1f0000] + bytes(0x120) + image[0x1f0000:0x210000] + image[0x210120:]
        self.assertEqual(len(moved), len(image))
        shift = lambda at: at + 0x120 if 0x1f0000 <= at < 0x210000 else at
        for name in ('Resolution Patch - 4K', 'Skip Intro'):
            reference = compile_patches(GOW3, [name], None, segments, image, {})
            found = compile_patches(GOW3, [name], None, segments, moved, {})
            self.assertEqual([at for at, _ in found], [shift(at) for at, _ in reference], name)
        # Skip Intro's cave moved: the boot call into it is recomputed, the flag (data) is not.
        writes = dict(compile_patches(GOW3, ['Skip Intro'], None, segments, moved, {}))
        rel = lambda data, at, end: end + int.from_bytes(data[at:at + 4], 'little', signed=True)
        self.assertEqual(rel(writes[0x01b36a], 1, 0x01b36f), shift(0x212170))
        flag = rel(writes[0x0c8440], 2, 0x0c844a)  # the getter (not moved) and the cave share it
        self.assertEqual(rel(writes[shift(0x212170)], 2, shift(0x212170) + 10), flag)

    def test_gow3_patches_compile(self):
        names = [m.get('Name') for m in ET.parse(GOW3).getroot().iter('Metadata')]
        self.assertTrue(compile_patches(GOW3, names, '01.02', SEGMENTS))

    def test_gow3_startup_black_screen(self):
        # The frame index variable (.bss) starts at 2, the index before the first frame's 0.
        self.assertEqual(compile_patches(GOW3, ['Bug Fix - Startup Black Screen'], '01.02', SEGMENTS),
                         [(0x14ba600, (2).to_bytes(4, 'little'))])

    def test_gow3_skip_intro_targets(self):
        # Ported from the other 01.02 build: every jump must land where this eboot has the code.
        writes = dict(compile_patches(GOW3, ['Skip Intro'], '01.02', SEGMENTS))
        rel = lambda data, at, end: end + int.from_bytes(data[at:at + 4], 'little', signed=True)
        self.assertEqual(writes[0x184950], bytes.fromhex('eb34'))  # jne over the logo start
        getter = writes[0x0c8440]  # mov [flag], 1; mov eax, 1; ret
        self.assertEqual(len(getter), 16)
        self.assertEqual(rel(getter, 2, 0x4c844a), 0x1e79258)
        call = writes[0x01b36a]  # the boot call now goes through the cave
        self.assertEqual(rel(call, 1, 0x41b36f), 0x612170)
        cave = writes[0x212170]  # mov [flag], 1; jmp to the original callee
        self.assertEqual(len(cave), 15)
        self.assertEqual(rel(cave, 2, 0x61217a), 0x1e79258)
        self.assertEqual(rel(cave, 11, 0x61217f), 0x4b4c90)

    @unittest.skipUnless(os.environ.get('GOW3_GAME'), 'GOW3_GAME (game folder) not set')
    def test_gow3_skip_intro_original_bytes(self):
        from prepare import parse_self
        elf, _, ph, _, _ = parse_self((Path(os.environ['GOW3_GAME']) / 'eboot.bin').read_bytes())
        text = next(p for p in ph if p['type'] == 1 and p['flags'] & 1)
        original = {0x184950: '7534', 0x0c8440: '488d05955b38008b00c3660f1f440000',
                    0x01b36a: 'e821990900', 0x212170: '90' * 15}
        for offset, value in original.items():
            at = text['offset'] + offset - text['vaddr']
            self.assertEqual(bytes(elf[at:at + len(value) // 2]).hex(), value, hex(offset))

    def test_requirements_from_patch_notes(self):
        # Notes ask for extra direct memory (shadPS4 "DMEM") and a VBlank rate.
        self.assertEqual(patch_requirements(GOW3, ['Resolution Patch - 4K', 'Skip Any Video With X Button'], '01.02'),
                         (5056 + 6144, None))
        self.assertEqual(patch_requirements(GOW3, ['Frame Rate Patch - 120 FPS'], '01.02'), (None, 120))
        self.assertEqual(patch_requirements(GOW3, [], '01.02'), (None, None))



class ModPatchTests(unittest.TestCase):
    MOD = """<?xml version="1.0"?>
<Patch>
  <TitleID><ID>CUSA01623</ID></TitleID>
  <Metadata Name="Resolution Patch - 3440x1440" Note="Requires setting DMEM to 4608 MB or more." AppVer="01.02" AppElf="eboot.bin" isEnabled="false">
    <PatchList><Line Type="bytes" Address="0x00401000" Value="9090"/></PatchList>
  </Metadata>
  <Metadata Name="Resolution Patch - 2560x1080" AppVer="01.02" AppElf="eboot.bin" isEnabled="false">
    <PatchList><Line Type="bytes" Address="0x00401000" Value="9090"/></PatchList>
  </Metadata>
  <Metadata Name="Skip Any Video With X Button" Note="replaced by the mod" AppVer="01.02" AppElf="eboot.bin" isEnabled="true">
    <PatchList><Line Type="bytes" Address="0x00401002" Value="90"/></PatchList>
  </Metadata>
</Patch>"""
    OTHER_GAME = '<Patch><TitleID><ID>CUSA99999</ID></TitleID><Metadata Name="Elsewhere" AppVer="01.02"><PatchList/></Metadata></Patch>'

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.dir = Path(directory.name)
        self.mod = self.dir / 'mod.xml'
        self.mod.write_text(self.MOD)

    def test_resolution_names_of_patch_files_smallest_first(self):
        self.assertEqual(resolution_names([self.mod]), ['2560x1080', '3440x1440'])
        self.assertEqual(resolution_names([self.dir / 'missing.xml']), [])

    def test_merge_adds_replaces_and_skips_other_titles(self):
        other = self.dir / 'other.xml'
        other.write_text(self.OTHER_GAME)
        merged = merge_patch_files(GOW3, [self.mod, other], self.dir / 'out' / 'merged.xml')
        names = [m.get('Name') for m in ET.parse(merged).getroot().iter('Metadata')]
        self.assertIn('Resolution Patch - 3440x1440', names)
        self.assertNotIn('Elsewhere', names)
        self.assertEqual(names.count('Skip Any Video With X Button'), 1)  # replaced, not repeated
        note = next(m.get('Note') for m in ET.parse(merged).getroot().iter('Metadata')
                    if m.get('Name') == 'Skip Any Video With X Button')
        self.assertEqual(note, 'replaced by the mod')
        # The built-in patches are all still there, and the merged file is selectable like the original.
        self.assertTrue(set(m.get('Name') for m in ET.parse(GOW3).getroot().iter('Metadata')) - {'Skip Any Video With X Button'} <= set(names))
        chosen = selected_patches(merged, '01.02', 'Resolution Patch - 3440x1440', only=True)
        self.assertIn('Resolution Patch - 3440x1440', chosen)  # (the startup fix is always added)
        self.assertNotIn('Resolution Patch - 2560x1080', chosen)
        self.assertEqual(patch_requirements(merged, ['Resolution Patch - 3440x1440'], '01.02'), (5056 + 4608, None))

    def test_a_broken_mod_file_is_left_out(self):
        broken = self.dir / 'broken.xml'
        broken.write_text('<Patch><Metadata')
        merged = merge_patch_files(GOW3, [broken, self.mod], self.dir / 'merged.xml')
        self.assertIn('Resolution Patch - 2560x1080',
                      [m.get('Name') for m in ET.parse(merged).getroot().iter('Metadata')])

    def test_default_enabled_patches_of_a_mod(self):
        self.assertEqual(default_enabled([self.mod], '01.02'), ['Skip Any Video With X Button'])
        self.assertEqual(default_enabled([self.mod], '01.00'), [])

    # --- the Ultrawide mod (extras/mods/Ultrawide): ULTRAWIDE sizes -> (extra direct memory MiB, k)
    SIZES = {
        (2560, 1080): (3072, (2560 / 1080) / (16 / 9)),
        (3440, 1440): (4608, (3440 / 1440) / (16 / 9)),
        (3840, 1600): (6144, (3840 / 1600) / (16 / 9)),
        (5120, 1440): (6144, 2.0),
    }

    # 16:10, taller than 16:9: only the resolution and the aspect ratio (the camera's fixed horizontal view
    # already shows more at the top and bottom), so no field of view change.
    TALLER = {(1920, 1200): 3072, (2560, 1600): 4608, (1280, 800): 3072,
              (1440, 900): 3072, (1360, 768): 3072,
              (1280, 960): 3072, (1440, 1080): 3072, (1920, 1440): 3072, (2048, 1536): 3072, (2560, 1920): 4608}

    def test_the_ultrawide_mod_lists_its_resolutions_and_memory_needs(self):
        self.assertEqual(resolution_names([ULTRAWIDE]), ['1280x800', '1360x768', '1280x960', '1440x900', '1440x1080', '1920x1200',
                                                         '1920x1440', '2560x1080', '2048x1536', '2560x1600', '2560x1920', '3440x1440',
                                                         '3840x1600', '5120x1440'])
        for (width, height), dmem in self.TALLER.items():
            name = f'Resolution Patch - {width}x{height}'
            self.assertEqual(patch_requirements(ULTRAWIDE, [name], '01.02'), (5056 + dmem, None), name)
        for (width, height), (dmem, _k) in self.SIZES.items():
            name = f'Resolution Patch - {width}x{height}'
            self.assertEqual(patch_requirements(ULTRAWIDE, [name], '01.02'), (5056 + dmem, None), name)

    def test_the_ultrawide_mod_camera_caves_form_closed_chains(self):
        # Each entry (its Address values are the 6e72 build's): the four camera sites jump into two chained caves.
        # The first runs the instruction the jump replaced, the second multiplies the field of view tangent by
        # k = (width / height) / (16 / 9) and returns right after the site.
        rel = lambda data, at, end: end + int.from_bytes(data[at:at + 4], 'little', signed=True)
        for (width, height), (_dmem, k) in self.SIZES.items():
            name = f'Resolution Patch - {width}x{height}'
            writes = dict(compile_patches(ULTRAWIDE, [name], '01.02', SEGMENTS))
            self.assertEqual(writes[0x1f0c77], struct.pack('<f', width / height) * 2, name)  # the aspect ratio
            constant = [at for at, data in writes.items() if data == struct.pack('<f', k)]
            self.assertEqual(len(constant), 1, name)
            for site, original in ((0x1cc01f, 'c5fa1089d8000000'), (0x1cbd89, 'c5fa1080d8000000'),
                                   (0x1ccaa9, 'c5fa1080d8000000'), (0x1cc20f, 'c5fa5e852cffffff')):
                jump = writes[site]
                self.assertEqual((jump[0], jump[5:]), (0xe9, bytes.fromhex('0f1f00')), name)  # jmp + 3-byte nop
                first = rel(jump, 1, site + 5)
                self.assertEqual(writes[first][:8], bytes.fromhex(original), name)
                self.assertEqual(writes[first][8], 0xe9, name)
                second = rel(writes[first], 9, first + 13)
                cave = writes[second]
                self.assertEqual((cave[0], cave[2], cave[8]), (0xc5, 0x59, 0xe9), name)  # vmulss ..., [rip+k]; jmp
                self.assertEqual(rel(cave, 4, second + 8), constant[0], name)
                self.assertEqual(rel(cave, 9, second + 13), site + 8, name)

    def test_taller_than_16_9_changes_no_camera(self):
        for (width, height) in self.TALLER:
            writes = dict(compile_patches(ULTRAWIDE, [f'Resolution Patch - {width}x{height}'], '01.02', SEGMENTS))
            aspect = struct.pack('<f', width / height)
            # the aspect ratio, twice; for 4:3 the second float is one unit above 4/3, which the game's letterbox
            # test (function at 0x56fc60, "== 4/3") does not match
            second = struct.pack('<I', struct.unpack('<I', aspect)[0] + 1) if aspect == struct.pack('<f', 4 / 3) else aspect
            self.assertEqual(writes[0x1f0c77], aspect + second)
            # the resolution (these lines keep the built-in entries' Address values, the reference build's)
            self.assertEqual(writes[0x1f0b3b], struct.pack('<I', width))
            self.assertEqual(writes[0x1f0b43], struct.pack('<I', height))
            for site in (0x1cc01f, 0x1cbd89, 0x1ccaa9, 0x1cc20f):  # no camera site, no cave
                self.assertNotIn(site, writes)
            self.assertEqual(len(writes), 15 + 1)  # the resolution lines (15) and the aspect ratio

    def test_the_ultrawide_mod_is_off_until_a_resolution_is_chosen(self):
        self.assertEqual(default_enabled([ULTRAWIDE], '01.02'), [])


if __name__ == '__main__':
    unittest.main()
