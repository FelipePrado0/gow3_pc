#!/usr/bin/env bash
# macOS: builds the libraries the port links, for x86-64, from pinned sources into
# out/macos-deps (GOW3_MACOS_DEPS overrides the location). The game's own code is x86-64 and runs
# inside the loader's process, so the whole process is x86-64 (Rosetta 2 on Apple Silicon) and
# every library it loads must be x86-64 too; Homebrew's Intel bottles are gone on current macOS.
# The build tools (nasm, glslang) are built for the host. Needs the Xcode command line tools,
# cmake, ninja, pkg-config, curl and python3 (glslang's build). Each package is built once
# (out/macos-deps/stamps); delete a stamp or the whole directory to rebuild.
set -euo pipefail
cd -- "$(dirname -- "$0")/../.."
root=$PWD
prefix=${GOW3_MACOS_DEPS:-$root/out/macos-deps}
host=$prefix/host
sources=$prefix/src
work=$prefix/build
stamps=$prefix/stamps
jobs=$(sysctl -n hw.ncpu)
export MACOSX_DEPLOYMENT_TARGET=15.0 # Rosetta 2 runs AVX code from macOS 15 on

for tool in cmake ninja pkg-config curl python3 clang; do
    command -v "$tool" >/dev/null || { echo "macOS deps: $tool is missing (brew install cmake ninja pkg-config)." >&2; exit 1; }
done
mkdir -p "$sources" "$work" "$stamps" "$host"
# The system's own tools first: other SDKs on PATH may ship look-alikes (a `diff` that reports
# missing files as equal breaks autoconf's config.status).
python_dir=$(dirname -- "$(python3 -c 'import sys; print(sys.executable)')")
export PATH="$host/bin:/usr/bin:/bin:/usr/sbin:/sbin:$(dirname -- "$(command -v cmake)"):$(dirname -- "$(command -v ninja)"):$(dirname -- "$(command -v pkg-config)"):$python_dir"
if ! clang -arch x86_64 -x c -o /dev/null - <<< 'int main(void){return 0;}' 2>/dev/null; then
    echo 'macOS deps: clang cannot link x86_64 programs (install the Xcode command line tools).' >&2; exit 1
fi
if [[ $(uname -m) == arm64 ]] && ! arch -x86_64 /usr/bin/true 2>/dev/null; then
    echo 'macOS deps: Rosetta 2 is needed (softwareupdate --install-rosetta --agree-to-license).' >&2; exit 1
fi

