"""The launcher's settings pages: which exist, in sidebar order, and what each refreshes when shown."""
import sys
import unittest

from paths import ROOT
sys.path.insert(0, str(ROOT / 'launcher'))
import gow3_launcher_win as launcher  # noqa: E402


class PagesTest(unittest.TestCase):
    def test_sidebar_lists_six_pages_in_order(self):
        self.assertEqual(launcher.PAGES, ('home', 'graphics', 'performance', 'game', 'trophies', 'advanced'))

    def test_showing_a_page_refreshes_what_it_holds(self):
        self.assertEqual(launcher.PAGE_REFRESH['home'], ('refresh_status',))
        self.assertNotIn('graphics', launcher.PAGE_REFRESH)  # nothing on it is a list to rebuild
        self.assertEqual(launcher.PAGE_REFRESH['game'], ('refresh_lists', 'refresh_game_patches'))

    def test_every_refreshed_page_is_a_sidebar_page(self):
        self.assertTrue(set(launcher.PAGE_REFRESH) <= set(launcher.PAGES))


if __name__ == '__main__':
    unittest.main()
