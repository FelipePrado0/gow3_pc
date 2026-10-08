#!/usr/bin/env python3
# gow3: compares fsr4cap captures (capture_<render>_<output> dirs): weights, scratch size and the
# shader/groups of each dispatch of frame 1.
import glob, os, re, sys
root = sys.argv[1]
def name(cap, h):
    data = open(os.path.join(cap, f'cs_{h}.dxil'), 'rb').read()
    for s in re.findall(rb'[A-Za-z_][A-Za-z0-9_]{5,80}', data):
        if s.startswith((b'fsr4_model', b'fsr_')):
            return s.decode().replace('fsr4_model_v07_fp8_no_scale_', '')
    return 'spd'
rows = {}
for cap in sorted(glob.glob(os.path.join(root, 'capture_*'))):
    t = open(os.path.join(cap, 'trace.txt')).read()
    init = re.search(r'COPYBUFFER r\d+\+0 <- r\d+\+0 size (\d+) data=data_(\w{8})', t)
    scratch = re.search(r'UAV r\d+ buffer first=0 num=(\d+)', t)
    frame = t.split('MARK frame 1')[1].split('MARK frame 2')[0]
    seq = [(name(cap, h), h[:6], g) for h, g in re.findall(r'DISPATCH #\d+ cs=(\w+) root=\w+ groups=(\S+)', frame)]
    key = os.path.basename(cap)[8:]
    rows[key] = seq
    print(f'{key:24} init {init[1]} {init[2]}  scratch words {scratch[1]}  dispatches {len(seq)}')
keys = list(rows)
print('\nshader hash per pass (columns = captures):')
for i, (n, _, _) in enumerate(rows[keys[0]]):
    cells = [f'{rows[k][i][1]}:{rows[k][i][2]}' if i < len(rows[k]) else '-' for k in keys]
    print(f'{n:16} ' + ' '.join(f'{c:16}' for c in cells))
