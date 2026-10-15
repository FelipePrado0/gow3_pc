#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Draw the pause-menu controller diagram (R_UI TXCE352DC5, 1024x512 DXT5) for another pad.

    draw_pad.py --sketch controllers.sketch --out art/CE352DC5.png [--scale 2]   # Xbox
    draw_pad.py --svg switch_pro.svg       --out art/CE352DC5.png [--scale 2]   # Nintendo

Geometry: the 'XBOX One Controller' artboard of a Sketch file (stroked vector paths; the logo
bitmap becomes a plain ring), or a filled-silhouette SVG icon (its ink is skeletonised to
lines). Style follows the game's DualShock diagram: ~2 px salmon-red lines, a near-black body
with faint wisps of red smoke, the silhouette carried by alpha, and the drawing stretched to
the same box as the original (913x489 of 1024x512: the game narrows it on screen).
"""
import argparse
import json
import math
import re
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

FONT = next(p for p in (Path(__file__).resolve().parent / 'gowfont.ttf', Path(__file__).resolve().parent.parent.parent / 'launcher' / 'gowfont.ttf') if p.is_file())
LINE = (222, 85, 66)
BODY = (10, 2, 2)
WISP = (96, 16, 12)
TARGET_BOX = (913 / 1024, 489 / 512)   # the original's extent, as a fraction of the texture
ARTBOARD = 'XBOX One Controller'
LOGO_LAYER = 'Bitmap'
SUPER = 4


# ---------------------------------------------------------------- Sketch (stroked paths)

def parse_point(text):
    x, y = re.match(r'\{([^,]+), ([^}]+)\}', text).groups()
    return float(x), float(y)


def bezier(p0, p1, p2, p3, n=24):
    out = []
    for i in range(n + 1):
        t = i / n
        u = 1 - t
        out.append((u ** 3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t ** 3 * p3[0],
                    u ** 3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t ** 3 * p3[1]))
    return out


def rounded_rect(w, h, r, n=12):
    r = min(r, w / 2, h / 2)
    pts = []
    for cx, cy, a0 in ((w - r, r, -90), (w - r, h - r, 0), (r, h - r, 90), (r, r, 180)):
        for i in range(n + 1):
            a = math.radians(a0 + 90 * i / n)
            pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts


def sketch_polyline(layer):
    f = layer['frame']
    w, h = f['width'], f['height']
    if layer['_class'] == 'oval':
        return [(w / 2 + w / 2 * math.cos(a), h / 2 + h / 2 * math.sin(a))
                for a in np.linspace(0, 2 * math.pi, 97)], True
    pts = layer['path']['points']
    closed = layer['path']['isClosed']
    if layer['_class'] == 'rectangle':
        r = max(p.get('cornerRadius', 0) for p in pts)
        return (rounded_rect(w, h, r) if r else [(0, 0), (w, 0), (w, h), (0, h)]), True
    nodes = [(parse_point(p['point']), p.get('hasCurveFrom'), parse_point(p['curveFrom']),
              p.get('hasCurveTo'), parse_point(p['curveTo'])) for p in pts]
    out = []
    count = len(nodes) if closed else len(nodes) - 1
    for i in range(count):
        a, b = nodes[i], nodes[(i + 1) % len(nodes)]
        p0, p3 = (a[0][0] * w, a[0][1] * h), (b[0][0] * w, b[0][1] * h)
        p1 = (a[2][0] * w, a[2][1] * h) if a[1] else p0
        p2 = (b[4][0] * w, b[4][1] * h) if b[3] else p3
        seg = bezier(p0, p1, p2, p3)
        out += seg if not out else seg[1:]
    return out, closed


def sketch_transform(points, layer, ox, oy):
    f = layer['frame']
    cx, cy = f['width'] / 2, f['height'] / 2
    rot = math.radians(-layer.get('rotation', 0))
    fx = -1 if layer.get('isFlippedHorizontal') else 1
    fy = -1 if layer.get('isFlippedVertical') else 1
    out = []
    for x, y in points:
        x, y = (x - cx) * fx, (y - cy) * fy
        x, y = x * math.cos(rot) - y * math.sin(rot), x * math.sin(rot) + y * math.cos(rot)
        out.append((x + cx + ox + f['x'], y + cy + oy + f['y']))
    return out


def sketch_collect(layer, ox=0, oy=0, strokes=None, texts=None):
    strokes = [] if strokes is None else strokes
    texts = [] if texts is None else texts
    for o in layer.get('layers', []):
        f = o['frame']
        if o.get('name') == LOGO_LAYER:
            cx, cy, r = ox + f['x'] + f['width'] / 2, oy + f['y'] + f['height'] / 2, f['width'] / 2
            strokes.append(([(cx + r * math.cos(a), cy + r * math.sin(a)) for a in np.linspace(0, 2 * math.pi, 97)], True))
        elif o['_class'] == 'text':
            text = o['attributedString'].get('string', o.get('name', ''))
            texts.append((text, ox + f['x'] + f['width'] / 2, oy + f['y'] + f['height'] / 2, f['height']))
        elif o['_class'] == 'group':
            sketch_collect(o, ox + f['x'], oy + f['y'], strokes, texts)
        elif o['_class'] == 'shapeGroup':
            for sub in o.get('layers', []):
                pts, closed = sketch_polyline(sub)
                strokes.append((sketch_transform(sketch_transform(pts, sub, 0, 0), o, ox, oy), closed))
    return strokes, texts


def load_sketch(path):
    """(strokes, texts, (width, height)) of the Xbox artboard."""
    with zipfile.ZipFile(path) as z:
        for name in z.namelist():
            if name.startswith('pages/') and name.endswith('.json'):
                for layer in json.loads(z.read(name))['layers']:
                    if layer.get('name') == ARTBOARD:
                        strokes, texts = sketch_collect(layer)
                        return strokes, texts, (layer['frame']['width'], layer['frame']['height'])
    raise ValueError(f'artboard {ARTBOARD!r} not found')


# ---------------------------------------------------------------- SVG (filled icon)

TOKEN = re.compile(r'[MmLlHhVvCcSsZz]|-?(?:\d+\.?\d*|\.\d+)(?:e-?\d+)?')


def svg_subpaths(d):
    """[(points, closed)] of an SVG path's subpaths, curves flattened."""
    toks = TOKEN.findall(d)
    i, cmd = 0, None
    cur = start = (0.0, 0.0)
    last_ctrl = None
    subs, pts = [], []

    def num():
        nonlocal i
        v = float(toks[i]); i += 1
        return v
    while i < len(toks):
        if toks[i].isalpha():
            cmd = toks[i]; i += 1
            if cmd in 'Zz':
                if pts:
                    subs.append((pts, True))
                pts, cur = [], start
                last_ctrl = None
                continue
        rel = cmd.islower()
        c = cmd.upper()
        if c == 'M':
            x, y = num(), num()
            if rel:
                x, y = cur[0] + x, cur[1] + y
            if pts:
                subs.append((pts, False))
            pts, cur, start = [(x, y)], (x, y), (x, y)
            cmd = 'l' if rel else 'L'
            last_ctrl = None
        elif c == 'L':
            x, y = num(), num()
            if rel:
                x, y = cur[0] + x, cur[1] + y
            cur = (x, y); pts.append(cur); last_ctrl = None
        elif c == 'H':
            x = num(); cur = ((cur[0] + x) if rel else x, cur[1]); pts.append(cur); last_ctrl = None
        elif c == 'V':
            y = num(); cur = (cur[0], (cur[1] + y) if rel else y); pts.append(cur); last_ctrl = None
        elif c in 'CS':
            if c == 'C':
                x1, y1 = num(), num()
            x2, y2, x, y = num(), num(), num(), num()
            if rel:
                if c == 'C':
                    x1, y1 = cur[0] + x1, cur[1] + y1
                x2, y2, x, y = cur[0] + x2, cur[1] + y2, cur[0] + x, cur[1] + y
            if c == 'S':
                x1, y1 = (2 * cur[0] - last_ctrl[0], 2 * cur[1] - last_ctrl[1]) if last_ctrl else cur
            pts += bezier(cur, (x1, y1), (x2, y2), (x, y), 16)[1:]
            last_ctrl, cur = (x2, y2), (x, y)
    if pts:
        subs.append((pts, False))
    return subs


