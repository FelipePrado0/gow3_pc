#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Windows counterpart of run.sh: prepares the game image and starts bb-probe.exe.

Same environment variables as run.sh (BB_GAME_DIR, BB_DATA_DIR, BB_FPS, BB_RENDER_RES,
BB_LIVE_RES, BB_PATCHES, BB_MODS_*, BB_USER_DIR, ...). Extra arguments go to bb-probe.
The in-game "Apply and restart" runs this script again through BB_RESTART_COMMAND;
`--after PID` waits for the previous game process to end first (its GPU device and memory).
"""
import ctypes
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

PORT = Path(__file__).resolve().parent


def fail(message):
    print(message, file=sys.stderr)
    sys.exit(1)


def wait_for(pid):
    kernel32 = ctypes.windll.kernel32
    SYNCHRONIZE = 0x00100000
    handle = kernel32.OpenProcess(SYNCHRONIZE, False, pid)
    if handle:
        kernel32.WaitForSingleObject(handle, 30000)
        kernel32.CloseHandle(handle)


def no_console():
    """Started without a console (the launcher): console programs must not open their own."""
    return 0 if ctypes.windll.kernel32.GetConsoleWindow() else subprocess.CREATE_NO_WINDOW


def run_script(name, *args, capture=False):
    # Inside the packaged Bloodborne.exe (PyInstaller) the executable runs scripts itself.
    script = ['--script'] if getattr(sys, 'frozen', False) else []
    command = [sys.executable, *script, str(PORT / 'scripts' / name), *map(str, args)]
    if capture:
        result = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, text=True,
                                creationflags=no_console())
        if result.returncode:
            sys.exit(result.returncode)
        return result.stdout
    # stdin given: the output handles are passed explicitly (a windowed Bloodborne.exe child would
    # get none otherwise).
    result = subprocess.run(command, stdin=subprocess.DEVNULL, creationflags=no_console())
    if result.returncode:
        sys.exit(result.returncode)
    return None


def ini_value(path, key):
    try:
        for line in Path(path).read_text(encoding='utf-8', errors='replace').splitlines():
            match = re.fullmatch(rf'{re.escape(key)}=(.*)', line.strip())
            if match:
                return match.group(1)
    except OSError:
        pass
    return None


def find_executable(name):
    """bin/ (packaged), else out/ (built with build.sh)."""
    for candidate in (PORT / 'bin' / name, PORT / 'out' / name):
        if candidate.is_file():
            return candidate
    return None


def main():
    args = sys.argv[1:]
    if len(args) >= 2 and args[0] == '--after':
        wait_for(int(args[1]))
        args = args[2:]
    # As run.sh: relative paths (BB_GAME_DIR, BB_DATA_DIR, fsr4_shaders/) are from the port.
    os.chdir(PORT)
    env = os.environ
    data = Path(env.get('BB_DATA_DIR', PORT))
    out = data / 'out'
    out.mkdir(parents=True, exist_ok=True)
    env.setdefault('BB_CONFIG', str(data / 'bbport.ini'))
    config = env['BB_CONFIG']
    if not env.get('BB_FSR411_DIR') and not (PORT / 'fsr4_411').is_dir() and (data / 'fsr4_411').is_dir():
        env['BB_FSR411_DIR'] = str(data / 'fsr4_411')

    probe = Path(env['BB_PROBE']) if env.get('BB_PROBE') else find_executable('bb-probe.exe')
    if not probe or not probe.is_file():
        fail('bb-probe.exe not found: build it with build.sh (MSYS2 CLANG64) or use a packaged build.')
    # Development builds run from the MSYS2 tree: its CLANG64 DLLs (SDL3, FFmpeg, ...).
    if not (probe.parent / 'SDL3.dll').is_file():
        clang64 = Path(env.get('MSYS2_ROOT', r'C:\msys64')) / 'clang64' / 'bin'
        if clang64.is_dir():
            env['PATH'] = f'{clang64}{os.pathsep}{env.get("PATH", "")}'

    game = Path(env.get('BB_GAME_DIR', PORT.parent / 'CUSA03173'))
    if not (game / 'eboot.bin').is_file():
        fail(f'No eboot.bin in {game} (set BB_GAME_DIR).')
    original_game = game.resolve()
    game = Path(run_script('mods.py', game, '--out', out,
                           '--mods-dir', env.get('BB_MODS_DIR', data / 'mods'),
                           '--config', env.get('BB_MODS_CONFIG', data / 'mods.json'),
                           '--enabled', env.get('BB_MODS_ENABLED', '1'), capture=True).strip())
    mod_view = game if game.resolve() != original_game else None
    try:
        run_script('prepare.py', game, '--out', out)
        run_script('link_libc.py', game, '--out', out)
        run_script('link_modules.py', game, '--out', out)
        run_script('content_profile.py', game, '--out', out, '--sku', env.get('BB_CONTENT_SKU', 'full'))

        # Patches exist for game version 01.09 only (patches.py applies none to others): other
        # versions keep the game's 30 FPS timing and change resolutions live.
        sys.path.insert(0, str(PORT / 'scripts'))
        from patches import (BLOODBORNE_IDS, eboot_matches, game_app_version, game_profile, game_title_id,
                             patch_requirements, selected_patches)
        version = game_app_version(game)
        profile = game_profile(game_title_id(game))
        needed = profile[2] if profile else '01.09'
        patched = (version in (None, needed) and eboot_matches(game, profile)) or bool(env.get('BB_FORCE_PATCHES'))
        if not patched:
            print(f'Game version {version}: community patches need {needed}; 30 FPS, no effect patches')
        bloodborne = not profile or profile[0] is BLOODBORNE_IDS
        if not bloodborne:
            # The window shows the game's icon (BMP: the format SDL loads without extra libraries).
            try:
                from PIL import Image
                with Image.open(game / 'sce_sys' / 'icon0.png') as icon:
                    icon.convert('RGBA').resize((64, 64), Image.LANCZOS).save(out / 'window_icon.bmp')
                env.setdefault('BB_WINDOW_ICON', str(out / 'window_icon.bmp'))
            except (ImportError, OSError):
                pass
            # The temporal upscalers and motion vectors read Bloodborne's scene constants; linear
            # image readbacks fix God of War III's corrupted textures.
            env.setdefault('BB_UPSCALER', 'none')
            env.setdefault('BB_READBACK_LINEAR', '1')
        if not bloodborne and patched:
            # Other games: the patch notes give the direct memory and VBlank rate they need.
            names = selected_patches(profile[1], needed, env.get('BB_PATCHES', ''), env.get('BB_PATCHES_ONLY') == '1')
            dmem, vblank = patch_requirements(profile[1], names, needed)
            if dmem:
                env.setdefault('BB_DMEM_MB', str(dmem))
            if vblank:
                env.setdefault('BB_VBLANK_HZ', str(vblank))
        # Sizes chosen below for the previous launch are recomputed after an in-game restart.
        if env.get('BB_AUTO_RENDER_RES') == '1':
            for key in ('BB_RENDER_RES', 'BB_OUTPUT_RES', 'BB_AUTO_RENDER_RES'):
                env.pop(key, None)
        # BB_FPS presets are Bloodborne patches; other games keep their own timing (60 Hz VBlank).
        fps = env.get('BB_FPS', 'uncap') if patched and bloodborne else '30'
        scaled_render = scaled_output = None
        # Output sizes and live resolution changes scale Bloodborne's own targets; other games set
        # their resolution with their patches.
        if bloodborne and not env.get('BB_RENDER_RES'):
            printed = run_script('patches.py', '--print-scaled', '--settings', config, capture=True).split()
            if len(printed) == 2:
                scaled_render, scaled_output = printed
        live = '0'
        if scaled_output:
            live = env.get('BB_LIVE_RES') or ini_value(config, 'live_resolution') or '0'
            if live == 'auto':
                caps = probe.parent / 'bb-gpu-capabilities.exe'
                result = subprocess.run([str(caps), '--live-resolution'], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                        text=True, creationflags=no_console())
                live = result.stdout.strip() if result.returncode == 0 else '0'
            live = '1' if live == '1' or not patched else '0'
        if live == '1':
            print(f'Output {scaled_output}: live resolution changes (live_resolution=0: startup patch)')
        elif scaled_output:
            env.update(BB_RENDER_RES=scaled_render, BB_OUTPUT_RES=scaled_output, BB_AUTO_RENDER_RES='1')
            env.setdefault('BB_DMEM_MB', '9152')
            print(f'Output {scaled_output}: scene {scaled_render}, direct memory {env["BB_DMEM_MB"]} MiB '
                  '(live_resolution=1: live changes)')
        run_script('patches.py', '--out', out, '--fps', fps, '--extra', env.get('BB_PATCHES', ''),
                   '--settings', config, '--game-dir', game, '--render-res', env.get('BB_RENDER_RES', ''),
                   '--output-res', env.get('BB_OUTPUT_RES', ''),
                   '--patches-dir', env.get('BB_PATCHES_DIR', data / 'patches'),
                   '--patches-config', env.get('BB_PATCHES_CONFIG', data / 'patches.json'))
        if not env.get('BB_VBLANK_HZ'):
            env['BB_VBLANK_HZ'] = {'uncap': '0', '90': '90'}.get(fps, '60')

        # The in-game restart starts this script again once this process is gone.
        # GPU caches (shaders, pipelines) beside the saves; read before main(), so set here.
        user_dir = Path(env.get('BB_USER_DIR', data / 'user')).resolve()
        user_dir.mkdir(parents=True, exist_ok=True)
        env.setdefault('BB_GPU_USER_DIR', str(user_dir))
        this = ['--run'] if getattr(sys, 'frozen', False) else [str(Path(__file__).resolve())]
        restart = [sys.executable, *this, '--after', str(os.getpid()), *args]
        env['BB_RESTART_COMMAND'] = subprocess.list2cmdline(restart)
        command = [str(probe), str(out / 'boot-linked.bin'), '--content-profile', str(out / 'content.bin'),
                   '--patches', str(out / 'patches.bin'), '--app0', str(game),
                   '--user', str(user_dir),
                   '--timeout', env.get('BB_TIMEOUT', '0'), *args]
        # No stdin: an inherited pipe (shells such as Git Bash) cost the game its console output.
        return subprocess.run(command, stdin=subprocess.DEVNULL, creationflags=no_console()).returncode
    finally:
        if mod_view:
            shutil.rmtree(mod_view, ignore_errors=True)


if __name__ == '__main__':
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        sys.exit(130)
