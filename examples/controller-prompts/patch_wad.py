#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Replace textures inside a God of War III Remastered (PS4_LE) WAD, in place.

    patch_wad.py R_UI.WAD art/ --out R_UI.patched.WAD

For every <hash>.png in the art folder, the texture chunk whose name starts with TX<hash> is
re-encoded in the original's DDS format (DXT1/DXT5), size and mip count, and written over the
original pixel data at the same offset. Nothing else in the file moves, so the game's indexes
stay valid. The input file is never modified.
"""
import argparse
import io
import struct
import sys
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
from wad import chunks  # noqa: E402

TEXTURE_TYPE = 0x000D0001
SUBHEADER = 76          # texture record before the embedded DDS
DDS_HEADER = 128


def dds_info(header):
    """(width, height, mips, fourcc) of a 128-byte DDS header."""
    height, width, _pitch, _depth, mips = struct.unpack_from('<5I', header, 12)
    pfflags, fourcc = struct.unpack_from('<II', header, 80)
    if not pfflags & 4:
        raise ValueError('uncompressed DDS: not supported')
    return width, height, max(mips, 1), fourcc.to_bytes(4, 'little').decode('ascii')


def encode_level(image, fourcc):
    """Pixel data of one mip level in DXT1/DXT5, via Pillow's DDS writer."""
    buf = io.BytesIO()
    image.save(buf, format='DDS', pixel_format=fourcc)
    data = buf.getvalue()
    return data[DDS_HEADER:]


