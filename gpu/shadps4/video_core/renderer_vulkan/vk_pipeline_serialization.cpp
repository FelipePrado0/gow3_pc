// SPDX-FileCopyrightText: Copyright 2025-2026 shadPS4 Emulator Project
// SPDX-License-Identifier: GPL-2.0-or-later

#include <algorithm>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdlib>
#include <cstring>
#include <string>
#include <deque>
#include <mutex>
#include <shared_mutex>
#include <thread>
#include <unordered_set>
#include <unordered_map>
#include <fstream>
#include <map>
#ifdef _WIN32
#include <windows.h>
#endif
#include "common/elf_info.h"
#include "common/path_util.h"
#include "common/hash.h"
#include "common/serdes.h"
#include "common/scope_exit.h"
#include "video_core/renderer_vulkan/vk_pipeline_serialization.h"
#include "common/thread.h"
#include "gow3_overlay.h"
#include "gow3_toggles.h"
#include "core/emulator_settings.h"
#include "shader_recompiler/frontend/fetch_shader.h"
#include "shader_recompiler/info.h"
#include "video_core/cache_storage.h"
#include "video_core/renderer_vulkan/vk_amd_paths.h"
#include "video_core/renderer_vulkan/vk_instance.h"
#include "video_core/renderer_vulkan/vk_lighting_test.h"
#include "video_core/renderer_vulkan/vk_pipeline_cache.h"
#include "video_core/renderer_vulkan/vk_shader_util.h"
#include "video_core/renderer_vulkan/vk_warmup_inbox.h"

namespace Serialization {
/* You should increment versions below once corresponding serialization scheme is changed. */
static constexpr u32 ShaderBinaryVersion = 7u; // gow3: interpolated integer fix (Pascal)
static constexpr u32 ShaderMetaVersion = 7u; // gow3: ImageResource::needs_native
static constexpr u32 PipelineKeyVersion = 5u; // gow3: Info layout (ImageResource::needs_native)
} // namespace Serialization

