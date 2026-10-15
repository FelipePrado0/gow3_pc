// SPDX-FileCopyrightText: Copyright 2025 shadPS4 Emulator Project
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <filesystem>
#include <functional>
#include <utility>
#include <vector>
#include <optional>
#include <span>

#include "shader_recompiler/frontend/fetch_shader.h"
#include "video_core/renderer_vulkan/vk_pipeline_cache.h"
#include "video_core/renderer_vulkan/vk_shader_util.h"

namespace Vulkan {

namespace PipelineBinaryCache {
using Key = std::vector<u8>;
struct Binary {
    Key key;
    std::vector<u8> data;
    u64 content = 0; ///< version 2 files: XXH3 of `data`, which names its shared file
};
constexpr size_t MaxFileSize = 64 * 1024 * 1024;
constexpr u32 MaxBinaries = 32;

std::vector<u8> Encode(const Key& global, const Key& pipeline, u64 graphics_hash,
                       const std::vector<Binary>& binaries);
std::optional<std::vector<Binary>> Decode(std::span<const u8> bytes, const Key& global,
                                          const Key& pipeline, u64 graphics_hash);
bool Save(const std::filesystem::path& file, std::span<const u8> bytes);
std::optional<std::vector<Binary>> Load(const std::filesystem::path& file, const Key& global,
                                        const Key& pipeline, u64 graphics_hash);
/// gow3: version 2, the binary keys and content hashes only; the data is in SharedPath files
/// (StoreShared). The driver reuses a binary key for different data, so files are named by content.
std::vector<u8> EncodeShared(const Key& global, const Key& pipeline, u64 graphics_hash,
                             const std::vector<Binary>& binaries);
std::filesystem::path SharedPath(const std::filesystem::path& directory, const Key& global,
                                 const Key& binary, u64 content);
bool StoreShared(const std::filesystem::path& file, const Key& global, const Key& pipeline,
                 u64 graphics_hash, const std::vector<Binary>& binaries);
/// StoreShared, or the version 1 file with Gow3Toggle::SharedBinaries off.
bool Store(const std::filesystem::path& file, const Key& global, const Key& pipeline,
           u64 graphics_hash, const std::vector<Binary>& binaries);
/// Rewrites every version 1 file of `directory` as version 2: {files, bytes freed}.
std::pair<u64, s64> ConvertToShared(const std::filesystem::path& directory,
                                    const std::function<bool()>& stop = {});
} // namespace PipelineBinaryCache

vk::ResultValue<vk::UniquePipeline> CreateCachedGraphicsPipeline(
    vk::Device device, vk::PipelineCache cache, const vk::GraphicsPipelineCreateInfo& info,
    const GraphicsPipelineKey& key, const vk::PipelineBinaryKeyKHR& global_key,
    const std::filesystem::path& directory);
void ReportPipelineBinaryCache();

void RegisterPipelineData(const ComputePipelineKey& key,
                          ComputePipeline::SerializationSupport& sdata);
void RegisterPipelineData(const GraphicsPipelineKey& key, u64 hash,
                          GraphicsPipeline::SerializationSupport& sdata);
void RegisterShaderMeta(const Shader::Info& info,
                        const std::optional<Shader::Gcn::FetchShaderData>& fetch_shader_data,
                        const Shader::StageSpecialization& spec, size_t perm_hash, size_t perm_idx);
void RegisterShaderBinary(std::vector<u32>&& spv, u64 pgm_hash, size_t perm_idx);

} // namespace Vulkan
