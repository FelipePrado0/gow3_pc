"""The launcher offers the upscalers the game can run on this platform, by the names it reads; a
Mac keeps upscaling to the window (FSR 1 or Apple's MetalFX) on its Graphics page."""
import os
from pathlib import Path
import re
import shutil
import sys
import tempfile
import unittest

from paths import ROOT
sys.path.insert(0, str(ROOT / 'launcher'))
import gow3_launcher_win as launcher  # noqa: E402

try:
    import tkinter as tk
    from tkinter import filedialog, messagebox, ttk
except ImportError:  # pragma: no cover
    tk = None


def game_upscaler_names():
    """The gow3.ini upscaler names of gpu/shim/gow3_settings.cpp (UpscalerName)."""
    source = (ROOT / 'gpu' / 'shim' / 'gow3_settings.cpp').read_text(encoding='utf-8')
    names = re.search(r'names\[UpscalerCount\] = \{([^}]*)\}', source)
    return re.findall(r'"(\w+)"', names.group(1))


def game_screen_upscaler_names():
    """The gow3.ini screen_upscaler values of gpu/shim/gow3_graphics.h (ScreenUpscalerKeys)."""
    source = (ROOT / 'gpu' / 'shim' / 'gow3_graphics.h').read_text(encoding='utf-8')
    keys = re.search(r'ScreenUpscalerKeys = \{([^}]*)\}', source)
    return re.findall(r'"(\w+)"', keys.group(1))


class LauncherUpscalersTest(unittest.TestCase):
    def test_every_choice_is_a_name_the_game_reads(self):
        game = game_upscaler_names()
        self.assertIn('metalfx', game)
        for name, _labels in launcher.UPSCALERS:
            with self.subTest(name=name):
                self.assertIn(name, game)

    def test_screen_upscalers_are_the_names_the_game_reads(self):
        # FSR 1 first: the game's default, and what it uses for anything it does not know.
        self.assertEqual([name for name, _labels in launcher.SCREEN_UPSCALERS], game_screen_upscaler_names())
        self.assertEqual(launcher.SCREEN_UPSCALERS[0][0], 'fsr1')

    def test_upscaling_to_the_window_is_kept_in_gow3_ini_on_a_mac_only(self):
        for key in ('fsr1', 'screen_upscaler'):
            self.assertEqual(key in launcher.INI_DEFAULTS, launcher.MACOS)
        if launcher.MACOS:
            self.assertEqual((launcher.INI_DEFAULTS['fsr1'], launcher.INI_DEFAULTS['screen_upscaler']), ('0', 'fsr1'))

    def test_upscaling_note(self):
        # Under a Mac's upscaler when it has nothing to enlarge, with the lower render resolutions:
        # each of them is enlarged, none is required.
        self.assertIn('nothing to enlarge', launcher.upscaling_note(True, 'native', 1080))
        self.assertIn('nothing to enlarge', launcher.upscaling_note(True, 'native', 900))  # a laptop
        for upscale, resolution in ((False, 'native'), (True, '720p'), (True, '480p'), (False, '480p')):
            self.assertEqual(launcher.upscaling_note(upscale, resolution, 1080), '')
        # A taller screen: FSR 1 or MetalFX enlarges native 1080p to it.
        self.assertEqual(launcher.upscaling_note(True, 'native', 2160), '')


