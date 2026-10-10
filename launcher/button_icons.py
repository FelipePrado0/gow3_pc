# SPDX-License-Identifier: GPL-2.0-or-later
"""Controller button icons for the launcher's Controls page, drawn on a Tk canvas (no images).

Buttons are named as SDL gamepad buttons ('a', 'leftshoulder', 'dpup', ...); the style chooses
PlayStation or Xbox symbols, colors and labels."""

# PlayStation symbols and Xbox letters of the face buttons, with their colors.
FACE = {
    'playstation': {'a': ('cross', '#6b9ae8'), 'b': ('circle', '#e5555e'),
                    'x': ('square', '#d683d6'), 'y': ('triangle', '#45c4a2')},
    'xbox': {'a': ('A', '#3fae49'), 'b': ('B', '#d9372f'), 'x': ('X', '#2f6fd9'), 'y': ('Y', '#e5b521')},
}
LABELS = {
    'playstation': {'leftshoulder': 'L1', 'rightshoulder': 'R1', 'lefttrigger': 'L2', 'righttrigger': 'R2',
                    'leftstick': 'L3', 'rightstick': 'R3', 'start': 'OPTIONS', 'back': 'SHARE',
                    'touchpad': 'TOUCH', 'guide': 'PS', 'misc1': 'MUTE'},
    'xbox': {'leftshoulder': 'LB', 'rightshoulder': 'RB', 'lefttrigger': 'LT', 'righttrigger': 'RT',
             'leftstick': 'LS', 'rightstick': 'RS', 'start': 'MENU', 'back': 'VIEW',
             'touchpad': 'TOUCH', 'guide': 'XBOX', 'misc1': 'SHARE'},
}
DPAD = {'dpup': 'up', 'dpdown': 'down', 'dpleft': 'left', 'dpright': 'right'}
# The game's PS4 buttons (launcher CONTROLS) as SDL buttons, for their PlayStation icon.
GAME_BUTTONS = {'cross': 'a', 'circle': 'b', 'square': 'x', 'triangle': 'y', 'l1': 'leftshoulder',
                'r1': 'rightshoulder', 'l2': 'lefttrigger', 'r2': 'righttrigger', 'l3': 'leftstick',
                'r3': 'rightstick', 'options': 'start', 'touchpad': 'touchpad', 'up': 'dpup',
                'down': 'dpdown', 'left': 'dpleft', 'right': 'dpright'}

INK, DIM, BODY = '#e6e0d8', '#5a5250', '#262020'


def spec(button, style):
    """(kind, text, color) of an SDL button in a style, or None for an unknown button."""
    if button in FACE.get(style, {}):
        text, color = FACE[style][button]
        return ('symbol' if style == 'playstation' else 'letter'), text, color
    if button in DPAD:
        return 'dpad', DPAD[button], INK
    label = LABELS.get(style, {}).get(button)
    if not label:
        return None
    kind = ('bumper' if button.endswith('shoulder') else 'trigger' if button.endswith('trigger')
            else 'stick' if button.endswith('stick') else 'pill')
    return kind, label, INK


def draw(canvas, button, style, size):
    """Draws the icon of `button` filling a size × size canvas (cleared first)."""
    canvas.delete('all')
    found = spec(button, style)
    if not found:
        return
    kind, text, color = found
    s, m = size, size / 2
    font = ('Segoe UI', max(6, int(s * 0.28)), 'bold')
    if kind in ('symbol', 'letter'):
        canvas.create_oval(1, 1, s - 1, s - 1, fill=BODY if kind == 'symbol' else color, outline='')
        if kind == 'letter':
            canvas.create_text(m, m, text=text, fill='#101010', font=('Segoe UI', max(7, int(s * 0.42)), 'bold'))
            return
        w, r = max(2, s // 11), s * 0.24
        if text == 'cross':
            canvas.create_line(m - r, m - r, m + r, m + r, fill=color, width=w)
            canvas.create_line(m - r, m + r, m + r, m - r, fill=color, width=w)
        elif text == 'circle':
            canvas.create_oval(m - r, m - r, m + r, m + r, outline=color, width=w)
        elif text == 'square':
            canvas.create_rectangle(m - r, m - r, m + r, m + r, outline=color, width=w)
        else:
            canvas.create_polygon(m, m - r * 1.15, m + r * 1.1, m + r * 0.8, m - r * 1.1, m + r * 0.8,
                                  outline=color, fill='', width=w)
    elif kind == 'dpad':
        t, a = s * 0.3, s * 0.08
        arms = {'up': (m - t / 2, a, m + t / 2, m), 'down': (m - t / 2, m, m + t / 2, s - a),
                'left': (a, m - t / 2, m, m + t / 2), 'right': (m, m - t / 2, s - a, m + t / 2)}
        for name, box in arms.items():
            canvas.create_rectangle(*box, fill=INK if name == text else DIM, outline='')
    elif kind == 'stick':
        canvas.create_oval(1, 1, s - 1, s - 1, fill=BODY, outline=DIM, width=max(1, s // 14))
        canvas.create_text(m, m, text=text, fill=color, font=font)
    else:
        # Bumpers: a flat bar; triggers: taller with a rounded top; the rest: a small pill.
        top, bottom = {'bumper': (s * 0.28, s * 0.72), 'trigger': (s * 0.12, s * 0.88)}.get(kind, (s * 0.3, s * 0.7))
        r = (bottom - top) / 2 if kind != 'trigger' else s * 0.2
        x0, x1 = 1, s - 1
        canvas.create_oval(x0, top, x0 + 2 * r, top + 2 * r, fill=BODY, outline='')
        canvas.create_oval(x1 - 2 * r, top, x1, top + 2 * r, fill=BODY, outline='')
        canvas.create_rectangle(x0 + r, top, x1 - r, bottom, fill=BODY, outline='')
        canvas.create_rectangle(x0, top + r, x1, bottom, fill=BODY, outline='')
        small = ('Segoe UI', max(5, int(s * (0.2 if len(text) > 3 else 0.3))), 'bold')
        canvas.create_text(m, (top + bottom) / 2, text=text, fill=color, font=small)
