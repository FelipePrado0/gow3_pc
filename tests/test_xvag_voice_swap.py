from paths import ROOT  # noqa: F401  (puts scripts/ on the path)
import hashlib
import shutil
import struct
import sys
import tempfile
import unittest
import zlib
from pathlib import Path

sys.path.insert(0, str(ROOT / 'tools'))
import xvag_voice_swap as swap  # noqa: E402


def frame(seed, flag=0):
    """A fake PS-ADPCM frame: shift/predictor byte, flag byte, 14 data bytes."""
    return bytes([0x0c, flag]) + bytes((seed + i) & 0xff for i in range(14))


def xvag(frames, big=True, padding=0, channels=1, codec=6, rate=48000):
    """A mono XVAG file made of `frames`, laid out like the game's own (0x20 preamble, fmat, end chunk)."""
    e = '>' if big else '<'
    samples = len(frames) * 28
    data = b''.join(frames) + bytes(padding)
    fmat = struct.pack(e + '7I', channels, codec, samples, samples, 1, rate, len(data))
    header = b'XVAG' + struct.pack(e + 'I', 0x4c) + bytes([1 if big else 2, 0, 0, 0x60]) + bytes(20)
    header += b'fmat' + struct.pack(e + 'I', 0x1c) + fmat + b'0000' + bytes(4)
    assert len(header) == 0x4c
    return header + data


def vagp(frames, rate=48000, blank=True, end=True):
    """A Sony VAG file: 0x30 byte header, blank first frame, the frames, 0x07 end frame."""
    body = (bytes(16) if blank else b'') + b''.join(frames) + (bytes([0, 7]) + bytes(14) if end else b'')
    return b'VAGp' + struct.pack('>4I', 0x20, 0, len(body), rate) + bytes(12) + bytes(16) + body


def psarc(files, shuffle=True):
    """A zlib PSARC holding `files` ({'/DIR/NAME': bytes}); TOC entries are stored in reverse to the name list."""
    names = list(files)
    entries = [(bytes(16), '\n'.join(names).encode())]
    order = reversed(names) if shuffle else names
    entries += [(hashlib.md5(n.encode()).digest(), files[n]) for n in order]
    blocks, first, stored = [], [], []
    for _md5, data in entries:
        first.append(len(blocks))
        packed = zlib.compress(data)
        packed = packed if len(packed) < len(data) else data
        blocks.append(len(packed))
        stored.append(packed)
    toc_length = 32 + 30 * len(entries) + 2 * len(blocks)
    toc, offset = b'', toc_length
    for (md5, data), index, packed in zip(entries, first, stored):
        toc += md5 + struct.pack('>I', index) + len(data).to_bytes(5, 'big') + offset.to_bytes(5, 'big')
        offset += len(packed)
    header = b'PSAR' + struct.pack('>HH', 1, 4) + b'zlib' + struct.pack('>5I', toc_length, 30, len(entries), 65536, 2)
    return header + toc + b''.join(struct.pack('>H', b) for b in blocks) + b''.join(stored)


ORIGINAL = [frame(10 * i, flag=1 if i == 3 else 0) for i in range(4)]
DUBBED = [frame(100 + 10 * i) for i in range(4)]


class SwapVoiceTests(unittest.TestCase):
    def test_only_the_frames_change(self):
        for big in (True, False):
            original, recording = xvag(ORIGINAL, big), vagp(DUBBED)
            new = swap.swap_voice(original, recording)
            self.assertEqual(len(new), len(original))
            self.assertEqual(new[:0x4c], original[:0x4c])  # header untouched
            for i, (old_frame, dubbed) in enumerate(zip(ORIGINAL, DUBBED)):
                got = new[0x4c + 16 * i:0x4c + 16 * i + 16]
                self.assertEqual(got[0], dubbed[0])
                self.assertEqual(got[1], old_frame[1])  # the original's flag byte stays
                self.assertEqual(got[2:], dubbed[2:])

    def test_padding_after_the_frames_is_kept(self):
        original = xvag(ORIGINAL, big=False, padding=40)
        new = swap.swap_voice(original, vagp(DUBBED))
        self.assertEqual(len(new), len(original))
        self.assertEqual(new[-40:], bytes(40))

    def test_an_xvag_recording_works_like_a_vagp_one(self):
        from_xvag = swap.swap_voice(xvag(ORIGINAL, big=False), xvag(DUBBED, big=True, padding=7))
        from_vagp = swap.swap_voice(xvag(ORIGINAL, big=False), vagp(DUBBED))
        self.assertEqual(from_xvag, from_vagp)

    def test_what_does_not_fit_is_refused_with_a_reason(self):
        original = xvag(ORIGINAL)
        cases = {
            'frame count differs': vagp(DUBBED + [frame(1)]),
            'sample rate differs': vagp(DUBBED, rate=44100),
            'VAGp lacks the blank first frame': vagp(DUBBED, blank=False),
            'VAGp lacks the 0x07 end frame': vagp(DUBBED, end=False),
            'recording is not mono PS-ADPCM': xvag(DUBBED, channels=2),
        }
        for reason, recording in cases.items():
            with self.assertRaisesRegex(ValueError, reason):
                swap.swap_voice(original, recording)
        for bad in (xvag(ORIGINAL, channels=2), xvag(ORIGINAL, codec=8), b'not an xvag file' * 8):
            with self.assertRaises(ValueError):
                swap.swap_voice(bad, vagp(DUBBED))


class RunTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root, True)
        self.game, self.update, self.source = self.root / 'game', self.root / 'update', self.root / 'source'
        voices = self.game / 'PORTUGUESE/SOUNDS/STREAMS_PS4'
        voices.mkdir(parents=True)
        (self.game / 'eboot.bin').write_bytes(b'eboot')
        (self.game / 'sce_sys').mkdir()
        (self.game / 'sce_sys/param.sfo').write_bytes(b'sfo')
        self.originals = {'A.XVAG': xvag(ORIGINAL), 'B.XVAG': xvag(ORIGINAL), 'C.XVAG': xvag(ORIGINAL)}
        for name, data in self.originals.items():
            (voices / name).write_bytes(data)
        (voices / 'CINEMATICS').mkdir()
        (voices / 'CINEMATICS/A.XVAG').write_bytes(b'cinematic')
        self.source.mkdir()
        (self.source / 'a.xvag').write_bytes(vagp(DUBBED))                    # fits
        (self.source / 'b.xvag').write_bytes(self.originals['B.XVAG'])        # the original audio
        (self.source / 'c.xvag').write_bytes(vagp(DUBBED + [frame(9)]))       # wrong length
        (self.source / 'other.xvag').write_bytes(vagp(DUBBED))                # not a line of the game

    def snapshot(self):
        return {p.relative_to(self.game).as_posix(): p.read_bytes() for p in self.game.rglob('*') if p.is_file()}

    def test_writes_the_fitting_lines_to_the_update_folder_and_leaves_the_game_alone(self):
        before = self.snapshot()
        result = swap.run(self.game, self.source, self.update)
        self.assertEqual(result['written'], ['A.XVAG'])
        self.assertEqual(result['same'], ['B.XVAG'])
        self.assertEqual(result['refused'], {'frame count differs': ['C.XVAG']})
        self.assertEqual(self.snapshot(), before)
        voices = self.update / 'PORTUGUESE/SOUNDS/STREAMS_PS4'
        self.assertEqual(sorted(p.name for p in voices.iterdir()), ['A.XVAG'])
        self.assertEqual(voices.joinpath('A.XVAG').read_bytes(), swap.swap_voice(self.originals['A.XVAG'], vagp(DUBBED)))
        self.assertEqual((self.update / 'eboot.bin').read_bytes(), b'eboot')       # the launcher needs both
        self.assertEqual((self.update / 'sce_sys/param.sfo').read_bytes(), b'sfo')

    def test_dry_run_writes_nothing(self):
        result = swap.run(self.game, self.source, self.update, dry_run=True)
        self.assertEqual(result['written'], ['A.XVAG'])
        self.assertFalse(self.update.exists())

    def test_reference_leaves_unchanged_recordings_alone(self):
        reference = self.root / 'reference'
        reference.mkdir()
        (reference / 'a.xvag').write_bytes(vagp(DUBBED))   # same as the source: this line was not redubbed
        result = swap.run(self.game, self.source, self.update, reference=reference)
        self.assertEqual(result['written'], [])
        self.assertIn('A.XVAG', result['same'])

    def test_only_limits_the_lines(self):
        names = self.root / 'names.txt'
        names.write_text('c.xvag\n')
        result = swap.run(self.game, self.source, self.update, only=names)
        self.assertEqual(result['written'], [])
        self.assertEqual(list(result['refused']), ['frame count differs'])

    def test_the_update_folder_cannot_be_the_game_or_inside_it(self):
        for folder in (self.game, self.game / 'new'):
            with self.assertRaises(ValueError):
                swap.run(self.game, self.source, folder)
        self.assertFalse((self.game / 'new').exists())

    def test_a_psarc_source_is_read_by_name_whatever_the_entry_order(self):
        archive = self.root / 'dub.psarc'
        archive.write_bytes(psarc({'/SOUNDS/STREAMS/A.XVAG': vagp(DUBBED), '/SOUNDS/STREAMS/B.XVAG': vagp(DUBBED),
                                   '/SOUNDS/STREAMS/CINEMATICS/Z.XVAG': b'zz'}))
        result = swap.run(self.game, archive, self.update)
        self.assertEqual(sorted(result['written']), ['A.XVAG', 'B.XVAG'])
        self.assertEqual((self.update / 'PORTUGUESE/SOUNDS/STREAMS_PS4/B.XVAG').read_bytes(),
                         swap.swap_voice(self.originals['B.XVAG'], vagp(DUBBED)))

    def test_cli_reports_and_returns_zero(self):
        self.assertEqual(swap.main([str(self.game), str(self.source), str(self.update), '--dry-run']), 0)
        self.assertEqual(swap.main([str(self.game), str(self.game), str(self.game / 'x')]), 1)


if __name__ == '__main__':
    unittest.main()
