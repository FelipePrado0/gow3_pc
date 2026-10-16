"""What the launcher offers on a Mac as on Windows and Linux: updates from the Mac port's releases
(a disk image to download) and Apple's Metal performance HUD in MangoHud's place. The Mac's
launcher pages: test_launcher_upscalers.MacLauncherTest."""
import io
import json
import sys
import unittest
import urllib.error
from unittest import mock

from paths import ROOT
sys.path.insert(0, str(ROOT / 'launcher'))
import gow3_launcher_win as launcher  # noqa: E402


class ReleaseTest(unittest.TestCase):
    def release(self, **fields):
        body = {'tag_name': 'macos-v2.2.0', 'html_url': 'https://example.com/page',
                'assets': [{'name': 'gow3-windows.zip', 'browser_download_url': 'https://example.com/w.zip'},
                           {'name': 'gow3-macos.dmg', 'browser_download_url': 'https://example.com/m.dmg'}]}
        body.update(fields)
        return mock.patch.object(launcher.urllib.request, 'urlopen',
                                 return_value=io.BytesIO(json.dumps(body).encode()))

    def test_the_mac_build_offers_the_disk_image(self):
        with self.release(), mock.patch.object(launcher, 'RELEASE_ASSET', '.dmg'):
            self.assertEqual(launcher.latest_release(), ('2.2.0', 'https://example.com/m.dmg', 'https://example.com/page'))
        with self.release(), mock.patch.object(launcher, 'RELEASE_ASSET', '.zip'):
            self.assertEqual(launcher.latest_release()[1], 'https://example.com/w.zip')

    @unittest.skipUnless(sys.platform == 'darwin', 'the Mac build')
    def test_a_mac_asks_this_repository_for_the_disk_image(self):
        self.assertEqual((launcher.RELEASES_REPO, launcher.RELEASE_ASSET), ('FelipePrado0/gow3_pc', '.dmg'))
        self.assertIn('FelipePrado0/gow3_pc', launcher.RELEASES_API)

    def test_download_opens_the_disk_image_on_a_mac(self):
        app = launcher.Launcher.__new__(launcher.Launcher)
        app.process = object()  # the game may run: nothing is replaced while it does
        with mock.patch.object(launcher, 'MACOS', True), mock.patch.object(launcher, 'FROZEN', True), \
                mock.patch.object(launcher, 'open_url') as opened:
            app.install_update('2.2.0', 'https://example.com/m.dmg', 'https://example.com/page')
        opened.assert_called_once_with('https://example.com/m.dmg')

    def test_no_release_yet_is_said_plainly(self):
        app = launcher.Launcher.__new__(launcher.Launcher)
        app.ui_calls, app.messagebox = mock.Mock(), mock.Mock()
        missing = urllib.error.HTTPError(launcher.RELEASES_API, 404, 'Not Found', {}, None)
        with mock.patch.object(launcher, 'latest_release', side_effect=missing):
            app.check_update(manual=True)
        app.ui_calls.put.call_args[0][0]()
        app.messagebox.showinfo.assert_called_once()
        self.assertIn('No release', app.messagebox.showinfo.call_args[0][1])
        app.messagebox.showerror.assert_not_called()


class GameEnvironmentTest(unittest.TestCase):
    def environment(self, **settings):
        values = {**launcher.APP_DEFAULTS, 'game_dir': '', **settings}
        with mock.patch.object(launcher, 'MACOS', True), mock.patch.object(launcher, 'LINUX', False):
            return launcher.game_environment(values)

    def test_metal_hud_follows_its_switch(self):
        self.assertEqual(self.environment()['MTL_HUD_ENABLED'], '0')
        self.assertEqual(self.environment(metal_hud=True)['MTL_HUD_ENABLED'], '1')


if __name__ == '__main__':
    unittest.main()
