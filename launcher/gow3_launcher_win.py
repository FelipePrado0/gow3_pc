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

INI_FLAGS = {'sharpen', 'object_motion', 'show_fps'}
INI_DEFAULTS = {'upscaler': 'fsr4', 'preset': '1', 'sharpen': '1', 'sharpness': '0.50',
                'object_motion': '1', 'show_fps': '1', 'output_res': '1920x1080',
                'live_resolution': 'auto'}
APP_DEFAULTS = {'ui_language': '', 'game_dir': os.environ.get('GOW3_GAME_DIR', str(PORT_DIR.parent / 'CUSA01623')), 'user_dir': '',
                'mods_dir': '', 'mods_enabled': True, 'patches_dir': '', 'language': '1',
                'player_name': '', 'fullscreen': False, 'hdr': False, 'present_mode': 'Mailbox',
                'frame_cap': '', 'draw_pipe': '', 'readbacks': '',
                'frames_ahead': '', 'parallel_warmup': True, 'async_shaders': True, 'deferred_readback': True, 'stale_readback': True, 'perf_diag': False, 'frame_stats': False, 'gpu_profile': False,
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
PRESENT_MODES = [('Mailbox', ('Mailbox (low latency, no tearing)', 'Mailbox (без разрывов)')),
                 ('Fifo', ('FIFO (VSync)',)), ('FifoRelaxed', ('FIFO Relaxed',)),
                 ('Immediate', ('Immediate (tearing)', 'Immediate (с разрывами)'))]
LANGUAGES = [('1', ('English', 'Английский')), ('8', ('Russian', 'Русский')), ('0', ('Japanese', 'Японский')),
             ('2', ('French', 'Французский')), ('3', ('Spanish', 'Испанский')), ('4', ('German', 'Немецкий')),
             ('5', ('Italian', 'Итальянский'))]
DRAW_PIPE = [('', ('Auto (8+ threads)', 'Авто (8+ потоков)')), ('1', ('On', 'Включён')),
             ('0', ('Off (more stable)', 'Выключен (стабильнее)'))]
READBACKS = [('', ('Relaxed (default)', 'Relaxed (по умолчанию)')), ('0', ('Off', 'Выключены')),
             ('2', ('Precise',))]
# Frame cap of the unlocked mode (GOW3_FPS_LIMIT). '' leaves the port's own: the display refresh,
# at most 120, because the game's movement timing breaks above about 120 FPS.
FRAME_CAPS = [('', ('Auto: display refresh, max 120 (recommended)', 'Авто: частота монитора, макс. 120 (рекомендуется)')),
              ('60', ('60',)), ('90', ('90',)), ('120', ('120',)), ('144', ('144  ⚠',)), ('165', ('165  ⚠',)),
              ('240', ('240  ⚠',)), ('0', ('No limit  ⚠', 'Без ограничения  ⚠'))]
FRAMES_AHEAD = [('', ('1 (default)', '1 (по умолчанию)')), ('2', ('2',)), ('0', ('Unbounded', 'Без ограничения'))]
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
    env['GOW3_FULLSCREEN'] = '1' if s['fullscreen'] else '0'
    env['GOW3_PRESENT_MODE'] = s['present_mode']
    if s['hdr']:
        env['GOW3_HDR'] = '1'
    if s.get('frame_cap', ''):
        env['GOW3_FPS_LIMIT'] = s['frame_cap']
    for key, name in (('draw_pipe', 'GOW3_DRAW_PIPE'), ('readbacks', 'GOW3_READBACKS'), ('frames_ahead', 'GOW3_FRAMES_AHEAD')):
        if s[key]:
            env[name] = s[key]
    env['GOW3_PARALLEL_WARMUP'] = '1' if s.get('parallel_warmup', True) else '0'
    env['GOW3_ASYNC_SHADERS'] = '1' if s.get('async_shaders', True) else '0'
    env['GOW3_DEFERRED_READBACK'] = '1' if s.get('deferred_readback', True) else '0'
    env['GOW3_STALE_READBACK'] = '1' if s.get('stale_readback', True) else '0'
    env['GOW3_PERF_DIAG'] = '1' if s.get('perf_diag', False) else '0'
    for key, name in (('frame_stats', 'GOW3_FRAME_STATS'), ('gpu_profile', 'GOW3_GPU_PROFILE'),
                      ('vk_validation', 'GOW3_VK_VALIDATION')):
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

BG, PANEL, CARD, LINE = '#0e0c0b', '#151210', '#1c1815', '#2e2722'
TEXT, MUTED, GOLD, BLOOD, BLOOD_HI = '#e9e2d6', '#9a8f80', '#c8a96a', '#7c1717', '#9e2222'


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
        self.gpu_text = _('• Checking the graphics card…', '• Проверка видеокарты…')
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
        self.show('play')
        root.protocol('WM_DELETE_WINDOW', self.close)
        root.after(100, self.drain_output)
        threading.Thread(target=self.detect_gpu, daemon=True).start()
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
            fill = BLOOD if checked else CARD
            image.put(GOLD if checked else '#5a4c40', to=(0, 0, n, n))
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
        self.root.option_add('*TCombobox*Listbox.selectBackground', BLOOD)
        self.root.option_add('*TCombobox*Listbox.font', base)
        s.configure('.', background=PANEL, foreground=TEXT, fieldbackground=CARD, bordercolor=LINE,
                    lightcolor=LINE, darkcolor=LINE, troughcolor=CARD, focuscolor=GOLD, font=base)
        s.configure('TFrame', background=PANEL)
        s.configure('TLabel', background=PANEL, foreground=TEXT)
        s.configure('Muted.TLabel', background=PANEL, foreground=MUTED, font=('Segoe UI', 9))
        s.configure('Section.TLabel', background=PANEL, foreground=GOLD, font=('Georgia', 13))
        s.configure('TCheckbutton', background=PANEL, foreground=TEXT, padding=(0, 3))
        s.map('TCheckbutton', background=[('active', PANEL)], foreground=[('disabled', MUTED)])
        s.configure('Warning.TLabel', background='#2a1d12', foreground='#e3b25a', padding=(10, 6))
        s.configure('TCombobox', arrowcolor=GOLD, foreground=TEXT, padding=4)
        s.map('TCombobox', fieldbackground=[('readonly', CARD)], foreground=[('readonly', TEXT)],
              selectbackground=[('readonly', CARD)], selectforeground=[('readonly', TEXT)])
        s.configure('TEntry', foreground=TEXT, insertcolor=TEXT, padding=4)
        s.configure('TSpinbox', foreground=TEXT, arrowcolor=GOLD, insertcolor=TEXT, padding=4)
        s.configure('TButton', background=CARD, foreground=TEXT, padding=(12, 6), borderwidth=1)
        s.map('TButton', background=[('active', LINE), ('disabled', PANEL)], foreground=[('disabled', MUTED)])
        s.configure('Play.TButton', background=BLOOD, foreground='#f4ece0', font=('Georgia', 15, 'bold'),
                    padding=(36, 10), borderwidth=0)
        s.map('Play.TButton', background=[('active', BLOOD_HI), ('disabled', '#3a2420')],
              foreground=[('disabled', '#8a7a70')])
        s.configure('Horizontal.TProgressbar', background=GOLD, troughcolor=CARD, bordercolor=LINE)
        s.configure('Vertical.TScrollbar', background=CARD, arrowcolor=GOLD, troughcolor=PANEL, bordercolor=PANEL)

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

    def choice(self, parent, key, store, options, width=38):
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

    def row(self, parent, title, widget, hint=None):
        ttk = self.ttk
        r = self.next_row(parent)
        ttk.Label(parent, text=title).grid(row=r, column=0, sticky='nw', padx=(0, 18), pady=(8, 0))
        widget.grid(row=r, column=1, sticky='w', pady=(5, 0))
        if hint:
            ttk.Label(parent, text=hint, style='Muted.TLabel', wraplength=self.px(560), justify='left').grid(
                row=r + 1, column=1, sticky='w', pady=(2, 2))
        return widget

    def check(self, parent, key, store, title, hint=None, var=None):
        ttk = self.ttk
        r = self.next_row(parent)
        ttk.Checkbutton(parent, text=title, variable=var if var is not None else self.var(key, store)).grid(
            row=r, column=0, columnspan=2, sticky='w', pady=(4, 0))
        if hint:
            ttk.Label(parent, text=hint, style='Muted.TLabel', wraplength=self.px(640), justify='left').grid(
                row=r + 1, column=0, columnspan=2, sticky='w', padx=(26, 0))

    def note(self, parent, text, top=12):
        self.ttk.Label(parent, text=text, style='Muted.TLabel', wraplength=self.px(680), justify='left').grid(
            row=self.next_row(parent), column=0, columnspan=2, sticky='w', pady=(top, 0))

    def section(self, parent, title, top=18):
        self.ttk.Label(parent, text=title, style='Section.TLabel').grid(
            row=self.next_row(parent), column=0, columnspan=2, sticky='w', pady=(top, 2))

    def folder(self, parent, key, title, prompt, hint=None, on_change=None):
        ttk = self.ttk
        var = self.var(key, 'app')
        holder = ttk.Frame(parent)
        ttk.Entry(holder, textvariable=var, width=54).pack(side='left')

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
        ttk.Label(head, text=title, background=PANEL, foreground=TEXT, font=('Georgia', 20)).pack(anchor='w')
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
        self.side_title = tk.Label(side, text=APP_NAME.upper(), bg=BG, fg=GOLD, font=('Georgia', 15),
                                   wraplength=self.px(190), justify='left')
        self.side_title.pack(anchor='w', padx=22, pady=(24, 0))
        self.side = side
        tk.Label(side, text=_('native port · Windows', 'нативный порт · Windows') + f'  ·  v{VERSION}', bg=BG, fg=MUTED,
                 font=('Segoe UI', 9)).pack(anchor='w', padx=22, pady=(0, 20))
        self.nav, self.current_page = {}, None
        for name, title in (('play', _('Play', 'Играть')), ('graphics', _('Graphics', 'Графика')),
                            ('display', _('Display & FPS', 'Экран и FPS')), ('game', _('Game & effects', 'Игра и эффекты')),
                            ('gamepatches', _('Game patches', 'Патчи игры')),
                            ('mods', _('Mods & patches', 'Моды и патчи')),
                            ('controls', _('Controls', 'Управление')),
                            ('advanced', _('Advanced', 'Дополнительно')),
                            ('log', _('Log', 'Журнал'))):
            item = tk.Label(side, text='    ' + title, bg=BG, fg=TEXT, anchor='w', font=('Segoe UI', 11),
                            pady=10, cursor='hand2')
            item.pack(fill='x')
            item.bind('<Button-1>', lambda _e, n=name: self.show(n))
            item.bind('<Enter>', lambda _e, n=name: n != self.current_page and self.nav[n].configure(bg='#1a1613'))
            item.bind('<Leave>', lambda _e, n=name: n != self.current_page and self.nav[n].configure(bg=BG))
            self.nav[name] = item
        # The temporal upscalers are not calibrated for this game yet (run.py turns them off):
        # their page stays in the code, out of the menu.
        self.nav['graphics'].pack_forget()
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
        self.stop_button = ttk.Button(bar, text=_('Stop', 'Остановить'), command=self.stop, state='disabled')
        self.stop_button.pack(side='right', pady=11)
        self.content = tk.Frame(right, bg=PANEL)
        self.content.pack(fill='both', expand=True)

        self.pages = {}
        self.build_play()
        self.build_graphics()
        self.build_display()
        self.build_game()
        self.build_game_patches()
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
            item.configure(bg=PANEL if n == name else BG, fg=GOLD if n == name else TEXT)
        if name == 'mods':
            self.refresh_lists()
        elif name == 'gamepatches':
            self.refresh_game_patches()
        elif name == 'graphics':
            self.refresh_fsr4()
        elif name == 'play':
            self.refresh_status()

    def build_play(self):
        tk, ttk = self.tk, self.ttk
        page = tk.Frame(self.content, bg=PANEL)
        self.pages['play'] = page
        self.banner = tk.Canvas(page, height=self.px(290), bg=BG, highlightthickness=0, bd=0)
        self.banner.pack(fill='x')
        self.banner.bind('<Configure>', lambda _e: self.draw_banner())
        body = ttk.Frame(page, padding=(28, 12, 28, 8))
        body.pack(fill='both', expand=True)
        body.columnconfigure(0, weight=3)
        body.columnconfigure(1, weight=2)
        info = ttk.Frame(body)
        info.grid(row=0, column=0, sticky='nw', padx=(0, 24))
        ttk.Label(info, text=_('Ready check', 'Проверка'), style='Section.TLabel').pack(anchor='w', pady=(0, 6))
        self.checks = {}
        for key in ('game', 'saves', 'gpu'):
            self.checks[key] = ttk.Label(info, text='', justify='left', wraplength=self.px(440))
            self.checks[key].pack(anchor='w', pady=3)
        quick = ttk.Frame(body)
        quick.grid(row=0, column=1, sticky='nw')
        ttk.Label(quick, text=_('Quick settings', 'Основное'), style='Section.TLabel').grid(
            row=0, column=0, columnspan=2, sticky='w', pady=(0, 2))
        ttk.Checkbutton(quick, text=_('Fullscreen', 'Полный экран'), variable=self.var('fullscreen', 'app')).grid(
            row=self.next_row(quick), column=1, sticky='w', pady=(8, 0))

    def draw_banner(self):
        """The cover art of the selected dump (sce_sys/pic1.png) under the title."""
        tk, c = self.tk, self.banner
        c.delete('all')
        w, h = max(c.winfo_width(), 400), int(c['height'])
        art = Path(self.var('game_dir', 'app').get() or '.') / 'sce_sys' / 'pic1.png'
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
            c.create_text(w // 2, h // 2 - 20, text=_('Choose your game folder (Game & effects)',
                                                       'Выберите папку игры («Игра и эффекты»)'),
                          fill=MUTED, font=('Segoe UI', 11))
        if not self.banner_image:
            c.create_text(30, h - 70, text=APP_NAME, anchor='w', fill='#f2ead9', font=('Georgia', 36))
        folder = self.var('game_dir', 'app').get()
        info = game_info(folder)
        title_id = game_profile_of(folder)[0] if info else None
        sub = (_('{} · game version {}', '{} · версия игры {}').format(title_id or '?', info[1]) if info
               else _('Game folder not set', 'Папка игры не выбрана'))
        c.create_text(33, h - 30, text=sub, anchor='w', fill=GOLD, font=('Segoe UI', 11))

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
        self.check(f, 'show_fps', 'ini', _('Show the FPS counter', 'Показывать FPS'))

    def build_display(self):
        ttk = self.ttk
        f = self.scrolled_page('display', _('Display & FPS', 'Экран и FPS'),
                               _('Applied when the game starts.', 'Применяется при запуске игры.'))
        self.section(f, _('Frame rate', 'Частота кадров'), top=4)
        self.row(f, _('Frame cap', 'Ограничение FPS'), self.choice(f, 'frame_cap', 'app', FRAME_CAPS),
                 _('Below its target frame rate the game runs slower than real time: higher caps are at '
                   'your own risk.',
                   'Ниже целевой частоты кадров игра идёт медленнее реального времени: более высокие '
                   'значения на ваш риск.'))
        self.row(f, _('Frames ahead of the GPU', 'Кадров впереди GPU'), self.choice(f, 'frames_ahead', 'app', FRAMES_AHEAD),
                 _('1 keeps frame pacing even; more can raise FPS when the graphics card is the limit.',
                   '1 — ровная подача кадров; больше может поднять FPS, если упирается в видеокарту.'))
        self.section(f, _('Window', 'Окно'))
        self.check(f, 'fullscreen', 'app', _('Fullscreen', 'Полноэкранный режим'))
        self.row(f, _('Presentation', 'Режим показа кадров'), self.choice(f, 'present_mode', 'app', PRESENT_MODES))
        self.check(f, 'hdr', 'app', _('Allow HDR output', 'Разрешить HDR'),
                   _('When HDR is on in Windows and the display supports it.',
                     'Если HDR включён в Windows и монитор его поддерживает.'))

    def build_game(self):
        f = self.scrolled_page('game', _('Game & effects', 'Игра и эффекты'),
                               _('Your game dump, saves and the game patches.', 'Дамп игры, сохранения и патчи игры.'))
        self.section(f, _('Game', 'Игра'), top=4)
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
        self.build_save_backups(f)
        self.row(f, _('Game language', 'Язык игры'), self.choice(f, 'language', 'app', LANGUAGES))
        self.row(f, _('Player name', 'Имя игрока'), self.ttk.Entry(f, textvariable=self.var('player_name', 'app'), width=30),
                 _('Where the game shows the PSN name; empty: the default.', 'Где игра показывает имя PSN; пусто — по умолчанию.'))

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

    def build_game_patches(self):
        """Patches of the selected game's built-in XML (God of War III): one resolution, the rest
        switched one by one; the notes come from the patch file."""
        f = self.scrolled_page('gamepatches', _('Game patches', 'Патчи игры'),
                               _('Community patches of the selected game, applied at start.',
                                 'Патчи сообщества для выбранной игры, применяются при запуске.'))
        self.game_patch_frame = self.ttk.Frame(f)
        self.game_patch_frame.grid(row=0, column=0, columnspan=2, sticky='we')
        self.game_patch_title, self.game_patch_res, self.game_patch_vars = None, None, {}

    def refresh_game_patches(self):
        import xml.etree.ElementTree as ET
        from patches import EXCLUSIVE_PREFIX
        tk, ttk, f = self.tk, self.ttk, self.game_patch_frame
        self.store_game_patches()
        for widget in f.winfo_children():
            widget.destroy()
        f.columnconfigure(1, weight=1)
        folder = self.var('game_dir', 'app').get()
        title_id, profile = game_profile_of(folder)
        self.game_patch_title, self.game_patch_res, self.game_patch_vars = None, None, {}
        if not profile:
            self.note(f, _('No built-in patches for this game.', 'Для этой игры нет встроенных патчей.'), top=0)
            return
        _ids, xml, version = profile[:3]
        metas = [m for m in ET.parse(xml).getroot().iter('Metadata') if m.get('AppVer') == version]
        chosen = self.app.get('game_patches', {}).get(title_id)
        on = set(chosen) if chosen is not None else {m.get('Name') for m in metas
                                                         if m.get('isEnabled', 'false').lower() == 'true'}
        self.note(f, _('{} · version {} · {}', '{} · версия {} · {}').format(title_id, version, xml.name), top=0)
        resolutions = [m for m in metas if m.get('Name', '').startswith(EXCLUSIVE_PREFIX)]
        if resolutions:
            names = [''] + [m.get('Name') for m in resolutions]
            labels = [_('Native (1920 × 1080)', 'Нативное (1920 × 1080)')] + [n[len(EXCLUSIVE_PREFIX):].strip(' -') for n in names[1:]]
            current = next((n for n in names[1:] if n in on), '')
            self.game_patch_res = tk.StringVar(value=current)
            box = ttk.Combobox(f, values=labels, state='readonly', width=32)
            box.current(names.index(current))
            box.bind('<<ComboboxSelected>>', lambda _e: self.game_patch_res.set(names[box.current()]))
            note = next((m.get('Note') for m in resolutions if m.get('Name') == current), None)
            self.row(f, _('Resolution', 'Разрешение'), box,
                     _('The game renders at this size; a resolution patch replaces the texture fix and '
                       'reserves the memory it needs.',
                       'Игра рисует в этом размере; патч разрешения заменяет исправление текстур и '
                       'резервирует нужную память.') if not note else note)
        self.section(f, _('Patches', 'Патчи'))
        for meta in metas:
            name = meta.get('Name')
            if name.startswith(EXCLUSIVE_PREFIX):
                continue
            var = tk.BooleanVar(value=name in on)
            self.game_patch_vars[name] = var
            author = meta.get('Author')
            self.check(f, None, None, name + (f'  ({author})' if author else ''), meta.get('Note'), var=var)
        self.game_patch_title = title_id

    def store_game_patches(self):
        """The page's selection into settings (the whole list for this game)."""
        if not self.game_patch_title:
            return
        names = [self.game_patch_res.get()] if self.game_patch_res and self.game_patch_res.get() else []
        names += [n for n, v in self.game_patch_vars.items() if v.get()]
        self.app.setdefault('game_patches', {})[self.game_patch_title] = names

    def build_mods(self):
        f = self.scrolled_page('mods', _('Mods & patches', 'Моды и патчи'),
                               _('The game files are never changed: mods are layered over them at start.',
                                 'Файлы игры не меняются: моды накладываются при запуске.'))
        self.section(f, _('Mods', 'Моды'), top=4)
        self.check(f, 'mods_enabled', 'app', _('Load mods', 'Загружать моды'),
                   _('Loose-file mods, each in its own folder with dvdroot_ps4.',
                     'Моды из файлов, каждый в своей папке с dvdroot_ps4.'))
        self.folder(f, 'mods_dir', _('Mods folder', 'Папка модов'), _('Choose the mods folder', 'Выберите папку модов'),
                    _('Empty: {}', 'Пусто: {}').format(DATA_DIR / 'mods'), on_change=self.refresh_lists)
        self.mods_frame = self.ttk.Frame(f)
        self.mods_frame.grid(row=self.next_row(f), column=0, columnspan=2, sticky='we', pady=(8, 0))
        self.section(f, _('Third-party patches', 'Сторонние патчи'))
        self.folder(f, 'patches_dir', _('Patches folder', 'Папка патчей'), _('Choose the patches folder', 'Выберите папку патчей'),
                    _('shadPS4/GoldHEN XML patch files for this game version. Empty: {}',
                      'XML-патчи shadPS4/GoldHEN для этой версии игры. Пусто: {}').format(DATA_DIR / 'patches'),
                    on_change=self.refresh_lists)
        self.patches_frame = self.ttk.Frame(f)
        self.patches_frame.grid(row=self.next_row(f), column=0, columnspan=2, sticky='we', pady=(8, 0))
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
        self.section(f, _('Gamepad button names'), top=4)
        names = ttk.Frame(f)
        for value, label in (('playstation', 'PlayStation'), ('xbox', 'Xbox')):
            ttk.Radiobutton(names, text=label, value=value, variable=self.pad_style,
                            command=self.relabel_pad_controls).pack(side='left', padx=(0, 14))
        self.row(f, _('Show as'), names)
        self.section(f, _('Game button: keyboard key / gamepad button'))
        self.control_vars, self.pad_boxes = {'key': {}, 'pad': {}}, {}
        labels = PAD_STYLES[style]
        for button, key, pad in CONTROLS:
            holder = ttk.Frame(f)
            key_var = tk.StringVar(value=saved.get('key', {}).get(button) or key)
            self.control_vars['key'][button] = key_var
            ttk.Entry(holder, textvariable=key_var, width=16).pack(side='left', padx=(0, 8))
            pad_name = saved.get('pad', {}).get(button) or pad
            pad_var = tk.StringVar(value=pad_name or '')
            self.control_vars['pad'][button] = pad_var
            if pad:
                box = ttk.Combobox(holder, state='readonly', width=24, values=list(labels.values()))
                box.set(labels.get(pad_name, pad_name))
                box.bind('<<ComboboxSelected>>', lambda _e, b=box, v=pad_var: v.set(self.pad_name(b.get())))
                self.pad_boxes[button] = box
            else:
                box = ttk.Label(holder, text=_('{} trigger').format(PS_BUTTON_NAMES[button]), style='Muted.TLabel', width=24)
            box.pack(side='left')
            self.row(f, PS_BUTTON_NAMES[button], holder)
        self.note(f, _('Keys use SDL names: letters, digits, Space, Return, Tab, Left Shift, Left Ctrl, Left Alt, '
                       'Backspace, Up, Down, Left, Right, F1 to F12. A name the game does not know keeps the default '
                       '(the log says which). L2/R2 on the gamepad stay on the triggers. Insert and R3+L2 always open '
                       'the port\'s menu. Applied when the game starts.'))
        self.ttk.Button(f, text=_('Reset to defaults'), command=self.reset_controls).grid(
            row=self.next_row(f), column=0, sticky='w', pady=(14, 0))

    def pad_name(self, label):
        """SDL gamepad button name of a label in the current naming style."""
        return next((n for n, l in PAD_STYLES[self.pad_style.get()].items() if l == label), label)

    def relabel_pad_controls(self):
        labels = PAD_STYLES[self.pad_style.get()]
        for button, box in self.pad_boxes.items():
            box.configure(values=list(labels.values()))
            name = self.control_vars['pad'][button].get()
            box.set(labels.get(name, name))

    def reset_controls(self):
        for button, key, pad in CONTROLS:
            self.control_vars['key'][button].set(key)
            self.control_vars['pad'][button].set(pad or '')
        self.relabel_pad_controls()

    def build_advanced(self):
        ttk = self.ttk
        f = self.scrolled_page('advanced', _('Advanced', 'Дополнительно'),
                               _('Launcher options, performance switches and diagnostics.',
                                 'Настройки лаунчера, производительность и диагностика.'))
        self.section(f, _('Launcher', 'Лаунчер'), top=4)
        self.row(f, _('Launcher language', 'Язык лаунчера'), self.choice(f, 'ui_language', 'app', UI_LANGUAGES),
                 _('Applies when the launcher opens again.', 'Применится при следующем открытии лаунчера.'))
        self.check(f, 'close_on_play', 'app', _('Close the launcher when the game starts', 'Закрывать лаунчер при запуске игры'))
        self.check(f, 'check_updates', 'app', _('Check for updates when the launcher opens',
                                                'Проверять обновления при открытии лаунчера'))
        holder = ttk.Frame(f)
        holder.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(10, 0))
        ttk.Button(holder, text=_('Desktop shortcut', 'Ярлык на рабочем столе'), command=self.shortcut).pack(side='left')
        ttk.Button(holder, text=_('Port folder', 'Папка порта'), command=lambda: self.open_path(DATA_DIR)).pack(side='left', padx=6)
        ttk.Button(holder, text='gow3.ini', command=lambda: self.open_path(ini_path())).pack(side='left')
        holder = ttk.Frame(f)
        holder.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(10, 0))
        ttk.Button(holder, text=_('Check for updates', 'Проверить обновления'),
                   command=lambda: threading.Thread(target=self.check_update, args=(True,), daemon=True).start()
                   ).pack(side='left')
        ttk.Label(holder, text=f'v{VERSION}', style='Muted.TLabel').pack(side='left', padx=10)
        holder = ttk.Frame(f)
        holder.grid(row=self.next_row(f), column=0, columnspan=2, sticky='w', pady=(10, 0))
        ttk.Button(holder, text=_('Clear shader cache', 'Очистить кэш шейдеров'), command=self.clear_cache).pack(side='left')
        ttk.Label(holder, text=_('If the game only shows a black screen, this usually helps.',
                                 'Если игра показывает только чёрный экран, обычно это помогает.'),
                  style='Muted.TLabel').pack(side='left', padx=10)
        self.section(f, _('Performance', 'Производительность'))
        self.row(f, _('Two-stage GPU pipeline', 'Двухстадийный конвейер GPU'), self.choice(f, 'draw_pipe', 'app', DRAW_PIPE),
                 _('20–30% faster; switch it off if the game is unstable.', 'Быстрее на 20–30%; при нестабильности выключите.'))
        self.row(f, _('GPU readbacks', 'Чтение данных GPU'), self.choice(f, 'readbacks', 'app', READBACKS),
                 _('How exactly data the GPU writes is copied back for the game.',
                   'Насколько точно данные, записанные GPU, возвращаются игре.'))
        self.check(f, 'async_shaders', 'app', _('Compile new shaders in the background'),
                   _('Fewer stutters in new areas: the game keeps running while new shaders compile. An object '
                     'or effect may appear a moment later. Switch it off if something stays missing.'))
        self.check(f, 'deferred_readback', 'app', _('Deferred GPU readbacks'),
                   _('The GPU copies data back for the game without stopping each time: fewer waits, a few '
                     'more FPS. Switch it off if textures or shadows look wrong.'))
        self.check(f, 'stale_readback', 'app', _('Game reads GPU data without waiting (big FPS gain)'),
                   _('The game reads data the GPU wrote, such as the scene exposure, from the previous frame '
                     'instead of waiting for the current one: about 40 to 66 FPS in a heavy scene. Needs '
                     'deferred GPU readbacks. Switch it off if lighting or objects look wrong.'))
        self.check(f, 'parallel_warmup', 'app', _('Fast shader cache loading (parallel)'),
                   _('Builds the cached pipelines on several CPU threads at start: about 70 s instead of 300 s '
                     'with a full cache. Switch it off if the image looks wrong after loading.'))
        self.section(f, _('Diagnostics', 'Для разработчика'))
        self.check(f, 'frame_stats', 'app', _('Frame statistics in the log (every 5 s)', 'Статистика кадров в журнале (раз в 5 с)'))
        self.check(f, 'gpu_profile', 'app', _('GPU time per pass in the log', 'Профиль GPU в журнале'))
        self.check(f, 'vk_validation', 'app', _('Vulkan validation layers (needs the Vulkan SDK; much slower)',
                                                'Слои валидации Vulkan (нужен Vulkan SDK; сильно замедляет)'))
        self.check(f, 'perf_diag', 'app', _('GPU wait and readback report in the log (every 5 s)'),
                   _('Shows where the frame waits for the GPU and which reads cause it. For performance tests.'))

    def build_log(self):
        tk, ttk = self.tk, self.ttk
        page = ttk.Frame(self.content, padding=(20, 14, 20, 8))
        self.pages['log'] = page
        top = ttk.Frame(page)
        top.pack(fill='x', pady=(0, 8))
        ttk.Label(top, text=_('Log', 'Журнал'), font=('Georgia', 20)).pack(side='left')
        ttk.Button(top, text=_('Copy', 'Копировать'), command=self.copy_log).pack(side='right')
        ttk.Button(top, text=_('Clear', 'Очистить'), command=lambda: self.set_log('')).pack(side='right', padx=6)
        self.log = tk.Text(page, wrap='none', bg='#0a0908', fg='#cfc6b8', insertbackground=TEXT, relief='flat',
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
        if self.current_page == 'gamepatches':
            self.refresh_game_patches()
        self.banner_source = None
        self.draw_banner()
        self.set_icon()
        self.refresh_status()

    def refresh_status(self):
        folder = self.var('game_dir', 'app').get()
        info = game_info(folder)
        _title_id, profile = game_profile_of(folder) if info else (None, None)
        if not info:
            game = _('✗ No eboot.bin in the game folder (Game & effects)', '✗ В папке игры нет eboot.bin («Игра и эффекты»)')
        elif profile:
            game = (_('✓ Game version {}: the game patches apply', '✓ Версия игры {}: патчи игры применяются').format(info[1])
                    if info[1] == profile[2] else
                    _('⚠ Game version {}: the game patches need {}; none applied',
                      '⚠ Версия игры {}: патчам нужна {}; не применяются').format(info[1], profile[2]))
        else:
            game = _('⚠ Game version {}: no built-in patches for this game',
                     '⚠ Версия игры {}: для этой игры нет встроенных патчей').format(info[1])
        user = Path(self.var('user_dir', 'app').get() or DATA_DIR / 'user')
        saves = list((user / 'savedata').glob('*/*/*')) if (user / 'savedata').is_dir() else []
        save = (_('✓ Saves found in {}', '✓ Найдены сохранения в {}').format(user) if saves
                else _('• No saves yet: the game creates them in {}', '• Сохранений пока нет: игра создаст их в {}').format(user))
        for key, text in (('game', game), ('saves', save), ('gpu', self.gpu_text)):
            self.checks[key].configure(text=text, foreground=MUTED if text.startswith('•') else
                                       '#d9a441' if text.startswith('⚠') else '#d36b5c' if text.startswith('✗') else TEXT)
        if not self.process:
            self.play_button.configure(state='normal' if info else 'disabled')
            self.status.configure(text=self.summary() if info else _('Choose the game folder first.',
                                                                     'Сначала выберите папку игры.'), fg=MUTED)

    def summary(self):
        self.store_game_patches()
        title_id, _profile = game_profile_of(self.var('game_dir', 'app').get())
        names = self.app.get('game_patches', {}).get(title_id) or []
        resolution = next((n.split(' - ', 1)[-1] for n in names if n.startswith('Resolution Patch')),
                          _('Native (1920 × 1080)', 'Нативное (1920 × 1080)'))
        others = len([n for n in names if not n.startswith('Resolution Patch')])
        return _('{} · {} more patches', '{} · ещё патчей: {}').format(resolution, others)

    def detect_gpu(self):
        exe = PORT_DIR / 'bin' / 'gow3-gpu-capabilities.exe'
        if not exe.is_file():
            exe = PORT_DIR / 'out' / 'gow3-gpu-capabilities.exe'
        env = dict(os.environ)
        if not (exe.parent / 'SDL3.dll').is_file():
            clang64 = Path(os.environ.get('MSYS2_ROOT', r'C:\msys64')) / 'clang64' / 'bin'
            env['PATH'] = f'{clang64}{os.pathsep}{env.get("PATH", "")}'
        text = _('• Graphics card: not checked', '• Видеокарта: не проверена')
        try:
            result = subprocess.run([str(exe), '--live-resolution'], capture_output=True, text=True, timeout=30,
                                    env=env, creationflags=NO_WINDOW)
            names = [line[5:].split(':')[0] for line in result.stderr.splitlines() if line.startswith('GPU: ')]
            if names:
                text = _('✓ Graphics card: {}', '✓ Видеокарта: {}').format(names[0])
            elif result.returncode:
                text = _('✗ No Vulkan 1.3 graphics card found (update the driver)',
                         '✗ Не найдена видеокарта с Vulkan 1.3 (обновите драйвер)')
        except (OSError, subprocess.TimeoutExpired):
            pass
        self.gpu_text = text
        self.ui_calls.put(self.refresh_status)

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
        self.conflict_label = ttk.Label(self.mods_frame, text='', justify='left', foreground='#d9a441',
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
        self.play_button.configure(state='disabled')
        self.stop_button.configure(state='normal')
        self.status.configure(text=_('Preparing the game; it opens in its own window…',
                                     'Подготовка игры; она откроется в своём окне…'), fg=GOLD)
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
                    self.stop_button.configure(state='disabled')
                    self.refresh_status()
                else:
                    if 'Entering original x86-64 code' in item:
                        self.status.configure(text=_('The game is running.', 'Игра запущена.'), fg=GOLD)
                    elif 'restarting through run.py' in item:
                        self.status.configure(text=_('Restarting with the new settings…', 'Перезапуск с новыми настройками…'), fg=GOLD)
                    self.append(item)
        except queue.Empty:
            pass
        self.root.after(100, self.drain_output)

    def stop(self):
        if self.job:
            self.job.terminate()

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
        box = tk.Frame(self.side, bg=CARD, highlightthickness=1, highlightbackground=GOLD)
        self.update_label = tk.Label(box, text=text, bg=CARD, fg=GOLD, font=('Segoe UI', 10, 'bold'),
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
            self.status.configure(text=text, fg=GOLD)

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
