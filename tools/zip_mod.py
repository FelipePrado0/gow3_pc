#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Packs a mod folder as <name>.zip, ready to be dropped into the mods folder (the launcher unpacks it).

  python tools/zip_mod.py extras/mods/Ultrawide [OUT.zip]

The mod's own files are at the root of the zip (patches/..., dvdroot_ps4/...); the zip's name is the mod's name.
"""
import sys
import zipfile
from pathlib import Path


def zip_mod(folder, out=None):
    folder = Path(folder).resolve()
    out = Path(out) if out else Path(f'{folder.name}.zip')
    with zipfile.ZipFile(out, 'w', zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(folder.rglob('*'), key=lambda p: p.as_posix().casefold()):
            if path.is_symlink():
                raise SystemExit(f'{path}: links are not allowed in a mod')
            if path.is_file():
                archive.write(path, path.relative_to(folder).as_posix())
    return out


if __name__ == '__main__':
    if len(sys.argv) not in (2, 3):
        raise SystemExit(__doc__)
    print(zip_mod(*sys.argv[1:]))
