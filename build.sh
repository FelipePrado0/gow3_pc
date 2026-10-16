#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")"
mkdir -p out
# Windows: MSYS2 CLANG64 (packages in packaging/windows/README.md). The GPU library is static
# there and CMake links gow3-probe.exe as well; no PGO (GCC-only flags), LTO off by default.
case $(uname -s) in
MINGW*|MSYS*)
    # shellcheck source=msys2-env.sh
    source ./msys2-env.sh
    for tool in clang cmake ninja pkg-config; do
        command -v "$tool" >/dev/null || { echo "Need $tool from MSYS2 CLANG64 (packaging/windows/README.md)." >&2; exit 1; }
    done
    if [[ ! -f gpu/third_party/fsr-vulkan/CMakeLists.txt || ! -f gpu/third_party/imgui/imgui.h ||
          ! -f gpu/third_party/xbyak/xbyak/xbyak.h ]]; then
        git submodule update --init --recursive
    fi
    for patch in gpu/patches/fsr-vulkan/*.patch; do
        if ! git -C gpu/third_party/fsr-vulkan apply --reverse --check "$PWD/$patch" 2>/dev/null; then
            git -C gpu/third_party/fsr-vulkan apply "$PWD/$patch"
        fi
    done
    cmake -S gpu -B out/gpu -G Ninja -DCMAKE_BUILD_TYPE=RelWithDebInfo -DGOW3_PGO=off \
        -DGOW3_LTO="${GOW3_LTO:-OFF}" -DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER=clang++ >/dev/null
    echo "GPU library and loader: LTO ${GOW3_LTO:-OFF}"
    if ! ninja -C out/gpu gow3-probe gow3-gpu-capabilities gow3-play > out/gpu-build.log 2>&1; then
        grep -v '^\[' out/gpu-build.log | tail -40 >&2
        echo 'Build failed (full log: out/gpu-build.log)' >&2; exit 1
    fi
    cp out/gpu/gow3-probe.exe out/gpu/gow3-gpu-capabilities.exe out/gpu/gow3-play.exe out/
    echo "Built $PWD/out/gow3-probe.exe"
    exit 0
    ;;
esac
if [[ ${1:-} == --setup || ${1:-} == --setup-apple-silicon ]]; then
    shift
    exec bash packaging/macos/setup-apple-silicon.sh "$@"
fi
if [[ ${1:-} == --download || ${1:-} == --download-prebuilt ]]; then
    shift
    exec bash packaging/macos/download-prebuilt.sh "$@"
fi
is_darwin=0
if [[ $(uname -s) == Darwin ]]; then
    is_darwin=1
    # On Apple Silicon (arm64), the PS4 loader runs as x86_64 under Rosetta 2.
    # If /usr/local has x86_64 Homebrew, re-exec seamlessly under arch -x86_64.
    if [[ $(uname -m) == arm64 ]]; then
        if arch -x86_64 /usr/bin/true 2>/dev/null; then
            if [[ -x /usr/local/bin/brew || -f /usr/local/lib/libSDL3.dylib ]]; then
                exec arch -x86_64 /bin/bash "$0" "$@"
            fi
        fi
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
fi
if [[ -z ${CC:-} ]]; then
    CC=$(command -v cc || command -v clang || command -v gcc || true)
    if [[ -z $CC ]]; then
        for candidate in /nix/store/*-gcc-wrapper-*/bin/gcc; do
            if [[ -x $candidate ]]; then CC=$candidate; break; fi
        done
    fi
fi
if [[ -z ${CC:-} ]]; then echo 'Install GCC/Clang or set CC.' >&2; exit 1; fi
# Dependencies come from pkg-config (Vulkan loader/headers, SDL3). On NixOS the
# environment is provided by shell.nix; re-enter it automatically if needed.
if ! { command -v pkg-config >/dev/null && pkg-config --exists vulkan sdl3 && command -v cmake >/dev/null && command -v ninja >/dev/null; }; then
    if [[ -z ${GOW3_IN_NIX_SHELL:-} ]] && command -v nix-shell >/dev/null; then
        exec env GOW3_IN_NIX_SHELL=1 nix-shell shell.nix --run "bash build.sh $*"
    fi
    if [[ $is_darwin -eq 1 ]]; then
        echo 'Missing required build dependencies:' >&2
        command -v pkg-config >/dev/null || echo '  - pkg-config' >&2
        command -v cmake >/dev/null || echo '  - cmake' >&2
        command -v ninja >/dev/null || echo '  - ninja' >&2
        if command -v pkg-config >/dev/null; then
            pkg-config --exists vulkan || echo '  - vulkan (provided by Homebrew package: vulkan-loader)' >&2
            pkg-config --exists sdl3 || echo '  - sdl3' >&2
        fi
        echo '' >&2
        echo 'To set up build dependencies on Apple Silicon (M1/M2/M3/M4):' >&2
        echo '  Run: bash build.sh --setup' >&2
        echo '' >&2
        echo 'Or install manually with Homebrew:' >&2
        echo '  • Intel Mac:' >&2
        echo '      brew install pkg-config cmake ninja molten-vk vulkan-headers vulkan-loader sdl3 ffmpeg boost fmt robin-map xxhash zydis glslang' >&2
        echo '  • Apple Silicon (M1/M2/M3/M4 via Rosetta 2 x86_64):' >&2
        echo '      arch -x86_64 /usr/local/bin/brew install pkg-config cmake ninja molten-vk vulkan-headers vulkan-loader sdl3 ffmpeg boost fmt robin-map xxhash zydis glslang' >&2
        exit 1
    fi
    echo 'Need pkg-config with vulkan and sdl3, cmake and ninja (see shell.nix).' >&2; exit 1
