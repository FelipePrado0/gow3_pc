#!/usr/bin/env bash
# Packages the macOS build into dist/gow3-macos.zip
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."

[[ -f out/gow3-probe && -f out/gpu/libgow3gpu.dylib ]] || {
    echo "Binaries missing in out/. Run 'bash build.sh' first." >&2
    exit 1
}

dest=out/stage/gow3-macos
rm -rf -- out/stage
mkdir -p "$dest/bin/gpu" "$dest/bin/lib" "$dest/launcher"

echo "Copying executables and libraries..."
cp out/gow3-probe "$dest/bin/"
[[ -f out/gow3-gpu-capabilities ]] && cp out/gow3-gpu-capabilities "$dest/bin/"
cp out/gpu/libgow3gpu.dylib "$dest/bin/gpu/"

# Strip debug symbols from release package to reduce size
for bin in "$dest/bin/gow3-probe" "$dest/bin/gow3-gpu-capabilities" "$dest/bin/gpu/libgow3gpu.dylib"; do
    if [[ -f $bin ]]; then
        strip -S "$bin" 2>/dev/null || true
    fi
done

echo "Bundling dynamic libraries and fixing Mach-O references..."
chmod -R u+w "$dest/bin"

# 1. Recursively collect all Homebrew / non-system dependencies into bin/lib/
to_process=("$dest/bin/gow3-probe" "$dest/bin/gpu/libgow3gpu.dylib")
[[ -f "$dest/bin/gow3-gpu-capabilities" ]] && to_process+=("$dest/bin/gow3-gpu-capabilities")

while ((${#to_process[@]} > 0)); do
    item="${to_process[0]}"
    to_process=("${to_process[@]:1}")
    [[ -f "$item" ]] || continue
    while read -r dep; do
        [[ -f "$dep" ]] || continue
        dep_name=$(basename "$dep")
        if [[ ! -f "$dest/bin/lib/$dep_name" ]]; then
            cp -L "$dep" "$dest/bin/lib/$dep_name"
            chmod u+w "$dest/bin/lib/$dep_name"
            strip -S "$dest/bin/lib/$dep_name" 2>/dev/null || true
            to_process+=("$dest/bin/lib/$dep_name")
        fi
    done < <(otool -L "$item" | awk '/(\/usr\/local\/|\/opt\/homebrew\/|\/Cellar\/)/ {print $1}')
done

# Also bundle libMoltenVK.dylib and MoltenVK_icd.json if available
for mvk in /usr/local/lib/libMoltenVK.dylib /opt/homebrew/lib/libMoltenVK.dylib; do
    if [[ -f "$mvk" && ! -f "$dest/bin/lib/libMoltenVK.dylib" ]]; then
        cp -L "$mvk" "$dest/bin/lib/libMoltenVK.dylib"
        chmod u+w "$dest/bin/lib/libMoltenVK.dylib"
        strip -S "$dest/bin/lib/libMoltenVK.dylib" 2>/dev/null || true
        cat <<EOF > "$dest/bin/lib/MoltenVK_icd.json"
{
    "file_format_version": "1.0.0",
    "ICD": {
        "library_path": "./libMoltenVK.dylib",
        "api_version": "1.2.0"
    }
}
EOF
        break
    fi
done

echo "Fixing library IDs and install names..."
# 2. Update install names (ID and dependencies) inside all bundled dylibs
for dylib in "$dest/bin/lib"/*.dylib; do
    [[ -f "$dylib" ]] || continue
    dname=$(basename "$dylib")
    install_name_tool -id "@rpath/$dname" "$dylib" 2>/dev/null || true
    otool -L "$dylib" | awk '/(\/usr\/local\/|\/opt\/homebrew\/|\/Cellar\/)/ {print $1}' | while read -r dep; do
        dep_name=$(basename "$dep")
        install_name_tool -change "$dep" "@rpath/$dep_name" "$dylib" 2>/dev/null || true
    done
done

# 3. Update references and add rpaths in main binaries
for bin in "$dest/bin/gow3-probe" "$dest/bin/gow3-gpu-capabilities"; do
    [[ -f "$bin" ]] || continue
    otool -L "$bin" | awk '/(\/usr\/local\/|\/opt\/homebrew\/|\/Cellar\/)/ {print $1}' | while read -r dep; do
        dep_name=$(basename "$dep")
        install_name_tool -change "$dep" "@rpath/$dep_name" "$bin" 2>/dev/null || true
    done
    install_name_tool -add_rpath "@loader_path/lib" "$bin" 2>/dev/null || true
    install_name_tool -add_rpath "@executable_path/lib" "$bin" 2>/dev/null || true
done

# Update references and rpaths for libgow3gpu.dylib
if [[ -f "$dest/bin/gpu/libgow3gpu.dylib" ]]; then
    otool -L "$dest/bin/gpu/libgow3gpu.dylib" | awk '/(\/usr\/local\/|\/opt\/homebrew\/|\/Cellar\/)/ {print $1}' | while read -r dep; do
        dep_name=$(basename "$dep")
        install_name_tool -change "$dep" "@rpath/$dep_name" "$dest/bin/gpu/libgow3gpu.dylib" 2>/dev/null || true
    done
    install_name_tool -add_rpath "@loader_path/../lib" "$dest/bin/gpu/libgow3gpu.dylib" 2>/dev/null || true
    install_name_tool -add_rpath "@executable_path/lib" "$dest/bin/gpu/libgow3gpu.dylib" 2>/dev/null || true
fi

echo "Ad-hoc signing binaries and libraries for Gatekeeper/Rosetta 2..."
for f in "$dest/bin/lib"/*.dylib "$dest/bin/gpu"/*.dylib "$dest/bin/gow3-probe" "$dest/bin/gow3-gpu-capabilities"; do
    [[ -f "$f" ]] || continue
    codesign -s - --force "$f" 2>/dev/null || true
done

echo "Copying scripts, launcher, and support assets..."
cp run.sh run.py LICENSE README.md "$dest/"
cp -r scripts patches "$dest/"
cp launcher/gow3.png launcher/gow3.ico launcher/gowfont.ttf launcher/gow3_launcher_win.py "$dest/launcher/"

# Clean up unwanted metadata/caches
find "$dest" -name __pycache__ -prune -exec rm -rf {} +
find "$dest" -name '.DS_Store' -delete

# Ensure forbidden game data is not packaged
forbidden=$(find "$dest" \( -iname eboot.bin -o -iname sce_module -o -iname sce_sys -o -iname savedata \
    -o -iname user -o -iname cache -o -iname '*.log' -o -iname gow3.ini -o -iname settings.json \
    -o -iname mods.json -o -iname patches.json -o -iname fsr4_shaders \
    -o -iname '*.spv' -o -iname '*.pkg' -o -iname '*.sprx' -o -iname '*.prx' \) -print)
if [[ -n $forbidden ]]; then
    echo "Package holds files that must not ship:" >&2
    echo "$forbidden" >&2
    exit 1
fi

mkdir -p dist
echo "Creating dist/gow3-macos.zip..."
(cd out/stage && zip -r -q "../../dist/gow3-macos.zip" gow3-macos)
(cd dist && shasum -a 256 gow3-macos.zip > gow3-macos.zip.sha256)

echo "Successfully created dist/gow3-macos.zip and dist/gow3-macos.zip.sha256"
