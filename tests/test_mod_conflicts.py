import tempfile
import unittest
from pathlib import Path

import paths  # noqa: F401  (puts scripts/ on sys.path)
import mods


class ModConflictTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def mod(self, name, *files):
        for file in files:
            path = self.root / name / 'dvdroot_ps4' / file
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b'x')

    def test_last_in_order_wins(self):
        self.mod('a', 'chr/kratos.wad', 'ui/hud.wad')
        self.mod('b', 'chr/kratos.wad')
        self.mod('c', 'CHR/Kratos.wad', 'music.wad')
        found = mods.conflicts(self.root, ['a', 'b', 'c'])
        self.assertEqual(len(found), 1)
        path, owners = found[0]
        self.assertEqual(path.casefold(), 'dvdroot_ps4/chr/kratos.wad')
        self.assertEqual(owners, ['a', 'b', 'c'])  # c, the last one, wins

    def test_only_given_mods_count(self):
        self.mod('a', 'x.wad')
        self.mod('b', 'x.wad')
        self.assertEqual(mods.conflicts(self.root, ['a']), [])

    def test_no_conflict_and_broken_mod(self):
        self.mod('a', 'x.wad')
        self.mod('b', 'y.wad')
        (self.root / 'broken').mkdir()
        self.assertEqual(mods.conflicts(self.root, ['a', 'b', 'broken', 'missing']), [])

    def test_preset_skips_missing_and_appends_new(self):
        order, disabled = mods.apply_preset({'order': ['b', 'gone', 'a'], 'disabled': ['a', 'gone']},
                                            ['a', 'b', 'c'])
        self.assertEqual(order, ['b', 'a', 'c'])
        self.assertEqual(disabled, ['a'])

    def test_preset_with_bad_shape(self):
        self.assertEqual(mods.apply_preset({'order': 'nope', 'disabled': None}, ['a']), (['a'], []))


if __name__ == '__main__':
    unittest.main()
