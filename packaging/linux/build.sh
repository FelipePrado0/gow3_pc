#!/usr/bin/env bash
# Builds dist/spartanport-x86_64.AppImage from the Linux build (run `bash build.sh` in the repo root first).
# Libraries are bundled with their dependencies, except the ones that must come from the host
# (glibc, Vulkan loader and drivers, X11/Wayland, GL). The launcher is the Rust one
# (launcher-rust/): a single executable, so players need no Python. Needs: ldd, patchelf, curl
# (appimagetool) and a Rust toolchain.
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."
[[ -f out/spartanport-probe && -f out/gpu/libspartanportgpu.so ]] || { echo 'Build first: bash build.sh' >&2; exit 1; }

appdir=out/AppDir
rm -rf "$appdir"
share=$appdir/usr/share/spartanport
mkdir -p "$share/bin/gpu" "$appdir/usr/lib"

install -m755 out/spartanport-probe out/spartanport-gpu-capabilities "$share/bin/"
install -m755 out/gpu/libspartanportgpu.so "$share/bin/gpu/"
(cd launcher-rust && "${CARGO:-cargo}" build --release --locked)
install -m755 launcher-rust/target/release/spartanport-launcher "$share/spartanport-launcher"
cp -r patches "$share/"
mkdir -p "$share/scripts"
cp -r scripts/controller_diagrams "$share/scripts/"
mkdir -p "$share/launcher"
# The icon of the AppImage, the launcher and the game window: packaging/linux/spartanport.png when it
# exists (a local choice), else the launcher's own.
icon=launcher/spartanport.png
[[ -f packaging/linux/spartanport.png ]] && icon=packaging/linux/spartanport.png
install -m644 "$icon" "$share/launcher/spartanport.png"
install -m644 launcher/logo.png "$share/launcher/logo.png"
install -m644 launcher/Cinzel-Black.ttf "$share/launcher/Cinzel-Black.ttf"
install -m644 launcher/OFL-Cinzel.txt "$share/launcher/OFL-Cinzel.txt"
install -m644 launcher/BodoniModa.ttf "$share/launcher/BodoniModa.ttf"
install -m644 launcher/OFL-BodoniModa.txt "$share/launcher/OFL-BodoniModa.txt"

# The package holds no Python and no frozen interpreter.
stray=$(find "$appdir" \( -name '*.py' -o -name _internal \) -print)
[[ -z $stray ]] || { echo 'Python files in the AppDir:' >&2; echo "$stray" >&2; exit 1; }
install -m755 packaging/linux/AppRun "$appdir/AppRun"
install -m644 packaging/linux/spartanport.desktop "$appdir/spartanport.desktop"
install -m644 "$icon" "$appdir/spartanport.png"
if [[ -f release.json ]]; then install -m644 release.json "$appdir/release.json"; fi

# Host-provided libraries: bundling any of these breaks drivers or the display connection.
excluded='^(ld-linux.*|libc|libm|libdl|librt|libpthread|libresolv|libutil|libnsl|libanl|libvulkan|libGL|libGLX|libGLdispatch|libEGL|libOpenGL|libgbm|libdrm.*|libX11|libX11-xcb|libxcb.*|libXau|libXdmcp|libXext|libXfixes|libXrender|libXi|libXcursor|libXrandr|libwayland-.*|libasound|libudev|libpulse.*|libfontconfig|libfreetype|libOpenCL|libva.*|libvdpau|libvpl|libnvidia-.*|libcuda|libz|libstdc\+\+|libgcc_s)\.so'
declare -A seen
bundle() {
    local elf=$1 line lib name
    while read -r line; do
        lib=$(awk '/=> \//{print $3}' <<<"$line")
        [[ -n $lib ]] || continue
        name=$(basename -- "$lib")
        [[ ${seen[$name]:-} ]] && continue
        seen[$name]=1
        [[ $name =~ $excluded ]] && continue
        install -m755 "$lib" "$appdir/usr/lib/$name"
        bundle "$lib"
    done < <(ldd "$elf")
}
bundle out/gpu/libspartanportgpu.so
bundle out/spartanport-probe
bundle launcher-rust/target/release/spartanport-launcher
# libstdc++/libgcc/libz come from the build host: the same glibc generation as the one that runs
# the AppImage is assumed, and a newer libstdc++ than Mesa's is the safe direction.
for name in libstdc++.so.6 libgcc_s.so.1; do
    install -m755 "$(ldd out/gpu/libspartanportgpu.so | awk -v n="$name" '$1==n{print $3}')" "$appdir/usr/lib/$name"
done
# $ORIGIN/gpu is already the probe's RUNPATH; the libraries it needs sit in usr/lib via AppRun.
find "$appdir/usr/lib" -name '*.so*' -exec strip --strip-unneeded {} + 2>/dev/null || true

# The newest glibc symbol in the bundle is the oldest glibc players can have. Built on a rolling
# distro it follows the host's and the AppImage stops starting on SteamOS, Debian and Fedora (#291):
# release builds come from .github/workflows/linux-appimage.yml (Ubuntu 24.04). SPARTAN_MAX_GLIBC
# raises the limit for local builds.
max_glibc=${SPARTAN_MAX_GLIBC:-2.39}
need=$(find "$appdir" -type f \( -name '*.so*' -o -perm -u+x \) -exec objdump -T {} + 2>/dev/null |
       grep -o 'GLIBC_2\.[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1 || true)  # objdump fails on scripts
if [[ $(printf '%s\n' "$need" "$max_glibc" | sort -V | tail -1) != "$max_glibc" ]]; then
    echo "The bundle needs glibc $need, newer than $max_glibc: build on Ubuntu 24.04 (packaging/linux/ci-deps.sh) or set SPARTAN_MAX_GLIBC" >&2
    exit 1
fi
echo "Minimum glibc: $need"

tool=${APPIMAGETOOL:-$(command -v appimagetool || true)}
if [[ -z $tool ]]; then
    tool=out/appimagetool-x86_64.AppImage
    [[ -x $tool ]] || {
        curl -fL -o "$tool" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
        chmod +x "$tool"
    }
fi
mkdir -p dist
# Written aside and moved over: replacing a running AppImage in place fails ("Text file busy").
ARCH=x86_64 "$tool" --appimage-extract-and-run "$appdir" dist/spartanport-x86_64.AppImage.new
mv -f dist/spartanport-x86_64.AppImage.new dist/spartanport-x86_64.AppImage
ls -lh dist/spartanport-x86_64.AppImage
