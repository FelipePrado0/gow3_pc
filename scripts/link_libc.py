"""Link the user's plaintext libc into a separate, reproducible probe image.

The original boot.bin remains usable. Symbols are matched by library/module
identity, versions and NID, not by dump-local suffix or NID alone.
"""
import collections
import hashlib
import json
from pathlib import Path
import struct
from prepare import parse_self, span, unpack


def encode_id(value):
    alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+-'
    result = alphabet[value & 63]
    value >>= 6
    while value:
        result = alphabet[value & 63] + result
        value >>= 6
    return result


def module(path):
    source = path.read_bytes()
    elf, header, ph, _, missing = parse_self(source)
    dp = next(p for p in ph if p['type'] == 2)
    dynamic = []
    for pos in range(dp['offset'], dp['offset']+dp['filesz'], 16):
        tag, value = unpack('<QQ', elf, pos)
        if not tag:
            break
        dynamic.append((tag, value))
    tags = dict(dynamic)
    lp = next(p for p in ph if p['type'] == 0x61000000)
    blob = span(elf, lp['offset'], lp['filesz'])
    strings = span(blob, tags[0x61000035], tags[0x61000037])
    def string(offset):
        span(strings, offset, 1)
        return strings[offset:strings.index(0, offset)].decode('ascii')
    libraries, modules = {}, {}
    for tag, value in dynamic:
        if tag in (0x61000013, 0x61000015, 0x6100000d, 0x6100000f):
            table = libraries if tag in (0x61000013, 0x61000015) else modules
            key = encode_id(value >> 48)
            identity = (string(value & 0xffffffff), (value >> 32) & 65535)
            if key in table and table[key] != identity:
                raise ValueError('conflicting module/library IDs')
            table[key] = identity
    def identity(name):
        parts = name.split('#')
        if len(parts) != 3:
            raise ValueError(f'unsupported symbol encoding {name!r}')
        n, lib, mod = parts
        return (n, libraries[lib], modules[mod])
    symbols = []
    table = span(blob, tags[0x61000039], tags[0x6100003f])
    for pos in range(0, len(table), 24):
        n, info, other, section, value, size = unpack('<IBBHQQ', table, pos)
        name = string(n)
        symbols.append(dict(name=name, type=info & 15, binding=info >> 4, section=section,
                            value=value, size=size, identity=identity(name) if name else None))
    relocations = []
    for ot, st in ((0x61000029, 0x6100002d), (0x6100002f, 0x61000031)):
        table = span(blob, tags[ot], tags[st])
        for pos in range(0, len(table), 24):
            target, info, addend = unpack('<QQq', table, pos)
            relocations.append((target, info & 0xffffffff, info >> 32, addend))
    return dict(elf=elf, header=header, ph=ph, tags=tags, symbols=symbols,
                relocs=relocations, identity=identity, libraries=libraries, modules=modules,
                sha256=hashlib.sha256(source).hexdigest(), missing=missing)