namespace Vulkan {

namespace {
// gow3: the test knobs that change recompiled SPIR-V (GOW3_LIGHTING_TEST=dref,
// GOW3_AMD_PATHS_OFF=shaderext) get their own shader binary cache version.
u32 CurrentShaderBinaryVersion() {
    return LightingTest::ShaderBinaryVersion(
        Serialization::ShaderBinaryVersion,
        LightingTest::ShaderBinarySalt(LightingTest::Current().dref,
                                       AmdPathsOff::Current().shaderext));
}
} // namespace

namespace PipelineBinaryCache {
bool ValidKey(const Key& key) {
    return !key.empty() && key.size() <= VK_MAX_PIPELINE_BINARY_KEY_SIZE_KHR;
}
namespace {
// Version 1 holds the driver binaries; version 2 (gow3) their keys and content hashes, each
// binary being stored once in shared/ (StoreShared): one ~2 MB binary was in thousands of files.
std::vector<u8> EncodeVersion(const Key& global, const Key& pipeline, u64 graphics_hash,
                              const std::vector<Binary>& binaries, u32 version) {
    if (!ValidKey(global) || !ValidKey(pipeline) || binaries.empty() ||
        binaries.size() > MaxBinaries) {
        return {};
    }
    std::vector<u8> bytes;
    const auto append = [&](const auto& value) {
        const auto* p = reinterpret_cast<const u8*>(&value);
        bytes.insert(bytes.end(), p, p + sizeof(value));
    };
    const auto blob = [&](const auto& data) {
        append(u32(data.size()));
        bytes.insert(bytes.end(), data.begin(), data.end());
    };
    append(u32{0x31425047}); // GPB1, little-endian Windows/Linux cache format.
    append(version);
    append(graphics_hash);
    blob(global);
    blob(pipeline);
    append(u32(binaries.size()));
    for (const auto& binary : binaries) {
        const size_t data = version == 1 ? binary.data.size() : 0;
        if (!ValidKey(binary.key) || (version == 1 && binary.data.empty()) ||
            data > MaxFileSize || bytes.size() + binary.key.size() + data + 16 > MaxFileSize) {
            return {};
        }
        blob(binary.key);
        if (version == 1) {
            blob(binary.data);
        } else {
            append(u64{XXH3_64bits(binary.data.data(), binary.data.size())});
        }
    }
    append(u64{XXH3_64bits(bytes.data(), bytes.size())});
    return bytes;
}

struct Parsed {
    u32 version;
    u64 graphics_hash;
    Key global, pipeline;
    std::vector<Binary> binaries; ///< version 2: keys only
};

std::optional<Parsed> Parse(std::span<const u8> bytes) {
    if (bytes.size() < 8 || bytes.size() > MaxFileSize) {
        return std::nullopt;
    }
    u64 checksum;
    std::memcpy(&checksum, bytes.data() + bytes.size() - 8, 8);
    bytes = bytes.first(bytes.size() - 8);
    if (checksum != XXH3_64bits(bytes.data(), bytes.size())) {
        return std::nullopt;
    }
    size_t offset = 0;
    const auto read = [&](auto& value) {
        if (sizeof(value) > bytes.size() - offset) {
            return false;
        }
        std::memcpy(&value, bytes.data() + offset, sizeof(value));
        offset += sizeof(value);
        return true;
    };
    const auto blob = [&](auto& data, size_t limit) {
        u32 size;
        if (!read(size) || !size || size > limit || size > bytes.size() - offset) {
            return false;
        }
        data.assign(bytes.begin() + offset, bytes.begin() + offset + size);
        offset += size;
        return true;
    };
    Parsed parsed{};
    u32 magic, count;
    if (!read(magic) || magic != 0x31425047 || !read(parsed.version) ||
        (parsed.version != 1 && parsed.version != 2) || !read(parsed.graphics_hash) ||
        !blob(parsed.global, 32) || !blob(parsed.pipeline, 32) || !read(count) || !count ||
        count > MaxBinaries) {
        return std::nullopt;
    }
    parsed.binaries.resize(count);
    for (auto& binary : parsed.binaries) {
        if (!blob(binary.key, 32) || (parsed.version == 1 && !blob(binary.data, MaxFileSize)) ||
            (parsed.version == 2 && !read(binary.content))) {
            return std::nullopt;
        }
    }
    return offset == bytes.size() ? std::optional{std::move(parsed)} : std::nullopt;
}

std::optional<std::vector<u8>> ReadFile(const std::filesystem::path& file) {
    std::ifstream stream(file, std::ios::binary | std::ios::ate);
    if (!stream) {
        return std::nullopt;
    }
    const auto size = stream.tellg();
    if (size <= 0 || size > std::streamoff(MaxFileSize)) {
        return std::nullopt;
    }
    std::vector<u8> bytes(static_cast<size_t>(size));
    stream.seekg(0);
    if (!stream.read(reinterpret_cast<char*>(bytes.data()), bytes.size())) {
        return std::nullopt;
    }
    return bytes;
}
} // namespace

std::atomic<u64> shared_written{}, shared_reused{}, shared_bytes{};

std::vector<u8> Encode(const Key& global, const Key& pipeline, u64 graphics_hash,
                       const std::vector<Binary>& binaries) {
    return EncodeVersion(global, pipeline, graphics_hash, binaries, 1);
}
std::vector<u8> EncodeShared(const Key& global, const Key& pipeline, u64 graphics_hash,
                             const std::vector<Binary>& binaries) {
    return EncodeVersion(global, pipeline, graphics_hash, binaries, 2);
}
std::optional<std::vector<Binary>> Decode(std::span<const u8> bytes, const Key& global,
                                          const Key& pipeline, u64 graphics_hash) {
    if (!ValidKey(global) || !ValidKey(pipeline)) {
        return std::nullopt;
    }
    auto parsed = Parse(bytes);
    if (!parsed || parsed->graphics_hash != graphics_hash || parsed->global != global ||
        parsed->pipeline != pipeline) {
        return std::nullopt;
    }
    return std::move(parsed->binaries);
}
std::filesystem::path SharedPath(const std::filesystem::path& directory, const Key& global,
                                 const Key& binary, u64 content) {
    Key name = global;
    name.insert(name.end(), binary.begin(), binary.end());
    const auto* bytes = reinterpret_cast<const u8*>(&content);
    name.insert(name.end(), bytes, bytes + sizeof(content));
    return directory / "shared" /
           fmt::format("{:016x}.bin", XXH3_64bits(name.data(), name.size()));
}
bool Save(const std::filesystem::path& file, std::span<const u8> bytes) {
    if (bytes.empty() || bytes.size() > MaxFileSize) {
        return false;
    }
    std::error_code error;
    std::filesystem::create_directories(file.parent_path(), error);
    if (error) {
        return false;
    }
    // Unique siblings avoid concurrent writers truncating one another's temporary data.
    static std::atomic<u64> sequence{0};
    auto temp = file;
    temp += "." + std::to_string(std::chrono::steady_clock::now().time_since_epoch().count()) +
            "." + std::to_string(sequence++) + ".tmp";
    bool written = false;
    {
        std::ofstream stream(temp, std::ios::binary | std::ios::trunc);
        if (stream) {
            stream.write(reinterpret_cast<const char*>(bytes.data()), bytes.size());
            stream.flush();
            written = bool(stream);
            stream.close();
            written = written && !stream.fail();
        }
    }
    bool renamed = false;
    if (written) {
#ifdef _WIN32
        renamed = MoveFileExW(temp.c_str(), file.c_str(), MOVEFILE_REPLACE_EXISTING) != 0;
#else
        std::filesystem::rename(temp, file, error);
        renamed = !error;
#endif
    }
    if (!renamed) {
        std::filesystem::remove(temp, error);
    }
    return renamed;
}
std::optional<std::vector<Binary>> Load(const std::filesystem::path& file, const Key& global,
                                        const Key& pipeline, u64 graphics_hash) {
    const auto bytes = ReadFile(file);
    auto binaries = bytes ? Decode(*bytes, global, pipeline, graphics_hash) : std::nullopt;
    if (!binaries) {
        return std::nullopt;
    }
    // Version 2: each binary from its shared file, which holds it under its own key.
    for (auto& binary : *binaries) {
        if (!binary.data.empty()) {
            continue;
        }
        auto shared = Load(SharedPath(file.parent_path(), global, binary.key, binary.content),
                           global, binary.key, 0);
        if (!shared || shared->size() != 1 || (*shared)[0].key != binary.key ||
            (*shared)[0].data.empty() ||
            XXH3_64bits((*shared)[0].data.data(), (*shared)[0].data.size()) != binary.content) {
            return std::nullopt;
        }
        binary.data = std::move((*shared)[0].data);
    }
    return binaries;
}
// gow3: each binary is written once to shared/ (kept when it already reads back the same),
// then the pipeline file with the keys only.
bool StoreShared(const std::filesystem::path& file, const Key& global, const Key& pipeline,
                 u64 graphics_hash, const std::vector<Binary>& binaries) {
    for (const auto& binary : binaries) {
        const auto shared =
            SharedPath(file.parent_path(), global, binary.key,
                       XXH3_64bits(binary.data.data(), binary.data.size()));
        if (const auto existing = Load(shared, global, binary.key, 0);
            existing && existing->size() == 1 && (*existing)[0].data == binary.data) {
            ++shared_reused;
            continue;
        }
        const auto encoded = Encode(global, binary.key, 0, {binary});
        if (!Save(shared, encoded)) {
            return false;
        }
        ++shared_written;
        shared_bytes += encoded.size();
    }
    return Save(file, EncodeShared(global, pipeline, graphics_hash, binaries));
}
bool Store(const std::filesystem::path& file, const Key& global, const Key& pipeline,
           u64 graphics_hash, const std::vector<Binary>& binaries) {
    if (Gow3Toggle::Disabled(Gow3Toggle::SharedBinaries)) {
        return Save(file, Encode(global, pipeline, graphics_hash, binaries));
    }
    return StoreShared(file, global, pipeline, graphics_hash, binaries);
}
std::pair<u64, s64> ConvertToShared(const std::filesystem::path& directory,
                                    const std::function<bool()>& stop) {
    u64 files = 0;
    s64 freed = 0;
    std::error_code error;
    for (std::filesystem::directory_iterator it(directory, error), end; !error && it != end;
         it.increment(error)) {
        if (stop && stop()) {
            break;
        }
        std::error_code status;
        if (it->path().extension() != ".bin" || !it->is_regular_file(status)) {
            continue;
        }
        const auto bytes = ReadFile(it->path());
        auto parsed = bytes ? Parse(*bytes) : std::nullopt;
        if (!parsed || parsed->version != 1) {
            continue;
        }
        const u64 shared_before = shared_bytes;
        if (!StoreShared(it->path(), parsed->global, parsed->pipeline, parsed->graphics_hash,
                         parsed->binaries)) {
            continue;
        }
        ++files;
        freed += s64(bytes->size()) - s64(std::filesystem::file_size(it->path(), status)) -
                 s64(shared_bytes - shared_before);
    }
    return {files, freed};
}
} // namespace PipelineBinaryCache

namespace {
std::atomic<u64> binary_hits{}, binary_misses{}, binary_rejected{}, binary_saved{}, binary_errors{};
std::atomic<u64> binary_restore_ns{}, binary_compile_ns{};
std::atomic<u64> binary_restore_calls{}, binary_compile_calls{};
std::atomic<u64> binary_key_ns{}, binary_load_ns{}, binary_handles_ns{};
u64 ElapsedNs(std::chrono::steady_clock::time_point started) {
    return u64(std::chrono::duration_cast<std::chrono::nanoseconds>(
                   std::chrono::steady_clock::now() - started).count());
}
PipelineBinaryCache::Key BinaryKey(const vk::PipelineBinaryKeyKHR& key) {
    if (!key.keySize || key.keySize > VK_MAX_PIPELINE_BINARY_KEY_SIZE_KHR) {
        return {};
    }
    return {key.key.begin(), key.key.begin() + key.keySize};
}

std::optional<std::vector<PipelineBinaryCache::Binary>> LoadLegacyBinary(
    const std::filesystem::path& directory, const std::string& name,
    const PipelineBinaryCache::Key& global, const PipelineBinaryCache::Key& pipeline) {
    using Index = std::unordered_multimap<std::string, std::pair<std::filesystem::path, u64>>;
    static std::mutex mutex;
    static std::map<std::filesystem::path, Index> indexes;
    std::vector<std::pair<std::filesystem::path, u64>> candidates;
    {
        std::scoped_lock lock{mutex};
        auto [it, inserted] = indexes.try_emplace(directory);
        if (inserted) {
            std::error_code error;
            for (std::filesystem::directory_iterator files(directory, error), end;
                 !error && files != end; files.increment(error)) {
                const auto stem = files->path().stem().string();
                if (files->path().extension() != ".bin" || stem.size() < 18 || stem[16] != '_') {
                    continue;
                }
                try {
                    size_t used = 0;
                    const auto hash = std::stoull(stem.substr(0, 16), &used, 16);
                    if (used == 16) {
                        it->second.emplace(stem.substr(17), std::pair{files->path(), hash});
                    }
                } catch (const std::exception&) {
                }
            }
        }
        const auto [first, last] = it->second.equal_range(name);
        for (auto entry = first; entry != last; ++entry) {
            candidates.push_back(entry->second);
        }
    }
    for (const auto& [file, hash] : candidates) {
        if (auto binaries = PipelineBinaryCache::Load(file, global, pipeline, hash)) {
            return binaries;
        }
    }
    return std::nullopt;
}
} // namespace

void ReportPipelineBinaryCache() {
    const auto hits = binary_hits.load();
    const auto misses = binary_misses.load();
    std::printf("Pipeline binary cache: %llu hits, %llu compiled, %llu rejected, %llu saved, "
                "%llu cache errors (restore %.3f ms, compile %.3f ms each)\n",
                static_cast<unsigned long long>(hits), static_cast<unsigned long long>(misses),
                static_cast<unsigned long long>(binary_rejected.load()),
                static_cast<unsigned long long>(binary_saved.load()),
                static_cast<unsigned long long>(binary_errors.load()),
                binary_restore_ns.load() / 1e6 / std::max(u64{1}, binary_restore_calls.load()),
                binary_compile_ns.load() / 1e6 / std::max(u64{1}, binary_compile_calls.load()));
    std::printf("Pipeline binaries: %llu shared binaries written, %llu already shared\n",
                static_cast<unsigned long long>(PipelineBinaryCache::shared_written.load()),
                static_cast<unsigned long long>(PipelineBinaryCache::shared_reused.load()));
}

vk::ResultValue<vk::UniquePipeline> CreateCachedGraphicsPipeline(
    vk::Device device, vk::PipelineCache cache, const vk::GraphicsPipelineCreateInfo& info,
    const GraphicsPipelineKey& key, const vk::PipelineBinaryKeyKHR& global_key,
    const std::filesystem::path& directory) {
    using namespace PipelineBinaryCache;
    const auto& dispatch = VULKAN_HPP_DEFAULT_DISPATCHER;
    const bool enabled = global_key.keySize && EmulatorSettings.IsPipelineCacheEnabled();
    const auto create = [&](vk::PipelineCache used_cache, const auto& used_info, bool restored) {
        const auto started = std::chrono::steady_clock::now();
        auto result = device.createGraphicsPipelineUnique(used_cache, used_info);
        const auto ns = u64(std::chrono::duration_cast<std::chrono::nanoseconds>(
                                std::chrono::steady_clock::now() - started)
                                .count());
        extern std::atomic<u64> g_gow3_pipeline_driver_ns;
        g_gow3_pipeline_driver_ns += ns;
        if (enabled) {
            (restored ? binary_restore_ns : binary_compile_ns) += ns;
            ++(restored ? binary_restore_calls : binary_compile_calls);
        }
        return result;
    };
    if (!enabled) {
        return create(cache, info, false);
    }
    SCOPE_EXIT {
        const auto count = binary_hits.load() + binary_misses.load();
        if (count && count % 256 == 0) {
            ReportPipelineBinaryCache();
        }
    };
    const auto global = BinaryKey(global_key);
    vk::PipelineCreateFlags2CreateInfoKHR flags{
        .pNext = info.pNext,
        .flags = vk::PipelineCreateFlags2{static_cast<VkPipelineCreateFlags>(info.flags)} |
                 vk::PipelineCreateFlagBits2::eCaptureDataKHR,
    };
    auto captured_info = info;
    captured_info.pNext = &flags;
    vk::PipelineCreateInfoKHR key_info{.pNext = &captured_info};
    vk::PipelineBinaryKeyKHR pipeline_key;
    const auto key_started = std::chrono::steady_clock::now();
    const auto key_result = device.getPipelineKeyKHR(&key_info, &pipeline_key);
    binary_key_ns += ElapsedNs(key_started);
    if (key_result != vk::Result::eSuccess ||
        !ValidKey(global) || !ValidKey(BinaryKey(pipeline_key))) {
        ++binary_errors;
        ++binary_misses;
        return create(cache, info, false);
    }
    const auto pipeline_id = BinaryKey(pipeline_key);
    std::string name;
    for (const auto byte : pipeline_id) {
        name += fmt::format("{:02x}", byte);
    }
    // The driver key identifies the pipeline independently of emulator draw-state keys.
    const auto file = directory / (name + ".bin");
    std::vector<VkPipelineBinaryKHR> handles;
    const auto destroy = [&] {
        for (auto handle : handles) {
            if (handle) {
                dispatch.vkDestroyPipelineBinaryKHR(device, handle, nullptr);
            }
        }
        handles.clear();
    };
    SCOPE_EXIT { destroy(); };
    try {
        const auto load_started = std::chrono::steady_clock::now();
        auto binaries = Load(file, global, pipeline_id, 0);
        if (!binaries) {
            binaries = LoadLegacyBinary(directory, name, global, pipeline_id);
            if (binaries) {
                Store(file, global, pipeline_id, 0, *binaries);
            }
        }
        binary_load_ns += ElapsedNs(load_started);
        if (binaries) {
            std::vector<VkPipelineBinaryKeyKHR> keys(binaries->size());
            std::vector<VkPipelineBinaryDataKHR> data(binaries->size());
            for (size_t i = 0; i < binaries->size(); ++i) {
                keys[i].sType = VK_STRUCTURE_TYPE_PIPELINE_BINARY_KEY_KHR;
                keys[i].keySize = u32((*binaries)[i].key.size());
                std::ranges::copy((*binaries)[i].key, keys[i].key);
                data[i] = {(*binaries)[i].data.size(), (*binaries)[i].data.data()};
            }
            VkPipelineBinaryKeysAndDataKHR stored{u32(keys.size()), keys.data(), data.data()};
            VkPipelineBinaryCreateInfoKHR binary_info{
                VK_STRUCTURE_TYPE_PIPELINE_BINARY_CREATE_INFO_KHR};
            binary_info.pKeysAndDataInfo = &stored;
            handles.resize(keys.size());
            VkPipelineBinaryHandlesInfoKHR output{
                VK_STRUCTURE_TYPE_PIPELINE_BINARY_HANDLES_INFO_KHR};
            output.pipelineBinaryCount = u32(handles.size());
            output.pPipelineBinaries = handles.data();
            const auto handles_started = std::chrono::steady_clock::now();
            const auto handles_result =
                dispatch.vkCreatePipelineBinariesKHR(device, &binary_info, nullptr, &output);
            binary_handles_ns += ElapsedNs(handles_started);
            if (handles_result == VK_SUCCESS) {
                VkPipelineBinaryInfoKHR restored{VK_STRUCTURE_TYPE_PIPELINE_BINARY_INFO_KHR};
                restored.pNext = &flags;
                restored.binaryCount = u32(handles.size());
                restored.pPipelineBinaries = handles.data();
                // CAPTURE_DATA is only for compilation, never for binary-backed creation.
                flags.flags =
                    vk::PipelineCreateFlags2{static_cast<VkPipelineCreateFlags>(info.flags)};
                auto restored_info = info;
                restored_info.pNext = &restored;
                auto result = create(nullptr, restored_info, true);
                if (result.result == vk::Result::eSuccess) {
                    ++binary_hits;
                    return result;
                }
            }
            ++binary_rejected;
            destroy();
        }
    } catch (const std::exception&) {
        ++binary_errors;
        destroy();
    }
    ++binary_misses;
    flags.flags = vk::PipelineCreateFlags2{static_cast<VkPipelineCreateFlags>(info.flags)} |
                  vk::PipelineCreateFlagBits2::eCaptureDataKHR;
    auto result = create(nullptr, captured_info, false);
    if (result.result != vk::Result::eSuccess) {
        ++binary_errors;
        return create(cache, info, false);
    }
    VkReleaseCapturedPipelineDataInfoKHR release{
        VK_STRUCTURE_TYPE_RELEASE_CAPTURED_PIPELINE_DATA_INFO_KHR};
    release.pipeline = *result.value;
    SCOPE_EXIT {
        if (dispatch.vkReleaseCapturedPipelineDataKHR(device, &release, nullptr) != VK_SUCCESS) {
            ++binary_errors;
        }
    };
    try {
        VkPipelineBinaryCreateInfoKHR binary_info{
            VK_STRUCTURE_TYPE_PIPELINE_BINARY_CREATE_INFO_KHR};
        binary_info.pipeline = *result.value;
        VkPipelineBinaryHandlesInfoKHR output{VK_STRUCTURE_TYPE_PIPELINE_BINARY_HANDLES_INFO_KHR};
        if (dispatch.vkCreatePipelineBinariesKHR(device, &binary_info, nullptr, &output) !=
                VK_SUCCESS ||
            !output.pipelineBinaryCount || output.pipelineBinaryCount > MaxBinaries) {
            ++binary_errors;
            return result;
        }
        handles.resize(output.pipelineBinaryCount);
        output.pPipelineBinaries = handles.data();
        if (dispatch.vkCreatePipelineBinariesKHR(device, &binary_info, nullptr, &output) !=
            VK_SUCCESS) {
            ++binary_errors;
            return result;
        }
        std::vector<Binary> binaries;
        size_t total_size = 0;
        for (const auto handle : handles) {
            VkPipelineBinaryDataInfoKHR data_info{VK_STRUCTURE_TYPE_PIPELINE_BINARY_DATA_INFO_KHR};
            data_info.pipelineBinary = handle;
            vk::PipelineBinaryKeyKHR binary_key;
            size_t size = 0;
            auto* raw_key = reinterpret_cast<VkPipelineBinaryKeyKHR*>(&binary_key);
            if (dispatch.vkGetPipelineBinaryDataKHR(device, &data_info, raw_key, &size, nullptr) !=
                    VK_SUCCESS ||
                !size || size > MaxFileSize || total_size + size > MaxFileSize ||
                !ValidKey(BinaryKey(binary_key))) {
                ++binary_errors;
                return result;
            }
            total_size += size;
            Binary binary{BinaryKey(binary_key), std::vector<u8>(size)};
            if (dispatch.vkGetPipelineBinaryDataKHR(device, &data_info, raw_key, &size,
                                                    binary.data.data()) != VK_SUCCESS ||
                size > binary.data.size()) {
                ++binary_errors;
                return result;
            }
            binary.key = BinaryKey(binary_key);
            binary.data.resize(size);
            binaries.push_back(std::move(binary));
        }
        if (Store(file, global, pipeline_id, 0, binaries)) {
            ++binary_saved;
        } else {
            ++binary_errors;
        }
    } catch (const std::exception&) {
        ++binary_errors;
    }
    return result;
}

// gow3: GOW3_PARALLEL_WARMUP=1. The cache store is still read and the shader modules created in
// order on the calling thread (program_cache and its permutation checks are not thread-safe);
// only the pipeline objects, the slow part, are built here. The runtime compiler builds
// pipelines on worker threads the same way (vk_pipeline_cache.cpp).
struct PipelineCache::WarmupPool {
    explicit WarmupPool(u32 count, bool low_priority = false) {
        for (u32 i = 0; i < count; ++i) {
            threads.emplace_back([this, low_priority] {
                if (low_priority) {
                    Common::SetCurrentThreadName("GoW3:ShaderWarmup");
                    Common::SetCurrentThreadPriority(Common::ThreadPriority::Low);
                }
                Run();
            });
        }
    }
    ~WarmupPool() {
        {
            std::scoped_lock lock{mutex};
            jobs.clear(); // only the running ones finish
            closing = true;
        }
        cv.notify_all();
        for (auto& thread : threads) {
            thread.join();
        }
    }
    void Push(std::function<bool()> job) {
        {
            std::scoped_lock lock{mutex};
            jobs.push_back(std::move(job));
        }
        cv.notify_one();
    }
    /// Waits for every job; `tick` runs on this thread meanwhile (the loading screen).
    void Wait(const std::function<void()>& tick) {
        std::unique_lock lock{mutex};
        while (!jobs.empty() || running) {
            idle.wait_for(lock, std::chrono::milliseconds(50));
            lock.unlock();
            tick();
            lock.lock();
        }
    }
    /// Drops the queued jobs and waits for the running ones; the threads stay parked.
    void Drain() {
        std::unique_lock lock{mutex};
        jobs.clear();
        idle.wait(lock, [this] { return running == 0; });
    }
    bool Idle() {
        std::scoped_lock lock{mutex};
        return jobs.empty() && !running;
    }
    static u32 Threads() {
        const char* env = std::getenv("GOW3_PARALLEL_WARMUP");
        if (!env || !*env || *env == '0') {
            return 0;
        }
        const u32 cores = std::max(1u, std::thread::hardware_concurrency());
        return std::clamp(cores > 2 ? cores - 2 : 1u, 1u, 6u);
    }

