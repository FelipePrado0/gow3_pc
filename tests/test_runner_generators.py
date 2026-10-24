"""The Rust runner's parity fixtures: what tools/runner_fixtures.py wrote from the Python modules."""
import unittest

from paths import ROOT

FIXTURES = ROOT / 'launcher-rust/fixtures/runner'
DXT_NAMES = ('gradient', 'edges', 'alpha', 'noise', 'flat')

# case -> (input files, expected files), relative to the case folder's input/ and expected/.
CASES = {
    'prepare': (['eboot.bin', 'sce_module/libc.prx', 'sce_sys/param.sfo', 'dvdroot_ps4/PART1.WAD'],
                ['boot.bin', 'eboot.elf', 'entry.bin', 'analysis.json']),
    'link': (['boot.bin', 'eboot.bin', 'sce_module/libc.prx', 'sce_module/libSceFios2.prx'],
             ['boot-libc.bin', 'libc-link.json', 'boot-linked.bin', 'link.json']),
    'isa': (['boot-linked.bin'], ['isa-rewrite.bin', 'sites.json']),
    'content': (['param.sfo'], ['full/content.bin', 'full/content-profile.json',
                                'trial/content.bin', 'trial/content-profile.json']),
    'patches': (['eboot.elf', 'eboot.bin', 'param.sfo', 'patches.xml', 'mod.xml'],
                ['patches-merged.xml', 'patches.bin', 'patch_status.json']),
    'graphics': (['spartanport.ini', 'graphics_state.json', 'edit.json'],
                 [f'{state}/{name}' for state in ('good', 'tried', 'reverted')
                  for name in ('spartanport.ini', 'graphics_state.json', 'environment.json')]),
    'dxt': ([f'{name}.png' for name in DXT_NAMES],
            [f'{name}.{fmt}' for name in DXT_NAMES for fmt in ('dxt1', 'dxt5')]),
    'icon': ([], ['window_icon.bmp']),
    'glyphs': ([f'{name}.png' for name in ('TX2CDFA5F8', 'TX4158CBF9', 'TX0C09B6DB', 'TXF489D97F', 'TXEACA5E71',
                                           'TXBD54B283', 'TXCEE0EE2B')],
               [f'{style}/{name}.png' for style in ('xbox', 'nintendo', 'keyboard')
                for name in ('TX1BBACF4D', 'TX7FF61BF7', 'TXFB91136E', 'TXD43D1443')]
               + [f'{style}/{name}.png' for style in ('xbox', 'nintendo') for name in ('TXCE352DC5', 'TX2CDFA5F8')]
               + ['keyboard/TXDCA1B818.png', 'keyboard/TX77AAD542.png', 'keyboard/TXBF2035D0.png']
               + ['encode/xbox/TX7FF61BF7.dxt1', 'encode/keyboard/TXD43D1443.dxt5',
                  'encode_region/xbox/TXEACA5E71.dxt1']),
}


class RunnerFixtures(unittest.TestCase):
    def test_every_case_has_its_files(self):
        for case, (inputs, expected) in CASES.items():
            for folder, names in (('input', inputs), ('expected', expected)):
                for name in names:
                    path = FIXTURES / case / folder / name
                    self.assertTrue(path.is_file(), f'{case}/{folder}/{name}')

    def test_dxt_strips_are_whole_blocks(self):
        for name in DXT_NAMES:
            # 64 x 64 pixels are 16 x 16 blocks of 8 (DXT1) and 16 (DXT5) bytes.
            dxt1 = (FIXTURES / 'dxt/expected' / f'{name}.dxt1').read_bytes()
            dxt5 = (FIXTURES / 'dxt/expected' / f'{name}.dxt5').read_bytes()
            self.assertEqual((len(dxt1) % 8, len(dxt1)), (0, 16 * 16 * 8), name)
            self.assertEqual((len(dxt5) % 16, len(dxt5)), (0, 16 * 16 * 16), name)

    def test_content_profile_is_twenty_eight_bytes(self):
        for sku in ('full', 'trial'):
            data = (FIXTURES / 'content/expected' / sku / 'content.bin').read_bytes()
            self.assertEqual(len(data), 28, sku)
            self.assertEqual(data[:8], b'G3CONT01', sku)

    def test_isa_table_of_the_c_loader_fixture_is_reproduced(self):
        # The isa input is a G3PROBE5 image holding isa_fixture.bin's code: its table is that file.
        table = (FIXTURES / 'isa/expected/isa-rewrite.bin').read_bytes()
        self.assertEqual(table, (ROOT / 'tests/data/isa_fixture.bin').read_bytes())


if __name__ == '__main__':
    unittest.main()