def mip_chain(image, width, height, mips, fourcc):
    """Concatenated DXT data for `mips` levels, resizing with premultiplied alpha."""
    if image.size != (width, height):
        raise ValueError(f'art is {image.size}, texture is {(width, height)}')
    image = image.convert('RGBA')
    out = bytearray()
    w, h = width, height
    level = image
    for i in range(mips):
        if i:
            w, h = max(1, w // 2), max(1, h // 2)
            level = premultiplied_resize(image, (w, h))
        # DXT blocks are 4x4: Pillow pads smaller levels itself; we keep the data it writes
        out += encode_level(level, fourcc)
    return bytes(out)


def premultiplied_resize(image, size):
    r, g, b, a = image.split()
    pm = Image.merge('RGBA', (r, g, b, a))
    px = pm.load()
    w, h = pm.size
    # premultiply
    for y in range(h):
        for x in range(w):
            cr, cg, cb, ca = px[x, y]
            if ca != 255:
                px[x, y] = (cr * ca // 255, cg * ca // 255, cb * ca // 255, ca)
    small = pm.resize(size, Image.LANCZOS)
    px = small.load()
    for y in range(size[1]):
        for x in range(size[0]):
            cr, cg, cb, ca = px[x, y]
            if 0 < ca < 255:
                px[x, y] = (min(255, cr * 255 // ca), min(255, cg * 255 // ca), min(255, cb * 255 // ca), ca)
    return small


def full_chain(width, height):
    n, w, h = 1, width, height
    while w > 1 or h > 1:
        w, h, n = max(1, w // 2), max(1, h // 2), n + 1
    return n


def rebuild(wad_path, art_dir, out_path, verbose=True):
    """Like patch(), but art may be larger than the original texture: the chunk grows and the
    WAD is re-emitted with every later chunk shifted. The DDS header keeps its flags; width,
    height and mip count follow the art (a full chain stays a full chain, 1 mip stays 1)."""
    blob = Path(wad_path).read_bytes()
    art = {p.stem.upper(): p for p in Path(art_dir).glob('*.png')}
    out = bytearray()
    done = {}
    for c in chunks(blob):
        header = blob[c['offset']:c['data_off']]
        data = blob[c['data_off']:c['data_off'] + c['size']]
        key = c['name'][2:10].upper()
        if c['raw_type'] == TEXTURE_TYPE and c['name'].startswith('TX') and key in art:
            if data[SUBHEADER:SUBHEADER + 4] != b'DDS ':
                raise ValueError(f'{c["name"]}: DDS magic not found at expected offset')
            dds = bytearray(data[SUBHEADER:SUBHEADER + DDS_HEADER])
            width, height, mips, fourcc = dds_info(dds)
            image = Image.open(art[key])
            new_w, new_h = image.size
            if (new_w / width) != (new_h / height):
                raise ValueError(f'{c["name"]}: art {image.size} does not keep the aspect of {(width, height)}')
            if mips <= 1 and (new_w, new_h) != (width, height):
                # The game's mipless UI glyphs alias badly once they are larger than they are drawn.
                # Give the grown texture the header of its mipmapped siblings (R_UI's 128x128
                # prompts): flags 0x900A1007, caps MIPMAP|TEXTURE, a full chain.
                new_mips = max(1, full_chain(new_w, new_h) - 1)  # the siblings stop at 2x2
                struct.pack_into('<I', dds, 8, 0x900A1007)
                struct.pack_into('<I', dds, 108, 0x00401000)
            else:
                new_mips = full_chain(new_w, new_h) if mips == full_chain(width, height) else max(mips, 1)
            struct.pack_into('<II', dds, 12, new_h, new_w)
            struct.pack_into('<I', dds, 28, new_mips)
            payload = mip_chain(image, new_w, new_h, new_mips, fourcc)
            data = data[:SUBHEADER] + bytes(dds) + payload
            header = bytearray(header)
            struct.pack_into('<I', header, 4, len(data))
            done[key] = (c['name'], new_w, new_h, new_mips, fourcc)
            if verbose:
                print(f'{c["name"]}: {width}x{height} -> {new_w}x{new_h} {fourcc} x{new_mips} mips, {len(payload)} bytes')
        out += header
        out += data
        out += b'\0' * ((-len(data)) % 16)
    missing = sorted(set(art) - set(done))
    if missing:
        raise ValueError(f'no texture found for: {", ".join(missing)}')
    Path(out_path).parent.mkdir(parents=True, exist_ok=True)
    Path(out_path).write_bytes(out)
    if verbose:
        print(f'{len(blob)} -> {len(out)} bytes')
    return done


def patch(wad_path, art_dir, out_path, verbose=True):
    blob = bytearray(Path(wad_path).read_bytes())
    art = {p.stem.upper(): p for p in Path(art_dir).glob('*.png')}
    done = {}
    for c in chunks(blob):
        if c['raw_type'] != TEXTURE_TYPE or not c['name'].startswith('TX'):
            continue
        key = c['name'][2:10].upper()
        if key not in art:
            continue
        dds_off = c['data_off'] + SUBHEADER
        if blob[dds_off:dds_off + 4] != b'DDS ':
            raise ValueError(f'{c["name"]}: DDS magic not found at expected offset')
        header = bytes(blob[dds_off:dds_off + DDS_HEADER])
        width, height, mips, fourcc = dds_info(header)
        payload_off = dds_off + DDS_HEADER
        payload_len = c['size'] - SUBHEADER - DDS_HEADER
        data = mip_chain(Image.open(art[key]), width, height, mips, fourcc)
        if len(data) != payload_len:
            raise ValueError(f'{c["name"]}: encoded {len(data)} bytes, texture holds {payload_len}')
        blob[payload_off:payload_off + payload_len] = data
        done[key] = (c['name'], width, height, mips, fourcc)
        if verbose:
            print(f'{c["name"]}: {width}x{height} {fourcc} x{mips} mips, {payload_len} bytes replaced')
    missing = sorted(set(art) - set(done))
    if missing:
        raise ValueError(f'no texture found for: {", ".join(missing)}')
    Path(out_path).parent.mkdir(parents=True, exist_ok=True)
    Path(out_path).write_bytes(blob)
    return done


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('wad', type=Path)
    parser.add_argument('art', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--rebuild', action='store_true',
                        help='allow larger art: re-emit the WAD with grown texture chunks')
    args = parser.parse_args()
    try:
        done = (rebuild if args.rebuild else patch)(args.wad, args.art, args.out)
    except (OSError, ValueError) as error:
        parser.exit(1, f'patch failed: {error}\n')
    print(f'{len(done)} textures replaced -> {args.out}')


if __name__ == '__main__':
    main()
