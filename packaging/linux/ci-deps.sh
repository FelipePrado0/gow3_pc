#!/usr/bin/env bash
# Build dependencies of the release AppImage on Ubuntu 24.04 (.github/workflows/linux-appimage.yml).
# Its glibc (2.39) is the oldest one players can have: SteamOS, Bazzite/Fedora, Debian 13 and
# Ubuntu 24.04 or newer. A build on a rolling distro needs the host's glibc instead (#291).
# What 24.04 lacks or ships too old is built into /usr/local: Vulkan-Headers, VMA, SDL3.
set -euo pipefail
sudo=$([[ $(id -u) == 0 ]] || echo sudo)
export DEBIAN_FRONTEND=noninteractive
$sudo apt-get update
$sudo apt-get install -y --no-install-recommends \
    build-essential g++-14 gcc-14 cmake ninja-build pkg-config git curl ca-certificates file patchelf binutils \
    libvulkan-dev glslang-tools libboost-dev libfmt-dev libxxhash-dev libzydis-dev robin-map-dev \
    libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev \
    libx11-dev libxext-dev libxrandr-dev libxcursor-dev libxi-dev libxss-dev libxtst-dev libxfixes-dev \
    libxinerama-dev libxkbcommon-dev libwayland-dev wayland-protocols libdecor-0-dev libdrm-dev libgbm-dev \
    libegl-dev libasound2-dev libpulse-dev libpipewire-0.3-dev libudev-dev libdbus-1-dev libibus-1.0-dev \
    python3
$sudo update-alternatives --install /usr/bin/gcc gcc /usr/bin/gcc-14 140 --slave /usr/bin/g++ g++ /usr/bin/g++-14
$sudo update-alternatives --install /usr/bin/cc cc /usr/bin/gcc-14 140
$sudo update-alternatives --install /usr/bin/c++ c++ /usr/bin/g++-14 140

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# Vulkan-Headers (and Vulkan-Hpp): 24.04 has 1.3.275, the renderer needs 1.4 types.
VULKAN_HEADERS=${VULKAN_HEADERS:-vulkan-sdk-1.4.357.0}
git clone -q --depth 1 -b "$VULKAN_HEADERS" https://github.com/KhronosGroup/Vulkan-Headers "$work/vulkan-headers"
cmake -S "$work/vulkan-headers" -B "$work/vulkan-headers/build" -G Ninja -DCMAKE_INSTALL_PREFIX=/usr/local >/dev/null
$sudo cmake --install "$work/vulkan-headers/build" >/dev/null
# Vulkan Memory Allocator: header only, not packaged in 24.04.
VMA=${VMA:-v3.1.0}
$sudo curl -fsSL -o /usr/local/include/vk_mem_alloc.h \
    "https://raw.githubusercontent.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator/$VMA/include/vk_mem_alloc.h"
# SDL3: not packaged in 24.04.
SDL=${SDL:-3.4.16}
curl -fsSL "https://github.com/libsdl-org/SDL/releases/download/release-$SDL/SDL3-$SDL.tar.gz" | tar xz -C "$work"
cmake -S "$work/SDL3-$SDL" -B "$work/sdl" -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr/local \
    -DSDL_TESTS=OFF -DSDL_EXAMPLES=OFF >/dev/null
ninja -C "$work/sdl" >/dev/null
$sudo ninja -C "$work/sdl" install >/dev/null
$sudo ldconfig
