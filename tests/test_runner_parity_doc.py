"""The runner parity map: every Python runner test has one row, a status, and a Rust test that exists."""
import unittest

from paths import ROOT
from parity_doc import check

DOC = ROOT / 'docs/specs/2026-10-22-rust-runner-parity.md'
TESTS = ROOT / 'tests'

# The Python files of the runner (spec §7), in the order the map lists them.
FILES = [
    'test_prepare.py', 'test_link_libc.py', 'test_link_modules.py', 'test_isa_rewrite.py',
    'test_run_isa.py', 'test_patches.py', 'test_mods.py', 'test_mod_conflicts.py',
    'test_glyphs.py', 'test_content.py', 'test_guest_memory.py', 'test_graphics_options.py',
    'test_save_backup.py', 'test_pause_menu.py', 'test_native_cheats.py', 'test_legacy.py',
    'test_discord_presence.py', 'test_game_update.py',
]

# Mechanics with no Rust equivalent, each named by its test (or class) and explained in its row:
# the C runtime's own test, the Python-only iced-x86 import guard and the PowerShell shortcut rows.
ALLOWED_DROPS = {
    'tests/test_content.py::ContentTests',
    'tests/test_isa_rewrite.py::ReadBootLinkedTests::test_main_without_iced_x86',
}


class RunnerParityDocTest(unittest.TestCase):
    maxDiff = None  # the failures name the rows to fix

    def test_the_map_is_complete_and_honest(self):
        check(DOC, [TESTS / name for name in FILES], self, allowed_drops=ALLOWED_DROPS)


if __name__ == '__main__':
    unittest.main()