    std::atomic<u32> built{0};
    std::atomic<u32> failed{0};

private:
    void Run() {
        std::unique_lock lock{mutex};
        while (true) {
            cv.wait(lock, [this] { return closing || !jobs.empty(); });
            if (jobs.empty()) {
                return;
            }
            auto job = std::move(jobs.front());
            jobs.pop_front();
            ++running;
            lock.unlock();
            bool ok = false;
            try {
                ok = job();
            } catch (const std::exception& e) {
                LOG_ERROR(Render_Vulkan, "gow3: warm-up pipeline failed: {}", e.what());
            } catch (...) {
                LOG_ERROR(Render_Vulkan, "gow3: warm-up pipeline failed");
            }
            (ok ? built : failed).fetch_add(1, std::memory_order_relaxed);
            lock.lock();
            --running;
            idle.notify_all();
        }
    }

    std::mutex mutex;
    std::condition_variable cv, idle;
    std::deque<std::function<bool()>> jobs;
    std::vector<std::thread> threads;
    u32 running = 0;
    bool closing = false;
};

void RegisterPipelineData(const ComputePipelineKey& key,
                          ComputePipeline::SerializationSupport& sdata) {
    if (!Storage::DataBase::Instance().IsOpened()) {
        return;
    }

    Serialization::Archive ar{};
    Serialization::Writer pldata{ar};

    pldata.Write(Serialization::PipelineKeyVersion);
    pldata.Write(u32{1}); // compute

    key.Serialize(ar);
    sdata.Serialize(ar);

    Storage::DataBase::Instance().Save(Storage::BlobType::PipelineKey,
                                       fmt::format("c_{:#018x}", key.value), ar.TakeOff());
}

void RegisterPipelineData(const GraphicsPipelineKey& key, u64 hash,
                          GraphicsPipeline::SerializationSupport& sdata) {
    if (!Storage::DataBase::Instance().IsOpened()) {
        return;
    }

    Serialization::Archive ar{};
    Serialization::Writer pldata{ar};

    pldata.Write(Serialization::PipelineKeyVersion);
    pldata.Write(u32{0}); // graphics

    key.Serialize(ar);
    sdata.Serialize(ar);

    Storage::DataBase::Instance().Save(Storage::BlobType::PipelineKey,
                                       fmt::format("g_{:#018x}", hash), ar.TakeOff());
}

void RegisterShaderMeta(const Shader::Info& info,
                        const std::optional<Shader::Gcn::FetchShaderData>& fetch_shader_data,
                        const Shader::StageSpecialization& spec, size_t perm_hash,
                        size_t perm_idx) {
    if (!Storage::DataBase::Instance().IsOpened()) {
        return;
    }

    Serialization::Archive ar;
    Serialization::Writer meta{ar};

    meta.Write(Serialization::ShaderMetaVersion);
    meta.Write(CurrentShaderBinaryVersion());

    meta.Write(perm_hash);
    meta.Write(perm_idx);

    spec.Serialize(ar);
    info.Serialize(ar);

    Storage::DataBase::Instance().Save(Storage::BlobType::ShaderMeta,
                                       fmt::format("{:#018x}", perm_hash), ar.TakeOff());
}

void RegisterShaderBinary(std::vector<u32>&& spv, u64 pgm_hash, size_t perm_idx) {
    if (!Storage::DataBase::Instance().IsOpened()) {
        return;
    }

    Storage::DataBase::Instance().Save(Storage::BlobType::ShaderBinary,
                                       fmt::format("{:#018x}_{}", pgm_hash, perm_idx),
                                       std::move(spv));
}

bool LoadShaderMeta(Serialization::Archive& ar, Shader::Info& info,
                    std::optional<Shader::Gcn::FetchShaderData>& fetch_shader_data,
                    Shader::StageSpecialization& spec, size_t& perm_idx) {
    Serialization::Reader meta{ar};

    u32 meta_version{};
    meta.Read(meta_version);
    if (meta_version != Serialization::ShaderMetaVersion) {
        return false;
    }

    u32 binary_version{};
    meta.Read(binary_version);
    if (binary_version != CurrentShaderBinaryVersion()) {
        return false;
    }

    u64 perm_hash_ar{};
    meta.Read(perm_hash_ar);
    meta.Read(perm_idx);

    spec.Deserialize(ar);
    info.Deserialize(ar);

    // Motion vertex shaders embed session-local buffer device addresses. They must be
    // recompiled for the current allocation, never loaded from a previous process.
    if (info.hw_stage == Shader::HwStage::Vertex && spec.runtime_info.hw.vs.motion_vectors) {
        return false;
    }

    fetch_shader_data = spec.fetch_shader_data;
    return true;
}

void ComputePipelineKey::Serialize(Serialization::Archive& ar) const {
    Serialization::Writer key{ar};
    key.Write(value);
}

bool ComputePipelineKey::Deserialize(Serialization::Archive& ar) {
    Serialization::Reader key{ar};
    key.Read(value);
    return true;
}

void ComputePipeline::SerializationSupport::Serialize(Serialization::Archive& ar) const {
    // Nothing here yet
    return;
}

bool ComputePipeline::SerializationSupport::Deserialize(Serialization::Archive& ar) {
    // Nothing here yet
    return true;
}

bool PipelineCache::LoadComputePipeline(Serialization::Archive& ar) {
    compute_key.Deserialize(ar);

    ComputePipeline::SerializationSupport sdata{};
    sdata.Deserialize(ar);

    std::vector<u8> meta_blob;
    Storage::DataBase::Instance().Load(Storage::BlobType::ShaderMeta,
                                       fmt::format("{:#018x}", compute_key.value), meta_blob);
    if (meta_blob.empty()) {
        return false;
    }

    Serialization::Archive meta_ar{std::move(meta_blob)};

    if (!LoadPipelineStage(meta_ar, 0)) {
        return false;
    }

    const auto [it, is_new] = compute_pipelines.try_emplace(compute_key);
    ASSERT(is_new);

    if (warmup_pool) {
        // The slot stays null until WarmUp moves the built pipeline in (the map may rehash).
        warmup_pool->Push([this, key = compute_key, info = sel.infos[0], module = sel.modules[0],
                           sdata]() mutable {
            auto pipeline = std::make_unique<ComputePipeline>(instance, scheduler, desc_heap,
                                                              profile, *pipeline_cache, key, *info,
                                                              module, sdata, true);
            std::scoped_lock lock{warmup_results_mutex};
            warmup_compute.emplace_back(key, std::move(pipeline));
            return true;
        });
    } else {
        it.value() = std::make_unique<ComputePipeline>(instance, scheduler, desc_heap, profile,
                                                       *pipeline_cache, compute_key,
                                                       *sel.infos[0], sel.modules[0], sdata, true);
    }

    sel.infos.fill(nullptr);
    sel.modules.fill(nullptr);

    return true;
}

void GraphicsPipelineKey::Serialize(Serialization::Archive& ar) const {
    Serialization::Writer key{ar};

    key.Write(this, sizeof(*this));
}

bool GraphicsPipelineKey::Deserialize(Serialization::Archive& ar) {
    Serialization::Reader key{ar};

    key.Read(this, sizeof(*this));
    return true;
}

void GraphicsPipeline::SerializationSupport::Serialize(Serialization::Archive& ar) const {
    Serialization::Writer sdata{ar};

    sdata.Write(&vertex_attributes, sizeof(vertex_attributes));
    sdata.Write(&vertex_bindings, sizeof(vertex_bindings));
    sdata.Write(&divisors, sizeof(divisors));
    sdata.Write(multisampling);
    sdata.Write(tcs);
    sdata.Write(tes);
}

bool GraphicsPipeline::SerializationSupport::Deserialize(Serialization::Archive& ar) {
    Serialization::Reader sdata{ar};

    sdata.Read(&vertex_attributes, sizeof(vertex_attributes));
    sdata.Read(&vertex_bindings, sizeof(vertex_bindings));
    sdata.Read(&divisors, sizeof(divisors));
    sdata.Read(multisampling);
    sdata.Read(tcs);
    sdata.Read(tes);
    return true;
}

bool PipelineCache::LoadGraphicsPipeline(Serialization::Archive& ar) {
    sel.graphics_key.Deserialize(ar);

    GraphicsPipeline::SerializationSupport sdata{};
    sdata.Deserialize(ar);

    for (int stage_idx = 0; stage_idx < MaxShaderStages; ++stage_idx) {
        const auto& hash = sel.graphics_key.stage_hashes[stage_idx];
        if (!hash) {
            continue;
        }

        std::vector<u8> meta_blob;
        Storage::DataBase::Instance().Load(Storage::BlobType::ShaderMeta,
                                           fmt::format("{:#018x}", hash), meta_blob);
        if (meta_blob.empty()) {
            return false;
        }

        Serialization::Archive meta_ar{std::move(meta_blob)};

        if (!LoadPipelineStage(meta_ar, stage_idx)) {
            return false;
        }
    }

    const auto [it, is_new] = graphics_pipelines.try_emplace(sel.graphics_key);
    ASSERT(is_new);

    if (warmup_pool) {
        warmup_pool->Push([this, key = sel.graphics_key, infos = sel.infos,
                           runtime_infos = sel.runtime_infos, fetch = sel.fetch_shader,
                           modules = sel.modules, sdata]() mutable {
            auto pipeline = std::make_unique<GraphicsPipeline>(
                instance, scheduler, desc_heap, profile, key, *pipeline_cache, infos,
                runtime_infos, fetch, modules, sdata, true);
            std::scoped_lock lock{warmup_results_mutex};
            warmup_graphics.emplace_back(key, std::move(pipeline));
            return true;
        });
    } else {
        it.value() = std::make_unique<GraphicsPipeline>(
            instance, scheduler, desc_heap, profile, sel.graphics_key, *pipeline_cache, sel.infos,
            sel.runtime_infos, sel.fetch_shader, sel.modules, sdata, true);
    }

    sel.infos.fill(nullptr);
    sel.modules.fill(nullptr);
    sel.fetch_shader.reset();

    return true;
}

bool PipelineCache::LoadPipelineStage(Serialization::Archive& ar, size_t stage) {
    auto program = std::make_unique<Program>();
    Shader::StageSpecialization spec{};
    spec.info = &program->info;
    size_t perm_idx{};
    if (!LoadShaderMeta(ar, program->info, sel.fetch_shader, spec, perm_idx)) {
        return false;
    }

    std::vector<u32> spv{};
    Storage::DataBase::Instance().Load(Storage::BlobType::ShaderBinary,
                                       fmt::format("{:#018x}_{}", program->info.pgm_hash, perm_idx),
                                       spv);
    if (spv.empty()) {
        return false;
    }

    // Permutation hash depends on shader variation index. To prevent collisions, we need insert it
    // at the exact position rather than append

    vk::ShaderModule module{};

    auto [it_pgm, new_program] = program_cache.try_emplace(program->info.pgm_hash);
    if (new_program) {
        module = CompileSPV(spv, instance.GetDevice());
        it_pgm.value() = std::move(program);
    } else {
        const auto& it = std::ranges::find(it_pgm.value()->modules, spec, &Program::Module::spec);
        if (it != it_pgm.value()->modules.end()) {
            // A matching permutation is valid only at its original index. A different index means
            // the store holds entries from more than one cache generation, so this pipeline is
            // left to compile at runtime.
            const auto idx = std::distance(it_pgm.value()->modules.begin(), it);
            if (perm_idx != idx) {
                LOG_WARNING(Render_Vulkan,
                            "Cached permutation {} of {}_{:x} conflicts with index {}, skipping "
                            "preload",
                            perm_idx, program->info.hw_stage, program->info.pgm_hash, idx);
                return false;
            }
            module = it->module;
        } else {
            module = CompileSPV(spv, instance.GetDevice());
        }
    }
    it_pgm.value()->InsertPermut(module, std::move(spec), perm_idx);

    sel.infos[stage] = &it_pgm.value()->info;
    sel.modules[stage] = module;

    return true;
}

bool PipelineCache::CheckCacheProfile() {
    if (!EmulatorSettings.IsPipelineCacheEnabled()) {
        return false;
    }

    Storage::DataBase::Instance().Open();

    // Check if cache is compatible
    std::vector<u8> profile_data{};
    Storage::DataBase::Instance().Load(Storage::BlobType::ShaderProfile, "profile", profile_data);
    if (profile_data.empty()) {
        Storage::DataBase::Instance().FinishPreload();

        profile_data.resize(sizeof(profile));
        std::memcpy(profile_data.data(), &profile, sizeof(profile));
        Storage::DataBase::Instance().Save(Storage::BlobType::ShaderProfile, "profile",
                                           std::move(profile_data));
        return false;
    }
    if (profile_data.size() != sizeof(Shader::Profile)) {
        LOG_WARNING(Render, "Pipeline cache profile has unexpected size ({} != {})",
                    profile_data.size(), sizeof(Shader::Profile));
    }
    Shader::Profile cached_profile{};
    if (profile_data.size() == sizeof(Shader::Profile)) {
        std::memcpy(&cached_profile, profile_data.data(), sizeof(cached_profile));
    }
    if (profile_data.size() != sizeof(Shader::Profile) || cached_profile != profile) {
        // gow3: upstream closed the cache for the session here, so it was never rewritten
        // and every later session compiled every shader again (stutters on each new area).
        // Start a fresh cache for this build and GPU instead.
        LOG_WARNING(Render, "Pipeline cache isn't compatible with current system: rebuilding it");
        Storage::DataBase::Instance().Clear();
        Storage::DataBase::Instance().FinishPreload();
        profile_data.resize(sizeof(profile));
        std::memcpy(profile_data.data(), &profile, sizeof(profile));
        Storage::DataBase::Instance().Save(Storage::BlobType::ShaderProfile, "profile",
                                           std::move(profile_data));
        return false;
    }
    return true;
}

void PipelineCache::WarmUp(const std::function<void(u32, u32)>& progress) {
    if (!CheckCacheProfile()) {
        return;
    }

    u32 num_pipelines{};
    u32 num_total_pipelines{};
    const u32 expected =
        u32(Storage::DataBase::Instance().CountBlobs(Storage::BlobType::PipelineKey));
    const auto started = std::chrono::steady_clock::now();
    const u32 warmup_threads = WarmupPool::Threads();
    std::unique_ptr<WarmupPool> pool;
    if (warmup_threads) {
        pool = std::make_unique<WarmupPool>(warmup_threads);
        warmup_pool = pool.get();
    }
    // In parallel mode the screen follows the pipelines built, not the ones read.
    const auto report = [&] {
        if (progress) {
            const u32 done = pool ? pool->built + pool->failed : num_total_pipelines;
            progress(done, std::max(expected, num_total_pipelines));
        }
    };

    Storage::DataBase::Instance().ForEachBlob(
        Storage::BlobType::PipelineKey, [&](std::vector<u8>&& data) {
            ++num_total_pipelines;
            report();

            Serialization::Archive ar{std::move(data)};
            Serialization::Reader pldata{ar};

            u32 version{};
            pldata.Read(version);
            if (version != Serialization::PipelineKeyVersion) {
                return;
            }

            u32 is_compute{};
            pldata.Read(is_compute);

            bool result{};
            if (is_compute) {
                result = LoadComputePipeline(ar);
            } else {
                result = LoadGraphicsPipeline(ar);
            }

            if (result) {
                ++num_pipelines;
            }
        });

    if (pool) {
        pool->Wait(report);
        warmup_pool = nullptr;
        // Every slot was reserved by the loaders; the ones whose build failed stay out.
        for (auto& [key, pipeline] : warmup_graphics) {
            graphics_pipelines[key] = std::move(pipeline);
        }
        for (auto& [key, pipeline] : warmup_compute) {
            compute_pipelines[key] = std::move(pipeline);
        }
        for (auto it = graphics_pipelines.begin(); it != graphics_pipelines.end();) {
            it = it->second ? std::next(it) : graphics_pipelines.erase(it);
        }
        for (auto it = compute_pipelines.begin(); it != compute_pipelines.end();) {
            it = it->second ? std::next(it) : compute_pipelines.erase(it);
        }
        num_pipelines -= pool->failed;
        warmup_graphics.clear();
        warmup_compute.clear();
        pool.reset();
    }
    std::printf("Pipeline warm-up: %u of %u pipelines in %.2f s (%u threads)\n", num_pipelines,
                num_total_pipelines,
                std::chrono::duration<double>(std::chrono::steady_clock::now() - started).count(),
                warmup_threads);
    LOG_INFO(Render, "Preloaded {} pipelines", num_pipelines);
    ReportPipelineBinaryCache();
    if (num_total_pipelines > num_pipelines) {
        LOG_WARNING(Render, "{} stale pipelines were found. Consider re-generating the cache",
                    num_total_pipelines - num_pipelines);
    }

    Storage::DataBase::Instance().FinishPreload();
}

void PipelineCache::StartOnDemand() {
    const bool compatible = CheckCacheProfile();
    auto& db = Storage::DataBase::Instance();
    // Launcher: "Reuse compiled shaders from disk" (GOW3_DISK_SHADERS), on by default.
    const bool disk_on = EmulatorSettingsImpl::Flag("GOW3_DISK_SHADERS", true) &&
                         !Gow3Toggle::Disabled(Gow3Toggle::DiskShaders);
    if (db.IsOpened() && db.ConcurrentReads() && !disk_on) {
        // This session numbers permutations by first use again: the store is rebuilt at the
        // next start with the bit on.
        db.Save(Storage::BlobType::ShaderProfile, "disk_shaders", std::vector<u8>{0});
    } else if (db.IsOpened() && db.ConcurrentReads()) {
        std::vector<u8> marker;
        if (compatible) {
            db.Load(Storage::BlobType::ShaderProfile, "disk_shaders", marker);
        }
        if (marker != std::vector<u8>{1}) {
            // Earlier on-demand sessions numbered permutations by first use and overwrote the
            // files by name, so a name may hold another session's shader: rebuilt once.
            db.Clear();
            std::vector<u8> profile_data(sizeof(profile));
            std::memcpy(profile_data.data(), &profile, sizeof(profile));
            db.Save(Storage::BlobType::ShaderProfile, "profile", std::move(profile_data));
            db.Save(Storage::BlobType::ShaderProfile, "disk_shaders", std::vector<u8>{1});
            std::printf("Pipeline startup: shader store rebuilt for on-demand reads\n");
        } else {
            IndexDiskShaders();
            // Launcher: "Read the shader cache into memory at start" (GOW3_SHADER_PREFETCH).
            if (EmulatorSettingsImpl::Flag("GOW3_SHADER_PREFETCH", true)) {
                StartDiskPrefetch();
            }
        }
        disk_shaders = true;
    }
    db.FinishPreload();
    StartBinaryConversion();
    std::printf("Pipeline startup: on demand, no bulk warm-up (%zu saved programs)\n",
                disk_permutations.size());
}

bool PipelineCache::DiskShadersActive() const {
    return disk_shaders && !Gow3Toggle::Disabled(Gow3Toggle::DiskShaders);
}

void PipelineCache::IndexDiskShaders() {
    auto& db = Storage::DataBase::Instance();
    for (const auto& name : db.ListBlobs(Storage::BlobType::ShaderBinary)) {
        const auto sep = name.rfind('_');
        if (sep == std::string::npos) {
            continue;
        }
        const u64 hash = std::strtoull(name.substr(0, sep).c_str(), nullptr, 16);
        const auto idx = std::strtoul(name.c_str() + sep + 1, nullptr, 10);
        if (hash && idx < 64) {
            disk_permutations[hash].push_back(u32(idx));
        }
    }
    for (auto& [hash, indices] : disk_permutations) {
        std::ranges::sort(indices);
    }
    for (auto& name : db.ListBlobs(Storage::BlobType::PipelineKey)) {
        disk_pipeline_keys.insert(std::move(name));
    }
}

// gow3: the saved files of one program, read off the GPU thread (DiskPrefetch) or on it.
struct DiskBlob {
    u32 idx;
    std::vector<u8> meta;
    std::vector<u32> spv;
};

static std::vector<DiskBlob> ReadDiskProgram(u64 hash, const std::vector<u32>& indices) {
    auto& db = Storage::DataBase::Instance();
    std::vector<DiskBlob> blobs;
    for (const u32 idx : indices) {
        DiskBlob blob{idx};
        db.Load(Storage::BlobType::ShaderMeta, fmt::format("{:#018x}", HashCombine(hash, u64{idx})),
                blob.meta);
        db.Load(Storage::BlobType::ShaderBinary, fmt::format("{:#018x}_{}", hash, idx), blob.spv);
        if (!blob.meta.empty() && !blob.spv.empty()) {
            blobs.push_back(std::move(blob));
        }
    }
    return blobs;
}

// gow3: one low-priority thread reads every saved program into memory at startup (a few MB),
// so the GPU thread only deserializes and creates the modules. A program the GPU thread needs
// first is claimed and read there instead.
struct PipelineCache::DiskPrefetch {
    std::mutex mutex;
    std::unordered_map<u64, std::vector<DiskBlob>> ready;
    std::unordered_set<u64> claimed;
    std::jthread thread;
};

void PipelineCache::StartDiskPrefetch() {
    std::vector<std::pair<u64, std::vector<u32>>> work(disk_permutations.begin(),
                                                       disk_permutations.end());
    if (work.empty()) {
        return;
    }
    disk_prefetch = new DiskPrefetch;
    disk_prefetch->thread = std::jthread{[prefetch = disk_prefetch, work = std::move(work)](
                                             std::stop_token stop) {
        Common::SetCurrentThreadName("gow3:ShaderPrefetch");
        Common::SetCurrentThreadPriority(Common::ThreadPriority::Low);
        const auto started = std::chrono::steady_clock::now();
        const u32 total = u32(work.size());
        u32 done = 0;
        u64 programs = 0, bytes = 0;
        for (const auto& [hash, indices] : work) {
            if (stop.stop_requested()) {
                Gow3Overlay::SetShaderPrefetch(0, 0);
                return;
            }
            Gow3Overlay::SetShaderPrefetch(done++, total);
            {
                std::scoped_lock lock{prefetch->mutex};
                if (prefetch->claimed.contains(hash)) {
                    continue;
                }
            }
            auto blobs = ReadDiskProgram(hash, indices);
            for (const auto& blob : blobs) {
                bytes += blob.meta.size() + blob.spv.size() * sizeof(u32);
            }
            std::scoped_lock lock{prefetch->mutex};
            if (!prefetch->claimed.contains(hash)) {
                prefetch->ready.emplace(hash, std::move(blobs));
                ++programs;
            }
        }
        Gow3Overlay::SetShaderPrefetch(total, total);
        std::printf("Disk shaders: prefetch read %llu programs (%.1f MB) in %.0f ms\n",
                    static_cast<unsigned long long>(programs), bytes / 1048576.0,
                    std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() -
                                                              started)
                        .count());
    }};
}

