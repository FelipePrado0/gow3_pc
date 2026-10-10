#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""God of War III (gow3) launcher for Windows (Tkinter; launcher/gow3_launcher.py is the Linux one).

Every setting of the port in one window: the game folder and saves, gow3.ini (upscaler,
preset, output, effects), start-up options passed to run.py as environment variables
(frame rate, presentation, HDR, ...), mods, third-party patches and the FSR 4 assets.
Launcher options live in %APPDATA%/gow3-launcher/settings.json.

Frozen with PyInstaller (packaging/windows/package.sh) the same God of War III.exe also runs the
game without the window (`--play`), run.py (`--run`) and the preparation scripts (`--script`),
so a packaged port needs no Python installation.
"""
import ctypes
import json
import os
from pathlib import Path
import queue
import re
import runpy
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
import webbrowser
import zipfile

FROZEN = getattr(sys, 'frozen', False)
PORT_DIR = Path(sys.executable).resolve().parent if FROZEN else Path(__file__).resolve().parent.parent
sys.path.insert(0, str(PORT_DIR / 'scripts'))
import save_backup  # noqa: E402  (scripts/)
DATA_DIR = Path(os.environ.get('GOW3_DATA_DIR', PORT_DIR))
CONFIG_DIR = Path(os.environ.get('APPDATA', Path.home())) / 'gow3-launcher'
CONFIG_FILE = CONFIG_DIR / 'settings.json'
MOD_PRESETS = DATA_DIR / 'mods_presets.json'
APP_NAME = 'God of War III'
MAX_LOG_LINES = 6000
NO_WINDOW = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
# This build; GitHub release tags are windows-v<VERSION>.
VERSION = '1.5'
RELEASES_API = 'https://api.github.com/repos/FelipePrado0/gow3_pc/releases/latest'
RELEASES_PAGE = 'https://github.com/FelipePrado0/gow3_pc/releases/latest'
UPDATE_DIR = Path(tempfile.gettempdir()) / 'gow3-update'
# Never copied over an installation by an update (the package does not hold them either).
USER_FILES = ('user', 'out', 'mods', 'gow3.ini', 'mods.json', 'mods_presets.json', 'patches.json', 'last_run.log')


# ---------------------------------------------------------------------------------------------
# Command line roles of the frozen executable.

def attach_stdio():
    """A windowed executable starts without sys.stdout; inherited pipes or a console still exist."""
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
            stream.reconfigure(line_buffering=True)
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


# ---------------------------------------------------------------------------------------------
# Languages: every text is written in English with the Russian next to it; the other
# languages are in gow3_lang.py, keyed by the English text.

sys.path.insert(0, str(Path(__file__).resolve().parent))
import button_icons  # noqa: E402
import gow3_lang  # noqa: E402

LANG = 'en'


WARN = '  ⚠'  # marks a risky choice; the text before it is translated as usual


def _(en, ru=None):
    if en.endswith(WARN):
        return _(en[:-len(WARN)], ru and ru.removesuffix(WARN)) + WARN
    if LANG == 'ru':
        return ru or en
    return gow3_lang.table(LANG).get(en) or en


def windows_language():
    try:
        primary = ctypes.windll.kernel32.GetUserDefaultUILanguage() & 0x3ff
    except (AttributeError, OSError):
        return 'en'
    return gow3_lang.WINDOWS_LANGUAGES.get(primary, 'en')


# ---------------------------------------------------------------------------------------------
# Settings. gow3.ini keys (the game reads them, the in-game menu edits them) and the
# launcher's own settings.json (passed to run.py as environment variables).

INI_FLAGS = {'sharpen', 'object_motion', 'show_fps', 'vsync', 'async_shaders', 'deferred_readback',
             'stale_readback', 'fsr1', 'rcas'}
# The graphics keys are also in the in-game menu (Insert / R3+L2); run.py reads the startup ones.
INI_DEFAULTS = {'upscaler': 'fsr4', 'preset': '1', 'sharpen': '1', 'sharpness': '0.50',
                'object_motion': '1', 'show_fps': '1', 'output_res': '1920x1080',
                'live_resolution': 'auto', 'display_mode': 'windowed', 'vsync': '1', 'fps_limit': '0',
                'async_shaders': '1', 'deferred_readback': '1', 'stale_readback': '1',
                'render_resolution': 'native', 'engine_fps': '120', 'fsr1': '0', 'rcas': '1',
                'rcas_strength': '75', 'frames_queued': '1'}
DISPLAY_MODES = [('windowed', ('Windowed',)), ('borderless', ('Borderless',)), ('fullscreen', ('Fullscreen',))]
FPS_LIMITS = [('30', ('30',)), ('60', ('60',)), ('120', ('120',)), ('240', ('240',)), ('0', ('Unlimited',))]
APP_DEFAULTS = {'ui_language': '', 'game_dir': os.environ.get('GOW3_GAME_DIR', str(PORT_DIR.parent / 'CUSA01623')), 'user_dir': '',
                'mods_dir': '', 'mods_enabled': True, 'patches_dir': '', 'language': '1',
                'player_name': '', 'hdr': False, 'draw_pipe': '', 'readbacks': '',
                'background_warmup': True, 'parallel_warmup': True, 'perf_diag': False, 'frame_capture_key': False, 'frame_stats': False, 'gpu_profile': False,
                'vk_validation': False, 'close_on_play': False,
                'check_updates': False, 'game_patches': {}, 'controls': {}, 'pad_style': 'playstation'}

# PS4 button, default key, default gamepad button (SDL names; src/runtime_pad.c). L2/R2 come
# from the analog triggers and are not remapped on the gamepad.
CONTROLS = [('cross', 'Space', 'a'), ('circle', 'Left Shift', 'b'), ('square', 'E', 'x'),
            ('triangle', 'Q', 'y'), ('l1', '1', 'leftshoulder'), ('r1', '3', 'rightshoulder'),
            ('l2', 'R', None), ('r2', 'F', None), ('l3', 'Z', 'leftstick'),
            ('r3', 'C', 'rightstick'), ('options', 'Return', 'start'), ('touchpad', 'Tab', 'back'),
            ('up', 'I', 'dpup'), ('down', 'K', 'dpdown'), ('left', 'J', 'dpleft'), ('right', 'L', 'dpright')]
PS_BUTTON_NAMES = {'cross': 'Cross', 'circle': 'Circle', 'square': 'Square', 'triangle': 'Triangle',
                   'l1': 'L1', 'r1': 'R1', 'l2': 'L2', 'r2': 'R2', 'l3': 'L3', 'r3': 'R3',
                   'options': 'Options', 'touchpad': 'Touchpad', 'up': 'D-pad up', 'down': 'D-pad down',
                   'left': 'D-pad left', 'right': 'D-pad right'}
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


def control_changes(values, kind):
    """The buttons whose chosen key or SDL gamepad name differs from the default."""
    column = 1 if kind == 'key' else 2
    return {row[0]: values[row[0]].strip() for row in CONTROLS
            if row[column] and values.get(row[0], '').strip()
            and values[row[0]].strip().casefold() != row[column].casefold()}


def control_map(controls, kind):
    """GOW3_KEY_MAP/GOW3_PAD_MAP value: 'cross=Q,circle=Left Shift' for the changed buttons."""
    chosen = controls.get(kind, {}) if isinstance(controls, dict) else {}
    return ','.join(f'{button}={chosen[button].strip()}' for button, _key, _pad in CONTROLS
                    if isinstance(chosen.get(button), str) and chosen[button].strip()
                    and not set(chosen[button]) & set(',='))

UPSCALERS = [('dlss', ('DLSS (NVIDIA GeForce RTX)',)),
             ('fsr4', ('FSR 4 (best quality)', 'FSR 4 (лучшее качество)')),
             ('fsr411', ('FSR 4.1.1 (needs fsr4_411 assets)', 'FSR 4.1.1 (нужны ассеты fsr4_411)')),
             ('fsr3', ('FSR 3.1 (every GPU)', 'FSR 3.1 (любая видеокарта)')),
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
FRAMES_QUEUED = [('1', ('1 (lowest latency)',)), ('2', ('2',)), ('0', ('Unlimited',))]
UI_LANGUAGES = gow3_lang.LANGUAGE_NAMES

FSR4_COMMIT = 'ae8d628fae208813172446d1e49ed94150b04658'
FSR4_BASE = f'https://raw.githubusercontent.com/FireBurn/Q2RTX/{FSR4_COMMIT}/baseq2/fsr4_shaders'


def fsr4_files():
    """The FSR 4 v07 asset set of tools/fetch_fsr4_assets.sh (1080 and 2160 tiers)."""
    files = ['LICENSE-FSR4-v07.txt', 'rcas.spv', 'spd_auto_exposure.spv']
    for model in ('native', 'quality', 'balanced', 'performance', 'ultraperf', 'drs'):
        files += [f'fsr4_model_v07_i8_{model}_initializers.bin', f'fsr4_model_v07_i8_{model}_pre_weights.bin',
                  f'fsr4_model_v07_i8_{model}_shader_manifest.json']
        for tier in ('1080', '2160'):
            files += [f'fsr4_model_v07_i8_{model}_{tier}_pre.spv', f'fsr4_model_v07_i8_{model}_{tier}_post.spv']
            files += [f'fsr4_model_v07_i8_{model}_{tier}_pass{n}.spv' for n in range(1, 13)]
    return files


def fsr4_missing():
    folder = PORT_DIR / 'fsr4_shaders'
    return [name for name in fsr4_files() if not (folder / name).is_file() or not (folder / name).stat().st_size]


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
    ini_path().write_text('\n'.join(out) + '\n', encoding='utf-8')


def game_info(folder):
    """(title, app version) of a game folder, or None without eboot.bin."""
    folder = Path(folder or '.')
    if not (folder / 'eboot.bin').is_file():
        return None
    try:
        from prepare import sfo
        values = sfo((folder / 'sce_sys/param.sfo').read_bytes())
        return values.get('TITLE', APP_NAME).replace('™', '').replace('®', '').strip(), values.get('APP_VER', '?')
    except (OSError, ValueError, ImportError):
        return APP_NAME, '?'


def game_profile_of(folder):
    """(title id, (ids, built-in XML, app version)) of a game with a patch profile, else (id, None)."""
    from patches import game_profile, game_title_id
    title_id = game_title_id(folder) if folder else None
    return title_id, game_profile(title_id)


def game_environment(s):
    env = dict(os.environ)
    env['GOW3_GAME_DIR'] = s['game_dir']
    # The launcher's patch selection is the whole list (run.py, patches.py).
    title_id, profile = game_profile_of(s['game_dir'])
    chosen = s.get('game_patches', {}).get(title_id) if title_id else None
    if profile and chosen is not None:
        env['GOW3_PATCHES'] = ';'.join(chosen)
        env['GOW3_PATCHES_ONLY'] = '1'
    if s['user_dir']:
        env['GOW3_USER_DIR'] = s['user_dir']
    env['GOW3_MODS_DIR'] = s['mods_dir'] or str(DATA_DIR / 'mods')
    env['GOW3_MODS_CONFIG'] = str(DATA_DIR / 'mods.json')
    env['GOW3_MODS_ENABLED'] = '1' if s['mods_enabled'] else '0'
    env['GOW3_PATCHES_DIR'] = s['patches_dir'] or str(DATA_DIR / 'patches')
    env['GOW3_PATCHES_CONFIG'] = str(DATA_DIR / 'patches.json')
    env['GOW3_LANGUAGE'] = s['language']
    if str(s['player_name']).strip():
        env['GOW3_USER_NAME'] = str(s['player_name']).strip()
    if s['hdr']:
        env['GOW3_HDR'] = '1'
    for key, name in (('draw_pipe', 'GOW3_DRAW_PIPE'), ('readbacks', 'GOW3_READBACKS')):
        if s[key]:
            env[name] = s[key]
    env['GOW3_BACKGROUND_WARMUP'] = '1' if s.get('background_warmup', True) else '0'
    env['GOW3_PARALLEL_WARMUP'] = '1' if s.get('parallel_warmup', True) else '0'
    env['GOW3_PERF_DIAG'] = '1' if s.get('perf_diag', False) else '0'
    for key, name in (('frame_stats', 'GOW3_FRAME_STATS'), ('gpu_profile', 'GOW3_GPU_PROFILE'),
                      ('vk_validation', 'GOW3_VK_VALIDATION'), ('frame_capture_key', 'GOW3_FRAME_CAPTURE_KEY')):
        if s[key]:
            env[name] = '1'
    for kind, name in (('key', 'GOW3_KEY_MAP'), ('pad', 'GOW3_PAD_MAP')):
        if value := control_map(s.get('controls', {}), kind):
            env[name] = value
    env['PYTHONUNBUFFERED'] = '1'
    env['PYTHONIOENCODING'] = 'utf-8'
    return env


# ---------------------------------------------------------------------------------------------
# The window.

# God of War III's own colors: near black, ash and blood red (no gold or orange).
BG, PANEL, CARD, LINE = '#0d0b0b', '#161212', '#1d1717', '#2a2222'
TEXT, MUTED, ACCENT, ACCENT_HI = '#e6e0d8', '#8a817a', '#a4161a', '#c81e25'
RESOLUTION_CHOICES = [('native', ('Native (1080p)',)), ('480p', ('480p',)), ('720p', ('720p',)),
                      ('1440p', ('1440p',)), ('1800p', ('1800p',)), ('4K', ('4K',))]
ENGINE_FPS_CHOICES = [('60', ('60 (original)',)), ('120', ('120',)), ('240', ('240',))]


class Launcher:
    def __init__(self, root, tk, ttk, filedialog, messagebox):
        self.tk, self.ttk, self.filedialog, self.messagebox = tk, ttk, filedialog, messagebox
        self.root = root
        self.app = {**APP_DEFAULTS, **load_json(CONFIG_FILE, {})}
        self.ini, self.ini_lines = load_ini()
        self.vars = {}
        self.process = self.job = None
        self.downloading = False
        self.output = queue.Queue()
        self.banner_source = self.banner_image = None
        self.ui_calls = queue.Queue()  # work for the Tk thread from helper threads
        self.mod_order, self.mod_vars, self.patch_vars = [], {}, {}
        root.title(f'{APP_NAME} (PC)')
        if sys.platform == 'win32':
            try:  # the taskbar shows this window's icon, not Python's
                import ctypes
                ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID('gow3pc.launcher')
            except (AttributeError, OSError):
                pass
        root.configure(bg=BG)
        self.dpi = root.winfo_fpixels('1i') / 96.0
        root.geometry(f'{self.px(1120)}x{self.px(740)}')
        root.minsize(self.px(980), self.px(660))
        self.set_icon()
        self.style()
        self.build()
        self.show('home')
        root.protocol('WM_DELETE_WINDOW', self.close)
        root.after(100, self.drain_output)
        if self.app.get('check_updates', True):
            threading.Thread(target=self.check_update, daemon=True).start()

    def px(self, size):
        return int(size * self.dpi)

    def set_icon(self):
        game_icon = Path(self.var('game_dir', 'app').get() or '.') / 'sce_sys' / 'icon0.png'
        try:
            from PIL import Image
            ico = CONFIG_DIR / 'game_icon.ico'
            CONFIG_DIR.mkdir(parents=True, exist_ok=True)
            with Image.open(game_icon) as image:
                image.convert('RGBA').save(ico, sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (256, 256)])
            self.root.iconbitmap(default=str(ico))
            return
        except (ImportError, OSError, self.tk.TclError):
            pass
        for icon in (game_icon, PORT_DIR / 'launcher' / 'gow3.png'):
            try:
                self.icon = self.tk.PhotoImage(file=str(icon))
                if self.icon.width() > 128:
                    self.icon = self.icon.subsample(self.icon.width() // 64)
                self.root.iconphoto(True, self.icon)
                return
            except (self.tk.TclError, OSError):
                continue

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
        self.check_images()
        s.element_create('Bb.indicator', 'image', self.check_off, ('selected', self.check_on), sticky='')
        s.layout('TCheckbutton', [('Checkbutton.padding', {'sticky': 'nswe', 'children': [
            ('Bb.indicator', {'side': 'left', 'sticky': ''}),
            ('Checkbutton.focus', {'side': 'left', 'sticky': 'w', 'children': [
                ('Checkbutton.label', {'sticky': 'nswe'})]})]})])
        base = ('Segoe UI', 10)
        self.root.option_add('*TCombobox*Listbox.background', CARD)
        self.root.option_add('*TCombobox*Listbox.foreground', TEXT)
        self.root.option_add('*TCombobox*Listbox.selectBackground', ACCENT)
        self.root.option_add('*TCombobox*Listbox.font', base)
        s.configure('.', background=PANEL, foreground=TEXT, fieldbackground=CARD, bordercolor=LINE,
                    lightcolor=LINE, darkcolor=LINE, troughcolor=CARD, focuscolor=ACCENT, font=base)
        s.configure('TFrame', background=PANEL)
        s.configure('TLabel', background=PANEL, foreground=TEXT)
        s.configure('Muted.TLabel', background=PANEL, foreground=MUTED, font=('Segoe UI', 9))
        s.configure('Restart.TLabel', background=PANEL, foreground=ACCENT_HI, font=('Segoe UI', 8, 'bold'))
        s.configure('Section.TLabel', background=PANEL, foreground=ACCENT_HI, font=('Georgia', 12))
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
        s.configure('Play.TButton', background=ACCENT, foreground='#f4ece0', font=('Georgia', 15, 'bold'),
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
                elif key == 'sharpness':
                    v = tk.DoubleVar(value=float(value or 0.5))
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

    def choice(self, parent, key, store, options, width=30):
        """A combobox over (value, (english, russian)) pairs, kept in sync with its variable."""
        var = self.var(key, store)
        values = [v for v, _t in options]
        box = self.ttk.Combobox(parent, values=[_(*text) for _v, text in options], state='readonly', width=width)
        if var.get() not in values:
            var.set(values[0])

        def show(*_args):
            current = var.get()
            box.current(values.index(current) if current in values else 0)
        show()
        box.bind('<<ComboboxSelected>>', lambda _e: var.set(values[box.current()]))
        var.trace_add('write', show)
        return box

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
        r = self.next_row(parent)
        self.ttk.Label(parent, text=title.upper(), style='Section.TLabel').grid(
            row=r, column=0, columnspan=2, sticky='w', pady=(top, 0))
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
        ttk.Label(head, text=title.upper(), background=PANEL, foreground=TEXT, font=('Georgia', 20)).pack(anchor='w')
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
        self.side_title = tk.Label(side, text=APP_NAME.upper(), bg=BG, fg=TEXT, font=('Georgia', 15),
                                   wraplength=self.px(190), justify='left')
        self.side_title.pack(anchor='w', padx=22, pady=(24, 22))
        self.side = side
        self.nav, self.current_page = {}, None
        for name, title in (('home', _('Home')), ('display', _('Display')), ('performance', _('Performance')),
                            ('game', _('Game', 'Игра')), ('patches', _('Patches', 'Патчи')),
                            ('mods', _('Mods', 'Моды')), ('controls', _('Controls', 'Управление')),
                            ('advanced', _('Advanced', 'Дополнительно')), ('log', _('Log', 'Журнал'))):
            item = tk.Label(side, text='    ' + title, bg=BG, fg=TEXT, anchor='w', font=('Segoe UI', 11),
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
                                  bg=BG, fg=MUTED, font=('Segoe UI', 9), justify='left', wraplength=self.px(210))
        self.side_note.pack(anchor='w', padx=22, pady=(0, 18))

        right = tk.Frame(self.root, bg=PANEL)
        right.pack(side='left', fill='both', expand=True)
        bar = tk.Frame(right, bg=BG, height=72)
        bar.pack(fill='x', side='bottom')
        bar.pack_propagate(False)
        self.status = tk.Label(bar, text='', bg=BG, fg=MUTED, font=('Segoe UI', 10), anchor='w', justify='left')
        self.status.pack(side='left', padx=24)
        self.play_button = ttk.Button(bar, text=_('PLAY', 'ИГРАТЬ'), style='Play.TButton', command=self.play)
        self.play_button.pack(side='right', padx=(10, 24), pady=11)
        self.content = tk.Frame(right, bg=PANEL)
        self.content.pack(fill='both', expand=True)

        self.pages = {}
        self.build_home()
        self.build_graphics()
        self.build_display()
        self.build_performance()
        self.build_game()
        self.build_patches()
        self.build_mods()
        self.build_controls()
        self.build_advanced()
        self.build_log()
        self.root.bind_all('<MouseWheel>', self.wheel)
        self.game_changed()

    def wheel(self, event):
        page = self.pages.get(self.current_page)
        if hasattr(page, 'scroll') and not isinstance(event.widget, (self.tk.Text, self.ttk.Combobox)) \
                and 'popdown' not in str(event.widget):
            page.scroll(int(-event.delta / 120))

    def show(self, name):
        for page in self.pages.values():
            page.pack_forget()
        self.pages[name].pack(fill='both', expand=True)
        self.current_page = name
        for n, item in self.nav.items():
            item.configure(bg=PANEL if n == name else BG, fg=ACCENT_HI if n == name else TEXT)
        if name in ('mods', 'patches'):
            self.refresh_lists()
        if name == 'patches':
            self.refresh_game_patches()
        elif name == 'graphics':
            self.refresh_fsr4()
        elif name == 'home':
            self.refresh_status()

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
        ttk.Label(quick, text=_('Quick settings', 'Основное').upper(), style='Section.TLabel').grid(
            row=0, column=0, columnspan=4, sticky='w')
        tk.Frame(quick, bg=LINE, height=1).grid(row=1, column=0, columnspan=4, sticky='we', pady=(2, 6))
        items = ((_('Display mode'), 'display_mode', DISPLAY_MODES), (_('Frame rate limit'), 'fps_limit', FPS_LIMITS),
                 (_('Render resolution'), 'render_resolution', RESOLUTION_CHOICES),
                 (_('Engine frame rate'), 'engine_fps', ENGINE_FPS_CHOICES))
        for index, (title, key, options) in enumerate(items):
            r, c = 2 + index // 2, (index % 2) * 2
            ttk.Label(quick, text=title).grid(row=r, column=c, sticky='w', pady=6, padx=(0, 12))
            self.choice(quick, key, 'ini', options, width=18).grid(row=r, column=c + 1, sticky='w', padx=(0, 40))

    def draw_banner(self):
        """The cover art of the selected dump (sce_sys/pic1.png) under the title."""
        tk, c = self.tk, self.banner
        c.delete('all')
        w, h = max(c.winfo_width(), 400), int(c['height'])
        folder = self.var('game_dir', 'app').get()
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
            for i, stipple in enumerate(('gray12', 'gray25', 'gray50', 'gray75')):
                c.create_rectangle(0, h - 120 + i * 24, w, h - 96 + i * 24, fill=PANEL, outline='', stipple=stipple)
            c.create_rectangle(0, h - 24, w, h, fill=PANEL, outline='')
        else:
            c.create_text(30, h - 74, text=APP_NAME.upper(), anchor='w', fill=TEXT, font=('Georgia', 32))
        if info:
            title_id = game_profile_of(folder)[0]
            sub = _('{} · version {}', '{} · версия {}').format(title_id or '?', info[1])
            c.create_text(33, h - 30, text=sub, anchor='w', fill=ACCENT_HI, font=('Segoe UI', 11, 'bold'))
        else:
            c.create_text(w // 2, h // 2 - 40, text=_('Choose your game folder to play.'),
                          fill=TEXT, font=('Segoe UI', 12))
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
        """Temporal upscalers: not calibrated for this game yet (run.py turns them off), so this page
        stays out of the menu."""
        ttk = self.ttk
        f = self.scrolled_page('graphics', _('Graphics', 'Графика'),
                               _('Stored in gow3.ini; the in-game menu (Insert or R3+L2) changes the same values.',
                                 'Хранится в gow3.ini; в игре меняется через меню (Insert или R3+L2).'))
        self.section(f, _('Upscaling', 'Апскейлинг'), top=4)
        self.row(f, _('Upscaler', 'Апскейлер'), self.choice(f, 'upscaler', 'ini', UPSCALERS),
                 _("Temporal upscaling with the game's own motion vectors. FSR 4 needs its assets (below) and "
                   'a GPU with INT8 dot products; otherwise the game falls back to FSR 3.1 by itself.',
                   'Временной апскейлинг с векторами движения игры. FSR 4 нужны ассеты (ниже) и GPU с INT8; '
                   'иначе игра сама переключится на FSR 3.1.'))
        self.row(f, _('Quality preset', 'Пресет'), self.choice(f, 'preset', 'ini', PRESETS),
                 _('Render scale per axis: Quality renders at 1/1.5 of the output size.',
                   'Масштаб рендера по каждой оси: Quality рисует в 1/1.5 размера вывода.'))
        self.row(f, _('Output resolution', 'Разрешение вывода'), self.choice(f, 'output_res', 'ini', OUTPUTS),
                 _('What the upscaler produces; the HUD is drawn at this size too.',
                   'Что выдаёт апскейлер; интерфейс рисуется в этом же размере.'))
        self.row(f, _('Live resolution changes', 'Смена разрешения на лету'), self.choice(f, 'live_resolution', 'ini', LIVE),
                 _('Off: outputs other than 1080p are set by a patch at start (fastest; changing them in the '
                   'game restarts it). On: change output and preset in the game without a restart, at a cost.',
                   'Выкл.: разрешения кроме 1080p задаются патчем при запуске (быстрее). Вкл.: менять в игре '
                   'без перезапуска, но медленнее.'))
        self.check(f, 'sharpen', 'ini', _('Sharpening (RCAS)', 'Резкость (RCAS)'))
        holder = ttk.Frame(f)
        ttk.Scale(holder, from_=0.0, to=2.0, variable=self.var('sharpness', 'ini'), length=300).pack(side='left')
        value = ttk.Label(holder, width=5)
        value.pack(side='left', padx=10)
        show = lambda *_a: value.configure(text=f'{self.vars["sharpness"].get():.2f}')
        self.vars['sharpness'].trace_add('write', show)
        show()
        self.row(f, _('Sharpness', 'Сила резкости'), holder)
        self.check(f, 'object_motion', 'ini', _('Object motion vectors', 'Векторы движения объектов'),
                   _('Less ghosting on characters, cloth and weapons; costs about 10% FPS.',
                     'Меньше гостинга на персонажах и одежде; стоит около 10% FPS.'))
        self.section(f, _('FSR 4 assets', 'Ассеты FSR 4'))
        self.fsr4_label = ttk.Label(f, text='', wraplength=self.px(640), justify='left')
        self.fsr4_label.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w')
        holder = ttk.Frame(f)
        holder.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(8, 0))
        self.fsr4_button = ttk.Button(holder, text=_('Download FSR 4 assets', 'Скачать ассеты FSR 4'),
                                      command=self.download_fsr4)
        self.fsr4_button.pack(side='left')
        self.fsr4_progress = ttk.Progressbar(holder, length=280, maximum=len(fsr4_files()))
        self.fsr4_progress.pack(side='left', padx=12)
        self.note(f, _("From FireBurn/Q2RTX on GitHub (built from AMD's MIT-licensed FidelityFX source), "
                       'about 30 MB, into the fsr4_shaders folder of the port.',
                       'С GitHub FireBurn/Q2RTX (собраны из MIT-исходников AMD FidelityFX), около 30 МБ, '
                       'в папку fsr4_shaders порта.'), top=6)

    def build_display(self):
        tk, ttk = self.tk, self.ttk
        f = self.scrolled_page('display', _('Display'), _('How the game appears on your screen.'))
        self.section(f, _('Window', 'Окно'), top=4)
        self.row(f, _('Display mode'), self.choice(f, 'display_mode', 'ini', DISPLAY_MODES),
                 _('Borderless covers the screen without changing the display mode.'))
        self.check(f, 'vsync', 'ini', _('VSync'), _('Off allows tearing for the lowest latency.'))
        self.check(f, 'hdr', 'app', _('Allow HDR output', 'Разрешить HDR'),
                   _('When HDR is on in Windows and the display supports it.',
                     'Если HDR включён в Windows и монитор его поддерживает.'))
        self.section(f, _('Scaling and sharpening'))
        self.check(f, 'fsr1', 'ini', _('Upscale with FSR 1'),
                   _('Enlarges a smaller game image to the window. With Render resolution 720p (Performance) '
                     'it raises the FPS; at the size of the window it does nothing.'))
        self.check(f, 'rcas', 'ini', _('Sharpening (RCAS)'))
        holder = ttk.Frame(f)
        strength = self.var('rcas_strength', 'ini')
        tk.Scale(holder, from_=0, to=100, orient='horizontal', variable=strength, showvalue=False, length=self.px(260),
                 width=self.px(12), sliderlength=self.px(18), bg=ACCENT, troughcolor=CARD, activebackground=ACCENT_HI,
                 highlightthickness=0, bd=0, sliderrelief='flat').pack(side='left')
        value = ttk.Label(holder, width=5)
        value.pack(side='left', padx=10)
        show = lambda *_a: value.configure(text=f'{strength.get()}%')
        strength.trace_add('write', show)
        show()
        self.row(f, _('Sharpening strength'), holder)
        self.section(f, _('Overlay'))
        self.check(f, 'show_fps', 'ini', _('Show performance overlay'),
                   _('Items, size and corner: in-game menu (Insert / R3+L2) > Overlay.'))

    def build_performance(self):
        f = self.scrolled_page('performance', _('Performance'), _('Frame rate and speed options.'))
        self.section(f, _('Frame rate', 'Частота кадров'), top=4)
        self.row(f, _('Frame rate limit'), self.choice(f, 'fps_limit', 'ini', FPS_LIMITS),
                 _('The engine frame rate (below) is the highest it can go.'))
        self.row(f, _('Frames queued'), self.choice(f, 'frames_queued', 'ini', FRAMES_QUEUED),
                 _('1 is the lowest input latency; more can raise FPS when the graphics card is the limit.'))
        self.row(f, _('Engine frame rate'), self.choice(f, 'engine_fps', 'ini', ENGINE_FPS_CHOICES),
                 _('The highest frame rate the game itself runs at (a game patch).'), restart=True)
        self.row(f, _('Render resolution'), self.choice(f, 'render_resolution', 'ini', RESOLUTION_CHOICES),
                 _('The size the game draws at; higher is sharper and slower. A resolution patch replaces the '
                   'texture fix and reserves the memory it needs.'), restart=True)
        self.section(f, _('Shaders'))
        self.check(f, 'background_warmup', 'app', _('Load shader cache in the background'),
                   _('The game opens at once and the saved shaders load while you play; a small note in the '
                     'corner shows the progress. Switch it off to wait on the loading screen instead.'))
        self.check(f, 'async_shaders', 'ini', _('Compile new shaders in the background'),
                   _('Fewer stutters in new areas: the game keeps running while new shaders compile. An object '
                     'or effect may appear a moment later. Switch it off if something stays missing.'))
        self.check(f, 'parallel_warmup', 'app', _('Fast shader cache loading (parallel)'),
                   _('Loading screen only: builds the cached pipelines on several CPU threads.'))
        self.section(f, _('GPU data'))
        self.check(f, 'stale_readback', 'ini', _('Game reads GPU data without waiting (big FPS gain)'),
                   _('About 40 to 66 FPS in a heavy scene. Needs deferred GPU readbacks. Switch it off if lighting '
                     'or objects look wrong.'), restart=True)
        self.check(f, 'deferred_readback', 'ini', _('Deferred GPU readbacks'),
                   _('Fewer waits for the GPU. Switch it off if textures or shadows look wrong.'), restart=True)
        self.row(f, _('Two-stage GPU pipeline', 'Двухстадийный конвейер GPU'), self.choice(f, 'draw_pipe', 'app', DRAW_PIPE),
                 _('20–30% faster; switch it off if the game is unstable.', 'Быстрее на 20–30%; при нестабильности выключите.'))
        self.row(f, _('GPU readbacks', 'Чтение данных GPU'), self.choice(f, 'readbacks', 'app', READBACKS),
                 _('How exactly data the GPU writes is copied back for the game.',
                   'Насколько точно данные, записанные GPU, возвращаются игре.'))

    def build_game(self):
        f = self.scrolled_page('game', _('Game', 'Игра'), _('Your game dump, saves and language.'))
        self.section(f, _('Folders'), top=4)
        self.folder(f, 'game_dir', _('Game folder', 'Папка игры'),
                    _('Choose the folder with eboot.bin', 'Выберите папку с eboot.bin'),
                    _('Your own dump of the game (eboot.bin, sce_module, sce_sys); God of War III Remastered: '
                      'CUSA01623, version 01.02 for the community patches.',
                      'Ваш дамп игры (eboot.bin, sce_module, sce_sys); God of War III Remastered: CUSA01623, '
                      'версия 01.02 для патчей сообщества.'), on_change=self.game_changed)
        self.folder(f, 'user_dir', _('Saves folder', 'Папка сохранений'),
                    _('Choose the saves folder', 'Выберите папку сохранений'),
                    _('Empty: {} (shader caches are kept there too).',
                      'Пусто: {} (там же кэш шейдеров).').format(DATA_DIR / 'user'), on_change=self.refresh_status)
        self.section(f, _('Saves'))
        self.build_save_backups(f)
        self.section(f, _('Language'))
        self.row(f, _('Game language', 'Язык игры'), self.choice(f, 'language', 'app', LANGUAGES))

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

    def build_patches(self):
        """Community patches of the selected game's built-in XML and third-party patch files.
        Resolution and frame rate patches follow Performance (gow3.ini; run.py adds them)."""
        f = self.scrolled_page('patches', _('Patches', 'Патчи'), _('Changes applied to the game code at start.'))
        self.section(f, _('Game patches', 'Патчи игры'), top=4)
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
        from patches import EXCLUSIVE_PREFIX
        tk, f = self.tk, self.game_patch_frame
        self.store_game_patches()
        for widget in f.winfo_children():
            widget.destroy()
        f.columnconfigure(1, weight=1)
        folder = self.var('game_dir', 'app').get()
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
            if name.startswith(EXCLUSIVE_PREFIX) or name.startswith('Frame Rate Patch'):
                continue
            var = tk.BooleanVar(value=name in on)
            self.game_patch_vars[name] = var
            author = meta.get('Author')
            self.check(f, None, None, name + (f'  ({author})' if author else ''), meta.get('Note'), var=var)
        self.game_patch_title = title_id

    def store_game_patches(self):
        """The page's selection into settings (the whole list for this game, without the resolution
        and frame rate patches: run.py adds those from gow3.ini)."""
        if not self.game_patch_title:
            return
        self.app.setdefault('game_patches', {})[self.game_patch_title] = \
            [n for n, v in self.game_patch_vars.items() if v.get()]

    def build_mods(self):
        f = self.scrolled_page('mods', _('Mods', 'Моды'),
                               _('Files layered over the game at start; the game files are never changed.'))
        self.check(f, 'mods_enabled', 'app', _('Load mods', 'Загружать моды'),
                   _('Loose-file mods, each in its own folder with dvdroot_ps4.',
                     'Моды из файлов, каждый в своей папке с dvdroot_ps4.'))
        self.folder(f, 'mods_dir', _('Mods folder', 'Папка модов'), _('Choose the mods folder', 'Выберите папку модов'),
                    _('Empty: {}', 'Пусто: {}').format(DATA_DIR / 'mods'), on_change=self.refresh_lists)
        self.section(f, _('Mods', 'Моды'))
        self.mods_frame = self.ttk.Frame(f)
        self.mods_frame.grid(row=self.next_row(f), column=0, columnspan=2, sticky='we', pady=(8, 0))
        self.ttk.Button(f, text=_('Refresh', 'Обновить'), command=self.refresh_lists).grid(
            row=self.next_row(f), column=0, sticky='w', pady=(14, 0))

    def build_controls(self):
        """Remapping of the PS4 buttons (run.py passes GOW3_KEY_MAP and GOW3_PAD_MAP to the game)."""
        tk, ttk = self.tk, self.ttk
        f = self.scrolled_page('controls', _('Controls', 'Управление'),
                               _('Which key or gamepad button presses each button of the game.'))
        saved = self.app.get('controls') if isinstance(self.app.get('controls'), dict) else {}
        style = self.app.get('pad_style') if self.app.get('pad_style') in PAD_STYLES else 'playstation'
        self.pad_style = tk.StringVar(value=style)
        names = ttk.Frame(f)
        for value, label in (('playstation', 'PlayStation'), ('xbox', 'Xbox')):
            ttk.Radiobutton(names, text=label, value=value, variable=self.pad_style,
                            command=self.relabel_pad_controls).pack(side='left', padx=(0, 14))
        self.row(f, _('Gamepad names'), names)
        table = ttk.Frame(f)
        table.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(16, 0))
        for column, span, title in ((0, 2, _('GAME BUTTON')), (2, 1, _('KEYBOARD')), (3, 2, _('GAMEPAD'))):
            ttk.Label(table, text=title, style='Section.TLabel').grid(row=0, column=column, columnspan=span, sticky='w')
        self.control_vars, self.pad_boxes, self.pad_icons = {'key': {}, 'pad': {}}, {}, {}
        size = self.px(28)
        labels = PAD_STYLES[style]

        def icon_canvas(parent):
            return tk.Canvas(parent, width=size, height=size, bg=PANEL, highlightthickness=0, bd=0)
        for r, (button, key, pad) in enumerate(CONTROLS, start=1):
            game_icon = icon_canvas(table)
            game_icon.grid(row=r, column=0, sticky='w', pady=3)
            button_icons.draw(game_icon, button_icons.GAME_BUTTONS[button], 'playstation', size)
            ttk.Label(table, text=PS_BUTTON_NAMES[button], width=14).grid(row=r, column=1, sticky='w', padx=(0, 12))
            key_var = tk.StringVar(value=saved.get('key', {}).get(button) or key)
            self.control_vars['key'][button] = key_var
            ttk.Entry(table, textvariable=key_var, width=16).grid(row=r, column=2, sticky='w', padx=(0, 18))
            pad_name = saved.get('pad', {}).get(button) or pad
            pad_var = tk.StringVar(value=pad_name or '')
            self.control_vars['pad'][button] = pad_var
            pad_icon = icon_canvas(table)
            pad_icon.grid(row=r, column=3, sticky='w', padx=(0, 6))
            self.pad_icons[button] = pad_icon
            if pad:
                box = ttk.Combobox(table, state='readonly', width=22, values=list(labels.values()))
                box.set(labels.get(pad_name, pad_name))

                def chosen(_e, b=box, v=pad_var, n=button):
                    v.set(self.pad_name(b.get()))
                    self.draw_pad_icon(n)
                box.bind('<<ComboboxSelected>>', chosen)
                self.pad_boxes[button] = box
            else:
                box = ttk.Label(table, text=_('{} trigger').format(PS_BUTTON_NAMES[button]), style='Muted.TLabel')
            box.grid(row=r, column=4, sticky='w')
            self.draw_pad_icon(button)
        self.note(f, _('Keys use SDL names: letters, digits, Space, Return, Tab, Left Shift, Left Ctrl, Left Alt, '
                       'Backspace, Up, Down, Left, Right, F1 to F12. A name the game does not know keeps the default '
                       '(the log says which). L2/R2 on the gamepad stay on the triggers. Insert and R3+L2 always open '
                       'the port\'s menu. Applied when the game starts.'))
        self.ttk.Button(f, text=_('Reset to defaults'), command=self.reset_controls).grid(
            row=self.next_row(f), column=0, sticky='w', pady=(14, 0))

    def draw_pad_icon(self, button):
        """The gamepad column's icon of a game button, in the chosen style (L2/R2: the triggers)."""
        name = self.control_vars['pad'][button].get() or button_icons.GAME_BUTTONS[button]
        button_icons.draw(self.pad_icons[button], name, self.pad_style.get(), self.px(28))

    def pad_name(self, label):
        """SDL gamepad button name of a label in the current naming style."""
        return next((n for n, l in PAD_STYLES[self.pad_style.get()].items() if l == label), label)

    def relabel_pad_controls(self):
        labels = PAD_STYLES[self.pad_style.get()]
        for button, box in self.pad_boxes.items():
            box.configure(values=list(labels.values()))
            name = self.control_vars['pad'][button].get()
            box.set(labels.get(name, name))
        for button in self.pad_icons:
            self.draw_pad_icon(button)

    def reset_controls(self):
        for button, key, pad in CONTROLS:
            self.control_vars['key'][button].set(key)
            self.control_vars['pad'][button].set(pad or '')
        self.relabel_pad_controls()

    def build_advanced(self):
        tk, ttk = self.tk, self.ttk
        f = self.scrolled_page('advanced', _('Advanced', 'Дополнительно'),
                               _('Launcher options, files and diagnostics.'))
        self.section(f, _('Launcher', 'Лаунчер'), top=4)
        self.row(f, _('Launcher language', 'Язык лаунчера'), self.choice(f, 'ui_language', 'app', UI_LANGUAGES),
                 _('Applies when the launcher opens again.', 'Применится при следующем открытии лаунчера.'))
        self.check(f, 'close_on_play', 'app', _('Close the launcher when the game starts', 'Закрывать лаунчер при запуске игры'))
        self.check(f, 'check_updates', 'app', _('Check for updates when the launcher opens',
                                                'Проверять обновления при открытии лаунчера'))
        holder = ttk.Frame(f)
        holder.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(10, 0))
        ttk.Button(holder, text=_('Check for updates', 'Проверить обновления'),
                   command=lambda: threading.Thread(target=self.check_update, args=(True,), daemon=True).start()
                   ).pack(side='left')
        ttk.Button(holder, text=_('Desktop shortcut', 'Ярлык на рабочем столе'), command=self.shortcut).pack(side='left', padx=6)
        ttk.Label(holder, text=f'v{VERSION}', style='Muted.TLabel').pack(side='left', padx=10)
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
            toggle.configure(text=('▾ ' if shown.get() else '▸ ') + _('Diagnostics (for development)').upper())
            if shown.get():
                body.grid(row=body_row, column=0, columnspan=2, sticky='we')
            else:
                body.grid_forget()
        toggle.bind('<Button-1>', flip)
        shown.set(True)
        flip()

    def build_log(self):
        tk, ttk = self.tk, self.ttk
        page = ttk.Frame(self.content, padding=(20, 14, 20, 8))
        self.pages['log'] = page
        top = ttk.Frame(page)
        top.pack(fill='x', pady=(0, 8))
        ttk.Label(top, text=_('Log', 'Журнал').upper(), font=('Georgia', 20)).pack(side='left')
        ttk.Button(top, text=_('Copy', 'Копировать'), command=self.copy_log).pack(side='right')
        ttk.Button(top, text=_('Clear', 'Очистить'), command=lambda: self.set_log('')).pack(side='right', padx=6)
        self.log = tk.Text(page, wrap='none', bg='#0a0808', fg='#cfc6bc', insertbackground=TEXT, relief='flat',
                           font=('Consolas', 9), padx=8, pady=6, state='disabled', highlightthickness=0)
        bar = ttk.Scrollbar(page, command=self.log.yview)
        self.log.configure(yscrollcommand=bar.set)
        bar.pack(side='right', fill='y')
        self.log.pack(fill='both', expand=True)

    # ---- state -------------------------------------------------------------------------------
    def game_changed(self):
        folder = self.var('game_dir', 'app').get()
        info = game_info(folder)
        title = info[0] if info else APP_NAME
        self.root.title(f'{title} (PC)')
        self.side_title.configure(text=title.upper())
        if self.current_page == 'patches':
            self.refresh_game_patches()
        self.banner_source = None
        self.draw_banner()
        self.set_icon()
        self.refresh_status()

    def refresh_status(self):
        info = game_info(self.var('game_dir', 'app').get())
        if not self.process:
            self.play_button.configure(text=_('PLAY', 'ИГРАТЬ'), state='normal' if info else 'disabled')
            self.status.configure(text=self.summary() if info else _('Choose the game folder first.',
                                                                     'Сначала выберите папку игры.'), fg=MUTED)

    def summary(self):
        self.store_game_patches()
        title_id, _profile = game_profile_of(self.var('game_dir', 'app').get())
        names = self.app.get('game_patches', {}).get(title_id) or []
        resolution = dict((v, t[0]) for v, t in RESOLUTION_CHOICES).get(self.var('render_resolution', 'ini').get(), '?')
        return _('{} · {} FPS engine · {} patches').format(resolution, self.var('engine_fps', 'ini').get(), len(names))

    def refresh_fsr4(self):
        total, missing = len(fsr4_files()), len(fsr4_missing())
        self.fsr4_progress.configure(value=total - missing)
        self.fsr4_label.configure(
            text=_('Installed: {} files in {}.', 'Установлены: {} файлов в {}.').format(total, PORT_DIR / 'fsr4_shaders')
            if not missing else _('{} of {} files missing in {}.', 'Нет {} из {} файлов в {}.').format(
                missing, total, PORT_DIR / 'fsr4_shaders'))
        self.fsr4_button.configure(state='normal' if missing and not self.downloading else 'disabled')

    def download_fsr4(self):
        self.downloading = True
        self.fsr4_button.configure(state='disabled')
        folder = PORT_DIR / 'fsr4_shaders'

        def work():
            error = None
            try:
                folder.mkdir(parents=True, exist_ok=True)
                for name in fsr4_missing():
                    part = folder / (name + '.part')
                    with urllib.request.urlopen(f'{FSR4_BASE}/{name}', timeout=60) as response:
                        part.write_bytes(response.read())
                    part.replace(folder / name)
                    self.ui_calls.put(self.refresh_fsr4)
            except OSError as failure:
                error = failure

            def done():
                self.downloading = False
                self.refresh_fsr4()
                self.refresh_status()
                if error:
                    self.messagebox.showerror('FSR 4', _('Download failed: {}', 'Не удалось скачать: {}').format(error))
            self.ui_calls.put(done)
        threading.Thread(target=work, daemon=True).start()

    def refresh_lists(self):
        if not hasattr(self, 'patches_frame'):
            return
        tk, ttk = self.tk, self.ttk
        from mods import discover
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
            ttk.Checkbutton(line, text=f'{index + 1}.  {name}', variable=var,
                            command=self.refresh_conflicts).pack(side='left')
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
        _title_id, game = game_profile_of(self.var('game_dir', 'app').get())
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

    def collect(self):
        """Writes settings.json, gow3.ini, mods.json and patches.json."""
        self.store_game_patches()
        for key, var in self.vars.items():
            try:
                value = var.get()
            except self.tk.TclError:  # an unfinished number in a spinbox
                value = APP_DEFAULTS.get(key, INI_DEFAULTS.get(key))
            if var.store == 'ini':
                self.ini[key] = ('1' if value else '0') if key in INI_FLAGS else \
                    f'{float(value):.2f}' if key == 'sharpness' else str(value)
            else:
                self.app[key] = value
        self.app['controls'] = {kind: control_changes({b: v.get() for b, v in vars_.items()}, kind)
                                for kind, vars_ in self.control_vars.items()}
        self.app['pad_style'] = self.pad_style.get()
        CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        CONFIG_FILE.write_text(json.dumps(self.app, indent=2, ensure_ascii=False), encoding='utf-8')
        save_ini({key: self.ini[key] for key in INI_DEFAULTS}, self.ini_lines)
        self.ini, self.ini_lines = load_ini()
        if self.mod_order:
            (DATA_DIR / 'mods.json').write_text(json.dumps(
                {'order': self.mod_order, 'disabled': [n for n, v in self.mod_vars.items() if not v.get()]},
                indent=2), encoding='utf-8')
        if self.patch_vars:
            chosen = load_json(DATA_DIR / 'patches.json', {})
            shown = set(self.patch_vars)
            enabled = {k for k in chosen.get('enabled', []) if k not in shown} | {k for k, v in self.patch_vars.items() if v.get()}
            disabled = {k for k in chosen.get('disabled', []) if k not in shown} | {k for k, v in self.patch_vars.items() if not v.get()}
            (DATA_DIR / 'patches.json').write_text(json.dumps(
                {'enabled': sorted(enabled), 'disabled': sorted(disabled)}, indent=2), encoding='utf-8')

    # ---- game process ------------------------------------------------------------------------
    def play(self):
        if self.process:
            return
        self.collect()
        if not game_info(self.app['game_dir']):
            self.messagebox.showerror(APP_NAME, _('Choose the game folder with eboot.bin (CUSA01623).',
                                                  'Выберите папку игры с eboot.bin (CUSA01623).'))
            self.show('game')
            return
        self.set_log('')
        try:
            self.process = subprocess.Popen(run_command(), cwd=PORT_DIR, env=game_environment(self.app),
                                            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                            stderr=subprocess.STDOUT, creationflags=NO_WINDOW)
        except OSError as error:
            self.append(_('Could not start: {}', 'Не удалось запустить: {}').format(error) + '\n')
            self.process = None
            return
        self.job = GameJob(self.process)
        threading.Thread(target=self.read_output, args=(self.process,), daemon=True).start()
        self.play_button.configure(text=_('RUNNING'), state='disabled')
        self.status.configure(text=_('Preparing the game; it opens in its own window…',
                                     'Подготовка игры; она откроется в своём окне…'), fg=TEXT)
        if self.app.get('close_on_play'):
            self.root.after(5000, self.root.destroy)  # the game keeps running

    def read_output(self, process):
        for raw in iter(process.stdout.readline, b''):
            self.output.put(raw.decode('utf-8', errors='replace'))
        self.output.put((process.wait(),))

    def drain_output(self):
        while not self.ui_calls.empty():
            self.ui_calls.get_nowait()()
        try:
            for _i in range(500):
                item = self.output.get_nowait()
                if isinstance(item, tuple):
                    self.append(_('\nThe game exited (code {}).\n', '\nИгра завершилась (код {}).\n').format(item[0]))
                    self.process = None
                    if self.job:
                        self.job.close()
                    self.refresh_status()
                else:
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
            self.collect()
        elif not path.exists():
            path.mkdir(parents=True, exist_ok=True)
        os.startfile(str(path))

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
        self.update_label = tk.Label(box, text=text, bg=CARD, fg=TEXT, font=('Segoe UI', 10, 'bold'),
                                     wraplength=self.px(160), justify='left')
        self.update_label.pack(anchor='w', padx=10, pady=(8, 6))
        buttons = tk.Frame(box, bg=CARD)
        buttons.pack(fill='x', padx=10, pady=(0, 10))
        self.update_button = ttk.Button(buttons, text=_('Update', 'Обновить'),
                                        command=lambda: self.install_update(version, url, page))
        self.update_button.pack(fill='x')
        ttk.Button(buttons, text=_("What's new", 'Что нового'), command=lambda: webbrowser.open(page)).pack(
            fill='x', pady=(4, 0))
        box.pack(fill='x', padx=(22, 18), pady=(0, 14), before=self.side_note)
        self.update_box = box
        if not self.process:
            self.status.configure(text=text, fg=TEXT)

    def install_update(self, version, url, page):
        if self.process:
            self.messagebox.showinfo(APP_NAME, _('Close the game before updating.', 'Закройте игру перед обновлением.'))
            return
        if not FROZEN or not url:  # a source tree updates with git
            webbrowser.open(page)
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
        """God of War III.lnk on the desktop."""
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

    def close(self):
        try:
            self.collect()
        except Exception:  # never keep the window open over a settings problem
            pass
        self.root.destroy()


