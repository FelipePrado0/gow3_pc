#!/usr/bin/env python3
"""Inspect an already plaintext PS4 SELF; prepare a native entry-point probe.

No decryption, patches to source files, third-party modules or Python dependencies.
Format references are linked in README.md. All offsets are checked before use.
"""
import argparse
import base64
import collections
import hashlib
import json
from pathlib import Path
import struct


def span(data, offset, size):
    if offset < 0 or size < 0 or offset + size > len(data):
        raise ValueError(f"out of bounds: {offset:#x}+{size:#x} / {len(data):#x}")
    return data[offset:offset + size]


def unpack(fmt, data, offset):
    return struct.unpack(fmt, span(data, offset, struct.calcsize(fmt)))


def parse_self(data):
    if span(data, 0, 4) != b'O\x15=\x1d':
        raise ValueError("expected PS4 SELF")
    count, = unpack('<H', data, 24)
    base = 32 + count * 32
    header = unpack('<16sHHIQQQIHHHHHH', data, base)
    if header[0][:7] != b'\x7fELF\x02\x01\x01' or header[2] != 62:
        raise ValueError("expected little-endian x86-64 ELF")
    if header[9] != 56 or not 0 < header[10] < 256:
        raise ValueError("unsupported program headers")
    ph = [dict(zip(('type', 'flags', 'offset', 'vaddr', 'paddr', 'filesz', 'memsz', 'align'),
                   unpack('<IIQQQQQQ', data, base + header[5] + i * 56)))
          for i in range(header[10])]
    end = max(p['offset'] + p['filesz'] for p in ph)
    if end > 512 * 1024 * 1024:
        raise ValueError("probe image exceeds 512 MiB limit")
    elf = bytearray(end)
    header_end = header[5] + header[10] * 56
    elf[:header_end] = span(data, base, header_end)
    covered = [(0, header_end)]
    segments = []
    for i in range(count):
        flags, off, size, mem = unpack('<QQQQ', data, 32 + i * 32)
        segments.append(dict(flags=hex(flags), offset=off, size=size))
        if not flags & 0x800:
            continue
        if flags & 10:
            raise ValueError("encrypted/compressed SELF segment is unsupported")
        index = (flags >> 20) & 4095
        if index >= len(ph):
            raise ValueError("invalid segment index")
        p = ph[index]
        if size != p['filesz'] or size != mem:
            raise ValueError("unsupported blocked segment layout")
        elf[p['offset']:p['offset'] + size] = span(data, off, size)
        covered.append((p['offset'], p['offset'] + size))
    missing = []
    for i, p in enumerate(ph):
        if p['filesz'] and not any(a <= p['offset'] and p['offset'] + p['filesz'] <= z
                                  for a, z in covered):
            missing.append(i)
            if p['type'] in (1, 2, 0x61000000, 0x61000010):
                raise ValueError(f"required segment {i} is unavailable")
    return elf, header, ph, segments, missing


def sfo(data):
    magic, version, keys, values, count = unpack('<4sIIII', data, 0)
    if magic != b'\0PSF':
        raise ValueError('bad SFO signature')
    result = {}
    for i in range(count):
        key, fmt, size, capacity, off = unpack('<HHIII', data, 20 + i * 16)
        k = data[keys + key:data.index(0, keys + key)].decode()
        v = span(data, values + off, size)
        result[k] = struct.unpack('<I', v)[0] if fmt == 0x404 else v.rstrip(b'\0').decode('utf8', 'replace')
    return result


def nid(name):
    salt = bytes.fromhex('518d64a635ded8c1e6b039b1c3e55230')
    digest = hashlib.sha1(name.encode('ascii') + salt).digest()[:8][::-1]
    return base64.b64encode(digest).decode().rstrip('=').replace('/', '-')


