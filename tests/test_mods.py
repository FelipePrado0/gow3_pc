from paths import ROOT
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('bbmods', ROOT / 'scripts/mods.py')
mods = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mods)


class ModTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.game = self.root / 'CUSA01623'
        self.assets = self.game / 'dvdroot_ps4' / 'data'
        self.assets.mkdir(parents=True)
        (self.game / 'eboot.bin').write_bytes(b'original executable')
        (self.assets / 'a.wad').write_bytes(b'original')
        (self.assets / 'b.wad').write_bytes(b'untouched')
        self.moddir = self.root / 'mods'

    def mod(self, name, contents=b'mod', file='a.wad'):
        root = self.moddir / name
        folder = root / 'dvdroot_ps4' / 'data'
        folder.mkdir(parents=True, exist_ok=True)
        (folder / file).write_bytes(contents)
        return root

    def test_merge_replacement_new_file_and_directory_listing(self):
        a = self.mod('A')
        self.mod('A', b'new', 'new.wad')
        result = mods.build_overlay(self.game, self.root / 'out', [('A', a)])
        folder = result / 'dvdroot_ps4' / 'data'
        self.assertEqual((folder / 'a.wad').read_bytes(), b'mod')
        self.assertEqual((folder / 'b.wad').read_bytes(), b'untouched')
        self.assertEqual((folder / 'new.wad').read_bytes(), b'new')
        self.assertEqual({p.name for p in folder.iterdir()}, {'a.wad', 'b.wad', 'new.wad'})
        self.assertEqual((self.assets / 'a.wad').read_bytes(), b'original')
        self.assertFalse((self.assets / 'new.wad').exists())
        self.assertEqual((result / 'eboot.bin').read_bytes(), b'original executable')

    def test_last_mod_wins_and_reordering_changes_winner(self):
        a, b = self.mod('A', b'A'), self.mod('B', b'B')
        for layers, winner in [([('A', a), ('B', b)], b'B'), ([('B', b), ('A', a)], b'A')]:
            result = mods.build_overlay(self.game, self.root / 'out', layers)
            self.assertEqual((result / 'dvdroot_ps4/data/a.wad').read_bytes(), winner)

    def test_selection_disable_and_new_mod(self):
        for name in ['C', 'B', 'A']:
            self.mod(name)
        config = self.root / 'mods.json'
        config.write_text(json.dumps({'order': ['B', 'A', 'deleted'], 'disabled': ['A']}))
        self.assertEqual(mods.selected(self.moddir, config), ['B', 'C'])

    def test_no_mod_returns_original_game(self):
        self.assertEqual(mods.build_overlay(self.game, self.root / 'out', []), self.game)
        self.assertFalse((self.root / 'out').exists())

    def test_directory_conflict_does_not_modify_base(self):
        a = self.mod('A')
        (a / 'dvdroot_ps4/data/a.wad').unlink()
        nested = a / 'dvdroot_ps4/data/b.wad'
        nested.mkdir()
        (nested / 'child').write_bytes(b'bad')
        with self.assertRaises(ValueError):
            mods.build_overlay(self.game, self.root / 'out', [('A', a)])
        self.assertEqual((self.assets / 'b.wad').read_bytes(), b'untouched')
        self.assertEqual(list((self.root / 'out').iterdir()), [])

    def test_mod_symlinks_rejected(self):
        a = self.mod('A')
        (a / 'dvdroot_ps4/escape').symlink_to(self.assets, target_is_directory=True)
        with self.assertRaises(ValueError):
            mods.build_overlay(self.game, self.root / 'out', [('A', a)])

    def test_executable_replacement_rejected(self):
        a = self.mod('A')
        (a / 'eboot.bin').write_bytes(b'unsupported')
        with self.assertRaises(ValueError):
            mods.build_overlay(self.game, self.root / 'out', [('A', a)])

    def test_shadps4_sibling_folder_and_global_disable(self):
        legacy = Path(str(self.game) + '-mods')
        folder = legacy / 'dvdroot_ps4/data'
        folder.mkdir(parents=True)
        (folder / 'a.wad').write_bytes(b'legacy')
        for enabled, expected in [('1', b'legacy'), ('0', b'original')]:
            result = subprocess.run([sys.executable, str(ROOT / 'scripts/mods.py'), str(self.game),
                '--out', str(self.root / 'out'), '--mods-dir', str(self.moddir), '--enabled', enabled],
                capture_output=True, text=True, check=True)
            self.assertEqual((Path(result.stdout.strip()) / 'dvdroot_ps4/data/a.wad').read_bytes(), expected)

    def test_app0_wrapped_mod_discovered(self):
        self.moddir.mkdir()
        (self.moddir / 'Wrapped/app0/dvdroot_ps4').mkdir(parents=True)
        self.assertEqual(mods.discover(self.moddir), ['Wrapped'])

    def test_other_case_replaces_the_game_file(self):
        root = self.moddir / 'Upper'
        (root / 'DVDROOT_PS4/Data').mkdir(parents=True)
        (root / 'DVDROOT_PS4/Data/A.WAD').write_bytes(b'upper')
        result = mods.build_overlay(self.game, self.root / 'out', [('Upper', root)])
        folder = result / 'dvdroot_ps4/data'
        self.assertEqual((folder / 'a.wad').read_bytes(), b'upper')
        self.assertEqual({p.name for p in folder.iterdir()}, {'a.wad', 'b.wad'})
        self.assertEqual({p.name for p in result.iterdir()}, {'dvdroot_ps4', 'eboot.bin'})

    def test_wrapped_layouts(self):
        layouts = {'Archive': 'Archive v1.2/dvdroot_ps4/data', 'Nested': 'Nested/app0/dvdroot_ps4/data'}
        for name, path in layouts.items():
            folder = self.moddir / name / path
            folder.mkdir(parents=True)
            (folder / 'a.wad').write_bytes(name.encode())
        (self.moddir / 'Archive/readme.txt').write_text('notes')
        (self.moddir / 'Junk/stuff').mkdir(parents=True)
        self.assertEqual(mods.discover(self.moddir), ['Archive', 'Nested'])
        for name in layouts:
            with self.subTest(name=name):
                result = mods.build_overlay(self.game, self.root / 'out',
                                            [(name, self.moddir / name)])
                self.assertEqual((result / 'dvdroot_ps4/data/a.wad').read_bytes(), name.encode())

    def test_invalid_profile_fails(self):
        self.mod('A')
        config = self.root / 'mods.json'
        config.write_text('{"disabled":"A"}')
        with self.assertRaises(ValueError):
            mods.selected(self.moddir, config)

    def test_run_uses_overlay_propagates_exit_and_cleans_view(self):
        self.mod('A')
        python = self.root / 'python'
        python.write_text(f'#!{sys.executable}\nimport subprocess,sys\n'
            'if sys.argv[1] == "scripts/mods.py" or sys.argv[1] == "-c":\n'
            '    sys.exit(subprocess.call([sys.executable,*sys.argv[1:]]))\n')
        python.chmod(0o755)
        probe = self.root / 'probe'
        probe.write_text(f'#!{sys.executable}\nimport json,sys,os\nfrom pathlib import Path\n'
            'game=Path(sys.argv[sys.argv.index("--app0")+1])\n'
            'Path(os.environ["GOW3_DATA_DIR"],"mounted.json").write_text(json.dumps({\n'
            '"path":str(game),"content":(game/"dvdroot_ps4/data/a.wad").read_text()}))\n'
            'sys.exit(7)\n')
        probe.chmod(0o755)
        env = dict(os.environ, GOW3_PREBUILT='1', GOW3_PROBE=str(probe), PYTHON=str(python),
            GOW3_DATA_DIR=str(self.root), GOW3_GAME_DIR=str(self.game),
            GOW3_MODS_DIR=str(self.moddir), GOW3_MODS_ENABLED='1', GOW3_MODS_CONFIG=str(self.root/'mods.json'))
        result = subprocess.run(['bash', 'run.sh'], cwd=ROOT, env=env, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 7, result.stderr)
        mounted = json.loads((self.root / 'mounted.json').read_text())
        self.assertEqual(mounted['content'], 'mod')
        self.assertFalse(Path(mounted['path']).exists())
        self.assertEqual((self.assets/'a.wad').read_bytes(), b'original')