class GameJob:
    """A Windows job holding run.py and everything it starts: gow3-probe.exe and the launches made
    by the in-game restart (no longer descendants of the first process)."""

    def __init__(self, process):
        kernel32 = ctypes.windll.kernel32
        kernel32.CreateJobObjectW.restype = ctypes.c_void_p
        self.handle = kernel32.CreateJobObjectW(None, None)
        if self.handle:
            kernel32.AssignProcessToJobObject(ctypes.c_void_p(self.handle), ctypes.c_void_p(int(process._handle)))

    def terminate(self):
        if self.handle:
            ctypes.windll.kernel32.TerminateJobObject(ctypes.c_void_p(self.handle), 1)

    def close(self):
        if self.handle:
            ctypes.windll.kernel32.CloseHandle(ctypes.c_void_p(self.handle))
            self.handle = None


def play_without_window(settings):
    """--play: the game with the saved settings (shortcuts, Steam). Output goes to the console
    when there is one and to <saves folder>/last_run.log."""
    attach_stdio()
    log_dir = Path(settings.get('user_dir') or DATA_DIR / 'user')
    log_dir.mkdir(parents=True, exist_ok=True)
    with open(log_dir / 'last_run.log', 'w', encoding='utf-8', buffering=1) as log:
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
        webbrowser.open(RELEASES_PAGE)
        return 1
    subprocess.Popen([str(target / 'God of War III.exe')], cwd=str(target))
    return 0


def main():
    global LANG
    args = sys.argv[1:]
    if args and args[0] in ('--run', '--script'):
        sys.exit(run_role(args))
    settings = {**APP_DEFAULTS, **load_json(CONFIG_FILE, {})}
    LANG = settings.get('ui_language') or windows_language()
    if args[:1] == ['--install-update'] and len(args) == 3:
        sys.exit(install_update(args[1], args[2]))
    if FROZEN and UPDATE_DIR not in PORT_DIR.parents:
        shutil.rmtree(UPDATE_DIR, ignore_errors=True)  # what a finished update left behind
    # Without a usable game folder there is nothing to play yet: open the launcher instead.
    if '--play' in args and (Path(settings['game_dir'] or '.') / 'eboot.bin').is_file():
        sys.exit(play_without_window(settings))
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(1)  # sharp text on scaled displays
    except (AttributeError, OSError):
        pass
    import tkinter as tk
    from tkinter import filedialog, messagebox, ttk
    root = tk.Tk()
    Launcher(root, tk, ttk, filedialog, messagebox)
    root.mainloop()


if __name__ == '__main__':
    main()
