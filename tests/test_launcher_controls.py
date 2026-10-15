import os
import queue
import struct
import sys
import unittest

from paths import ROOT
sys.path.insert(0, str(ROOT / 'launcher'))
import gow3_launcher_win as launcher  # noqa: E402


def sfo_bytes(**values):
    """A minimal param.sfo (PSF 1.1) holding string values."""
    entries, keys, data = b'', b'', b''
    for key, value in values.items():
        text = value.encode() + b'\0'
        entries += struct.pack('<HHIII', len(keys), 0x204, len(text), len(text), len(data))
        keys += key.encode() + b'\0'
        data += text
    keys_offset = 20 + len(entries)
    return (struct.pack('<4sIIII', b'\0PSF', 0x101, keys_offset, keys_offset + len(keys), len(values))
            + entries + keys + data)


BASE_SFO = sfo_bytes(TITLE_ID='CUSA01623', APP_VER='01.00')


class ControlMapTest(unittest.TestCase):
    def test_new_cheats_default_to_disabled(self):
        self.assertEqual(launcher.INI_DEFAULTS['cheat_double_jumps'], '0')
        self.assertEqual(launcher.INI_DEFAULTS['cheat_very_hard'], '0')
        self.assertIn('cheat_double_jumps', launcher.INI_FLAGS)
        self.assertIn('cheat_very_hard', launcher.INI_FLAGS)

    def test_pipeline_binary_cache_switch(self):
        settings = {**launcher.APP_DEFAULTS}
        self.assertTrue(settings['pipeline_binary_cache'])
        self.assertEqual(launcher.game_environment(settings)['GOW3_PIPELINE_BINARIES'], '1')
        settings['pipeline_binary_cache'] = False
        self.assertEqual(launcher.game_environment(settings)['GOW3_PIPELINE_BINARIES'], '0')

    def test_changed_buttons_in_table_order(self):
        controls = {'key': {'circle': ' Left Shift ', 'cross': 'Q', 'square': ''}, 'pad': {'cross': 'b'}}
        self.assertEqual(launcher.control_map(controls, 'key'), 'cross=Q,circle=Left Shift')
        self.assertEqual(launcher.control_map(controls, 'pad'), 'cross=b')

    def test_rejects_separators_unknown_buttons_and_bad_shapes(self):
        controls = {'key': {'cross': 'a,b', 'circle': 'x=y', 'jump': 'Q', 'square': 5}}
        self.assertEqual(launcher.control_map(controls, 'key'), '')
        self.assertEqual(launcher.control_map([], 'key'), '')
        self.assertEqual(launcher.control_map({}, 'pad'), '')

    def test_only_changes_from_the_defaults_are_kept(self):
        keys = {b: k for b, k, _p in launcher.CONTROLS}
        keys.update(cross='q', circle='LEFT SHIFT', square='  ')
        self.assertEqual(launcher.control_changes(keys, 'key'), {'cross': 'q'})
        pads = {b: p or '' for b, _k, p in launcher.CONTROLS}
        pads.update(cross='b', l2='a')
        self.assertEqual(launcher.control_changes(pads, 'pad'), {'cross': 'b'})  # L2/R2: triggers only

    def test_touchpad_sides_are_separate_buttons(self):
        rows = {b: (k, p) for b, k, p in launcher.CONTROLS}
        self.assertNotIn('touchpad', rows)
        self.assertEqual(rows['touchpad_left'], ('Tab', None))  # photo mode: L1 + the right-click button
        self.assertEqual(rows['touchpad_right'], ('Backspace', 'back'))  # pause menu
        self.assertEqual(launcher.control_map({'pad': {'touchpad_right': 'guide'}}, 'pad'), 'touchpad_right=guide')
        self.assertEqual(launcher.control_map({'key': {'touchpad_right': 'Delete'}}, 'key'), 'touchpad_right=Delete')

    def test_saved_touchpad_key_moves_to_the_left_side(self):
        # Settings saved before the sides were split named the left click 'touchpad'.
        self.assertEqual(launcher.control_map({'key': {'touchpad': 'T'}}, 'key'), 'touchpad_left=T')
        self.assertEqual(launcher.control_map({'key': {'touchpad': 'T', 'touchpad_left': 'Y'}}, 'key'), 'touchpad_left=Y')
        self.assertEqual(launcher.control_values({'key': {'touchpad': 'T'}}, 'key'), {'touchpad_left': 'T'})
        self.assertEqual(launcher.control_values({'pad': {'touchpad': 'misc1'}}, 'pad'), {})  # no gamepad box for it

    def test_both_styles_name_every_default_gamepad_button(self):
        for labels in launcher.PAD_STYLES.values():
            self.assertTrue(all(p in labels for _b, _k, p in launcher.CONTROLS if p))
            self.assertEqual(len(set(labels.values())), len(labels))

    def test_environment(self):
        settings = {**launcher.APP_DEFAULTS, 'controls': {'key': {'cross': 'Q'}}}
        env = launcher.game_environment(settings)
        self.assertEqual(env['GOW3_KEY_MAP'], 'cross=Q')
        self.assertNotIn('GOW3_PAD_MAP', env)
        self.assertEqual(env['GOW3_PARALLEL_WARMUP'], '1')
        self.assertEqual(env['GOW3_BACKGROUND_WARMUP'], '1')
        self.assertEqual(env['GOW3_PERF_DIAG'], '0')
        self.assertEqual(env['GOW3_PARTIAL_HEIGHT_VIEWS'], '1')
        self.assertEqual(launcher.game_environment({**settings, 'partial_height_views': False})['GOW3_PARTIAL_HEIGHT_VIEWS'], '0')
        self.assertEqual((env['GOW3_DISK_SHADERS'], env['GOW3_SHADER_PREFETCH'], env['GOW3_SHADER_NOTES']),
                         ('1', '1', '1'))
        off = launcher.game_environment({**settings, 'disk_shaders': False, 'shader_prefetch': False,
                                         'shader_notes': False})
        self.assertEqual((off['GOW3_DISK_SHADERS'], off['GOW3_SHADER_PREFETCH'], off['GOW3_SHADER_NOTES']),
                         ('0', '0', '0'))
        self.assertNotIn('GOW3_FRAME_CAPTURE_KEY', {k for k in env if k not in __import__('os').environ})
        self.assertEqual(launcher.game_environment({**settings, 'frame_capture_key': True})['GOW3_FRAME_CAPTURE_KEY'], '1')
        # Graphics options live in gow3.ini (run.py and the in-game menu), not in the environment.
        for name in ('GOW3_FULLSCREEN', 'GOW3_PRESENT_MODE', 'GOW3_FPS_LIMIT', 'GOW3_ASYNC_SHADERS',
                     'GOW3_DEFERRED_READBACK', 'GOW3_STALE_READBACK', 'GOW3_FSR1', 'GOW3_RCAS'):
            self.assertNotIn(name, {k for k in env if k not in __import__('os').environ})
        old = launcher.game_environment({**settings, 'extra_env': 'GOW3_DRAW_PIPE=0'})
        self.assertNotIn('GOW3_DRAW_PIPE', old)  # the old free-text field is ignored
        self.assertEqual(launcher.game_environment({**settings, 'parallel_warmup': False})['GOW3_PARALLEL_WARMUP'], '0')
        self.assertEqual(launcher.game_environment({**settings, 'background_warmup': False})['GOW3_BACKGROUND_WARMUP'], '0')

    def test_temporal_upscaler_stays_off_with_a_saved_2_1_8_switch(self):
        # 2.1.8 showed the experimental switch; a player who turned it on must not keep it on now that it is hidden.
        for saved in (True, False, None):
            env = launcher.game_environment({**launcher.APP_DEFAULTS, 'experimental_upscaler': saved})
            self.assertEqual(env['GOW3_EXPERIMENTAL_UPSCALER'], '0')

    def test_glyph_style_is_off_by_default(self):
        self.assertEqual(launcher.APP_DEFAULTS['glyph_style'], 'off')
        self.assertNotIn('GOW3_GLYPH_STYLE', launcher.game_environment({**launcher.APP_DEFAULTS}))

    def test_glyph_style_choices_reach_the_game(self):
        for style in ('auto', 'xbox', 'nintendo'):
            env = launcher.game_environment({**launcher.APP_DEFAULTS, 'glyph_style': style})
            self.assertEqual(env['GOW3_GLYPH_STYLE'], style)

    def test_unknown_glyph_style_is_off(self):
        for style in ('playstation', 'XBOX', 7, None, ''):
            env = launcher.game_environment({**launcher.APP_DEFAULTS, 'glyph_style': style})
            self.assertNotIn('GOW3_GLYPH_STYLE', env)

    def test_inherited_glyph_style_is_dropped_when_off(self):
        from unittest import mock

        with mock.patch.dict(launcher.os.environ, {'GOW3_GLYPH_STYLE': 'xbox'}):
            self.assertNotIn('GOW3_GLYPH_STYLE', launcher.game_environment({**launcher.APP_DEFAULTS}))
            self.assertEqual(launcher.game_environment({**launcher.APP_DEFAULTS, 'glyph_style': 'nintendo'})
                             ['GOW3_GLYPH_STYLE'], 'nintendo')

    def test_settings_keys_kept(self):
        for key in ('game_dir', 'user_dir', 'mods_dir', 'patches_dir', 'language', 'hdr',
                    'draw_pipe', 'readbacks', 'background_warmup', 'parallel_warmup', 'controls', 'pad_style', 'glyph_style'):
            self.assertIn(key, launcher.APP_DEFAULTS)
        # The player name option is gone: an old settings.json value is ignored.
        self.assertNotIn('player_name', launcher.APP_DEFAULTS)
        env = launcher.game_environment({**launcher.APP_DEFAULTS, 'player_name': ' Kratos '})
        self.assertNotIn('GOW3_USER_NAME', env)
        for key in ('display_mode', 'fps_limit', 'render_resolution', 'engine_fps'):
            self.assertIn(key, launcher.INI_DEFAULTS)

    def test_graphics_defaults_match_the_game(self):
        d = launcher.INI_DEFAULTS
        self.assertEqual((d['display_mode'], d['vsync'], d['fps_limit'], d['engine_fps']), ('windowed', '1', '0', '120'))
        self.assertEqual([v for v, _t in launcher.FPS_LIMITS], ['30', '60', '120', '240', '0'])
        self.assertEqual(d['frames_queued'], '1')
        self.assertEqual([v for v, _t in launcher.FRAMES_QUEUED], ['1', '2', '0'])
        self.assertNotIn('GOW3_FRAMES_AHEAD', launcher.game_environment({**launcher.APP_DEFAULTS, 'frames_ahead': '2'}))


