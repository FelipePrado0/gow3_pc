// SPDX-FileCopyrightText: Copyright 2025-2026 shadPS4 Emulator Project
// SPDX-License-Identifier: GPL-2.0-or-later

#include <algorithm>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdlib>
#include <deque>
#include <mutex>
#include <shared_mutex>
#include <thread>
#include <unordered_map>
#include "common/serdes.h"
#include "common/thread.h"
#include "gow3_overlay.h"
#include "core/emulator_settings.h"
#include "shader_recompiler/frontend/fetch_shader.h"
#include "shader_recompiler/info.h"
#include "video_core/cache_storage.h"
#include "video_core/renderer_vulkan/vk_instance.h"
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
    meta.Write(Serialization::ShaderBinaryVersion);

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
    if (binary_version != Serialization::ShaderBinaryVersion) {
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
    if (num_total_pipelines > num_pipelines) {
        LOG_WARNING(Render, "{} stale pipelines were found. Consider re-generating the cache",
                    num_total_pipelines - num_pipelines);
    }

    Storage::DataBase::Instance().FinishPreload();
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
    bool finished = false;
    u32 loaded = 0, threads = 0;
    std::chrono::steady_clock::time_point started, last_batch;
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
        auto& db = Storage::DataBase::Instance();
        warm.total = u32(db.CountBlobs(Storage::BlobType::PipelineKey));
        db.ForEachBlob(
            Storage::BlobType::PipelineKey,
            [&](std::vector<u8>&& data) { ReadWarmEntry(warm, std::move(data)); },
            [&] { return warm.stopping.load(std::memory_order_relaxed); });
        warm.stages.clear();
        warm.reading = false;
        return true;
    });
    std::printf("Pipeline warm-up: loading in the background (%u threads)\n", warm.threads);
    return true;
}

void PipelineCache::ReadWarmEntry(BackgroundWarmup& warm, std::vector<u8>&& data) {
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
                        stage->module = CompileSPV(spv, stage->device);
                    }
                }
            }
            if (!stage->module) {
                stage->merged = true; // unusable: info stays null
            }
        }
        entry.stages[i] = stage;
    }
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
    const u32 done = warm.done + warm.pool->failed;
    Gow3Overlay::SetBackgroundLoading(done, std::max(warm.total.load(), done));

    static const std::array<Shader::RuntimeInfo, MaxShaderStages> no_runtime_infos{};
    for (auto& entry : warm.read.Take(32)) {
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
        warm.pool->Push([this, &warm, compute = entry.compute, graphics_key = entry.graphics_key,
                         compute_key = entry.compute_key, sdata = entry.sdata, infos, modules,
                         fetch]() mutable {
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

    if (warm.reading || !warm.read.Empty() || !warm.pool->Idle() || !warm.built.Empty()) {
        return;
    }
    std::printf("Pipeline warm-up: %u of %u pipelines in %.2f s in the background (%u threads)\n",
                warm.loaded, warm.total.load(),
                std::chrono::duration<double>(now - warm.started).count(), warm.threads);
    LOG_INFO(Render, "Preloaded {} pipelines in the background", warm.loaded);
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
