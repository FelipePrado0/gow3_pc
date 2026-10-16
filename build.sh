#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "$0")"
mkdir -p out
# Windows: MSYS2 CLANG64 (packages in packaging/windows/README.md). The GPU library is static
# there and CMake links gow3-probe.exe as well; no PGO (GCC-only flags), LTO off by default.
# macOS: x86-64 under Rosetta 2, see packaging/macos/README.md.
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
Darwin)
    # macOS: the game's x86-64 code runs inside the loader's process, so everything is built for
    # x86-64 (Rosetta 2 on Apple Silicon) against the libraries packaging/macos/deps.sh builds
    # once into out/macos-deps. Vulkan: the Khronos loader and Mesa's KosmicKrisp driver (copied
    # to out/vulkan, where the loader process finds it); no PGO, LTO on by default.
    bash packaging/macos/deps.sh
    # shellcheck source=packaging/macos/env.sh
    source packaging/macos/env.sh
    if [[ ! -f gpu/third_party/fsr-vulkan/CMakeLists.txt || ! -f gpu/third_party/imgui/imgui.h ||
          ! -f gpu/third_party/xbyak/xbyak/xbyak.h ]]; then
        git submodule update --init --recursive
    fi
    for patch in gpu/patches/fsr-vulkan/*.patch; do
        if ! git -C gpu/third_party/fsr-vulkan apply --reverse --check "$PWD/$patch" 2>/dev/null; then
            git -C gpu/third_party/fsr-vulkan apply "$PWD/$patch"
        fi
    done
    cmake -S gpu -B out/gpu -G Ninja "${gow3_macos_cmake[@]}" -DCMAKE_BUILD_TYPE=RelWithDebInfo -DGOW3_PGO=off \
        -DGOW3_LTO="${GOW3_LTO:-ON}" >/dev/null
    echo "GPU library: LTO ${GOW3_LTO:-ON}"
    if ! ninja -C out/gpu gow3gpu > out/gpu-build.log 2>&1; then
        grep -v '^\[' out/gpu-build.log | tail -40 >&2
        echo 'GPU library build failed (full log: out/gpu-build.log)' >&2; exit 1
    fi
    cc=("${gow3_macos_cc[@]}")
    read -r -a includes <<< "$(pkg-config --cflags sdl3 libavcodec libavutil libswscale) -I$gow3_macos_deps/include"
    # The libraries are found through run paths: packaged copies keep the GPU library in gpu/ and
    # the others in lib/ next to the binary; a build uses out/gpu and out/macos-deps/lib.
    rpaths=(-Wl,-rpath,@executable_path/gpu -Wl,-rpath,@executable_path/lib -Wl,-rpath,"$PWD/out/gpu"
            -Wl,-rpath,"$gow3_macos_deps/lib")
    libraries=()
    for flag in $(pkg-config --libs sdl3 libavcodec libavutil libswscale); do
        [[ $flag == -Wl,-rpath,* ]] || libraries+=("$flag") # sdl3.pc adds its own run path
    done
    libraries+=("${rpaths[@]}")
    vulkan=("$gow3_macos_deps/lib/libvulkan.1.dylib")
    rm -rf out/vulkan && cp -R "$gow3_macos_deps/lib/vulkan" out/vulkan
    gpu=(-Lout/gpu -lgow3gpu)
    # mimalloc becomes the process's malloc: every heap object stays below 1 TiB (runtime_memory.c).
    runtime=(src/runtime*.c src/compat_mac.c)
    allocator=("$gow3_macos_deps/lib/gow3-mimalloc.o")
    if [[ ! -f out/libatrac9.a || -n $(find third_party/LibAtrac9/C/src -newer out/libatrac9.a -name '*.c') ]]; then
        rm -rf out/atrac9 && mkdir -p out/atrac9
        for source in third_party/LibAtrac9/C/src/*.c; do
            "${cc[@]}" -std=c99 -O2 -g -w -c "$source" -o "out/atrac9/$(basename "${source%.c}").o"
        done
        rm -f out/libatrac9.a && ar rcs out/libatrac9.a out/atrac9/*.o
    fi
    # isa_emulate runs inside guest code: no XMM/YMM.
    "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -mgeneral-regs-only -c src/isa_emulate.c -o out/isa_emulate.o
    "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc src/probe.c "${runtime[@]}" \
        src/cpu_check.c src/vulkan_smoke.c out/isa_emulate.o src/isa_rewrite.c out/libatrac9.a "${allocator[@]}" \
        -lm "${gpu[@]}" "${libraries[@]}" "${vulkan[@]}" -o out/gow3-probe
    echo "Built $PWD/out/gow3-probe"
    "${cc[@]}" -std=c11 -O2 -Wall -Wextra -Werror -I. -Igpu/shadps4 -I"$gow3_macos_deps/include" tools/gpu_capabilities.c \
        src/compat_mac.c "${vulkan[@]}" "${rpaths[@]}" -o out/gow3-gpu-capabilities
    if [[ ${1:-} == --test ]]; then
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_pad.c src/compat_mac.c "${libraries[@]}" -o out/pad-test
        out/pad-test
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_runtime.c "${runtime[@]}" out/libatrac9.a "${allocator[@]}" -lm "${gpu[@]}" "${libraries[@]}" -o out/runtime-test
        out/runtime-test
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread -Isrc tests/test_file_mods.c src/compat_mac.c -o out/file-mods-test
        out/file-mods-test
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_sema.c "${runtime[@]}" out/libatrac9.a "${allocator[@]}" -lm "${gpu[@]}" "${libraries[@]}" -o out/sema-test
        out/sema-test
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -I. -Isrc tests/test_content.c src/runtime_content.c src/compat_mac.c -o out/content-test
        out/content-test
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_mp3.c src/runtime_mp3.c src/compat_mac.c out/libatrac9.a -lm "${libraries[@]}" -o out/mp3-test
        out/mp3-test tests/data/sine.mp3
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_videodec.c src/compat_mac.c "${libraries[@]}" -o out/videodec-test
        out/videodec-test tests/data/tiny.h264
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread -I. -Isrc tests/test_services.c src/compat_mac.c -o out/services-test
        out/services-test
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -mbmi -mlzcnt -mmovbe -I. -Isrc tests/test_isa_emulate.c out/isa_emulate.o -o out/isa-emulate-test
        # Rosetta reports BMI1, LZCNT and MOVBE to processes started with ROSETTA_ADVERTISE_AVX=1 (probe.c).
        ROSETTA_ADVERTISE_AVX=1 out/isa-emulate-test
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -c src/isa_rewrite.c -I. -Isrc -o out/isa_rewrite.o
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -mbmi -mlzcnt -I. -Isrc tests/test_isa_stub.c out/isa_rewrite.o out/isa_emulate.o -o out/isa-stub-test
        ROSETTA_ADVERTISE_AVX=1 out/isa-stub-test tests/data/isa_fixture.bin
        # Window, Vulkan device and presenter without the game (needs the desktop session).
        "${cc[@]}" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_gpu_start.c "${runtime[@]}" out/libatrac9.a "${allocator[@]}" -lm "${gpu[@]}" "${libraries[@]}" "${vulkan[@]}" -o out/gpu-start-test
        mkdir -p out/gpu-start-test-user
        GOW3_GPU_USER_DIR="$PWD/out/gpu-start-test-user" out/gpu-start-test 2
    fi
    exit 0
    ;;
esac
if [[ -z ${CC:-} ]]; then
    CC=$(command -v cc || command -v gcc || true)
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
    echo 'Need pkg-config with vulkan and sdl3, cmake and ninja (see shell.nix).' >&2; exit 1
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
cmake -S gpu -B out/gpu -G Ninja -DCMAKE_BUILD_TYPE=RelWithDebInfo -DGOW3_PGO="$pgo" \
    -DGOW3_LTO="${GOW3_LTO:-ON}" -DGOW3_PGO_DIR="$PWD/pgo" >/dev/null
echo "GPU library: PGO $pgo, LTO ${GOW3_LTO:-ON}"
# A failed GPU build must stop here: an older libgow3gpu.so would otherwise be used silently.
if ! ninja -C out/gpu gow3gpu > out/gpu-build.log 2>&1; then
    grep -v '^\[' out/gpu-build.log | tail -40 >&2
    echo 'GPU library build failed (full log: out/gpu-build.log)' >&2; exit 1
fi
# $ORIGIN/gpu: packaged copies keep the library next to the binary without patching it.
gpu=(-Lout/gpu -lgow3gpu -Wl,-rpath,'$ORIGIN/gpu' -Wl,-rpath,"$PWD/out/gpu" -rdynamic)
runtime=(src/runtime*.c)
# Third-party decoders: compiled once, without this project's -Werror policy.
atrac9=(third_party/LibAtrac9/C/src/*.c)
if [[ ! -f out/libatrac9.a || -n $(find third_party/LibAtrac9/C/src -newer out/libatrac9.a -name '*.c') ]]; then
    rm -rf out/atrac9 && mkdir -p out/atrac9
    for source in "${atrac9[@]}"; do "$CC" -std=c99 -O2 -g -w -c "$source" -o "out/atrac9/$(basename "${source%.c}").o"; done
    ar rcs out/libatrac9.a out/atrac9/*.o
fi
# isa_emulate runs inside guest code: no XMM/YMM.
"$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -mgeneral-regs-only -c src/isa_emulate.c -o out/isa_emulate.o
"$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread -no-pie "${includes[@]}" -I. -Isrc src/probe.c "${runtime[@]}" src/cpu_check.c src/vulkan_smoke.c out/isa_emulate.o src/isa_rewrite.c out/libatrac9.a -lm "${gpu[@]}" "${libraries[@]}" -o out/gow3-probe
echo "Built $PWD/out/gow3-probe"
# GPU check for run.sh (live_resolution=auto): links only the Vulkan loader.
"$CC" -std=c11 -O2 -Wall -Wextra -Werror -Igpu/shadps4 tools/gpu_capabilities.c "${libraries[@]}" -o out/gow3-gpu-capabilities
if [[ ${1:-} == --test ]]; then
    "$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_pad.c "${libraries[@]}" -o out/pad-test
    out/pad-test
    "$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread -I. -Isrc tests/test_runtime.c "${runtime[@]}" out/libatrac9.a -lm "${gpu[@]}" "${libraries[@]}" -o out/runtime-test
    out/runtime-test
    "$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread -Isrc tests/test_file_mods.c -o out/file-mods-test
    out/file-mods-test
    "$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread -I. -Isrc tests/test_sema.c "${runtime[@]}" out/libatrac9.a -lm "${gpu[@]}" "${libraries[@]}" -o out/sema-test
    out/sema-test
    "$CC" -std=c11 -D_GNU_SOURCE -O2 -g -Wall -Wextra -Werror -I. -Isrc tests/test_content.c src/runtime_content.c -o out/content-test
    out/content-test
    "$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_mp3.c src/runtime_mp3.c out/libatrac9.a -lm "${libraries[@]}" -o out/mp3-test
    out/mp3-test tests/data/sine.mp3
    "$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread "${includes[@]}" -I. -Isrc tests/test_videodec.c "${libraries[@]}" -o out/videodec-test
    out/videodec-test tests/data/tiny.h264
    "$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -pthread -I. -Isrc tests/test_services.c -o out/services-test
    out/services-test
    "$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -mbmi -mlzcnt -mmovbe -I. -Isrc tests/test_isa_emulate.c out/isa_emulate.o -o out/isa-emulate-test
    out/isa-emulate-test
    "$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -c src/isa_rewrite.c -I. -Isrc -o out/isa_rewrite.o
    "$CC" -std=c11 -O2 -g -Wall -Wextra -Werror -mbmi -mlzcnt -I. -Isrc tests/test_isa_stub.c out/isa_rewrite.o out/isa_emulate.o -o out/isa-stub-test
    out/isa-stub-test tests/data/isa_fixture.bin
fi