class GameFilesTest(unittest.TestCase):
    def test_copy_in_progress_blocks_another_play_or_status_refresh(self):
        from unittest import mock

        app = launcher.Launcher.__new__(launcher.Launcher)
        app.process = None
        app.app = {**launcher.APP_DEFAULTS}
        app.ui_calls = queue.Queue()
        for name in ('collect', 'play_button', 'status', 'meta_dir', 'summary', 'messagebox', 'show'):
            setattr(app, name, mock.Mock())
        with mock.patch.object(launcher.threading, 'Thread'), \
                mock.patch.object(launcher, 'game_info', return_value=('God of War III', '01.02')):
            app.adopt_in_background('Sc0', 'Image0')
            app.play()
            app.refresh_status()
        app.collect.assert_not_called()
        self.assertEqual(app.play_button.configure.call_args.kwargs['state'], 'disabled')
        self.assertEqual(app.status.configure.call_args.kwargs['text'], 'Copying the system files…')

    def test_unreadable_executable_is_named_before_starting(self):
        import tempfile
        from pathlib import Path
        from unittest import mock

        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'eboot.bin'
            path.write_bytes(b'x')
            with mock.patch.object(Path, 'open', side_effect=PermissionError('Access is denied')):
                self.assertEqual(launcher.file_state(path), 'unreadable')

    def test_missing_game_files(self):
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            (folder / 'sce_module').mkdir()
            (folder / 'sce_module/libSceFios2.prx').write_bytes(b'x')
            self.assertEqual(launcher.missing_game_files(folder),
                             [('eboot.bin', 'missing'), ('sce_sys/param.sfo', 'missing'), ('sce_module/libc.prx', 'missing')])
            (folder / 'sce_sys').mkdir()
            (folder / 'eboot.bin').write_bytes(b'x')
            (folder / 'sce_sys/param.sfo').write_bytes(BASE_SFO)
            (folder / 'sce_module/libc.prx').write_bytes(b'x')
            self.assertEqual(launcher.missing_game_files(folder), [])
            self.assertEqual(launcher.missing_game_files(folder, launcher.REQUIRED_UPDATE_FILES), [])
            # A param.sfo that is not one (a one-byte placeholder, a copy cut short) is named too.
            (folder / 'sce_sys/param.sfo').write_bytes(b'x')
            self.assertEqual(launcher.missing_game_files(folder), [('sce_sys/param.sfo', 'invalid')])
            self.assertEqual(launcher.describe_missing([('sce_sys/param.sfo', 'invalid')]),
                             ['sce_sys/param.sfo (not a param.sfo)'])
            # One that is there but cannot be read is 'unreadable', whether the stat or the read fails.
            from unittest import mock
            (folder / 'sce_sys/param.sfo').write_bytes(BASE_SFO)
            real_open = Path.open

            def denied_open(path, *args, **kwargs):
                if path.name == 'param.sfo':
                    raise PermissionError(13, 'Access is denied', str(path))
                return real_open(path, *args, **kwargs)
            with mock.patch.object(Path, 'open', denied_open):
                self.assertEqual(launcher.missing_game_files(folder), [('sce_sys/param.sfo', 'unreadable')])
            # A folder in the file's place is named as such, not as missing.
            (folder / 'sce_module/libc.prx').unlink()
            (folder / 'sce_module/libc.prx').mkdir()
            self.assertEqual(launcher.missing_game_files(folder), [('sce_module/libc.prx', 'folder')])
            self.assertEqual(launcher.describe_missing([('sce_module/libc.prx', 'folder')]),
                             ['sce_module/libc.prx (a folder, not a file)'])
            (folder / 'sce_module/libc.prx').rmdir()
            (folder / 'sce_module/libc.prx').write_bytes(b'x')
            # Empty files (a placeholder made by hand, a copy that failed) count as missing: the
            # game cannot start from them, and the message says which they are.
            (folder / 'eboot.bin').write_bytes(b'')
            (folder / 'sce_sys/param.sfo').write_bytes(b'')
            missing = launcher.missing_game_files(folder)
            self.assertEqual(missing, [('eboot.bin', 'empty'), ('sce_sys/param.sfo', 'empty')])
            self.assertEqual(launcher.describe_missing(missing), ['eboot.bin (empty)', 'sce_sys/param.sfo (empty)'])
            with mock.patch.object(launcher, 'LANG', 'pt'):
                self.assertEqual(launcher.describe_missing(missing), ['eboot.bin (vazio)', 'sce_sys/param.sfo (vazio)'])
            # A file this user may not read is there, so it is named as such, not as missing.
            real_stat = Path.stat

            def denied_stat(path, *args, **kwargs):
                if path.name == 'libc.prx':
                    raise PermissionError(13, 'Access is denied', str(path))
                return real_stat(path, *args, **kwargs)
            with mock.patch.object(Path, 'stat', denied_stat):
                missing = launcher.missing_game_files(folder)
            self.assertEqual(missing, [('eboot.bin', 'empty'), ('sce_sys/param.sfo', 'empty'),
                                       ('sce_module/libc.prx', 'unreadable')])
            self.assertEqual(launcher.describe_missing(missing)[2], 'sce_module/libc.prx (unreadable)')

    def test_ready_to_play_needs_a_complete_dump(self):
        # The --play role (Steam, "Play God of War III.exe") opens the launcher instead of
        # starting a dump the Play button would refuse.
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            self.assertFalse(launcher.ready_to_play(folder))
            (folder / 'eboot.bin').write_bytes(b'x')
            self.assertFalse(launcher.ready_to_play(folder))
            (folder / 'sce_sys').mkdir()
            (folder / 'sce_module').mkdir()
            for name, _check in launcher.REQUIRED_GAME_FILES:
                (folder / name).write_bytes(BASE_SFO if name.endswith('param.sfo') else b'x')
            self.assertTrue(launcher.ready_to_play(folder))
            self.assertFalse(launcher.ready_to_play(folder, str(folder)))  # one folder for both
            # The update folder, when set, must be complete as well.
            update = folder / 'update'
            (update / 'sce_sys').mkdir(parents=True)
            (update / 'eboot.bin').write_bytes(b'x')
            self.assertFalse(launcher.ready_to_play(folder, str(update)))
            (update / 'sce_sys/param.sfo').write_bytes(BASE_SFO)
            self.assertTrue(launcher.ready_to_play(folder, str(update)))

    def test_system_files_beside_a_pkg_extraction(self):
        # PKG extractors keep the system files (param.sfo, icon0.png, pic1.png, trophy/) in Sc0/
        # next to the game files in Image0/; the port reads them from the game's sce_sys/.
        import tempfile
        from pathlib import Path
        from unittest import mock
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            game = root / 'Image0'
            (game / 'sce_sys').mkdir(parents=True)
            (game / 'sce_sys/playgo-chunk.dat').write_bytes(b'game')
            (root / 'Sc1').mkdir()  # a sibling without param.sfo is not the one
            self.assertIsNone(launcher.system_files_beside(game))
            (root / 'Sc0/trophy').mkdir(parents=True)
            (root / 'Sc0/param.sfo').write_bytes(BASE_SFO)
            (root / 'Sc0/icon0.png').write_bytes(b'icon')
            (root / 'Sc0/playgo-chunk.dat').write_bytes(b'other')
            (root / 'Sc0/trophy/trophy00.trp').write_bytes(b'trophies')
            self.assertEqual(launcher.system_files_beside(game), root / 'Sc0')
            # param.sfo goes last: while it is missing the copy is offered again after a failure.
            self.assertEqual(launcher.adopt_system_files(root / 'Sc0', game),
                             ['icon0.png', 'trophy/trophy00.trp', 'param.sfo'])
            self.assertEqual((game / 'sce_sys/param.sfo').read_bytes(), BASE_SFO)
            self.assertEqual((game / 'sce_sys/trophy/trophy00.trp').read_bytes(), b'trophies')
            # The game's own files are never replaced, and once param.sfo is there nothing is offered.
            self.assertEqual((game / 'sce_sys/playgo-chunk.dat').read_bytes(), b'game')
            self.assertIsNone(launcher.system_files_beside(game))
            self.assertEqual(launcher.adopt_system_files(root / 'Sc0', game), [])
            # An empty param.sfo made by hand to get past the old check counts as missing and is replaced.
            (game / 'sce_sys/param.sfo').write_bytes(b'')
            self.assertEqual(launcher.system_files_beside(game), root / 'Sc0')

            # A copy that fails leaves nothing half-written behind to be mistaken for the file.
            def cut_short(src, dst):
                Path(dst).write_bytes(b'half')
                raise OSError(28, 'No space left on device', str(dst))
            with mock.patch.object(launcher.shutil, 'copy2', cut_short), self.assertRaises(OSError):
                launcher.adopt_system_files(root / 'Sc0', game)
            self.assertEqual((game / 'sce_sys/param.sfo').read_bytes(), b'')
            self.assertEqual(list((game / 'sce_sys').glob('*.part')), [])
            self.assertEqual(launcher.adopt_system_files(root / 'Sc0', game), ['param.sfo'])
            self.assertEqual((game / 'sce_sys/param.sfo').read_bytes(), BASE_SFO)
            # ... and so does a non-empty one that is not a param.sfo.
            (game / 'sce_sys/param.sfo').write_bytes(b'x')
            self.assertEqual(launcher.system_files_beside(game), root / 'Sc0')
            self.assertEqual(launcher.adopt_system_files(root / 'Sc0', game), ['param.sfo'])
            self.assertEqual((game / 'sce_sys/param.sfo').read_bytes(), BASE_SFO)
            (game / 'sce_sys/param.sfo').unlink()
            # A file that would land on one of the game's folders is refused, not copied inside it.
            (root / 'Clash').mkdir()
            (root / 'Clash/trophy').write_bytes(b'file')
            with self.assertRaises(OSError):
                launcher.adopt_system_files(root / 'Clash', game)
            self.assertFalse((game / 'sce_sys/trophy/trophy').exists())
            # For an update folder the candidate must be the same title in another version: the
            # base game's own Sc0 is not it.
            base = launcher.load_sfo(root / 'Sc0/param.sfo')
            self.assertEqual(base['APP_VER'], '01.00')
            beside = root / 'Base0'  # the base game beside this folder: Sc0 at its version is its own
            self.assertIsNone(launcher.system_files_beside(game, reference=base, base=beside))
            self.assertEqual(launcher.system_files_beside(game, reference={'TITLE_ID': 'CUSA01623', 'APP_VER': '01.02'},
                                                         base=beside), root / 'Sc0')
            self.assertIsNone(launcher.system_files_beside(game, reference={'TITLE_ID': 'CUSA00001', 'APP_VER': '01.02'},
                                                           base=beside))
            # In its own folder, an update at the base game's version is still this title's Sc0.
            self.assertEqual(launcher.system_files_beside(game, reference=base, base=root / 'elsewhere' / 'Image0'),
                             root / 'Sc0')
            # An unreadable param.sfo in the folder is a different problem: nothing is offered.
            (game / 'sce_sys/param.sfo').write_bytes(BASE_SFO)
            real_stat = Path.stat

            def denied_stat(path, *args, **kwargs):
                if path.name == 'param.sfo' and 'Image0' in path.parts:
                    raise PermissionError(13, 'Access is denied', str(path))
                return real_stat(path, *args, **kwargs)
            with mock.patch.object(Path, 'stat', denied_stat):
                self.assertIsNone(launcher.system_files_beside(game))
            (game / 'sce_sys/param.sfo').unlink()
            # Without the base game's param.sfo an Sc0 beside the base game is ambiguous.
            self.assertIsNone(launcher.system_files_beside(game, base=beside))
            # Only the extractors' own Sc0 counts: another folder with a param.sfo, or an Sc0 whose
            # param.sfo is not one, is another title's or cannot be read, offers nothing.
            (root / 'Sc0').rename(root / 'Other')
            self.assertIsNone(launcher.system_files_beside(game))
            (root / 'Other').rename(root / 'Sc0')
            (root / 'Sc0/param.sfo').write_bytes(b'not an sfo')
            self.assertIsNone(launcher.system_files_beside(game))
            (root / 'Sc0/param.sfo').write_bytes(sfo_bytes(TITLE_ID='CUSA00001', TITLE='Other Game', APP_VER='01.00'))
            self.assertIsNone(launcher.system_files_beside(game))
            (root / 'Sc0/param.sfo').write_bytes(BASE_SFO)
            real_open = Path.open

            def locked_open(path, *args, **kwargs):
                if 'Sc0' in path.parts:
                    raise PermissionError(13, 'Access is denied', str(path))
                return real_open(path, *args, **kwargs)
            with mock.patch.object(Path, 'open', locked_open):
                self.assertIsNone(launcher.system_files_beside(game))
            self.assertEqual(launcher.system_files_beside(game), root / 'Sc0')
            # A drive root has no siblings to look at.
            self.assertIsNone(launcher.system_files_beside(root.anchor))

    def test_play_offers_to_adopt_the_system_files(self):
        import tempfile
        from pathlib import Path
        from unittest import mock

        def extraction(root, name, version, placeholder=None):
            """A PKG extraction in <name>/: Image0/ with the game files, Sc0/ with param.sfo beside it."""
            game = root / name / 'Image0'
            (game / 'sce_module').mkdir(parents=True)
            (game / 'sce_sys').mkdir()
            (game / 'eboot.bin').write_bytes(b'x')
            for prx in ('libc.prx', 'libSceFios2.prx'):
                (game / 'sce_module' / prx).write_bytes(b'x')
            if placeholder is not None:
                (game / 'sce_sys/param.sfo').write_bytes(placeholder)
            (root / name / 'Sc0').mkdir()
            (root / name / 'Sc0/param.sfo').write_bytes(sfo_bytes(TITLE_ID='CUSA01623', APP_VER=version))
            return game

        def launcher_app(game, update=''):
            app = launcher.Launcher.__new__(launcher.Launcher)
            app.process = None
            app.app = {**launcher.APP_DEFAULTS, 'game_dir': str(game), 'update_dir': update}
            app.messagebox = mock.Mock()
            app.ui_calls = queue.Queue()
            for name in ('collect', 'show', 'set_log', 'game_changed', 'play_button', 'status'):
                setattr(app, name, mock.Mock())
            return app

        def play(app):
            """Whether the game was started. The copy runs in a worker thread that presses Play
            again through ui_calls: both run here, in order."""
            with mock.patch.object(launcher.subprocess, 'Popen', return_value=mock.Mock()) as start, \
                    mock.patch.object(launcher, 'GameJob'), mock.patch.object(launcher.threading, 'Thread') as thread, \
                    mock.patch.object(launcher, 'open_play_log', return_value=launcher._NullLog()):
                app.play()
                for call in thread.call_args_list:  # grows while the workers press Play again
                    target = call.kwargs.get('target')
                    if target is not None and target.__name__ == 'work':
                        target()
                        while not app.ui_calls.empty():
                            app.ui_calls.get()()
            return start.called

        cases = [('yes', True, None), ('yes over an empty placeholder', True, b''),
                 ('yes over a one-byte placeholder', True, b'x'), ('no', False, None)]
        for label, answer, placeholder in cases:
            with self.subTest(label), tempfile.TemporaryDirectory() as root:
                root = Path(root)
                game = extraction(root, 'game', '01.00', placeholder)
                app = launcher_app(game)
                app.messagebox.askyesno.return_value = answer
                started = play(app)
                self.assertIn(str(root / 'game' / 'Sc0'), app.messagebox.askyesno.call_args.args[1])
                if answer:
                    self.assertEqual((game / 'sce_sys/param.sfo').read_bytes(), BASE_SFO)
                    app.game_changed.assert_called_once()
                    app.messagebox.showerror.assert_not_called()
                    self.assertTrue(started)
                else:
                    self.assertFalse((game / 'sce_sys/param.sfo').exists())
                    self.assertIn('sce_sys/param.sfo', app.messagebox.showerror.call_args.args[1])
                    self.assertFalse(started)
        with self.subTest('update folder'), tempfile.TemporaryDirectory() as root:
            # A separately dumped update has the same layout and gets the same offer.
            root = Path(root)
            game = extraction(root, 'game', '01.00', BASE_SFO)
            update = extraction(root, 'update', '01.02')
            app = launcher_app(game, str(update))
            app.messagebox.askyesno.return_value = True
            self.assertTrue(play(app))
            self.assertIn(str(root / 'update' / 'Sc0'), app.messagebox.askyesno.call_args.args[1])
            self.assertEqual(launcher.game_sfo(update)['APP_VER'], '01.02')
        with self.subTest('update beside the base game'), tempfile.TemporaryDirectory() as root:
            # The base game's own Sc0 (same version) is never offered to the update folder, which
            # is then refused as incomplete.
            root = Path(root)
            game = extraction(root, 'game', '01.00', BASE_SFO)
            update = root / 'game' / 'Update0'
            (update / 'sce_sys').mkdir(parents=True)
            (update / 'eboot.bin').write_bytes(b'x')
            app = launcher_app(game, str(update))
            self.assertFalse(play(app))
            app.messagebox.askyesno.assert_not_called()
            self.assertIn('sce_sys/param.sfo', app.messagebox.showerror.call_args.args[1])
            self.assertIn(str(update), app.messagebox.showerror.call_args.args[1])
        with self.subTest('update at the base version in its own folder'), tempfile.TemporaryDirectory() as root:
            root = Path(root)
            game = extraction(root, 'game', '01.00', BASE_SFO)
            update = extraction(root, 'update', '01.00')
            app = launcher_app(game, str(update))
            app.messagebox.askyesno.return_value = True
            self.assertTrue(play(app))
            self.assertIn(str(root / 'update' / 'Sc0'), app.messagebox.askyesno.call_args.args[1])
        with self.subTest('update folder without eboot.bin'), tempfile.TemporaryDirectory() as root:
            # Not a game folder (a typo): no Sc0 search for it, and the error names what is missing.
            root = Path(root)
            game = extraction(root, 'game', '01.00', BASE_SFO)
            update = extraction(root, 'update', '01.02')
            (update / 'eboot.bin').unlink()
            app = launcher_app(game, str(update))
            self.assertFalse(play(app))
            app.messagebox.askyesno.assert_not_called()
            self.assertIn('eboot.bin', app.messagebox.showerror.call_args.args[1])
        with self.subTest('update declined'), tempfile.TemporaryDirectory() as root:
            root = Path(root)
            game = extraction(root, 'game', '01.00', BASE_SFO)
            update = extraction(root, 'update', '01.02')
            app = launcher_app(game, str(update))
            app.messagebox.askyesno.return_value = False
            self.assertFalse(play(app))
            self.assertIn('sce_sys/param.sfo', app.messagebox.showerror.call_args.args[1])
        with self.subTest('update before the game is complete'), tempfile.TemporaryDirectory() as root:
            # Each folder's own Sc0 is offered in turn; declining both leaves the game incomplete.
            root = Path(root)
            game = extraction(root, 'game', '01.00')
            update = extraction(root, 'update', '01.02')
            app = launcher_app(game, str(update))
            app.messagebox.askyesno.return_value = False
            self.assertFalse(play(app))
            self.assertEqual(app.messagebox.askyesno.call_count, 2)
            self.assertIn(str(root / 'update' / 'Sc0'), app.messagebox.askyesno.call_args.args[1])
            self.assertFalse((update / 'sce_sys/param.sfo').exists())
            self.assertIn('sce_sys/param.sfo', app.messagebox.showerror.call_args.args[1])
        with self.subTest('update folder is the game folder'), tempfile.TemporaryDirectory() as root:
            # Not offered twice, and refused before run.py would refuse it.
            root = Path(root)
            game = extraction(root, 'game', '01.00', BASE_SFO)
            app = launcher_app(game, str(game) + os.sep)
            self.assertFalse(play(app))
            app.messagebox.askyesno.assert_not_called()
            self.assertIn('two separate folders', app.messagebox.showerror.call_args.args[1])
        with self.subTest('copy fails'), tempfile.TemporaryDirectory() as root:
            # One error, naming the cause; not the incomplete-dump message on top of it.
            root = Path(root)
            game = extraction(root, 'game', '01.00')
            app = launcher_app(game)
            app.messagebox.askyesno.return_value = True
            with mock.patch.object(launcher, 'adopt_system_files', side_effect=PermissionError(13, 'Access is denied')):
                self.assertFalse(play(app))
            self.assertEqual(app.messagebox.showerror.call_count, 1)
            self.assertIn('Access is denied', app.messagebox.showerror.call_args.args[1])
            app.game_changed.assert_called_once()  # what was copied before the failure shows
            app.show.assert_called_with('game')


