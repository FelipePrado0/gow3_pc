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
        self.assertEqual(env['GOW3_BACKGROUND_WARMUP'], '1')
        self.assertEqual(env['GOW3_PERF_DIAG'], '0')
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

    def test_settings_keys_kept(self):
        # The redesign hides some options (player name) but keeps every saved setting working.
        for key in ('game_dir', 'user_dir', 'mods_dir', 'patches_dir', 'language', 'player_name', 'hdr',
                    'draw_pipe', 'readbacks', 'background_warmup', 'parallel_warmup', 'controls', 'pad_style'):
            self.assertIn(key, launcher.APP_DEFAULTS)
        env = launcher.game_environment({**launcher.APP_DEFAULTS, 'player_name': ' Kratos '})
        self.assertEqual(env['GOW3_USER_NAME'], 'Kratos')
        for key in ('display_mode', 'fps_limit', 'render_resolution', 'engine_fps', 'rcas_strength'):
            self.assertIn(key, launcher.INI_DEFAULTS)

    def test_graphics_defaults_match_the_game(self):
        d = launcher.INI_DEFAULTS
        self.assertEqual((d['display_mode'], d['vsync'], d['fps_limit'], d['engine_fps']), ('windowed', '1', '0', '120'))
        self.assertEqual([v for v, _t in launcher.FPS_LIMITS], ['30', '60', '120', '240', '0'])
        self.assertEqual((d['fsr1'], d['rcas'], d['rcas_strength'], d['frames_queued']), ('0', '1', '75', '1'))
        self.assertEqual([v for v, _t in launcher.FRAMES_QUEUED], ['1', '2', '0'])
        self.assertNotIn('GOW3_FRAMES_AHEAD', launcher.game_environment({**launcher.APP_DEFAULTS, 'frames_ahead': '2'}))


if __name__ == '__main__':
    unittest.main()
