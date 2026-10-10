import json
import tempfile
import unittest
from pathlib import Path

import paths  # noqa: F401  (puts scripts/ on sys.path)
import graphics_options as g


class GraphicsOptionsTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.ini = Path(self.temp.name) / 'gow3.ini'
        self.state = Path(self.temp.name) / 'state.json'

    def tearDown(self):
        self.temp.cleanup()

    def test_defaults_and_invalid_values(self):
        self.ini.write_text('render_resolution=8K\nengine_fps=90\nstale_readback=yes\n', encoding='utf-8')
        options, reverted = g.prepare(self.ini, self.state)
        self.assertFalse(reverted)
        self.assertEqual(options, {'render_resolution': 'native', 'engine_fps': '120', 'deferred_readback': '1',
                                   'stale_readback': '1', 'async_shaders': '1'})

    def test_write_keys_keeps_other_lines(self):
        self.ini.write_text('# comment\nshow_fps=1\nengine_fps=60\n', encoding='utf-8')
        g.write_keys(self.ini, {'engine_fps': '240', 'vsync': '0'})
        self.assertEqual(self.ini.read_text(encoding='utf-8'), '# comment\nshow_fps=1\nengine_fps=240\nvsync=0\n')

    def test_unconfirmed_restart_is_tried_once_then_reverted(self):
        self.ini.write_text('render_resolution=1440p\nengine_fps=120\n', encoding='utf-8')
        good, _ = g.prepare(self.ini, self.state)  # a normal, confirmed launch
        g.write_keys(self.ini, {'render_resolution': '4K', 'engine_fps': '240', 'restart_unconfirmed': '1'})
        tried, reverted = g.prepare(self.ini, self.state)  # the launch of "Apply and restart"
        self.assertFalse(reverted)
        self.assertEqual((tried['render_resolution'], tried['engine_fps']), ('4K', '240'))
        back, reverted = g.prepare(self.ini, self.state)  # it never confirmed: back to the good ones
        self.assertTrue(reverted)
        self.assertEqual(back, good)
        ini = g.read_ini(self.ini)
        self.assertEqual((ini['render_resolution'], ini['restart_unconfirmed']), ('1440p', '0'))

    def test_confirmed_restart_becomes_the_last_good(self):
        g.prepare(self.ini, self.state)
        g.write_keys(self.ini, {'engine_fps': '240', 'restart_unconfirmed': '1'})
        g.prepare(self.ini, self.state)
        g.write_keys(self.ini, {'restart_unconfirmed': '0'})  # the game confirmed
        g.prepare(self.ini, self.state)
        self.assertEqual(json.loads(self.state.read_text())['last_good']['engine_fps'], '240')

    def test_patch_names(self):
        launcher = ['Bug Fix - Texture Corruption Fix', 'Frame Rate Patch - 120 FPS', 'Skip Any Video With X Button']
        options = g.startup_options({'render_resolution': '1440p', 'engine_fps': '240'})
        self.assertEqual(g.patch_names(launcher, options),
                         ['Skip Any Video With X Button', 'Resolution Patch - 1440p', 'Frame Rate Patch - 240 FPS'])
        native = g.startup_options({'engine_fps': '60'})
        self.assertEqual(g.patch_names(['Resolution Patch - 4K', 'Skip Any Video With X Button'], native),
                         ['Bug Fix - Texture Corruption Fix', 'Skip Any Video With X Button'])

    def test_environment(self):
        env = g.environment(g.startup_options({'stale_readback': '0'}))
        self.assertEqual(env, {'GOW3_DEFERRED_READBACK': '1', 'GOW3_STALE_READBACK': '0', 'GOW3_ASYNC_SHADERS': '1'})


if __name__ == '__main__':
    unittest.main()
