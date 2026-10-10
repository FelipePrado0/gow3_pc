import sys
import unittest

from paths import ROOT
sys.path.insert(0, str(ROOT / 'launcher'))
import gow3_launcher_win as launcher  # noqa: E402


class ControlMapTest(unittest.TestCase):
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
        self.assertEqual(env['GOW3_ASYNC_SHADERS'], '1')
        self.assertEqual((env['GOW3_DEFERRED_READBACK'], env['GOW3_PERF_DIAG']), ('1', '0'))
        self.assertEqual(env['GOW3_STALE_READBACK'], '1')
        self.assertEqual(launcher.game_environment({**settings, 'stale_readback': False})['GOW3_STALE_READBACK'], '0')
        old = launcher.game_environment({**settings, 'extra_env': 'GOW3_ASYNC_SHADERS=0'})
        self.assertEqual(old['GOW3_ASYNC_SHADERS'], '1')  # the old free-text field is ignored
        self.assertEqual(launcher.game_environment({**settings, 'async_shaders': False})['GOW3_ASYNC_SHADERS'], '0')
        self.assertEqual(launcher.game_environment({**settings, 'parallel_warmup': False})['GOW3_PARALLEL_WARMUP'], '0')


if __name__ == '__main__':
    unittest.main()
