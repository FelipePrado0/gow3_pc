import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'launcher'))
from gow3_assets import fsr411_problem


class UpscalerAssetsTests(unittest.TestCase):
    def test_tier_depends_on_output_not_render_preset(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.assertIn('t2160_m0/spd.spv', fsr411_problem(root, '3840x2160', 3))
            self.assertIn('t2160_m0/spd.spv', fsr411_problem(root, '2560x1440', 0))
            self.assertIn('t1080_m1/spd.spv', fsr411_problem(root, '1280x720', 4))

    def test_partial_model_is_not_reported_available(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            folder = root / 't2160_m0'
            folder.mkdir()
            (folder / 'spd.spv').write_bytes(bytes(20))
            self.assertIn('prepass.spv', fsr411_problem(root, '3840x2160', 0))

    def test_complete_model_and_wrong_initializer(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            folder = root / 't1080_m0'
            folder.mkdir()
            names = ['spd', 'prepass', 'pass0_post', 'postpass', 'rcas']
            for i in range(1, 13):
                names.extend([f'pass{i}', f'pass{i}_post'])
            for name in names:
                (folder / (name+'.spv')).write_bytes(bytes(20))
            (folder / 'initializer.bin').write_bytes(bytes(8))
            self.assertIn('initializer.bin', fsr411_problem(root, '1920x1080', 1))
            (folder / 'initializer.bin').write_bytes(bytes(131072))
            self.assertIsNone(fsr411_problem(root, '1920x1080', 1))
