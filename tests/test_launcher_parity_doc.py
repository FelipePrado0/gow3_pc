"""The launcher parity map: every Python launcher test has one row, a status, and a Rust test that exists."""
import unittest

from paths import ROOT
from parity_doc import check

DOC = ROOT / 'docs/specs/2026-10-21-rust-launcher-parity.md'
TESTS = ROOT / 'tests'
THIS = 'test_launcher_parity_doc.py'

# Tk-only mechanics the Rust launcher has no equivalent for; every other drop is a gap.
ALLOWED_DROP_FILES = {
    'tests/test_launcher_font.py',
    'tests/test_launcher_scaling.py',
    'tests/test_launcher_stdio.py',
    'tests/test_launcher_diagnostics_layout.py',
}


def covered_files():
    """The test files the map covers, in file order."""
    files = sorted(p for p in TESTS.glob('test_launcher_*.py') if p.name != THIS)
    return files + [TESTS / 'test_trophies.py', TESTS / 'test_button_icons.py']


class ParityDocTest(unittest.TestCase):
    maxDiff = None  # the failures name the rows to fix

    def test_the_map_is_complete_and_honest(self):
        check(DOC, covered_files(), self, allowed_drops=ALLOWED_DROP_FILES)


if __name__ == '__main__':
    unittest.main()
