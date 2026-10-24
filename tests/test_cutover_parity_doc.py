"""The cutover parity map: every ported Python test has one row, a status, and a Rust test that exists."""
import unittest

from paths import ROOT
from parity_doc import check

DOC = ROOT / 'docs/specs/2026-10-23-rust-cutover-parity.md'
TESTS = ROOT / 'tests'


class CutoverParityDocTest(unittest.TestCase):
    maxDiff = None  # the failures name the rows to fix

    def test_the_map_is_complete_and_honest(self):
        check(DOC, [TESTS / 'test_steam_shortcut.py', TESTS / 'test_xvag_voice_swap.py'], self, allowed_drops=set())


if __name__ == '__main__':
    unittest.main()
