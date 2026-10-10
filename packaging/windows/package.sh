#!/usr/bin/env bash
# Builds dist/gow3-windows/ (and dist/gow3-windows.zip): God of War III.exe (the launcher, frozen
# with PyInstaller so players need no Python), gow3-probe.exe with the MSYS2 CLANG64 DLLs it needs,
# the preparation scripts and run.py. Run from an MSYS2 CLANG64 shell after `bash build.sh`.
# Freezing uses a Windows Python 3.10+ (python.org; WINPYTHON overrides) and a private venv in
# out/pyenv with PyInstaller. The temporal upscalers are off, so neither the FSR 4 assets nor the
# DLSS runtime are packaged.
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."
source ./msys2-env.sh
[[ -f out/gow3-probe.exe && -f out/gow3-gpu-capabilities.exe && -f out/gow3-play.exe ]] || { echo 'Build first: bash build.sh' >&2; exit 1; }

# A Windows Python (not MSYS2's) for PyInstaller.
python=${WINPYTHON:-}
if [[ -z $python ]]; then
    for candidate in /c/Python3*/python.exe "${LOCALAPPDATA:-/c/Users/$USER/AppData/Local}"/Programs/Python/Python3*/python.exe; do
        [[ -x $candidate ]] && python=$candidate
    done
fi
[[ -n $python ]] || { echo 'Need a Windows Python 3 (python.org) or WINPYTHON=path\to\python.exe' >&2; exit 1; }
# Windows Python needs USERPROFILE (some MSYS2 shells start without it).
export USERPROFILE=${USERPROFILE:-$(cygpath -w "/c/Users/$(id -un)")}
if [[ ! -x out/pyenv/Scripts/python.exe ]]; then
    "$python" -m venv out/pyenv
fi
out/pyenv/Scripts/python.exe -m pip install -q --disable-pip-version-check pyinstaller
# The scripts run inside God of War III.exe (--script): the standard modules they import come along.
hidden=()
for module in argparse base64 collections hashlib json re shutil struct tempfile xml.etree.ElementTree \
              urllib.request ctypes.wintypes; do
    hidden+=(--hidden-import "$module")
done
out/pyenv/Scripts/python.exe -m PyInstaller --noconfirm --clean --log-level WARN --windowed \
    --name "God of War III" --icon "$(cygpath -w "$PWD/launcher/gow3.ico")" --distpath out/pyi-dist \
    --workpath out/pyi-work --specpath out/pyi-work --paths "$(cygpath -w "$PWD/scripts")" "${hidden[@]}" \
    "$(cygpath -w "$PWD/launcher/gow3_launcher_win.py")"

# The package is assembled in a fresh staging folder and zipped from there; dist/gow3-windows
# (a playable copy that may hold saves and settings) is only refreshed afterwards.
dest=out/stage/gow3-windows
rm -rf -- out/stage
mkdir -p "$dest/bin" "$dest/launcher"
cp -r "out/pyi-dist/God of War III/." "$dest/"
llvm-strip -o "$dest/Play God of War III.exe" out/gow3-play.exe
cp launcher/gow3.ico launcher/gow3.png "$dest/launcher/"
# The executables without debug information (out/ keeps the symbols for crash reports).
for exe in gow3-probe.exe gow3-gpu-capabilities.exe; do
    llvm-strip --strip-debug -o "$dest/bin/$exe" "out/$exe"
done
# Every DLL the executables load from the CLANG64 tree (SDL3, FFmpeg, Vulkan loader, ...).
ldd "$dest/bin/gow3-probe.exe" "$dest/bin/gow3-gpu-capabilities.exe" |
    awk '/\/clang64\/bin\// {print $3}' | sort -u | while read -r dll; do
        cp -u "$dll" "$dest/bin/"
    done
cp -r scripts patches "$dest/"
cp run.py LICENSE README.md packaging/windows/README-Windows.txt "$dest/"
find "$dest" -name __pycache__ -prune -exec rm -r {} +
# No game files and nothing a player made (saves, shader caches built from the game's shaders,
# settings, logs) may ship: stop when one is found.
forbidden=$(find "$dest" \( -iname eboot.bin -o -iname sce_module -o -iname sce_sys -o -iname savedata \
    -o -iname user -o -iname cache -o -iname '*.log' -o -iname gow3.ini -o -iname settings.json \
    -o -iname mods.json -o -iname patches.json -o -iname fsr4_shaders -o -iname 'nvngx_dlss*.dll' \
    -o -iname '*.spv' -o -iname '*.pkg' -o -iname '*.sprx' -o -iname '*.prx' \) -print)
if [[ -n $forbidden ]]; then
    echo "Package holds files that must not ship:" >&2
    echo "$forbidden" >&2
    exit 1
fi
mkdir -p dist
rm -f dist/gow3-windows.zip
(cd out/stage && powershell -NoProfile -Command \
    "Compress-Archive -Path gow3-windows -DestinationPath ../../dist/gow3-windows.zip")

# Refresh dist/gow3-windows, keeping what players create there (saves, settings, mods), and
# only while nothing runs from it: deleting a running launcher's files breaks it.
play=dist/gow3-windows
running=$(powershell -NoProfile -Command \
    "@(Get-Process | Where-Object { \$_.Path -like '$(cygpath -w "$PWD/$play")\\*' }).Count" | tr -d '\r')
if [[ ${running:-0} != 0 ]]; then
    echo "dist/gow3-windows is in use ($running processes): not refreshed; the zip is ready." >&2
else
    mkdir -p "$play"
    find "$play" -mindepth 1 -maxdepth 1 ! -name user ! -name mods ! -name gow3.ini \
        ! -name mods.json ! -name patches.json -exec rm -rf {} +
    cp -r "$dest/." "$play/"
fi
du -sh "$dest" dist/gow3-windows.zip
