from paths import ROOT
import importlib.util
import json
import os
from pathlib import Path
import shutil
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
        self.root = Path(self.temp.name).resolve()  # the long name: build_overlay resolves the game folder
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
        link = a / 'dvdroot_ps4/escape'
        try:
            link.symlink_to(self.assets, target_is_directory=True)
        except OSError:
            if os.name != 'nt':
                raise
            # Without the symlink privilege a junction is the fallback, so it must be
            # rejected just like a symlink (os.walk follows junctions too).
            import _winapi
            _winapi.CreateJunction(str(self.assets), str(link))
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

    @unittest.skipIf(os.name == 'nt', 'the fake probe is a shebang script')
    def test_run_uses_overlay_propagates_exit_and_cleans_view(self):
        self.mod('A')
        # run.py with the game preparation scripts stubbed out: the fake game is not a SELF.
        port = self.root / 'port'
        shutil.copytree(ROOT / 'scripts', port / 'scripts')
        shutil.copy(ROOT / 'run.py', port / 'run.py')
        for name in ('prepare', 'link_libc', 'link_modules', 'content_profile'):
            (port / 'scripts' / f'{name}.py').write_text('')
        probe = self.root / 'probe'
        probe.write_text(f'#!{sys.executable}\nimport json,sys,os\nfrom pathlib import Path\n'
            'game=Path(sys.argv[sys.argv.index("--app0")+1])\n'
            'Path(os.environ["GOW3_DATA_DIR"],"mounted.json").write_text(json.dumps({\n'
            '"path":str(game),"content":(game/"dvdroot_ps4/data/a.wad").read_text()}))\n'
            'sys.exit(7)\n')
        probe.chmod(0o755)
        env = dict(os.environ, GOW3_PROBE=str(probe),
            GOW3_DATA_DIR=str(self.root), GOW3_GAME_DIR=str(self.game),
            GOW3_MODS_DIR=str(self.moddir), GOW3_MODS_ENABLED='1', GOW3_MODS_CONFIG=str(self.root/'mods.json'))
        result = subprocess.run([sys.executable, 'run.py'], cwd=port, env=env, capture_output=True, timeout=30)
        self.assertEqual(result.returncode, 7, result.stderr)
        mounted = json.loads((self.root / 'mounted.json').read_text())
        self.assertEqual(mounted['content'], 'mod')
        self.assertFalse(Path(mounted['path']).exists())
        self.assertEqual((self.assets/'a.wad').read_bytes(), b'original')


class GodOfWarLayout(unittest.TestCase):
    """God of War III keeps its data in EXEC and one folder per language at the game root."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.game = self.root / 'CUSA01623'
        for folder in ('EXEC', 'ENGLISH', 'GERMAN', 'sce_sys', 'sce_module'):
            (self.game / folder).mkdir(parents=True)
        (self.game / 'eboot.bin').write_bytes(b'original executable')
        (self.game / 'EXEC' / 'ui.wad').write_bytes(b'original')
        (self.game / 'ENGLISH' / 'text.wad').write_bytes(b'english')
        self.moddir = self.root / 'mods'
        self.moddir.mkdir()

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def test_exec_and_language_folders(self):
        mod = self.moddir / 'Xbox Prompts'
        (mod / 'exec').mkdir(parents=True)
        (mod / 'exec' / 'UI.WAD').write_bytes(b'xbox')
        (mod / 'English').mkdir()
        (mod / 'English' / 'text.wad').write_bytes(b'xbox text')
        (mod / 'readme.txt').write_text('ignored')
        self.assertEqual(mods.discover(self.moddir), ['Xbox Prompts'])
        result = mods.build_overlay(self.game, self.root / 'out', [('Xbox Prompts', mod)])
        self.assertEqual((result / 'EXEC' / 'ui.wad').read_bytes(), b'xbox')
        self.assertEqual((result / 'ENGLISH' / 'text.wad').read_bytes(), b'xbox text')
        self.assertEqual({p.name for p in (result / 'EXEC').iterdir()}, {'ui.wad'})
        self.assertEqual((self.game / 'EXEC' / 'ui.wad').read_bytes(), b'original')
        self.assertEqual((result / 'eboot.bin').read_bytes(), b'original executable')

    def test_wrapper_folder_around_exec(self):
        mod = self.moddir / 'Archive'
        (mod / 'Archive v1.0' / 'EXEC').mkdir(parents=True)
        (mod / 'Archive v1.0' / 'EXEC' / 'ui.wad').write_bytes(b'wrapped')
        self.assertEqual(mods.discover(self.moddir), ['Archive'])
        result = mods.build_overlay(self.game, self.root / 'out', [('Archive', mod)])
        self.assertEqual((result / 'EXEC' / 'ui.wad').read_bytes(), b'wrapped')

    def test_unknown_region_folder_learned_from_game(self):
        (self.game / 'JAPANESE').mkdir()
        (self.game / 'JAPANESE' / 'text.wad').write_bytes(b'jp')
        mod = self.moddir / 'JP'
        (mod / 'JAPANESE').mkdir(parents=True)
        (mod / 'JAPANESE' / 'text.wad').write_bytes(b'jp mod')
        result = mods.build_overlay(self.game, self.root / 'out', [('JP', mod)])
        self.assertEqual((result / 'JAPANESE' / 'text.wad').read_bytes(), b'jp mod')

    def test_system_folders_rejected(self):
        mod = self.moddir / 'Bad'
        (mod / 'EXEC').mkdir(parents=True)
        (mod / 'sce_sys').mkdir()
        (mod / 'sce_sys' / 'param.sfo').write_bytes(b'x')
        with self.assertRaises(ValueError):
            mods.build_overlay(self.game, self.root / 'out', [('Bad', mod)])