@unittest.skipUnless(sys.platform == 'darwin' and tk, 'the Mac launcher, with Tk')
class MacLauncherTest(unittest.TestCase):
    """A real launcher window over temporary settings: the user's files are never touched."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.saved = (launcher.CONFIG_DIR, launcher.CONFIG_FILE, launcher.DATA_DIR, launcher.LANG,
                      os.environ.get('GOW3_CONFIG'))
        launcher.CONFIG_DIR = self.tmp / 'config'
        launcher.CONFIG_FILE = launcher.CONFIG_DIR / 'settings.json'
        launcher.DATA_DIR = self.tmp / 'data'
        launcher.DATA_DIR.mkdir()
        launcher.LANG = 'en'
        os.environ['GOW3_CONFIG'] = str(self.tmp / 'gow3.ini')
        self.root = tk.Tk()
        self.root.withdraw()
        self.app = launcher.Launcher(self.root, tk, ttk, filedialog, messagebox)
        self.root.winfo_screenheight = lambda: 1080  # the upscaling note concerns screens up to 1080p

    def tearDown(self):
        for pending in self.root.tk.call('after', 'info'):
            self.root.after_cancel(pending)
        self.root.destroy()
        launcher.CONFIG_DIR, launcher.CONFIG_FILE, launcher.DATA_DIR, launcher.LANG, config = self.saved
        if config is None:
            os.environ.pop('GOW3_CONFIG', None)
        else:
            os.environ['GOW3_CONFIG'] = config

    def widgets(self, page):
        def walk(parent):
            for child in parent.winfo_children():
                yield child
                yield from walk(child)
        return list(walk(self.app.pages[page]))

    def texts(self, page):
        found = []
        for widget in self.widgets(page):
            try:
                found.append(str(widget.cget('text')))
            except tk.TclError:
                pass
        return found

    def test_graphics_page_offers_metalfx_and_saves_it(self):
        labels = tuple(launcher._(*text) for _value, text in launcher.SCREEN_UPSCALERS)
        boxes = [w for w in self.widgets('graphics') if isinstance(w, ttk.Combobox) and tuple(w['values']) == labels]
        self.assertEqual(len(boxes), 1)
        texts = self.texts('graphics')
        for text in ('Upscale to the window', 'Fewer GPU waits (experimental)'):
            self.assertIn(text, texts)
        box, upscale = boxes[0], self.app.vars['fsr1']
        upscale.set(False)
        self.assertEqual(str(box.cget('state')), 'disabled')  # it counts only with upscaling on
        upscale.set(True)
        self.assertEqual(str(box.cget('state')), 'readonly')
        box.current(1)
        box.event_generate('<<ComboboxSelected>>')
        self.assertTrue(self.app.collect())
        saved = launcher.load_ini()[0]
        self.assertEqual((saved['fsr1'], saved['screen_upscaler']), ('1', 'metalfx'))

    def test_the_note_offers_the_lower_resolutions(self):
        buttons = {w.cget('text'): w for w in self.widgets('graphics')
                   if isinstance(w, ttk.Button) and w.cget('text') in ('720p', '480p')}
        note = buttons['720p'].master
        self.app.vars['render_resolution'].set('native')
        self.app.vars['fsr1'].set(True)
        self.assertTrue(note.winfo_manager())
        buttons['480p'].invoke()
        self.assertEqual(self.app.vars['render_resolution'].get(), '480p')
        self.assertFalse(note.winfo_manager())
        self.app.vars['render_resolution'].set('native')
        buttons['720p'].invoke()
        self.assertEqual(self.app.vars['render_resolution'].get(), '720p')

    def test_fewer_gpu_waits_follows_its_readback_mode(self):
        box = next(w for w in self.widgets('graphics') if isinstance(w, ttk.Checkbutton)
                   and str(w.cget('variable')) == str(self.app.vars['fewer_gpu_waits']))
        self.app.vars['stale_readback'].set(False)
        self.assertEqual(str(box.cget('state')), 'disabled')
        self.app.vars['stale_readback'].set(True)
        self.assertEqual(str(box.cget('state')), 'normal')
        self.app.vars['fewer_gpu_waits'].set(True)
        self.assertTrue(self.app.collect())
        self.assertEqual(launcher.load_ini()[0]['fewer_gpu_waits'], '1')

    def test_performance_page_has_no_card_or_stream_memory_choice(self):
        texts = self.texts('performance')
        for text in ('Graphics card', 'Stream buffer memory', 'Fewer GPU waits (experimental)'):
            self.assertNotIn(text, texts)  # one GPU; Fewer GPU waits is beside the upscaling
        self.assertIn('Game multisampling', texts)

    def test_advanced_page_has_updates_steam_and_the_metal_hud(self):
        texts = self.texts('advanced')
        for text in ('Check for updates', 'Add to Steam', 'Metal performance HUD'):
            self.assertIn(text, texts)
        self.assertTrue(any(t.lower() == 'macos' for t in texts))
        self.assertNotIn('Window system', texts)  # the Linux options stay on Linux


if __name__ == '__main__':
    unittest.main()