fi
if [[ $is_darwin -eq 1 ]]; then
    sdl_lib=$(pkg-config --variable=libdir sdl3 2>/dev/null || true)
    if [[ -n $sdl_lib && -f "$sdl_lib/libSDL3.dylib" ]]; then
        if file "$sdl_lib/libSDL3.dylib" | grep -q 'arm64' && ! file "$sdl_lib/libSDL3.dylib" | grep -q 'x86_64'; then
            if [[ -f out/gow3-probe || -f bin/gow3-probe ]]; then
                echo "Notice: Found arm64 libraries in $sdl_lib. Keeping existing x86_64 gow3-probe."
            else
                echo "Apple Silicon (ARM64) environment detected." >&2
                echo "The PS4 guest code and loader (gow3-probe) must run as x86_64 under Rosetta 2." >&2
                echo "" >&2
                echo "Options to run or build on Apple Silicon:" >&2
                echo "  1. Automatically set up x86_64 build dependencies and build from source:" >&2
                echo "       bash build.sh --setup" >&2
                echo "" >&2
                echo "  2. Download prebuilt release binaries:" >&2
                echo "       bash build.sh --download" >&2
                echo "     Then run:" >&2
                echo "       bash run.sh" >&2
                exit 1
            fi
        fi
    fi
fi
read -r -a includes <<< "$(pkg-config --cflags vulkan sdl3 libavcodec libavutil libswscale)"
read -r -a libraries <<< "$(pkg-config --libs vulkan sdl3 libavcodec libavutil libswscale)"
# GPU library (shadPS4 video core + drivers), built by CMake into out/gpu/libgow3gpu.so.
# GOW3_PGO: generate (instrumented build that writes pgo/ while the game runs), use, off.
# Default: use the profile in pgo/ when there is one. GOW3_LTO=OFF disables link-time optimization.
pgo=${GOW3_PGO:-}
if [[ -z $pgo ]]; then
    if [[ -n $(find pgo -name '*.gcda' -print -quit 2>/dev/null) ]]; then pgo=use; else pgo=off; fi
fi
mkdir -p pgo
# Submodules (git clone --recursive, or: git submodule update --init) and this port's changes
# to FSR-Vulkan (gpu/patches/fsr-vulkan), applied to its working tree once.
if [[ ! -f gpu/third_party/fsr-vulkan/CMakeLists.txt || ! -f gpu/third_party/imgui/imgui.h ]]; then
    git submodule update --init --recursive
