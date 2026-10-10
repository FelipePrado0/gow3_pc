#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Finds God of War III's frame pieces in a frame capture (in-game menu, Diagnostics, Capture
frame): scene depth, camera view-projection, scene color before the HUD and the HUD passes.
These are the inputs a temporal upscaler or frame generation needs.

Usage: python tools/gow3_frame_map.py user/captures/frame_<...>.txt [...]
"""
import math
import re
import sys


def passes(text):
    """[(index, kind, draws, header+body)] in frame order."""
    out = []
    for block in re.split(r'\n(?=#\d+ )', text)[1:]:
        m = re.match(r'#(\d+) (PASS|COMPUTE) (?:draws|dispatches) (\d+)', block)
        out.append((int(m[1]), m[2], int(m[3]), block))
    return out


def targets(block, kind):
    return re.findall(rf'{kind}\d* (0x[0-9a-f]+) (\S+) (\d+)x(\d+)', block)


def depth(block):
    m = re.search(r'depth  (0x[0-9a-f]+) (\S+) (\d+)x(\d+)', block)
    return m.groups() if m else None


def buffers(block):
    """[(stage, slot, size, floats)] dumped for the pass's first draws."""
    out = []
    for m in re.finditer(r'buffer stage ([0-9a-f]+) slot (\d+) at \S+ size (\d+):((?:\n    \[ *\d+\][^\n]*)+)', block):
        floats = []
        for line in m[4].strip().split('\n'):
            floats += [float(x) for x in line.split(']', 1)[1].split()]
        out.append((m[1], int(m[2]), int(m[3]), floats))
    return out


def view_projection(m, aspect):
    """Whether 16 column-major floats are a view-projection: right and up rows orthogonal to the
    view direction, scaled by the projection with the screen's aspect."""
    cols = [m[i:i + 4] for i in range(0, 16, 4)]
    rows = [[cols[c][r] for c in range(4)] for r in range(4)]

    def norm(v):
        return math.sqrt(sum(x * x for x in v[:3]))
    n0, n1, n3 = norm(rows[0]), norm(rows[1]), norm(rows[3])
    if min(n0, n1, n3) < 1e-3 or abs(n0 / n1 - 1 / aspect) > 0.01:
        return False
    dot = lambda a, b, na, nb: abs(sum(x * y for x, y in zip(a[:3], b[:3]))) / (na * nb)
    return max(dot(rows[0], rows[1], n0, n1), dot(rows[0], rows[3], n0, n3), dot(rows[1], rows[3], n1, n3)) < 0.01


def analyze(path):
    ps = passes(open(path, encoding='utf-8', errors='replace').read())
    # Scene depth: the depth-only pass with the most draws (the Z pre-pass).
    prepass = max((p for p in ps if p[1] == 'PASS' and not targets(p[3], 'color') and depth(p[3])),
                  key=lambda p: p[2])
    d = depth(prepass[3])
    width, height = int(d[2]), int(d[3])
    # Camera: the pre-pass vertex constants with a view-projection at float 0 (world at 16).
    camera = next(((s, slot, f[:16]) for s, slot, size, f in buffers(prepass[3])
                   if size >= 128 and view_projection(f[:16], width / height)), None)
    # Scene color: the last full-size pass that samples the bloom chain's 1280x720-class image
    # (the post-process composite); every later pass drawing into it is HUD.
    composite = None
    for p in ps:
        colors = targets(p[3], 'color')
        sampled = targets(p[3], 'samples')
        if (p[1] == 'PASS' and len(colors) == 1 and int(colors[0][2]) == width and
                any(int(w) == width // 2 for _a, _f, w, _h in sampled)):
            composite = (p[0], colors[0])
    hud = [p[0] for p in ps if composite and p[0] > composite[0] and p[1] == 'PASS'
           and any(c[0] == composite[1][0] for c in targets(p[3], 'color'))]
    print(path)
    print(f'  depth        pass #{prepass[0]} ({prepass[2]} draws): {d[0]} {d[1]} {width}x{height}')
    if camera:
        print(f'  camera       vertex shader {camera[0]} slot {camera[1]}, floats 0-15 (column-major '
              f'view-projection), sx {math.hypot(camera[2][0], camera[2][4], camera[2][8]):.4f}')
    else:
        print('  camera       NOT FOUND')
    if composite:
        print(f'  scene color  after pass #{composite[0]}: {composite[1][0]} {composite[1][1]} '
              f'{composite[1][2]}x{composite[1][3]}')
        print(f'  HUD passes   {hud}')
    else:
        print('  scene color  NOT FOUND (pause menus have no 3D scene)')
    return bool(camera and composite)


if __name__ == '__main__':
    ok = [analyze(p) for p in sys.argv[1:]]
    sys.exit(0 if ok and all(ok) else 1)