def inspect_libc(path):
    """Prove that this dump's _init_env is exactly RET; never assume it."""
    source = path.read_bytes()
    elf, header, ph, _, missing = parse_self(source)
    dynamic = next(p for p in ph if p['type'] == 2)
    tags = dict(unpack('<QQ', elf, pos) for pos in
                range(dynamic['offset'], dynamic['offset'] + dynamic['filesz'], 16))
    lib = next(p for p in ph if p['type'] == 0x61000000)
    blob = span(elf, lib['offset'], lib['filesz'])
    strings = span(blob, tags[0x61000035], tags[0x61000037])
    syms = span(blob, tags[0x61000039], tags[0x6100003f])
    evidence = {'sha256': hashlib.sha256(source).hexdigest(), 'init_env_is_ret': False}
    for pos in range(0, len(syms), 24):
        name, info, other, section, value, size = unpack('<IBBHQQ', syms, pos)
        span(strings, name, 1)
        symbol = strings[name:strings.index(0, name)].decode('ascii')
        key = symbol.split('#')[0]
        if key not in (nid('_init_env'), nid('_ZNSt8ios_base4InitC1Ev')) or not section or info & 15 != 2:
            continue
        p = next(p for p in ph if p['type'] == 1 and p['flags'] & 1
                 and p['vaddr'] <= value < p['vaddr'] + p['filesz'])
        code = span(elf, p['offset'] + value - p['vaddr'], size)
        if key == nid('_init_env'):
            evidence.update(symbol=symbol, address=hex(value), size=size, bytes=code.hex(),
                            init_env_is_ret=code == b'\xc3')
        else:
            evidence['ios_base_init'] = dict(symbol=symbol, address=hex(value), size=size,
                                             code_sha256=hashlib.sha256(code).hexdigest(),
                                             name='_ZNSt8ios_base4InitC1Ev')
    return evidence


