"""Render resolution: the built-in sizes plus those of the enabled mods' patch files (an ultrawide mod)."""
import unittest

from test_launcher_save import LauncherOverTemporaryFiles, launcher

PATCH = ('<Patch><TitleID><ID>CUSA01623</ID></TitleID>'
         '<Metadata Name="Resolution Patch - 3840x1080" AppVer="01.02"><PatchList/></Metadata></Patch>')


class ModResolutionsTest(LauncherOverTemporaryFiles):
    def make_mod(self, name='Ultrawide'):
        folder = launcher.DATA_DIR / 'mods' / name / 'patches'
        folder.mkdir(parents=True)
        (folder / 'u.xml').write_text(PATCH)

    def sizes(self):
        return [value for value, _text in self.app.resolution_options()]

    def test_the_list_follows_the_mods(self):
        builtin = [value for value, _text in launcher.RESOLUTION_CHOICES]
        self.assertEqual(self.sizes(), builtin)  # no mod yet
        self.make_mod()
        self.app.refresh_lists()  # the Mods page lists the new folder
        self.assertEqual(self.sizes(), builtin + ['3840x1080'])
        label = dict(self.app.resolution_options())['3840x1080'][0]
        self.assertEqual(label, '3840 × 1080 · 32:9 (mod)')
        self.app.mod_vars['Ultrawide'].set(False)  # unticked on the Mods page
        self.assertEqual(self.sizes(), builtin)
        self.app.mod_vars['Ultrawide'].set(True)
        self.app.var('mods_enabled', 'app').set(False)  # "Load mods" off
        self.assertEqual(self.sizes(), builtin)

    def test_a_saved_mod_resolution_stays_chosen_while_its_mod_is_on(self):
        self.make_mod()
        self.app.refresh_lists()
        self.app.var('render_resolution', 'ini').set('3840x1080')
        self.root.update()
        self.assertEqual(self.app.var('render_resolution', 'ini').get(), '3840x1080')
        self.assertIn('3840 × 1080 · 32:9 (mod)', self.app.summary())


if __name__ == '__main__':
    unittest.main()
