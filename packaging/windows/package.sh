#!/usr/bin/env bash
# Builds dist/spartanport-windows/ (and dist/spartanport-windows.zip): SpartanPort.exe (the Rust launcher,
# built outside MSYS2 with `cd launcher-rust && cargo build --release --locked`; it must run inside launcher-rust/ to read its .cargo/config.toml),
# spartanport-probe.exe with the MSYS2 CLANG64 DLLs it needs and the controller diagrams the runner reads.
# Run from an MSYS2 CLANG64 shell after `bash build.sh` and the cargo build. No Python is packaged. The
# temporal upscalers are off, so neither the FSR 4 assets nor the DLSS runtime are packaged.
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."
source ./msys2-env.sh
[[ -f out/spartanport-probe.exe && -f out/spartanport-gpu-capabilities.exe && -f out/spartanport-play.exe ]] || { echo 'Build first: bash build.sh' >&2; exit 1; }

launcher=launcher-rust/target/release/spartanport-launcher.exe
[[ -f $launcher ]] || { echo 'Build the launcher first, outside MSYS2: cd launcher-rust && cargo build --release --locked' >&2; exit 1; }

# The package is assembled in a fresh staging folder and zipped from there; dist/spartanport-windows
# (a playable copy that may hold saves and settings) is only refreshed afterwards.
dest=out/stage/spartanport-windows
rm -rf -- out/stage
mkdir -p "$dest/bin" "$dest/launcher"
cp "$launcher" "$dest/SpartanPort.exe"
llvm-strip -o "$dest/Play SpartanPort.exe" out/spartanport-play.exe
# The same launcher and windowless start under their former names, which installed updaters look for in
# the download and existing shortcuts point to (core::legacy::LEGACY_EXE and legacy::LEGACY_PLAY_EXE;
# launcher-rust/tests/packaging.rs checks the names).
cp "$dest/SpartanPort.exe" "$dest/God of War III.exe"
cp "$dest/Play SpartanPort.exe" "$dest/Play God of War III.exe"
cp launcher/spartanport.ico launcher/spartanport.png launcher/logo.png launcher/Cinzel-Black.ttf launcher/OFL-Cinzel.txt "$dest/launcher/"
cp launcher/BodoniModa.ttf launcher/OFL-BodoniModa.txt "$dest/launcher/"
# The executables without debug information (out/ keeps the symbols for crash reports).
for exe in spartanport-probe.exe spartanport-gpu-capabilities.exe; do
    llvm-strip --strip-debug -o "$dest/bin/$exe" "out/$exe"
done
# Every DLL the executables load from the CLANG64 tree (SDL3, FFmpeg, Vulkan loader, ...).
ldd "$dest/bin/spartanport-probe.exe" "$dest/bin/spartanport-gpu-capabilities.exe" |
    awk '/\/clang64\/bin\// {print $3}' | sort -u | while read -r dll; do
        cp -u "$dll" "$dest/bin/"
    done
mkdir -p "$dest/scripts" "$dest/docs"
cp -r scripts/controller_diagrams "$dest/scripts/"
cp -r patches "$dest/"
cp docs/VOICE_REPLACEMENT.md "$dest/docs/"
cp LICENSE README.md packaging/windows/README-Windows.txt "$dest/"
if [[ -f release.json ]]; then cp release.json "$dest/"; fi
# No game files and nothing a player made (saves, shader caches built from the game's shaders,
# settings, logs) may ship: stop when one is found.
forbidden=$(find "$dest" \( -iname eboot.bin -o -iname sce_module -o -iname sce_sys -o -iname savedata \
    -o -iname user -o -iname cache -o -iname '*.log' -o -iname spartanport.ini -o -iname settings.json \
    -o -iname game_icon.ico \
    -o -iname mods.json -o -iname patches.json -o -iname fsr4_shaders -o -iname 'nvngx_dlss*.dll' \
    -o -iname '*.py' -o -iname _internal \
    -o -iname '*.spv' -o -iname '*.pkg' -o -iname '*.sprx' -o -iname '*.prx' \) -print)
if [[ -n $forbidden ]]; then
    echo "Package holds files that must not ship:" >&2
    echo "$forbidden" >&2
    exit 1
fi
mkdir -p dist
rm -f dist/spartanport-windows.zip
(cd out/stage && powershell -NoProfile -Command \
    "Compress-Archive -Path spartanport-windows -DestinationPath ../../dist/spartanport-windows.zip")

# Refresh dist/spartanport-windows, keeping what players create there (saves, settings, mods), and
# only while nothing runs from it: deleting a running launcher's files breaks it.
play=dist/spartanport-windows
running=$(powershell -NoProfile -Command \
    "@(Get-Process | Where-Object { \$_.Path -like '$(cygpath -w "$PWD/$play")\\*' }).Count" | tr -d '\r')
if [[ ${running:-0} != 0 ]]; then
    echo "dist/spartanport-windows is in use ($running processes): not refreshed; the zip is ready." >&2
else
    mkdir -p "$play"
    find "$play" -mindepth 1 -maxdepth 1 ! -name user ! -name mods ! -name spartanport.ini \
        ! -name mods.json ! -name patches.json ! -name settings.json ! -name game_icon.ico ! -name gamecontrollerdb.txt -exec rm -rf {} +
    cp -r "$dest/." "$play/"
fi
du -sh "$dest" dist/spartanport-windows.zip