// gow3 (Gow3Toggle::SharedBinaries): files written before shared storage (or with the bit off)
// are rewritten once, on a lowest-priority thread, so the folder shrinks without a cache loss.
struct PipelineCache::BinaryConversion {
    std::jthread thread;
};

void PipelineCache::StartBinaryConversion() {
    if (Gow3Toggle::Disabled(Gow3Toggle::SharedBinaries) ||
        !instance.GetPipelineBinaryGlobalKey().keySize ||
        !EmulatorSettings.IsPipelineCacheEnabled()) {
        return;
    }
    const auto directory = Common::FS::GetUserPath(Common::FS::PathType::CacheDir) /
                           Common::ElfInfo::Instance().GameSerial() / "pipeline-binaries";
    binary_conversion = new BinaryConversion;
    binary_conversion->thread = std::jthread{[directory](std::stop_token stop) {
        Common::SetCurrentThreadName("gow3:BinaryConvert");
        Common::SetCurrentThreadPriority(Common::ThreadPriority::Low);
        const auto started = std::chrono::steady_clock::now();
        const auto [files, freed] = PipelineBinaryCache::ConvertToShared(
            directory, [&stop] { return stop.stop_requested(); });
        if (files) {
            std::printf("Pipeline binaries: %llu files moved to shared storage, %.0f MB freed "
                        "in %.1f s\n",
                        static_cast<unsigned long long>(files), freed / 1048576.0,
                        std::chrono::duration<double>(std::chrono::steady_clock::now() - started)
                            .count());
        }
    }};
}

