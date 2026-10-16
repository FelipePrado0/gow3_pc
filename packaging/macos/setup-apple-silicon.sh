#!/usr/bin/env bash
# Sets up x86_64 Homebrew and dependencies on Apple Silicon (M1/M2/M3/M4)
# to build God of War III x86_64 PS4 runtime under Rosetta 2.
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."

echo "============================================================"
echo " God of War III: Apple Silicon (ARM64) Build Setup"
echo "============================================================"

# 1. Verify / Install Rosetta 2
if ! arch -x86_64 /usr/bin/true 2>/dev/null; then
    echo "==> Installing Rosetta 2..."
    softwareupdate --install-rosetta --agree-to-license
else
    echo "==> Rosetta 2 is active."
fi

# 2. Verify / Install x86_64 Homebrew in /usr/local
if [[ ! -x /usr/local/bin/brew ]]; then
    echo "==> Setting up x86_64 Homebrew into /usr/local..."
    echo "    (Apple Silicon macOS installs x86_64 Homebrew to /usr/local without affecting /opt/homebrew)"
    sudo mkdir -p /usr/local/Homebrew /usr/local/bin /usr/local/lib /usr/local/etc /usr/local/var /usr/local/share 2>/dev/null || true
    sudo chown -R "$(whoami)" /usr/local/Homebrew /usr/local/bin /usr/local/lib /usr/local/etc /usr/local/var /usr/local/share 2>/dev/null || true
    if [[ ! -d /usr/local/Homebrew/.git ]]; then
        git clone https://github.com/Homebrew/brew /usr/local/Homebrew
    fi
    ln -sf /usr/local/Homebrew/bin/brew /usr/local/bin/brew
else
    echo "==> x86_64 Homebrew found at /usr/local/bin/brew."
fi

# 3. Install required x86_64 build dependencies
echo "==> Installing required x86_64 dependencies via /usr/local/bin/brew..."
arch -x86_64 /usr/local/bin/brew install pkg-config cmake ninja molten-vk vulkan-headers vulkan-loader \
                                        sdl3 ffmpeg boost fmt robin-map xxhash zydis glslang

echo ""
echo "============================================================"
echo " Setup complete! Building God of War III for macOS..."
echo "============================================================"
arch -x86_64 bash build.sh "$@"
