#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Walk a God of War III Remastered (PS4_LE) WAD and list its chunks.

Chunk = 0x40-byte header (u32 type, u32 size, char name[0x38]) + data padded to 16 bytes.
"""
import struct
import sys
from pathlib import Path

HEADER = 0x40
TYPE_NAMES = {1: 'Server/Data', 2: 'GroupStart', 3: 'GroupEnd', 7: 'DATA_BLOCK',
              0x12: '18?', 0x14: 'PUSH_HEAP', 0x15: 'PopHeap'}


def chunks(blob):
    off = 0
    while off + HEADER <= len(blob):
        raw_type, size = struct.unpack_from('<II', blob, off)
        name = blob[off + 8:off + HEADER].split(b'\0', 1)[0].decode('latin-1')
        data_off = off + HEADER
        yield dict(offset=off, raw_type=raw_type, type=raw_type & 0xFFFF, flags=raw_type >> 16,
                   size=size, name=name, data_off=data_off)
        off = data_off + ((size + 15) & ~15)


def main():
    blob = Path(sys.argv[1]).read_bytes()
    depth = 0
    for i, c in enumerate(chunks(blob)):
        if c['type'] == 3:
            depth -= 1
        tname = TYPE_NAMES.get(c['type'], str(c['type']))
        print(f"{i:4d} {c['offset']:08x} {'  ' * depth}{tname:<12} flags={c['flags']:<5x} "
              f"{c['size']:>9}  {c['name']}")
        if c['type'] == 2:
            depth += 1
    print(f'total {len(blob)} bytes, ended at {c["data_off"] + ((c["size"] + 15) & ~15):#x}')


if __name__ == '__main__':
    main()
