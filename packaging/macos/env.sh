# Sourced by build.sh on macOS (from the repository root): the x86-64 libraries that
# packaging/macos/deps.sh builds, the system's own tools first on PATH (other SDKs may ship
# look-alikes), and CMake settings that keep Homebrew's packages (arm64) out of the build.
gow3_macos_deps=${GOW3_MACOS_DEPS:-$PWD/out/macos-deps}
export MACOSX_DEPLOYMENT_TARGET=15.0 # Rosetta 2 runs AVX code from macOS 15 on
export PKG_CONFIG_LIBDIR=$gow3_macos_deps/lib/pkgconfig:$gow3_macos_deps/share/pkgconfig
gow3_ninja=$(command -v ninja)
gow3_pkg_config=$(command -v pkg-config)
export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$(dirname -- "$(command -v cmake)"):$(dirname -- "$gow3_ninja"):$(dirname -- "$gow3_pkg_config"):$(dirname -- "$(python3 -c 'import sys; print(sys.executable)')")"
gow3_macos_cmake=(-DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER=clang++ -DCMAKE_MAKE_PROGRAM="$gow3_ninja"
    -DCMAKE_OSX_ARCHITECTURES=x86_64 -DCMAKE_OSX_DEPLOYMENT_TARGET="$MACOSX_DEPLOYMENT_TARGET"
    -DCMAKE_PREFIX_PATH="$gow3_macos_deps" -DCMAKE_IGNORE_PREFIX_PATH="/opt/homebrew;/usr/local;/opt/local"
    -DCMAKE_FIND_FRAMEWORK=LAST -DPKG_CONFIG_EXECUTABLE="$gow3_pkg_config"
    -DGLSLANG_COMPILER="$gow3_macos_deps/host/bin/glslang"
    -DVulkan_INCLUDE_DIR="$gow3_macos_deps/include" -DVulkan_LIBRARY="$gow3_macos_deps/lib/libvulkan.1.dylib")
# x86-64 C compiler for the loader, its tests and tools.
gow3_macos_cc=(clang -arch x86_64 -mmacosx-version-min="$MACOSX_DEPLOYMENT_TARGET")
