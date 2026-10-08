#!/usr/bin/env python3
# gow3: summarizes an fsr4cap capture (capture/trace.txt): per dispatch of one frame the shader
# name (from the DXIL), groups, bindings and the first words of its constant buffer.
#   analyze.py <capture dir> [frame] [words]
import os, re, struct, sys

cap = sys.argv[1]
frame = sys.argv[2] if len(sys.argv) > 2 else '1'
words = int(sys.argv[3]) if len(sys.argv) > 3 else 24

def shader_name(h):
    path = os.path.join(cap, f'cs_{h}.dxil')
    if not os.path.exists(path):
        return '?'
    data = open(path, 'rb').read()
    for s in re.findall(rb'[A-Za-z_][A-Za-z0-9_]{5,80}', data):
        if s.startswith((b'fsr4_model', b'fsr_', b'ffx_')):
            return s.decode()
    return '(unnamed, %d bytes)' % len(data)

lines = open(os.path.join(cap, 'trace.txt')).read().split('\n')
inside = False
for i, line in enumerate(lines):
    if line.startswith('MARK frame '):
        inside = line == f'MARK frame {frame}'
        continue
    if not inside:
        continue
    m = re.match(r'DISPATCH #(\d+) cs=(\w+) root=(\w+) groups=(\S+)', line)
    if m:
        print(f'#{m[1]} {shader_name(m[2])} groups {m[4]} root {m[3][:8]}')
        continue
    m = re.search(r'(CBV r\d+\+\d+) data=(data_\w+\.bin)', line)
    if m:
        data = open(os.path.join(cap, m[2]), 'rb').read()[:words * 4]
        u = struct.unpack(f'<{len(data) // 4}I', data)
        f = struct.unpack(f'<{len(data) // 4}f', data)
        cells = []
        for a, b in zip(u, f):
            cells.append(f'{b:.6g}' if 1e-6 < abs(b) < 1e7 else str(a))
        print(f'    {m[1]}: ' + ' '.join(cells))
        continue
    if line.startswith('    [') or line.startswith('  param'):
        print('  ' + line.strip())
