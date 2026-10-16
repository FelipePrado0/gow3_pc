"""Launcher setting for the game's multisampling (GOW3_NO_MSAA): environment, page and texts.
Automatic leaves the decision to the game (off on AMD's and Intel's Windows drivers,
gow3_msaa_default.h) and names the chosen card and that decision, read from
`gow3-gpu-capabilities --list`."""
import os
import sys
import unittest
from unittest import mock

from paths import ROOT
sys.path.insert(0, str(ROOT / 'launcher'))
import gow3_lang  # noqa: E402
import gow3_launcher_win as launcher  # noqa: E402

# `gow3-gpu-capabilities --list`: index, name, type, multisampling default, the game's pick.
ARC_LAPTOP = [('0', 'Intel(R) Arc(TM) Graphics', 'integrated', 'off', '0'),
              ('1', 'NVIDIA GeForce RTX 4060 Laptop GPU', 'discrete', 'on', '1')]
OLD_TOOL = [('0', 'AMD Radeon RX 6600', 'discrete')]


def added(env):
    return {key for key in env if key not in os.environ}


class NoMsaaEnvironmentTest(unittest.TestCase):
    def test_auto_leaves_the_game_default_and_keeps_an_inherited_value(self):
        self.assertEqual(launcher.APP_DEFAULTS['no_msaa'], '')
        self.assertNotIn('GOW3_NO_MSAA', added(launcher.game_environment(dict(launcher.APP_DEFAULTS))))
        # A GOW3_NO_MSAA set by a .bat before the launcher still reaches the game on Auto.
        with mock.patch.dict(os.environ, {'GOW3_NO_MSAA': '1'}):
            self.assertEqual(launcher.game_environment(dict(launcher.APP_DEFAULTS))['GOW3_NO_MSAA'], '1')

    def test_off_and_on_set_the_variable(self):
        for value in ('1', '0'):
            with self.subTest(value=value):
                env = launcher.game_environment({**launcher.APP_DEFAULTS, 'no_msaa': value})
                self.assertEqual(env['GOW3_NO_MSAA'], value)

    def test_choices_and_legacy_settings(self):
        self.assertEqual([value for value, _text in launcher.NO_MSAA], ['', '1', '0'])
        legacy = dict(launcher.APP_DEFAULTS)
        legacy.pop('no_msaa')
        self.assertNotIn('GOW3_NO_MSAA', added(launcher.game_environment(legacy)))


class ChosenGpuTest(unittest.TestCase):
    def test_the_graphics_card_setting_wins_else_the_games_pick(self):
        self.assertEqual(launcher.chosen_gpu(ARC_LAPTOP, '0'), ARC_LAPTOP[0])
        self.assertEqual(launcher.chosen_gpu(ARC_LAPTOP, ''), ARC_LAPTOP[1])
        self.assertEqual(launcher.chosen_gpu(ARC_LAPTOP, '7'), ARC_LAPTOP[1])  # a card that is gone
        self.assertEqual(launcher.chosen_gpu(ARC_LAPTOP, 1), ARC_LAPTOP[1])

    def test_without_the_decision_columns_or_a_list_there_is_no_answer(self):
        self.assertIsNone(launcher.chosen_gpu(OLD_TOOL, ''))
        self.assertEqual(launcher.chosen_gpu(OLD_TOOL, '0'), OLD_TOOL[0])
        self.assertIsNone(launcher.chosen_gpu([], ''))

    def test_auto_label_names_the_card_and_the_decision(self):
        options = launcher.no_msaa_options(ARC_LAPTOP, '')
        self.assertEqual([value for value, _text in options], ['', '1', '0'])
        self.assertEqual(options[0][1], ('Automatic (NVIDIA GeForce RTX 4060 Laptop GPU: on)',))
        self.assertEqual(launcher.no_msaa_options(ARC_LAPTOP, '0')[0][1], ('Automatic (Intel(R) Arc(TM) Graphics: off)',))
        self.assertEqual(options[1:], launcher.NO_MSAA[1:])
        # An old tool, none at all, or a spelling this launcher does not know: the plain label.
        self.assertEqual(launcher.no_msaa_options(OLD_TOOL, '0')[0][1], ('Automatic',))
        self.assertEqual(launcher.no_msaa_options([], '')[0][1], ('Automatic',))
        self.assertEqual(launcher.no_msaa_options([('0', 'x', 'discrete', 'OFF', '1')], '')[0][1], ('Automatic',))
        with mock.patch.object(launcher, 'LANG', 'pt'):
            self.assertEqual(launcher.no_msaa_options(ARC_LAPTOP, '0')[0][1],
                             ('Automático (Intel(R) Arc(TM) Graphics: desligado)',))
            self.assertEqual(launcher.no_msaa_options(ARC_LAPTOP, '1')[0][1],
                             ('Automático (NVIDIA GeForce RTX 4060 Laptop GPU: ligado)',))

    def test_portuguese_texts(self):
        table = gow3_lang.table('pt')
        texts = ('Game multisampling', 'Automatic ({}: off)', 'Automatic ({}: on)', launcher.NO_MSAA_HINT)
        for text in texts + tuple(text[0] for _value, text in launcher.NO_MSAA):
            with self.subTest(text=text[:40]):
                self.assertTrue(table.get(text), f'no Portuguese text for {text!r}')


