# gow3 GPU library: shadPS4 video core + shader recompiler + Gnm/VideoOut
# drivers, adapted to the native_probe runtime through shims in shim/.
cmake_minimum_required(VERSION 3.24)
project(gow3gpu LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
if (NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE RelWithDebInfo)
endif()
# Frame pointers keep the loader's thread dumps (watchdog, SIGUSR2 sampling) walkable.
add_compile_options(-fno-omit-frame-pointer)

# gow3: link-time optimization (GOW3_LTO, default on) and profile-guided optimization
# (GOW3_PGO: off, generate = instrumented build writing into GOW3_PGO_DIR, use = build with that
# profile). Covers gow3gpu and the static libraries linked into it. No -march: the same build
# runs on the Steam Deck.
option(GOW3_LTO "Link-time optimization of gow3gpu" ON)
set(GOW3_PGO "off" CACHE STRING "Profile-guided optimization: off, generate, use")
set(GOW3_PGO_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../pgo" CACHE PATH "Profile directory")
if (GOW3_LTO)
    include(CheckIPOSupported)
    check_ipo_supported(RESULT gow3_ipo_ok OUTPUT gow3_ipo_error)
    if (gow3_ipo_ok)
        set(CMAKE_INTERPROCEDURAL_OPTIMIZATION ON)
    else()
        message(WARNING "LTO unavailable: ${gow3_ipo_error}")
    endif()
endif()
if (GOW3_PGO STREQUAL "generate")
    # Atomic counters: the GPU, recording, preparation and copy threads run the same code.
    add_compile_options(-fprofile-generate=${GOW3_PGO_DIR} -fprofile-update=atomic)
    add_link_options(-fprofile-generate=${GOW3_PGO_DIR})
    add_compile_definitions(GOW3_PGO_GENERATE)
elseif (GOW3_PGO STREQUAL "use")
    # Partial training keeps unprofiled code optimized normally; counters from concurrent
    # threads can be slightly inconsistent (correction). Functions changed since the profile
    # (or built differently for it, gow3gpu_init) are compiled without it.
    add_compile_options(-fprofile-use=${GOW3_PGO_DIR} -fprofile-partial-training
                        -fprofile-correction -Wno-missing-profile -Wno-coverage-mismatch)
endif()

find_package(Vulkan REQUIRED)
find_package(fmt REQUIRED)
find_package(Boost REQUIRED)
# gow3 (Windows): MSYS2 has no magic_enum/miniz packages; the submodules are used instead.
find_package(magic_enum QUIET)
if (NOT magic_enum_FOUND)
    add_library(magic_enum INTERFACE)
    add_library(magic_enum::magic_enum ALIAS magic_enum)
    target_include_directories(magic_enum INTERFACE third_party/magic_enum/include
                                                    third_party/magic_enum/include/magic_enum)
endif()
find_package(tsl-robin-map REQUIRED)
find_package(VulkanMemoryAllocator CONFIG QUIET)
if (NOT VulkanMemoryAllocator_FOUND)
    # MSYS2 ships the header (include/vma/vk_mem_alloc.h) without a CMake package.
    find_path(VMA_INCLUDE_DIR vk_mem_alloc.h PATH_SUFFIXES vma REQUIRED)
    add_library(VulkanMemoryAllocator INTERFACE)
    add_library(GPUOpen::VulkanMemoryAllocator ALIAS VulkanMemoryAllocator)
    target_include_directories(VulkanMemoryAllocator INTERFACE ${VMA_INCLUDE_DIR})
endif()
find_package(PkgConfig REQUIRED)
pkg_check_modules(XXHASH REQUIRED IMPORTED_TARGET libxxhash)
pkg_check_modules(SDL3 REQUIRED IMPORTED_TARGET sdl3)
if (NOT WIN32)
    pkg_check_modules(X11 REQUIRED IMPORTED_TARGET x11)
endif()
find_package(miniz QUIET)
if (NOT miniz_FOUND)
    set(BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
    set(BUILD_TESTS OFF CACHE BOOL "" FORCE)
    set(BUILD_FUZZERS OFF CACHE BOOL "" FORCE)
    set(BUILD_SHARED_LIBS OFF)
    set(CMAKE_POLICY_VERSION_MINIMUM 3.5)
    add_subdirectory(third_party/miniz EXCLUDE_FROM_ALL)
    unset(CMAKE_POLICY_VERSION_MINIMUM)
    add_library(miniz::miniz ALIAS miniz)
endif()
find_package(Zydis REQUIRED)
find_program(GLSLANG_COMPILER NAMES glslang glslangValidator REQUIRED)

set(SIRIT_USE_SYSTEM_SPIRV_HEADERS OFF CACHE BOOL "" FORCE)
add_subdirectory(third_party/sirit EXCLUDE_FROM_ALL)

# gow3: FSR 3.1 temporal upscaling / frame generation, native Vulkan (MIT, FireBurn/FSR-Vulkan).
set(FFX_VK_PORTABLE_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(FFX_VK_PORTABLE_BUILD_PROBE OFF CACHE BOOL "" FORCE)
add_subdirectory(third_party/fsr-vulkan EXCLUDE_FROM_ALL)

# Host shaders -> headers (same scheme as shadPS4's host_shaders/CMakeLists.txt).
set(SHADER_SRC ${CMAKE_CURRENT_SOURCE_DIR}/shadps4/video_core/host_shaders)
set(SHADER_INCLUDE ${CMAKE_CURRENT_BINARY_DIR}/include)
set(SHADER_DIR ${SHADER_INCLUDE}/video_core/host_shaders)
set(SHADER_HEADERS "")
function(add_host_shader FILENAME SHADER_NAME)
    set(OUT ${SHADER_DIR}/${SHADER_NAME}.h)
    string(TOUPPER ${SHADER_NAME} SYMBOL)
    set(DEFINES ${ARGN})
    list(TRANSFORM DEFINES PREPEND "-D")
    add_custom_command(OUTPUT ${OUT}
        COMMAND ${CMAKE_COMMAND} -E make_directory ${SHADER_DIR}
        COMMAND ${GLSLANG_COMPILER} --target-env vulkan1.3 -o ${OUT} --vn ${SYMBOL} ${DEFINES} ${SHADER_SRC}/${FILENAME}
        COMMAND ${CMAKE_COMMAND} "-DHEADER_FILE=${OUT}" -P ${SHADER_SRC}/AddShaderHeaderInclude.cmake
        DEPENDS ${SHADER_SRC}/${FILENAME} VERBATIM)
    set(SHADER_HEADERS ${SHADER_HEADERS} ${OUT} PARENT_SCOPE)
endfunction()
add_host_shader(color_to_ms_depth.frag color_to_ms_depth_frag)
add_host_shader(ms_image_blit.frag ms_image_blit_frag)
add_host_shader(ms_image_blit.frag ms_image_blit_msaa_frag SRC_MSAA=1)
add_host_shader(fault_buffer_process.comp fault_buffer_process_comp)
add_host_shader(fs_tri.vert fs_tri_vert)
add_host_shader(fsr.comp fsr_easu_comp SAMPLE_EASU=1)
add_host_shader(fsr.comp fsr_rcas_comp SAMPLE_RCAS=1)
add_host_shader(post_process.frag post_process_frag)
add_host_shader(camera_motion.comp camera_motion_comp)
add_host_shader(upscale_merge.comp upscale_merge_comp)
add_host_shader(upscale_reactive.comp upscale_reactive_comp)
add_host_shader(taa.comp taa_comp)
add_host_shader(taa_sharpen.comp taa_sharpen_comp)
add_host_shader(taa_sharpen.comp taa_sharpen_ldr_comp LDR_OUTPUT=1)
add_host_shader(buffer_multi_copy.comp buffer_multi_copy_comp)
add_host_shader(depth_resample.frag depth_resample_frag)
add_host_shader(depth_resample.frag depth_stencil_resample_frag HAS_STENCIL=1)
add_host_shader(depth_resample.frag depth_stencil_bits_frag STENCIL_BITS=1)
add_host_shader(camera_motion.comp camera_motion_debug_comp DEBUG_OVERLAY=1)
foreach(BPP IN ITEMS 8 16 32 64 96 128)
    add_host_shader(tiling.comp tiling_micro_${BPP}_comp BITS_PER_PIXEL=${BPP} IS_MACRO_TILED=0)
    add_host_shader(tiling.comp tiling_macro_${BPP}_comp BITS_PER_PIXEL=${BPP} IS_MACRO_TILED=1)
endforeach()
add_custom_target(host_shaders DEPENDS ${SHADER_HEADERS})

file(GLOB_RECURSE VIDEO_CORE CONFIGURE_DEPENDS shadps4/video_core/*.cpp)
file(GLOB_RECURSE RECOMPILER CONFIGURE_DEPENDS shadps4/shader_recompiler/*.cpp)
list(FILTER VIDEO_CORE EXCLUDE REGEX "renderdoc\\.cpp$")
set(COMMON
    shadps4/common/decoder.cpp shadps4/common/error.cpp
    shadps4/common/io_file.cpp shadps4/common/native_clock.cpp shadps4/common/number_utils.cpp
    shadps4/common/path_util.cpp shadps4/common/rdtsc.cpp shadps4/common/spin_lock.cpp shadps4/common/recursive_lock.cpp
    shadps4/common/string_util.cpp shadps4/common/thread.cpp shadps4/common/signal_context.cpp
    shim/common/logging/log.cpp)
file(GLOB AVPLAYER CONFIGURE_DEPENDS shadps4/core/libraries/avplayer/*.cpp)
pkg_check_modules(FFMPEG REQUIRED IMPORTED_TARGET libavformat libavcodec libavutil libswscale libswresample)
file(GLOB SHIM_SOURCES CONFIGURE_DEPENDS shim/*.cpp shim/*/*.cpp)
# Windows: a DLL cannot leave the runtime_* symbols for gow3-probe.exe to resolve, so the
# library is static there and linked into the executable.
if (WIN32)
    set(GOW3GPU_KIND STATIC)
else()
    set(GOW3GPU_KIND SHARED)
endif()
add_library(gow3gpu ${GOW3GPU_KIND} ${VIDEO_CORE} ${RECOMPILER} ${COMMON} ${SHIM_SOURCES}
    shadps4/core/libraries/gnmdriver/gnmdriver.cpp
    shadps4/core/libraries/videoout/driver.cpp shadps4/core/libraries/videoout/video_out.cpp
    shadps4/core/libraries/kernel/equeue.cpp ${AVPLAYER}
    shadps4/core/libraries/videodec/video_utils.cpp)
add_dependencies(gow3gpu host_shaders)
# In-game settings menu: Dear ImGui (third_party/imgui, MIT) with its Vulkan backend, and an
# embedded Cyrillic font (third_party/fonts).
target_sources(gow3gpu PRIVATE third_party/imgui/imgui.cpp third_party/imgui/imgui_draw.cpp
    third_party/imgui/imgui_tables.cpp third_party/imgui/imgui_widgets.cpp
    third_party/imgui/backends/imgui_impl_vulkan.cpp)
target_include_directories(gow3gpu PRIVATE third_party/imgui third_party/imgui/backends)
target_compile_definitions(gow3gpu PRIVATE IMGUI_IMPL_VULKAN_NO_PROTOTYPES
    GOW3_FONT_PATH="${CMAKE_CURRENT_SOURCE_DIR}/third_party/fonts/DejaVuSans.ttf")
set_source_files_properties(shim/gow3_overlay.cpp PROPERTIES
    OBJECT_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/third_party/fonts/DejaVuSans.ttf)
target_include_directories(gow3gpu PRIVATE shim shadps4 ${SHADER_INCLUDE} third_party/gcn/include third_party/half/include)
target_compile_definitions(gow3gpu PRIVATE BOOST_ASIO_STANDALONE NDEBUG)
target_compile_options(gow3gpu PRIVATE -Wno-deprecated-declarations -Wno-missing-field-initializers)
# Runtime symbols (runtime_memory_*, runtime_process_*) resolve against gow3-probe.
if (NOT WIN32)
    target_link_options(gow3gpu PRIVATE -Wl,--allow-shlib-undefined)
endif()
target_link_libraries(gow3gpu PRIVATE ffx-vulkan::portable ffx-vulkan::fsr4-v07-vulkan)
target_link_libraries(gow3gpu PUBLIC Vulkan::Headers fmt::fmt Boost::headers magic_enum::magic_enum
    tsl::robin_map GPUOpen::VulkanMemoryAllocator PkgConfig::XXHASH PkgConfig::SDL3 PkgConfig::FFMPEG miniz::miniz Zydis::Zydis sirit)
if (WIN32)
    # Guest code is entered with the System V ABI; NOMINMAX/WIN32_LEAN_AND_MEAN keep
    # <windows.h> out of the way of std::min/max and the Vulkan/SDL headers.
    target_compile_definitions(gow3gpu PUBLIC NOMINMAX WIN32_LEAN_AND_MEAN)
    target_compile_options(gow3gpu PRIVATE -include ${CMAKE_CURRENT_SOURCE_DIR}/shim/gow3_win_compat.h)
    target_include_directories(gow3gpu PRIVATE third_party/xbyak)
    target_link_libraries(gow3gpu PUBLIC Vulkan::Vulkan ws2_32 bcrypt ntdll synchronization winmm)
else()
    target_link_libraries(gow3gpu PUBLIC PkgConfig::X11)
endif()

# gow3 (Windows): gow3gpu is a static library there, so the loader and the HLE runtime are
# linked here, where CMake knows its dependencies (build.sh builds them with ninja).
if (WIN32)
    enable_language(RC)
    file(GLOB GOW3_RUNTIME CONFIGURE_DEPENDS ../src/runtime*.c)
    file(GLOB GOW3_ATRAC9 CONFIGURE_DEPENDS ../third_party/LibAtrac9/C/src/*.c)
    add_library(atrac9 STATIC ${GOW3_ATRAC9})
    target_compile_options(atrac9 PRIVATE -w)
    set_target_properties(atrac9 PROPERTIES C_STANDARD 99 INTERPROCEDURAL_OPTIMIZATION OFF)
    add_executable(gow3-probe ../src/probe.c ${GOW3_RUNTIME} ../src/compat_win.c ../src/vulkan_smoke.c
        ../packaging/windows/gow3-probe.rc)
    set_target_properties(gow3-probe PROPERTIES C_STANDARD 11 C_EXTENSIONS OFF)
    target_compile_options(gow3-probe PRIVATE $<$<COMPILE_LANGUAGE:C>:-Wall -Wextra -Werror>)
    target_include_directories(gow3-probe PRIVATE .. ../src)
    target_link_libraries(gow3-probe PRIVATE gow3gpu atrac9 onecore bcrypt ws2_32 -pthread)
    # The PS4 range (64 GiB - 1 TiB) must stay free: no high-entropy ASLR for heaps and stacks.
    # The guest main thread runs on the process stack: 16 MiB, committed (no stack probes).
    target_link_options(gow3-probe PRIVATE -Wl,--disable-high-entropy-va "SHELL:-Xlinker --stack -Xlinker 16777216,16777216")
    # gow3: runtime unit tests (ninja -C out/gpu mp3-test videodec-test services-test).
    add_executable(mp3-test EXCLUDE_FROM_ALL ../tests/test_mp3.c ../src/runtime_mp3.c ../src/compat_win.c)
    add_executable(videodec-test EXCLUDE_FROM_ALL ../tests/test_videodec.c ../src/compat_win.c)
    add_executable(services-test EXCLUDE_FROM_ALL ../tests/test_services.c ../src/compat_win.c)
    foreach(test mp3-test videodec-test services-test)
        set_target_properties(${test} PROPERTIES C_STANDARD 11 C_EXTENSIONS OFF)
        target_compile_options(${test} PRIVATE -Wall -Wextra -Werror)
        target_include_directories(${test} PRIVATE .. ../src)
        target_link_libraries(${test} PRIVATE atrac9 PkgConfig::FFMPEG bcrypt -pthread)
    endforeach()
    add_executable(pad-map-test EXCLUDE_FROM_ALL ../tests/test_pad_map.c ../src/compat_win.c)
    set_target_properties(pad-map-test PROPERTIES C_STANDARD 11 C_EXTENSIONS OFF)
    target_compile_options(pad-map-test PRIVATE -UNDEBUG -Wall -Wextra -Werror)
    target_include_directories(pad-map-test PRIVATE .. ../src)
    target_link_libraries(pad-map-test PRIVATE PkgConfig::SDL3 bcrypt -pthread)
    add_executable(gow3-gpu-capabilities ../tools/gpu_capabilities.c ../packaging/windows/gow3-gpu-capabilities.rc)
    target_link_libraries(gow3-gpu-capabilities PRIVATE Vulkan::Vulkan)
    # "Play God of War III.exe": the game with the launcher's saved settings, no launcher window.
    add_executable(gow3-play WIN32 ../packaging/windows/play.c ../packaging/windows/play.rc)
    target_compile_options(gow3-play PRIVATE -municode)
    target_link_options(gow3-play PRIVATE -municode)
endif()

# Explicit test targets; neither launches the game nor needs a Vulkan device.
add_executable(motion-history-test EXCLUDE_FROM_ALL ../tests/test_motion_history.cpp)
target_compile_options(motion-history-test PRIVATE -UNDEBUG -Wall -Wextra -Werror)
target_include_directories(motion-history-test PRIVATE ..)
add_executable(motion-shader-test EXCLUDE_FROM_ALL ../tests/test_motion_shaders.cpp ../tests/test_gpu_runtime_stubs.cpp)
target_include_directories(motion-shader-test PRIVATE shim shadps4 third_party/gcn/include)
target_link_libraries(motion-shader-test PRIVATE gow3gpu)
add_executable(aliasing-test EXCLUDE_FROM_ALL ../tests/test_aliasing.cpp
    shadps4/video_core/texture_cache/aliasing.cpp shadps4/video_core/texture_cache/host_compatibility.cpp)
target_compile_options(aliasing-test PRIVATE -UNDEBUG)
target_include_directories(aliasing-test PRIVATE shim shadps4 third_party/gcn/include)
target_link_libraries(aliasing-test PRIVATE Vulkan::Headers fmt::fmt Boost::headers magic_enum::magic_enum)
add_executable(blend-rewrite-test EXCLUDE_FROM_ALL ../tests/test_blend_rewrite.cpp)
target_compile_options(blend-rewrite-test PRIVATE -UNDEBUG)
target_include_directories(blend-rewrite-test PRIVATE shim shadps4 third_party/gcn/include)
target_link_libraries(blend-rewrite-test PRIVATE Vulkan::Headers fmt::fmt Boost::headers magic_enum::magic_enum)
add_executable(depth-stencil-state-test EXCLUDE_FROM_ALL ../tests/test_depth_stencil_state.cpp)
add_executable(async-compiler-test EXCLUDE_FROM_ALL ../tests/test_async_compiler.cpp)
target_compile_options(async-compiler-test PRIVATE -UNDEBUG)
target_include_directories(async-compiler-test PRIVATE shadps4)
add_executable(async-pipeline-cache-test EXCLUDE_FROM_ALL ../tests/test_async_pipeline_cache.cpp)
target_compile_options(async-pipeline-cache-test PRIVATE -UNDEBUG)
target_include_directories(async-pipeline-cache-test PRIVATE shadps4)
target_link_libraries(async-pipeline-cache-test PRIVATE Vulkan::Vulkan)
add_executable(async-fetch-snapshot-test EXCLUDE_FROM_ALL ../tests/test_async_fetch_snapshot.cpp
    shadps4/shader_recompiler/frontend/fetch_shader.cpp
    shadps4/shader_recompiler/frontend/decode.cpp
    shadps4/shader_recompiler/frontend/format.cpp)
target_compile_options(async-fetch-snapshot-test PRIVATE -UNDEBUG)
target_include_directories(async-fetch-snapshot-test PRIVATE shim shadps4 third_party/gcn/include)
target_link_libraries(async-fetch-snapshot-test PRIVATE Vulkan::Headers fmt::fmt Boost::headers magic_enum::magic_enum)
target_compile_options(depth-stencil-state-test PRIVATE -UNDEBUG)
target_include_directories(depth-stencil-state-test PRIVATE shim shadps4 third_party/gcn/include)
target_link_libraries(depth-stencil-state-test PRIVATE Vulkan::Headers fmt::fmt Boost::headers magic_enum::magic_enum)
add_executable(ui-composition-test EXCLUDE_FROM_ALL ../tests/test_ui_composition.cpp)
target_compile_options(ui-composition-test PRIVATE -UNDEBUG -Wall -Wextra -Werror)
target_include_directories(ui-composition-test PRIVATE ..)

add_executable(scene-resolution-test EXCLUDE_FROM_ALL ../tests/test_scene_resolution.cpp ../tests/test_gpu_runtime_stubs.cpp)
target_include_directories(scene-resolution-test PRIVATE shim shadps4 third_party/gcn/include)
# The test's Vulkan-Hpp inline methods have a different set of platform extensions
# from vk_platform.cpp. Keep them local and use an explicit dispatcher in the test.
target_compile_options(scene-resolution-test PRIVATE -UNDEBUG -fPIC)
set_source_files_properties(../tests/test_scene_resolution.cpp PROPERTIES
    COMPILE_OPTIONS "-fvisibility=hidden;-fvisibility-inlines-hidden")
target_link_libraries(scene-resolution-test PRIVATE gow3gpu)
add_executable(taa-shader-test EXCLUDE_FROM_ALL ../tests/test_taa_shader.cpp ../tests/test_gpu_runtime_stubs.cpp)
target_include_directories(taa-shader-test PRIVATE shim shadps4 ${SHADER_INCLUDE} third_party/gcn/include)
target_compile_options(taa-shader-test PRIVATE -UNDEBUG -fPIC)
set_source_files_properties(../tests/test_taa_shader.cpp PROPERTIES
    COMPILE_OPTIONS "-fvisibility=hidden;-fvisibility-inlines-hidden")
target_link_libraries(taa-shader-test PRIVATE gow3gpu)
add_dependencies(taa-shader-test host_shaders)

add_executable(camera-motion-test EXCLUDE_FROM_ALL ../tests/test_camera_motion.cpp ../tests/test_gpu_runtime_stubs.cpp)
target_include_directories(camera-motion-test PRIVATE shim shadps4 ${SHADER_INCLUDE} third_party/gcn/include)
target_compile_options(camera-motion-test PRIVATE -UNDEBUG -fPIC)
target_link_libraries(camera-motion-test PRIVATE gow3gpu)
add_dependencies(camera-motion-test host_shaders)

# gow3: FSR 4 benchmark outside the game (tools/fsr4_bench.cpp): ninja -C out/gpu fsr4-bench.
add_executable(upscaler-support-test EXCLUDE_FROM_ALL ../tests/test_upscaler_support.cpp
    shim/gow3_settings.cpp)
target_include_directories(upscaler-support-test PRIVATE shim)
target_compile_options(upscaler-support-test PRIVATE -UNDEBUG)
add_executable(orb-multiplier-test EXCLUDE_FROM_ALL ../tests/test_orb_multiplier.cpp
    shim/gow3_orbs.cpp shim/gow3_settings.cpp)
target_include_directories(orb-multiplier-test PRIVATE shim)
target_compile_options(orb-multiplier-test PRIVATE -UNDEBUG)
add_executable(cheat-hooks-test EXCLUDE_FROM_ALL ../tests/test_cheat_hooks.cpp)
target_compile_options(cheat-hooks-test PRIVATE -UNDEBUG)
add_executable(readback-queue-test EXCLUDE_FROM_ALL ../tests/test_readback_queue.cpp)
target_include_directories(readback-queue-test PRIVATE shadps4)
target_compile_options(readback-queue-test PRIVATE -UNDEBUG -Wall -Wextra -Werror)
add_executable(wait-diagnostics-test EXCLUDE_FROM_ALL ../tests/test_wait_diagnostics.cpp)
target_include_directories(wait-diagnostics-test PRIVATE shadps4)
target_compile_options(wait-diagnostics-test PRIVATE -UNDEBUG)
add_executable(actor-hook-test EXCLUDE_FROM_ALL ../tests/test_actor_hook.cpp)
target_include_directories(actor-hook-test PRIVATE shim)
target_compile_options(actor-hook-test PRIVATE -UNDEBUG -Wall -Wextra -Werror)
add_executable(readback-sources-test EXCLUDE_FROM_ALL ../tests/test_readback_sources.cpp)
target_include_directories(readback-sources-test PRIVATE shadps4)
target_compile_options(readback-sources-test PRIVATE -UNDEBUG -Wall -Wextra -Werror)
add_executable(page-state-test EXCLUDE_FROM_ALL ../tests/test_page_state.cpp)
target_include_directories(page-state-test PRIVATE ../tests/stubs shim shadps4)
target_link_libraries(page-state-test PRIVATE Boost::headers)
target_compile_options(page-state-test PRIVATE -UNDEBUG -Wall -Wextra -Werror)

add_executable(fsr4-bench EXCLUDE_FROM_ALL tools/fsr4_bench.cpp
    shadps4/video_core/renderer_vulkan/fsr411/fsr411.cpp)
target_include_directories(fsr4-bench PRIVATE shadps4/video_core/renderer_vulkan/fsr411)
target_link_libraries(fsr4-bench PRIVATE ffx-vulkan::fsr4-v07-vulkan ffx-vulkan::fsr4-v07-assets Vulkan::Vulkan)
