#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Swaps game voice lines for other recordings, in an Update folder, without touching the dump.

  python tools/xvag_voice_swap.py GAME_DIR SOURCE UPDATE_DIR [--language PORTUGUESE] [--reference ORIGINAL]
                                  [--only NAMES.txt] [--dry-run]

GAME_DIR    the dumped game (the folder with eboot.bin), read only.
SOURCE      a PS3 language archive (.psarc) or a folder, with mono PS-ADPCM recordings named like the game's
            voice files (NAME.XVAG). Each one is either a Sony VAG file ("VAGp") or an XVAG file.
UPDATE_DIR  an empty or new folder. Set it as "Update folder" in the launcher (Game page): the launcher lays it over
            the game, so GAME_DIR stays as it is. Never inside GAME_DIR.

A voice file of the game is `<LANGUAGE>/SOUNDS/STREAMS_PS4/NAME.XVAG`: an XVAG header followed by PS-ADPCM frames
(16 bytes each, 28 samples). The new file is the original one byte for byte, with only the frames replaced and the
flag byte of every frame kept, so the header, the size and the padding are exactly the original's. A recording is
refused, with the reason, when that is not possible: the original is not mono PS-ADPCM, the sample rate or the number
of frames differs, or a VAGp file lacks the blank first frame and the 0x07 end frame. Lines whose recording is the
original audio are skipped; when SOURCE is a modified copy of an official archive, pass the official one as
--reference and every line the copy did not change is left alone too. Multichannel lines and the cinematics (MPEG)
are not handled.

