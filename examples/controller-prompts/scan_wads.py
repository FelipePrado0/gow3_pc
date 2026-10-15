#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Report which WADs under a folder carry a copy of the button prompt textures.

    scan_wads.py "<game>\\EXEC\\WAD\\PS4_LE"

Textures are referenced by name, and a WAD loaded earlier wins, so every WAD that holds a
copy of a prompt texture needs patching for the replacement to show everywhere.
"""
import sys
from pathlib import Path

HASHES = ['1BBACF4D', 'BA40AFD2', '64F3D812', '573C282A', '91A47980', '5CED5AE9', '6FC83ACE',
          '67E86446', '9AE3BA81', 'FB33513A', '7FF61BF7', '9D590CC8', '1F5FC983', 'E8889579',
          '0FFF3CCB', 'FB91136E']
NAMES = {h: b'TX' + h.encode() for h in HASHES}


def scan(folder):
    found = {}
    for path in sorted(Path(folder).rglob('*.WAD')):
        hits = set()
        with open(path, 'rb') as f:
            # chunk headers are tiny and names sit right after an 8-byte prefix: a plain
            # substring search over the file is enough and needs no parsing
            while True:
                block = f.read(64 << 20)
                if not block:
                    break
                for h, name in NAMES.items():
                    if name in block:
                        hits.add(h)
                if len(block) == 64 << 20:
                    f.seek(f.tell() - 64)  # a name could straddle the boundary
        if hits:
            found[path] = sorted(hits)
            print(f'{path}: {", ".join(sorted(hits))}')
    if not found:
        print('no WAD holds a prompt texture')
    return found


if __name__ == '__main__':
    scan(sys.argv[1] if len(sys.argv) > 1 else '.')