# name|file|url|sha256
packages=(
"nasm|nasm-2.16.03.tar.xz|https://www.nasm.us/pub/nasm/releasebuilds/2.16.03/nasm-2.16.03.tar.xz|1412a1c760bbd05db026b6c0d1657affd6631cd0a63cddb6f73cc6d4aa616148"
"glslang|glslang-16.6.0.tar.gz|https://github.com/KhronosGroup/glslang/archive/refs/tags/16.6.0.tar.gz|9c09b901149c729df745057dafa815278aaa101b84d2b6e14f16a42de52f97f2"
"boost|boost-1.90.0-b2-nodocs.tar.xz|https://github.com/boostorg/boost/releases/download/boost-1.90.0/boost-1.90.0-b2-nodocs.tar.xz|9e6bee9ab529fb2b0733049692d57d10a72202af085e553539a05b4204211a6f"
"fmt|fmt-12.2.0.tar.gz|https://github.com/fmtlib/fmt/archive/refs/tags/12.2.0.tar.gz|8b852bb5aa6e7d8564f9e81394055395dd1d1936d38dfd3a17792a02bebd7af0"
"robin-map|robin-map-1.4.1.tar.gz|https://github.com/Tessil/robin-map/archive/refs/tags/v1.4.1.tar.gz|0e3f53a377fdcdc5f9fed7a4c0d4f99e82bbb64175233bd13427fef9a771f4a1"
"vma|VulkanMemoryAllocator-3.3.0.tar.gz|https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator/archive/refs/tags/v3.3.0.tar.gz|c4f6bbe6b5a45c2eb610ca9d231158e313086d5b1a40c9922cb42b597419b14e"
"vulkan-headers|Vulkan-Headers-1.4.363.0.tar.gz|https://github.com/KhronosGroup/Vulkan-Headers/archive/refs/tags/vulkan-sdk-1.4.363.0.tar.gz|4a078be12bef21cfebc09d878b77a63cff9d68f899254a0b00d0e37ef73e7f7e"
"xxhash|xxHash-0.8.3.tar.gz|https://github.com/Cyan4973/xxHash/archive/refs/tags/v0.8.3.tar.gz|aae608dfe8213dfd05d909a57718ef82f30722c392344583d3f39050c7f29a80"
"zydis|zydis-4.1.1.tar.gz|https://github.com/zyantific/zydis/archive/refs/tags/v4.1.1.tar.gz|45c6d4d499a1cc80780f7834747c637509777c01dca1e98c5e7c0bfaccdb1514"
"zycore|zycore-c-0b2432ce.tar.gz|https://github.com/zyantific/zycore-c/archive/0b2432ced0884fd152b471d97ecf0258ff4d859f.tar.gz|698e4623e661db312eb9da327519f53f0a9dd7fbbfacf02159687bca62369467"
"sdl3|SDL3-3.4.18.tar.gz|https://github.com/libsdl-org/SDL/releases/download/release-3.4.18/SDL3-3.4.18.tar.gz|9c75cf16330322c217dedd2e0609f1124f1b54b8633e763467b4684d0f4334a3"
"ffmpeg|ffmpeg-8.0.2.tar.xz|https://ffmpeg.org/releases/ffmpeg-8.0.2.tar.xz|5d16962332603c427b3d0887fc12b9166d6ee2cb1108b1865dd2d5eb06a09505"
"vulkan-loader|Vulkan-Loader-1.4.363.0.tar.gz|https://github.com/KhronosGroup/Vulkan-Loader/archive/refs/tags/vulkan-sdk-1.4.363.0.tar.gz|941eff558fc74f248745fb7df367640cf89a8457f216dae498a74ae14b6ba6f7"
"kosmickrisp|shadps4-macos-sdl-0.19.0.zip|https://github.com/shadps4-emu/shadPS4/releases/download/v.0.19.0/shadps4-macos-sdl-0.19.0.zip|47676b4875343b69f49689c7668d71de79ecaaa17ffeab3415d4137800b7ee02"
"mimalloc|mimalloc-3.5.3.tar.gz|https://github.com/microsoft/mimalloc/archive/refs/tags/v3.5.3.tar.gz|3b4a15153a59905995f7070296ed604bb5ccc00cabb8b93446931aff77224d47"
)
field() { local entry; for entry in "${packages[@]}"; do [[ ${entry%%|*} == "$1" ]] && { cut -d'|' -f"$2" <<< "$entry"; return; }; done; }

# Downloads (once) and checks a package's archive; prints its path.
fetch() {
    local file url sum path
    file=$(field "$1" 2) url=$(field "$1" 3) sum=$(field "$1" 4)
    path=$sources/$file
    if [[ ! -f $path ]] || [[ $(shasum -a 256 "$path" | cut -d' ' -f1) != "$sum" ]]; then
        echo "macOS deps: downloading $file" >&2
        curl -fsSL --retry 3 -o "$path.part" "$url"
        mv "$path.part" "$path"
    fi
    if [[ $(shasum -a 256 "$path" | cut -d' ' -f1) != "$sum" ]]; then
        echo "macOS deps: checksum mismatch for $file" >&2; exit 1
    fi
    printf '%s\n' "$path"
}
# Unpacks a package into a fresh build directory; prints the source directory.
unpack() {
    local archive dir
    archive=$(fetch "$1")
    dir=$work/$1
    rm -rf "$dir" && mkdir -p "$dir"
    tar -xf "$archive" -C "$dir"
    find "$dir" -mindepth 1 -maxdepth 1 -type d | head -1
}
done_already() { [[ -f $stamps/$1-$(field "$1" 4) ]]; }
# A built package's sources and build tree are not needed again (the archive stays in src/).
mark_done() {
    rm -f "$stamps/$1"-*; touch "$stamps/$1-$(field "$1" 4)"
    rm -rf "${work:?}/$1" "$work/$1"-*
    echo "macOS deps: $1 ready"
}

