#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Startup graphics options of gow3.ini (gpu/shim/gow3_graphics.h) for run.py.

The in-game menu and the launcher write them; run.py turns them into game patches and
environment variables at every start, so "Apply and restart" uses the new values. After a
restart the game confirms them (restart_unconfirmed=0) once it shows frames; when a launch
never did, the next start goes back to the last confirmed values.
"""
import json
import math
from pathlib import Path

RESOLUTIONS = ('native', '480p', '720p', '1440p', '1800p', '4K',
               '2560x1080', '3440x1440', '3840x1600', '5120x1440', '1920x1200', '2560x1600', '1280x800',
               '1440x900', '1360x768',
               '1280x960', '1440x1080', '1920x1440', '2048x1536', '2560x1920')
# The ultrawide and 16:10 sizes of the Ultrawide mod (#202), built in: beta until more players confirm them.
BETA_RESOLUTIONS = RESOLUTIONS[6:]
SIZES = {'native': (1920, 1080), '480p': (854, 480), '720p': (1280, 720), '1440p': (2560, 1440),
         '1800p': (3200, 1800), '4K': (3840, 2160)}
ASPECTS = (('16:9', 16 / 9), ('16:10', 16 / 10), ('21:9', 2.37), ('32:9', 32 / 9))
ENGINE_FPS = (60, 120, 240)
DEFAULTS = {'render_resolution': 'native', 'engine_fps': '120', 'deferred_readback': '1',
            'stale_readback': '1', 'async_shaders': '1'}
RESOLUTION_PREFIX = 'Resolution Patch'
FPS_PREFIX = 'Frame Rate Patch'
TEXTURE_FIX = 'Bug Fix - Texture Corruption Fix'


def read_ini(path):
    values = {}
    try:
        lines = Path(path).read_text(encoding='utf-8').splitlines()
    except OSError:
        return values
    for line in lines:
        if '=' in line and not line.lstrip().startswith('#'):
            key, value = line.split('=', 1)
            values[key.strip()] = value.strip()
    return values


def write_keys(path, updates):
    """Rewrites `updates` in place and appends missing keys, keeping every other line."""
    path = Path(path)
    try:
        lines = path.read_text(encoding='utf-8').splitlines()
    except OSError:
        lines = ['# gow3 settings (in-game menu: Insert / R3+L2)']
    done, out = set(), []
    for line in lines:
        key = line.split('=', 1)[0].strip() if '=' in line and not line.lstrip().startswith('#') else None
        if key in updates:
            out.append(f'{key}={updates[key]}')
            done.add(key)
        else:
            out.append(line)
    out += [f'{k}={v}' for k, v in updates.items() if k not in done]
    path.write_text('\n'.join(out) + '\n', encoding='utf-8')


def resolutions(extra=()):
    """The render resolutions: the built-in ones and those the enabled mods' patch files add."""
    return RESOLUTIONS + tuple(e for e in extra if e not in RESOLUTIONS)


def aspect_name(width, height):
    """The marketing name of the aspect ratio ("21:9" covers 2560x1080 to 3840x1600), else the reduced ratio."""
    ratio = width / height
    for name, value in ASPECTS:
        if abs(ratio - value) < 0.05:
            return name
    divisor = math.gcd(width, height)
    return f'{width // divisor}:{height // divisor}'


def resolution_label(name):
    """What the lists show for a render resolution: '1440p · 2560 × 1440 · 16:9', '3440 × 1440 · 21:9 (beta)'.
    `name` is a built-in name or a mod's "<width>x<height>"; anything else is shown as it is."""
    try:
        width, height = SIZES.get(name) or map(int, name.split('x'))
    except ValueError:
        return name
    title = 'Native' if name == 'native' else name if name in SIZES else None
    label = ' · '.join(p for p in (title, f'{width} × {height}', aspect_name(width, height)) if p)
    return label + (' (beta)' if name in BETA_RESOLUTIONS else '')


def startup_options(ini, extra=()):
    """The startup options, normalized; unknown values fall back to the defaults. `extra`: the
    resolutions of the enabled mods (a mod that is off leaves its resolution out: native)."""
    options = {key: ini.get(key, value) for key, value in DEFAULTS.items()}
    if options['render_resolution'] not in resolutions(extra):
        options['render_resolution'] = 'native'
    if options['engine_fps'] not in {str(f) for f in ENGINE_FPS}:
        options['engine_fps'] = '120'
    for key in ('deferred_readback', 'stale_readback', 'async_shaders'):
        options[key] = '0' if options[key] == '0' else '1'
    return options


def prepare(config, state_path, extra=()):
    """(options, reverted): the startup options of this launch, restoring the last confirmed
    ones when the previous launch after a restart never confirmed its own."""
    ini = read_ini(config)
    state = {}
    try:
        state = json.loads(Path(state_path).read_text(encoding='utf-8'))
    except (OSError, ValueError):
        pass
    options = startup_options(ini, extra)
    reverted = False
    if ini.get('restart_unconfirmed') == '1':
        if state.get('attempt') and state.get('last_good'):
            options = startup_options(state['last_good'], extra)
            write_keys(config, {**options, 'restart_unconfirmed': '0'})
            state['attempt'] = False
            reverted = True
        else:
            state['attempt'] = True
    else:
        state = {'last_good': options, 'attempt': False}
    Path(state_path).write_text(json.dumps(state, indent=2), encoding='utf-8')
    return options, reverted


def patch_names(names, options, texture_fix_available=True):
    """The launcher's patch list with the resolution and frame rate patches of `options`."""
    names = [n for n in names if not n.startswith(RESOLUTION_PREFIX) and not n.startswith(FPS_PREFIX)]
    resolution = options['render_resolution']
    if resolution != 'native':
        names = [n for n in names if n != TEXTURE_FIX] + [f'{RESOLUTION_PREFIX} - {resolution}']
    elif texture_fix_available and TEXTURE_FIX not in names:
        names.insert(0, TEXTURE_FIX)
    if options['engine_fps'] != '60':
        names.append(f'{FPS_PREFIX} - {options["engine_fps"]} FPS')
    return names


def restrict_to_available(names, available):
    """names without the patches this XML lacks. A missing frame rate patch is replaced by the
    highest one it has below the requested rate (the 240 FPS patch of the first XML has no
    counterpart in the second)."""
    kept = [n for n in names if n in available]
    for name in names:
        if name in available:
            continue
        print(f'Patches: {name} is not available for this game build: skipped', flush=True)
        if name.startswith(f'{FPS_PREFIX} - '):
            wanted = int(name.split(' - ')[1].split()[0])
            lower = [int(n.split(' - ')[1].split()[0]) for n in available if n.startswith(f'{FPS_PREFIX} - ')
                     and int(n.split(' - ')[1].split()[0]) <= wanted]
            if lower:
                kept.append(f'{FPS_PREFIX} - {max(lower)} FPS')
                print(f'Patches: using {kept[-1]} instead', flush=True)
    return kept


def applied_engine_fps(names):
    """The engine rate the applied patch list gives: 60 without a frame rate patch."""
    return max((int(n.split(' - ')[1].split()[0]) for n in names if n.startswith(f'{FPS_PREFIX} - ')), default=60)


def environment(options):
    return {'GOW3_DEFERRED_READBACK': options['deferred_readback'],
            'GOW3_STALE_READBACK': options['stale_readback'],
            'GOW3_ASYNC_SHADERS': options['async_shaders']}