class NoMsaaLauncherTest(unittest.TestCase):
    def test_performance_page_offers_the_choice(self):
        try:
            import tkinter as tk
            from tkinter import ttk
            root = tk.Tk()
        except ImportError as error:
            self.skipTest(f'Tk unavailable: {error}')
        except tk.TclError as error:
            self.skipTest(f'Tk display unavailable: {error}')
        root.withdraw()
        self.addCleanup(root.destroy)

        def build(language='en', gpu_id=''):
            with mock.patch.dict(os.environ, {'GOW3_CONFIG': os.devnull}), \
                    mock.patch.object(launcher, 'LANG', language), \
                    mock.patch.object(launcher, 'list_gpus', return_value=ARC_LAPTOP):
                app = launcher.Launcher.__new__(launcher.Launcher)
                app.tk, app.ttk, app.root, app.dpi = tk, ttk, root, 1.0
                app.content, app.pages, app.vars = ttk.Frame(root), {}, {}
                app.app = {**launcher.APP_DEFAULTS, 'gpu_id': gpu_id}
                app.ini, app.ini_lines = launcher.load_ini()
                app.control_vars, app.mod_order, app.patch_vars = {}, [], {}
                app.pad_style = tk.StringVar(root, value='playstation')
                app.build_performance()
            return app

        def widgets(parent):
            for child in parent.winfo_children():
                yield child
                yield from widgets(child)

        app = build()
        def multisampling_boxes(page, off_label):
            return [widget for widget in widgets(page)
                    if isinstance(widget, ttk.Combobox) and widget['values'][1] == off_label]

        boxes = multisampling_boxes(app.pages['performance'], 'Off (fixes black lines)')
        self.assertEqual(len(boxes), 1)
        self.assertEqual(tuple(boxes[0]['values']),
                         ('Automatic (NVIDIA GeForce RTX 4060 Laptop GPU: on)', 'Off (fixes black lines)', 'On'))
        self.assertEqual(app.vars['no_msaa'].get(), '')
        boxes[0].current(1)
        boxes[0].event_generate('<<ComboboxSelected>>')
        self.assertEqual(app.vars['no_msaa'].get(), '1')
        # The graphics card list still offers both cards, and Automatic follows that choice at once.
        cards = [widget for widget in widgets(app.pages['performance'])
                 if isinstance(widget, ttk.Combobox) and widget['values'][0] == 'Automatic']
        if launcher.MACOS:  # a Mac has one GPU: no card to choose (build_performance)
            self.assertEqual(cards, [])
            return
        self.assertEqual(tuple(cards[0]['values'])[1:], tuple(name for _i, name, *_rest in ARC_LAPTOP))
        cards[0].current(1)
        cards[0].event_generate('<<ComboboxSelected>>')
        self.assertEqual(app.vars['gpu_id'].get(), '0')
        self.assertEqual(boxes[0]['values'][0], 'Automatic (Intel(R) Arc(TM) Graphics: off)')
        self.assertEqual(boxes[0].current(), 1)
        self.assertEqual(app.vars['no_msaa'].get(), '1')
        # A saved Graphics card choice is what the label starts from.
        app = build(gpu_id='0')
        boxes = multisampling_boxes(app.pages['performance'], 'Off (fixes black lines)')
        self.assertEqual(boxes[0]['values'][0], 'Automatic (Intel(R) Arc(TM) Graphics: off)')

        # The Portuguese page shows the translated title, label and hint.
        table = gow3_lang.table('pt')
        app = build('pt')
        texts = {widget.cget('text') for widget in widgets(app.pages['performance'])
                 if isinstance(widget, ttk.Label)}
        self.assertIn(table[launcher.NO_MSAA_HINT], texts)
        self.assertTrue(any(text.startswith(table['Game multisampling']) for text in texts))
        boxes = multisampling_boxes(app.pages['performance'], table['Off (fixes black lines)'])
        self.assertEqual(tuple(boxes[0]['values']),
                         ('Automático (NVIDIA GeForce RTX 4060 Laptop GPU: ligado)', table['Off (fixes black lines)'],
                          table['On']))


if __name__ == '__main__':
    unittest.main()