void PipelineCache::StopBinaryConversion() {
    delete binary_conversion; // the jthread member asks the thread to stop and joins it
    binary_conversion = nullptr;
}

void PipelineCache::StopDiskPrefetch() {
    delete disk_prefetch; // the jthread member asks the thread to stop and joins it
    disk_prefetch = nullptr;
}

// gow3: GPU thread, once per program: every saved permutation at its saved index, so names and
// pipeline keys stay those of the store. Rejected metas (other versions, motion vertex
// shaders) leave their slot empty for the next compile (NewPermutationIndex).
bool PipelineCache::LoadDiskProgram(u64 hash) {
    const auto found = disk_permutations.find(hash);
    if (found == disk_permutations.end()) {
        return false;
    }
    const auto started = std::chrono::steady_clock::now();
    std::optional<std::vector<DiskBlob>> blobs;
    if (disk_prefetch) {
        std::scoped_lock lock{disk_prefetch->mutex};
        if (const auto it = disk_prefetch->ready.find(hash); it != disk_prefetch->ready.end()) {
            blobs = std::move(it->second);
            disk_prefetch->ready.erase(it);
        } else {
            disk_prefetch->claimed.insert(hash);
        }
    }
    if (!blobs) {
        blobs = ReadDiskProgram(hash, found->second);
    }
    auto program = std::make_unique<Program>();
    bool have_info = false;
    size_t inserted = 0;
    for (auto& blob : *blobs) {
        Serialization::Archive meta_ar{std::move(blob.meta)};
        Shader::Info info{};
        Shader::StageSpecialization spec{};
        std::optional<Shader::Gcn::FetchShaderData> fetch;
        size_t perm_idx{};
        if (!LoadShaderMeta(meta_ar, info, fetch, spec, perm_idx) || perm_idx != blob.idx ||
            info.pgm_hash != hash) {
            continue;
        }
        const auto module = CompileSPV(blob.spv, instance.GetDevice());
        if (!module) {
            continue;
        }
        if (!have_info) {
            program->info = std::move(info);
            have_info = true;
        }
        spec.info = &program->info;
        program->InsertPermut(module, std::move(spec), blob.idx);
        disk_modules_unused.insert(module);
        ++inserted;
        ++disk_stats.permutations;
    }
    if (inserted) {
        program->modules.resize(std::max<size_t>(program->modules.size(), found->second.back() + 1));
        program->info_template = std::make_unique<Shader::Info>(program->info);
        std::unique_lock lock{programs_mutex};
        program_cache.emplace(hash, std::move(program));
        ++disk_stats.programs;
    } else {
        for (const auto& module : program->modules) {
            if (module.module) {
                instance.GetDevice().destroyShaderModule(module.module);
            }
        }
    }
    disk_permutations.erase(found);
    disk_stats.load_ns += u64(std::chrono::duration_cast<std::chrono::nanoseconds>(
                                  std::chrono::steady_clock::now() - started)
                                  .count());
    return inserted != 0;
}

