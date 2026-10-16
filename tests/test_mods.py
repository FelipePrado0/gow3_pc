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
from unittest import mock
import zipfile

spec = importlib.util.spec_from_file_location('bbmods', ROOT / 'scripts/mods.py')
mods = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mods)


class ModTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()  # the long name: build_overlay resolves the game folder (macOS: /var is /private/var)
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

    PATCH = '<Patch><Metadata Name="Resolution Patch - 3440x1440" AppVer="01.02"><PatchList/></Metadata></Patch>'

    def patch_mod(self, name, files=('mine.xml',), wrapper=None):
        folder = self.moddir / name / (wrapper or '') / 'patches'
        folder.mkdir(parents=True, exist_ok=True)
        for file in files:
            (folder / file).write_text(self.PATCH)
        return self.moddir / name

    def test_a_mod_with_only_patches_is_a_mod_and_replaces_no_file(self):
        folder = self.patch_mod('Only patches')
        self.mod('Files')
        self.assertEqual(mods.discover(self.moddir), ['Files', 'Only patches'])
        self.assertEqual(list(mods.mod_files(folder)), [])
        result = mods.build_overlay(self.game, self.root / 'out', [('Only patches', folder)])
        self.assertEqual(result, self.game)  # nothing to link

    def test_patch_files_of_a_mod_and_its_wrapper_folder(self):
        self.patch_mod('A', ('b.xml', 'a.xml', 'notes.txt'))
        self.patch_mod('Wrapped', wrapper='Wrapped v1')
        self.assertEqual([f.name for f in mods.patch_files(self.moddir / 'A')], ['a.xml', 'b.xml'])
        self.assertEqual([f.name for f in mods.patch_files(self.moddir / 'Wrapped')], ['mine.xml'])
        self.assertEqual(mods.patch_files(self.root / 'missing'), [])

    def test_enabled_patch_files_follow_the_selection(self):
        self.patch_mod('A')
        self.patch_mod('B', ('b.xml',))
        self.mod('Files only')
        config = self.root / 'mods.json'
        config.write_text(json.dumps({'order': ['B', 'A'], 'disabled': []}))
        names = lambda enabled=True: [f.parent.parent.name + '/' + f.name
                                      for f in mods.enabled_patch_files(self.moddir, config, enabled)]
        self.assertEqual(names(), ['B/b.xml', 'A/mine.xml'])  # load order
        config.write_text(json.dumps({'order': ['B', 'A'], 'disabled': ['B']}))
        self.assertEqual(names(), ['A/mine.xml'])
        self.assertEqual(names(False), [])  # "Load mods" off

    def make_zip(self, name, entries):
        """<mods>/<name>.zip with {path: bytes}; returns its path."""
        self.moddir.mkdir(exist_ok=True)
        path = self.moddir / f'{name}.zip'
        with zipfile.ZipFile(path, 'w') as archive:
            for member, data in entries.items():
                archive.writestr(member, data)
        return path

    def test_a_zip_in_the_mods_folder_is_unpacked_and_is_a_mod(self):
        self.make_zip('Ultra', {'patches/u.xml': self.PATCH, 'README.md': 'hello'})
        self.make_zip('Assets', {'dvdroot_ps4/data/a.wad': b'zipped'})
        self.assertEqual(mods.discover(self.moddir), ['Assets', 'Ultra'])
        self.assertEqual([f.name for f in mods.patch_files(self.moddir / 'Ultra')], ['u.xml'])
        result = mods.build_overlay(self.game, self.root / 'out', [('Assets', self.moddir / 'Assets')])
        self.assertEqual((result / 'dvdroot_ps4/data/a.wad').read_bytes(), b'zipped')
        self.assertTrue((self.moddir / 'Ultra' / mods.ZIP_STAMP).is_file())
        self.assertTrue((self.moddir / 'Ultra.zip').is_file())  # the zip stays

    def test_a_zip_with_a_folder_around_the_mod(self):
        self.make_zip('Wrapped', {'Wrapped v1/patches/u.xml': self.PATCH})
        self.assertEqual(mods.discover(self.moddir), ['Wrapped'])
        self.assertEqual([f.name for f in mods.patch_files(self.moddir / 'Wrapped')], ['u.xml'])

    def test_a_changed_zip_is_unpacked_again_without_leftovers(self):
        zip_path = self.make_zip('Ultra', {'patches/old.xml': self.PATCH})
        mods.discover(self.moddir)
        self.assertTrue((self.moddir / 'Ultra/patches/old.xml').is_file())
        zip_path.unlink()
        self.make_zip('Ultra', {'patches/new.xml': self.PATCH, 'extra.txt': 'x'})
        os.utime(zip_path, (1, 1))  # another time stamp, whatever the file system's resolution
        mods.discover(self.moddir)
        self.assertEqual([f.name for f in mods.patch_files(self.moddir / 'Ultra')], ['new.xml'])
        self.assertFalse((self.moddir / 'Ultra/patches/old.xml').exists())

    def test_a_folder_the_user_made_is_never_replaced_by_a_zip(self):
        self.patch_mod('Ultra', ('mine.xml',))
        self.make_zip('Ultra', {'patches/zipped.xml': self.PATCH})
        mods.discover(self.moddir)
        self.assertEqual([f.name for f in mods.patch_files(self.moddir / 'Ultra')], ['mine.xml'])
        self.assertFalse((self.moddir / 'Ultra' / mods.ZIP_STAMP).exists())

    def test_unsafe_zips_are_refused(self):
        outside = self.root / 'evil.txt'
        self.make_zip('Slip', {'../evil.txt': 'x', 'patches/u.xml': self.PATCH})
        self.make_zip('Absolute', {'/abs.txt': 'x', 'patches/u.xml': self.PATCH})
        self.make_zip('Drive', {'C:/drive.txt': 'x', 'patches/u.xml': self.PATCH})
        link = self.moddir / 'Link.zip'
        with zipfile.ZipFile(link, 'w') as archive:
            info = zipfile.ZipInfo('patches/u.xml')
            info.external_attr = (0o120777 << 16)  # a symbolic link
            archive.writestr(info, 'target')
        self.assertEqual(mods.discover(self.moddir), [])
        for name in ('Slip', 'Absolute', 'Drive', 'Link'):
            self.assertFalse((self.moddir / name).exists(), name)
        self.assertFalse(outside.exists())

    def test_a_zip_that_is_not_a_mod_or_not_a_zip_is_skipped(self):
        self.make_zip('Notes', {'readme.txt': 'nothing to load'})
        (self.moddir / 'Broken.zip').write_bytes(b'not a zip')
        self.assertEqual(mods.discover(self.moddir), [])
        self.assertFalse((self.moddir / 'Broken').exists())

    def test_nested_drive_and_alternate_stream_paths_are_refused(self):
        for member in ('wrapper/E:/outside.txt', 'wrapper/E:outside.txt',
                       'wrapper\\E:\\outside.txt', 'patches/u.xml:stream'):
            with self.subTest(member=member):
                path = self.make_zip('Unsafe', {'patches/u.xml': self.PATCH, member: 'unsafe'})
                with zipfile.ZipFile(path) as archive:
                    with self.assertRaisesRegex(ValueError, 'unsafe path'):
                        mods.zip_members(archive)

    def test_windows_normalized_traversal_paths_are_refused(self):
        for member in ('patches/.. /outside.txt', 'patches/.../outside.txt',
                       'patches/..\\outside.txt'):
            with self.subTest(member=member):
                path = self.make_zip('Unsafe', {member: 'unsafe'})
                with zipfile.ZipFile(path) as archive:
                    with self.assertRaisesRegex(ValueError, 'unsafe path'):
                        mods.zip_members(archive)

    def test_unsafe_zip_is_refused_before_any_member_is_extracted(self):
        self.make_zip('Unsafe', {'patches/u.xml': self.PATCH, 'wrapper/E:/outside.txt': 'unsafe'})
        with mock.patch.object(zipfile.ZipFile, 'open', side_effect=AssertionError('extraction started')):
            self.assertEqual(mods.unpack_zips(self.moddir), [])
        self.assertFalse((self.moddir / 'Unsafe').exists())

    def test_unresolvable_zip_target_does_not_stop_other_mods(self):
        self.make_zip('Broken', {'patches/u.xml': self.PATCH})
        self.make_zip('Good', {'patches/u.xml': self.PATCH})
        resolve = Path.resolve
        for error in (OSError('unreadable target'), RuntimeError('symlink loop')):
            with self.subTest(error=type(error).__name__):
                def resolve_target(path, *args, **kwargs):
                    if path == self.moddir / 'Broken':
                        raise error
                    return resolve(path, *args, **kwargs)
                with mock.patch.object(Path, 'resolve', autospec=True, side_effect=resolve_target):
                    mods.unpack_zips(self.moddir)
                self.assertTrue((self.moddir / 'Good/patches/u.xml').is_file())
                self.assertFalse((self.moddir / 'Broken').exists())

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
