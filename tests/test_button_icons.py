import sys
import unittest

from paths import ROOT
sys.path.insert(0, str(ROOT / 'launcher'))
import button_icons  # noqa: E402
import gow3_launcher_win as launcher  # noqa: E402


class FakeCanvas:
    def __init__(self):
        self.items = []

    def delete(self, _tag):
        self.items.clear()

    def __getattr__(self, name):
        if name.startswith('create_'):
            return lambda *args, **kw: self.items.append((name, args, kw))
        raise AttributeError(name)


class ButtonIconTest(unittest.TestCase):
    def test_every_game_button_has_a_playstation_icon(self):
        for button, _key, _pad in launcher.CONTROLS:
            sdl = button_icons.GAME_BUTTONS[button]
            self.assertIsNotNone(button_icons.spec(sdl, 'playstation'), button)

    def test_every_gamepad_button_has_an_icon_in_both_styles(self):
        for style, labels in launcher.PAD_STYLES.items():
            for sdl in labels:
                self.assertIsNotNone(button_icons.spec(sdl, style), (style, sdl))
        for _button, _key, pad in launcher.CONTROLS:
            if pad:
                for style in launcher.PAD_STYLES:
                    self.assertIsNotNone(button_icons.spec(pad, style), (style, pad))

    def test_styles_differ_where_the_controllers_do(self):
        self.assertEqual(button_icons.spec('a', 'playstation')[:2], ('symbol', 'cross'))
        self.assertEqual(button_icons.spec('a', 'xbox')[:2], ('letter', 'A'))
        self.assertEqual(button_icons.spec('leftshoulder', 'xbox')[:2], ('bumper', 'LB'))
        self.assertEqual(button_icons.spec('righttrigger', 'playstation')[:2], ('trigger', 'R2'))
        self.assertEqual(button_icons.spec('dpleft', 'xbox')[:2], ('dpad', 'left'))
        self.assertIsNone(button_icons.spec('nonsense', 'xbox'))

    def test_draw_fills_and_clears_the_canvas(self):
        canvas = FakeCanvas()
        for sdl in list(button_icons.LABELS['xbox']) + ['a', 'b', 'x', 'y', 'dpup', 'dpright']:
            for style in ('playstation', 'xbox'):
                button_icons.draw(canvas, sdl, style, 28)
                self.assertTrue(canvas.items, (style, sdl))
        button_icons.draw(canvas, 'nonsense', 'xbox', 28)
        self.assertEqual(canvas.items, [])


if __name__ == '__main__':
    unittest.main()