No game audio or recording is part of this repository; bring your own files.
"""
import argparse
import hashlib
import math
import shutil
import struct
import sys
import zlib
from pathlib import Path


class Psarc:
    """Read-only reader of a PlayStation Archive (PSARC): the file names and their contents."""

    def __init__(self, path):
        self.file = open(path, 'rb')
        header = self.file.read(32)
        if header[:4] != b'PSAR':
            raise ValueError(f'{path}: not a PSARC archive')
        self.compression = header[8:12]
        toc_length, entry_size, count, self.block_size, _flags = struct.unpack('>5I', header[12:32])
        toc = self.file.read(toc_length - 32)
        self.entries, by_hash = [], {}
        for i in range(count):
            e = toc[i * entry_size:(i + 1) * entry_size]
            by_hash[e[:16]] = i
            self.entries.append((struct.unpack('>I', e[16:20])[0], int.from_bytes(e[20:25], 'big'),
                                 int.from_bytes(e[25:30], 'big')))
        step = 2 if self.block_size <= 0x10000 else 3 if self.block_size <= 0x1000000 else 4
        rest = toc[count * entry_size:]
        self.blocks = [int.from_bytes(rest[i:i + step], 'big') for i in range(0, len(rest) - step + 1, step)]
        # Entry 0 is the list of names; every other entry is found by the MD5 of its name (the order differs).
        self.names = {}
        for name in self.read(0).decode('utf-8', 'replace').split('\n'):
            for spelled in (name, '/' + name.lstrip('/')):
                index = by_hash.get(hashlib.md5(spelled.encode('utf-8')).digest())
                if name and index is not None:
                    self.names[name.rsplit('/', 1)[-1].upper()] = index
                    break

    def close(self):
        self.file.close()

    def _inflate(self, data, size):
        if len(data) == size:  # a block stored as it is
            return data
        if self.compression != b'zlib':
            raise ValueError(f'PSARC compression {self.compression.decode("ascii", "replace")!r} is not supported (zlib only)')
        return zlib.decompress(data)

    def read(self, index):
        first, size, offset = self.entries[index]
        self.file.seek(offset)
        out = bytearray()
        for b in range(math.ceil(size / self.block_size)):
            want = min(self.block_size, size - len(out))
            raw = self.file.read(self.blocks[first + b] or self.block_size)
            out += self._inflate(raw, want)
        return bytes(out)


def read_xvag_header(data):
    """(big_endian, payload_start, channels, codec, samples, sample_rate) of an XVAG file."""
    if data[:4] != b'XVAG' or len(data) < 0x44 or data[0x20:0x24] != b'fmat':
        raise ValueError('not an XVAG file')
    big = bool(data[8] & 1)
    # start, channels, codec, samples, samples (again), interleave factor, sample rate, data size
    fields = struct.unpack(('>' if big else '<') + '8I', data[4:8] + data[0x28:0x44])
    return big, fields[0], fields[1], fields[2], fields[3], fields[6]


def replacement_frames(data, rate):
    """The PS-ADPCM frames of a mono recording, as a list of 16-byte frames (VAGp or XVAG)."""
    if data[:4] == b'VAGp':
        if len(data) < 0x40 or struct.unpack('>I', data[0x10:0x14])[0] != rate:
            raise ValueError('sample rate differs')
        body = data[0x30:]
        if len(body) % 16:
            raise ValueError('VAGp data is not a whole number of frames')
        frames = [body[i:i + 16] for i in range(0, len(body), 16)]
        if len(frames) < 3 or any(frames[0]):
            raise ValueError('VAGp lacks the blank first frame')
        if frames[-1][1] != 0x07:
            raise ValueError('VAGp lacks the 0x07 end frame')
        return frames[1:-1]
    _big, start, channels, codec, samples, file_rate = read_xvag_header(data)
    if channels != 1 or codec != 6:
        raise ValueError('recording is not mono PS-ADPCM')
    if file_rate != rate:
        raise ValueError('sample rate differs')
    return [data[start + i * 16:start + i * 16 + 16] for i in range(samples // 28)]


def swap_voice(original, recording):
    """The original XVAG file with the frames of `recording` (same length required). Raises ValueError otherwise."""
    _big, start, channels, codec, samples, rate = read_xvag_header(original)
    if channels != 1 or codec != 6:
        raise ValueError('original is not mono PS-ADPCM')
    frames = replacement_frames(recording, rate)
    count = samples // 28
    if samples % 28 or len(frames) != count:
        raise ValueError(f'frame count differs: recording {len(frames)} vs original {count}')
    payload = bytearray(original[start:])
    if len(payload) < count * 16:
        raise ValueError('original is shorter than its own header says')
    for i, frame in enumerate(frames):
        payload[i * 16:i * 16 + 16] = bytes([frame[0], payload[i * 16 + 1]]) + frame[2:]
    return original[:start] + bytes(payload)


def is_inside(inner, outer):
    inner, outer = Path(inner).resolve(), Path(outer).resolve()
    return inner == outer or outer in inner.parents


def load_source(source):
    """{UPPERCASE NAME: bytes provider} for a .psarc or a folder of NAME.XVAG files."""
    source = Path(source)
    if source.is_dir():
        files = {p.name.upper(): p for p in source.rglob('*') if p.is_file()}
        return {n: p.read_bytes for n, p in files.items()}, None
    archive = Psarc(source)
    return {n: (lambda i=i: archive.read(i)) for n, i in archive.names.items()}, archive


def run(game, source, update, language='PORTUGUESE', only=None, dry_run=False, reference=None):
    """Writes the swapped files to `update`; returns {'written': [...], 'same': [...], 'refused': {reason: [names]}}.

    `reference` is the unmodified archive or folder `source` was made from: recordings identical to it are skipped."""
    game, update = Path(game), Path(update)
    if not game.is_dir():
        raise ValueError(f'{game}: not a folder')
    if is_inside(update, game) or is_inside(game, update):
        raise ValueError('UPDATE_DIR and GAME_DIR must be two separate folders')
    voices = next((p for p in game.iterdir() if p.is_dir() and p.name.upper() == language.upper()), None)
    voices = voices / 'SOUNDS' / 'STREAMS_PS4' if voices else None
    if voices is None or not voices.is_dir():
        raise ValueError(f'{game}: no {language}/SOUNDS/STREAMS_PS4 folder')
    wanted = {l.strip().upper() for l in Path(only).read_text(encoding='utf-8').splitlines() if l.strip()} if only else None
    recordings, archive = load_source(source)
    untouched, reference_archive = load_source(reference) if reference else ({}, None)
    result = {'written': [], 'same': [], 'refused': {}}
    target = update / voices.relative_to(game)
    try:
        for original in sorted(p for p in voices.iterdir() if p.is_file()):
            name = original.name.upper()
            if name not in recordings or (wanted is not None and name not in wanted):
                continue
            old, new_audio = original.read_bytes(), recordings[name]()
            if new_audio == old or (name in untouched and new_audio == untouched[name]()):
                result['same'].append(original.name)
                continue
            try:
                new = swap_voice(old, new_audio)
            except ValueError as error:
                result['refused'].setdefault(str(error).split(':')[0], []).append(original.name)
                continue
            if new == old:
                result['same'].append(original.name)
                continue
            if not dry_run:
                target.mkdir(parents=True, exist_ok=True)
                (target / original.name).write_bytes(new)
            result['written'].append(original.name)
    finally:
        for opened in (archive, reference_archive):
            if opened:
                opened.close()
    if result['written'] and not dry_run:
        # The launcher accepts an Update folder only with the game's eboot.bin and sce_sys/param.sfo (identical copies).
        for relative in ('eboot.bin', 'sce_sys/param.sfo'):
            found = find_file(game, relative)
            if found and not (update / relative).exists():
                (update / relative).parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(found, update / relative)
    return result


def find_file(folder, relative):
    """`relative` (with / separators) inside `folder`, matching names without regard to case; None if absent."""
    path = Path(folder)
    for part in relative.split('/'):
        path = next((p for p in path.iterdir() if p.name.casefold() == part.casefold()), None) if path.is_dir() else None
        if path is None:
            return None
    return path if path.is_file() else None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('game', type=Path)
    parser.add_argument('source', type=Path)
    parser.add_argument('update', type=Path)
    parser.add_argument('--language', default='PORTUGUESE', help='voice folder of the game (default PORTUGUESE)')
    parser.add_argument('--only', type=Path, help='text file with the NAME.XVAG files to handle, one per line')
    parser.add_argument('--reference', type=Path,
                        help='the unmodified archive or folder SOURCE was made from: recordings identical to it are left alone')
    parser.add_argument('--dry-run', action='store_true', help='report only, write nothing')
    args = parser.parse_args(argv)
    try:
        result = run(args.game, args.source, args.update, args.language, args.only, args.dry_run, args.reference)
    except (OSError, ValueError, zlib.error) as error:
        print(f'xvag_voice_swap: {error}', file=sys.stderr)
        return 1
    verb = 'would be written' if args.dry_run else 'written'
    print(f'{len(result["written"])} files {verb} to {args.update}')
    print(f'{len(result["same"])} lines skipped: the recording is the original audio')
    for reason, names in sorted(result['refused'].items(), key=lambda kv: -len(kv[1])):
        print(f'{len(names)} refused ({reason}), e.g. {", ".join(names[:3])}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
