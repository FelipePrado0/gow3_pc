"""Deleting the shader cache from the launcher: asks first, refuses while the game runs."""
import unittest
from unittest import mock

from test_launcher_save import LauncherOverTemporaryFiles, launcher


class ClearShaderCache(LauncherOverTemporaryFiles):
    def make_cache(self):
        cache = launcher.DATA_DIR / 'user' / 'cache' / 'CUSA01623' / 'pipeline-binaries' / 'shared'
        cache.mkdir(parents=True)
        (cache / 'a.bin').write_bytes(b'x' * 4096)
        return launcher.DATA_DIR / 'user' / 'cache'

    def test_asks_then_deletes(self):
        cache = self.make_cache()
        with mock.patch.object(self.app.messagebox, 'askyesno', return_value=False) as ask:
            self.app.clear_cache()
        self.assertTrue(cache.exists())
        self.assertIn(str(cache), ask.call_args.args[1])
        with mock.patch.object(self.app.messagebox, 'askyesno', return_value=True), \
                mock.patch.object(self.app.messagebox, 'showinfo') as info:
            self.app.clear_cache()
        self.assertFalse(cache.exists())
        info.assert_called_once()

    def test_refuses_while_the_game_runs(self):
        cache = self.make_cache()
        self.app.process = object()
        try:
            with mock.patch.object(self.app.messagebox, 'showwarning') as warning, \
                    mock.patch.object(self.app.messagebox, 'askyesno') as ask:
                self.app.clear_cache()
        finally:
            self.app.process = None
        warning.assert_called_once()
        ask.assert_not_called()
        self.assertTrue(cache.exists())

    def test_empty_cache_needs_no_question(self):
        with mock.patch.object(self.app.messagebox, 'showinfo') as info, \
                mock.patch.object(self.app.messagebox, 'askyesno') as ask:
            self.app.clear_cache()
        ask.assert_not_called()
        info.assert_called_once()


if __name__ == '__main__':
    unittest.main()