def link(game, out):
    main = module(game/'eboot.bin')
    libc = module(game/'sce_module/libc.prx')
    raw = (out/'boot.bin').read_bytes()
    magic, size, entry, ns, nr, ni, flags = unpack('<8s6Q', raw, 0)
    if magic != b'G3PROBE2':
        raise ValueError('linker expects freshly prepared G3PROBE2')
    pos = 56
    segments = [unpack('<3Q', raw, pos+i*24) for i in range(ns)]
    pos += ns*24
    names = [span(raw, pos+i*128, 128).split(b'\0',1)[0].decode() for i in range(ni)]
    pos += ni*128
    relocs = [unpack('<QQqq', raw, pos+i*32) for i in range(nr)]
    pos += nr*32
    image = bytearray(span(raw, pos, size))
    if pos+size != len(raw):
        raise ValueError('unexpected boot trailer')
    base = (size+65535) & ~65535
    loads = [p for p in libc['ph'] if p['type'] in (1,0x61000010)]
    libsize = max(p['vaddr']+p['memsz'] for p in loads)
    if base+libsize > 512*1024*1024:
        raise ValueError('linked image exceeds probe limit')
    image.extend(bytes(base+libsize-len(image)))
    def mapped(address, size=8):
        return any(p['vaddr'] <= address and address+size <= p['vaddr']+p['memsz'] for p in loads)
    for p in loads:
        if p['filesz'] > p['memsz']:
            raise ValueError('invalid libc segment size')
        image[base+p['vaddr']:base+p['vaddr']+p['filesz']] = span(libc['elf'],p['offset'],p['filesz'])
        segments.append((base+p['vaddr'],p['memsz'],p['flags']))
    # Preserve main-program spelling for the existing host resolver. New kernel
    # NIDs get the main module's spelling only if identity AND version match.
    identities = {main['identity'](name):i for i,name in enumerate(names)}
    main_libraries = {identity:key for key,identity in main['libraries'].items()}
    main_modules = {identity:key for key,identity in main['modules'].items()}
    def imported(symbol):
        key = symbol['identity']
        if key not in identities:
            n,lib,mod = key
            name = (f'{n}#{main_libraries[lib]}#{main_modules[mod]}'
                    if lib in main_libraries and mod in main_modules else f'libc:{symbol["name"]}')
            identities[key] = len(names)
            names.append(name)
        return identities[key]
    tls_count = 0
    for target, kind, symid, addend in libc['relocs']:
        if not mapped(target):
            raise ValueError(f'libc relocation outside image {target:#x}')
        target += base
        if kind == 8:
            relocs.append((target,0,base+addend,0))
        elif kind == 16:
            # libc is module 2 in this probe. Descriptor offset is already stored
            # in the adjacent word; __tls_get_addr will use a separate TLS block.
            symbol = libc['symbols'][symid]
            if symbol['name'] or symbol['type'] != 3 or addend:
                raise ValueError('unsupported external TLS relocation')
            struct.pack_into('<Q',image,target,2)
            tls_count += 1
        elif kind in (1,6,7):
            symbol = libc['symbols'][symid]
            if symbol['section']:
                if symbol['section'] == 0xfff1:
                    struct.pack_into('<Q',image,target,symbol['value']+addend)
                else:
                    relocs.append((target,0,base+symbol['value']+addend,0))
            else:
                rk = 2 if symbol['type']==1 else 1
                if (rk==1 and addend) or not 0 <= addend < 4096:
                    raise ValueError('unsupported libc import addend')
                relocs.append((target,rk,imported(symbol),addend))
        else:
            raise ValueError(f'unsupported libc relocation type {kind}')
    exports = {}
    for s in libc['symbols']:
        if s['section'] and s['binding'] in (1,2) and s['identity'] and s['type'] in (1,2):
            if not mapped(s['value'],max(1,s['size'])):
                raise ValueError('export outside libc load segments')
            if s['identity'] in exports:
                raise ValueError('ambiguous libc export')
            exports[s['identity']] = s
    bindings = []
    for identity,index in identities.items():
        if identity in exports:
            s = exports[identity]
            bindings.append((index,base+s['value'],2 if s['type']==1 else 1))
    # The eboot reads its thread pointer with `mov rax, fs:[0]` (initial-exec
    # TLS). glibc owns FS on Linux, so rewrite the segment prefix to GS; the
    # runtime points GS at each guest thread's TCB. Only executable eboot
    # segments are scanned, and only this exact 9-byte instruction.
    fs_load = bytes.fromhex('64488b042500000000')
    patched = 0
    for p in main['ph']:
        if p['type'] != 1 or not p['flags'] & 1:
            continue
        start, end = p['vaddr'], p['vaddr'] + p['filesz']
        at = image.find(fs_load, start, end)
        while at >= 0:
            image[at] = 0x65
            patched += 1
            at = image.find(fs_load, at + len(fs_load), end)
    main_tls = next((p for p in main['ph'] if p['type']==7), None)
    main_tls_values = (main_tls['vaddr'], main_tls['filesz'], main_tls['memsz'], main_tls['align']) if main_tls else (0,0,0,0)
    if main_tls and (main_tls['filesz'] > main_tls['memsz'] or main_tls['memsz'] > 1024*1024
                     or main_tls['vaddr'] + main_tls['filesz'] > size):
        raise ValueError('unsupported eboot TLS layout')
    tls = next(p for p in libc['ph'] if p['type']==7)
    if tls['filesz'] > tls['memsz'] or not mapped(tls['vaddr'],tls['memsz']):
        raise ValueError('unsupported TLS layout')
    procparam = next(p for p in main['ph'] if p['type']==0x61000001)
    metadata = (base,libsize,base+libc['tags'][12],base+tls['vaddr'],tls['memsz'],tls['filesz'],len(bindings),procparam['vaddr'])
    with (out/'boot-libc.bin').open('wb') as f:
        f.write(struct.pack('<8s6Q',b'G3PROBE4',len(image),entry,len(segments),len(relocs),len(names),flags))
        f.write(struct.pack('<8Q',*metadata))
        f.write(struct.pack('<4Q',*main_tls_values))
        for binding in bindings: f.write(struct.pack('<3Q',*binding))
        for segment in segments: f.write(struct.pack('<3Q',*segment))
        for name in names:
            if len(name.encode())>=128: raise ValueError('name too long')
            f.write(name.encode().ljust(128,b'\0'))
        for relocation in relocs: f.write(struct.pack('<QQqq',*relocation))
        f.write(image)
    report = dict(base=hex(base),size=libsize,sha256=libc['sha256'],init=hex(metadata[2]),
                  bindings=len(bindings),tls_module_id=2,fs_loads_patched=patched,
                  main_tls=dict(zip(('vaddr','filesz','memsz','align'),main_tls_values)),tls_relocations=tls_count,
                  tls_template_bytes=tls['filesz'],tls_memory_bytes=tls['memsz'],
                  imports=names,relocation_counts=dict(collections.Counter(r[1] for r in libc['relocs'])),
                  symbol_bindings=[dict(import_name=names[i],address=hex(a),kind=k) for i,a,k in bindings])
    (out/'libc-link.json').write_text(json.dumps(report,indent=2)+'\n')
    print(f'Linked native libc: base={base:#x}, {len(bindings)} fallback exports, TLS={tls["memsz"]} bytes; '
          f'eboot TLS={main_tls_values[2]} bytes, fs->gs patched={patched}')


if __name__=='__main__':
    import argparse
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('game',type=Path);p.add_argument('--out',type=Path,default=Path(__file__).resolve().parent.parent/'out')
    a=p.parse_args()
    link(a.game,a.out)
