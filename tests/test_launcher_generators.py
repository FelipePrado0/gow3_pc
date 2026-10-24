"""The Rust launcher's data: the translation table and the parity fixtures the Python launcher wrote."""
import json
import unittest

from paths import ROOT


class LauncherGeneratorOutput(unittest.TestCase):
    def test_i18n_json_has_every_language_and_the_russian_pairs(self):
        data = json.loads((ROOT / 'launcher-rust/assets/i18n.json').read_text(encoding='utf-8'))
        self.assertEqual(sorted(data['translations']),
                         ['ar', 'de', 'es', 'fr', 'id', 'it', 'ja', 'ko', 'pl', 'pt', 'tr', 'zh'])
        for language, texts in data['translations'].items():
            self.assertEqual(len(texts), len(data['keys']), language)
        self.assertEqual(data['ru']['Graphics'], 'Графика')
        self.assertEqual(data['trophies']['count'], 35)
        self.assertEqual(len(data['trophies']['texts']['en']), 35)

    def test_fixture_cases_exist_with_every_file(self):
        for case in ('defaults', 'edited', 'legacy_controls'):
            folder = ROOT / 'launcher-rust/fixtures' / case
            for name in ('input.json', 'settings.json', 'spartanport.ini', 'environment.json'):
                self.assertTrue((folder / name).is_file(), f'{case}/{name}')

    def test_collected_case_holds_what_collect_wrote(self):
        folder = ROOT / 'launcher-rust/fixtures/collected'
        for name in ('input.json', 'settings.json', 'spartanport.ini'):
            self.assertTrue((folder / name).is_file(), f'collected/{name}')
        # collect() writes every ini key the window shows that the file lacked, not only the edits.
        lines = (folder / 'spartanport.ini').read_text(encoding='utf-8').splitlines()
        self.assertIn('vibration=100', lines)
        self.assertNotIn('preset=1', lines)


if __name__ == '__main__':
    unittest.main()