def prepare(game, out):
    source = (game / 'eboot.bin').read_bytes()
    elf, header, ph, segments, missing = parse_self(source)
    loads = [p for p in ph if p['type'] in (1, 0x61000010)]
    def mapped(addr, size=8):
        return any(p['vaddr'] <= addr and addr + size <= p['vaddr'] + p['memsz'] for p in loads)
    size = max(p['vaddr'] + p['memsz'] for p in loads)
    if size > 512 * 1024 * 1024 or not mapped(header[4], 1):
        raise ValueError('invalid memory image')
    image = bytearray(size)
    for p in loads:
        if p['filesz'] > p['memsz']:
            raise ValueError('segment file size exceeds memory size')
        image[p['vaddr']:p['vaddr'] + p['filesz']] = span(elf, p['offset'], p['filesz'])
    dp = next(p for p in ph if p['type'] == 2)
    dyn = []
    for pos in range(dp['offset'], dp['offset'] + dp['filesz'], 16):
        tag, value = unpack('<QQ', elf, pos)
        if tag == 0:
            break
        dyn.append((tag, value))
    tags = dict(dyn)
    lib = next(p for p in ph if p['type'] == 0x61000000)
    blob = span(elf, lib['offset'], lib['filesz'])
    strings = span(blob, tags[0x61000035], tags[0x61000037])
    def string(off):
        span(strings, off, 1)
        return strings[off:strings.index(0, off)].decode('ascii')
    syms = span(blob, tags[0x61000039], tags[0x6100003f])
    if len(syms) % 24:
        raise ValueError('bad symbol table size')
    symbols = []
    for pos in range(0, len(syms), 24):
        name, info, other, shndx, value, sz = unpack('<IBBHQQ', syms, pos)
        symbols.append(dict(name=string(name), type=info & 15, section=shndx, value=value, size=sz))
    relocs, counts, imports = [], collections.Counter(), {}
    for offset_tag, size_tag in ((0x61000029, 0x6100002d), (0x6100002f, 0x61000031)):
        table = span(blob, tags[offset_tag], tags[size_tag])
        if len(table) % 24:
            raise ValueError('bad relocation table size')
        for pos in range(0, len(table), 24):
            target, info, addend = unpack('<QQq', table, pos)
            kind, sym = info & 0xffffffff, info >> 32
            counts[kind] += 1
            if not mapped(target):
                raise ValueError(f'relocation outside mapped segments: {target:#x}')
            if kind == 8:
                relocs.append((target, 0, addend, 0))  # base-relative
            elif kind in (1, 6, 7):
                symbol = symbols[sym]
                if symbol['section']:
                    if symbol['section'] == 0xfff1:
                        struct.pack_into('<Q', image, target, symbol['value'] + addend)
                    else:
                        relocs.append((target, 0, symbol['value'] + addend, 0))
                else:
                    if sym not in imports:
                        imports[sym] = len(imports)
                    kind = 2 if symbol['type'] == 1 else 1
                    if (kind == 1 and addend) or not 0 <= addend < 4096:
                        raise ValueError('unsupported import addend')
                    relocs.append((target, kind, imports[sym], addend))
            else:
                raise ValueError(f'unsupported relocation {kind}')
    names = [symbols[i]['name'] for i in imports]
    candidates = ('_init_env', 'atexit', 'exit', '_exit', '__cxa_atexit', '__cxa_finalize',
                  '__cxa_guard_acquire', '__cxa_guard_release', '__cxa_guard_abort',
                  '__stack_chk_guard', '__stack_chk_fail', 'malloc', 'free',
                  'memcpy', 'memmove', 'memset', 'memcmp', 'strlen', 'printf',
                  '_ZNSt8ios_base4InitC1Ev',
                  'sceKernelLoadStartModule', 'sceVideoOutOpen', 'sceKernelCreateSema', 'sceKernelWaitSema', 'sceKernelPollSema',
                  'sceKernelSignalSema', 'sceKernelCancelSema', 'sceKernelDeleteSema',
                  'sceAppContentInitialize', 'sceAppContentAppParamGetInt', 'sceAppContentGetAddcontInfoList',
                  'pthread_cond_init', 'sceSysmoduleLoadModule', 'pthread_mutexattr_init', 'gettimeofday',
                  'scePthreadMutexattrInit', 'scePthreadMutexattrSettype', 'scePthreadMutexattrDestroy',
                  'scePthreadMutexInit', 'scePthreadMutexLock', 'scePthreadMutexTrylock',
                  'scePthreadMutexUnlock', 'scePthreadMutexDestroy', 'scePthreadRwlockInit',
                  'scePthreadRwlockDestroy', 'scePthreadRwlockRdlock', 'scePthreadRwlockWrlock',
                  'scePthreadRwlockTryrdlock', 'scePthreadRwlockTrywrlock', 'scePthreadRwlockUnlock',
                  'scePthreadRwlockTimedrdlock', 'scePthreadRwlockTimedwrlock',
                  'sceKernelGetDirectMemorySize', 'sceKernelAllocateDirectMemory',
                  'sceKernelMapDirectMemory', 'sceKernelReleaseDirectMemory', 'sceKernelMunmap')
    known = {nid(name): name for name in candidates}
    libc_evidence = inspect_libc(game / 'sce_module/libc.prx')
    out.mkdir(parents=True, exist_ok=True)
    # A deliberately small format, consumed by probe.c; no host struct packing.
    with (out / 'boot.bin').open('wb') as f:
        f.write(struct.pack('<8sQQQQQQ', b'G3PROBE2', size, header[4], len(loads), len(relocs), len(names),
                            int(libc_evidence['init_env_is_ret'])))
        for p in loads:
            f.write(struct.pack('<QQQ', p['vaddr'], p['memsz'], p['flags']))
        for name in names:
            encoded = name.encode()
            if len(encoded) >= 128:
                raise ValueError('import name too long')
            f.write(encoded.ljust(128, b'\0'))
        for relocation in relocs:
            f.write(struct.pack('<QQqq', *relocation))
        f.write(image)
    (out / 'eboot.elf').write_bytes(elf)
    (out / 'entry.bin').write_bytes(image[:1024])
    resources = collections.Counter()
    total_bytes = 0
    for path in (game / 'dvdroot_ps4').rglob('*'):
        if path.is_file():
            resources[path.relative_to(game / 'dvdroot_ps4').parts[0]] += 1
            total_bytes += path.stat().st_size
    report = dict(source_sha256=hashlib.sha256(source).hexdigest(), source_bytes=len(source),
                  sfo=sfo((game / 'sce_sys/param.sfo').read_bytes()), entry=hex(header[4]),
                  image_bytes=size, program_headers=ph, self_segments=segments,
                  unavailable_metadata_headers=missing, needed=[string(v) for t, v in dyn if t == 1],
                  relocation_counts=dict(counts), import_count=len(names), imports=names,
                  import_name_hints={name: known[name.split('#')[0]] for name in names if name.split('#')[0] in known},
                  libc_evidence=libc_evidence,
                  bundled_modules=sorted(p.name for p in (game / 'sce_module').iterdir()),
                  resources=dict(resources), resource_bytes=total_bytes,
                  status='Prepared only; execution and Vulkan are tested separately.')
    (out / 'analysis.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    print(f"{report['sfo'].get('TITLE')} | entry={header[4]:#x} | image={size:,} bytes")
    print(f"{len(names)} imported symbols; {sum(counts.values()):,} relocations; {len(report['needed'])} required modules")
    print(f"Unavailable non-loadable metadata headers: {missing}; not a byte-exact ELF reconstruction")
    print(f"Output: {out.resolve()}")
    print(f"libc _init_env verified RET: {libc_evidence['init_env_is_ret']}")


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('game', type=Path)
    parser.add_argument('--out', type=Path, default=Path(__file__).resolve().parent.parent / 'out')
    args = parser.parse_args()
    try:
        prepare(args.game, args.out)
    except (ValueError, OSError, StopIteration, KeyError, IndexError) as error:
        parser.exit(1, f'prepare failed: {error}\n')