size_t PipelineCache::NewPermutationIndex(const Program& program) const {
    if (DiskShadersActive()) {
        const auto hole = std::ranges::find(program.modules, vk::ShaderModule{}, &Program::Module::module);
        if (hole != program.modules.end()) {
            return std::distance(program.modules.begin(), hole);
        }
    }
    return program.modules.size();
}

bool PipelineCache::NewPipelineKey(const std::string& name) {
    if (!DiskShadersActive()) {
        return true;
    }
    if (!disk_pipeline_keys.insert(name).second) {
        ++disk_stats.keys_skipped;
        return false;
    }
    ++disk_stats.keys_written;
    return true;
}

void PipelineCache::ReportDiskShaders() {
    if (disk_shaders && !disk_store_untrusted && Gow3Toggle::Disabled(Gow3Toggle::DiskShaders)) {
        // Switched off live: later permutations follow first-use order again (see StartOnDemand).
        Storage::DataBase::Instance().Save(Storage::BlobType::ShaderProfile, "disk_shaders",
                                           std::vector<u8>{0});
        disk_store_untrusted = true;
    }
    const auto now = std::chrono::steady_clock::now();
    if (!disk_shaders || now - disk_report < std::chrono::seconds(5)) {
        return;
    }
    disk_report = now;
    const auto& s = disk_stats;
    if (std::memcmp(&s, &disk_reported, sizeof(s)) == 0) {
        return;
    }
    disk_reported = s;
    std::printf("Disk shaders: %llu programs, %llu permutations read (%.1f ms GPU thread); "
                "%llu used, %llu compiled; pipeline keys %llu written, %llu already saved\n",
                static_cast<unsigned long long>(s.programs),
                static_cast<unsigned long long>(s.permutations), s.load_ns / 1e6,
                static_cast<unsigned long long>(s.matched),
                static_cast<unsigned long long>(s.compiled),
                static_cast<unsigned long long>(s.keys_written),
                static_cast<unsigned long long>(s.keys_skipped));
}

