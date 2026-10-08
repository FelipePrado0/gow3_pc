#!/usr/bin/env python3
# gow3: builds the FSR 4.1.1 asset set for vk_fsr411.cpp from fsr4cap captures
# (capture_<render>_<output> directories written by capture_all.sh).
#
#   extract.py <dxil-spirv> <capture root> <output dir>
# (spirv-dis, spirv-as from SPIRV-Tools on PATH: the postpass is rewritten by postpass_lds.py.)
#
# Per (tier, model) set: the shaders of one frame translated to SPIR-V (dxil-spirv with
# --class-bindings: binding = register + 32 * class, SRV/UAV/CBV/sampler), named after the pass,
# and the model's initializer (weights). Tiers: t1080 (output up to 1920x1080), t2160 (larger).
# Models: m0 (quality ratios up to 2.0), m1 (ultra performance, 3.0).
#
# It also checks the rules vk_fsr411.cpp uses against every capture: the dispatch sequence, the
# group counts, the tensor size table and the frame constants. A mismatch is an error.
import glob, hashlib, math, os, re, struct, subprocess, sys

dxil_spirv, root, out = sys.argv[1:4]
FLAGS = ['--enable-shader-i8-dot', '--ssbo-uav', '--ssbo-srv', '--class-bindings', '--use-reflection-names',
         '--mixed-float-dot-product']  # as vkd3d-proton: dot2 of halves into float (VALVE extension)
PREFIX = 'fsr4_model_v07_fp8_no_scale_'
SEQUENCE = ['spd', 'prepass', 'pass0_post'] + [f'pass{k}{s}' for k in range(1, 13) for s in ('', '_post')] + ['postpass', 'rcas']
LEVEL = {1: 1, 2: 1, 3: 2, 4: 2, 5: 2, 6: 3, 7: 3, 8: 3, 9: 3, 10: 2, 11: 2, 12: 1}
errors = 0

def fail(msg):
    global errors
    errors += 1
    print('MISMATCH', msg)

def shader_name(path):
    data = open(path, 'rb').read()
    for s in re.findall(rb'[A-Za-z_][A-Za-z0-9_]{5,80}', data):
        s = s.decode()
        if s.startswith(PREFIX):
            return s[len(PREFIX):]
        if s.startswith('fsr_rcas'):
            return 'rcas'
    return 'spd'

def ceil_div(a, b):
    return (a + b - 1) // b

def tensor_sizes(aw, ah):
    d = [1, 0, 1, 1, 2, 2, 2, 3, 3, 3, 2, 2, 1, 1, 0, 0, 0]
    return [(aw >> s, ah >> s) for s in d]

def expected_groups(name, rw, rh, ow, oh):
    aw, ah = (ow + 7) & ~7, (oh + 7) & ~7
    if name == 'spd':
        return (ceil_div(rw, 64), ceil_div(rh, 64), 1)
    if name in ('prepass', 'rcas'):
        return (ceil_div(aw, 16), ceil_div(ah, 16), 1)
    if name == 'postpass':
        return (ceil_div(aw, 32), ceil_div(ah, 32), 1)
    m = re.fullmatch(r'pass(\d+)', name)
    if m:
        lw, lh = aw >> LEVEL[int(m[1])], ah >> LEVEL[int(m[1])]
        return (ceil_div(lw, 64), lh, 1)
    return None  # _post passes: bounds-checked in the shader, dispatched with a margin

