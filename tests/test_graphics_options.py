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
        self.ini.write_text('render_resolution=8K\nengine_fps=90\nstale_readback=yes\nfewer_gpu_waits=yes\n',
                            encoding='utf-8')
        options, reverted = g.prepare(self.ini, self.state)
        self.assertFalse(reverted)
        self.assertEqual(options, {'render_resolution': 'native', 'engine_fps': '120', 'deferred_readback': '1',
                                   'stale_readback': '1', 'async_shaders': '1', 'fewer_gpu_waits': '0'})

    def test_corrupt_state_file_preserves_current_settings(self):
        self.ini.write_text('render_resolution=1440p\nengine_fps=240\n', encoding='utf-8')
        self.state.write_text('{invalid json', encoding='utf-8')

        options, reverted = g.prepare(self.ini, self.state)

        self.assertFalse(reverted)
        self.assertEqual(options['render_resolution'], '1440p')
        self.assertEqual(options['engine_fps'], '240')

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

    def test_resolution_labels_show_the_size_and_aspect_ratio(self):
        self.assertEqual(g.resolution_label('native'), 'Native · 1920 × 1080 · 16:9')
        self.assertEqual(g.resolution_label('480p'), '480p · 854 × 480 · 16:9')
        self.assertEqual(g.resolution_label('3440x1440'), '3440 × 1440 · 21:9 (beta)')
        self.assertEqual(g.resolution_label('5120x1440'), '5120 × 1440 · 32:9 (beta)')
        self.assertEqual(g.resolution_label('2560x1600'), '2560 × 1600 · 16:10 (beta)')
        self.assertEqual(g.resolution_label('3840x1080'), '3840 × 1080 · 32:9')  # a mod's size, not beta
        self.assertEqual(g.resolution_label('1600x1200'), '1600 × 1200 · 4:3')
        self.assertEqual(g.resolution_label('custom'), 'custom')
        # Every ultrawide and 16:10 size is beta; the 16:9 ones are not.
        self.assertEqual([r for r in g.RESOLUTIONS if g.resolution_label(r).endswith('(beta)')],
                         list(g.BETA_RESOLUTIONS))

    def test_every_built_in_resolution_has_a_patch(self):
        import patches
        xml = Path(__file__).resolve().parent.parent / 'patches' / 'God_of_War_III_Remastered.xml'
        self.assertEqual(sorted(r for r in g.RESOLUTIONS if r != 'native'), sorted(patches.resolution_names([xml])))

    def test_resolutions_of_enabled_mods(self):
        # A mod's "Resolution Patch - <size>" is a render resolution only while its mod is on.
        self.assertEqual(g.resolutions(), g.RESOLUTIONS)
        self.assertEqual(g.resolutions(['3840x1080', '4K']), g.RESOLUTIONS + ('3840x1080',))
        ini = {'render_resolution': '3840x1080', 'engine_fps': '60'}
        self.assertEqual(g.startup_options(ini)['render_resolution'], 'native')
        options = g.startup_options(ini, ['3840x1080'])
        self.assertEqual(options['render_resolution'], '3840x1080')
        self.assertEqual(g.patch_names(['Bug Fix - Texture Corruption Fix'], options), ['Resolution Patch - 3840x1080'])

    def test_a_confirmed_mod_resolution_is_restored_while_its_mod_is_on(self):
        with tempfile.TemporaryDirectory() as folder:
            ini, state = Path(folder) / 'gow3.ini', Path(folder) / 'state.json'
            g.write_keys(ini, {'render_resolution': '3840x1080', 'engine_fps': '60'})
            tried, _ = g.prepare(ini, state, ['3840x1080'])
            self.assertEqual(tried['render_resolution'], '3840x1080')
            g.write_keys(ini, {'render_resolution': '4K', 'restart_unconfirmed': '1'})
            self.assertEqual(g.prepare(ini, state, ['3840x1080'])[0]['render_resolution'], '4K')  # attempt
            options, reverted = g.prepare(ini, state, ['3840x1080'])  # the restart never confirmed
            self.assertTrue(reverted)
            self.assertEqual(options['render_resolution'], '3840x1080')

    def test_applied_engine_fps(self):
        # The engine runs above 60 only with a frame rate patch in the applied list.
        self.assertEqual(g.applied_engine_fps(['Skip Any Video With X Button', 'Frame Rate Patch - 240 FPS']), 240)
        self.assertEqual(g.applied_engine_fps(['Frame Rate Patch - 120 FPS']), 120)
        self.assertEqual(g.applied_engine_fps(['Bug Fix - Texture Corruption Fix']), 60)
        self.assertEqual(g.applied_engine_fps([]), 60)

    def test_environment(self):
        env = g.environment(g.startup_options({'stale_readback': '0'}))
        self.assertEqual(env, {'GOW3_DEFERRED_READBACK': '1', 'GOW3_STALE_READBACK': '0', 'GOW3_ASYNC_SHADERS': '1',
                               'GOW3_FEWER_GPU_WAITS': '0'})
        env = g.environment(g.startup_options({'fewer_gpu_waits': '1'}))
        self.assertEqual(env['GOW3_FEWER_GPU_WAITS'], '1')


if __name__ == '__main__':
    unittest.main()
