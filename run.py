#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Prepares the game image and starts gow3-probe (Windows, Linux and macOS; run.sh wraps it on
Linux and macOS).

Environment variables (GOW3_GAME_DIR, GOW3_DATA_DIR, GOW3_PATCHES, GOW3_MODS_*,
GOW3_USER_DIR, ...). Extra arguments go to gow3-probe.
The in-game "Apply and restart" runs this script again through GOW3_RESTART_COMMAND;
`--after PID` waits for the previous game process to end first (its GPU device and memory).
"""
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import time

WINDOWS = sys.platform == 'win32'
EXE_SUFFIX = '.exe' if WINDOWS else ''
if WINDOWS:
    import ctypes

PORT = Path(__file__).resolve().parent


def fail(message):
    print(message, file=sys.stderr)
    sys.exit(1)


def wait_for(pid):
    if not WINDOWS:
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return
            except PermissionError:
                pass
            time.sleep(0.1)
        return
    kernel32 = ctypes.windll.kernel32
    SYNCHRONIZE = 0x00100000
    handle = kernel32.OpenProcess(SYNCHRONIZE, False, pid)
    if handle:
        kernel32.WaitForSingleObject(handle, 30000)
        kernel32.CloseHandle(handle)


def no_console():
    """Started without a console (the launcher): console programs must not open their own."""
    if not WINDOWS:
        return 0
    return 0 if ctypes.windll.kernel32.GetConsoleWindow() else subprocess.CREATE_NO_WINDOW


# macOS runs the x86-64 loader through Rosetta 2, which a new Mac does not have yet.
ROSETTA_MESSAGE = ('The game needs Rosetta 2. Install it in Terminal with:\n'
                   '  softwareupdate --install-rosetta --agree-to-license')


def rosetta_missing(error):
    return sys.platform == 'darwin' and getattr(error, 'errno', None) == 86  # EBADARCH


def parse_cpu_features(output):
    """Instruction sets `gow3-probe --cpu-features` reports missing ([] when complete)."""
    for line in output.splitlines():
        if line.startswith('CPU: missing '):
            return [name.strip() for name in line[len('CPU: missing '):].split(',') if name.strip()]
    return []


def isa_rewrite_wanted(missing, env):
    """GOW3_ISA_REWRITE=1 forces the ISA rewrite table, 0 disables it; otherwise the CPU decides."""
    policy = env.get('GOW3_ISA_REWRITE')
    if policy in ('0', '1'):
        return policy == '1'
    return bool(missing)


def run_script(name, *args, capture=False, check=True):
    # Inside the packaged launcher executable (PyInstaller) the executable runs scripts itself.
    script = ['--script'] if getattr(sys, 'frozen', False) else []
    command = [sys.executable, *script, str(PORT / 'scripts' / name), *map(str, args)]
    env = {**os.environ, 'PYTHONIOENCODING': 'utf-8'}
    if capture:
        result = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, text=True,
                                encoding='utf-8', env=env, creationflags=no_console())
        if not check:
            return result.returncode, result.stdout
        if result.returncode:
            sys.exit(result.returncode)
        return result.stdout
    # stdin given: the output handles are passed explicitly (a windowed launcher child would
    # get none otherwise).
    result = subprocess.run(command, stdin=subprocess.DEVNULL, env=env, creationflags=no_console())
    if result.returncode:
        sys.exit(result.returncode)
    return None


def launch_prefix(env):
    """Linux integrations chosen in the launcher: MangoHud is its Vulkan layer (an environment
    variable), GameMode a command that starts the game."""
    if WINDOWS:
        return []
    if env.get('GOW3_MANGOHUD') == '1':
        env.setdefault('MANGOHUD', '1')
    if env.get('GOW3_GAMEMODE') == '1':
        if gamemode := shutil.which('gamemoderun'):
            return [gamemode]
        print('GameMode: gamemoderun not found, starting without it', flush=True)
    return []


def find_executable(name):
    """bin/ (packaged), else out/ (built with build.sh)."""
    for candidate in (PORT / 'bin' / name, PORT / 'out' / name):
        if candidate.is_file():
            return candidate
    return None


def glyph_mounts(env, probe, game, out):
    """(probe arguments, style) of the controller glyphs: patched copies of the WADs that hold the
    game's button icons, mounted over them (scripts/glyphs.py). Opt-in: unless GOW3_GLYPH_STYLE is
    auto, xbox or nintendo nothing is asked, drawn or mounted, and old copies are removed. The
    game folder is never written."""
    choice = env.get('GOW3_GLYPH_STYLE', 'off')
    if choice not in ('auto', 'xbox', 'nintendo'):
        shutil.rmtree(Path(out) / 'glyphs', ignore_errors=True)
        return [], None
    try:
        import glyphs
        pad = ''
        if choice == 'auto':
            pad = subprocess.run([str(probe), '--gamepad'], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                 text=True, timeout=15, creationflags=no_console()).stdout
        style, faces, name = glyphs.resolve(choice, pad)
        mounts = glyphs.build(game, out, style, faces, env.get('GOW3_PAD_MAP', ''))
    except Exception as error:  # noqa: BLE001  (icons only: whatever fails, the game still starts)
        print(f'Glyphs: {error}; the game keeps its own button icons', flush=True)
        return [], None
    if not mounts:
        print('Glyphs: no Xbox or Nintendo controller, the game keeps its own button icons' if style == 'playstation'
              else 'Glyphs: no button icons found in this game folder', flush=True)
        return [], None
    print(f'Glyphs: {style}{f" ({name})" if name else ""}, on copies in {Path(out) / "glyphs"}; game files untouched',
          flush=True)
    return [part for guest, path in mounts.items() for part in ('--mount', f'{guest}={path}')], style


def main():
    args = sys.argv[1:]
    if len(args) >= 2 and args[0] == '--after':
        wait_for(int(args[1]))
        args = args[2:]
    # As run.sh: relative paths (GOW3_GAME_DIR, GOW3_DATA_DIR, fsr4_shaders/) are from the port.
    os.chdir(PORT)
    env = os.environ
    data = Path(env.get('GOW3_DATA_DIR', PORT))
    out = data / 'out'
    out.mkdir(parents=True, exist_ok=True)
    env.setdefault('GOW3_CONFIG', str(data / 'gow3.ini'))
    # Values this script derived on the previous start ("Apply and restart" inherits them) are
    # derived again from the new options.
    for key in env.pop('GOW3_DERIVED', '').split(','):
        env.pop(key, None)
    derived = []
    sys.path.insert(0, str(PORT / 'scripts'))
    import graphics_options
    try:
        import discord_presence
    except Exception:
        discord_presence = None
    import mods
    import patches
    # The enabled mods' patch files: their "Resolution Patch - <size>" entries are render resolutions too
    # (the launcher and the in-game menu list them; GOW3_EXTRA_RESOLUTIONS tells the game).
    mod_patch_files = mods.enabled_patch_files(env.get('GOW3_MODS_DIR', data / 'mods'),
                                               env.get('GOW3_MODS_CONFIG', data / 'mods.json'),
                                               env.get('GOW3_MODS_ENABLED', '1') != '0')
    extra_resolutions = [s for s in patches.resolution_names(mod_patch_files) if s not in graphics_options.RESOLUTIONS]
    env['GOW3_EXTRA_RESOLUTIONS'] = ';'.join(extra_resolutions)
    derived.append('GOW3_EXTRA_RESOLUTIONS')
    graphics, reverted = graphics_options.prepare(env['GOW3_CONFIG'], out / 'graphics_state.json', extra_resolutions)
    if reverted:
        print('Graphics: the last restart never reached the game; previous options restored', flush=True)
    env.update(graphics_options.environment(graphics))
    if not env.get('GOW3_FSR411_DIR') and not (PORT / 'fsr4_411').is_dir() and (data / 'fsr4_411').is_dir():
        env['GOW3_FSR411_DIR'] = str(data / 'fsr4_411')

    probe = Path(env['GOW3_PROBE']) if env.get('GOW3_PROBE') else find_executable('gow3-probe' + EXE_SUFFIX)
    if not probe or not probe.is_file():
        fail(f'gow3-probe{EXE_SUFFIX} not found: build it with build.sh or use a packaged build.')
    # Development builds run from the MSYS2 tree: its CLANG64 DLLs (SDL3, FFmpeg, ...).
    if WINDOWS and not (probe.parent / 'SDL3.dll').is_file():
        clang64 = Path(env.get('MSYS2_ROOT', r'C:\msys64')) / 'clang64' / 'bin'
        if clang64.is_dir():
            env['PATH'] = f'{clang64}{os.pathsep}{env.get("PATH", "")}'
    # Instruction sets this CPU lacks (MOVBE, BMI1, LZCNT): the ISA rewrite covers them; exit 20
    # is a set nothing covers.
    try:
        features = subprocess.run([str(probe), '--cpu-features'], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                  text=True, creationflags=no_console())
    except OSError as error:  # the first start of the x86-64 loader
        if rosetta_missing(error):
            fail(ROSETTA_MESSAGE)
        raise
    if features.returncode == 20:
        print(features.stdout, end='', flush=True)
        sys.exit(20)
    missing = parse_cpu_features(features.stdout)

    game = Path(env.get('GOW3_GAME_DIR', PORT.parent / 'CUSA01623'))
    if not (game / 'eboot.bin').is_file():
        fail(f'No eboot.bin in {game} (set GOW3_GAME_DIR).')
    update_view = None
    if env.get('GOW3_PATCH_DIR'):
        import game_update
        try:
            update_view = game_update.create_view(game, env['GOW3_PATCH_DIR'], out)
        except (ValueError, OSError) as error:
            fail(str(error))
        game = update_view
        print(f'Game and update combined from {env["GOW3_PATCH_DIR"]}', flush=True)
    original_game = game.resolve()
    game = Path(run_script('mods.py', game, '--out', out,
                           '--mods-dir', env.get('GOW3_MODS_DIR', data / 'mods'),
                           '--config', env.get('GOW3_MODS_CONFIG', data / 'mods.json'),
                           '--enabled', env.get('GOW3_MODS_ENABLED', '1'), capture=True).strip())
    mod_view = game if game.resolve() != original_game else None
    try:
        run_script('prepare.py', game, '--out', out)
        run_script('link_libc.py', game, '--out', out)
        run_script('link_modules.py', game, '--out', out)
        # A failed rewrite prints its reason and the game starts without the table.
        isa_args = []
        if isa_rewrite_wanted(missing, env):
            code, output = run_script('isa_rewrite.py', out / 'boot-linked.bin', '--out', out,
                                      capture=True, check=False)
            print(output, end='', flush=True)
            if code == 0:
                isa_args = ['--isa-rewrite', str(out / 'isa-rewrite.bin')]
        run_script('content_profile.py', game, '--out', out, '--sku', env.get('GOW3_CONTENT_SKU', 'full'))

        from patches import game_profile_for, patch_requirements, patch_status, selected_patches
        profile = game_profile_for(game)
        engine_fps = 60
        # The window shows the game's icon (BMP: the format SDL loads without extra libraries); on
        # Linux the launcher's own icon (the AppImage ships the one chosen when it was built) and
        # an application id the desktop entry matches, so the shell shows the same icon.
        icon_file = game / 'sce_sys' / 'icon0.png'
        if not WINDOWS:
            env.setdefault('SDL_APP_ID', 'gow3')
            if (PORT / 'launcher' / 'gow3.png').is_file():
                icon_file = PORT / 'launcher' / 'gow3.png'
        try:
            from PIL import Image
            with Image.open(icon_file) as icon:
                icon.convert('RGBA').resize((64, 64), Image.LANCZOS).save(out / 'window_icon.bmp')
            env.setdefault('GOW3_WINDOW_ICON', str(out / 'window_icon.bmp'))
        except (ImportError, OSError):
            pass
        # The temporal upscalers and motion vectors are not calibrated for this game yet: they stay
        # off unless the launcher's experimental switch is on (then gow3.ini decides); linear
        # image readbacks fix its corrupted textures.
        if env.get('GOW3_EXPERIMENTAL_UPSCALER') != '1':
            env.setdefault('GOW3_UPSCALER', 'none')
        env.setdefault('GOW3_READBACK_LINEAR', '1')
        patch_xml = profile[1] if profile else None
        if profile and mod_patch_files:
            # The built-in patches plus the enabled mods' (the same file for the selection and the notes).
            patch_xml = patches.merge_patch_files(profile[1], mod_patch_files, out / 'patches-merged.xml')
        if profile:
            names = selected_patches(patch_xml, profile[2], env.get('GOW3_PATCHES', ''), env.get('GOW3_PATCHES_ONLY') == '1')
            # A mod's patches that are on by default (isEnabled) stay on when the launcher chose the list.
            names += [n for n in patches.default_enabled(mod_patch_files, profile[2]) if n not in names]
            # gow3.ini decides the resolution and frame rate patches (in-game menu, launcher).
            names = graphics_options.patch_names(names, graphics)
            names = graphics_options.restrict_to_available(names, patches.available_patches(patch_xml, profile[2]))
            env['GOW3_PATCHES'] = ';'.join(names)
            env['GOW3_PATCHES_ONLY'] = '1'
        (out / 'patch_status.json').unlink(missing_ok=True)
        run_script('patches.py', '--out', out, '--extra', env.get('GOW3_PATCHES', ''), '--game-dir', game,
                   '--patches-dir', env.get('GOW3_PATCHES_DIR', data / 'patches'),
                   '--patches-config', env.get('GOW3_PATCHES_CONFIG', data / 'patches.json'),
                   *[arg for f in mod_patch_files for arg in ('--mod-patches', f)])
        if profile:
            # Only the patches found in this eboot.bin count: a frame rate patch that was skipped
            # leaves the engine at 60 FPS, and the notes' memory and VBlank needs follow the same list.
            applied = (patch_status(out) or {}).get('applied', [])
            engine_fps = graphics_options.applied_engine_fps(applied)
            dmem, vblank = patch_requirements(patch_xml, applied, profile[2])
            for key, value in (('GOW3_DMEM_MB', dmem), ('GOW3_VBLANK_HZ', vblank)):
                if value and key not in env:
                    env[key] = str(value)
                    derived.append(key)
        if 'GOW3_VBLANK_HZ' not in env:
            env['GOW3_VBLANK_HZ'] = '60'
            derived.append('GOW3_VBLANK_HZ')
        # Without the frame rate patch the engine steps 1/60 s per frame: the present thread must
        # not flip faster (gow3_graphics.h PresentLimit), or the game speeds up.
        env['GOW3_ENGINE_FPS'] = str(engine_fps)
        derived.append('GOW3_ENGINE_FPS')
        print(f'Engine frame rate: {engine_fps} FPS'
              + ('' if engine_fps > 60 or graphics['engine_fps'] == '60' else
                 ' (the frame rate patch was not found in this eboot.bin)'), flush=True)
        glyph_args, glyph_style = glyph_mounts(env, probe, game, out)
        if glyph_style:  # the in-game menu names the buttons the same way
            env['GOW3_GLYPHS'] = glyph_style
            derived.append('GOW3_GLYPHS')
        env['GOW3_DERIVED'] = ','.join(derived)
        # The guest memory pool is committed in full at start: a resolution patch above 1080p
        # can need more than 16 GB of RAM, and the game then closes when the page file cannot grow.
        import guest_memory
        if warning := guest_memory.memory_warning(env.get('GOW3_DMEM_MB'), guest_memory.physical_memory_mb(),
                                                  env.get('GOW3_STREAM_MEMORY', 'cached')):
            print(warning, flush=True)

        # The in-game restart starts this script again once this process is gone.
        # GPU caches (shaders, pipelines) beside the saves; read before main(), so set here.
        user_dir = Path(env.get('GOW3_USER_DIR', data / 'user')).resolve()
        user_dir.mkdir(parents=True, exist_ok=True)
        env.setdefault('GOW3_GPU_USER_DIR', str(user_dir))
        from save_backup import backup
        try:
            if made := backup(user_dir):
                print(f'Saves backed up to {made}', flush=True)
        except OSError as error:
            print(f'Save backup failed, starting anyway: {error}', flush=True)
        this = ['--run'] if getattr(sys, 'frozen', False) else [str(Path(__file__).resolve())]
        # Windows: the probe starts the new launch and ends, so it waits for this process. Linux:
        # the probe replaces itself with the new launch while this process still waits for it.
        after = ['--after', str(os.getpid())] if WINDOWS else []
        restart = [sys.executable, *this, *after, *args]
        env['GOW3_RESTART_COMMAND'] = subprocess.list2cmdline(restart) if WINDOWS else shlex.join(restart)
        command = [*launch_prefix(env), str(probe), str(out / 'boot-linked.bin'), '--content-profile', str(out / 'content.bin'),
                   '--patches', str(out / 'patches.bin'), '--app0', str(game),
                   '--user', str(user_dir),
                   '--timeout', env.get('GOW3_TIMEOUT', '0'), *isa_args, *glyph_args, *args]
        # No stdin: an inherited pipe (shells such as Git Bash) cost the game its console output.
        try:
            process = subprocess.Popen(command, stdin=subprocess.DEVNULL, creationflags=no_console())
        except OSError as error:
            if rosetta_missing(error):
                fail(ROSETTA_MESSAGE)
            raise
        presence = None
        try:
            try:
                presence = discord_presence.Presence(pid=process.pid) if discord_presence else None
            except (OSError, RuntimeError):
                print('Discord presence unavailable; the game will continue', flush=True)
            return process.wait()
        finally:
            if presence:
                presence.close()
    finally:
        if mod_view:
            shutil.rmtree(mod_view, ignore_errors=True)
        if update_view:
            game_update.remove_view(update_view)


if __name__ == '__main__':
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        sys.exit(130)