ninja_path=$(command -v ninja)
pkg_config=$(command -v pkg-config)
# x86-64 libraries; nothing from Homebrew (/opt/homebrew is arm64, /usr/local may hold old Intel
# bottles) may leak into them.
cmake_x86=(-G Ninja -DCMAKE_MAKE_PROGRAM="$ninja_path" -DCMAKE_BUILD_TYPE=Release
    -DCMAKE_OSX_ARCHITECTURES=x86_64 -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOSX_DEPLOYMENT_TARGET"
    -DCMAKE_INSTALL_PREFIX="$prefix" -DCMAKE_PREFIX_PATH="$prefix" -DCMAKE_INSTALL_LIBDIR=lib
    -DCMAKE_IGNORE_PREFIX_PATH="/opt/homebrew;/usr/local;/opt/local" -DCMAKE_FIND_FRAMEWORK=LAST
    -DPKG_CONFIG_EXECUTABLE="$pkg_config" -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DBUILD_TESTING=OFF)
export PKG_CONFIG_LIBDIR=$prefix/lib/pkgconfig:$prefix/share/pkgconfig
cmake_build() { # source dir, build dir, extra args...
    local source=$1 build=$2; shift 2
    cmake -S "$source" -B "$build" "$@" >"$build.log" 2>&1 || { tail -30 "$build.log" >&2; exit 1; }
    ninja -C "$build" install >>"$build.log" 2>&1 || { grep -v '^\[' "$build.log" | tail -40 >&2; exit 1; }
}

if ! done_already nasm; then
    src=$(unpack nasm)
    (cd "$src" && ./configure --prefix="$host" >/dev/null && make -j"$jobs" nasm ndisasm >/dev/null &&
        mkdir -p "$host/bin" && cp nasm ndisasm "$host/bin/")
    mark_done nasm
fi
if ! done_already glslang; then
    src=$(unpack glslang)
    cmake_build "$src" "$work/glslang-build" -G Ninja -DCMAKE_MAKE_PROGRAM="$ninja_path" -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX="$host" -DENABLE_OPT=OFF -DGLSLANG_TESTS=OFF -DENABLE_HLSL=OFF \
        -DBUILD_EXTERNAL=OFF -DGLSLANG_ENABLE_INSTALL=ON -DCMAKE_POLICY_VERSION_MINIMUM=3.5
    mark_done glslang
fi
if ! done_already boost; then
    # Header-only use (Boost::headers): the headers plus a small package configuration.
    src=$(unpack boost)
    rm -rf "$prefix/include/boost" && mkdir -p "$prefix/include" && mv "$src/boost" "$prefix/include/boost"
    mkdir -p "$prefix/lib/cmake/Boost-1.90.0"
    cat > "$prefix/lib/cmake/Boost-1.90.0/BoostConfig.cmake" <<'EOF'