// gow3: one cached shader stage read by the background warm-up, shared by every cached
// pipeline that uses it. A pool thread fills it; the GPU thread merges it into program_cache
// once (MergeWarmStage).
struct PipelineCache::WarmStage {
    vk::Device device;
    std::unique_ptr<Program> program;
    Shader::StageSpecialization spec{};
    std::optional<Shader::Gcn::FetchShaderData> fetch;
    size_t perm_idx{};
    vk::ShaderModule module{};
    // GPU thread, from the merge on.
    bool merged = false;
    bool module_kept = false;           ///< the module now belongs to a Program
    const Shader::Info* info = nullptr; ///< null: not usable (read failed or conflict)
    vk::ShaderModule result{};

    ~WarmStage() {
        if (module && !module_kept) {
            device.destroyShaderModule(module);
        }
    }
};

// gow3: StartBackgroundWarmUp. The pool reads the store and builds the pipelines at low
// priority; the GPU thread merges the stages and publishes the pipelines a few at a time.
struct PipelineCache::BackgroundWarmup {
    struct Entry {
        std::chrono::steady_clock::time_point queued;
        bool compute = false;
        GraphicsPipelineKey graphics_key{};
        ComputePipelineKey compute_key{};
        GraphicsPipeline::SerializationSupport sdata{};
        std::array<std::shared_ptr<WarmStage>, MaxShaderStages> stages{};
    };
    struct Built {
        GraphicsPipelineKey graphics_key{};
        ComputePipelineKey compute_key{};
        std::unique_ptr<GraphicsPipeline> graphics;
        std::unique_ptr<ComputePipeline> compute;
    };

    std::unordered_map<u64, std::shared_ptr<WarmStage>> stages; ///< read job only
    WarmupInbox<Entry> read;
    WarmupInbox<Built> built;
    std::atomic<u32> total{0}, done{0};
    std::atomic<bool> reading{true}, stopping{false};
    std::atomic<u64> reader_ns{0}, entry_ns{0}, shader_io_ns{0}, module_ns{0};
    std::atomic<u64> read_queue_ns{0}, build_queue_ns{0}, build_ns{0};
    std::atomic<u32> entries{0}, modules{0}, submitted{0}, completed{0};
    u64 pump_ns = 0, pump_max_ns = 0, pump_gap_max_ns = 0;
    u32 batches = 0;
    bool finished = false;
    u32 loaded = 0, threads = 0;
    std::chrono::steady_clock::time_point started, last_batch;
    std::chrono::steady_clock::time_point last_pump{}, last_report{};
    std::unique_ptr<WarmupPool> pool; ///< last: its jobs use the members above
};

bool PipelineCache::StartBackgroundWarmUp() {
    if (!Storage::DataBase::Instance().ConcurrentReads()) {
        return false;
    }
    if (!CheckCacheProfile()) {
        return true; // nothing cached yet
    }
    background = new BackgroundWarmup;
    auto& warm = *background;
    warm.started = std::chrono::steady_clock::now();
    warm.threads = WarmupThreadCount(std::max(1u, std::thread::hardware_concurrency()));
    warm.pool = std::make_unique<WarmupPool>(warm.threads, true);
    // The first job reads the whole store; with one thread the builds queue behind it.
    warm.pool->Push([this, &warm] {
        const auto reader_started = std::chrono::steady_clock::now();
        auto& db = Storage::DataBase::Instance();
        warm.total = u32(db.CountBlobs(Storage::BlobType::PipelineKey));
        db.ForEachBlob(
            Storage::BlobType::PipelineKey,
            [&](std::vector<u8>&& data) { ReadWarmEntry(warm, std::move(data)); },
            [&] { return warm.stopping.load(std::memory_order_relaxed); });
        warm.stages.clear();
        warm.reader_ns = ElapsedNs(reader_started);
        warm.reading = false;
        return true;
    });
    std::printf("Pipeline warm-up: loading in the background (%u threads)\n", warm.threads);
    return true;
}

void PipelineCache::ReadWarmEntry(BackgroundWarmup& warm, std::vector<u8>&& data) {
    const auto entry_started = std::chrono::steady_clock::now();
    SCOPE_EXIT { warm.entry_ns += ElapsedNs(entry_started); ++warm.entries; };
    const auto skip = [&] { warm.done.fetch_add(1, std::memory_order_relaxed); };
    Serialization::Archive ar{std::move(data)};
    Serialization::Reader pldata{ar};
    u32 version{};
    pldata.Read(version);
    if (version != Serialization::PipelineKeyVersion) {
        return skip();
    }
    u32 is_compute{};
    pldata.Read(is_compute);

    BackgroundWarmup::Entry entry{};
    entry.compute = is_compute != 0;
    std::array<u64, MaxShaderStages> hashes{};
    if (entry.compute) {
        entry.compute_key.Deserialize(ar);
        ComputePipeline::SerializationSupport sdata{};
        sdata.Deserialize(ar);
        hashes[0] = entry.compute_key.value;
    } else {
        entry.graphics_key.Deserialize(ar);
        entry.sdata.Deserialize(ar);
        for (u32 i = 0; i < MaxShaderStages; ++i) {
            hashes[i] = entry.graphics_key.stage_hashes[i];
        }
    }

    auto& db = Storage::DataBase::Instance();
    for (u32 i = 0; i < MaxShaderStages; ++i) {
        if (!hashes[i]) {
            continue;
        }
        auto& stage = warm.stages[hashes[i]];
        if (!stage) {
            stage = std::make_shared<WarmStage>();
            stage->device = instance.GetDevice();
            stage->program = std::make_unique<Program>();
            stage->spec.info = &stage->program->info;
            const auto io_started = std::chrono::steady_clock::now();
            u64 module_time = 0;
            std::vector<u8> meta_blob;
            db.Load(Storage::BlobType::ShaderMeta, fmt::format("{:#018x}", hashes[i]), meta_blob);
            if (!meta_blob.empty()) {
                Serialization::Archive meta_ar{std::move(meta_blob)};
                if (LoadShaderMeta(meta_ar, stage->program->info, stage->fetch, stage->spec,
                                   stage->perm_idx)) {
                    std::vector<u32> spv;
                    db.Load(Storage::BlobType::ShaderBinary,
                            fmt::format("{:#018x}_{}", stage->program->info.pgm_hash,
                                        stage->perm_idx),
                            spv);
                    if (!spv.empty()) {
                        const auto module_started = std::chrono::steady_clock::now();
                        stage->module = CompileSPV(spv, stage->device);
                        module_time = ElapsedNs(module_started);
                        warm.module_ns += module_time;
                        ++warm.modules;
                    }
                }
            }
            warm.shader_io_ns += ElapsedNs(io_started) - module_time;
            if (!stage->module) {
                stage->merged = true; // unusable: info stays null
            }
        }
        entry.stages[i] = stage;
    }
    entry.queued = std::chrono::steady_clock::now();
    warm.read.Push(std::move(entry));
}

bool PipelineCache::MergeWarmStage(WarmStage& stage) {
    stage.merged = true;
    const u64 hash = stage.program->info.pgm_hash;
    const auto it_pgm = program_cache.find(hash);
    if (it_pgm == program_cache.end()) {
        Program* program = stage.program.get();
        program->InsertPermut(stage.module, std::move(stage.spec), stage.perm_idx);
        program_cache.emplace(hash, std::move(stage.program));
        stage.module_kept = true;
        stage.result = stage.module;
        stage.info = &program->info;
        return true;
    }
    // The game already uses this shader: its permutations are the ones that count.
    Program& program = *it_pgm->second;
    if (program.pending) {
        return false; // the game is compiling a permutation that takes the next index
    }
    const auto it = std::ranges::find(program.modules, stage.spec, &Program::Module::spec);
    if (it != program.modules.end()) {
        if (size_t(std::distance(program.modules.begin(), it)) != stage.perm_idx) {
            return false;
        }
        stage.result = it->module;
    } else {
        if (stage.perm_idx < program.modules.size() && program.modules[stage.perm_idx].module) {
            return false;
        }
        stage.spec.info = &program.info;
        program.InsertPermut(stage.module, std::move(stage.spec), stage.perm_idx);
        stage.module_kept = true;
        stage.result = stage.module;
    }
    stage.info = &program.info;
    return true;
}

