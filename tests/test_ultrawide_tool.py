# SPDX-License-Identifier: GPL-2.0-or-later
"""tools/make_ultrawide_patch.py: the aspect ratio floats it writes (needs capstone)."""
import struct
import sys
import unittest
from pathlib import Path

try:
    import capstone  # noqa: F401
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'tools'))
    import make_ultrawide_patch as tool
except ImportError:  # the tool needs capstone
    tool = None


@unittest.skipIf(tool is None, 'capstone is not installed')
class AspectFloatsTests(unittest.TestCase):
    def test_both_floats_are_the_aspect_ratio(self):
        for width, height in ((3440, 1440), (1920, 1200), (1600, 900), (5120, 1440)):
            data = tool.aspect_floats(width / height)
            self.assertEqual(data[:4], data[4:], (width, height))
            self.assertEqual(data[:4], struct.pack('<f', width / height))

    def test_4_3_does_not_hit_the_games_letterbox_test(self):
        # The game letterboxes to 16:9 when the second float is exactly 4/3 (function at 0x56fc60), so for 4:3 it is
        # one unit in the last place above: first and second differ by 1 in their integer form.
        for width, height in ((1280, 960), (1440, 1080), (1920, 1440), (2048, 1536), (2560, 1920)):
            data = tool.aspect_floats(width / height)
            first, second = struct.unpack('<II', data)
            self.assertEqual(first, struct.unpack('<I', struct.pack('<f', 4 / 3))[0])
            self.assertEqual(second, first + 1, (width, height))
            self.assertNotEqual(data[4:], struct.pack('<f', 4 / 3))


if __name__ == '__main__':
    unittest.main()