sets = {}
for cap in sorted(glob.glob(os.path.join(root, 'capture_*'))):
    m = re.search(r'capture_(\d+)x(\d+)_(\d+)x(\d+)$', cap)
    rw, rh, ow, oh = map(int, m.groups())
    tier = 't1080' if ow <= 1920 and oh <= 1080 else 't2160'
    model = 'm1' if ow / rw > 2.5 else 'm0'
    trace = open(os.path.join(cap, 'trace.txt')).read()
    # Frame 1 (pipelines exist), or frame 0 of a one-frame run.
    mark = 'MARK frame 1\n' if 'MARK frame 1\n' in trace else 'MARK frame 0\n'
    frame = re.split(r'MARK (?:frame \d+|end)\n', trace.split(mark)[1])[0]
    disp = re.findall(r'DISPATCH #\d+ cs=(\w+) root=\w+ groups=(\d+),(\d+),(\d+)\n((?:  .*\n)*)', frame)
    names = [shader_name(os.path.join(cap, f'cs_{h}.dxil')) for h, *_ in disp]
    if names != SEQUENCE:
        fail(f'{cap}: sequence {names}')
        continue
    key = f'{tier}_{model}'
    entry = sets.setdefault(key, {'shaders': {}, 'init': None, 'caps': []})
    entry['caps'].append(os.path.basename(cap))
    for (h, gx, gy, gz, body), name in zip(disp, names):
        groups = (int(gx), int(gy), int(gz))
        want = expected_groups(name, rw, rh, ow, oh)
        if want and want != groups:
            fail(f'{cap} {name}: groups {groups}, rule {want}')
        prev = entry['shaders'].get(name)
        if prev and prev != h:
            fail(f'{cap} {name}: shader {h} differs from {prev} in the same set')
        entry['shaders'][name] = h
        entry.setdefault('dxil', {})[name] = os.path.join(cap, f'cs_{h}.dxil')
        cbv = re.search(r'CBV r\d+\+\d+ data=(data_\w+\.bin)', body)
        data = open(os.path.join(cap, cbv[1]), 'rb').read() if cbv else b''
        if data and re.fullmatch(r'pass\d+(_post)?', name):
            u = struct.unpack('<68I', data[:272])
            got = [u[i:i + 2] for i in range(0, 68, 4)]
            if got != tensor_sizes((ow + 7) & ~7, (oh + 7) & ~7):
                fail(f'{cap} {name}: tensor sizes {got}')
        if name == 'prepass':
            f = struct.unpack('<16f', data[:64])
            u = struct.unpack('<10I', data[64:104])
            want_f = [1 / ow, 1 / oh, ow / rw, oh / rh, rw / ow, rh / oh, None, None, 1 / rw, 1 / rh, ow, oh, ow, oh, 0, 0]
            for i, w in enumerate(want_f):
                if w is not None and abs(f[i] - w) > 1e-6 * max(1, abs(w)):
                    fail(f'{cap} prepass constant {i}: {f[i]} != {w}')
            if (u[0], u[1], u[3], u[4]) != (ow, oh, rw, rh):
                fail(f'{cap} prepass sizes {u[:5]}')
    init = re.search(r'COPYBUFFER r\d+\+0 <- r\d+\+0 size 131072 data=(data_\w+\.bin)', trace)
    blob = open(os.path.join(cap, init[1]), 'rb').read()
    if entry['init'] and entry['init'] != blob:
        fail(f'{cap}: initializer differs within {key}')
    entry['init'] = blob

for key, entry in sorted(sets.items()):
    d = os.path.join(out, key)
    os.makedirs(d, exist_ok=True)
    for name, path in entry['dxil'].items():
        spv = os.path.join(d, f'{name}.spv')
        subprocess.run([dxil_spirv, path, *FLAGS, '--output', spv], check=True, stderr=subprocess.DEVNULL)
        if name == 'postpass':
            # Stores through workgroup memory (bit-exact, ~2.3x faster): postpass_lds.py.
            os.replace(spv, os.path.join(d, 'postpass_orig.spv'))
            asm = subprocess.run(['spirv-dis', os.path.join(d, 'postpass_orig.spv')], check=True,
                                 capture_output=True, text=True).stdout
            lds = subprocess.run([sys.executable, os.path.join(os.path.dirname(__file__), 'postpass_lds.py')],
                                 input=asm, check=True, capture_output=True, text=True).stdout
            subprocess.run(['spirv-as', '--target-env', 'spv1.3', '-', '-o', spv], input=lds, check=True,
                           text=True)
    open(os.path.join(d, 'initializer.bin'), 'wb').write(entry['init'])
    print(f'{key}: {len(entry["dxil"])} shaders, initializer {hashlib.sha256(entry["init"]).hexdigest()[:12]}, '
          f'from {len(entry["caps"])} captures')
if errors:
    print(f'{errors} mismatches')
    sys.exit(1)
print('all rules match the captures')
