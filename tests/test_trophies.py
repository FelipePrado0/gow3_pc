# SPDX-License-Identifier: GPL-2.0-or-later
"""Tests of the trophy support of the launcher (launcher/gow3_trophies.py and its language tables).

They need no game files: the trophy package (trophy00.trp) is built here, byte by byte."""

import io
import struct
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'launcher'))

import gow3_lang  # noqa: E402
import gow3_trophies  # noqa: E402

try:
    from PIL import Image
except ImportError:  # the colour test is skipped without Pillow
    Image = None

LANGUAGES = ('en', 'ru', 'ar', 'es', 'pt', 'fr', 'de', 'it', 'pl', 'tr', 'zh', 'ja', 'ko')


def build_trp(entries, magic=b'\xdc\xa2\x4d\x00', element=0x40):
    """A trophy package: 0x60-byte header, one entry per (name, bytes), then the files."""
    header_size = 0x60
    offset = header_size + element * len(entries)
    table, body = b'', b''
    for name, data in entries:
        table += name.encode().ljust(0x20, b'\0') + struct.pack('>QQ', offset + len(body), len(data))
        table = table.ljust(len(table) + element - 0x30, b'\0')
        body += data
    header = magic + struct.pack('>I', 3) + struct.pack('>Q', header_size + len(table) + len(body))
    header += struct.pack('>II', len(entries), element)
    return header.ljust(header_size, b'\0') + table + body


def solid_png(rgb):
    out = io.BytesIO()
    Image.new('RGB', (32, 32), rgb).save(out, 'PNG')
    return out.getvalue()


class TrophyTexts(unittest.TestCase):
    def test_every_language_lists_all_trophies(self):
        for language in LANGUAGES:
            texts = gow3_trophies.TEXTS[language]
            self.assertEqual(len(texts), gow3_trophies.TROPHY_COUNT, language)
            for number, (name, description) in enumerate(texts):
                self.assertTrue(name.strip() and description.strip(), (language, number))
                self.assertNotIn('|', name + description, (language, number))  # the separator of trophy_texts.txt
                self.assertLessEqual(len(name.encode()), 63, (language, number))  # runtime buffers
                self.assertLessEqual(len(description.encode()), 159, (language, number))

    def test_text_falls_back_to_english(self):
        self.assertEqual(gow3_trophies.text('xx', 3), gow3_trophies.TEXTS['en'][3])
        self.assertEqual(gow3_trophies.text('pt', 0)[0], 'Rei da colina')
        self.assertEqual(gow3_trophies.text('en', 99), ('', ''))
        self.assertEqual(gow3_trophies.text('en', -1), ('', ''))


class PageTexts(unittest.TestCase):
    def test_every_language_has_the_same_texts(self):
        keys = set(gow3_trophies.UI['pt'])
        for language, table in gow3_trophies.UI.items():
            self.assertEqual(set(table), keys, language)
            for key, text in table.items():
                self.assertTrue(text.strip(), (language, key))
                self.assertEqual(text.count('{}'), key.count('{}'), (language, key))

    def test_launcher_tables_carry_them(self):
        for language in gow3_trophies.UI:
            table = gow3_lang.table(language)
            for key, text in gow3_trophies.UI[language].items():
                self.assertEqual(table.get(key), text, (language, key))


class TrophyPackage(unittest.TestCase):
    def setUp(self):
        self.folder = Path(tempfile.mkdtemp())

    def write(self, name, data):
        path = self.folder / name
        path.write_bytes(data)
        return path

    def test_icons_are_read_by_id(self):
        png = b'\x89PNG-fake'
        path = self.write('trophy00.trp', build_trp([
            ('TROPCONF.ESFM', b'encrypted'), ('TROP000.PNG', png + b'0'), ('TROP034.PNG', png + b'34'),
            ('TROP_00.ESFM', b'x'), ('ICON0.PNG', b'icon')]))
        self.assertEqual(gow3_trophies.read_trp_icons(path), {0: png + b'0', 34: png + b'34'})

    def test_bad_packages_give_nothing(self):
        good = build_trp([('TROP001.PNG', b'abc')])
        self.assertEqual(gow3_trophies.read_trp_icons(self.folder / 'missing.trp'), {})
        self.assertEqual(gow3_trophies.read_trp_icons(self.write('a.trp', b'')), {})
        self.assertEqual(gow3_trophies.read_trp_icons(self.write('b.trp', b'\0\0\0\0' + good[4:])), {})
        self.assertEqual(gow3_trophies.read_trp_icons(self.write('c.trp', build_trp([('TROP001.PNG', b'abc')], element=8))), {})
        self.assertEqual(gow3_trophies.read_trp_icons(self.write('d.trp', good[:-2])), {})  # a file past the end

    def test_unlocked_bits(self):
        flags = [0, 0, 0, 0]
        for trophy in (0, 5, 31, 32, 34, 127):
            flags[trophy // 32] |= 1 << (trophy % 32)
        path = self.write('trophies.dat', struct.pack('<4I', *flags))
        self.assertEqual(gow3_trophies.read_unlocked_trophies(path), {0, 5, 31, 32, 34, 127})
        self.assertEqual(gow3_trophies.read_unlocked_trophies(self.folder / 'none.dat'), set())
        self.assertEqual(gow3_trophies.read_unlocked_trophies(self.write('short.dat', b'\x02')), {1})

    @unittest.skipUnless(Image, 'Pillow is not installed')
    def test_tier_follows_the_colour_of_the_icon(self):
        colours = {'platinum': (139, 167, 167), 'bronze': (137, 89, 52), 'silver': (136, 119, 104),
                   'gold': (141, 104, 27)}  # the average colours of the icons of the game
        for tier, rgb in colours.items():
            self.assertEqual(gow3_trophies.trophy_tier(solid_png(rgb)), tier)
        self.assertEqual(gow3_trophies.trophy_tier(b'not a png'), '')
        self.assertEqual(gow3_trophies.trophy_tier(solid_png((0, 0, 0))), '')


if __name__ == '__main__':
    unittest.main()
