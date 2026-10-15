#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Draw Xbox-style replacements for God of War III Remastered's button prompt textures.

Output: PNGs in <out>/ named by the texture hash they replace. Everything is drawn from
scratch (no game pixels), in the style of the originals: dark bevelled bodies, coloured glyphs
with a soft glow, serif text for the shoulder/trigger/stick buttons.
"""
import argparse
from pathlib import Path
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
SERIF = str((HERE / 'gowfont.ttf') if (HERE / 'gowfont.ttf').is_file() else HERE.parent.parent / 'launcher' / 'gowfont.ttf')

# Button styles: which letter sits on each PlayStation position, its colour, and the labels of
# the other buttons. Nintendo pads are monochrome.
INK = (240, 240, 240)
STYLES = {
    'xbox': {
        'face': {'cross': ('A', (96, 190, 72)), 'circle': ('B', (226, 64, 64)),
                 'square': ('X', (70, 140, 235)), 'triangle': ('Y', (240, 196, 56))},
        'stroke': {'cross': (57, 203, 74), 'circle': (232, 36, 22), 'square': (40, 128, 255),
                   'triangle': (255, 196, 0)},
        'shoulder': {'l1': 'LB', 'r1': 'RB', 'l2': 'LT', 'r2': 'RT', 'l3': 'LS', 'r3': 'RS'},
    },
    'nintendo': {
        'face': {'cross': ('B', INK), 'circle': ('A', INK), 'square': ('Y', INK), 'triangle': ('X', INK)},
        'stroke': {'cross': INK, 'circle': INK, 'square': INK, 'triangle': INK},
        'shoulder': {'l1': 'L', 'r1': 'R', 'l2': 'ZL', 'r2': 'ZR', 'l3': 'LS', 'r3': 'RS'},
    },
}
STYLE = 'xbox'
FACE = STYLES[STYLE]['face']
SHOULDER = STYLES[STYLE]['shoulder']

# Texture hash (first half of the TX name) -> what to draw.
TARGETS = {
    # in-text glyphs, 64x64 DXT5
    '1BBACF4D': ('face', 'cross'), 'BA40AFD2': ('face', 'circle'),
    '64F3D812': ('face', 'square'), '573C282A': ('face', 'triangle'),
    '91A47980': ('pill', 'l1'), '5CED5AE9': ('pill', 'r1'), '6FC83ACE': ('pill', 'l2'),
    '67E86446': ('pill', 'r2'), '9AE3BA81': ('pill', 'l3'), 'FB33513A': ('pill', 'r3'),
    # QTE prompts, 128x128 DXT5
    '7FF61BF7': ('qte', 'cross'), '9D590CC8': ('qte', 'circle'),
    '1F5FC983': ('qte', 'square'), 'E8889579': ('qte', 'triangle'),
    # mash prompts, 256x256 DXT1 atlases: top entry rotated 180 degrees, bottom upright
    '0FFF3CCB': ('atlas', ('r2', 'r1')), 'FB91136E': ('atlas', ('l2', 'l1')),
}


def glow(layer, radius, strength=1.0):
    """A blurred copy of `layer`, for the halo under a glyph."""
    g = layer.filter(ImageFilter.GaussianBlur(radius))
    if strength != 1.0:
        a = g.getchannel('A').point(lambda v: min(255, int(v * strength)))
        g.putalpha(a)
    return g


def text_layer(size, text, font, fill, scale=4, weight=0.0):
    """`text` centred in a size×size RGBA layer, rendered at `scale`× and downsampled.
    `weight` thickens the strokes by that many output pixels on each side."""
    big = Image.new('RGBA', (size[0] * scale, size[1] * scale), (0, 0, 0, 0))
    d = ImageDraw.Draw(big)
    f = ImageFont.truetype(font.path, font.size * scale)
    box = d.textbbox((0, 0), text, font=f, anchor='ls')
    w, h = box[2] - box[0], box[3] - box[1]
    x = (big.width - w) / 2 - box[0]
    y = (big.height - h) / 2 - box[1]
    d.text((x, y), text, font=f, fill=fill, anchor='ls', stroke_width=int(weight * scale), stroke_fill=fill)
    return big.resize(size, Image.LANCZOS)


def disc_body(size, radius, top=(40, 40, 40), bottom=(8, 8, 8), rim=None):
    """A dark bevelled disc with a vertical gradient and a thin lighter rim."""
    s = 4
    big = Image.new('RGBA', (size * s, size * s), (0, 0, 0, 0))
    d = ImageDraw.Draw(big)
    c, r = size * s / 2, radius * s
    inner = r
    grad = Image.new('RGBA', big.size, (0, 0, 0, 0))
    gd = ImageDraw.Draw(grad)
    for y in range(int(c - inner), int(c + inner) + 1):
        t = (y - (c - inner)) / (2 * inner)
        col = tuple(int(top[i] * (1 - t) + bottom[i] * t) for i in range(3)) + (255,)
        gd.line((0, y, big.width, y), fill=col)
    mask = Image.new('L', big.size, 0)
    ImageDraw.Draw(mask).ellipse((c - inner, c - inner, c + inner, c + inner), fill=255)
    big.paste(grad, (0, 0), mask)
    # soft highlight on the upper half
    hl = Image.new('RGBA', big.size, (0, 0, 0, 0))
    ImageDraw.Draw(hl).ellipse((c - inner * 0.8, c - inner * 0.95, c + inner * 0.8, c - inner * 0.1),
                               fill=(255, 255, 255, 22))
    hl = hl.filter(ImageFilter.GaussianBlur(3 * s))
    big.alpha_composite(hl)
    return big.resize((size, size), Image.LANCZOS)


def pill_body(size, w, h, radius):
    s = 4
    big = Image.new('RGBA', (size * s, size * s), (0, 0, 0, 0))
    d = ImageDraw.Draw(big)
    c = size * s / 2
    box = (c - w * s / 2, c - h * s / 2, c + w * s / 2, c + h * s / 2)
    inner = box
    grad = Image.new('RGBA', big.size, (0, 0, 0, 0))
    gd = ImageDraw.Draw(grad)
    for y in range(int(inner[1]), int(inner[3]) + 1):
        t = (y - inner[1]) / (inner[3] - inner[1])
        v = int(42 * (1 - t) + 8 * t)
        gd.line((0, y, big.width, y), fill=(v, v, v, 255))
    mask = Image.new('L', big.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(inner, radius * s, fill=255)
    big.paste(grad, (0, 0), mask)
    return big.resize((size, size), Image.LANCZOS)


SCALE = 1  # 1 = the game's own texture sizes, 2 = double (needs a rebuilt WAD)


def S(v):
    return int(round(v * SCALE))


def draw_face(symbol):
    """64x64 in-text face button: dark disc, coloured serif letter with a thin glow."""
    letter, col = FACE[symbol]
    im = disc_body(S(64), S(29))
    font = ImageFont.truetype(SERIF, S(44))
    t = text_layer((S(64), S(64)), letter, font, col + (255,), weight=S(0.9))
    im.alpha_composite(t)
    return im


def draw_pill(button):
    """64x64 in-text shoulder/trigger/stick button: dark pill, white serif text."""
    size = (S(64), S(64))
    if button in ('l3', 'r3'):  # the stick buttons are round in the original
        im = disc_body(S(64), S(29))
        font = ImageFont.truetype(SERIF, S(28))
    else:
        im = pill_body(S(64), S(60), S(42), S(9))
        font = ImageFont.truetype(SERIF, S(30))
    t = text_layer(size, SHOULDER[button], font, (255, 255, 255, 255), weight=S(0.5))
    im.alpha_composite(t)
    return im


def fractal_noise(shape, octaves=5, base=8, seed=0):
    """Value noise in [0,1]: several blurred random layers at doubling frequency."""
    rng = np.random.default_rng(seed)
    h, w = shape
    total = np.zeros(shape, dtype=np.float32)
    amp, norm = 1.0, 0.0
    for o in range(octaves):
        n = base * (2 ** o)
        layer = rng.random((n, n), dtype=np.float32)
        layer = np.array(Image.fromarray((layer * 255).astype(np.uint8)).resize((w, h), Image.BICUBIC),
                         dtype=np.float32) / 255
        total += layer * amp
        norm += amp
        amp *= 0.55
    total /= norm
    lo, hi = total.min(), total.max()
    return (total - lo) / (hi - lo + 1e-6)


def stone_letter(letter, deep, hot, n, seed):
    """A letter rendered as cracked, veined stone, for additive prompts: mottled body in `deep`,
    hot veins towards `hot`, dark fissures the scene shows through, a lit bevel on the edge."""
    s = 2
    big = n * s
    font = ImageFont.truetype(SERIF, S(100) * s)
    mask_img = text_layer((big, big), letter, font, (255, 255, 255, 255), scale=2, weight=S(1.5) * s)
    mask = np.array(mask_img.getchannel('A'), dtype=np.float32) / 255
    mottle = fractal_noise((big, big), octaves=5, base=6, seed=seed)
    fine = fractal_noise((big, big), octaves=4, base=24, seed=seed + 7)
    ridged = 1 - np.abs(2 * fractal_noise((big, big), octaves=3, base=5, seed=seed + 13) - 1)
    cracks = np.clip((ridged - 0.86) / 0.1, 0, 1)           # thin lines along noise ridges
    veins = np.clip((mottle - 0.62) / 0.3, 0, 1) ** 1.5        # hot patches
    deep, hot = np.array(deep, np.float32), np.array(hot, np.float32)
    shade = 0.55 + 0.65 * mottle + 0.3 * (fine - 0.5)
    col = deep[None, None, :] * shade[..., None]
    col = col * (1 - veins[..., None]) + hot[None, None, :] * veins[..., None]
    col *= (1 - 0.85 * cracks)[..., None]
    # bevel: an inner erosion; the rim is lit from the top-left
    inner = np.array(mask_img.getchannel('A').filter(ImageFilter.MinFilter(2 * (S(3) * s) + 1)), np.float32) / 255
    rim = np.clip(mask - inner, 0, 1)
    yy, xx = np.mgrid[0:big, 0:big]
    light = np.clip(0.5 + 0.5 * ((-(xx + yy) / big) + 1.0), 0, 1)   # bright top-left, dark bottom-right
    rim_col = hot[None, None, :] * (0.35 + 0.75 * light)[..., None]
    col = col * (1 - rim[..., None]) + rim_col * rim[..., None]
    col = np.clip(col, 0, 255) * mask[..., None]
    img = Image.fromarray(col.astype(np.uint8), 'RGB').resize((n, n), Image.LANCZOS)
    return img


# Pastel stroke colours for the QTE glyphs, in the key of the originals' pale cross/circle.


def letter_paths(letter, size):
    """Polyline strokes of a wide, monoline capital, in a unit box scaled to `size`.
    The letters are drawn as marks (like the original cross/circle), not typeset."""
    L, R, T, B = 0.25, 0.75, 0.24, 0.76      # glyph box inside the texture (the originals use the middle half)
    M = (L + R) / 2
    if letter == 'A':
        return [[(L, B), (M, T), (R, B)], [(L + 0.12, 0.6), (R - 0.12, 0.6)]]
    if letter == 'X':
        return [[(L, T), (R, B)], [(L, B), (R, T)]]
    if letter == 'Y':
        return [[(L, T), (M, 0.52)], [(R, T), (M, 0.52)], [(M, 0.52), (M, B)]]
    if letter == 'B':
        # stem plus two bowls, as arcs: approximated with polylines
        import math
        stem = [(L + 0.04, T), (L + 0.04, B)]
        def bowl(y0, y1, rx):
            cy = (y0 + y1) / 2
            ry = (y1 - y0) / 2
            pts = [(L + 0.04, y0)]
            pts += [(L + 0.04 + 0.22 + rx * math.sin(t), cy - ry * math.cos(t))
                    for t in [i / 16 * math.pi for i in range(0, 17)]]
            pts.append((L + 0.04, y1))
            return pts
        return [stem, bowl(T, 0.5, 0.17), bowl(0.5, B, 0.2)]
    raise ValueError(letter)


def draw_qte(symbol):
    """128x128 QTE prompt over black: a pale monoline letter, stroke and edge like the
    original cross/circle marks (no body, no texture)."""
    letter, _ = FACE[symbol]
    col = STYLES[STYLE]['stroke'][symbol]
    n = S(128)
    s = 4
    big = Image.new('RGB', (n * s, n * s), (0, 0, 0))
    d = ImageDraw.Draw(big)
    width = int(S(9) * s)                  # the originals' stroke is ~9 px at 128
    for path in letter_paths(letter, n):
        pts = [(x * n * s, y * n * s) for x, y in path]
        d.line(pts, fill=col, width=width, joint='curve')
        for x, y in (pts[0], pts[-1]):
            d.ellipse((x - width / 2, y - width / 2, x + width / 2, y + width / 2), fill=col)
    im = big.resize((n, n), Image.LANCZOS).filter(ImageFilter.GaussianBlur(S(0.6)))
    return im.convert('RGBA')


def draw_atlas(buttons):
    """256x256 DXT1 atlas, black background: top entry rotated 180 degrees, bottom upright."""
    n = S(256)
    im = Image.new('RGBA', (n, n), (0, 0, 0, 255))
    font = ImageFont.truetype(SERIF, S(88))
    top = text_layer((n, n // 2), SHOULDER[buttons[0]], font, (255, 255, 255, 255), weight=S(1)).rotate(180)
    bottom = text_layer((n, n // 2), SHOULDER[buttons[1]], font, (255, 255, 255, 255), weight=S(1))
    im.alpha_composite(top, (0, 0))
    im.alpha_composite(bottom, (0, n // 2))
    return im


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, default=HERE.parent / 'out' / 'art')
    parser.add_argument('--scale', type=int, default=1, help='1 = original sizes, 2 = double')
    parser.add_argument('--style', choices=sorted(STYLES), default='xbox')
    args = parser.parse_args()
    global SCALE, STYLE, FACE, SHOULDER
    SCALE = args.scale
    STYLE = args.style
    FACE = STYLES[STYLE]['face']
    SHOULDER = STYLES[STYLE]['shoulder']
    args.out.mkdir(parents=True, exist_ok=True)
    draw = {'face': draw_face, 'pill': draw_pill, 'qte': draw_qte, 'atlas': draw_atlas}
    for tex, (kind, arg) in TARGETS.items():
        draw[kind](arg).save(args.out / f'{tex}.png')
    print(f'{len(TARGETS)} textures drawn in {args.out}')


if __name__ == '__main__':
    main()