fi
for patch in gpu/patches/fsr-vulkan/*.patch; do
    if ! git -C gpu/third_party/fsr-vulkan apply --reverse --check "$PWD/$patch" 2>/dev/null; then
        git -C gpu/third_party/fsr-vulkan apply "$PWD/$patch"
    fi
done
cmake_args=(-S gpu -B out/gpu -G Ninja -DCMAKE_BUILD_TYPE=RelWithDebInfo -DGOW3_PGO="$pgo"
    -DGOW3_LTO="${GOW3_LTO:-ON}" -DGOW3_PGO_DIR="$PWD/pgo")
if [[ $is_darwin -eq 1 ]]; then
    cmake_args+=(-DCMAKE_OSX_ARCHITECTURES="x86_64")
fi
cmake "${cmake_args[@]}" >/dev/null
echo "GPU library: PGO $pgo, LTO ${GOW3_LTO:-ON}"
# A failed GPU build must stop here: an older libgow3gpu.so would otherwise be used silently.
if ! ninja -C out/gpu gow3gpu > out/gpu-build.log 2>&1; then
    grep -v '^\[' out/gpu-build.log | tail -40 >&2
    echo 'GPU library build failed (full log: out/gpu-build.log)' >&2; exit 1
fi
# $ORIGIN/gpu on Linux, @loader_path/gpu and @executable_path/gpu on Darwin
if [[ $is_darwin -eq 1 ]]; then
    if [[ -d bin/lib ]]; then
        for f in bin/lib/lib*.*.dylib; do
            base=$(basename "$f")
            name=$(echo "$base" | sed -E 's/\.[0-9]+(\.[0-9]+)*\.dylib$/.dylib/')
            if [[ "$name" != "$base" && ! -e "bin/lib/$name" ]]; then
                ln -s "$base" "bin/lib/$name" 2>/dev/null || true
            fi
        done
        libraries=(-L"$PWD/bin/lib" "${libraries[@]}")
    fi
    gpu=(-Lout/gpu -lgow3gpu -Wl,-rpath,'@loader_path/gpu' -Wl,-rpath,'@executable_path/gpu' -Wl,-rpath,'@loader_path/lib' -Wl,-rpath,'@executable_path/lib' -Wl,-rpath,"$PWD/out/gpu" -Wl,-export_dynamic -framework Metal -framework Foundation -framework QuartzCore)
    probe_link_flags=(-Wl,-pie -Wl,-rpath,'@loader_path/lib' -Wl,-rpath,'@executable_path/lib')
    arch_flags=(-arch x86_64)
else
    gpu=(-Lout/gpu -lgow3gpu -Wl,-rpath,'$ORIGIN/gpu' -Wl,-rpath,"$PWD/out/gpu" -rdynamic)
    probe_link_flags=(-no-pie)
    arch_flags=(-march=x86-64)
fi
runtime=(src/runtime*.c)
# Third-party decoders: compiled once, without this project's -Werror policy.
atrac9=(third_party/LibAtrac9/C/src/*.c)
if [[ ! -f out/libatrac9.a || -n $(find third_party/LibAtrac9/C/src -newer out/libatrac9.a -name '*.c') ]]; then
    rm -rf out/atrac9 && mkdir -p out/atrac9
    for source in "${atrac9[@]}"; do "$CC" "${arch_flags[@]}" -std=c99 -O2 -g -w -c "$source" -o "out/atrac9/$(basename "${source%.c}").o"; done
    ar rcs out/libatrac9.a out/atrac9/*.o
fi
# isa_emulate runs inside guest code: no XMM/YMM.
"$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -mgeneral-regs-only -c src/isa_emulate.c -o out/isa_emulate.o
"$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${probe_link_flags[@]}" "${includes[@]}" -I. -Isrc src/probe.c "${runtime[@]}" src/cpu_check.c src/vulkan_smoke.c out/isa_emulate.o src/isa_rewrite.c out/libatrac9.a -lm "${gpu[@]}" "${libraries[@]}" -o out/gow3-probe
echo "Built $PWD/out/gow3-probe"
# GPU check for run.sh (live_resolution=auto): links only the Vulkan loader.
"$CC" "${arch_flags[@]}" -std=c11 -O2 -Wall -Wextra -Werror "${probe_link_flags[@]}" "${includes[@]}" -Igpu/shadps4 tools/gpu_capabilities.c "${libraries[@]}" -o out/gow3-gpu-capabilities
if [[ $is_darwin -eq 1 && -d bin ]]; then
    cp out/gow3-probe out/gow3-gpu-capabilities bin/ 2>/dev/null || true
    if [[ -f out/gpu/libgow3gpu.dylib && -d bin/gpu ]]; then
        cp out/gpu/libgow3gpu.dylib bin/gpu/ 2>/dev/null || true
    fi
fi
if [[ ${1:-} == --test ]]; then
    "$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_pad.c "${libraries[@]}" -o out/pad-test
    out/pad-test
    "$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread -I. -Isrc tests/test_runtime.c "${runtime[@]}" out/libatrac9.a -lm "${gpu[@]}" "${libraries[@]}" -o out/runtime-test
    out/runtime-test
    "$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread -Isrc tests/test_file_mods.c -o out/file-mods-test
    out/file-mods-test
    "$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread -I. -Isrc tests/test_sema.c "${runtime[@]}" out/libatrac9.a -lm "${gpu[@]}" "${libraries[@]}" -o out/sema-test
    out/sema-test
    "$CC" "${arch_flags[@]}" -std=c11 -D_GNU_SOURCE -O2 -g -Wall -Wextra -Werror -I. -Isrc tests/test_content.c src/runtime_content.c -o out/content-test
    out/content-test
    "$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_mp3.c src/runtime_mp3.c out/libatrac9.a -lm "${libraries[@]}" -o out/mp3-test
    out/mp3-test tests/data/sine.mp3
    "$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_videodec.c "${libraries[@]}" -o out/videodec-test
    out/videodec-test tests/data/tiny.h264
    "$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread -I. -Isrc tests/test_services.c -o out/services-test
    out/services-test
    "$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -mbmi -mlzcnt -mmovbe -I. -Isrc tests/test_isa_emulate.c out/isa_emulate.o -o out/isa-emulate-test
    out/isa-emulate-test
    "$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -c src/isa_rewrite.c -I. -Isrc -o out/isa_rewrite.o
    "$CC" "${arch_flags[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -mbmi -mlzcnt -I. -Isrc tests/test_isa_stub.c out/isa_rewrite.o out/isa_emulate.o -o out/isa-stub-test
    out/isa-stub-test tests/data/isa_fixture.bin
fi
