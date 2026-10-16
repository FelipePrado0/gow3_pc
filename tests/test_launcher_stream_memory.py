"""Saved launcher selection, legacy settings and both launch paths for stream memory."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

from paths import ROOT
sys.path.insert(0, str(ROOT / 'launcher'))
import gow3_launcher_win as launcher  # noqa: E402


class StreamMemoryEnvironmentTest(unittest.TestCase):
    def test_legacy_settings_and_invalid_values_use_cached_memory(self):
        legacy = dict(launcher.APP_DEFAULTS)
        legacy.pop('stream_memory')
        with mock.patch.dict(os.environ, {'GOW3_STREAM_MEMORY': 'device'}):
            self.assertEqual(launcher.game_environment(legacy)['GOW3_STREAM_MEMORY'], 'cached')
            for value in ('', 'invalid', None, [], 1):
                with self.subTest(value=value):
                    env = launcher.game_environment({**legacy, 'stream_memory': value})
                    self.assertEqual(env['GOW3_STREAM_MEMORY'], 'cached')

    def test_every_selection_overrides_inherited_environment_without_changing_it(self):
        with mock.patch.dict(os.environ, {'GOW3_STREAM_MEMORY': 'host'}):
            for value in ('cached', 'host', 'device'):
                with self.subTest(value=value):
                    env = launcher.game_environment({**launcher.APP_DEFAULTS, 'stream_memory': value})
                    self.assertEqual(env['GOW3_STREAM_MEMORY'], value)
                    self.assertEqual(os.environ['GOW3_STREAM_MEMORY'], 'host')

    def test_play_without_window_uses_saved_selection(self):
        with tempfile.TemporaryDirectory() as folder:
            settings_path = Path(folder) / 'settings.json'
            for value in ('cached', 'host', 'device'):
                with self.subTest(value=value):
                    settings_path.write_text(json.dumps({'stream_memory': value}), encoding='utf-8')
                    settings = {**launcher.APP_DEFAULTS, **launcher.load_json(settings_path, {}),
                                'user_dir': folder}
                    process = mock.Mock()
                    process.stdout.readline.return_value = b''
                    process.wait.return_value = 0
                    with mock.patch.object(launcher, 'attach_stdio'), \
                            mock.patch.object(launcher.subprocess, 'Popen', return_value=process) as start:
                        self.assertEqual(launcher.play_without_window(settings), 0)
                    self.assertEqual(start.call_args.kwargs['env']['GOW3_STREAM_MEMORY'], value)


class StreamMemoryLauncherTest(unittest.TestCase):
    def test_ui_selection_persists_reopens_and_reverts_on_next_launch(self):
        # Exercise the real Tk combobox event and collect/play methods using temporary settings.
        # The process is mocked so this never opens or changes a game installation.
        try:
            import tkinter as tk
            from tkinter import ttk
            root = tk.Tk()
        except ImportError as error:
            self.skipTest(f'Tk unavailable: {error}')
        except tk.TclError as error:
            self.skipTest(f'Tk display unavailable: {error}')
        root.withdraw()
        self.addCleanup(root.destroy)
        # The page as Windows and Linux build it (a Mac has no stream memory to choose).
        with tempfile.TemporaryDirectory() as folder, \
                mock.patch.object(launcher, 'MACOS', False), \
                mock.patch.object(launcher, 'CONFIG_DIR', Path(folder)), \
                mock.patch.object(launcher, 'CONFIG_FILE', Path(folder) / 'settings.json'), \
                mock.patch.dict(os.environ, {'GOW3_CONFIG': str(Path(folder) / 'gow3.ini')}):
            app = launcher.Launcher.__new__(launcher.Launcher)
            app.tk, app.ttk, app.root, app.dpi = tk, ttk, root, 1.0
            app.content, app.pages, app.vars = ttk.Frame(root), {}, {}
            app.app = dict(launcher.APP_DEFAULTS)
            app.ini, app.ini_lines = launcher.load_ini()
            app.control_vars, app.mod_order, app.mod_vars, app.patch_vars = {}, [], {}, {}
            app.game_patch_title, app.game_patch_vars = None, {}
            app.save_button = mock.Mock()  # collect() refreshes it
            app.pad_style = tk.StringVar(root, value='playstation')
            app.store_game_patches = mock.Mock()
            app.build_performance()

            def widgets(parent):
                for child in parent.winfo_children():
                    yield child
                    yield from widgets(child)

            labels = tuple(launcher._(*text) for _value, text in launcher.STREAM_MEMORY)
            boxes = [widget for widget in widgets(app.pages['performance'])
                     if isinstance(widget, ttk.Combobox) and tuple(widget['values']) == labels]
            self.assertEqual(len(boxes), 1)
            box = boxes[0]
            self.assertEqual(app.vars['stream_memory'].get(), 'cached')

            for index, (value, _text) in enumerate(launcher.STREAM_MEMORY):
                with self.subTest(value=value):
                    box.current(index)
                    box.event_generate('<<ComboboxSelected>>')
                    self.assertEqual(app.vars['stream_memory'].get(), value)
                    app.collect()
                    saved = {**launcher.APP_DEFAULTS, **launcher.load_json(launcher.CONFIG_FILE, {})}
                    self.assertEqual(saved['stream_memory'], value)
                    reopened = launcher.Launcher.__new__(launcher.Launcher)
                    reopened.tk, reopened.ttk, reopened.app, reopened.vars = tk, ttk, saved, {}
                    holder = ttk.Frame(root)
                    reopened_box = reopened.choice(holder, 'stream_memory', 'app', launcher.STREAM_MEMORY)
                    self.assertEqual(reopened_box.current(), index)
                    holder.destroy()

                    app.process = None
                    app.play_button, app.status = mock.Mock(), mock.Mock()
                    process = mock.Mock()
                    with mock.patch.object(launcher, 'game_info', return_value=('God of War III', '01.02')), \
                            mock.patch.object(launcher, 'missing_game_files', return_value=[]), \
                            mock.patch.object(launcher.subprocess, 'Popen', return_value=process) as start, \
                            mock.patch.object(launcher, 'GameJob'), \
                            mock.patch.object(launcher.threading, 'Thread'), \
                            mock.patch.object(launcher, 'open_play_log', return_value=launcher._NullLog()), \
                            mock.patch.object(app, 'set_log', create=True):
                        app.play()
                    self.assertEqual(start.call_args.kwargs['env']['GOW3_STREAM_MEMORY'], value)
            # Selecting cached again after device restores the optimization on the next launch.
            box.current(0)
            box.event_generate('<<ComboboxSelected>>')
            app.collect()
            self.assertEqual(launcher.load_json(launcher.CONFIG_FILE, {})['stream_memory'], 'cached')
            app.content.destroy()


if __name__ == '__main__':
    unittest.main()
