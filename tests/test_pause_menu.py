"""Exit to Desktop rows (scripts/pause_menu.py). The script is the game's own, so the patch is
checked on the dump's R_UI.WAD when GOW3_GAME_DIR points at one; the refusals need no files."""
import os
import struct
import tempfile
import unittest
from pathlib import Path

from paths import ROOT  # noqa: F401  (puts scripts/ on the path)
import pause_menu
from pause_menu import Script, decode

SEND = bytes([0x13, 0x00, 0x5e, 0x43])  # builtin 0x5e (send a message), then pop
GAME = Path(os.environ.get('GOW3_GAME_DIR', '')) if os.environ.get('GOW3_GAME_DIR') else None


def wad(name, payload):
    return struct.pack('<HHI', 1, 0, len(payload)) + name.encode().ljust(56, b'\0') + payload.ljust(
        (len(payload) + 15) & ~15, b'\0')


def gotos(data, base, spans):
    """{code offset: index of the state it goes to} for the `47` instructions in `spans`."""
    s = Script(data, base)
    index = {r['at'] - 4: i for i, r in enumerate(s.records)}
    return {at - base - s.lit: index[int.from_bytes(arg, 'big')]
            for a, b in spans for at, op, arg in decode(data, base + s.lit + a, base + s.lit + b) if op == 0x47}


class Refusals(unittest.TestCase):
    def test_wad_without_the_script(self):
        with self.assertRaises(ValueError):
            pause_menu.patch(bytearray(wad('TXRX_R_UI', b'x' * 8)))

    def test_build_leaves_no_copy(self):
        with tempfile.TemporaryDirectory() as folder:
            source = Path(folder) / 'R_UI.WAD'
            source.write_bytes(wad('TXRX_R_UI', b'x' * 8))
            self.assertEqual(pause_menu.build(folder, folder, source), {})
            self.assertFalse((Path(folder) / 'pause_menu' / 'R_UI.WAD').exists())


