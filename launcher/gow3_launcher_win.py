#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""God of War III (gow3) launcher for Windows and Linux (Tkinter).

Every setting of the port in one window: the game folder and saves, gow3.ini (upscaler,
preset, output, effects), start-up options passed to run.py as environment variables
(frame rate, presentation, HDR, ...), mods, third-party patches and controls.
Launcher options live in %APPDATA%/gow3-launcher/settings.json (~/.config/gow3-launcher on Linux).

Frozen with PyInstaller (packaging/windows/package.sh) the same God of War III.exe also runs the
game without the window (`--play`), run.py (`--run`) and the preparation scripts (`--script`),
so a packaged port needs no Python installation.
"""
import contextlib
import glob
import json
import os
from pathlib import Path
import queue
import re
import shlex
import runpy
import shutil
import stat
import signal
import subprocess
import sys
import tempfile
import threading
import unicodedata
import time
import urllib.request
import webbrowser
import zipfile

WINDOWS = sys.platform == 'win32'
import ctypes  # GDI on Windows, fontconfig elsewhere
# Windows without a cursor of their own show the X root window's, which XWayland leaves blank.
ARROW = '' if WINDOWS else 'left_ptr'
if WINDOWS:
    # AMD hybrid laptops: this layer hides the NVIDIA GPU from Vulkan and the game stays black (#263).
    os.environ.setdefault('VK_LOADER_LAYERS_DISABLE', 'VK_LAYER_AMD_switchable_graphics')
FROZEN = getattr(sys, 'frozen', False)
PORT_DIR = Path(sys.executable).resolve().parent if FROZEN else Path(__file__).resolve().parent.parent
sys.path.insert(0, str(PORT_DIR / 'scripts'))
import graphics_options  # noqa: E402  (scripts/)
import save_backup  # noqa: E402  (scripts/)
import steam_shortcut  # noqa: E402  (scripts/)
from patches import game_profile, game_sfo  # noqa: E402  (scripts/)
from prepare import load_sfo  # noqa: E402  (scripts/)
DATA_DIR = Path(os.environ.get('GOW3_DATA_DIR', PORT_DIR))
CONFIG_DIR = (Path(os.environ.get('APPDATA', Path.home())) if WINDOWS else
              Path(os.environ.get('XDG_CONFIG_HOME') or Path.home() / '.config')) / 'gow3-launcher'
CONFIG_FILE = CONFIG_DIR / 'settings.json'
MOD_PRESETS = DATA_DIR / 'mods_presets.json'
APP_NAME = 'God of War III'
MAX_LOG_LINES = 6000
NO_WINDOW = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
# The game and everything it starts form one process group, ended together (GameJob).
NEW_GROUP = {} if WINDOWS else {'start_new_session': True}
UI_FONT, MONO_FONT, SERIF_FONT = 'Segoe UI', 'Consolas', 'Georgia'
# This build; GitHub release tags are windows-v<VERSION>.
VERSION = '2.4.0'
RELEASES_API = 'https://api.github.com/repos/FelipePrado0/gow3_pc/releases/latest'
RELEASES_PAGE = 'https://github.com/FelipePrado0/gow3_pc/releases/latest'
UPDATE_DIR = Path(tempfile.gettempdir()) / 'gow3-update'
# Never copied over an installation by an update (the package does not hold them either).
USER_FILES = ('user', 'out', 'mods', 'gow3.ini', 'mods.json', 'mods_presets.json', 'patches.json', 'last_run.log')
# The God of War face for titles, next to gow3.ico; main() registers it for this process only.
FONT_FILE = PORT_DIR / 'launcher' / 'gowfont.ttf'
FR_PRIVATE = 0x10


# ---------------------------------------------------------------------------------------------
# Command line roles of the frozen executable.

def attach_stdio():
    """A windowed executable starts without sys.stdout; inherited pipes or a console still exist."""
    if not WINDOWS:
        return
    import msvcrt
    for name, std in (('stdout', -11), ('stderr', -12)):
        if getattr(sys, name) is not None:
            continue
        stream = None
        handle = ctypes.windll.kernel32.GetStdHandle(std)
        if handle and handle != ctypes.c_void_p(-1).value:
            try:
                stream = open(msvcrt.open_osfhandle(handle, os.O_WRONLY), 'w', encoding='utf-8',
                              errors='replace', buffering=1)
            except OSError:
                stream = None
        setattr(sys, name, stream or open(os.devnull, 'w'))


def run_role(argv):
    """--run [args]: run.py; --script <file> [args]: a preparation script. Returns an exit code."""
    attach_stdio()
    for stream in (sys.stdout, sys.stderr):  # keep messages in order with the game's output
        try:
            stream.reconfigure(encoding='utf-8', line_buffering=True)
        except (AttributeError, ValueError):
            pass
    if argv[0] == '--script':
        path, sys.argv = argv[1], argv[1:]
    else:
        path = str(PORT_DIR / 'run.py')
        sys.argv = [path, *argv[1:]]
    try:
        runpy.run_path(path, run_name='__main__')
    except SystemExit as stop:
        return stop.code if isinstance(stop.code, int) else (0 if stop.code is None else 1)
    return 0


def run_command():
    """The command that starts run.py: this executable when frozen, else Python."""
    return [sys.executable, '--run'] if FROZEN else [sys.executable, str(PORT_DIR / 'run.py')]


def find_tool(name):
    """A port executable: bin/ in a packaged build, out/ in a source checkout (as run.py looks)."""
    return next((str(p) for p in (PORT_DIR / 'bin' / name, PORT_DIR / 'out' / name) if p.is_file()), None)


def parse_gpu_list(text):
    """One tuple per line of `gow3-gpu-capabilities --list`: index, name, type and, from the tools
    that print them, the game's multisampling default on that driver ('off' or 'on') and '1' on
    the GPU the game picks without GOW3_GPU_ID. Other lines are skipped."""
    rows = [line.split('\t') for line in text.splitlines()]
    return [tuple(row) for row in rows if len(row) >= 3 and row[0].isdigit()]


def chosen_gpu(gpus, gpu_id):
    """The row of parse_gpu_list the game will run on: the Graphics card setting when it is in the
    list, else the GPU the tool marks as the game's pick; None when neither is known."""
    chosen = [row for row in gpus if row[0] == str(gpu_id)]
    return (chosen or [row for row in gpus if len(row) > 4 and row[4] == '1'] or [None])[0]


def no_msaa_options(gpus, gpu_id):
    """NO_MSAA with Automatic naming the chosen card and the game's decision for it."""
    row = chosen_gpu(gpus, gpu_id)
    if row is None or len(row) < 4 or row[3] not in ('off', 'on'):
        return list(NO_MSAA)
    label = _('Automatic ({}: off)' if row[3] == 'off' else 'Automatic ({}: on)').format(row[1])
    return [('', (label,))] + NO_MSAA[1:]


def list_gpus():
    """The Vulkan GPUs in the order the game numbers them, or [] when the tool cannot tell."""
    tool = find_tool('gow3-gpu-capabilities' + ('.exe' if WINDOWS else ''))
    if not tool:
        return []
    try:
        done = subprocess.run([tool, '--list'], capture_output=True, text=True, timeout=5,
                              creationflags=NO_WINDOW)
    except (OSError, subprocess.SubprocessError):
        return []
    return parse_gpu_list(done.stdout) if done.returncode == 0 else []


# ---------------------------------------------------------------------------------------------
# Languages: every text is written in English with the Russian next to it; the other
# languages are in gow3_lang.py, keyed by the English text.

sys.path.insert(0, str(Path(__file__).resolve().parent))
import button_icons  # noqa: E402
import gow3_lang  # noqa: E402

LANG = 'en'
FONT_LOADED = False  # set by main() for the window; the command-line roles never need the font


WARN = '  ⚠'  # marks a risky choice; the text before it is translated as usual


def monitor_under_pointer(root):
    """(x, y, width, height) of the monitor holding the mouse pointer, from xrandr; zeros if unknown."""
    try:
        text = subprocess.run(['xrandr', '--query'], capture_output=True, text=True, timeout=3,
                              env=host_environment()).stdout
    except (OSError, subprocess.SubprocessError):
        return 0, 0, 0, 0
    pointer_x, pointer_y = root.winfo_pointerxy()
    monitors = [(x, y, width, height) for width, height, x, y in
                (tuple(map(int, match.groups())) for match in
                 re.finditer(r' connected (?:primary )?(\d+)x(\d+)\+(\d+)\+(\d+)', text))]
    for x, y, width, height in monitors:
        if x <= pointer_x < x + width and y <= pointer_y < y + height:
            return x, y, width, height
    return monitors[0] if monitors else (0, 0, 0, 0)


def pick_fonts(families):
    """Windows fonts first, then the DejaVu family that most Linux desktops ship."""
    global UI_FONT, MONO_FONT, SERIF_FONT
    available = set(families)
    pick = lambda names: next((name for name in names if name in available), names[-1])
    UI_FONT = pick(('Segoe UI', 'Noto Sans', 'DejaVu Sans'))
    MONO_FONT = pick(('Consolas', 'Noto Sans Mono', 'DejaVu Sans Mono'))
    SERIF_FONT = pick(('Georgia', 'Noto Serif', 'DejaVu Serif'))


def host_environment():
    """The environment of the user's session for programs outside the bundle (xdg-open)."""
    env = dict(os.environ)
    for name in ('LD_LIBRARY_PATH', 'PYTHONHOME', 'PYTHONPATH', 'TCL_LIBRARY', 'TK_LIBRARY'):
        env.pop(name, None)
    return env


