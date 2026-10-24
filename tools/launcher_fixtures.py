"""One-off: runs the Python launcher's own save path over known inputs and stores the files it writes in
launcher-rust/fixtures/<case>/, for the Rust launcher's byte-for-byte parity tests.

Run from anywhere: python tools/launcher_fixtures.py

Each case folder holds input.json, settings.json, spartanport.ini and environment.json: the ini edits saved
over the existing lines and the whole app dict, without the window. environment.json keeps only the SPARTAN_*,
SDL_* and PYTHON* keys, sorted; the temporary data folder in its values is written as <DATA_DIR>. The inputs
name the game folder through SPARTAN_GAME_DIR because APP_DEFAULTS reads it when the launcher module is
imported (on Windows it would be a machine path).

The 'collected' folder is what the Save button writes: the real Launcher.collect() of a Tk window (kept
hidden) opened over the 'edited' input, with its edits set on the window's variables. It holds input.json,
settings.json, spartanport.ini, and mods.json / patches.json only when collect() wrote them. The monitor and
GPU probes are replaced first so the tools of this machine never reach the files: no monitors (the Monitor
box falls back to the primary default) and one GPU numbered 0 (the saved Graphics card stays valid)."""
import json
import os
import shutil
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / 'launcher-rust' / 'fixtures'
GAME_DIR = r'D:\Games\CUSA01623'
WORK = Path(tempfile.mkdtemp(prefix='launcher_fixtures_'))

# The launcher computes its folders from the environment when it is imported.
os.environ['SPARTAN_DATA_DIR'] = str(WORK / 'data')
os.environ['SPARTAN_GAME_DIR'] = GAME_DIR
sys.path.insert(0, str(ROOT / 'launcher'))
sys.path.insert(0, str(ROOT / 'scripts'))
import spartanport_launcher_win as launcher  # noqa: E402  (never builds the Tk window)

CASES = {
    'defaults': {
        'settings': {}, 'ini_lines': [], 'edits': {'app': {}, 'ini': {}},
        'env': {'SPARTAN_GAME_DIR': GAME_DIR},
    },
    'edited': {
        'settings': {'game_dir': r'D:\Games\GOW3', 'ui_language': 'pt', 'monitor': '1', 'hdr': True,
                     'gpu_id': '0', 'controls': {'key': {'cross': 'Space', 'square': 'F'}, 'pad': {'cross': 'b'}},
                     'controls_layout': 2, 'game_patches': {'CUSA01623': ['Skip Intro']}},
        'ini_lines': ['# spartanport settings (in-game menu: Insert / R3+L2)', 'upscaler=fsr3', 'fps_limit=60',
                      'extra=a=b'],
        'edits': {'app': {'close_on_play': True}, 'ini': {'fps_limit': '120', 'vsync': '0'}},
        'env': {'SPARTAN_GAME_DIR': GAME_DIR, 'SPARTAN_GPU_ID': '7', 'SDL_VIDEODRIVER': 'windows',
                'PYTHONPATH': r'C:\lib', 'UNRELATED': 'dropped'},
    },
    'legacy_controls': {
        'settings': {'controls': {'native': {'cross': 'Space'}}, 'native_input': True, 'native_rage': False},
        'ini_lines': [], 'edits': {'app': {}, 'ini': {}},
        'env': {'SPARTAN_GAME_DIR': GAME_DIR},
    },
}


def prepare_data():
    """An empty data folder, where the launcher reads and writes its files."""
    data = WORK / 'data'
    shutil.rmtree(data, ignore_errors=True)
    data.mkdir(parents=True)
    launcher.DATA_DIR = launcher.CONFIG_DIR = data
    launcher.CONFIG_FILE = data / 'settings.json'
    launcher.LANG = 'en'
    os.environ['SPARTAN_CONFIG'] = str(data / 'spartanport.ini')
    return data


def run_case(name, case):
    data = prepare_data()

    # What collect() writes: the ini edits over the existing lines, then the whole app dict.
    app = {**launcher.APP_DEFAULTS, **launcher.migrate_controls(case['settings']), **case['edits']['app']}
    launcher.save_ini(case['edits']['ini'], case['ini_lines'])
    launcher.atomic_write(launcher.CONFIG_FILE, json.dumps(app, indent=2, ensure_ascii=False))

    inherited = dict(os.environ)
    os.environ.clear()
    os.environ.update(case['env'])
    try:
        env = launcher.game_environment(app)
    finally:
        os.environ.clear()
        os.environ.update(inherited)
    kept = {key: value.replace(str(data), '<DATA_DIR>') for key, value in sorted(env.items())
            if key.startswith(('SPARTAN_', 'SDL_', 'PYTHON'))}

    folder = FIXTURES / name
    folder.mkdir(parents=True, exist_ok=True)
    # Fixtures hold the LF form: the launcher's text-mode writes give CRLF on Windows, the Rust launcher writes \n.
    for fixture, value in (('input.json', case), ('environment.json', kept)):
        text = json.dumps(value, indent=2, ensure_ascii=False) + '\n'
        (folder / fixture).write_bytes(text.encode('utf-8'))
    for written in ('settings.json', 'spartanport.ini'):
        (folder / written).write_bytes((data / written).read_bytes().replace(b'\r\n', b'\n'))


def run_collect_case(name, case):
    """The real Launcher.collect() of a hidden window opened over `case`, its edits set on the variables."""
    import tkinter as tk
    from tkinter import filedialog, messagebox, ttk
    data = prepare_data()
    (data / 'settings.json').write_text(json.dumps(case['settings'], indent=2), encoding='utf-8')
    if case['ini_lines']:
        (data / 'spartanport.ini').write_text('\n'.join(case['ini_lines']) + '\n', encoding='utf-8')
    launcher.list_displays = lambda backend='auto': []
    launcher.list_gpus = lambda: [('0', 'Fixture GPU', 'discrete')]
    root = tk.Tk()
    root.withdraw()
    try:
        app = launcher.Launcher(root, tk, ttk, filedialog, messagebox)
        root.withdraw()
        for key, value in case['edits']['app'].items():
            app.vars[key].set(value)
        for key, value in case['edits']['ini'].items():
            app.vars[key].set(value == '1' if key in launcher.INI_FLAGS else value)
        if not app.collect():
            raise SystemExit(f'{name}: collect() could not write the files')
    finally:
        for pending in root.tk.call('after', 'info'):  # drain_output and watch_changes
            root.after_cancel(pending)
        root.destroy()

    folder = FIXTURES / name
    shutil.rmtree(folder, ignore_errors=True)
    folder.mkdir(parents=True)
    text = json.dumps(case, indent=2, ensure_ascii=False) + '\n'
    (folder / 'input.json').write_bytes(text.encode('utf-8'))
    for written in ('settings.json', 'spartanport.ini', 'mods.json', 'patches.json'):
        if (data / written).is_file():  # mods.json and patches.json only when collect() wrote them
            (folder / written).write_bytes((data / written).read_bytes().replace(b'\r\n', b'\n'))


if __name__ == '__main__':
    try:
        for case_name, case_input in CASES.items():
            run_case(case_name, case_input)
            print('wrote', FIXTURES.relative_to(ROOT) / case_name)
        run_collect_case('collected', CASES['edited'])
        print('wrote', FIXTURES.relative_to(ROOT) / 'collected')
    finally:
        shutil.rmtree(WORK, ignore_errors=True)
