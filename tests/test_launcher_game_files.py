import json
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest import mock

import paths  # noqa: F401
from test_launcher_controls import launcher, sfo_bytes


class LauncherGameFilesTests(unittest.TestCase):
    def setUp(self):
        try:
            import tkinter as tk
            from tkinter import filedialog, messagebox, ttk
        except ImportError as error:
            self.skipTest(f'Tk unavailable: {error}')

        try:
            self.root = tk.Tk()
        except tk.TclError as error:
            self.skipTest(f'Tk display unavailable: {error}')
        self.root.withdraw()
        self.addCleanup(self.destroy_root)
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.folder = Path(temporary.name)
        self.game = self.folder / 'Image0'
        (self.game / 'sce_module').mkdir(parents=True)
        (self.game / 'eboot.bin').write_bytes(b'synthetic executable')
        (self.game / 'sce_module/libSceFios2.prx').write_bytes(b'synthetic module')
        self.source = self.folder / 'Sc0'
        self.source.mkdir()
        (self.source / 'param.sfo').write_bytes(sfo_bytes(TITLE_ID='CUSA01623', APP_VER='01.02'))
        (self.source / 'trophy').mkdir()
        (self.source / 'trophy/test.trp').write_bytes(b'synthetic trophy')
        config = self.folder / 'config'
        config.mkdir()
        config_file = config / 'settings.json'
        config_file.write_text(json.dumps({**launcher.APP_DEFAULTS, 'game_dir': str(self.game),
                                           'ui_language': 'en', 'check_updates': False}), encoding='utf-8')
        for name, value in [('CONFIG_DIR', config), ('CONFIG_FILE', config_file), ('DATA_DIR', self.folder)]:
            patch = mock.patch.object(launcher, name, value)
            patch.start()
            self.addCleanup(patch.stop)
        gpu = mock.patch.object(launcher, 'list_gpus', return_value=[])
        gpu.start()
        self.addCleanup(gpu.stop)
        self.app = launcher.Launcher(self.root, tk, ttk, filedialog, messagebox)
        self.app.messagebox = mock.Mock()
        self.root.update()

    def destroy_root(self):
        for callback in self.root.tk.call('after', 'info'):
            self.root.after_cancel(callback)
        self.root.destroy()

    def wait_for_copy(self):
        deadline = time.monotonic() + 5
        while getattr(self.app, 'copying_system_files', False) and time.monotonic() < deadline:
            self.root.update()
            time.sleep(0.01)
        self.assertFalse(getattr(self.app, 'copying_system_files', False), 'copy did not finish')

    def test_cancel_then_copy_keeps_existing_files_and_reports_missing_module(self):
        target = self.game / 'sce_sys'
        target.mkdir()
        (target / 'pic1.png').write_bytes(b'existing artwork')
        (self.source / 'pic1.png').write_bytes(b'replacement artwork')
        self.app.messagebox.askyesno.return_value = False
        with mock.patch.object(launcher.subprocess, 'Popen') as start:
            self.app.play_button.invoke()
            self.assertFalse((target / 'param.sfo').exists())
            self.assertIn('sce_sys/param.sfo', self.app.messagebox.showerror.call_args.args[1])
            self.app.messagebox.askyesno.return_value = True
            self.app.play_button.invoke()
            self.wait_for_copy()
            start.assert_not_called()
        self.assertEqual((target / 'param.sfo').read_bytes(), (self.source / 'param.sfo').read_bytes())
        self.assertEqual((target / 'trophy/test.trp').read_bytes(), b'synthetic trophy')
        self.assertEqual((target / 'pic1.png').read_bytes(), b'existing artwork')
        self.assertEqual((self.source / 'pic1.png').read_bytes(), b'replacement artwork')
        self.assertIn('sce_module/libc.prx', self.app.messagebox.showerror.call_args.args[1])
        self.assertEqual(self.app.current_page, 'game')
        self.assertFalse(self.app.play_button.instate(['disabled']))

    def test_slow_copy_blocks_reentry_and_failure_allows_retry(self):
        entered, release = threading.Event(), threading.Event()

        def denied_copy(*_args):
            entered.set()
            if not release.wait(5):
                raise TimeoutError('test copy timed out')
            raise PermissionError('test access denied')

        self.app.messagebox.askyesno.return_value = True
        with mock.patch.object(launcher, 'adopt_system_files', side_effect=denied_copy) as copy, \
                mock.patch.object(launcher.subprocess, 'Popen') as start:
            self.app.play_button.invoke()
            self.assertTrue(entered.wait(2))
            try:
                self.root.update()
                self.app.show('game')
                self.app.game_changed()
                self.app.play_button.invoke()
                self.app.play()
                self.assertTrue(self.app.play_button.instate(['disabled']))
                self.assertIn('Copying', self.app.status.cget('text'))
                copy.assert_called_once()
            finally:
                release.set()
                self.wait_for_copy()
            start.assert_not_called()
        self.assertIn('test access denied', self.app.messagebox.showerror.call_args.args[1])
        self.assertFalse(self.app.play_button.instate(['disabled']))
        with mock.patch.object(launcher.subprocess, 'Popen') as start:
            self.app.play_button.invoke()
            self.wait_for_copy()
            start.assert_not_called()
        self.assertTrue((self.game / 'sce_sys/param.sfo').is_file())


if __name__ == '__main__':
    unittest.main()