def load_svg(path):
    """(paths, size): each path is its list of subpaths; rects become paths."""
    root = ET.parse(path).getroot()
    vb = [float(v) for v in root.get('viewBox', '0 0 1024 1024').split()]
    paths = []
    for el in root.iter():
        tag = el.tag.split('}')[-1]
        if tag == 'path':
            paths.append(svg_subpaths(el.get('d')))
        elif tag == 'rect':
            x, y, w, h = (float(el.get(k)) for k in ('x', 'y', 'width', 'height'))
            paths.append([([(x, y), (x + w, y), (x + w, y + h), (x, y + h)], True)])
    return paths, (vb[2], vb[3])


# ---------------------------------------------------------------- composition

def fit(bounds_w, bounds_h, W, H):
    """Scale factors and offsets that stretch a drawing of bounds_w x bounds_h into the
    original's box inside a W x H texture (the game narrows the quad on screen)."""
    kx, ky = W * TARGET_BOX[0] / bounds_w, H * TARGET_BOX[1] / bounds_h
    return kx, ky, (W - bounds_w * kx) / 2, (H - bounds_h * ky) / 2


def wisps(shape, seed=11):
    """Faint, soft clouds of smoke in [0,1]: sparse low blobs with a wispy high-frequency
    texture inside them, like the original's barely-there body."""
    rng = np.random.default_rng(seed)
    h, w = shape

    def fractal(base, octaves, gain):
        total = np.zeros(shape, np.float32)
        amp, norm = 1.0, 0.0
        for o in range(octaves):
            n = base * 2 ** o
            layer = np.array(Image.fromarray((rng.random((n, n)) * 255).astype(np.uint8)).resize((w, h), Image.BICUBIC),
                             np.float32) / 255
            total += layer * amp
            norm += amp
            amp *= gain
        total /= norm
        return (total - total.min()) / (total.max() - total.min() + 1e-6)
    blobs = np.clip((fractal(9, 4, 0.55) - 0.52) / 0.48, 0, 1) ** 1.6
    detail = fractal(24, 3, 0.6)
    sm = blobs * (0.35 + 0.65 * detail)
    return np.array(Image.fromarray((sm * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(w / 700)), np.float32) / 255


def compose(ink, silhouette, W, H, scale):
    """ink and silhouette: 'L' images at W x H. Returns the finished RGBA texture."""
    sm = wisps((H, W))
    rgb = np.array(BODY, np.float32)[None, None, :] + np.array(WISP, np.float32)[None, None, :] * sm[..., None]
    img = Image.fromarray(np.clip(rgb, 0, 255).astype(np.uint8), 'RGB').convert('RGBA')
    img.putalpha(silhouette)
    lines = Image.new('RGBA', (W, H), LINE + (0,))
    lines.putalpha(ink)
    img.alpha_composite(lines.filter(ImageFilter.GaussianBlur(1.0 * scale)))
    img.alpha_composite(lines)
    return img


def draw_letters(draw, texts, kx, ky, offx, offy):
    for text, cx, cy, fh in texts:
        font = ImageFont.truetype(str(FONT), int(fh * ky * 0.95))
        box = draw.textbbox((0, 0), text, font=font)
        draw.text((cx * kx + offx - (box[0] + box[2]) / 2, cy * ky + offy - (box[1] + box[3]) / 2), text, font=font, fill=255)


def render_sketch(path, W, H, scale):
    strokes, texts, _size = load_sketch(path)
    s = SUPER
    xs = [p[0] for pts, _ in strokes for p in pts]
    ys = [p[1] for pts, _ in strokes for p in pts]
    bx, by = min(xs), min(ys)
    kx, ky, offx, offy = fit(max(xs) - bx, max(ys) - by, W * s, H * s)
    strokes = [([(x - bx, y - by) for x, y in pts], closed) for pts, closed in strokes]
    texts = [(t, cx - bx, cy - by, fh) for t, cx, cy, fh in texts]
    P = lambda pts: [(x * kx + offx, y * ky + offy) for x, y in pts]
    sil = Image.new('L', (W * s, H * s), 0)
    sd = ImageDraw.Draw(sil)
    for pts, closed in strokes:
        if len(pts) > 20:
            sd.polygon(P(pts), fill=255)
    ink = Image.new('L', (W * s, H * s), 0)
    d = ImageDraw.Draw(ink)
    lw = max(1, int(2.2 * scale * s))
    for pts, closed in strokes:
        pp = P(pts)
        d.line(pp + ([pp[0]] if closed else []), fill=255, width=lw, joint='curve')
    # letters: the sketch's font is generic; the game's serif at the same spot
    draw_letters(d, texts, kx, ky, offx, offy)
    return ink.resize((W, H), Image.LANCZOS), sil.resize((W, H), Image.LANCZOS)


def render_svg(path, W, H, scale, letters):
    """A filled icon: even-odd ink skeletonised to lines; the outer subpath is the silhouette.
    `letters` = {(approx cx, cy): 'A'} replaces the icon's own letter shapes with the game font."""
    from skimage.morphology import skeletonize, dilation, disk
    paths, (vw, vh) = load_svg(path)
    # the icon's own bounds
    allpts = [p for sp in paths for pts, _ in sp for p in pts]
    xs, ys = [p[0] for p in allpts], [p[1] for p in allpts]
    bx, by, bw, bh = min(xs), min(ys), max(xs) - min(xs), max(ys) - min(ys)
    s = 2
    kx, ky, offx, offy = fit(bw, bh, W * s, H * s)
    P = lambda pts: [((x - bx) * kx + offx, (y - by) * ky + offy) for x, y in pts]
    ink = np.zeros((H * s, W * s), bool)
    sil = Image.new('L', (W * s, H * s), 0)
    body = max(paths, key=lambda sp: len(sp[0][0]))
    ImageDraw.Draw(sil).polygon(P(body[0][0]), fill=255)
    text_spots = []
    for sp in paths:
        # a path whose bounds sit inside a letter spot is the icon's letter: skip it
        px = [p[0] for pts, _ in sp for p in pts]; py = [p[1] for pts, _ in sp for p in pts]
        cx, cy, w = (min(px) + max(px)) / 2, (min(py) + max(py)) / 2, max(px) - min(px)
        hit = next((L for (lx, ly), L in letters.items() if abs(lx - cx) < 25 and abs(ly - cy) < 25), None)
        if hit and w < 40:
            continue
        if hit:
            text_spots.append((hit, cx - bx, cy - by, w * 0.62))
        m = np.zeros_like(ink)
        for pts, _ in sp:
            layer = Image.new('L', (W * s, H * s), 0)
            ImageDraw.Draw(layer).polygon(P(pts), fill=255)
            m ^= np.array(layer, bool)
        ink |= m
    sk = skeletonize(ink)
    sk = dilation(sk, disk(max(1, int(1.1 * scale * s))))
    ink_img = Image.fromarray((sk * 255).astype(np.uint8))
    d = ImageDraw.Draw(ink_img)
    draw_letters(d, text_spots, kx, ky, offx, offy)
    return ink_img.resize((W, H), Image.LANCZOS), sil.resize((W, H), Image.LANCZOS)


SWITCH_LETTERS = {(740, 322): 'X', (683, 377): 'Y', (797, 377): 'A', (740, 432): 'B'}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--sketch', type=Path)
    parser.add_argument('--svg', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--scale', type=int, default=1)
    args = parser.parse_args()
    W, H = 1024 * args.scale, 512 * args.scale
    if args.sketch:
        ink, sil = render_sketch(args.sketch, W, H, args.scale)
    elif args.svg:
        ink, sil = render_svg(args.svg, W, H, args.scale, SWITCH_LETTERS)
    else:
        parser.error('--sketch or --svg')
    img = compose(ink, sil, W, H, args.scale)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    img.save(args.out)
    print(f'{W}x{H} -> {args.out}')


if __name__ == '__main__':
    main()