@unittest.skipUnless(GAME and (GAME / pause_menu.WAD).is_file(), 'needs GOW3_GAME_DIR with the game')
class GameScript(unittest.TestCase):
    def setUp(self):
        self.original = (GAME / pause_menu.WAD).read_bytes()
        self.data = bytearray(self.original)
        pause_menu.patch(self.data)
        entry = lambda d: next(o + 0x40 for o, _s, n in pause_menu.entries(d) if n == 'SMI_R_UI')  # noqa: E731
        self.base, self.old_base = entry(self.data), entry(self.original)
        self.old, self.new = Script(self.original, self.old_base), Script(self.data, self.base)

    def test_pause_row(self):
        main, row = self.new.one('PauseMain'), self.new.one('WadChoose')
        self.assertEqual(row['parent'], main['at'] - 4)
        self.assertEqual(main['at'] + main['size'], row['at'] + row['size'])
        self.assertEqual([t for k, _v, t in self.new.blocks(row) if k == 3], [pause_menu.UP, pause_menu.DOWN, pause_menu.CONFIRM, pause_menu.REPLY])
        with self.assertRaises(ValueError):  # a patched copy is refused, not patched twice
            pause_menu.patch(bytearray(self.data))

    def test_exit_asks_quit_games_question_with_the_mark(self):
        quit_ = self.new.one('HLQuitGame')
        block = lambda row, trigger: self.new.block_bytes(pause_menu.transition(self.new, row, trigger))  # noqa: E731
        ask = block(quit_, pause_menu.CONFIRM)
        for row in (self.new.one('WadChoose'), self.new.by_name['HLDisplay'][1]):
            mine = block(row, pause_menu.CONFIRM)
            third = b'\x01' + pause_menu.MARK.to_bytes(4, 'big') + b'\x01' + (5).to_bytes(4, 'big') + SEND  # the mark, the count of 5, the send
            self.assertEqual(mine.count(third), 1)
            self.assertEqual(mine.replace(third, b'\x01' + (4).to_bytes(4, 'big') + SEND), ask[:len(mine) - 5])  # the mark is five bytes longer
            self.assertEqual(block(row, pause_menu.REPLY)[:len(pause_menu.REPLY_TEMPLATE)], pause_menu.REPLY_TEMPLATE)  # "no" as Quit Game

    def test_failsafe_states_end_the_process_on_the_mark(self):
        old_s, new_s = self.old, self.new
        for name, trigger, prefix in (('FailSafeDisplay', pause_menu.REPLY, pause_menu.SHOW_PREFIX),
                                      ('FailSafeYes', pause_menu.CONFIRM, pause_menu.YES_PREFIX)):
            ref_old = pause_menu.transition(old_s, old_s.one(name), trigger)
            ref_new = pause_menu.transition(new_s, new_s.one(name), trigger)
            self.assertNotEqual(ref_old, ref_new)  # the state runs the new block
            before = old_s.block_bytes(ref_old)
            after = new_s.block_bytes(ref_new)
            self.assertTrue(after.startswith(prefix))
            first, second = self.old_base + old_s.lit + ref_old, self.base + new_s.lit + ref_new + len(prefix)
            a = decode(self.original, first, first + len(before))
            b = decode(self.data, second, second + len(before))
            self.assertEqual([(op, arg) for _, op, arg in a if op != 0x47], [(op, arg) for _, op, arg in b if op != 0x47])
            index = lambda script, ref: next(i for i, r in enumerate(script.records) if r['at'] - 4 == ref)  # noqa: E731
            gotos = lambda script, code: [index(script, int.from_bytes(arg, 'big')) for _, op, arg in code if op == 0x47]  # noqa: E731
            self.assertEqual(len(gotos(old_s, a)), len(gotos(new_s, b)))
        # The exit call is skipped unless the local holds the mark.
        code = decode(self.data, self.base + new_s.lit + pause_menu.transition(new_s, new_s.one('FailSafeYes'), pause_menu.CONFIRM),
                      self.base + new_s.lit + pause_menu.transition(new_s, new_s.one('FailSafeYes'), pause_menu.CONFIRM) + len(pause_menu.YES_PREFIX))
        jump = next(c for c in code if c[1] == 0x17)
        self.assertEqual(jump[0] + 3 + struct.unpack('>h', jump[2])[0], code[-1][0] + 1)

    def test_options_row(self):
        options, row = self.new.one('PauseOptions'), self.new.by_name['HLDisplay'][1]
        self.assertEqual(len(self.new.records), len(self.old.records) + 1)
        self.assertEqual(row['parent'], options['at'] - 4)
        self.assertEqual(options['size'], self.old.one('PauseOptions')['size'] + pause_menu.ROW_SIZE)
        self.assertEqual(row['at'], options['at'] + options['size'] - pause_menu.ROW_SIZE)
        # The WAD entry, the script header and the padding agree with the new size.
        size = struct.unpack_from('<I', self.data, self.base - 0x40 + 4)[0]
        old_size = struct.unpack_from('<I', self.original, self.old_base - 0x40 + 4)[0]
        self.assertEqual(struct.unpack_from('<I', self.data, self.base + 8)[0], size - 4)
        self.assertEqual(len(self.data) - len(self.original), ((size + 15) & ~15) - ((old_size + 15) & ~15))
        self.assertEqual([n for _o, _s, n in pause_menu.entries(self.data)][-1], 'GroupEnd')

    def test_every_other_goto_is_unchanged(self):
        dead = self.old.one('WadChoose')  # its code is replaced by the pause row's
        lo = min(v for k, v in dead['items'] if k in (1, 2, 3))
        hi = next(r for r in self.old.code_refs() if r > max(v for k, v in dead['items'] if k in (1, 2, 3)))
        spans = [(a, b) for a, b in self.old.spans() if not lo <= a < hi]
        before = gotos(self.original, self.old_base, spans)
        after = gotos(self.data, self.base, spans)
        options = self.new.one('PauseOptions')
        inserted = next(i for i, r in enumerate(self.new.records) if r['at'] == options['at'] + options['size'] - pause_menu.ROW_SIZE)
        moved = lambda i: i + (i >= inserted)  # noqa: E731  (states after the new record shift by one)
        changed = {at: after[at] for at, b in before.items() if after[at] != moved(b)}
        wad_choose = moved(next(i for i, r in enumerate(self.old.records) if r['at'] == dead['at']))
        # Continue up and Quit Game down go to the pause row; Controls up and Display down to the options row.
        self.assertEqual(sorted(changed.values()), sorted([wad_choose, wad_choose, inserted, inserted]))


if __name__ == '__main__':
    unittest.main()
