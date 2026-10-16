#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")"
if [[ ${1:-} == --software ]]; then
    shift
    if [[ -z ${VK_DRIVER_FILES:-} ]]; then
        for candidate in /run/opengl-driver/share/vulkan/icd.d/lvp_icd*.json /usr/share/vulkan/icd.d/lvp_icd*.json; do
            if [[ -f $candidate ]]; then export VK_DRIVER_FILES=$candidate; break; fi
        done
    fi
    if [[ -z ${VK_DRIVER_FILES:-} ]]; then echo 'Lavapipe not found; set VK_DRIVER_FILES.' >&2; exit 1; fi
    export VK_LOADER_LAYERS_DISABLE='~implicit~'
fi
if [[ $(uname -s) == Darwin ]]; then
    # Clear quarantine attributes from bundled files if extracted on macOS
    xattr -dr com.apple.quarantine "$PWD" 2>/dev/null || true
    # Ensure bundled binaries and libraries are executable
    if [[ -d bin ]]; then
        chmod -R u+rx bin 2>/dev/null || true
    fi
    # If prebuilt bundled libraries exist, add them to dyld search paths
    if [[ -d bin/lib ]]; then
        export DYLD_FALLBACK_LIBRARY_PATH="$PWD/bin/lib:${DYLD_FALLBACK_LIBRARY_PATH:-}"
        export DYLD_LIBRARY_PATH="$PWD/bin/lib:${DYLD_LIBRARY_PATH:-}"
    fi
    # If bundled MoltenVK ICD exists, point to it
    if [[ -f bin/lib/MoltenVK_icd.json && -z ${VK_DRIVER_FILES:-} && -z ${VK_ICD_FILENAMES:-} ]]; then
        export VK_DRIVER_FILES="$PWD/bin/lib/MoltenVK_icd.json"
    fi
    # If /usr/local has x86_64 libraries, prioritize /usr/local for the x86_64 PS4 runtime
    if [[ -f /usr/local/lib/libSDL3.dylib ]] && file /usr/local/lib/libSDL3.dylib | grep -q 'x86_64'; then
        export PATH="/usr/local/bin:/opt/homebrew/bin:${PATH}"
        export PKG_CONFIG_PATH="/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig:/opt/homebrew/lib/pkgconfig:/opt/homebrew/share/pkgconfig:${PKG_CONFIG_PATH:-}"
    elif [[ $(uname -m) == x86_64 ]]; then
        export PATH="/usr/local/bin:/opt/homebrew/bin:${PATH}"
        export PKG_CONFIG_PATH="/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig:/opt/homebrew/lib/pkgconfig:/opt/homebrew/share/pkgconfig:${PKG_CONFIG_PATH:-}"
    else
        export PATH="/opt/homebrew/bin:/usr/local/bin:${PATH}"
        export PKG_CONFIG_PATH="/opt/homebrew/lib/pkgconfig:/opt/homebrew/share/pkgconfig:/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig:${PKG_CONFIG_PATH:-}"
    fi
    if [[ -n ${VULKAN_SDK:-} ]]; then
        export PKG_CONFIG_PATH="$VULKAN_SDK/lib/pkgconfig:${PKG_CONFIG_PATH}"
    fi
    if [[ -z ${VK_DRIVER_FILES:-} && -z ${VK_ICD_FILENAMES:-} ]]; then
        for candidate in "$PWD/bin/lib/MoltenVK_icd.json" \
                         /usr/local/share/vulkan/icd.d/MoltenVK_icd.json \
                         /usr/local/etc/vulkan/icd.d/MoltenVK_icd.json \
                         ${VULKAN_SDK:-/nonexistent}/share/vulkan/icd.d/MoltenVK_icd.json \
                         /opt/homebrew/share/vulkan/icd.d/MoltenVK_icd.json \
                         /opt/homebrew/etc/vulkan/icd.d/MoltenVK_icd.json; do
            if [[ -f $candidate ]]; then export VK_DRIVER_FILES=$candidate; break; fi
        done
    fi
fi
# If prebuilt binaries exist in bin/, treat as prebuilt (no rebuild needed)
if [[ -z ${GOW3_PREBUILT:-} && -f bin/gow3-probe && -f bin/gpu/libgow3gpu.dylib ]]; then
    export GOW3_PREBUILT=1
fi
# On Apple Silicon macOS, if binaries are missing, automatically download prebuilt package
if [[ $(uname -s) == Darwin && $(uname -m) == arm64 && -z ${GOW3_PREBUILT:-} ]]; then
    if [[ ! -f bin/gow3-probe && ! -f out/gow3-probe ]]; then
        echo "Prebuilt binaries not found. Downloading latest package for Apple Silicon..."
        bash packaging/macos/download-prebuilt.sh || true
        if [[ -f bin/gow3-probe && -f bin/gpu/libgow3gpu.dylib ]]; then
            export GOW3_PREBUILT=1
        fi
    fi
fi
# GOW3_PREBUILT=1 (packaged builds, the AppImage): bin/gow3-probe and its GPU library are installed
# next to this script; nothing is built and no nix-shell is needed.
# GOW3_DATA_DIR: writable directory for the generated files (out/), saves (user/) and gow3.ini;
# by default this directory. The preparation itself is in run.py.
data=${GOW3_DATA_DIR:-.}
if [[ -z ${GOW3_PREBUILT:-} && -z ${GOW3_IN_NIX_SHELL:-} ]] && ! { command -v pkg-config >/dev/null && pkg-config --exists vulkan sdl3; } && command -v nix-shell >/dev/null; then
    args=''; if (( $# )); then args=$(printf '%q ' "$@"); fi
    exec env GOW3_IN_NIX_SHELL=1 nix-shell shell.nix --run "bash run.sh $args"
fi
if [[ -z ${PYTHON:-} ]]; then
    PYTHON=$(command -v python3 || true)
    if [[ -z $PYTHON ]]; then
        for candidate in /nix/store/*-python3-*/bin/python3; do
            if [[ -x $candidate ]]; then PYTHON=$candidate; break; fi
        done
    fi
fi
if [[ -z ${PYTHON:-} ]]; then echo 'Install Python 3 or set PYTHON.' >&2; exit 1; fi
# FSR 4: faster post passes next to the downloaded ones (incremental; tools/fsr4_optimize.sh).
if [[ -z ${GOW3_PREBUILT:-} && -d fsr4_shaders ]] && command -v spirv-cross >/dev/null; then
    bash tools/fsr4_optimize.sh || echo 'FSR 4: optimized post passes not built' >&2
fi
if [[ -z ${GOW3_PREBUILT:-} ]]; then bash build.sh; fi
export GOW3_DATA_DIR=$data
exec "$PYTHON" run.py "$@"
