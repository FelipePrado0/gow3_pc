"""Brazilian Portuguese covers every text the launcher shows (issue #451)."""
import ast
import re
import sys
import unittest
import xml.etree.ElementTree as ET

from paths import ROOT
sys.path.insert(0, str(ROOT / 'launcher'))
import gow3_lang  # noqa: E402
import gow3_launcher_win as launcher  # noqa: E402

# Button names that read the same in Portuguese.
SAME = {'Options', 'Share / Create', 'Menu', 'View', 'Share'}


class PortugueseTest(unittest.TestCase):
    def setUp(self):
        self.pt = gow3_lang.table('pt')

    def test_every_literal_text_is_translated(self):
        tree = ast.parse((ROOT / 'launcher' / 'gow3_launcher_win.py').read_text(encoding='utf-8'))
        texts = {n.args[0].value for n in ast.walk(tree)
                 if isinstance(n, ast.Call) and getattr(n.func, 'id', None) == '_' and n.args
                 and isinstance(n.args[0], ast.Constant) and isinstance(n.args[0].value, str)}
        self.assertEqual(sorted(t for t in texts if t not in self.pt), [])

    def test_button_names_are_translated(self):
        labels = set(launcher.PS_BUTTON_NAMES.values())
        for style in launcher.PAD_STYLES.values():
            labels |= set(style.values())
        missing = [l for l in labels if l not in self.pt and l not in SAME and not re.fullmatch(r'[A-Z0-9]{1,2}', l)]
        self.assertEqual(sorted(missing), [])

    def test_the_built_in_patch_notes_are_translated(self):
        root = ET.parse(ROOT / 'patches' / 'God_of_War_III_Remastered.xml').getroot()
        notes = {m.get('Note') for m in root.iter('Metadata')
                 if not m.get('Name').startswith(('Resolution Patch', 'Frame Rate Patch'))}
        self.assertEqual(sorted(n for n in notes if n not in self.pt), [])

    def test_resolution_labels(self):
        self.assertEqual(self.pt[launcher.RESOLUTION_CHOICES[0][1][0]], 'Nativa · 1920 × 1080 · 16:9')


if __name__ == '__main__':
    unittest.main()
