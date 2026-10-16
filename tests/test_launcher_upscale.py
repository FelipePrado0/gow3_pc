"""Temporal upscaling stays parked; spatial upscaling is removed from the launcher."""
import sys
import unittest

from paths import ROOT
sys.path.insert(0, str(ROOT / 'launcher'))
import gow3_launcher_win as launcher


class UpscalerSettingsTest(unittest.TestCase):
    def test_remaining_choices_have_no_spatial_upscaler(self):
        self.assertEqual([value for value, _text in launcher.UPSCALERS], ['fsr3', 'taa', 'off'])

    def test_removed_controls_do_not_supply_defaults_to_save(self):
        # A Mac keeps upscaling to the window (MetalFX or FSR 1): fsr1 is its control there.
        removed = ('sharpen', 'sharpness', 'rcas', 'rcas_strength') + (() if launcher.MACOS else ('fsr1',))
        for key in removed:
            self.assertNotIn(key, launcher.INI_DEFAULTS)


if __name__ == '__main__':
    unittest.main()