# gow3 (packaging/macos/deps.sh): header-only Boost.
get_filename_component(_gow3_boost_prefix "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)
set(Boost_VERSION 1.90.0)
set(Boost_VERSION_STRING 1.90.0)
set(Boost_INCLUDE_DIRS "${_gow3_boost_prefix}/include")
set(Boost_INCLUDE_DIR "${_gow3_boost_prefix}/include")
if(NOT TARGET Boost::headers)
    add_library(Boost::headers INTERFACE IMPORTED)
    set_target_properties(Boost::headers PROPERTIES INTERFACE_INCLUDE_DIRECTORIES "${_gow3_boost_prefix}/include")
    add_library(Boost::boost INTERFACE IMPORTED)
    set_target_properties(Boost::boost PROPERTIES INTERFACE_LINK_LIBRARIES Boost::headers)
endif()
set(Boost_FOUND TRUE)
EOF
    cat > "$prefix/lib/cmake/Boost-1.90.0/BoostConfigVersion.cmake" <<'EOF'
set(PACKAGE_VERSION 1.90.0)
if(PACKAGE_FIND_VERSION VERSION_GREATER PACKAGE_VERSION)
    set(PACKAGE_VERSION_COMPATIBLE FALSE)
else()
    set(PACKAGE_VERSION_COMPATIBLE TRUE)
    if(PACKAGE_FIND_VERSION STREQUAL PACKAGE_VERSION)
        set(PACKAGE_VERSION_EXACT TRUE)
    endif()
endif()
EOF
    mark_done boost
fi
if ! done_already fmt; then
    src=$(unpack fmt)
    cmake_build "$src" "$work/fmt-build" "${cmake_x86[@]}" -DFMT_TEST=OFF -DFMT_DOC=OFF -DFMT_INSTALL=ON \
        -DBUILD_SHARED_LIBS=OFF -DCMAKE_POSITION_INDEPENDENT_CODE=ON
    mark_done fmt
fi
if ! done_already robin-map; then
    src=$(unpack robin-map)
    cmake_build "$src" "$work/robin-map-build" "${cmake_x86[@]}"
    mark_done robin-map
fi
if ! done_already vulkan-headers; then
    src=$(unpack vulkan-headers)
    cmake_build "$src" "$work/vulkan-headers-build" "${cmake_x86[@]}" -DVULKAN_HEADERS_ENABLE_TESTS=OFF \
        -DVULKAN_HEADERS_ENABLE_MODULE=OFF
    mark_done vulkan-headers
fi
if ! done_already vma; then
    src=$(unpack vma)
    cmake_build "$src" "$work/vma-build" "${cmake_x86[@]}" -DVMA_BUILD_SAMPLES=OFF -DVMA_BUILD_DOCUMENTATION=OFF
    mark_done vma
fi
if ! done_already xxhash; then
    src=$(unpack xxhash)
    cmake_build "$src/cmake_unofficial" "$work/xxhash-build" "${cmake_x86[@]}" -DBUILD_SHARED_LIBS=OFF \
        -DXXHASH_BUILD_XXHSUM=OFF -DCMAKE_POSITION_INDEPENDENT_CODE=ON
    mark_done xxhash
fi
if ! done_already zycore; then
    # Zydis' package configuration needs an installed Zycore (its bundled copy is not installed).
    src=$(unpack zycore)
    cmake_build "$src" "$work/zycore-build" "${cmake_x86[@]}" -DZYCORE_BUILD_SHARED_LIB=OFF \
        -DZYCORE_BUILD_EXAMPLES=OFF -DZYCORE_BUILD_TESTS=OFF -DCMAKE_POSITION_INDEPENDENT_CODE=ON
    rm -f "$stamps"/zydis-*
    mark_done zycore
fi
if ! done_already zydis; then
    src=$(unpack zydis)
    # 4.1.1 links the plain name "Zycore", which only its bundled copy defines.
    sed -i '' 's/target_link_libraries("Zydis" PUBLIC "Zycore")/target_link_libraries("Zydis" PUBLIC Zycore::Zycore)/' "$src/CMakeLists.txt"
    grep -q 'PUBLIC Zycore::Zycore' "$src/CMakeLists.txt" || { echo 'macOS deps: zydis link patch did not apply' >&2; exit 1; }
    cmake_build "$src" "$work/zydis-build" "${cmake_x86[@]}" -DZYAN_SYSTEM_ZYCORE=ON -DZYDIS_BUILD_TOOLS=OFF \
        -DZYDIS_BUILD_EXAMPLES=OFF -DZYDIS_BUILD_DOXYGEN=OFF -DZYDIS_BUILD_SHARED_LIB=OFF -DZYDIS_BUILD_MAN=OFF \
        -DZYDIS_BUILD_TESTS=OFF -DCMAKE_POSITION_INDEPENDENT_CODE=ON
    mark_done zydis
fi
if ! done_already sdl3; then
    src=$(unpack sdl3)
    cmake_build "$src" "$work/sdl3-build" "${cmake_x86[@]}" -DSDL_SHARED=ON -DSDL_STATIC=OFF -DSDL_TEST_LIBRARY=OFF \
        -DSDL_TESTS=OFF -DSDL_EXAMPLES=OFF -DSDL_INSTALL_DOCS=OFF -DSDL_X11=OFF -DSDL_WAYLAND=OFF
    mark_done sdl3
fi
if ! done_already ffmpeg; then
    src=$(unpack ffmpeg)
    # Cross-compiled for x86-64 (configure runs no target programs); hand-written x86 SIMD kept.
    (cd "$src" && ./configure --prefix="$prefix" --enable-cross-compile --arch=x86_64 --target-os=darwin \
        --cc=clang --cxx=clang++ --host-cc=clang --x86asmexe="$host/bin/nasm" \
        --extra-cflags="-arch x86_64 -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET" \
        --extra-ldflags="-arch x86_64 -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET" \
        --enable-shared --disable-static --install-name-dir='@rpath' --disable-programs --disable-doc --disable-avdevice \
        --disable-network --disable-autodetect --enable-zlib >"$work/ffmpeg-configure.log" 2>&1 ||
        { tail -30 "$work/ffmpeg-configure.log" >&2; exit 1; }
     make -j"$jobs" >"$work/ffmpeg-build.log" 2>&1 || { tail -40 "$work/ffmpeg-build.log" >&2; exit 1; }
     make install >>"$work/ffmpeg-build.log" 2>&1)
    mark_done ffmpeg
fi
if ! done_already vulkan-loader; then
    src=$(unpack vulkan-loader)
    cmake_build "$src" "$work/vulkan-loader-build" "${cmake_x86[@]}" -DVULKAN_HEADERS_INSTALL_DIR="$prefix" \
        -DBUILD_TESTS=OFF -DUPDATE_DEPS=OFF -DBUILD_WERROR=OFF -DAPPLE_STATIC_LOADER=OFF
    mark_done vulkan-loader
fi
if ! done_already kosmickrisp; then
    # The Vulkan driver: Mesa's KosmicKrisp (Vulkan on Metal, macOS 26 or newer, Apple GPUs), the
    # x86-64 build shadPS4 ships with its macOS release, the same video core's driver there.
    # MoltenVK lacks robustBufferAccess2 and nullDescriptor, which the video core requires.
    archive=$(fetch kosmickrisp)
    rm -rf "$prefix/lib/vulkan" && mkdir -p "$prefix/lib/vulkan"
    unzip -q -o "$archive" libvulkan_kosmickrisp.dylib kosmickrisp_mesa_icd.json -d "$prefix/lib/vulkan"
    [[ $(lipo -archs "$prefix/lib/vulkan/libvulkan_kosmickrisp.dylib") == *x86_64* ]] ||
        { echo 'macOS deps: the KosmicKrisp driver is not x86-64' >&2; exit 1; }
    mark_done kosmickrisp
fi
if ! done_already mimalloc; then
    # The loader's allocator (macOS): every host heap allocation must stay below 1 TiB, because
    # the game packs pointers to host objects into 40 bits (see src/runtime_memory.c). Without
    # aligned hints mimalloc takes its memory from plain mmap, which macOS places bottom-up.
    src=$(unpack mimalloc)
    cmake_build "$src" "$work/mimalloc-build" "${cmake_x86[@]}" -DMI_OVERRIDE=ON -DMI_OSX_ZONE=ON \
        -DMI_OSX_INTERPOSE=OFF -DMI_BUILD_SHARED=OFF -DMI_BUILD_STATIC=ON -DMI_BUILD_OBJECT=ON \
        -DMI_BUILD_TESTS=OFF -DMI_INSTALL_TOPLEVEL=ON -DCMAKE_C_FLAGS="-DMI_NO_ALIGNED_HINT=1"
    object=$(find "$prefix/lib" -name 'mimalloc.o' | head -1)
    [[ -n $object ]] || object=$(find "$work/mimalloc-build" -name 'static.c.o' -path '*mimalloc-obj*' | head -1)
    [[ -n $object ]] || { echo 'macOS deps: mimalloc object file not found' >&2; exit 1; }
    cp "$object" "$prefix/lib/gow3-mimalloc.o"
    mark_done mimalloc
fi
echo "macOS deps: all in $prefix"
