#!/usr/bin/env bash
# Builds dist/gow3-macos.dmg (and dist/God of War III.app): the launcher frozen with PyInstaller
# into an application, so players need no Python, holding in its Contents/Resources the game's
# x86-64 executables, the libraries they load (SDL3, FFmpeg, the Vulkan loader), the KosmicKrisp
# Vulkan driver, the preparation scripts and run.py. Run after `bash build.sh`.
# Freezing uses Python 3 with Tk (PYTHON overrides python3; brew install python python-tk) and a
# private venv in out/pyenv with PyInstaller and Pillow; the application runs on the Python's
# architecture (arm64 on Apple Silicon) and the game under Rosetta 2, as from the repository.
# It is signed ad hoc, not with an Apple Developer ID: players allow it once (README-macOS.txt).
# Its data (generated files, saves, gow3.ini) go to ~/Library/Application Support/gow3.
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."
[[ $(uname -s) == Darwin ]] || { echo 'packaging/macos/package.sh is for macOS.' >&2; exit 1; }
for file in out/gow3-probe out/gow3-gpu-capabilities out/gpu/libgow3gpu.dylib \
            out/vulkan/kosmickrisp_mesa_icd.json out/vulkan/libvulkan_kosmickrisp.dylib; do
    [[ -f $file ]] || { echo "Build first: bash build.sh ($file is missing)" >&2; exit 1; }
done
deps=${GOW3_MACOS_DEPS:-$PWD/out/macos-deps}
name='God of War III'

python=${PYTHON:-$(command -v python3 || true)}
if [[ -z $python ]] || ! "$python" -c 'import tkinter' 2>/dev/null; then
    echo 'Need Python 3 with Tk (brew install python python-tk) or PYTHON=/path/to/python3.' >&2
    exit 1
fi
if [[ ! -x out/pyenv/bin/python ]]; then
    "$python" -m venv out/pyenv