def open_url(url):
    if WINDOWS:
        webbrowser.open(url)
    else:
        subprocess.Popen(['xdg-open', url], env=host_environment(), stdin=subprocess.DEVNULL,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def _(en, ru=None):
    if en.endswith(WARN):
        return _(en[:-len(WARN)], ru and ru.removesuffix(WARN)) + WARN
    if LANG == 'ru':
        return ru or en
    return gow3_lang.table(LANG).get(en) or en


def environment_language():
    for name in ('LC_ALL', 'LC_MESSAGES', 'LANG'):
        code = re.match(r'[a-z]{2}', os.environ.get(name, ''))
        if code:
            return code.group()
    return 'en'


def windows_language():
    if not WINDOWS:
        return environment_language()
    try:
        primary = ctypes.windll.kernel32.GetUserDefaultUILanguage() & 0x3ff
    except (AttributeError, OSError):
        return 'en'
    return gow3_lang.WINDOWS_LANGUAGES.get(primary, 'en')


def register_font(path):
    """Makes a font file available to this process (nothing else sees it): GDI on Windows, fontconfig (which Tk
    uses) elsewhere. False when the file is missing or the system refuses it: the launcher then keeps its
    standard fonts."""
    if not path.is_file():
        return False
    try:
        if WINDOWS:
            return bool(ctypes.windll.gdi32.AddFontResourceExW(str(path), FR_PRIVATE, 0))
        fontconfig = ctypes.CDLL('libfontconfig.so.1')
        fontconfig.FcConfigAppFontAddFile.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        fontconfig.FcConfigAppFontAddFile.restype = ctypes.c_int
        return bool(fontconfig.FcConfigAppFontAddFile(None, os.fsencode(path)))
    except (AttributeError, OSError):
        return False


# The God of War face has ASCII letters only: the Latin-script languages use it for headings with
# the accents stripped (heading_text); the other scripts keep the launcher's usual families.
LATIN_UI_LANGUAGES = ('en', 'es', 'pt', 'fr', 'de', 'it', 'pl', 'tr')
HEADING_LETTERS = {'ß': 'ss', 'ẞ': 'SS', 'œ': 'oe', 'Œ': 'OE', 'æ': 'ae', 'Æ': 'AE', 'ø': 'o', 'Ø': 'O',
                   'ł': 'l', 'Ł': 'L', 'ı': 'i', 'đ': 'd', 'Đ': 'D'}


def heading_face(every_language=False):
    """Whether headings are drawn with the God of War face: once its file is registered, in the
    Latin-script languages, or in every language for the game's name."""
    return FONT_LOADED and (every_language or LANG in LATIN_UI_LANGUAGES)


def heading_font(size, fallback=None, every_language=False):
    if heading_face(every_language):
        return ('GodOfWar', size)
    return (fallback or SERIF_FONT, size)


def plain_letter(c):
    """c without its accents when its base letter is ASCII (Á becomes A). The other scripts keep
    their letters whole: Й is not И, ゴ is not コ, a hangul syllable is not its jamo."""
    base = unicodedata.normalize('NFD', c)[0]
    return base if base.isascii() else c


def heading_text(text, every_language=False):
    """A heading's text for its font: in the God of War face the accents go (GRÁFICOS becomes
    GRAFICOS) and the letters it lacks become their plain pair (ß becomes ss)."""
    if not heading_face(every_language):
        return text
    text = ''.join(HEADING_LETTERS.get(c, c) for c in text)
    return ''.join(plain_letter(c) for c in text)


# ---------------------------------------------------------------------------------------------
# Settings. gow3.ini keys (the game reads them, the in-game menu edits them) and the
# launcher's own settings.json (passed to run.py as environment variables).

INI_FLAGS = {'cheat_double_jumps', 'cheat_very_hard', 'object_motion', 'show_fps', 'vsync', 'async_shaders', 'deferred_readback',
             'stale_readback'}
# The graphics keys are also in the in-game menu (Insert / R3+L2); run.py reads the startup ones.
INI_DEFAULTS = {'upscaler': 'fsr3', 'preset': '1',
                'object_motion': '1', 'show_fps': '1', 'output_res': '1920x1080',
                'live_resolution': 'auto', 'display_mode': 'windowed', 'vsync': '1', 'fps_limit': '0',
                'async_shaders': '1', 'deferred_readback': '1', 'stale_readback': '1',
                'render_resolution': 'native', 'engine_fps': '120',
                'frames_queued': '1', 'cheat_very_hard': '0', 'cheat_double_jumps': '0',
                'anisotropy': '0', 'vibration': '100', 'mouse_prompt_distance': '48', 'mouse_lock': '1',
                'mouse_aim_style': '0', 'mouse_aim_sensitivity': '100'}
DISPLAY_MODES = [('windowed', ('Windowed',)), ('borderless', ('Borderless',)), ('fullscreen', ('Fullscreen',))]
ANISOTROPY = [('0', ('Game default',)), ('2', ('2x',)), ('4', ('4x',)), ('8', ('8x',)), ('16', ('16x',))]
FPS_LIMITS = [('30', ('30',)), ('60', ('60',)), ('120', ('120',)), ('240', ('240',)), ('0', ('Unlimited',))]
# The sidebar's pages, top to bottom, and the methods that refresh a page's lists when it is shown.
PAGES = ('home', 'graphics', 'performance', 'game', 'controls', 'advanced')
PAGE_REFRESH = {'home': ('refresh_status',), 'game': ('refresh_lists', 'refresh_game_patches')}
APP_DEFAULTS = {'ui_language': 'en', 'game_dir': os.environ.get('GOW3_GAME_DIR', str(PORT_DIR.parent / 'CUSA01623') if WINDOWS else ''), 'update_dir': '', 'experimental_upscaler': False,
                'video_backend': 'auto', 'gamemode': False, 'mangohud': False, 'desktop_integration': True, 'user_dir': '',
                'mods_dir': '', 'mods_enabled': True, 'patches_dir': '', 'language': '1',
                'hdr': False, 'draw_pipe': '', 'readbacks': '', 'no_msaa': '', 'stream_memory': 'cached',
                'gpu_id': '', 'partial_height_views': True, 'background_warmup': True, 'parallel_warmup': True, 'pipeline_binary_cache': True,
                'perf_diag': False, 'frame_capture_key': False, 'frame_stats': False, 'gpu_profile': False,
                'vk_validation': False, 'close_on_play': False, 'discord_rpc': False,
                'check_updates': False, 'game_patches': {}, 'controls': {}, 'pad_style': 'playstation',
                'rage_one_input': True, 'stick_prompts': 'keyboard', 'controls_layout': 2,
                'glyph_style': 'off'}

# PS4 button, default key, default gamepad button (SDL names; src/runtime_pad.c). L2/R2 come
# from the analog triggers and the left touchpad click (photo mode) from L1 + the right click's
# button; neither is remapped on the gamepad.
CONTROLS = [('cross', 'Space', 'a'), ('circle', 'Left Shift', 'b'), ('square', 'E', 'x'),
            ('triangle', 'Q', 'y'), ('l1', '1', 'leftshoulder'), ('r1', '3', 'rightshoulder'),
            ('l2', 'R', None), ('r2', 'F', None), ('l3', 'Z', 'leftstick'),
            ('r3', 'C', 'rightstick'), ('options', 'Return', 'start'),
            ('touchpad_left', 'Tab', None), ('touchpad_right', 'Backspace', 'back'),
            ('up', 'I', 'dpup'), ('down', 'K', 'dpdown'), ('left', 'J', 'dpleft'), ('right', 'L', 'dpright')]
PS_BUTTON_NAMES = {'cross': 'Cross', 'circle': 'Circle', 'square': 'Square', 'triangle': 'Triangle',
                   'l1': 'L1', 'r1': 'R1', 'l2': 'L2', 'r2': 'R2', 'l3': 'L3', 'r3': 'R3',
                   'options': 'Options', 'touchpad_left': 'Touchpad left', 'touchpad_right': 'Touchpad right',
                   'up': 'D-pad up', 'down': 'D-pad down', 'left': 'D-pad left', 'right': 'D-pad right'}
# Gamepad buttons by SDL name, labelled as on a PlayStation or an Xbox controller.
PAD_STYLES = {
    'playstation': {'a': 'Cross', 'b': 'Circle', 'x': 'Square', 'y': 'Triangle', 'leftshoulder': 'L1',
                    'rightshoulder': 'R1', 'leftstick': 'L3', 'rightstick': 'R3', 'start': 'Options',
                    'back': 'Share / Create', 'touchpad': 'Touchpad click', 'guide': 'PS button',
                    'dpup': 'D-pad up', 'dpdown': 'D-pad down', 'dpleft': 'D-pad left',
                    'dpright': 'D-pad right', 'misc1': 'Mute button'},
    'xbox': {'a': 'A', 'b': 'B', 'x': 'X', 'y': 'Y', 'leftshoulder': 'LB', 'rightshoulder': 'RB',
             'leftstick': 'LS (left stick click)', 'rightstick': 'RS (right stick click)', 'start': 'Menu',
             'back': 'View', 'touchpad': 'Touchpad click', 'guide': 'Xbox button', 'dpup': 'D-pad up',
             'dpdown': 'D-pad down', 'dpleft': 'D-pad left', 'dpright': 'D-pad right', 'misc1': 'Share'},
}


# Keyboard & mouse: how the game's stick-rotation prompts (QTEs) are answered (runtime_pad.c).
STICK_PROMPTS = [('keyboard', ('Keyboard (native)',)), ('mouse', ('Adaptive mouse (experimental)',))]
# L2 aim with the mouse (Head of Helios, Bow of Apollo): gow3.ini mouse_aim_style.
AIM_STYLES = [('0', ('Relative',)), ('1', ('Absolute',))]
GLYPH_STYLES = [('off', ('Original (default)',)), ('auto', ('Match my controller',)), ('xbox', ('Xbox',)),
                ('nintendo', ('Nintendo',))]


# Keyboard capture on the Controls page: the Tk keysym of a pressed key and the SDL scancode
# name the game reads (key_source in runtime_pad.c). Letters, digits and F1-F24 map to themselves.
# Comma and = are saved as Comma and Equals: their own names separate GOW3_KEY_MAP entries. The
# numpad with Num Lock off reports its other names; SDL sees the same keypad keys.
TK_TO_SDL_KEYS = {
    'space': 'Space', 'Return': 'Return', 'Tab': 'Tab', 'BackSpace': 'Backspace', 'Delete': 'Delete',
    'Escape': 'Escape', 'Home': 'Home', 'End': 'End', 'Prior': 'PageUp', 'Next': 'PageDown',
    'Caps_Lock': 'CapsLock', 'Num_Lock': 'Numlock', 'Scroll_Lock': 'ScrollLock', 'Pause': 'Pause',
    'App': 'Application', 'Menu': 'Application',
    'Up': 'Up', 'Down': 'Down', 'Left': 'Left', 'Right': 'Right',
    'Shift_L': 'Left Shift', 'Shift_R': 'Right Shift', 'Control_L': 'Left Ctrl', 'Control_R': 'Right Ctrl',
    'Alt_L': 'Left Alt', 'Alt_R': 'Right Alt',
    'minus': '-', 'equal': 'Equals', 'comma': 'Comma', 'bracketleft': '[', 'bracketright': ']',
    'backslash': '\\', 'semicolon': ';', 'apostrophe': "'", 'period': '.', 'slash': '/', 'grave': '`',
    'KP_Enter': 'Keypad Enter', 'KP_Add': 'Keypad +', 'KP_Subtract': 'Keypad -',
    'KP_Multiply': 'Keypad *', 'KP_Divide': 'Keypad /', 'KP_Decimal': 'Keypad .',
    **{f'KP_{d}': f'Keypad {d}' for d in range(10)},
    'KP_Insert': 'Keypad 0', 'KP_End': 'Keypad 1', 'KP_Down': 'Keypad 2', 'KP_Next': 'Keypad 3',
    'KP_Left': 'Keypad 4', 'KP_Begin': 'Keypad 5', 'KP_Right': 'Keypad 6', 'KP_Home': 'Keypad 7',
    'KP_Up': 'Keypad 8', 'KP_Prior': 'Keypad 9', 'KP_Delete': 'Keypad .',
}
# Keys that never reach the game as a button: shown in the box when pressed.
RESERVED_KEYS = {'Insert': 'Insert opens the port\'s menu',
                 'Win_L': 'The Windows key opens the Start menu', 'Win_R': 'The Windows key opens the Start menu',
                 'Super_L': 'The Windows key opens the Start menu', 'Super_R': 'The Windows key opens the Start menu',
                 'Print': 'Windows keeps Print Screen'}
CAPTURE_SECONDS = 5  # a waiting key box gives up after this long

# Tk mouse button numbers while a key box waits: 1 left, 2 middle, 3 right; on Windows 4 and 5 are
# the side buttons (XButton1 back, XButton2 forward). The wheel arrives as <MouseWheel> deltas.
TK_TO_MOUSE = {1: 'Mouse Left', 2: 'Mouse Middle', 3: 'Mouse Right', 4: 'Mouse Back', 5: 'Mouse Forward'}


def mouse_name(button=None, wheel_delta=0):
    """Name of a mouse button (Tk number) or of a wheel turn (delta: up > 0), or None."""
    if wheel_delta:
        return 'Wheel Up' if wheel_delta > 0 else 'Wheel Down'
    return TK_TO_MOUSE.get(button)


def sdl_key_name(keysym):
    """SDL name of a Tk keysym, or None when the key cannot be bound."""
    if keysym in TK_TO_SDL_KEYS:
        return TK_TO_SDL_KEYS[keysym]
    if len(keysym) == 1 and keysym.isascii() and keysym.isalnum():
        return keysym.upper()
    if keysym[:1] == 'F' and keysym[1:].isdigit() and 1 <= int(keysym[1:]) <= 24:
        return keysym
    return None


def bind_key(bindings, button, name):
    """Gives `name` to `button` in {button: key name}; a button that had it takes the old key of
    `button`, so no key presses two buttons. Returns the button that swapped, or None."""
    old = bindings.get(button, '')
    other = next((b for b, k in bindings.items() if b != button and k.strip().lower() == name.lower()), None)
    bindings[button] = name
    if other:
        bindings[other] = old
    return other


# The keyboard & mouse table: keys and mouse inputs for the PS4 buttons plus actions of their own.
# Its defaults put the attacks and magic on the mouse, the shoulder buttons next to WASD and the
# weapons on 1-4 (clockwise from up, the order they are acquired); CONTROLS keeps the game's own
# keys (src/runtime_pad.c), so only what differs from them is sent.
KEY_DEFAULTS = {'cross': 'Space', 'circle': 'Left Shift', 'square': 'Mouse Left', 'triangle': 'Mouse Right',
                'l1': 'Q', 'r1': 'E', 'l2': 'R', 'r2': 'Mouse Forward', 'l3': 'Z', 'r3': 'X',
                'options': 'Tab', 'touchpad_left': 'P', 'touchpad_right': 'Escape',
                'up': '1', 'right': '2', 'down': '3', 'left': '4'}
# Actions of the keyboard map only (runtime_pad.c's host-only sources), as CONTROLS rows. Rage of
# Sparta (L3 + R3 on one input) follows R3; it is used instead of L3 and R3 while 'rage_one_input'
# is on. "None" leaves a row without an input.
RAGE_ROW, GRAB_ROW, DODGE_ROW = ('rage', 'Mouse Middle', None), ('grab', 'C', None), ('dodge', 'Mouse Back', None)
KEY_ACTIONS = [RAGE_ROW, GRAB_ROW, DODGE_ROW]
ACTION_NAMES = {'rage': 'Rage of Sparta', 'grab': 'Grab', 'dodge': 'Dodge'}
RAGE_BUTTONS = ('l3', 'r3')
CONTROLS_LAYOUT = 2  # settings layout: 2 = one keyboard & mouse table (migrate_controls)


def table_rows(kind):
    """CONTROLS rows of a table, in display order; the keyboard one adds its actions."""
    if kind != 'key':
        return CONTROLS
    rows = []
    for row in CONTROLS:
        rows.append(row)
        if row[0] == 'r3':
            rows.append(RAGE_ROW)
    return rows + [GRAB_ROW, DODGE_ROW]
MOUSE_NAMES = {name.casefold() for name in (*TK_TO_MOUSE.values(), 'Wheel Up', 'Wheel Down')}


def is_mouse_name(name):
    return name.strip().casefold() in MOUSE_NAMES


def default_input(row, kind):
    """Default input of a table row for 'key' or 'pad' ('' or None when it has none)."""
    button, key, pad = row
    if kind == 'pad':
        return pad
    return KEY_DEFAULTS.get(button, key) if key else key


def keyboard_map(controls, rage=True):
    """GOW3_KEY_MAP of the keyboard & mouse table: each button on its chosen input, written where
    that differs from the game's own key. With `rage` (one input for Rage of Sparta) L3 and R3 get
    none and the Rage row is used; otherwise the Rage row is left out."""
    chosen = control_values(controls, 'key')
    entries = []
    for row in table_rows('key'):
        button, key, _pad = row
        if not key or (button == 'rage' and not rage):
            continue
        if rage and button in RAGE_BUTTONS:
            entries.append(f'{button}=None')
            continue
        value = (chosen.get(button) or default_input(row, 'key')).strip()
        game_default = key if row in CONTROLS else ''  # the game has no key of its own for an action
        if value.casefold() != game_default.casefold() and not set(value) & set(',='):
            entries.append(f'{button}={value}')
    return ','.join(entries)


def control_changes(values, kind):
    """The buttons whose chosen key or SDL gamepad name differs from the default."""
    return {row[0]: values[row[0]].strip() for row in table_rows(kind)
            if default_input(row, kind) and values.get(row[0], '').strip()
            and values[row[0]].strip().casefold() != default_input(row, kind).casefold()}


def control_values(controls, kind):
    """The saved inputs ('key') or gamepad names ('pad') by game button, for the rows of that
    table. 'touchpad', saved before the sides were split, is the left click."""
    chosen = controls.get(kind, {}) if isinstance(controls, dict) else {}
    if not isinstance(chosen, dict):
        return {}
    values = {button: value for button, value in chosen.items() if button != 'touchpad'}
    if 'touchpad' in chosen and 'touchpad_left' not in values:
        values['touchpad_left'] = chosen['touchpad']
    return {button: values[button] for button, key, pad in table_rows(kind)
            if (pad if kind == 'pad' else key) and isinstance(values.get(button), str) and values[button].strip()}


def control_map(controls, kind):
    """GOW3_PAD_MAP value: 'cross=a,circle=b' for the changed buttons."""
    chosen = control_values(controls, kind)
    return ','.join(f'{button}={chosen[button].strip()}' for button, _key, _pad in CONTROLS
                    if button in chosen and not set(chosen[button]) & set(',='))


def migrate_controls(saved):
    """Settings saved before the keyboard & mouse table (CONTROLS_LAYOUT). A player who saved keys
    in Keyboard Mapping keeps the whole layout they had (the game's keys for the rest); one who
    used Native Translation keeps that table. A new action whose default input is already taken
    stays without one."""
    if not isinstance(saved, dict) or not saved or saved.get('controls_layout') == CONTROLS_LAYOUT:
        return saved
    saved = dict(saved)
    controls = dict(saved['controls']) if isinstance(saved.get('controls'), dict) else {}
    native = controls.pop('native', None)
    if saved.pop('native_input', False) and isinstance(native, dict):
        controls['key'] = dict(native)  # its defaults are the new ones
    elif controls.get('key'):
        kept = control_values(controls, 'key')
        layout = {button: kept.get(button) or key for button, key, _pad in CONTROLS if key}
        rage = saved.get('native_rage', True)
        taken = {value.strip().casefold() for button, value in layout.items()
                 if not (rage and button in RAGE_BUTTONS)}
        for button, default, _pad in KEY_ACTIONS:
            layout[button] = 'None' if default.casefold() in taken else default
        controls['key'] = layout
    if 'native_rage' in saved:
        saved['rage_one_input'] = saved.pop('native_rage')
    saved['controls'] = controls
    saved['controls_layout'] = CONTROLS_LAYOUT
    return saved

UPSCALERS = [('fsr3', ('FSR 3.1 (every GPU)', 'FSR 3.1 (любая видеокарта)')),
             ('taa', ('TAA (native resolution anti-aliasing)', 'TAA (нативное сглаживание)')),
             ('off', ('Off', 'Выключен'))]
PRESETS = [('0', ('Native AA (×1.0)',)), ('1', ('Quality (×1.5)',)), ('2', ('Balanced (×1.7)',)),
           ('3', ('Performance (×2)',)), ('4', ('Ultra Performance (×3)',))]
OUTPUTS = [('1280x720', ('1280 × 720 (Steam Deck)',)), ('1920x1080', ('1920 × 1080',)),
           ('2560x1440', ('2560 × 1440',)), ('3840x2160', ('3840 × 2160 (4K)',))]
LIVE = [('auto', ('Auto (by graphics card)', 'Авто (по видеокарте)')), ('0', ('Off (faster)', 'Выключена (быстрее)')),
        ('1', ('On (change without restarting)', 'Включена (без перезапуска)'))]
LANGUAGES = [('1', ('English', 'Английский')), ('8', ('Russian', 'Русский')), ('0', ('Japanese', 'Японский')),
             ('2', ('French', 'Французский')), ('3', ('Spanish', 'Испанский')), ('4', ('German', 'Немецкий')),
             ('5', ('Italian', 'Итальянский'))]
DRAW_PIPE = [('', ('Auto (8+ threads)', 'Авто (8+ потоков)')), ('1', ('On', 'Включён')),
             ('0', ('Off (more stable)', 'Выключен (стабильнее)'))]
READBACKS = [('', ('Relaxed (default)', 'Relaxed (по умолчанию)')), ('0', ('Off', 'Выключены')),
             ('2', ('Precise',))]
# GOW3_NO_MSAA: '' leaves the game's own default, off on AMD's and Intel's Windows drivers
# (video_core/amdgpu/gow3_msaa_default.h); no_msaa_options names the chosen card in Automatic.
NO_MSAA = [('', ('Automatic',)), ('1', ('Off (fixes black lines)',)), ('0', ('On',))]
NO_MSAA_HINT = ("The game's 2-sample depth buffer, read as a texture by its lighting. AMD's and Intel's Windows "
                'drivers read it wrong and the lighting shows black lines, so Automatic turns multisampling off '
                'on those cards (Radeon, Arc) and keeps it on elsewhere (NVIDIA). Off and On force the choice on '
                'any card.')
STREAM_MEMORY = [('cached', ('System RAM (cached, default)',)),
                 ('host', ('System RAM (uncached)',)),
                 ('device', ('GPU memory (previous behavior)',))]
FRAMES_QUEUED = [('1', ('1 (lowest latency)',)), ('2', ('2',)), ('0', ('Unlimited',))]
UI_LANGUAGES = gow3_lang.LANGUAGE_NAMES

def fsr4_folder():
    """Next to the port; in the data directory when the port is read-only (AppImage)."""
    if WINDOWS or os.access(PORT_DIR, os.W_OK):
        return PORT_DIR / 'fsr4_shaders'
    return DATA_DIR / 'fsr4_shaders'


def load_json(path, default):
    try:
        return json.loads(Path(path).read_text(encoding='utf-8'))
    except (OSError, ValueError):
        return default


def ini_path():
    return Path(os.environ.get('GOW3_CONFIG', DATA_DIR / 'gow3.ini'))


def load_ini():
    values, lines = dict(INI_DEFAULTS), []
    try:
        lines = ini_path().read_text(encoding='utf-8').splitlines()
    except OSError:
        pass
    for line in lines:
        if '=' in line and not line.lstrip().startswith('#'):
            key, value = line.split('=', 1)
            values[key.strip()] = value.strip()
    return values, lines


def atomic_write(path, text):
    """Replace a complete file from its own directory, preserving the old file on write failure."""
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', dir=path.parent,
                                         suffix='.tmp', delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def save_ini(values, lines):
    """Rewrites the edited keys in place, appends missing ones, keeps comments and other keys."""
    written, out = set(), []
    for line in lines:
        if '=' in line and not line.lstrip().startswith('#'):
            key = line.split('=', 1)[0].strip()
            if key in values:
                out.append(f'{key}={values[key]}')
                written.add(key)
                continue
        out.append(line)
    if not lines:
        out.append('# gow3 settings (in-game menu: Insert / R3+L2)')
    out += [f'{key}={value}' for key, value in values.items() if key not in written]
    atomic_write(ini_path(), '\n'.join(out) + '\n')


def game_info(folder):
    """(title, app version) of a game folder, or None without eboot.bin."""
    folder = Path(folder or '.')
    if not (folder / 'eboot.bin').is_file():
        return None
    values = game_sfo(folder)
    return values.get('TITLE', APP_NAME).replace('™', '').replace('®', '').strip(), values.get('APP_VER', '?')


def meta_folder(game_dir, update_dir):
    """The folder the title, version and patches are read from: the update when there is one."""
    if update_dir and (Path(update_dir) / 'eboot.bin').is_file():
        return update_dir
    return game_dir


VIDEO_BACKENDS = [('auto', ('Automatic',)), ('wayland', ('Wayland',)), ('x11', ('X11 (XWayland)',))]
VULKAN_LAYER_DIRS = ('/usr/share/vulkan/implicit_layer.d', '/etc/vulkan/implicit_layer.d',
                     os.path.expanduser('~/.local/share/vulkan/implicit_layer.d'))


def linux_tools():
    """Which of the optional Linux integrations this system has."""
    return {'wayland': bool(os.environ.get('WAYLAND_DISPLAY')),
            'gamemode': bool(shutil.which('gamemoderun')),
            'mangohud': bool(shutil.which('mangohud')) or any(
                glob.glob(os.path.join(d, '*angoHud*.json')) for d in VULKAN_LAYER_DIRS)}


def file_state(path):
    """'ok', 'missing', 'folder', 'empty' or 'unreadable' for a file the port needs, from one
    stat: an empty one (a copy that failed, a placeholder made by hand) is as good as missing."""
    try:
        status = Path(path).stat()
    except (FileNotFoundError, NotADirectoryError):
        return 'missing'
    except OSError:
        return 'unreadable'
    if stat.S_ISDIR(status.st_mode):
        return 'folder'
    if not stat.S_ISREG(status.st_mode):
        return 'missing'
    if status.st_size == 0:
        return 'empty'
    try:
        with Path(path).open('rb') as stream:
            stream.read(1)
    except OSError:
        return 'unreadable'
    return 'ok'


def usable(path):
    return file_state(path) == 'ok'


def sfo_state(path):
    """file_state for a param.sfo, which must also parse: 'invalid' when it does not."""
    state = file_state(path)
    if state != 'ok':
        return state
    try:
        load_sfo(path)
    except OSError:
        return 'unreadable'
    except ValueError:
        return 'invalid'
    return 'ok'


# The files the port loads from a dump, each with the check it must pass.
REQUIRED_GAME_FILES = (('eboot.bin', file_state), ('sce_sys/param.sfo', sfo_state),
                       ('sce_module/libc.prx', file_state), ('sce_module/libSceFios2.prx', file_state))
REQUIRED_UPDATE_FILES = REQUIRED_GAME_FILES[:2]


def missing_game_files(folder, requirements=REQUIRED_GAME_FILES):
    """[(name, state)] for the required files that do not pass their check in the folder: an
    incomplete dump, a placeholder made by hand, a file this user may not read, a param.sfo
    that is not one."""
    folder = Path(folder or '.')
    return [(name, state) for name, check in requirements if (state := check(folder / name)) != 'ok']


def describe_missing(missing):
    """The entries of missing_game_files as shown to the player."""
    labels = {'empty': _('{} (empty)'), 'unreadable': _('{} (unreadable)'),
              'invalid': _('{} (not a param.sfo)'), 'folder': _('{} (a folder, not a file)')}
    return [labels[state].format(name) if state in labels else name for name, state in missing]


def ready_to_play(game_dir, update_dir=''):
    """Whether --play (Steam, "Play God of War III.exe") may start the game at once; anything
    the Play button would refuse opens the launcher window instead."""
    if update_dir and same_folder(update_dir, game_dir or '.'):
        return False
    return not (missing_game_files(game_dir)
                or (update_dir and missing_game_files(update_dir, REQUIRED_UPDATE_FILES)))


def same_folder(a, b):
    """Whether two paths name one folder (links and 8.3 spellings included)."""
    return os.path.normcase(os.path.realpath(a)) == os.path.normcase(os.path.realpath(b))


def system_files_beside(folder, reference=None, base=None):
    """PKG extractors write the system files (param.sfo, icon0.png, pic1.png, trophy/) to Sc0/
    beside the game files (Image0/), while the port reads them from the folder's sce_sys/: the
    Sc0 folder beside `folder` holding a God of War III param.sfo (patches.game_profile), while
    `folder` has none worth keeping (an unreadable one is a different problem). For an update
    folder the title must be that of `reference` (the base game's param.sfo values); an Sc0
    beside the base game itself (`base`) is the base game's own unless the base game's param.sfo
    says another version."""
    folder = Path(os.path.abspath(folder or '.'))
    if sfo_state(folder / 'sce_sys/param.sfo') in ('ok', 'unreadable') or folder.parent == folder:
        return None
    try:
        with os.scandir(folder.parent) as entries:  # a linked Sc0 (mklink /J) counts too
            names = [e.name for e in entries if e.is_dir() and e.name.casefold() == 'sc0']
    except OSError:
        return None
    if len(names) != 1 or os.path.normcase(names[0]) == os.path.normcase(folder.name):
        return None
    sibling = folder.parent / names[0]
    try:
        values = load_sfo(sibling / 'param.sfo')
    except (OSError, ValueError):
        return None
    title = values.get('TITLE')
    if not game_profile(values.get('TITLE_ID'), title if isinstance(title, str) else None):
        return None
    if reference is not None and values.get('TITLE_ID') != reference.get('TITLE_ID'):
        return None
    if base is not None and same_folder(folder.parent, Path(os.path.abspath(base)).parent) and (
            reference is None or values.get('APP_VER') == reference.get('APP_VER')):
        return None
    return sibling


@contextlib.contextmanager
def replacing(destination):
    """A .part path to write `destination` through: the file takes its place whole on success
    and is removed on failure, so a write cut short never passes for the file."""
    part = destination.with_name(destination.name + '.part')
    try:
        yield part
        os.replace(part, destination)
    except BaseException:
        part.unlink(missing_ok=True)
        raise


def adopt_system_files(source, game):
    """Copies the files of `source` (an Sc0/ folder) into <game>/sce_sys, keeping every usable
    file the game already has there (empty ones, and a param.sfo that is not one, are replaced);
    returns the relative names copied. Each file lands whole or not at all, and param.sfo goes
    last: while it is missing, a copy that failed halfway is offered again."""
    source, target = Path(source), Path(game) / 'sce_sys'

    def unreadable(error):  # os.walk would skip the folder silently
        raise error
    files = sorted(((Path(directory) / name).relative_to(source)
                    for directory, _folders, names in os.walk(source, onerror=unreadable) for name in names),
                   key=lambda relative: (relative.as_posix() == 'param.sfo', relative.as_posix()))
    copied = []
    for relative in files:
        destination = target / relative
        if destination.is_dir():
            raise OSError(f'{destination} is a folder: {source / relative} cannot replace it')
        check = sfo_state if relative.as_posix() == 'param.sfo' else file_state
        if check(destination) == 'ok':
            continue
        destination.parent.mkdir(parents=True, exist_ok=True)
        with replacing(destination) as part:
            shutil.copy2(source / relative, part)
        copied.append(relative.as_posix())
    return copied


DESKTOP_MARKER = 'X-GOW3-Integration=true'


def desktop_entry(command, icon, marker=False):
    """The menu entry; its StartupWMClass is the application id of the launcher and of the game window."""
    return ('[Desktop Entry]\nType=Application\nName=God of War III Remastered\n'
            f'Exec={command}\nIcon={icon}\nTerminal=false\nCategories=Game;\nStartupWMClass=gow3\n'
            + (DESKTOP_MARKER + '\n' if marker else ''))


def other_desktop_entries(applications, appimage):
    """Menu entries, other than ours, that start this very AppImage: another tool (Gear Lever,
    AppImageLauncher, appimaged) has integrated it. The program is the first word of Exec or
    TryExec, after any `env NAME=value` prefix and the quotes."""
    here = os.path.realpath(appimage)
    found = []
    try:
        entries = sorted(Path(applications).glob('*.desktop'))
    except OSError:
        return found
    for entry in entries:
        if entry.name == 'gow3.desktop':
            continue
        try:
            lines = entry.read_text(encoding='utf-8', errors='replace').splitlines()
        except OSError:
            continue
        for line in lines:
            key, _, value = line.partition('=')
            if key not in ('Exec', 'TryExec'):
                continue
            try:
                words = shlex.split(value)
                while words and (words[0] == 'env' or re.match(r'[A-Za-z_][A-Za-z0-9_]*=', words[0])):
                    words.pop(0)
                # A file that another program left broken (a NUL byte in a path) is ignored, not fatal.
                matches = bool(words) and os.path.realpath(words[0]) == here
            except (OSError, ValueError):
                continue
            if matches:
                found.append(entry)
                break
    return found


def integrated_by_other_tool(applications, appimage, environment=None):
    """True when another tool (Gear Lever sets DESKTOPINTEGRATION, AppImageLauncher and appimaged leave an
    entry) already put this AppImage in the menu: an entry of ours would be a duplicate."""
    return bool((environment if environment is not None else os.environ).get('DESKTOPINTEGRATION')
                or (appimage and other_desktop_entries(applications, appimage)))


def integrate_desktop(enabled, appimage, icon_source, data_dir, home=None, environment=None):
    """AppImage on Linux: the menu entry the desktop matches the windows (application id gow3) to,
    which is what shows their icon in the dock. The entry is ours while it carries the marker; one
    without it was written by hand (the Desktop shortcut button) and is left alone. When another
    tool already integrated this AppImage (an entry that starts it, or DESKTOPINTEGRATION in the
    environment, which Gear Lever sets), ours would be a duplicate: it is not made, and removed."""
    applications = Path(home or Path.home()) / '.local' / 'share' / 'applications'
    entry = applications / 'gow3.desktop'
    try:
        ours = entry.is_file() and DESKTOP_MARKER in entry.read_text(encoding='utf-8', errors='replace')
        if not enabled:
            if ours:
                entry.unlink()
            return None
        if not appimage or (entry.is_file() and not ours):
            return None
        if integrated_by_other_tool(applications, appimage, environment):
            if ours:
                entry.unlink()
            return None
        icon = Path(data_dir) / 'gow3.png'
        icon.parent.mkdir(parents=True, exist_ok=True)
        if not icon.is_file() or icon.read_bytes() != Path(icon_source).read_bytes():
            shutil.copyfile(icon_source, icon)
        text = desktop_entry(f'"{appimage}"', icon, marker=True)
        entry.parent.mkdir(parents=True, exist_ok=True)
        if not entry.is_file() or entry.read_text(encoding='utf-8') != text:
            entry.write_text(text, encoding='utf-8')
        return entry
    except (OSError, ValueError):  # runs before the window opens: it must never stop the launcher
        return None


def write_linux_shortcut(command, appimage, icon_source, desktop, home=None, environment=None):
    """The shortcut button: the desktop entry on the desktop folder (when it exists) and in the applications
    menu, except in the menu when another tool already integrated this AppImage (a second entry would
    show the game twice). Returns the files written."""
    home = Path(home or Path.home())
    icon_dir = home / '.local' / 'share' / 'icons'
    icon = icon_dir / 'gow3.png'
    try:
        icon_dir.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(icon_source, icon)
    except OSError:
        icon = 'gow3'
    entry = desktop_entry(command, icon)
    applications = home / '.local' / 'share' / 'applications'
    folders = [] if integrated_by_other_tool(applications, appimage, environment) else [applications]
    folders.append(Path(desktop))
    created = []
    for folder in folders:
        if not folder.is_dir() and folder != folders[0]:  # the first one is the point of the click
            continue
        folder.mkdir(parents=True, exist_ok=True)
        # In the applications menu the file is named after the window's application id.
        target = folder / ('gow3.desktop' if folder == applications else 'God of War III.desktop')
        target.write_text(entry, encoding='utf-8')
        target.chmod(0o755)
        created.append(str(target))
    return created


def steam_icon(game):
    """Picture of the Steam shortcut's icon: the launcher's own on Linux, the game's on Windows."""
    return Path(game) / 'sce_sys' / 'icon0.png' if WINDOWS else PORT_DIR / 'launcher' / 'gow3.png'


def steam_launch_command():
    """(program, launch options, start folder) with which Steam starts the game without the window."""
    appimage = os.environ.get('APPIMAGE')
    if appimage:
        return appimage, '--play', str(Path(appimage).parent)
    if FROZEN:
        return sys.executable, '--play', str(Path(sys.executable).parent)
    return sys.executable, f'"{Path(__file__).resolve()}" --play', str(PORT_DIR)


def game_profile_of(folder):
    """(title id, (ids, built-in XML, app version)) of a game with a patch profile, else (id, None)."""
    from patches import game_profile_for, game_title_id
    title_id = game_title_id(folder) if folder else None
    return title_id, game_profile_for(folder)


def game_environment(s):
    env = dict(os.environ)
    env['GOW3_GAME_DIR'] = s['game_dir']
    env.pop('GOW3_PATCH_DIR', None)
    # Temporal upscaling is parked (build_graphics): a switch saved on by 2.1.8 has no control left to
    # turn it off, so it is ignored until the upscaler section comes back.
    env['GOW3_EXPERIMENTAL_UPSCALER'] = '0'
    if s.get('update_dir'):
        env['GOW3_PATCH_DIR'] = s['update_dir']
    if not WINDOWS:
        backend = s.get('video_backend', 'auto')
        if backend in ('wayland', 'x11'):
            env['SDL_VIDEO_DRIVER'] = env['SDL_VIDEODRIVER'] = backend
        env['GOW3_GAMEMODE'] = '1' if s.get('gamemode') else '0'
        env['GOW3_MANGOHUD'] = '1' if s.get('mangohud') else '0'
    # The launcher's patch selection is the whole list (run.py, patches.py).
    title_id, profile = game_profile_of(meta_folder(s['game_dir'], s.get('update_dir')))
    chosen = s.get('game_patches', {}).get(title_id) if title_id else None
    if profile and chosen is not None:
        env['GOW3_PATCHES'] = ';'.join(chosen)
        env['GOW3_PATCHES_ONLY'] = '1'
    if s['user_dir']:
        env['GOW3_USER_DIR'] = s['user_dir']
    if not WINDOWS:
        env['GOW3_FSR4_DIR'] = str(fsr4_folder())
    env['GOW3_MODS_DIR'] = s['mods_dir'] or str(DATA_DIR / 'mods')
    env['GOW3_MODS_CONFIG'] = str(DATA_DIR / 'mods.json')
    env['GOW3_MODS_ENABLED'] = '1' if s['mods_enabled'] else '0'
    env['GOW3_PATCHES_DIR'] = s['patches_dir'] or str(DATA_DIR / 'patches')
    env['GOW3_PATCHES_CONFIG'] = str(DATA_DIR / 'patches.json')
    env['GOW3_LANGUAGE'] = s['language']
    env['GOW3_DISCORD'] = '1' if s.get('discord_rpc') else '0'
    if s['hdr']:
        env['GOW3_HDR'] = '1'
    for key, name in (('draw_pipe', 'GOW3_DRAW_PIPE'), ('readbacks', 'GOW3_READBACKS'), ('no_msaa', 'GOW3_NO_MSAA')):
        if s.get(key):
            env[name] = s[key]
    # The saved UI choice wins over inherited diagnostic overrides, including when reverting.
    stream_memory = s.get('stream_memory', APP_DEFAULTS['stream_memory'])
    env['GOW3_STREAM_MEMORY'] = stream_memory if stream_memory in ('cached', 'host', 'device') else 'cached'
    gpu_id = str(s.get('gpu_id', ''))
    env.pop('GOW3_GPU_ID', None)
    if gpu_id.isdigit():
        env['GOW3_GPU_ID'] = gpu_id
    env['GOW3_BACKGROUND_WARMUP'] = '1' if s.get('background_warmup', True) else '0'
    env['GOW3_PARALLEL_WARMUP'] = '1' if s.get('parallel_warmup', True) else '0'
    env['GOW3_PIPELINE_BINARIES'] = '1' if s.get('pipeline_binary_cache', True) else '0'
    env['GOW3_PERF_DIAG'] = '1' if s.get('perf_diag', False) else '0'
    env['GOW3_PARTIAL_HEIGHT_VIEWS'] = '1' if s.get('partial_height_views', True) else '0'
    for key, name in (('frame_stats', 'GOW3_FRAME_STATS'), ('gpu_profile', 'GOW3_GPU_PROFILE'),
                      ('vk_validation', 'GOW3_VK_VALIDATION'), ('frame_capture_key', 'GOW3_FRAME_CAPTURE_KEY')):
        if s[key]:
            env[name] = '1'
    controls = s.get('controls', {})
    if value := keyboard_map(controls, bool(s.get('rage_one_input', True))):
        env['GOW3_KEY_MAP'] = value
    if value := control_map(controls, 'pad'):
        env['GOW3_PAD_MAP'] = value
    env.pop('GOW3_STICK_PROMPTS', None)
    if s.get('stick_prompts') in dict(STICK_PROMPTS):
        env['GOW3_STICK_PROMPTS'] = s['stick_prompts']
    glyph_style = s.get('glyph_style')
    env.pop('GOW3_GLYPH_STYLE', None)
    if glyph_style in ('auto', 'xbox', 'nintendo'):
        env['GOW3_GLYPH_STYLE'] = glyph_style
    env['PYTHONUNBUFFERED'] = '1'
    env['PYTHONIOENCODING'] = 'utf-8'
    return env


# ---------------------------------------------------------------------------------------------
# The window.

# God of War III's own colors: near black, ash and blood red (no gold or orange).
BG, PANEL, CARD, LINE = '#0d0b0b', '#161212', '#1d1717', '#2a2222'
TEXT, MUTED, ACCENT, ACCENT_HI = '#e6e0d8', '#8a817a', '#a4161a', '#c81e25'
def pad_labels(style):
    """SDL gamepad button name -> its label in this naming style, in the launcher's language."""
    return {name: _(label) for name, label in PAD_STYLES[style].items()}


RESOLUTION_CHOICES = [(name, (graphics_options.resolution_label(name),)) for name in graphics_options.RESOLUTIONS]
ENGINE_FPS_CHOICES = [('60', ('60 (original)',)), ('120', ('120',)), ('240', ('240',))]


def window_icon(root, tk, game_icon, own_icon, ico_file):
    """The launcher window's icon: the game's (icon0.png) when the game is set, else the port's own.
    Windows gets an .ico made with Pillow (ico_file); without Pillow, Tk reads the PNG itself.
    Returns the PhotoImage the caller must keep alive, or None.
    Tk 9 applies the -default forms only to toplevels created later (the window itself kept Tk's
    feather in the title bar and the taskbar), so the window gets the icon by itself first."""
    if WINDOWS:
        try:
            from PIL import Image
            ico_file.parent.mkdir(parents=True, exist_ok=True)
            with Image.open(game_icon) as image:
                image.convert('RGBA').save(ico_file, sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (256, 256)])
            root.iconbitmap(str(ico_file))
            root.iconbitmap(default=str(ico_file))
            return None
        except (ImportError, OSError, tk.TclError):
            pass
    for icon in ((game_icon, own_icon) if WINDOWS else (own_icon, game_icon)):
        try:
            photo = tk.PhotoImage(file=str(icon))
            if photo.width() > 128:
                photo = photo.subsample(photo.width() // 64)
            root.iconphoto(False, photo)
            root.iconphoto(True, photo)
            return photo
        except (tk.TclError, OSError):
            continue
    return None


class Launcher:
    def __init__(self, root, tk, ttk, filedialog, messagebox):
        self.tk, self.ttk, self.filedialog, self.messagebox = tk, ttk, filedialog, messagebox
        self.root = root
        self.app = {**APP_DEFAULTS, **migrate_controls(load_json(CONFIG_FILE, {}))}
        self.ini, self.ini_lines = load_ini()
        self.vars = {}
        self.process = self.job = None
        self.output = queue.Queue()
        self.banner_source = self.banner_image = None
        self.ui_calls = queue.Queue()  # work for the Tk thread from helper threads
        self.mod_order, self.mod_vars, self.patch_vars = [], {}, {}
        self.pending_update = None  # (version, url, page) once offered
        root.title(f'{APP_NAME} (PC)')
        if sys.platform == 'win32':
            try:  # the taskbar shows this window's icon, not Python's
                import ctypes
                ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID('gow3pc.launcher')
            except (AttributeError, OSError):
                pass
        root.configure(bg=BG)
        self.dpi = root.winfo_fpixels('1i') / 96.0
        left, top, width, height = (0, 0, 0, 0) if WINDOWS else monitor_under_pointer(root)
        if width:
            # Linux reports 96 dpi everywhere: size the interface by the monitor it opens on.
            boost = max(1.0, min(width / 1920, height / 1080, 2.0)) * 1.25
            self.dpi *= boost
            root.tk.call('tk', 'scaling', root.winfo_fpixels('1i') / 72.0 * boost)
        window_width, window_height = self.px(1120), self.px(740)
        if width:
            window_width, window_height = min(window_width, int(width * 0.9)), min(window_height, int(height * 0.9))
            root.geometry(f'{window_width}x{window_height}+{left + (width - window_width) // 2}'
                          f'+{top + (height - window_height) // 2}')
        else:
            root.geometry(f'{window_width}x{window_height}')
        root.minsize(min(self.px(980), window_width), min(self.px(660), window_height))
        self.set_icon()
        self.style()
        self.build()
        self.show('home')
        self.refresh_lists()  # the Game page's lists now, so the first visit does not count as a change
        self.refresh_game_patches()
        self.saved = self.gather()
        root.protocol('WM_DELETE_WINDOW', self.close)
        root.after(100, self.drain_output)
        root.after(500, self.watch_changes)
        if WINDOWS and self.app.get('check_updates', True):
            threading.Thread(target=self.check_update, daemon=True).start()

    def meta_dir(self):
        return meta_folder(self.var('game_dir', 'app').get(), self.var('update_dir', 'app').get())

    def px(self, size):
        return int(size * self.dpi)

    def set_icon(self):
        game_icon = Path(self.meta_dir() or '.') / 'sce_sys' / 'icon0.png'
        self.icon = window_icon(self.root, self.tk, game_icon, PORT_DIR / 'launcher' / 'gow3.png',
                                CONFIG_DIR / 'game_icon.ico')

    # ---- look --------------------------------------------------------------------------------
    def check_images(self):
        """16 px (DPI-scaled) check box images: the clam theme draws a cross."""
        tk = self.tk
        n = max(14, int(16 * self.dpi))
        images = []
        for checked in (False, True):
            image = tk.PhotoImage(width=n + self.px(8), height=n)  # unset pixels stay transparent
            fill = ACCENT if checked else CARD
            image.put(ACCENT_HI if checked else '#4a4040', to=(0, 0, n, n))
            image.put(fill, to=(1, 1, n - 1, n - 1))
            if checked:
                # Tick: down from (0.22n, 0.52n) to (0.42n, 0.72n), up to (0.78n, 0.30n).
                pts = []
                x0, y0, x1, y1, x2, y2 = .22 * n, .52 * n, .42 * n, .72 * n, .78 * n, .28 * n
                for t in range(40):
                    f = t / 39
                    pts.append((x0 + (x1 - x0) * f, y0 + (y1 - y0) * f))
                    pts.append((x1 + (x2 - x1) * f, y1 + (y2 - y1) * f))
                width = max(1, round(n / 9))
                for x, y in pts:
                    image.put('#f4ece0', to=(int(x), int(y), int(x) + width, int(y) + width))
            images.append(image)
        self.check_off, self.check_on = images

    def style(self):
        ttk = self.ttk
        s = ttk.Style(self.root)
        s.theme_use('clam')
        if not hasattr(self, 'check_on'):  # once: a rebuilt window keeps the element and its images
            self.check_images()
            s.element_create('Bb.indicator', 'image', self.check_off, ('selected', self.check_on), sticky='')
        s.layout('TCheckbutton', [('Checkbutton.padding', {'sticky': 'nswe', 'children': [
            ('Bb.indicator', {'side': 'left', 'sticky': ''}),
            ('Checkbutton.focus', {'side': 'left', 'sticky': 'w', 'children': [
                ('Checkbutton.label', {'sticky': 'nswe'})]})]})])
        base = (UI_FONT, 10)
        self.root.option_add('*TCombobox*Listbox.background', CARD)
        self.root.option_add('*TCombobox*Listbox.foreground', TEXT)
        self.root.option_add('*TCombobox*Listbox.selectBackground', ACCENT)
        self.root.option_add('*TCombobox*Listbox.font', base)
        s.configure('.', background=PANEL, foreground=TEXT, fieldbackground=CARD, bordercolor=LINE,
                    lightcolor=LINE, darkcolor=LINE, troughcolor=CARD, focuscolor=ACCENT, font=base)
        s.configure('TFrame', background=PANEL)
        s.configure('TLabel', background=PANEL, foreground=TEXT)
        s.configure('Muted.TLabel', background=PANEL, foreground=MUTED, font=(UI_FONT, 9))
        s.configure('Restart.TLabel', background=PANEL, foreground=ACCENT_HI, font=(UI_FONT, 8, 'bold'))
        s.configure('Section.TLabel', background=PANEL, foreground=ACCENT_HI, font=heading_font(12))
        s.configure('TCheckbutton', background=PANEL, foreground=TEXT, padding=(0, 3))
        s.map('TCheckbutton', background=[('active', PANEL)], foreground=[('disabled', MUTED)])
        s.configure('TRadiobutton', background=PANEL, foreground=TEXT, indicatorcolor=CARD)
        s.map('TRadiobutton', background=[('active', PANEL)], indicatorcolor=[('selected', ACCENT)])
        s.configure('Warning.TLabel', background='#2a1414', foreground='#e0a0a0', padding=(10, 6))
        s.configure('TCombobox', arrowcolor=MUTED, foreground=TEXT, padding=4)
        s.map('TCombobox', fieldbackground=[('readonly', CARD)], foreground=[('readonly', TEXT)],
              selectbackground=[('readonly', CARD)], selectforeground=[('readonly', TEXT)])
        s.configure('TEntry', foreground=TEXT, insertcolor=TEXT, padding=4)
        s.configure('TSpinbox', foreground=TEXT, arrowcolor=MUTED, insertcolor=TEXT, padding=4)
        s.configure('TButton', background=CARD, foreground=TEXT, padding=(12, 6), borderwidth=1)
        s.map('TButton', background=[('active', LINE), ('disabled', PANEL)], foreground=[('disabled', MUTED)])
        s.configure('Play.TButton', background=ACCENT, foreground='#f4ece0', font=(*heading_font(15), 'bold'),
                    padding=(36, 10), borderwidth=0)
        s.map('Play.TButton', background=[('active', ACCENT_HI), ('disabled', '#2e1c1c')],
              foreground=[('disabled', '#8a7a78')])
        s.configure('Horizontal.TProgressbar', background=ACCENT, troughcolor=CARD, bordercolor=LINE)
        s.configure('Vertical.TScrollbar', background=CARD, arrowcolor=MUTED, troughcolor=PANEL, bordercolor=PANEL)

    # ---- widget helpers ----------------------------------------------------------------------
    def var(self, key, store):
        """The Tk variable of a setting (one per setting, shared by every widget that shows it)."""
        tk = self.tk
        if key not in self.vars:
            if store == 'ini':
                value = self.ini.get(key, INI_DEFAULTS[key])
                if key in INI_FLAGS:
                    v = tk.BooleanVar(value=value == '1')
                else:
                    v = tk.StringVar(value=value)
            else:
                value = self.app.get(key, APP_DEFAULTS[key])
                if isinstance(APP_DEFAULTS[key], bool):
                    v = tk.BooleanVar(value=bool(value))
                else:
                    v = tk.StringVar(value=str(value))
            v.store = store
            self.vars[key] = v
        return self.vars[key]

    def choice(self, parent, key, store, options, width=30, var=None):
        """A combobox over (value, (english, russian)) pairs, kept in sync with its variable.
        `options` can be a function returning the pairs: it is asked again each time the list opens."""
        var = var if var is not None else self.var(key, store)
        pairs = options if callable(options) else (lambda: options)

        def values():
            return [v for v, _t in pairs()]

        def labels():
            return [_(*text) for _v, text in pairs()]

        def fit(texts):
            # Wide enough for the longest label: the dropdown list is as wide as the box.
            return max([width] + [len(t) + 2 for t in texts])

        def reload():
            texts = labels()
            box.configure(values=texts, width=fit(texts))
        texts = labels()
        box = self.ttk.Combobox(parent, values=texts, state='readonly', width=fit(texts), postcommand=reload)
        if var.get() not in values():
            var.set(values()[0])

        def show(*_args):
            reload()  # the list may have changed since it was last opened
            current, known = var.get(), values()
            box.current(known.index(current) if current in known else 0)
        show()
        box.bind('<<ComboboxSelected>>', lambda _e: var.set(values()[box.current()]))
        var.trace_add('write', show)
        return box

    def mod_resolutions(self):
        """(size, (label,)) of the render resolutions the enabled mods add: the "Resolution Patch - <size>"
        entries of their patch files (a mod is on when its box is ticked on the Mods page, else as saved)."""
        from mods import patch_files, selected
        from patches import resolution_names
        if not self.var('mods_enabled', 'app').get():
            return []
        root = Path(self.var('mods_dir', 'app').get() or DATA_DIR / 'mods')
        names = ([n for n in self.mod_order if self.mod_vars.get(n) is None or self.mod_vars[n].get()]
                 if getattr(self, 'mod_vars', None) else selected(root, DATA_DIR / 'mods.json'))
        files = [f for n in names for f in patch_files(root / n)]
        return [(size, (graphics_options.resolution_label(size) + ' (mod)',)) for size in resolution_names(files)
                if size not in graphics_options.RESOLUTIONS]

    def resolution_options(self):
        """The render resolutions to choose from: the built-in ones and those of the enabled mods."""
        return RESOLUTION_CHOICES + self.mod_resolutions()

    def next_row(self, parent):
        return parent.grid_size()[1]

    def title_cell(self, parent, title, restart=False):
        """The name of a row; `restart` adds a mark: changing it needs a new start of the game."""
        cell = self.ttk.Frame(parent)
        self.ttk.Label(cell, text=title).pack(side='left')
        if restart:
            self.ttk.Label(cell, text=_('restart'), style='Restart.TLabel').pack(side='left', padx=(8, 0))
        return cell

    def row(self, parent, title, widget, hint=None, restart=False):
        """Name on the left, control on the right, an optional note below."""
        r = self.next_row(parent)
        self.title_cell(parent, title, restart).grid(row=r, column=0, sticky='w', padx=(0, 18), pady=(8, 0))
        widget.grid(row=r, column=1, sticky='w', pady=(5, 0))
        if hint:
            self.ttk.Label(parent, text=hint, style='Muted.TLabel', wraplength=self.px(640), justify='left').grid(
                row=r + 1, column=0, columnspan=2, sticky='w', pady=(1, 2))
        return widget

    def check(self, parent, key, store, title, hint=None, var=None, restart=False):
        box = self.ttk.Checkbutton(parent, variable=var if var is not None else self.var(key, store))
        return self.row(parent, title, box, hint, restart)

    def note(self, parent, text, top=12):
        self.ttk.Label(parent, text=text, style='Muted.TLabel', wraplength=self.px(680), justify='left').grid(
            row=self.next_row(parent), column=0, columnspan=2, sticky='w', pady=(top, 0))

    def section(self, parent, title, top=20):
        """A section heading with a line under it."""
        r = self.next_row(parent)
        head = self.ttk.Frame(parent)
        head.grid(row=r, column=0, columnspan=2, sticky='w', pady=(top, 0))
        self.ttk.Label(head, text=heading_text(title.upper()), style='Section.TLabel').pack(side='left')
        self.tk.Frame(parent, bg=LINE, height=1).grid(row=r + 1, column=0, columnspan=2, sticky='we', pady=(2, 2))

    def folder(self, parent, key, title, prompt, hint=None, on_change=None):
        ttk = self.ttk
        var = self.var(key, 'app')
        holder = ttk.Frame(parent)
        ttk.Entry(holder, textvariable=var, width=46).pack(side='left')

        def browse():
            chosen = self.filedialog.askdirectory(title=prompt, initialdir=var.get() or str(PORT_DIR))
            if chosen:
                var.set(str(Path(chosen)))
        ttk.Button(holder, text=_('Browse…', 'Обзор…'), command=browse).pack(side='left', padx=(6, 0))
        ttk.Button(holder, text=_('Open', 'Открыть'), command=lambda: self.open_path(var.get(), key)).pack(
            side='left', padx=(6, 0))
        if on_change:
            var.trace_add('write', lambda *_a: on_change())
        return self.row(parent, title, holder, hint)

    def scrolled_page(self, name, title, subtitle):
        """A settings page: a heading and content that scrolls vertically."""
        tk, ttk = self.tk, self.ttk
        outer = ttk.Frame(self.content)
        head = ttk.Frame(outer, padding=(28, 22, 28, 6))
        head.pack(fill='x')
        ttk.Label(head, text=heading_text(title.upper()), background=PANEL, foreground=TEXT, font=heading_font(20)).pack(anchor='w')
        ttk.Label(head, text=subtitle, style='Muted.TLabel').pack(anchor='w')
        canvas = tk.Canvas(outer, bg=PANEL, highlightthickness=0, bd=0)
        bar = ttk.Scrollbar(outer, orient='vertical', command=canvas.yview)
        inner = ttk.Frame(canvas, padding=(28, 0, 28, 24))
        inner.columnconfigure(1, weight=1)
        window = canvas.create_window(0, 0, window=inner, anchor='nw')
        inner.bind('<Configure>', lambda _e: canvas.configure(scrollregion=canvas.bbox('all')))
        canvas.bind('<Configure>', lambda e: canvas.itemconfigure(window, width=e.width))
        canvas.configure(yscrollcommand=bar.set)
        bar.pack(side='right', fill='y')
        canvas.pack(side='left', fill='both', expand=True)
        outer.scroll = lambda units: canvas.yview_scroll(units, 'units') if inner.winfo_height() > canvas.winfo_height() else None
        self.pages[name] = outer
        return inner

    # ---- layout ------------------------------------------------------------------------------
    def build(self):
        tk, ttk = self.tk, self.ttk
        side = tk.Frame(self.root, bg=BG, width=self.px(220))
        side.pack(side='left', fill='y')
        side.pack_propagate(False)
        self.side_title = tk.Label(side, text=heading_text(APP_NAME.upper(), every_language=True), bg=BG, fg=TEXT,
                                   font=heading_font(15, every_language=True),
                                   wraplength=self.px(190), justify='left')
        self.side_title.pack(anchor='w', padx=22, pady=(24, 22))
        self.side = side
        self.nav, self.current_page = {}, None
        titles = {'home': _('Home'), 'graphics': _('Graphics', 'Графика'), 'performance': _('Performance'),
                  'controls': _('Controls', 'Управление'),
                  'game': _('Game', 'Игра'), 'advanced': _('Advanced', 'Дополнительно')}
        for name in PAGES:
            item = tk.Label(side, text='    ' + heading_text(titles[name]), bg=BG, fg=TEXT, anchor='w',
                            font=heading_font(11, UI_FONT),
                            pady=10, cursor='hand2')
            item.pack(fill='x')
            item.bind('<Button-1>', lambda _e, n=name: self.show(n))
            item.bind('<Enter>', lambda _e, n=name: n != self.current_page and self.nav[n].configure(bg='#1a1414'))
            item.bind('<Leave>', lambda _e, n=name: n != self.current_page and self.nav[n].configure(bg=BG))
            self.nav[name] = item
        tk.Frame(side, bg=BG).pack(fill='both', expand=True)
        self.update_box = None
        self.side_note = tk.Label(side, text=_('In the game: Insert or R3+L2\nopens the port\'s menu.',
                                               'В игре: Insert или R3+L2\nоткрывает меню порта.'),
                                  bg=BG, fg=MUTED, font=(UI_FONT, 9), justify='left', wraplength=self.px(210))
        self.side_note.pack(anchor='w', padx=22, pady=(0, 4))
        tk.Label(side, text=f'v{VERSION}', bg=BG, fg=MUTED, font=(UI_FONT, 9)).pack(anchor='w', padx=22, pady=(0, 18))

        right = tk.Frame(self.root, bg=PANEL)
        right.pack(side='left', fill='both', expand=True)
        bar = tk.Frame(right, bg=BG, height=self.px(72))
        bar.pack(fill='x', side='bottom')
        bar.pack_propagate(False)
        self.status = tk.Label(bar, text='', bg=BG, fg=MUTED, font=(UI_FONT, 10), anchor='w', justify='left')
        self.status.pack(side='left', padx=24)
        self.play_button = ttk.Button(bar, text=heading_text(_('PLAY', 'ИГРАТЬ')), style='Play.TButton', command=self.play)
        self.play_button.pack(side='right', padx=(self.px(10), self.px(24)), pady=self.px(11))
        self.save_button = ttk.Button(bar, text=_('Save', 'Сохранить'), command=self.save, state='disabled')
        self.save_button.pack(side='right', pady=self.px(11))
        self.content = tk.Frame(right, bg=PANEL)
        self.content.pack(fill='both', expand=True)

        self.pages = {}
        self.build_home()
        self.build_graphics()
        self.build_performance()
        self.build_game()
        self.build_controls()
        self.build_advanced()
        self.root.bind_all('<MouseWheel>', self.wheel)
        if not WINDOWS:  # X11 (Tk 8.6): the wheel arrives as buttons 4 and 5; on Windows those are side buttons
            self.root.bind_all('<Button-4>', self.wheel)
            self.root.bind_all('<Button-5>', self.wheel)
        self.game_changed()

    def wheel(self, event):
        if self.capture_wheel:
            return self.capture_wheel(event)
        page = self.pages.get(self.current_page)
        if hasattr(page, 'scroll') and not isinstance(event.widget, (self.tk.Text, self.ttk.Combobox)) \
                and 'popdown' not in str(event.widget):
            page.scroll({4: -1, 5: 1}.get(event.num) or int(-event.delta / 120))

    def show(self, name):
        for page in self.pages.values():
            page.pack_forget()
        self.pages[name].pack(fill='both', expand=True)
        self.current_page = name
        for n, item in self.nav.items():
            item.configure(bg=PANEL if n == name else BG, fg=ACCENT_HI if n == name else TEXT)
        for method in PAGE_REFRESH.get(name, ()):
            getattr(self, method)()

    def build_home(self):
        tk, ttk = self.tk, self.ttk
        page = tk.Frame(self.content, bg=PANEL)
        self.pages['home'] = page
        self.banner = tk.Canvas(page, height=self.px(330), bg=BG, highlightthickness=0, bd=0)
        self.banner.pack(fill='x')
        self.banner.bind('<Configure>', lambda _e: self.draw_banner())
        self.banner_button = ttk.Button(self.banner, text=_('Choose the game folder'),
                                        command=lambda: self.show('game'))
        quick = ttk.Frame(page, padding=(28, 14, 28, 8))
        quick.pack(fill='x')
        ttk.Label(quick, text=heading_text(_('Quick settings', 'Основное').upper()), style='Section.TLabel').grid(
            row=0, column=0, columnspan=4, sticky='w')
        tk.Frame(quick, bg=LINE, height=1).grid(row=1, column=0, columnspan=4, sticky='we', pady=(2, 6))
        items = ((_('Display mode'), 'display_mode', DISPLAY_MODES), (_('Frame rate limit'), 'fps_limit', FPS_LIMITS),
                 (_('Render resolution'), 'render_resolution', self.resolution_options),
                 (_('Engine frame rate'), 'engine_fps', ENGINE_FPS_CHOICES))
        for index, (title, key, options) in enumerate(items):
            r, c = 2 + index // 2, (index % 2) * 2
            ttk.Label(quick, text=title).grid(row=r, column=c, sticky='w', pady=6, padx=(0, 12))
            self.choice(quick, key, 'ini', options, width=18).grid(row=r, column=c + 1, sticky='w', padx=(0, 40))

    def draw_banner(self):
        """The cover art of the selected dump (sce_sys/pic1.png) under the title."""
        tk, c = self.tk, self.banner
        c.delete('all')
        w, h, bar = max(c.winfo_width(), 400), int(c['height']), self.px(44)
        folder = self.meta_dir()
        info = game_info(folder)
        art = Path(folder or '.') / 'sce_sys' / 'pic1.png'
        if self.banner_source != art:
            self.banner_source, self.banner_image, self.banner_size = art, None, None
            try:
                self.banner_image = tk.PhotoImage(file=str(art))
            except (tk.TclError, OSError):
                pass
        if self.banner_image:
            if self.banner_size != w:
                self.banner_size, self.banner_scaled = w, self.scaled_art(art, w)
            # The logo sits in the upper half of the cover: show that part.
            c.create_image(w // 2, int(h * 0.62), image=self.banner_scaled)
            # A solid strip under the art: the title id and version sit in it, not over the picture.
            c.create_rectangle(0, h - bar, w, h, fill=PANEL, outline='')
        else:
            c.create_text(30, h - 74, text=heading_text(APP_NAME.upper(), every_language=True), anchor='w', fill=TEXT,
                          font=heading_font(32, every_language=True))
        if info:
            title_id = game_profile_of(folder)[0]
            sub = _('{} · version {}', '{} · версия {}').format(title_id or '?', info[1])
            c.create_text(33, h - bar // 2, text=sub, anchor='w', fill=ACCENT_HI, font=(UI_FONT, 11, 'bold'))
        else:
            c.create_text(w // 2, h // 2 - 40, text=_('Choose your game folder to play.'),
                          fill=TEXT, font=(UI_FONT, 12))
            c.create_window(w // 2, h // 2, window=self.banner_button)

    def scaled_art(self, path, width):
        """The cover art scaled to `width`: Pillow (smooth) or Tk's integer subsampling."""
        try:
            from PIL import Image, ImageTk
            with Image.open(path) as image:
                height = round(image.height * width / image.width)
                return ImageTk.PhotoImage(image.convert('RGB').resize((width, height), Image.LANCZOS))
        except (ImportError, OSError):
            factor = max(1, round(self.banner_image.width() / max(width, 1)))
            return self.banner_image.subsample(factor) if factor > 1 else self.banner_image

    def build_graphics(self):
        """Window and overlay controls; temporal upscaling stays commented out."""
        f = self.scrolled_page('graphics', _('Graphics', 'Графика'), _('How the game appears on your screen.'))
        self.build_display(f)
        # Temporal upscaling is parked until its image quality is validated.
        # self.section(f, _('Experimental'))
        # self.check(f, 'experimental_upscaler', 'app', _('Enable the upscaler (experimental)'),
        #            _('Off (default): the game runs without temporal upscaling; FSR 3.1 and TAA need this switch on. '
        #              'FSR 3.1 is not calibrated for this game yet: expect ghosting or flicker. The remaining options require this switch; '
        #              'leave it off for normal gameplay.',
        #              'Выкл. (по умолчанию): игра без временного апскейлинга; FSR 3.1 и TAA нужен этот переключатель. '
        #              'FSR 3.1 ещё не откалиброван для этой игры: возможны гостинг и мерцание.'), restart=True)
        # self.section(f, _('Upscaling', 'Апскейлинг'))
        # self.row(f, _('Upscaler', 'Апскейлер'), self.choice(f, 'upscaler', 'ini', UPSCALERS))
        # self.row(f, _('Quality preset', 'Пресет'), self.choice(f, 'preset', 'ini', PRESETS),
        #          _('Render scale per axis: Quality renders at 1/1.5 of the output size.',
        #            'Масштаб рендера по каждой оси: Quality рисует в 1/1.5 размера вывода.'))
        # self.row(f, _('Output resolution', 'Разрешение вывода'), self.choice(f, 'output_res', 'ini', OUTPUTS),
        #          _('What the upscaler produces; the HUD is drawn at this size too.',
        #            'Что выдаёт апскейлер; интерфейс рисуется в этом же размере.'))
        # self.row(f, _('Live resolution changes', 'Смена разрешения на лету'), self.choice(f, 'live_resolution', 'ini', LIVE),
        #          _('Off: outputs other than 1080p are set by a patch at start (fastest; changing them in the '
        #            'game restarts it). On: change output and preset in the game without a restart, at a cost.',
        #            'Выкл.: разрешения кроме 1080p задаются патчем при запуске (быстрее). Вкл.: менять в игре '
        #            'без перезапуска, но медленнее.'))
        # self.check(f, 'object_motion', 'ini', _('Object motion vectors', 'Векторы движения объектов'),
        #            _('Less ghosting on characters, cloth and weapons; costs about 10% FPS.',
        #              'Меньше гостинга на персонажах и одежде; стоит около 10% FPS.'))

    def build_display(self, f):
        """Window and overlay: the top of the Graphics page."""
        self.section(f, _('Window', 'Окно'), top=4)
        self.row(f, _('Display mode'), self.choice(f, 'display_mode', 'ini', DISPLAY_MODES),
                 _('Borderless covers the screen without changing the display mode.'))
        self.check(f, 'vsync', 'ini', _('VSync'), _('Off allows tearing for the lowest latency.'))
        self.check(f, 'hdr', 'app', _('Allow HDR output', 'Разрешить HDR'),
                   _('When HDR is on in Windows and the display supports it.',
                     'Если HDR включён в Windows и монитор его поддерживает.'))
        self.section(f, _('Textures'))
        self.row(f, _('Anisotropic filtering'), self.choice(f, 'anisotropy', 'ini', ANISOTROPY),
                 _('Sharper floors and walls seen at an angle, at little cost on most graphics cards.'),
                 restart=True)
        self.section(f, _('Overlay'))
        self.check(f, 'show_fps', 'ini', _('Show performance overlay'),
                   _('Items, size and corner: in-game menu (Insert / R3+L2) > Overlay.'))
        self.section(f, _('Screenshots'))
        shots = self.ttk.Button(f, text=_('Open the screenshots folder'),
                           command=lambda: self.open_path(
                               Path(self.var('user_dir', 'app').get() or DATA_DIR / 'user') / 'screenshots'))
        self.row(f, _('F12 in the game'), shots,
                 _("Saves the game's image, without the overlay, as a PNG in the screenshots folder of the saves."))

    def build_performance(self):
        f = self.scrolled_page('performance', _('Performance'), _('Frame rate and speed options.'))
        self.section(f, _('Frame rate', 'Частота кадров'), top=4)
        self.row(f, _('Frame rate limit'), self.choice(f, 'fps_limit', 'ini', FPS_LIMITS),
                 _('The engine frame rate (below) is the highest it can go.'))
        self.row(f, _('Frames queued'), self.choice(f, 'frames_queued', 'ini', FRAMES_QUEUED),
                 _('1 is the lowest input latency; more can raise FPS when the graphics card is the limit.'))
        self.row(f, _('Engine frame rate'), self.choice(f, 'engine_fps', 'ini', ENGINE_FPS_CHOICES),
                 _('The highest frame rate the game itself runs at (a game patch).'), restart=True)
        self.row(f, _('Render resolution'), self.choice(f, 'render_resolution', 'ini', self.resolution_options),
                 _('The size the game draws at; higher is sharper and slower. A resolution patch replaces the '
                   'texture fix and reserves the memory it needs.'), restart=True)
        self.section(f, _('Shaders'))
        self.check(f, 'pipeline_binary_cache', 'app', _('Use pipeline binary cache (A/B test)'),
                   _('Reuse saved Vulkan pipelines. Disable to compare FPS with normal compilation. '
                     'Takes effect on the next launch; saved caches are preserved.'))
        self.check(f, 'background_warmup', 'app', _('Load shader cache in the background'),
                   _('The game opens at once and the saved shaders load while you play; a small note in the '
                     'corner shows the progress. Switch it off to wait on the loading screen instead.'))
        self.check(f, 'async_shaders', 'ini', _('Compile new shaders in the background'),
                   _('Fewer stutters in new areas: the game keeps running while new shaders compile. An object '
                     'or effect may appear a moment later. Switch it off if something stays missing.'))
        self.check(f, 'parallel_warmup', 'app', _('Fast shader cache loading (parallel)'),
                   _('Loading screen only: builds the cached pipelines on several CPU threads.'))
        self.section(f, _('GPU data'))
        gpus = list_gpus()
        cards = [('', ('Automatic',))] + [(row[0], (row[1],)) for row in gpus]
        self.row(f, _('Graphics card'), self.choice(f, 'gpu_id', 'app', cards),
                 _('The GPU the game runs on. Choose your dedicated card if the game uses the integrated one.'),
                 restart=True)
        self.row(f, _('Stream buffer memory'), self.choice(f, 'stream_memory', 'app', STREAM_MEMORY),
                 _('System RAM can improve FPS on NVIDIA laptops. Choose GPU memory to restore the previous '
                   'behavior. Applies on the next game launch.'), restart=True)
        self.check(f, 'stale_readback', 'ini', _('Game reads GPU data without waiting (big FPS gain)'),
                   _('About 40 to 66 FPS in a heavy scene. Needs deferred GPU readbacks. Switch it off if lighting '
                     'or objects look wrong.'), restart=True)
        self.check(f, 'deferred_readback', 'ini', _('Deferred GPU readbacks'),
                   _('Fewer waits for the GPU. Switch it off if textures or shadows look wrong.'), restart=True)
        self.check(f, 'partial_height_views', 'app', _('Fast screen effects (Rage of Sparta, weapons)'),
                   _('Keeps powers and weapon effects at full FPS. Switch it off if a screen effect '
                     '(blur, distortion, the color change of a power) looks stretched, shifted or wrong.'),
                   restart=True)
        self.row(f, _('Two-stage GPU pipeline', 'Двухстадийный конвейер GPU'), self.choice(f, 'draw_pipe', 'app', DRAW_PIPE),
                 _('20–30% faster; switch it off if the game is unstable.', 'Быстрее на 20–30%; при нестабильности выключите.'))
        self.row(f, _('GPU readbacks', 'Чтение данных GPU'), self.choice(f, 'readbacks', 'app', READBACKS),
                 _('How exactly data the GPU writes is copied back for the game.',
                   'Насколько точно данные, записанные GPU, возвращаются игре.'))
        card = self.var('gpu_id', 'app')
        box = self.choice(f, 'no_msaa', 'app', no_msaa_options(gpus, card.get()), width=46)
        self.row(f, _('Game multisampling'), box, _(NO_MSAA_HINT), restart=True)

        def relabel(*_args):  # Automatic names the card the game will run on
            index = max(box.current(), 0)
            box['values'] = [_(*text) for _value, text in no_msaa_options(gpus, card.get())]
            box.current(index)
        card.trace_add('write', relabel)

    def build_game(self):
        f = self.scrolled_page('game', _('Game', 'Игра'), _('Your game dump, saves and the game patches.'))
        self.section(f, _('Cheats'), top=4)
        self.check(f, 'cheat_double_jumps', 'ini', _('Infinite Double Jumps'),
                   _('Repeat air jumps without landing. Also available in the in-game Cheats menu.'))
        self.check(f, 'cheat_very_hard', 'ini', _('Unlock Very Hard'),
                   _('Select Very Hard without completing the game. Does not change existing save difficulty. '
                     'In-game: enable before opening New Game.'))
        self.section(f, _('Folders'))
        self.folder(f, 'game_dir', _('Game folder', 'Папка игры'),
                    _('Choose the folder with eboot.bin', 'Выберите папку с eboot.bin'),
                    _('Your own dump of the game (eboot.bin, sce_module, sce_sys): God of War III Remastered, '
                      'any region and version.',
                      'Ваш дамп игры (eboot.bin, sce_module, sce_sys): God of War III Remastered, '
                      'любой регион и версия.'), on_change=self.game_changed)
        self.folder(f, 'update_dir', _('Update folder (optional)'),
                    _('Choose the folder of the update (eboot.bin, sce_sys)'),
                    _('Only if the update is dumped apart from the game: it is laid over the game folder '
                      'without changing either. Leave empty when the game folder already has the update.'),
                    on_change=self.game_changed)
        self.folder(f, 'user_dir', _('Saves folder', 'Папка сохранений'),
                    _('Choose the saves folder', 'Выберите папку сохранений'),
                    _('Empty: {} (shader caches are kept there too).',
                      'Пусто: {} (там же кэш шейдеров).').format(DATA_DIR / 'user'), on_change=self.refresh_status)
        self.section(f, _('Saves'))
        self.build_save_backups(f)
        self.section(f, _('Language'))
        self.row(f, _('Game language', 'Язык игры'), self.choice(f, 'language', 'app', LANGUAGES))
        self.build_patches(f)
        self.build_mods(f)

    def user_dir(self):
        return Path(self.var('user_dir', 'app').get() or DATA_DIR / 'user')

    def build_save_backups(self, f):
        """run.py backs the saves up before every start; one of them can be restored here."""
        ttk = self.ttk
        holder = ttk.Frame(f)
        self.backup_choice = ttk.Combobox(holder, state='readonly', width=30)
        self.backup_choice.pack(side='left')
        ttk.Button(holder, text=_('Restore', 'Восстановить'), command=self.restore_backup).pack(side='left', padx=(6, 0))
        ttk.Button(holder, text=_('Refresh', 'Обновить'), command=self.refresh_backups).pack(side='left', padx=(6, 0))
        self.row(f, _('Save backups', 'Резервные копии'), holder,
                 _('Made before every start, newest first; the last {} are kept. Restoring backs up the '
                   'current saves first.').format(save_backup.KEEP))
        self.refresh_backups()

    def refresh_backups(self):
        names = save_backup.list_backups(self.user_dir())
        self.backup_choice.configure(values=names)
        self.backup_choice.set(names[0] if names else '')

    def restore_backup(self):
        name = self.backup_choice.get()
        if not name:
            return
        if self.process:
            self.messagebox.showwarning(APP_NAME, _('Close the game before restoring saves.'))
            return
        if not self.messagebox.askyesno(APP_NAME, _('Replace the current saves with the backup {}? '
                                                    'The current saves are backed up first.').format(name)):
            return
        try:
            safety = save_backup.restore(self.user_dir(), name)
        except (OSError, ValueError) as error:
            self.messagebox.showerror(APP_NAME, _('Could not restore: {}').format(error))
            return
        self.refresh_backups()
        self.refresh_status()
        self.messagebox.showinfo(APP_NAME, _('Saves restored from {}.').format(name) +
                                 (_(' Previous saves: {}.').format(safety.name) if safety else ''))

    def build_patches(self, f):
        """Community patches of the selected game's built-in XML and third-party patch files, on the
        Game page. Resolution and frame rate patches follow Performance (gow3.ini; run.py adds them)."""
        self.section(f, _('Game patches', 'Патчи игры'))
        self.game_patch_frame = self.ttk.Frame(f)
        self.game_patch_frame.grid(row=self.next_row(f), column=0, columnspan=2, sticky='we')
        self.game_patch_title, self.game_patch_vars = None, {}
        self.section(f, _('Third-party patches', 'Сторонние патчи'))
        self.folder(f, 'patches_dir', _('Patches folder', 'Папка патчей'), _('Choose the patches folder', 'Выберите папку патчей'),
                    _('shadPS4/GoldHEN XML patch files for this game version. Empty: {}',
                      'XML-патчи shadPS4/GoldHEN для этой версии игры. Пусто: {}').format(DATA_DIR / 'patches'),
                    on_change=self.refresh_lists)
        self.patches_frame = self.ttk.Frame(f)
        self.patches_frame.grid(row=self.next_row(f), column=0, columnspan=2, sticky='we', pady=(8, 0))
        self.ttk.Button(f, text=_('Refresh', 'Обновить'), command=self.refresh_lists).grid(
            row=self.next_row(f), column=0, sticky='w', pady=(14, 0))

    def refresh_game_patches(self):
        import xml.etree.ElementTree as ET
        from patches import ALWAYS_ON, EXCLUSIVE_PREFIX
        tk, f = self.tk, self.game_patch_frame
        self.store_game_patches()
        for widget in f.winfo_children():
            widget.destroy()
        f.columnconfigure(1, weight=1)
        folder = self.meta_dir()
        title_id, profile = game_profile_of(folder)
        self.game_patch_title, self.game_patch_vars = None, {}
        if not profile:
            self.note(f, _('No built-in patches for this game.', 'Для этой игры нет встроенных патчей.'), top=0)
            return
        _ids, xml, version = profile[:3]
        metas = [m for m in ET.parse(xml).getroot().iter('Metadata') if m.get('AppVer') == version]
        chosen = self.app.get('game_patches', {}).get(title_id)
        on = set(chosen) if chosen is not None else {m.get('Name') for m in metas
                                                         if m.get('isEnabled', 'false').lower() == 'true'}
        self.note(f, _('{} · version {} · {}. Resolution and frame rate patches follow Performance.',
                       '{} · версия {} · {}.').format(title_id, version, xml.name), top=0)
        for meta in metas:
            name = meta.get('Name')
            if name.startswith(EXCLUSIVE_PREFIX) or name.startswith('Frame Rate Patch') or name in ALWAYS_ON:
                continue
            var = tk.BooleanVar(value=name in on)
            self.game_patch_vars[name] = var
            author = meta.get('Author')
            self.check(f, None, None, name + (f'  ({author})' if author else ''), meta.get('Note') and _(meta.get('Note')),
                       var=var)
        self.game_patch_title = title_id

    def store_game_patches(self):
        """The page's selection into settings (the whole list for this game, without the resolution
        and frame rate patches: run.py adds those from gow3.ini)."""
        if not self.game_patch_title:
            return
        self.app.setdefault('game_patches', {})[self.game_patch_title] = \
            [n for n, v in self.game_patch_vars.items() if v.get()]

    def build_mods(self, f):
        """Loose-file mods layered over the game at start, on the Game page."""
        self.section(f, _('Mods', 'Моды'))
        self.check(f, 'mods_enabled', 'app', _('Load mods', 'Загружать моды'),
                   _('Loose-file mods, each in its own folder with dvdroot_ps4.',
                     'Моды из файлов, каждый в своей папке с dvdroot_ps4.'))
        self.folder(f, 'mods_dir', _('Mods folder', 'Папка модов'), _('Choose the mods folder', 'Выберите папку модов'),
                    _('Empty: {}', 'Пусто: {}').format(DATA_DIR / 'mods'), on_change=self.refresh_lists)
        self.mods_frame = self.ttk.Frame(f)
        self.mods_frame.grid(row=self.next_row(f), column=0, columnspan=2, sticky='we', pady=(8, 0))
        self.ttk.Button(f, text=_('Refresh', 'Обновить'), command=self.refresh_lists).grid(
            row=self.next_row(f), column=0, sticky='w', pady=(14, 0))

    def build_controls(self):
        """The Controls page: the keyboard & mouse table, button icons and the gamepad remapping of
        the PS4 buttons (run.py passes GOW3_KEY_MAP and GOW3_PAD_MAP to the game)."""
        tk, ttk = self.tk, self.ttk
        f = self.scrolled_page('controls', _('Controls', 'Управление'),
                               _('Keyboard and mouse, button icons and gamepad mapping.'))
        saved = {kind: control_values(self.app.get('controls'), kind) for kind in ('key', 'pad')}
        style = self.app.get('pad_style') if self.app.get('pad_style') in PAD_STYLES else 'playstation'
        self.pad_style = tk.StringVar(value=style)
        self.control_vars, self.pad_boxes, self.pad_icons = {'key': {}, 'pad': {}}, {}, {}
        self.key_rows = {}  # keyboard button: (input box, reset arrow)
        self.capturing_key = self.capture_wheel = None  # the key box waiting for an input
        size = self.px(28)
        labels = pad_labels(style)

        def icon_canvas(parent):
            return tk.Canvas(parent, width=size, height=size, bg=PANEL, highlightthickness=0, bd=0)

        def mapping_table():
            table = ttk.Frame(f)
            table.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(12, 0))
            return table

        def game_button(table, r, button):
            game_icon = icon_canvas(table)
            game_icon.grid(row=r, column=0, sticky='w', pady=3)
            if button in button_icons.GAME_BUTTONS:
                button_icons.draw(game_icon, button_icons.GAME_BUTTONS[button], 'playstation', size)
            elif button in ('rage', 'grab'):  # two buttons together
                button_icons.draw(game_icon, 'label:L3+R3' if button == 'rage' else 'label:L1+O', 'playstation', size)
            else:  # an action of the right stick (dodge): the stick icon labelled RS
                button_icons.draw(game_icon, 'rightstick', 'xbox', size)
            title = _(PS_BUTTON_NAMES.get(button) or ACTION_NAMES[button])
            ttk.Label(table, text=title, width=-14).grid(row=r, column=1, sticky='w', padx=(0, 12))

        def slider(key, low, high, unit):
            holder = ttk.Frame(f)
            amount = self.var(key, 'ini')
            tk.Scale(holder, from_=low, to=high, orient='horizontal', variable=amount, showvalue=False,
                     length=self.px(260), width=self.px(12), sliderlength=self.px(18), bg=ACCENT, troughcolor=CARD,
                     activebackground=ACCENT_HI, highlightthickness=0, bd=0, sliderrelief='flat').pack(side='left')
            value = ttk.Label(holder, width=7)
            value.pack(side='left', padx=10)
            show = lambda *_a: value.configure(text=f'{amount.get()}{unit}')
            amount.trace_add('write', show)
            show()
            return holder

        # Keyboard & mouse: one table for the keys and the mouse, with its own actions.
        self.section(f, _('Keyboard & Mouse Mapping'), top=4)
        self.note(f, _('Which key or mouse input presses each button of the game.'), top=4)
        table = mapping_table()
        r = 0
        for row in table_rows('key'):
            button = row[0]
            r += 1
            if button == 'rage':
                ttk.Checkbutton(table, text=_('Rage of Sparta on one input (L3 + R3 together)'),
                                variable=self.var('rage_one_input', 'app')).grid(
                    row=r, column=0, columnspan=4, sticky='w', pady=(6, 6))
                r += 1
            default = default_input(row, 'key')
            game_button(table, r, button)
            var = tk.StringVar(value=saved['key'].get(button) or default)
            self.control_vars['key'][button] = var
            box = self.key_binder(table, 'key', button, var)
            box.grid(row=r, column=2, sticky='w')
            arrow = ttk.Button(table, text='\u21a9', width=3, takefocus=False,
                               command=lambda b=button, d=default: self.assign_key('key', b, d))
            arrow.grid(row=r, column=3, sticky='w', padx=(6, 0))
            self.key_rows[button] = (box, arrow)
        self.var('rage_one_input', 'app').trace_add('write', lambda *_a: self.grey_rage_rows())
        self.grey_rage_rows()
        self.note(f, _('Click a box, then press a key, click a mouse button or turn the wheel (it gives up after '
                       '{} seconds). An input already used by another button swaps with it; the arrow puts one '
                       'button back on its default. Rage of Sparta presses L3 and R3 together: tick the box above '
                       'it to use one input instead of two. Grab presses L1 and Circle together. Dodge evades the '
                       'way you are moving (WASD), or back when standing still. Touchpad right opens the game\'s '
                       'pause menu. Insert always opens the port\'s menu and cannot be bound. Applied when the '
                       'game starts.').format(CAPTURE_SECONDS))
        ttk.Button(f, text=_('Reset keyboard & mouse to defaults'), command=lambda: self.reset_controls('key')).grid(
            row=self.next_row(f), column=0, sticky='w', pady=(14, 0))
        self.row(f, _('Joystick prompts'), self.choice(f, 'stick_prompts', 'app', STICK_PROMPTS),
                 hint=_('How the stick-rotation prompts are answered. Keyboard: type the turn instead, half turns '
                        'from the top D S A (clockwise) or A S D, from the bottom A W D or D W A, full turns D S A W '
                        'or A S D W; a key out of turn starts again. Adaptive mouse: draw the turn with the mouse.'))
        self.row(f, _('Mouse prompt distance'), slider('mouse_prompt_distance', 16, 200, ' px'),
                 _('Adaptive mouse: how far the mouse moves for one step of a rotation. Also in the in-game menu.'))
        self.row(f, _('Right stick style'), self.choice(f, 'mouse_aim_style', 'ini', AIM_STYLES),
                 hint=_('Aiming with the mouse while L2 is held (Head of Helios, Bow of Apollo). Relative: the stick '
                        'follows the movement and goes back to neutral when the mouse stops. Absolute: where the mouse '
                        'was when L2 was pressed is the centre, and the stick points from it to the mouse. Also in '
                        'the in-game menu.'))
        self.row(f, _('Right stick sensitivity'), slider('mouse_aim_sensitivity', 10, 200, '%'),
                 _('Aiming with the mouse while L2 is held: lower needs more mouse movement for the same stick, '
                   'higher less. Also in the in-game menu.'))
        self.check(f, 'mouse_lock', 'ini', _('Keep the mouse in the game window'),
                   _('While you play, the mouse stays in the game window, so a click cannot land on another monitor '
                     'or app. The in-game menu and switching windows get it back. Also in the in-game menu.'))

        self.section(f, _('General'))
        self.row(f, _('In-game button icons'), self.choice(f, 'glyph_style', 'app', GLYPH_STYLES),
                 hint=_('Replaces the PlayStation button icons inside the game with the ones of your controller. '
                        'Works on a copy of two game files in the port\'s data folder (about 70 MB); your game folder '
                        'is never changed. Applied when the game starts.'))

        self.section(f, _('Gamepad Mapping'))
        self.note(f, _('Which gamepad button presses each button of the game.'), top=4)
        names = ttk.Frame(f)
        for value, label in (('playstation', 'PlayStation'), ('xbox', 'Xbox')):
            ttk.Radiobutton(names, text=label, value=value, variable=self.pad_style,
                            command=self.relabel_pad_controls).pack(side='left', padx=(0, 14))
        self.row(f, _('Gamepad names'), names)
        self.row(f, _('Vibration'), slider('vibration', 0, 100, '%'), _("Strength of the gamepad's vibration; 0% turns it off."))
        table = mapping_table()
        for r, (button, _key, pad) in enumerate(CONTROLS, start=1):
            game_button(table, r, button)
            pad_name = saved['pad'].get(button) or pad
            pad_var = tk.StringVar(value=pad_name or '')
            self.control_vars['pad'][button] = pad_var
            pad_icon = icon_canvas(table)
            pad_icon.grid(row=r, column=2, sticky='w', padx=(0, 6))
            self.pad_icons[button] = pad_icon
            if pad:
                box = ttk.Combobox(table, state='readonly', values=list(labels.values()),
                                   width=max(22, *(len(l) + 2 for l in labels.values())))
                box.set(labels.get(pad_name, pad_name))

                def chosen(_e, b=box, v=pad_var, n=button):
                    v.set(self.pad_name(b.get()))
                    self.draw_pad_icon(n)
                box.bind('<<ComboboxSelected>>', chosen)
                self.pad_boxes[button] = box
            else:
                fixed = (_('L1 + Touchpad right') if button == 'touchpad_left'
                         else _('{} trigger').format(_(PS_BUTTON_NAMES[button])))
                box = ttk.Label(table, text=fixed, style='Muted.TLabel')
            box.grid(row=r, column=3, sticky='w')
            self.draw_pad_icon(button)
        self.note(f, _('L2/R2 stay on the triggers. Touchpad right opens the game\'s pause menu; Touchpad left '
                       '(photo mode) is L1 + Touchpad right. R3+L2 always opens the port\'s menu. '
                       'Applied when the game starts.'))
        ttk.Button(f, text=_('Reset gamepad to defaults'), command=lambda: self.reset_controls('pad')).grid(
            row=self.next_row(f), column=0, sticky='w', pady=(14, 0))

    def key_binder(self, parent, kind, button, var):
        """An input box of the keyboard & mouse table: click it, then press a key, click a mouse
        button or turn the wheel."""
        box = self.ttk.Button(parent, width=16, takefocus=False)
        mouse = kind == 'key'
        countdown = None

        def show(*_a):
            box.configure(text=_('(none)') if var.get() in ('', 'None') else var.get())
        var.trace_add('write', show)
        show()

        def finish(_e=None):
            nonlocal countdown
            for sequence in ('<KeyPress>', '<FocusOut>', '<ButtonPress>'):
                box.unbind(sequence)
            box.grab_release()
            if countdown:
                box.after_cancel(countdown)
                countdown = None
            if self.capturing_key is box:
                self.capturing_key = self.capture_wheel = None
            show()
            return 'break'

        def bind(name):
            self.assign_key(kind, button, name)
            return finish()

        def pressed(event):
            name = sdl_key_name(event.keysym)
            if name is None:
                reason = RESERVED_KEYS.get(event.keysym)
                box.configure(text=_(reason) if reason else _('Key not supported'))
                return 'break'
            return bind(name)

        def clicked(event):
            name = mouse_name(event.num) if mouse else None
            return bind(name) if name else finish()

        def wheeled(event):
            return bind(mouse_name(wheel_delta=event.delta or {4: 1, 5: -1}.get(event.num, 0))) if mouse else finish()

        def capture():
            if self.capturing_key not in (None, box):
                self.capturing_key.event_generate('<FocusOut>')
            self.capturing_key, self.capture_wheel = box, wheeled
            prompt = _('Press a key or mouse') if mouse else _('Press a key')

            def tick(left):
                nonlocal countdown
                if left <= 0:
                    finish()
                    return
                box.configure(text=f'{prompt} ({left})')
                countdown = box.after(1000, tick, left - 1)
            tick(CAPTURE_SECONDS)
            box.bind('<KeyPress>', pressed)
            box.bind('<ButtonPress>', clicked)
            box.bind('<FocusOut>', finish)
            box.focus_set()
            box.grab_set()  # clicks anywhere in the window come to the box
        box.configure(command=capture)
        return box

    def grey_rage_rows(self):
        """Rage of Sparta on one input greys L3 and R3; off, it greys the Rage row."""
        combined = bool(self.var('rage_one_input', 'app').get())
        for button in (*RAGE_BUTTONS, 'rage'):
            enabled = combined if button == 'rage' else not combined
            for widget in self.key_rows.get(button, ()):
                widget.state(['!disabled'] if enabled else ['disabled'])

    def assign_key(self, kind, button, name):
        """Binds `name` to `button` in a table; a button that had it takes the old one (bind_key)."""
        table = self.control_vars[kind]
        bindings = {b: v.get() for b, v in table.items()}
        other = bind_key(bindings, button, name)
        if other:
            table[other].set(bindings[other])
        table[button].set(name)

    def draw_pad_icon(self, button):
        """The gamepad column's icon of a game button, in the chosen style (L2/R2: the triggers)."""
        name = self.control_vars['pad'][button].get() or button_icons.GAME_BUTTONS[button]
        button_icons.draw(self.pad_icons[button], name, self.pad_style.get(), self.px(28))

    def pad_name(self, label):
        """SDL gamepad button name of a label in the current naming style."""
        return next((n for n, l in pad_labels(self.pad_style.get()).items() if l == label), label)

    def relabel_pad_controls(self):
        labels = pad_labels(self.pad_style.get())
        for button, box in self.pad_boxes.items():
            box.configure(values=list(labels.values()))
            name = self.control_vars['pad'][button].get()
            box.set(labels.get(name, name))
        for button in self.pad_icons:
            self.draw_pad_icon(button)

    def reset_controls(self, kind=None):
        """Puts one table ('key' or 'pad') or both (None) back on the defaults."""
        for table in ('key', 'pad'):
            for row in table_rows(table):
                if kind in (None, table) and row[0] in self.control_vars.get(table, {}):
                    self.control_vars[table][row[0]].set(default_input(row, table) or '')
        self.relabel_pad_controls()

    def build_linux_options(self, f):
        """Wayland / X11, GameMode and MangoHud: only the ones this system has can be turned on."""
        tools = linux_tools()
        self.section(f, _('Linux'), top=4)
        self.check(f, 'desktop_integration', 'app', _('Menu entry and icon (AppImage)'),
                   _('Creates gow3.desktop in ~/.local/share/applications so the launcher and the game show their '
                     'icon in the dock and the menu. Turning it off removes the entry.'))
        self.row(f, _('Window system'), self.choice(f, 'video_backend', 'app', VIDEO_BACKENDS), restart=True,
                 hint=_('Automatic lets SDL choose. Pick X11 if the window misbehaves on Wayland (it runs through '
                        'XWayland).') + ('' if tools['wayland'] else '  ' + _('No Wayland session detected.')))
        for key, title, name, hint in (
                ('gamemode', _('GameMode'), 'gamemode', _('Asks GameMode (gamemoderun) for the performance governor '
                                                          'while the game runs.')),
                ('mangohud', _('MangoHud overlay'), 'mangohud', _('Shows MangoHud (FPS, frame times, temperatures) '
                                                                   'through its Vulkan layer.'))):
            box = self.check(f, key, 'app', title, hint + ('' if tools[name] else '  ' + _('Not installed.')), restart=True)
            if not tools[name]:
                box.configure(state='disabled')

    def build_advanced(self):
        tk, ttk = self.tk, self.ttk
        f = self.scrolled_page('advanced', _('Advanced', 'Дополнительно'),
                               _('Launcher options, files and diagnostics.'))
        self.section(f, _('Launcher', 'Лаунчер'), top=4)
        self.row(f, _('Launcher language', 'Язык лаунчера'), self.choice(f, 'ui_language', 'app', UI_LANGUAGES),
                 _('Applies when the launcher opens again.', 'Применится при следующем открытии лаунчера.'))
        self.check(f, 'discord_rpc', 'app', _('Show Discord Rich Presence', 'Показывать Discord Rich Presence'),
                   _('Shows only the game name and the time played on your Discord profile.',
                     'Показывает в профиле Discord только название игры и время в игре.'))
        self.check(f, 'close_on_play', 'app', _('Close the launcher when the game starts', 'Закрывать лаунчер при запуске игры'))
        if WINDOWS:
            self.check(f, 'check_updates', 'app', _('Check for updates when the launcher opens',
                                                    'Проверять обновления при открытии лаунчера'))
        holder = ttk.Frame(f)
        holder.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(10, 0))
        if WINDOWS:
            ttk.Button(holder, text=_('Check for updates', 'Проверить обновления'),
                       command=lambda: threading.Thread(target=self.check_update, args=(True,), daemon=True).start()
                       ).pack(side='left')
        ttk.Button(holder, text=_('Desktop shortcut', 'Ярлык на рабочем столе'), command=self.shortcut).pack(side='left', padx=6)
        ttk.Button(holder, text=_('Add to Steam'), command=self.add_to_steam).pack(side='left')
        ttk.Label(holder, text=f'v{VERSION}', style='Muted.TLabel').pack(side='left', padx=10)
        if not WINDOWS:
            self.build_linux_options(f)
        self.section(f, _('Files'))
        holder = ttk.Frame(f)
        holder.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(6, 0))
        ttk.Button(holder, text=_('Port folder', 'Папка порта'), command=lambda: self.open_path(DATA_DIR)).pack(side='left')
        ttk.Button(holder, text='gow3.ini', command=lambda: self.open_path(ini_path())).pack(side='left', padx=6)
        holder = ttk.Frame(f)
        holder.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(10, 0))
        ttk.Button(holder, text=_('Clear shader cache', 'Очистить кэш шейдеров'), command=self.clear_cache).pack(side='left')
        ttk.Label(holder, text=_('If the game only shows a black screen, this usually helps.',
                                 'Если игра показывает только чёрный экран, обычно это помогает.'),
                  style='Muted.TLabel').pack(side='left', padx=10)
        # Diagnostics: for development, folded away until asked for.
        toggle = ttk.Label(f, text='', style='Section.TLabel', cursor='hand2')
        toggle.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(22, 2))
        body = ttk.Frame(f)
        body.columnconfigure(1, weight=1)
        body_row = self.next_row(f)
        self.check(body, 'frame_stats', 'app', _('Frame statistics in the log (every 5 s)', 'Статистика кадров в журнале (раз в 5 с)'))
        self.check(body, 'gpu_profile', 'app', _('GPU time per pass in the log', 'Профиль GPU в журнале'))
        self.check(body, 'perf_diag', 'app', _('GPU wait and readback report in the log (every 5 s)'),
                   _('Shows where the frame waits for the GPU and which reads cause it. For performance tests.'))
        self.check(body, 'frame_capture_key', 'app', _('F11 captures a frame for analysis'),
                   _('Records how the next frame is drawn into the captures folder.'))
        self.check(body, 'vk_validation', 'app', _('Vulkan validation layers (needs the Vulkan SDK; much slower)',
                                                   'Слои валидации Vulkan (нужен Vulkan SDK; сильно замедляет)'))
        shown = tk.BooleanVar(value=False)

        def flip(_e=None):
            shown.set(not shown.get())
            toggle.configure(text=('▾ ' if shown.get() else '▸ ') + heading_text(_('Diagnostics (for development)').upper()))
            if shown.get():
                body.grid(row=body_row, column=0, columnspan=2, sticky='we')
            else:
                body.grid_forget()
        toggle.bind('<Button-1>', flip)
        shown.set(True)
        flip()
        self.build_log(f)

    def build_log(self, f):
        """The game's output, at the end of the Advanced page (the wheel over it scrolls the log)."""
        tk, ttk = self.tk, self.ttk
        self.section(f, _('Log', 'Журнал'))
        buttons = ttk.Frame(f)
        buttons.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(6, 8))
        ttk.Button(buttons, text=_('Copy', 'Копировать'), command=self.copy_log).pack(side='left')
        ttk.Button(buttons, text=_('Clear', 'Очистить'), command=lambda: self.set_log('')).pack(side='left', padx=6)
        holder = ttk.Frame(f)
        holder.grid(row=self.next_row(f), column=0, columnspan=2, sticky='we')
        self.log = tk.Text(holder, wrap='none', height=18, bg='#0a0808', fg='#cfc6bc', insertbackground=TEXT,
                           relief='flat', font=(MONO_FONT, 9), padx=8, pady=6, state='disabled', highlightthickness=0)
        bar = ttk.Scrollbar(holder, command=self.log.yview)
        self.log.configure(yscrollcommand=bar.set)
        bar.pack(side='right', fill='y')
        self.log.pack(side='left', fill='both', expand=True)

    # ---- state -------------------------------------------------------------------------------
    def game_changed(self):
        folder = self.meta_dir()
        info = game_info(folder)
        title = info[0] if info else APP_NAME
        self.root.title(f'{title} (PC)')
        self.side_title.configure(text=heading_text(title.upper(), every_language=True))
        if self.current_page == 'game':
            self.refresh_game_patches()
        self.banner_source = None
        self.draw_banner()
        self.set_icon()
        self.refresh_status()

    def refresh_status(self):
        if getattr(self, 'copying_system_files', False):
            return
        info = game_info(self.meta_dir())
        if not self.process:
            self.play_button.configure(text=heading_text(_('PLAY', 'ИГРАТЬ')), state='normal' if info else 'disabled')
            self.status.configure(text=self.summary() if info else _('Choose the game folder first.',
                                                                     'Сначала выберите папку игры.'), fg=MUTED)

    def summary(self):
        self.store_game_patches()
        title_id, _profile = game_profile_of(self.meta_dir())
        names = self.app.get('game_patches', {}).get(title_id) or []
        resolution = dict((v, _(*t)) for v, t in self.resolution_options()).get(self.var('render_resolution', 'ini').get(), '?')
        from patches import game_patchable, patch_status
        if not game_patchable(self.meta_dir()):
            return _('{} · 60 FPS engine · no patches for this game').format(resolution)
        text = _('{} · {} FPS engine · {} patches').format(resolution, self.var('engine_fps', 'ini').get(), len(names))
        # Which patches the game's eboot.bin had is known once the game has started (patches.py).
        status = patch_status(DATA_DIR / 'out')
        if status and status.get('skipped'):
            text += ' · ' + _('{} not found in this eboot.bin at the last start (see Log)').format(len(status['skipped']))
        return text

    def refresh_lists(self):
        if not hasattr(self, 'patches_frame'):
            return
        tk, ttk = self.tk, self.ttk
        from mods import discover, patch_files
        from patches import external_patches
        for holder in (self.mods_frame, self.patches_frame):
            for widget in holder.winfo_children():
                widget.destroy()
        root = Path(self.var('mods_dir', 'app').get() or DATA_DIR / 'mods')
        profile = load_json(DATA_DIR / 'mods.json', {})
        available = discover(root)
        if set(self.mod_order) != set(available):
            known = [n for n in profile.get('order', []) if n in available]
            self.mod_order = known + [n for n in available if n not in known]
        old = {n: v.get() for n, v in self.mod_vars.items()}
        self.mod_vars = {}
        if not available:
            ttk.Label(self.mods_frame, text=_('No mods in {} yet.', 'В {} пока нет модов.').format(root),
                      style='Muted.TLabel').pack(anchor='w')
        for index, name in enumerate(self.mod_order):
            line = ttk.Frame(self.mods_frame)
            line.pack(fill='x', pady=1)
            var = tk.BooleanVar(value=old.get(name, name not in profile.get('disabled', [])))
            self.mod_vars[name] = var
            ttk.Button(line, text='▲', width=3, command=lambda i=index: self.move_mod(i, -1)).pack(side='left')
            ttk.Button(line, text='▼', width=3, command=lambda i=index: self.move_mod(i, 1)).pack(side='left', padx=(2, 10))
            has_patches = bool(patch_files(root / name))
            ttk.Checkbutton(line, text=f'{index + 1}.  {name}' + (_('   (game patches)') if has_patches else ''),
                            variable=var, command=self.refresh_conflicts).pack(side='left')
        if len(self.mod_order) > 1:
            ttk.Label(self.mods_frame, text=_('Lower in the list loads later and wins conflicts.',
                                              'Ниже в списке — загружается позже и перекрывает.'),
                      style='Muted.TLabel').pack(anchor='w', pady=(4, 0))
        self.conflict_label = ttk.Label(self.mods_frame, text='', justify='left', foreground='#e08080',
                                        wraplength=self.px(680))
        self.conflict_label.pack(anchor='w', pady=(4, 0))
        self.refresh_conflicts()
        if available:
            presets = ttk.Frame(self.mods_frame)
            presets.pack(anchor='w', pady=(8, 0))
            ttk.Label(presets, text=_('Preset', 'Пресет')).pack(side='left', padx=(0, 6))
            self.preset_choice = ttk.Combobox(presets, values=sorted(load_json(MOD_PRESETS, {})), width=24)
            self.preset_choice.pack(side='left')
            ttk.Button(presets, text=_('Load', 'Загрузить'), command=self.load_mod_preset).pack(side='left', padx=(6, 0))
            ttk.Button(presets, text=_('Save', 'Сохранить'), command=self.save_mod_preset).pack(side='left', padx=(6, 0))
            ttk.Button(presets, text=_('Delete', 'Удалить'), command=self.delete_mod_preset).pack(side='left', padx=(6, 0))
        chosen = load_json(DATA_DIR / 'patches.json', {})
        _title_id, game = game_profile_of(self.meta_dir())
        found = external_patches(Path(self.var('patches_dir', 'app').get() or DATA_DIR / 'patches'),
                                 game[2], game[1], game[0]) if game else []
        old = {n: v.get() for n, v in self.patch_vars.items()}
        self.patch_vars = {}
        if not found:
            ttk.Label(self.patches_frame, text=_('No patch files for this game version yet.', 'Пока нет патчей для этой версии игры.'),
                      style='Muted.TLabel').pack(anchor='w')
        for key, _path, meta in found:
            on = key in chosen.get('enabled', []) or (key not in chosen.get('disabled', []) and
                                                      meta.get('isEnabled', 'false').lower() == 'true')
            var = tk.BooleanVar(value=old.get(key, on))
            self.patch_vars[key] = var
            author = meta.get('Author')
            ttk.Checkbutton(self.patches_frame, text=key + (f'  —  {author}' if author else ''), variable=var).pack(anchor='w')

    def refresh_conflicts(self):
        from mods import conflicts
        if not getattr(self, 'conflict_label', None) or not self.conflict_label.winfo_exists():
            return
        root = Path(self.var('mods_dir', 'app').get() or DATA_DIR / 'mods')
        found = conflicts(root, [n for n in self.mod_order if self.mod_vars[n].get()])
        lines = [_('⚠ {} file(s) replaced by more than one mod (the last one wins):').format(len(found))] if found else []
        lines += [f'   {path}: {" > ".join(owners)}' for path, owners in found[:12]]
        if len(found) > 12:
            lines.append(_('   … and {} more').format(len(found) - 12))
        self.conflict_label.configure(text='\n'.join(lines))

    def save_mod_preset(self):
        name = self.preset_choice.get().strip()
        if not name:
            self.messagebox.showinfo(APP_NAME, _('Type a name for the preset first.'))
            return
        presets = load_json(MOD_PRESETS, {})
        presets[name] = {'order': self.mod_order, 'disabled': [n for n, v in self.mod_vars.items() if not v.get()]}
        MOD_PRESETS.write_text(json.dumps(presets, indent=2), encoding='utf-8')
        self.preset_choice.configure(values=sorted(presets))

    def load_mod_preset(self):
        from mods import apply_preset
        preset = load_json(MOD_PRESETS, {}).get(self.preset_choice.get().strip())
        if not isinstance(preset, dict):
            return
        order, disabled = apply_preset(preset, list(self.mod_order))
        self.mod_order = order
        for name, var in self.mod_vars.items():
            var.set(name not in disabled)
        chosen = self.preset_choice.get()
        self.refresh_lists()
        self.preset_choice.set(chosen)

    def delete_mod_preset(self):
        presets = load_json(MOD_PRESETS, {})
        if presets.pop(self.preset_choice.get().strip(), None) is not None:
            MOD_PRESETS.write_text(json.dumps(presets, indent=2), encoding='utf-8')
        self.preset_choice.configure(values=sorted(presets))
        self.preset_choice.set('')

    def move_mod(self, index, step):
        other = index + step
        if 0 <= other < len(self.mod_order):
            self.mod_order[index], self.mod_order[other] = self.mod_order[other], self.mod_order[index]
            self.refresh_lists()

    def gather(self):
        """What collect() writes, as data: settings.json, the gow3.ini values, the mods selection and
        the third-party patches selection. Reads the window and changes nothing."""
        app, ini = dict(self.app), dict(self.ini)
        if self.game_patch_title:
            app['game_patches'] = {**app.get('game_patches', {}),
                                   self.game_patch_title: [n for n, v in self.game_patch_vars.items() if v.get()]}
        for key, var in self.vars.items():
            try:
                value = var.get()
            except self.tk.TclError:  # an unfinished number in a spinbox
                value = APP_DEFAULTS.get(key, INI_DEFAULTS.get(key))
            if var.store == 'ini':
                ini[key] = ('1' if value else '0') if key in INI_FLAGS else str(value)
            else:
                app[key] = value
        app['controls'] = {kind: control_changes({b: v.get() for b, v in vars_.items()}, kind)
                           for kind, vars_ in self.control_vars.items()}
        app['pad_style'] = self.pad_style.get()
        mods = {'order': list(self.mod_order),
                'disabled': [n for n, v in self.mod_vars.items() if not v.get()]} if self.mod_order else None
        patches = {'enabled': sorted(k for k, v in self.patch_vars.items() if v.get()),
                   'disabled': sorted(k for k, v in self.patch_vars.items() if not v.get())} if self.patch_vars else None
        return {'app': app, 'ini': ini, 'mods': mods, 'patches': patches}

    def collect(self):
        """Writes settings.json, gow3.ini, mods.json and patches.json."""
        state = self.gather()
        _disk, lines = load_ini()
        persisted = {line.split('=', 1)[0].strip() for line in lines
                     if '=' in line and not line.lstrip().startswith('#')}
        baseline = getattr(self, 'saved', {'ini': self.ini})['ini']
        edited = {key: state['ini'][key] for key, var in self.vars.items()
                  if var.store == 'ini' and
                  (key not in persisted or state['ini'][key] != baseline.get(key))}
        try:
            save_ini(edited, lines)
            atomic_write(CONFIG_FILE, json.dumps(state['app'], indent=2, ensure_ascii=False))
            if state['mods']:
                atomic_write(DATA_DIR / 'mods.json', json.dumps(state['mods'], indent=2))
            if state['patches']:
                # Patches of other game versions keep their choice.
                chosen = load_json(DATA_DIR / 'patches.json', {})
                shown = set(self.patch_vars)
                enabled = {k for k in chosen.get('enabled', []) if k not in shown} | set(state['patches']['enabled'])
                disabled = {k for k in chosen.get('disabled', []) if k not in shown} | set(state['patches']['disabled'])
                atomic_write(DATA_DIR / 'patches.json', json.dumps(
                    {'enabled': sorted(enabled), 'disabled': sorted(disabled)}, indent=2))
        except OSError:
            self.messagebox.showerror(APP_NAME, _('Could not save all settings. Check write permissions and free disk space, then try again.',
                                                   'Не удалось сохранить все настройки. Проверьте права записи и свободное место, затем повторите.'))
            return False
        self.app = state['app']
        self.ini, self.ini_lines = load_ini()
        for key, var in self.vars.items():
            if var.store == 'ini':
                var.set(self.ini[key] == '1' if key in INI_FLAGS else self.ini[key])
        self.saved = self.gather()
        self.refresh_save_button()
        return True

    def save(self):
        """The Save button: the files, without starting the game. A new launcher language rebuilds
        the window in it."""
        if not self.collect():
            return
        language = self.app.get('ui_language') or windows_language()
        if language != LANG:
            self.rebuild(language)

    def rebuild(self, language):
        """The window again in another language: every setting stays in its variable; the page,
        the log, the running game and a pending update offer come back."""
        global LANG
        LANG = language
        log, page, update = self.log.get('1.0', 'end-1c'), self.current_page, self.pending_update
        status = self.status['text'], self.status['fg']  # of the running game; otherwise build() redoes it
        for var in self.vars.values():  # the old widgets' value labels and links
            for modes, name in var.trace_info():
                var.trace_remove(modes, name)
        for child in self.root.winfo_children():
            child.destroy()
        self.style()
        self.build()
        self.set_log(log)
        if self.process:
            self.play_button.configure(text=heading_text(_('RUNNING')), state='disabled')
            self.status.configure(text=status[0], fg=status[1])
        if update:
            self.offer_update(*update)
        self.show(page)
        self.refresh_lists()
        self.refresh_game_patches()
        self.saved = self.gather()
        self.refresh_save_button()

    def refresh_save_button(self):
        """Save is only active while the window differs from the files (gather() against the last save)."""
        self.save_button.configure(state='normal' if self.gather() != self.saved else 'disabled')

    def watch_changes(self):
        self.refresh_save_button()
        self.root.after(500, self.watch_changes)

    # ---- game process ------------------------------------------------------------------------
    def offer_system_files(self):
        """A PKG extraction (game or update) has its system files beside the game: offers the
        copy (system_files_beside) for the game folder and for an update folder with an
        eboot.bin, the update's candidate checked against the game's param.sfo. False when a
        copy was started: it runs in the background and presses Play again when it ends."""
        game_dir = self.app['game_dir'] or '.'
        update_dir = self.app.get('update_dir') or ''
        folders = [(game_dir, False)]
        if update_dir and not same_folder(update_dir, game_dir) and (Path(update_dir) / 'eboot.bin').is_file():
            folders.append((update_dir, True))
        for folder, is_update in folders:
            reference = (game_sfo(game_dir) or None) if is_update else None
            source = system_files_beside(folder, reference, game_dir if is_update else None)
            if not source or not self.messagebox.askyesno(APP_NAME, _(
                    'The files in {} look like a PKG extraction: the system files (param.sfo, icon0.png, '
                    'pic1.png and the rest) are in\n\n{}\n\nCopy all of them into its sce_sys folder now? '
                    'Files already there are kept.').format(folder, source)):
                continue
            self.adopt_in_background(source, folder)
            return False
        return True

    def adopt_in_background(self, source, folder):
        """Copies the Sc0 files off the Tk thread (a slow drive would freeze the window), shows
        the result and presses Play again, so the next offer or check follows from what is on
        disk."""
        self.copying_system_files = True
        self.play_button.configure(state='disabled')
        self.status.configure(text=_('Copying the system files…'), fg=TEXT)

        def work():
            error = None
            try:
                adopt_system_files(source, folder)
            except OSError as failure:
                error = failure

            def done():
                self.copying_system_files = False
                self.game_changed()  # whatever was copied already shows
                if error:
                    self.messagebox.showerror(APP_NAME, _('Could not copy the files: {}').format(error))
                    self.show('game')
                else:
                    self.play()
            self.ui_calls.put(done)
        threading.Thread(target=work, daemon=True).start()

    def play(self):
        if self.process or getattr(self, 'copying_system_files', False):
            return
        if not self.collect():
            return
        if not game_info(self.app['game_dir']):
            self.messagebox.showerror(APP_NAME, _('Choose the game folder with eboot.bin (CUSA01623).',
                                                  'Выберите папку игры с eboot.bin (CUSA01623).'))
            self.show('game')
            return
        if not self.offer_system_files():
            return
        missing = missing_game_files(self.app['game_dir'])
        if missing:
            self.messagebox.showerror(APP_NAME, _('The game dump is incomplete. Missing or empty files:\n\n{}\n\n'
                                                  'Dump the game again, or copy these files from a complete dump.')
                                      .format('\n'.join(describe_missing(missing))))
            self.show('game')
            return
        update_dir = self.app.get('update_dir') or ''
        if update_dir and same_folder(update_dir, self.app['game_dir'] or '.'):
            self.messagebox.showerror(APP_NAME, _('Choose two separate folders for the game and its update, '
                                                  'or leave the update folder empty.'))
            self.show('game')
            return
        missing = missing_game_files(update_dir, REQUIRED_UPDATE_FILES) if update_dir else []
        if missing:
            self.messagebox.showerror(APP_NAME, _('The update folder {} is incomplete. Missing or empty files:\n\n{}\n\n'
                                                  'Choose the folder of the update (eboot.bin, sce_sys), or leave it empty.')
                                      .format(update_dir, '\n'.join(describe_missing(missing))))
            self.show('game')
            return
        self.set_log('')
        self.exit_hint = None
        # The same last_run.log as --play, so a report from the PLAY button has the file too.
        run_log = open_play_log(self.app)
        try:
            self.process = subprocess.Popen(run_command(), cwd=PORT_DIR, env=game_environment(self.app),
                                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                            stderr=subprocess.STDOUT, creationflags=NO_WINDOW, **NEW_GROUP)
        except OSError as error:
            run_log.close()
            self.append(_('Could not start: {}', 'Не удалось запустить: {}').format(error) + '\n')
            self.process = None
            return
        self.job = GameJob(self.process)
        threading.Thread(target=self.read_output, args=(self.process, run_log), daemon=True).start()
        self.play_button.configure(text=heading_text(_('RUNNING')), state='disabled')
        self.status.configure(text=_('Preparing the game; it opens in its own window…',
                                     'Подготовка игры; она откроется в своём окне…'), fg=TEXT)
        if self.app.get('close_on_play'):
            self.root.after(5000, self.root.destroy)  # the game keeps running

    def read_output(self, process, run_log=None):
        try:
            for raw in iter(process.stdout.readline, b''):
                text = raw.decode('utf-8', errors='replace')
                if run_log:
                    try:
                        run_log.write(text)
                    except (OSError, ValueError):
                        # Disk full or folder gone: keep draining the pipe, or the game blocks
                        # on its next print and the button stays at RUNNING.
                        _close_quietly(run_log)
                        run_log = None
                self.output.put(text)
        finally:
            if run_log:
                _close_quietly(run_log)
            self.output.put((process.wait(),))

    def drain_output(self):
        while not self.ui_calls.empty():
            self.ui_calls.get_nowait()()
        try:
            for _i in range(500):
                item = self.output.get_nowait()
                if isinstance(item, tuple):
                    self.append(_('\nThe game exited (code {}).\n', '\nИгра завершилась (код {}).\n').format(item[0]))
                    if getattr(self, 'exit_hint', None):
                        self.append(self.exit_hint + '\n')
                    self.process = None
                    if self.job:
                        self.job.close()
                    self.refresh_status()
                else:
                    self.exit_hint = exit_hint(item) or getattr(self, 'exit_hint', None)
                    if 'Entering original x86-64 code' in item:
                        self.status.configure(text=_('The game is running.', 'Игра запущена.'), fg=TEXT)
                    elif 'restarting through run.py' in item:
                        self.status.configure(text=_('Restarting with the new settings…', 'Перезапуск с новыми настройками…'), fg=TEXT)
                    self.append(item)
        except queue.Empty:
            pass
        self.root.after(100, self.drain_output)

    def append(self, text):
        self.log.configure(state='normal')
        self.log.insert('end', text)
        lines = int(self.log.index('end-1c').split('.')[0])
        if lines > MAX_LOG_LINES:
            self.log.delete('1.0', f'{lines - MAX_LOG_LINES}.0')
        self.log.see('end')
        self.log.configure(state='disabled')

    def set_log(self, text):
        self.log.configure(state='normal')
        self.log.delete('1.0', 'end')
        self.log.insert('end', text)
        self.log.configure(state='disabled')

    def copy_log(self):
        self.root.clipboard_clear()
        self.root.clipboard_append(self.log.get('1.0', 'end'))

    def open_path(self, path, key=None):
        if key == 'user_dir' and not path:
            path = DATA_DIR / 'user'
        elif key == 'mods_dir' and not path:
            path = DATA_DIR / 'mods'
        elif key == 'patches_dir' and not path:
            path = DATA_DIR / 'patches'
        path = Path(path or DATA_DIR)
        if path.suffix == '.ini':
            if not self.collect():
                return
        elif not path.exists():
            path.mkdir(parents=True, exist_ok=True)
        if WINDOWS:
            os.startfile(str(path))
        else:
            subprocess.Popen(['xdg-open', str(path)], env=host_environment(), stdin=subprocess.DEVNULL,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    # ---- updates -------------------------------------------------------------------------------
    def check_update(self, manual=False):
        """Helper thread: asks GitHub for the newest release and offers it when it is newer."""
        try:
            version, url, page = latest_release()
        except (OSError, ValueError, KeyError) as failure:
            if manual:
                self.ui_calls.put(lambda error=failure: self.messagebox.showerror(
                    APP_NAME, _('Could not check for updates: {}', 'Не удалось проверить обновления: {}').format(error)))
            return

        def show():
            if version_tuple(version) > version_tuple(VERSION):
                self.offer_update(version, url, page)
            elif manual:
                self.messagebox.showinfo(APP_NAME, _('You have the latest version ({}).',
                                                         'У вас последняя версия ({}).').format(VERSION))
        self.ui_calls.put(show)

    def offer_update(self, version, url, page):
        if self.update_box:
            return
        tk, ttk = self.tk, self.ttk
        text = _('Version {} is available.', 'Доступна версия {}.').format(version)
        box = tk.Frame(self.side, bg=CARD, highlightthickness=1, highlightbackground=ACCENT)
        self.update_label = tk.Label(box, text=text, bg=CARD, fg=TEXT, font=(UI_FONT, 10, 'bold'),
                                     wraplength=self.px(160), justify='left')
        self.update_label.pack(anchor='w', padx=10, pady=(8, 6))
        buttons = tk.Frame(box, bg=CARD)
        buttons.pack(fill='x', padx=10, pady=(0, 10))
        self.update_button = ttk.Button(buttons, text=_('Update', 'Обновить'),
                                        command=lambda: self.install_update(version, url, page))
        self.update_button.pack(fill='x')
        ttk.Button(buttons, text=_("What's new", 'Что нового'), command=lambda: open_url(page)).pack(
            fill='x', pady=(4, 0))
        box.pack(fill='x', padx=(22, 18), pady=(0, 14), before=self.side_note)
        self.update_box = box
        self.pending_update = (version, url, page)
        if not self.process:
            self.status.configure(text=text, fg=TEXT)

    def install_update(self, version, url, page):
        if self.process:
            self.messagebox.showinfo(APP_NAME, _('Close the game before updating.', 'Закройте игру перед обновлением.'))
            return
        if not FROZEN or not url:  # a source tree updates with git
            open_url(page)
            return
        if not self.messagebox.askyesno(APP_NAME, _(
                'Install version {} now? The launcher closes, installs it and opens again. Saves and settings are kept.',
                'Установить версию {} сейчас? Лаунчер закроется, установит её и откроется снова. Сохранения и '
                'настройки останутся.').format(version)):
            return
        self.update_button.configure(state='disabled')

        def progress(percent):
            self.update_label.configure(text=_('Downloading version {}… {}%', 'Загрузка версии {}… {}%').format(
                version, percent))

        def work():
            try:
                shutil.rmtree(UPDATE_DIR, ignore_errors=True)
                UPDATE_DIR.mkdir(parents=True, exist_ok=True)
                archive = UPDATE_DIR / 'update.zip'
                request = urllib.request.Request(url, headers={'User-Agent': 'gow3-launcher'})
                with urllib.request.urlopen(request, timeout=60) as response, open(archive, 'wb') as out:
                    total, done, shown = int(response.headers.get('Content-Length') or 0), 0, -1
                    while chunk := response.read(1 << 20):
                        out.write(chunk)
                        done += len(chunk)
                        percent = done * 100 // total if total else 0
                        if percent != shown:
                            shown = percent
                            self.ui_calls.put(lambda p=percent: progress(p))
                with zipfile.ZipFile(archive) as package:
                    package.extractall(UPDATE_DIR / 'new')
                archive.unlink()
                new = next((p.parent for p in (UPDATE_DIR / 'new').rglob('God of War III.exe')), None)
                if not new:
                    raise OSError('God of War III.exe is missing from the download')
                # The new launcher copies itself over this installation once this one has closed.
                subprocess.Popen([str(new / 'God of War III.exe'), '--install-update', str(PORT_DIR), str(os.getpid())],
                                 cwd=str(new), stdin=subprocess.DEVNULL, creationflags=NO_WINDOW)
                self.ui_calls.put(self.root.destroy)
            except (OSError, zipfile.BadZipFile) as failure:
                def failed(error=failure):
                    self.update_button.configure(state='normal')
                    self.update_label.configure(text=_('Version {} is available.', 'Доступна версия {}.').format(version))
                    self.messagebox.showerror(APP_NAME, _('Update failed: {}', 'Не удалось обновить: {}').format(error))
                self.ui_calls.put(failed)
        threading.Thread(target=work, daemon=True).start()

    def clear_cache(self):
        """Shader and pipeline caches; they are rebuilt while playing."""
        if self.process:
            return
        import shutil
        cache = Path(self.app['user_dir'] or DATA_DIR / 'user') / 'cache'
        shutil.rmtree(cache, ignore_errors=True)
        self.messagebox.showinfo(APP_NAME, _('Shader cache cleared. The next start stutters for a few minutes while '
                                                 'it is rebuilt.', 'Кэш шейдеров очищен. Следующий запуск несколько '
                                                 'минут будет подтормаживать, пока кэш собирается заново.'))

    def shortcut(self):
        """God of War III.lnk on the desktop (a .desktop entry in the menu and on the desktop on Linux)."""
        if not WINDOWS:
            self.linux_shortcut()
            return
        if FROZEN:
            target, arguments, icon = sys.executable, '', f'{sys.executable},0'
        else:  # a source tree: the launcher script with the windowless Python
            pythonw = Path(sys.executable).with_name('pythonw.exe')
            target = str(pythonw if pythonw.exists() else sys.executable)
            arguments, icon = Path(__file__).resolve(), PORT_DIR / 'launcher' / 'gow3.ico'
        script = ('$s=(New-Object -ComObject WScript.Shell).CreateShortcut([Environment]::GetFolderPath("Desktop")'
                  '+"\\God of War III.lnk");'
                  f'$s.TargetPath="{target}";$s.WorkingDirectory="{PORT_DIR}";$s.IconLocation="{icon}";'
                  + (f"$s.Arguments='\"{arguments}\"';" if arguments else '') + '$s.Save()')
        result = subprocess.run(['powershell', '-NoProfile', '-Command', script], capture_output=True,
                                creationflags=NO_WINDOW)
        if result.returncode == 0:
            self.messagebox.showinfo(APP_NAME, _('Shortcut created on the desktop.', 'Ярлык создан на рабочем столе.'))
        else:
            self.messagebox.showerror(APP_NAME, result.stderr.decode(errors='replace')[:400])

    def linux_shortcut(self):
        launcher = os.environ.get('APPIMAGE') or (sys.executable if FROZEN else '')
        command = f'"{launcher}"' if launcher else f'python3 "{Path(__file__).resolve()}"'
        try:
            desktop = subprocess.run(['xdg-user-dir', 'DESKTOP'], capture_output=True, text=True,
                                     env=host_environment()).stdout.strip()
        except OSError:
            desktop = ''
        try:
            created = write_linux_shortcut(command, launcher, PORT_DIR / 'launcher' / 'gow3.png',
                                           Path(desktop) if desktop else Path.home() / 'Desktop')
        except OSError as error:
            self.messagebox.showerror(APP_NAME, str(error))
            return
        self.messagebox.showinfo(APP_NAME, _('Shortcut created:', 'Ярлык создан:') + '\n' + '\n'.join(created))

    def add_to_steam(self):
        """A non-Steam shortcut with the game's icon, capsules, hero and logo (scripts/steam_shortcut.py)."""
        if not self.collect():
            return
        game = self.var('game_dir', 'app').get()
        if not game_info(game):
            self.messagebox.showerror(APP_NAME, _('Choose the game folder with eboot.bin (CUSA01623).',
                                                  'Выберите папку игры с eboot.bin (CUSA01623).'))
            return
        self.root.configure(cursor='watch')
        self.root.update_idletasks()
        try:
            result = steam_shortcut.add_to_steam(game, steam_launch_command(), steam_icon(game))
        except steam_shortcut.SteamError as error:
            text = {'running': _('Steam is open. Close it completely (Steam > Exit) and click again: Steam '
                                 'rewrites its shortcuts when it closes.'),
                    'no_steam': _('Steam was not found for this user.'),
                    'no_art': _('The game folder has no sce_sys/pic1.png for the pictures.')}
            self.messagebox.showerror(APP_NAME, text.get(error.code, str(error)))
        except (OSError, ValueError, ImportError) as error:
            self.messagebox.showerror(APP_NAME, str(error))
        else:
            text = (_('The shortcut "{}" was updated in Steam (accounts: {}), with its icon, capsules, background '
                      'and logo. Open Steam to see it in the library.') if result['updated'] else
                    _('The shortcut "{}" was added to Steam (accounts: {}), with its icon, capsules, background '
                      'and logo. Open Steam to see it in the library.'))
            self.messagebox.showinfo(APP_NAME, text.format(steam_shortcut.APP_NAME, result['accounts']))
        finally:
            self.root.configure(cursor=ARROW)

    def close(self):
        if not self.collect():
            return
        self.root.destroy()


class GameJob:
    """A Windows job holding run.py and everything it starts: gow3-probe.exe and the launches made
    by the in-game restart (no longer descendants of the first process)."""

    def __init__(self, process):
        if not WINDOWS:
            self.group = process.pid  # started with start_new_session: its own process group
            self.handle = None
            return
        kernel32 = ctypes.windll.kernel32
        kernel32.CreateJobObjectW.restype = ctypes.c_void_p
        self.handle = kernel32.CreateJobObjectW(None, None)
        if self.handle:
            kernel32.AssignProcessToJobObject(ctypes.c_void_p(self.handle), ctypes.c_void_p(int(process._handle)))

    def terminate(self):
        if not WINDOWS:
            self.signal_group(signal.SIGTERM)
            threading.Timer(3, self.signal_group, args=(signal.SIGKILL,)).start()
            return
        if self.handle:
            ctypes.windll.kernel32.TerminateJobObject(ctypes.c_void_p(self.handle), 1)

    def signal_group(self, number):
        try:
            os.killpg(self.group, number)
        except (ProcessLookupError, PermissionError):
            pass

    def close(self):
        if self.handle:
            ctypes.windll.kernel32.CloseHandle(ctypes.c_void_p(self.handle))
            self.handle = None


def run_log_path(settings):
    """<saves folder>/last_run.log: the game's output, written by both launch paths."""
    return Path(settings.get('user_dir') or DATA_DIR / 'user') / 'last_run.log'


class _NullLog:
    """Stands in for the run log when its folder cannot be written: the game still starts."""
    def write(self, text):
        pass

    def close(self):
        pass

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


def open_run_log(settings):
    """The run log, truncated, line-buffered; a null log when the folder is unusable."""
    path = run_log_path(settings)
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        return open(path, 'w', encoding='utf-8', buffering=1)
    except (OSError, ValueError):
        return _NullLog()


def open_play_log(settings):
    """The run log for the PLAY button. With 'close_on_play' the launcher exits 5 s after the
    start and the reader thread dies with it: a truncated file would replace the previous full
    log, so that mode leaves the file alone."""
    if settings.get('close_on_play'):
        return _NullLog()
    return open_run_log(settings)


def exit_hint(line):
    """Advice for an output line that explains why the game closed, or None."""
    if 'of guest memory (error ' in line:
        return _('Windows could not reserve the memory the game needs. Close other programs, or let the '
                 'Windows page file grow (for example 16384 MB initial, 32768 MB maximum), and start again.',
                 'Windows не смог выделить память для игры. Закройте другие программы или дайте файлу '
                 'подкачки Windows расти (например, 16384 МБ исходный, 32768 МБ максимальный) и запустите снова.')
    if 'guest raised debug exception code=0xa0020001' in line:
        return _('The game stopped while reading its files: the game dump may be incomplete. Extract the pkg '
                 'again and check that the game folder is about 40 GB.',
                 'Игра остановилась при чтении своих файлов: дамп игры может быть неполным. Распакуйте pkg '
                 'заново и проверьте, что папка игры занимает около 40 ГБ.')
    return None


def _close_quietly(log):
    try:
        log.close()
    except (OSError, ValueError):
        pass


def play_without_window(settings):
    """--play: the game with the saved settings (shortcuts, Steam). Output goes to the console
    when there is one and to <saves folder>/last_run.log."""
    attach_stdio()
    with open_run_log(settings) as log:
        process = subprocess.Popen(run_command(), cwd=PORT_DIR, env=game_environment(settings),
                                   stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, creationflags=NO_WINDOW)
        for raw in iter(process.stdout.readline, b''):
            text = raw.decode('utf-8', errors='replace')
            log.write(text)
            try:
                sys.stdout.write(text)
            except (OSError, ValueError):
                pass
        return process.wait()


def version_tuple(text):
    return tuple(int(number) for number in re.findall(r'\d+', text or ''))


def latest_release():
    """(version, zip URL, page URL) of the newest GitHub release."""
    request = urllib.request.Request(RELEASES_API, headers={'Accept': 'application/vnd.github+json',
                                                            'User-Agent': 'gow3-launcher'})
    with urllib.request.urlopen(request, timeout=15) as response:
        release = json.load(response)
    version = '.'.join(re.findall(r'\d+', release['tag_name']))
    url = next((asset['browser_download_url'] for asset in release.get('assets', [])
                if asset.get('name', '').lower().endswith('.zip')), None)
    return version, url, release.get('html_url') or RELEASES_PAGE


def install_update(target, wait_pid):
    """--install-update TARGET PID, run by the downloaded version from its temporary folder:
    waits for the old launcher to close, copies this version over TARGET (never the saves,
    settings or mods) and starts it."""
    target = Path(target)
    kernel = ctypes.windll.kernel32
    handle = kernel.OpenProcess(0x00100000, False, int(wait_pid))  # SYNCHRONIZE
    if handle:
        kernel.WaitForSingleObject(handle, 60000)
        kernel.CloseHandle(handle)
    ignore = shutil.ignore_patterns(*USER_FILES)
    for attempt in range(30):
        try:
            shutil.copytree(PORT_DIR, target, dirs_exist_ok=True, ignore=ignore)
            break
        except OSError:  # a file still in use: the old launcher is closing
            time.sleep(1)
    else:
        ctypes.windll.user32.MessageBoxW(None, _(
            'Could not install the update. Download it from the releases page.',
            'Не удалось установить обновление. Скачайте его со страницы релизов.'), APP_NAME, 0x10)
        open_url(RELEASES_PAGE)
        return 1
    subprocess.Popen([str(target / 'God of War III.exe')], cwd=str(target))
    return 0


def main():
    global LANG, FONT_LOADED
    args = sys.argv[1:]
    if args and args[0] in ('--run', '--script'):
        sys.exit(run_role(args))
    settings = {**APP_DEFAULTS, **migrate_controls(load_json(CONFIG_FILE, {}))}
    LANG = settings.get('ui_language') or windows_language()
    if not WINDOWS:
        integrate_desktop(settings.get('desktop_integration', True), os.environ.get('APPIMAGE'),
                          PORT_DIR / 'launcher' / 'gow3.png', DATA_DIR)
    if WINDOWS and args[:1] == ['--install-update'] and len(args) == 3:
        sys.exit(install_update(args[1], args[2]))
    if WINDOWS and FROZEN and UPDATE_DIR not in PORT_DIR.parents:
        shutil.rmtree(UPDATE_DIR, ignore_errors=True)  # what a finished update left behind
    # Without a usable game folder there is nothing to play yet: open the launcher instead.
    if '--play' in args and ready_to_play(settings['game_dir'], settings.get('update_dir') or ''):
        sys.exit(play_without_window(settings))
    if WINDOWS:
        try:
            ctypes.windll.shcore.SetProcessDpiAwareness(1)  # sharp text on scaled displays
        except (AttributeError, OSError):
            pass
    FONT_LOADED = register_font(FONT_FILE)  # titles keep the usual serif without it
    import tkinter as tk
    from tkinter import filedialog, font, messagebox, ttk
    root = tk.Tk() if WINDOWS else tk.Tk(className='gow3')
    if not WINDOWS:
        root.configure(cursor=ARROW)
        for window in ('Toplevel', 'Dialog', 'TkFDialog', 'TkChooseDir'):  # dialogs: tk_messageBox, file pickers
            root.option_add(f'*{window}.cursor', ARROW)
    pick_fonts(font.families(root))
    Launcher(root, tk, ttk, filedialog, messagebox)
    root.mainloop()


if __name__ == '__main__':
    main()
