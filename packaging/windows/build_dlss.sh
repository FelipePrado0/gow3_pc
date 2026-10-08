#!/usr/bin/env bash
# Builds gow3_dlss.dll (gpu/dlss_bridge, the only code that uses the NVIDIA DLSS SDK) with
# MSVC and puts it with NVIDIA's nvngx_dlss.dll next to out/gow3-probe.exe; package.sh ships both.
# Needs Visual Studio 2022 (Build Tools) and a checkout of https://github.com/NVIDIA/DLSS:
#   DLSS_SDK_ROOT=F:/sdk/DLSS bash packaging/windows/build_dlss.sh
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."
sdk=${DLSS_SDK_ROOT:?Set DLSS_SDK_ROOT to a checkout of github.com/NVIDIA/DLSS}
cmake=${CMAKE_EXE:-cmake}
# Only the Vulkan headers: MSYS2's C headers must not reach MSVC.
rm -rf out/vkinclude && mkdir -p out/vkinclude
cp -r /c/msys64/clang64/include/vulkan /c/msys64/clang64/include/vk_video out/vkinclude/
"$cmake" -S gpu/dlss_bridge -B out/dlss-bridge -G "Visual Studio 17 2022" -A x64 \
    -DDLSS_SDK_ROOT="$(cygpath -m "$sdk")" -DVULKAN_INCLUDE="$(cygpath -m "$PWD/out/vkinclude")"
"$cmake" --build out/dlss-bridge --config Release
cp out/dlss-bridge/Release/gow3_dlss.dll "$sdk/lib/Windows_x86_64/rel/nvngx_dlss.dll" out/
cp "$sdk/LICENSE.txt" out/NVIDIA-DLSS-LICENSE.txt
echo "DLSS bridge ready: out/gow3_dlss.dll, out/nvngx_dlss.dll"
