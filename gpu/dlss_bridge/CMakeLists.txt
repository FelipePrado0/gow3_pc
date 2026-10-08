# SPDX-FileCopyrightText: Copyright 2026 IFreemz (shadps4_dlss bridge), gow3 contributors
# SPDX-License-Identifier: MIT
#
# gow3_dlss.dll links the NVIDIA DLSS SDK (MSVC only) and is built separately from the port:
#   cmake -S gpu/dlss_bridge -B out/dlss-bridge -G "Visual Studio 17 2022" -A x64 \
#         -DDLSS_SDK_ROOT=<github.com/NVIDIA/DLSS checkout> -DVULKAN_INCLUDE=<Vulkan-Headers include>
#   cmake --build out/dlss-bridge --config Release

cmake_minimum_required(VERSION 3.20)
project(gow3_dlss LANGUAGES CXX)

set(DLSS_SDK_ROOT "" CACHE PATH "NVIDIA DLSS SDK root (contains include/ and lib/)")
set(VULKAN_INCLUDE "" CACHE PATH "Vulkan-Headers include directory")

if (NOT WIN32 OR NOT CMAKE_SIZEOF_VOID_P EQUAL 8)
    message(FATAL_ERROR "The DLSS bridge is Windows x64 only")
endif()
if (NOT EXISTS "${DLSS_SDK_ROOT}/include/nvsdk_ngx_vk.h")
    message(FATAL_ERROR "Set DLSS_SDK_ROOT to the NVIDIA DLSS SDK (github.com/NVIDIA/DLSS)")
endif()

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
# nvsdk_ngx_s.lib is the static-CRT (/MT) variant: no Visual C++ runtime install needed.
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded")

add_library(gow3_dlss SHARED gow3_dlss_bridge.cpp gow3_dlss_bridge.h)
target_include_directories(gow3_dlss PRIVATE "${DLSS_SDK_ROOT}/include" "${VULKAN_INCLUDE}")
target_link_libraries(gow3_dlss PRIVATE "${DLSS_SDK_ROOT}/lib/Windows_x86_64/x64/nvsdk_ngx_s.lib")
target_compile_definitions(gow3_dlss PRIVATE NOMINMAX WIN32_LEAN_AND_MEAN)