fi
out/pyenv/bin/python -m pip install -q --disable-pip-version-check pyinstaller pillow
# The scripts run inside the frozen launcher (--run, --script), and PyInstaller follows only the
# launcher's own imports: the standard modules run.py and scripts/ import come along.
hidden=()
for module in $(out/pyenv/bin/python -c '
import ast
import sys
from pathlib import Path
names = set()
for path in [Path("run.py"), *Path("scripts").glob("*.py")]:
    for node in ast.walk(ast.parse(path.read_text(encoding="utf-8"))):
        if isinstance(node, ast.Import):
            names.update(alias.name for alias in node.names)
        elif isinstance(node, ast.ImportFrom) and node.module and not node.level:
            names.add(node.module)
windows_only = {"msvcrt", "winreg", "_winapi", "ctypes.wintypes"}
print(" ".join(sorted(n for n in names - windows_only if n.split(".")[0] in sys.stdlib_module_names)))
'); do
    hidden+=(--hidden-import "$module")
done
mkdir -p out/pyi-work
sips -s format icns launcher/gow3.png --out out/pyi-work/gow3.icns >/dev/null
out/pyenv/bin/python -m PyInstaller --noconfirm --clean --log-level WARN --windowed --name "$name" \
    --icon "$PWD/out/pyi-work/gow3.icns" --osx-bundle-identifier pc.gow3.launcher \
    --distpath out/pyi-dist --workpath out/pyi-work --specpath out/pyi-work --paths "$PWD/scripts" \
    "${hidden[@]}" "$PWD/launcher/gow3_launcher_win.py"

# The application is assembled in a fresh staging folder, the disk image's contents.
stage=out/stage/gow3-macos
app=$stage/$name.app
res=$app/Contents/Resources
rm -rf -- out/stage
mkdir -p "$stage"
ditto "out/pyi-dist/$name.app" "$app"
mkdir -p "$res/bin/gpu" "$res/bin/lib" "$res/bin/vulkan" "$res/launcher"
# The executables without debug information (out/ keeps it for crash reports).
for exe in gow3-probe gow3-gpu-capabilities; do
    strip -S -o "$res/bin/$exe" "out/$exe"
done
strip -S -o "$res/bin/gpu/libgow3gpu.dylib" out/gpu/libgow3gpu.dylib
# Every library they load through @rpath, from the build's dependency folder: the executables'
# run paths find them in bin/lib (@executable_path/lib), and libgow3gpu.dylib in bin/gpu.
queue=(out/gow3-probe out/gow3-gpu-capabilities out/gpu/libgow3gpu.dylib)
i=0
while ((i < ${#queue[@]})); do
    file=${queue[i]}
    i=$((i + 1))
    while read -r library; do
        [[ $library == libgow3gpu.dylib || -f $res/bin/lib/$library ]] && continue
        [[ -f $deps/lib/$library ]] || { echo "$library (loaded by $file) is not in $deps/lib" >&2; exit 1; }
        cp -L "$deps/lib/$library" "$res/bin/lib/$library"
        chmod 755 "$res/bin/lib/$library"
        queue+=("$deps/lib/$library")
    done < <(otool -L "$file" | sed -n 's|^[[:space:]]*@rpath/\([^ ]*\) (.*|\1|p')
done
# The loader's process finds the driver next to itself (src/compat_mac.c).
cp out/vulkan/kosmickrisp_mesa_icd.json out/vulkan/libvulkan_kosmickrisp.dylib "$res/bin/vulkan/"
# Run paths into this checkout would load libraries from it rather than from the application.
for file in "$res/bin/gow3-probe" "$res/bin/gow3-gpu-capabilities" "$res/bin/gpu/libgow3gpu.dylib"; do
    rpaths=$(otool -l "$file" | sed -n 's|^ *path \(.*\) (offset [0-9]*)$|\1|p')
    while IFS= read -r rpath; do
        # (Its warning: the signature is renewed below.)
        [[ -z $rpath || $rpath == @* ]] || install_name_tool -delete_rpath "$rpath" "$file" 2>/dev/null
    done <<< "$rpaths"
done
cp -R scripts patches "$res/"
cp run.py "$res/"
cp launcher/gow3.png launcher/gowfont.ttf "$res/launcher/"
find "$res" -name __pycache__ -prune -exec rm -r {} +
plist=$app/Contents/Info.plist
version=$(sed -n "s/^VERSION = '\(.*\)'$/\1/p" launcher/gow3_launcher_win.py)
plutil -replace CFBundleShortVersionString -string "$version" "$plist"
plutil -replace CFBundleVersion -string "$version" "$plist"
plutil -replace LSMinimumSystemVersion -string 26.0 "$plist"  # KosmicKrisp needs macOS 26
plutil -replace LSApplicationCategoryType -string public.app-category.action-games "$plist"

# No game files and nothing a player made (saves, shader caches built from the game's shaders,
# settings, logs) may ship: stop when one is found.
forbidden=$(find "$stage" \( -iname eboot.bin -o -iname sce_module -o -iname sce_sys -o -iname savedata \
    -o -iname user -o -iname cache -o -iname '*.log' -o -iname gow3.ini -o -iname settings.json \
    -o -iname mods.json -o -iname patches.json -o -iname fsr4_shaders -o -iname '*.spv' -o -iname '*.pkg' \
    -o -iname '*.sprx' -o -iname '*.prx' \) -print)
if [[ -n $forbidden ]]; then
    echo "Package holds files that must not ship:" >&2
    echo "$forbidden" >&2
    exit 1
fi

# Ad hoc signatures, inside out: Apple Silicon runs only signed code, and the application's own
# signature seals everything in Resources (PyInstaller signed what it put in Frameworks).
find "$res/bin" -type f \( -perm -u+x -o -name '*.dylib' \) -print0 | xargs -0 codesign --force --sign -
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"

cp packaging/macos/README-macOS.txt LICENSE "$stage/"
ln -s /Applications "$stage/Applications"
mkdir -p dist
# Made beside the last one and renamed over it: a mounted copy of that one keeps working.
hdiutil create -quiet -ov -volname "$name" -srcfolder "$stage" -fs HFS+ -format UDZO dist/gow3-macos.new.dmg
mv -f dist/gow3-macos.new.dmg dist/gow3-macos.dmg

# dist/God of War III.app: a copy to play from this Mac, refreshed only while nothing runs from
# it (its data lives in Application Support, so nothing in it needs keeping).
play=dist/$name.app
if pgrep -f "$PWD/$play/" >/dev/null; then
    echo "$play is in use: not refreshed; the disk image is ready." >&2
else
    rm -rf -- "$play"
    ditto "$app" "$play"
fi
du -sh "$app" dist/gow3-macos.dmg