class GpuChoiceTest(unittest.TestCase):
    def test_parse_gpu_list(self):
        text = '0\tAMD Radeon(TM) Graphics\tintegrated\nbroken line\n1\tNVIDIA GeForce RTX 2070 SUPER\tdiscrete\n'
        self.assertEqual(launcher.parse_gpu_list(text),
                         [('0', 'AMD Radeon(TM) Graphics', 'integrated'),
                          ('1', 'NVIDIA GeForce RTX 2070 SUPER', 'discrete')])
        # Newer tools add the game's multisampling default on that driver and its default GPU.
        text = '0\tIntel(R) Arc(TM) A750 Graphics\tdiscrete\toff\t1\n1\tllvmpipe\tother\ton\t0\n'
        self.assertEqual(launcher.parse_gpu_list(text),
                         [('0', 'Intel(R) Arc(TM) A750 Graphics', 'discrete', 'off', '1'),
                          ('1', 'llvmpipe', 'other', 'on', '0')])
        self.assertEqual(launcher.parse_gpu_list(''), [])

    def test_list_gpus_failures_leave_only_automatic(self):
        import subprocess
        from unittest import mock
        with mock.patch.object(launcher, 'find_tool', return_value=None):
            self.assertEqual(launcher.list_gpus(), [])
        with mock.patch.object(launcher, 'find_tool', return_value='gow3-gpu-capabilities.exe'):
            with mock.patch.object(launcher.subprocess, 'run', side_effect=subprocess.TimeoutExpired('x', 5)):
                self.assertEqual(launcher.list_gpus(), [])
            with mock.patch.object(launcher.subprocess, 'run', side_effect=OSError):
                self.assertEqual(launcher.list_gpus(), [])
            done = mock.Mock(returncode=0, stdout='0\tRX 6700 XT\tdiscrete\n')
            with mock.patch.object(launcher.subprocess, 'run', return_value=done) as run:
                self.assertEqual(launcher.list_gpus(), [('0', 'RX 6700 XT', 'discrete')])
                self.assertEqual(run.call_args.kwargs['timeout'], 5)

    def test_gpu_choice_reaches_the_game(self):
        from unittest import mock
        with mock.patch.dict(launcher.os.environ, {'GOW3_GPU_ID': '7'}):
            env = launcher.game_environment({**launcher.APP_DEFAULTS, 'game_dir': 'x'})
            self.assertIsNone(env.get('GOW3_GPU_ID'))  # Automatic wins over an inherited value
            env = launcher.game_environment({**launcher.APP_DEFAULTS, 'game_dir': 'x', 'gpu_id': '1'})
            self.assertEqual(env['GOW3_GPU_ID'], '1')
            env = launcher.game_environment({**launcher.APP_DEFAULTS, 'game_dir': 'x', 'gpu_id': 'bad'})
            self.assertIsNone(env.get('GOW3_GPU_ID'))
        self.assertEqual(launcher.APP_DEFAULTS['gpu_id'], '')


class ReleaseVersionTest(unittest.TestCase):
    def test_god_of_war_release_is_newer_than_the_inherited_tags(self):
        self.assertEqual(launcher.VERSION, '2.3.0')
        self.assertGreater(launcher.version_tuple(launcher.VERSION), launcher.version_tuple('windows-v1.5'))
        self.assertGreater(launcher.version_tuple('windows-v2.3.1'), launcher.version_tuple(launcher.VERSION))

    def test_first_start_is_in_english(self):
        # The translations are partial; 'System' stays a choice in Advanced.
        self.assertEqual(launcher.APP_DEFAULTS['ui_language'], 'en')


if __name__ == '__main__':
    unittest.main()
