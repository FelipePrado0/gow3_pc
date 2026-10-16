#!/usr/bin/env bash
# macOS: builds what changed, then starts the game (or the launcher).
#   ./play-mac.sh                   the dump found in ~/Downloads, ~/Desktop, ~/Documents, ~/Games
#                                   or next to this folder
#   ./play-mac.sh FOLDER [UPDATE]   the dump in FOLDER or below it; UPDATE: an update dumped apart
#   ./play-mac.sh --launcher        the launcher (settings, saves, mods), the dump found preselected
# A dump is a folder with eboot.bin, sce_module and sce_sys (CUSA01623 or CUSA01715); .pkg files
# cannot be read. GOW3_GAME_DIR, GOW3_PATCH_DIR and the other variables of run.py apply.
set -euo pipefail
caller=$PWD
cd -- "$(dirname -- "$0")"

if [[ $(uname -s) != Darwin ]]; then
    echo 'play-mac.sh is for macOS; use run.sh.' >&2
    exit 1
fi
python=${PYTHON:-$(command -v python3 || true)}
if [[ -z $python ]]; then
    echo 'Install Python 3 (brew install python python-tk) or set PYTHON.' >&2
    exit 1
fi

# Prints the exports of GOW3_GAME_DIR and GOW3_PATCH_DIR. $1: the folder the script was started from;
# $2: "required" (stops without a dump) or "optional" (the launcher asks for the folder itself); then
# the folders given.
find_game() {
    "$python" - "$@" <<'PY'
import os, re, sys
from pathlib import Path
from shlex import quote

sys.path.insert(0, 'scripts')
from prepare import sfo

TITLE_IDS = ('CUSA01623', 'CUSA01715')
REQUIRED = ('sce_sys/param.sfo', 'sce_module/libc.prx', 'sce_module/libSceFios2.prx')
SKIP = re.compile(r'^\.|^(node_modules|Library|__pycache__)$|\.(app|photoslibrary|xcresult)$')
PACKAGE = re.compile(r'CUSA0(1623|1715)|god.?of.?war', re.I)
HOME = Path.home()
required = sys.argv[2] == 'required'


def shown(path):
    return '~' + str(path)[len(str(HOME)):] if path == HOME or HOME in path.parents else str(path)


def fail(message):
    sys.exit(message if required else 0)


def game_sfo(folder):
    """param.sfo of a God of War III dump (the game or an update), or None."""
    try:
        values = sfo((folder / 'sce_sys/param.sfo').read_bytes())
    except Exception:
        return None
    title = str(values.get('TITLE', '')).replace('™', '').replace('®', '').lower()
    return values if values.get('TITLE_ID') in TITLE_IDS or 'god of war iii' in title else None


def search(roots):
    """{folder: param.sfo} of the dumps, and the God of War III packages, below (root, depth) pairs."""
    port = Path.cwd().resolve()
    dumps, packages = {}, []
    for root, depth in roots:
        root = root.resolve()
        for top, dirs, files in os.walk(root):
            here = Path(top)
            values = 'eboot.bin' in files and game_sfo(here)
            if values:
                dumps.setdefault(here, values)
            packages += [here / name for name in sorted(files) if name.lower().endswith('.pkg') and PACKAGE.search(name)]
            deeper = not values and len(here.relative_to(root).parts) < depth
            dirs[:] = [d for d in dirs if deeper and not SKIP.search(d) and here / d != port]
    return dumps, list(dict.fromkeys(packages))


def env_path(name):
    return Path(os.environ[name]).expanduser() if os.environ.get(name) else None


given = [Path(sys.argv[1], Path(arg).expanduser()) for arg in sys.argv[3:]]
folder = given[0] if given else env_path('GOW3_GAME_DIR')
update = given[1] if len(given) > 1 else env_path('GOW3_PATCH_DIR')
if folder and not folder.is_dir():
    fail(f'No folder {shown(folder)}.')
if folder and (folder / 'eboot.bin').is_file():
    game, dumps = folder.resolve(), {}
else:
    roots = [(folder, 4)] if folder else [(Path.cwd().parent, 2)] + [
        (HOME / name, 4) for name in ('Downloads', 'Desktop', 'Documents', 'Games') if (HOME / name).is_dir()]
    dumps, packages = search(roots)
    games = sorted((path for path, values in dumps.items() if values.get('CATEGORY') != 'gp'),
                   key=lambda path: (any(not (path / name).is_file() for name in REQUIRED), str(path)))
    where = shown(folder) if folder else '~/Downloads, ~/Desktop, ~/Documents, ~/Games or next to the port'
    if not games and packages:
        listed = '\n'.join('  ' + shown(path) for path in packages[:4])
        fail(f'No game folder in {where}, only packages (.pkg), which the port cannot read:\n{listed}\n'
             'It needs the game as a folder with eboot.bin, sce_module and sce_sys: a dump from your own PS4.')
    if not games:
        fail(f'No God of War III dump in {where}.\n'
             'Pass its folder (the one with eboot.bin, sce_module and sce_sys): ./play-mac.sh /path/to/CUSA01623')
    game = games[0]
    for other in games[1:]:
        print(f'Also found {shown(other)}; pass its folder to play that one.', file=sys.stderr)
missing = [name for name in REQUIRED if not (game / name).is_file()]
if missing:
    fail(f'{shown(game)} is an incomplete dump: it has no {", ".join(missing)}.')
values = game_sfo(game) or {}
if update is None:
    # An update dumped apart from the game: a newer one of the same title, next to it.
    if not dumps:
        dumps, _ = search([(game.parent, 2)])
    newer = [(found.get('APP_VER', ''), path) for path, found in dumps.items()
             if found.get('CATEGORY') == 'gp' and found.get('TITLE_ID') == values.get('TITLE_ID')
             and found.get('APP_VER', '') > values.get('APP_VER', '')]
    update = max(newer)[1] if newer else None
elif not (update / 'eboot.bin').is_file():
    fail(f'No eboot.bin in the update folder {shown(update)}.')
print(f'Game: {shown(game)} ({values.get("TITLE_ID", "?")}, version {values.get("APP_VER", "?")})', file=sys.stderr)
print(f'export GOW3_GAME_DIR={quote(str(game))}')
if update:
    print(f'Update: {shown(update)} (version {(game_sfo(update) or {}).get("APP_VER", "?")})', file=sys.stderr)
    print(f'export GOW3_PATCH_DIR={quote(str(update.resolve()))}')
PY
}

mode=required
if [[ ${1:-} == --launcher ]]; then
    shift
    mode=optional
    if ! "$python" -c 'import tkinter' 2>/dev/null; then
        echo "The launcher needs Python with Tk: brew install python-tk, or set PYTHON." >&2
        exit 1
    fi
fi
exports=$(find_game "$caller" "$mode" "$@")
eval "$exports"
# Builds what changed; quietly once the port is built (the first build shows its progress: the
# x86-64 libraries take several minutes).
if [[ -x out/gow3-probe ]]; then
    echo 'Updating the build (log: out/play-build.log)'
    if ! bash build.sh > out/play-build.log 2>&1; then
        grep -v '^\[' out/play-build.log | tail -40 >&2
        echo 'Build failed (full log: out/play-build.log)' >&2; exit 1
    fi
else
    bash build.sh
fi
if [[ $mode == optional ]]; then
    exec "$python" launcher/gow3_launcher_win.py
fi
exec "$python" run.py