void PipelineCache::PumpWarmUp() {
    if (!background || background->finished) {
        return;
    }
    auto& warm = *background;
    using namespace std::chrono_literals;
    const auto pump_started = std::chrono::steady_clock::now();
    if (warm.last_pump.time_since_epoch().count()) {
        warm.pump_gap_max_ns = std::max(warm.pump_gap_max_ns,
            u64(std::chrono::duration_cast<std::chrono::nanoseconds>(
                    pump_started - warm.last_pump).count()));
    }
    warm.last_pump = pump_started;
    SCOPE_EXIT {
        const auto ns = ElapsedNs(pump_started);
        warm.pump_ns += ns;
        warm.pump_max_ns = std::max(warm.pump_max_ns, ns);
    };

    // Built pipelines: the first one ready for a key wins (the game may have compiled it).
    for (auto& built : warm.built.Take(64)) {
        const bool kept =
            built.graphics
                ? PublishFirst(graphics_pipelines, built.graphics_key, std::move(built.graphics))
                : PublishFirst(compute_pipelines, built.compute_key, std::move(built.compute));
        warm.loaded += kept;
        warm.done.fetch_add(1, std::memory_order_relaxed);
    }

    const auto now = std::chrono::steady_clock::now();
    if (now - warm.last_batch < 4ms) {
        return;
    }
    warm.last_batch = now;
    ++warm.batches;
    const u32 done = warm.done + warm.pool->failed;
    Gow3Overlay::SetBackgroundLoading(done, std::max(warm.total.load(), done));

    static const std::array<Shader::RuntimeInfo, MaxShaderStages> no_runtime_infos{};
    for (auto& entry : warm.read.Take(32)) {
        warm.read_queue_ns += ElapsedNs(entry.queued);
        if (entry.compute ? compute_pipelines.contains(entry.compute_key)
                          : graphics_pipelines.contains(entry.graphics_key)) {
            warm.done.fetch_add(1, std::memory_order_relaxed); // the game compiled it first
            continue;
        }
        std::array<const Shader::Info*, MaxShaderStages> infos{};
        std::array<vk::ShaderModule, MaxShaderStages> modules{};
        std::optional<Shader::Gcn::FetchShaderData> fetch;
        bool usable = true;
        {
            std::unique_lock lock{programs_mutex};
            for (u32 i = 0; i < MaxShaderStages && usable; ++i) {
                if (!entry.stages[i]) {
                    continue;
                }
                auto& stage = *entry.stages[i];
                if (!stage.merged) {
                    MergeWarmStage(stage);
                }
                usable = stage.info != nullptr;
                infos[i] = stage.info;
                modules[i] = stage.result;
                fetch = stage.fetch; // the blocking load keeps the last stage's too
            }
        }
        if (!usable) {
            warm.done.fetch_add(1, std::memory_order_relaxed);
            continue;
        }
        ++warm.submitted;
        warm.pool->Push([this, &warm, queued = std::chrono::steady_clock::now(),
                          compute = entry.compute, graphics_key = entry.graphics_key,
                         compute_key = entry.compute_key, sdata = entry.sdata, infos, modules,
                         fetch]() mutable {
            const auto build_started = std::chrono::steady_clock::now();
            warm.build_queue_ns += ElapsedNs(queued);
            SCOPE_EXIT { warm.build_ns += ElapsedNs(build_started); ++warm.completed; };
            BackgroundWarmup::Built built{graphics_key, compute_key};
            if (compute) {
                ComputePipeline::SerializationSupport compute_data{};
                built.compute = std::make_unique<ComputePipeline>(
                    instance, scheduler, desc_heap, profile, *pipeline_cache, compute_key,
                    *infos[0], modules[0], compute_data, true);
            } else {
                built.graphics = std::make_unique<GraphicsPipeline>(
                    instance, scheduler, desc_heap, profile, graphics_key, *pipeline_cache, infos,
                    no_runtime_infos, fetch, modules, sdata, true);
            }
            warm.built.Push(std::move(built));
            return true;
        });
    }

    if (now - warm.last_report >= 1s) {
        warm.last_report = now;
        std::printf("Warm-up progress: %.2f s, read %u, submitted %u, completed %u, done %u/%u, "
                    "batches %u, reading %u\n",
                    std::chrono::duration<double>(now - warm.started).count(),
                    warm.entries.load(), warm.submitted.load(), warm.completed.load(),
                    warm.done.load(), warm.total.load(), warm.batches, unsigned(warm.reading.load()));
    }

    if (warm.reading || !warm.read.Empty() || !warm.pool->Idle() || !warm.built.Empty()) {
        return;
    }
    std::printf("Pipeline warm-up: %u of %u pipelines in %.2f s in the background (%u threads)\n",
                warm.loaded, warm.total.load(),
                std::chrono::duration<double>(now - warm.started).count(), warm.threads);
    LOG_INFO(Render, "Preloaded {} pipelines in the background", warm.loaded);
    ReportPipelineBinaryCache();
    std::printf("Warm-up profile: reader wall %.3f s, entries %.3f s, shader IO/meta %.3f s, "
                "modules %.3f s (%u); GPU pump %.3f s, max %.3f ms, gap max %.3f ms, "
                "batches %u; builds %.3f thread-s, read queue %.3f item-s, "
                "build queue %.3f item-s\n",
                warm.reader_ns.load() / 1e9, warm.entry_ns.load() / 1e9,
                warm.shader_io_ns.load() / 1e9, warm.module_ns.load() / 1e9, warm.modules.load(),
                warm.pump_ns / 1e9, warm.pump_max_ns / 1e6, warm.pump_gap_max_ns / 1e6,
                warm.batches, warm.build_ns.load() / 1e9, warm.read_queue_ns.load() / 1e9,
                warm.build_queue_ns.load() / 1e9);
    std::printf("Pipeline binary profile: keys %.3f thread-s, disk/decode/migration %.3f thread-s, "
                "binary handles %.3f thread-s, driver restore %.3f thread-s, "
                "driver compile %.3f thread-s\n",
                binary_key_ns.load() / 1e9, binary_load_ns.load() / 1e9,
                binary_handles_ns.load() / 1e9, binary_restore_ns.load() / 1e9,
                binary_compile_ns.load() / 1e9);
    extern std::atomic<u64> g_gow3_pipeline_total_ns, g_gow3_pipeline_driver_ns;
    extern std::atomic<u32> g_gow3_pipelines_built;
    const u32 built = std::max(1u, g_gow3_pipelines_built.load());
    std::printf("Pipeline build time: %u graphics pipelines, %.1f ms each (driver %.1f ms, the rest "
                "%.1f ms), %.1f s in all\n",
                built, g_gow3_pipeline_total_ns / 1e6 / built, g_gow3_pipeline_driver_ns / 1e6 / built,
                (g_gow3_pipeline_total_ns - g_gow3_pipeline_driver_ns) / 1e6 / built,
                g_gow3_pipeline_total_ns / 1e9);
    // The idle threads stay parked (StopWarmUp).
    warm.finished = true;
    Gow3Overlay::SetBackgroundLoading(0, 0);
    Storage::DataBase::Instance().FinishPreload();
}

void PipelineCache::StopWarmUp() {
    if (!background) {
        return;
    }
    background->stopping = true;
    background->pool->Drain();
    // ponytail: leaked with its parked threads (this runs at exit only): a thread that ended
    // while the game ran crashed in ntdll (RtlWakeAllConditionVariable). Join them once the
    // cause is known.
    background = nullptr;
    Gow3Overlay::SetBackgroundLoading(0, 0);
}

void PipelineCache::Sync() {
    StopWarmUp();
    FinishCompilations();
    Storage::DataBase::Instance().Close();
}

} // namespace Vulkan

namespace Shader {

void Info::Serialize(Serialization::Archive& ar) const {
    Serialization::Writer info{ar};

    info.Write(this, sizeof(InfoPersistent));
    info.Write(flattened_ud_buf);
    srt_info.Serialize(ar);
}

bool Info::Deserialize(Serialization::Archive& ar) {
    Serialization::Reader info{ar};

    info.Read(this, sizeof(Shader::InfoPersistent));
    info.Read(flattened_ud_buf);

    return srt_info.Deserialize(ar);
}

void Gcn::FetchShaderData::Serialize(Serialization::Archive& ar) const {
    Serialization::Writer fetch{ar};
    ar.Grow(6 + attributes.size() * sizeof(VertexAttribute));

    fetch.Write(size);
    fetch.Write(vertex_offset_sgpr);
    fetch.Write(instance_offset_sgpr);
    fetch.Write(attributes);
}

bool Gcn::FetchShaderData::Deserialize(Serialization::Archive& ar) {
    Serialization::Reader fetch{ar};

    fetch.Read(size);
    fetch.Read(vertex_offset_sgpr);
    fetch.Read(instance_offset_sgpr);
    fetch.Read(attributes);

    return true;
}

void PersistentSrtInfo::Serialize(Serialization::Archive& ar) const {
    Serialization::Writer srt{ar};

    srt.Write(this, sizeof(*this));
    if (walker_func_size) {
        srt.Write(reinterpret_cast<void*>(walker_func), walker_func_size);
    }
}

bool PersistentSrtInfo::Deserialize(Serialization::Archive& ar) {
    Serialization::Reader srt{ar};

    srt.Read(this, sizeof(*this));

    if (walker_func_size) {
        walker_func = RegisterWalkerCode(ar.CurrPtr(), walker_func_size);
        ar.Advance(walker_func_size);
    }

    return true;
}

void StageSpecialization::Serialize(Serialization::Archive& ar) const {
    Serialization::Writer spec{ar};

    spec.Write(start);
    spec.Write(runtime_info);

    spec.Write(bitset.to_string());

    if (fetch_shader_data) {
        spec.Write(sizeof(*fetch_shader_data));
        fetch_shader_data->Serialize(ar);
    } else {
        spec.Write(size_t{0});
    }

    spec.Write(vs_attribs);
    spec.Write(buffers);
    spec.Write(images);
    spec.Write(fmasks);
    spec.Write(samplers);
}

bool StageSpecialization::Deserialize(Serialization::Archive& ar) {
    Serialization::Reader spec{ar};

    spec.Read(start);
    spec.Read(runtime_info);

    std::string bits{};
    spec.Read(bits);
    bitset = std::bitset<MaxStageResources>(bits);

    u64 fetch_data_size{};
    spec.Read(fetch_data_size);

    if (fetch_data_size) {
        Gcn::FetchShaderData fetch_data;
        fetch_data.Deserialize(ar);
        fetch_shader_data = fetch_data;
    }

    spec.Read(vs_attribs);
    spec.Read(buffers);
    spec.Read(images);
    spec.Read(fmasks);
    spec.Read(samplers);

    return true;
}

} // namespace Shader
