#!/usr/bin/env bash
# tools/fsr4cap/build_assets.sh <amd_fidelityfx_upscaler_dx12.dll 4.1.x> <amd_fidelityfx_loader_dx12.dll 2.3.x>
#
# Builds the FSR 4.1.1 asset set (fsr4_411/: SPIR-V of every pass, model weights) from AMD's
# upscaler DLL, for upscaler=fsr411 (gpu/.../fsr411). Nothing of AMD's is downloaded or
# shipped: the DLLs are the user's own (OptiScaler ships them, many games do).
#   1. builds dxil-spirv (pinned commit + dxil-spirv-class-bindings.patch) and fsr4cap.exe (MinGW);
#   2. runs fsr4cap.exe under umu-run (Proton, vkd3d-proton): every output size class and quality
#      ratio, recording the D3D12 frames (capture_all.sh);
#   3. extract.py translates them, checks the replay rules and writes fsr4_411/;
#   4. with VERIFY=1, verify.sh compares the replay with the DLL byte by byte.
# Needs nix-shell (or the tools on PATH: x86_64-w64-mingw32-gcc, cmake, ninja, python3,
# spirv-dis/spirv-as, umu-run), network for the two git repositories, and a Proton build
# (PROTONPATH, default: newest GE-Proton in Steam's compatibilitytools.d).
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."
upscaler=$(realpath "${1:?upscaler DLL}")
loader=$(realpath "${2:?loader DLL}")
work=$PWD/out/fsr4cap
mkdir -p "$work"

if [[ -z ${GOW3_FSR4CAP_SHELL:-} ]] && command -v nix-shell >/dev/null; then
    exec env GOW3_FSR4CAP_SHELL=1 nix-shell -p pkgsCross.mingwW64.buildPackages.gcc cmake ninja gcc \
        python3 spirv-tools umu-launcher git --run "bash $(printf %q "$0") $(printf %q "$upscaler") $(printf %q "$loader")"
fi

# dxil-spirv: DXIL -> SPIR-V as vkd3d-proton translates it.
dx=$work/dxil-spirv
if [[ ! -x $dx/build/dxil-spirv ]]; then
    rm -rf "$dx"
    git clone -q https://github.com/HansKristian-Work/dxil-spirv.git "$dx"
    git -C "$dx" checkout -q 7dc52786cdb1f53c54dd5c5c2698dff00ea5f0a3
    git -C "$dx" submodule update -q --init --recursive
    git -C "$dx" apply "$PWD/tools/fsr4cap/dxil-spirv-class-bindings.patch"
    cmake -S "$dx" -B "$dx/build" -G Ninja -DCMAKE_BUILD_TYPE=Release >/dev/null
    ninja -C "$dx/build" dxil-spirv >/dev/null
fi

# FidelityFX API headers (MIT), for fsr4cap.exe.
sdk=$work/ffxsdk
if [[ ! -f $sdk/Kits/FidelityFX/api/include/ffx_api.h ]]; then
    rm -rf "$sdk"
    git clone -q --filter=blob:none --sparse https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK.git "$sdk"
    git -C "$sdk" checkout -q 60f4ea81909200d8542eca14dccb2628b763a9a3
    git -C "$sdk" sparse-checkout set --no-cone /Kits/FidelityFX/api/include/ /Kits/FidelityFX/upscalers/include/
fi
k=$sdk/Kits/FidelityFX
x86_64-w64-mingw32-gcc -std=c11 -O1 -Wall -I"$k/api/include" -I"$k/upscalers/include" \
    tools/fsr4cap/fsr4cap.c tools/fsr4cap/capture.c tools/fsr4cap/rootsig.c \
    -o "$work/fsr4cap.exe" -ld3d12 -ldxguid -static
cp "$upscaler" "$work/amd_fidelityfx_upscaler_dx12.dll"
cp "$loader" "$work/amd_fidelityfx_loader_dx12.dll"

bash tools/fsr4cap/capture_all.sh "$work"
python3 tools/fsr4cap/extract.py "$dx/build/dxil-spirv" "$work" fsr4_411
if [[ ${VERIFY:-0} == 1 ]]; then
    bash build.sh
    ninja -C out/gpu fsr4-bench >/dev/null
    bash tools/fsr4cap/verify.sh "$work"
fi
echo "FSR 4.1.1 assets in $PWD/fsr4_411 (gow3.ini: upscaler=fsr411)"
