// SPDX-FileCopyrightText: Copyright 2024-2026 shadPS4 Emulator Project
// SPDX-License-Identifier: GPL-2.0-or-later

#include <filesystem>
#include <xxhash.h>
#include "video_core/amdgpu/liverpool.h"

#include "gow3_toggles.h"
#include "common/assert.h"
#include "common/debug.h"
#include "common/div_ceil.h"
#include "common/hash.h"
#include "common/scope_exit.h"
#include "core/emulator_settings.h"
#include "core/memory.h"
#include "video_core/buffer_cache/buffer_cache.h"
#include "video_core/page_manager.h"
#include "video_core/renderer_vulkan/vk_instance.h"
#include "video_core/renderer_vulkan/vk_runtime.h"
#include "video_core/renderer_vulkan/vk_scheduler.h"
#include "video_core/renderer_vulkan/vk_wait_diagnostics.h"
#include "video_core/texture_cache/aliasing.h"
#include "video_core/texture_cache/host_compatibility.h"
#include "video_core/texture_cache/texture_cache.h"
#include <vulkan/vulkan_format_traits.hpp>
#include "video_core/texture_cache/tile_manager.h"

namespace VideoCore {

static constexpr u64 PageShift = 12;
static constexpr u64 NumFramesBeforeRemoval = 32;

TextureCache::TextureCache(const Vulkan::Instance& instance_, Vulkan::Scheduler& scheduler_,
                           Vulkan::Runtime& runtime_, AmdGpu::Liverpool* liverpool_,
                           BufferCache& buffer_cache_, PageManager& tracker_)
    : instance{instance_}, scheduler{scheduler_}, runtime{runtime_}, liverpool{liverpool_},
      buffer_cache{buffer_cache_}, tracker{tracker_}, blit_helper{instance, scheduler},
      tile_manager{instance, scheduler, runtime, buffer_cache.GetStreamBuffer()},
      readback_linear_images{EmulatorSettings.IsReadbackLinearImagesEnabled()} {

    std::printf("GPU: deferred image readback %s (64 copies, 64 MiB pending limit)%s\n",
                deferred_readbacks ? "enabled" : "disabled",
                stale_readbacks ? "; CPU reads use the previous frame's copy" : "");

    u32 max_samplers = instance.GetMaxSamplerAllocationCount();
    trigger_gc_samplers = max_samplers * 3 / 4;
    pressure_gc_samplers = max_samplers * 7 / 8;
    critical_gc_samplers = max_samplers * 15 / 16;

    // Set up garbage collection parameters.
    if (!instance.CanReportMemoryUsage()) {
        trigger_gc_memory = 0;
        pressure_gc_memory = DEFAULT_PRESSURE_GC_MEMORY;
        critical_gc_memory = DEFAULT_CRITICAL_GC_MEMORY;
        return;
    }

    const s64 device_local_memory = static_cast<s64>(instance.GetTotalMemoryBudget());
    const s64 min_spacing_expected = device_local_memory - 1_GB;
    const s64 min_spacing_critical = device_local_memory - 512_MB;
    const s64 mem_threshold = std::min<s64>(device_local_memory, TARGET_GC_THRESHOLD);
    const s64 min_vacancy_expected = (6 * mem_threshold) / 10;
    const s64 min_vacancy_critical = (2 * mem_threshold) / 10;
    pressure_gc_memory = static_cast<u64>(
        std::max<u64>(std::min(device_local_memory - min_vacancy_expected, min_spacing_expected),
                      DEFAULT_PRESSURE_GC_MEMORY));
    critical_gc_memory = static_cast<u64>(
        std::max<u64>(std::min(device_local_memory - min_vacancy_critical, min_spacing_critical),
                      DEFAULT_CRITICAL_GC_MEMORY));
    trigger_gc_memory = static_cast<u64>((device_local_memory - mem_threshold) / 2);
}

TextureCache::~TextureCache() {
    if (deferred_readbacks) {
        scheduler.Finish();
        std::scoped_lock lock{readback_mutex};
        RetireReadbacks();
    }
}

template <bool track>
void TextureCache::WatchReadback(VAddr address, u64 size) {
    if (stale_readbacks) {
        tracker.UpdatePageWatchers<track, false>(address, size);
    } else {
        tracker.UpdatePageWatchers<track, true>(address, size);
    }
}

void TextureCache::RetireReadbacks() {
    pending_readbacks.Retire([this](u64 tick) { return scheduler.IsFree(tick); },
        [this](auto& entry) {
            if (entry.valid) {
                entry.payload.Invalidate();
                Core::Memory::Instance()->TryWriteBacking(std::bit_cast<u8*>(entry.address),
                    entry.payload.mapped, entry.size);
                if (entry.watched) WatchReadback<false>(entry.address, entry.size);
                ++readbacks_completed;
            }
            runtime.GetStagingPool().FreeDeferred(entry.payload);
        });
    has_pending_readbacks.store(pending_readbacks.Bytes() != 0, std::memory_order_release);
}

bool TextureCache::ResolveReadbacks(VAddr address, u64 size, bool assume_locks,
                                    ReadbackSource source) {
    if (!deferred_readbacks || !has_pending_readbacks.load(std::memory_order_acquire)) return false;
    // gow3: image uploads read, and frees drop, exactly the image's bytes. A copy of a
    // neighboring image that only shares a page is published later over other bytes. Only with
    // stale readbacks: otherwise pending pages are read-protected and the copy threads fault.
    const bool exact = ExactReadbackWaits() &&
        (source == ReadbackSource::ImageRefresh || source == ReadbackSource::BufferForImage ||
         source == ReadbackSource::ImageFree);
    {
        std::scoped_lock lock{readback_mutex};
        // Stale mode, byte rule: God of War III writes every frame to the page that holds a
        // 1x1 exposure copy, but not to its bytes. That write need not wait for the GPU: the
        // copy only stops protecting its page and is still published when the GPU is done.
        if (stale_readbacks && source == ReadbackSource::CpuWrite) {
            readbacks_released += pending_readbacks.ReleaseOutside(address, size, 64_KB,
                [this](auto& entry) { WatchReadback<false>(entry.address, entry.size); });
        }
        if (!pending_readbacks.RequiredTick(address, size, exact)) {
            if (exact && pending_readbacks.RequiredTick(address, size)) ++readbacks_neighbors;
            return false;
        }
    }
    const auto resolve = [this, address, size, source, exact] {
        const bool diag = Vulkan::WaitDiagnostics::Enabled();
        std::scoped_lock lock{readback_mutex};
        bool waited = false;
        u64 wait_ns = 0;
        if (diag && (source == ReadbackSource::CpuRead || source == ReadbackSource::CpuWrite)) {
            // Which copied image the CPU touched while its copy was pending.
            for (auto& image : readback_images) {
                if (image.copies && address < image.address + image.Bytes() && image.address < address + size) {
                    ++image.cpu_accesses;
                }
            }
        }
        Gow3Stats::Timer readback_timer{Gow3Stats::t_readback_wait};
        if (const u64 tick = pending_readbacks.RequiredTick(address, size, exact)) {
            ++readbacks_waited;
            waited = !scheduler.IsFree(tick);
            const auto start = diag ? std::chrono::steady_clock::now()
                                    : std::chrono::steady_clock::time_point{};
            scheduler.Wait(tick);
            if (diag) {
                wait_ns = std::chrono::duration_cast<std::chrono::nanoseconds>(
                              std::chrono::steady_clock::now() - start).count();
            }
            RetireReadbacks();
        }
        if (diag) readback_sources.Record(source, waited, wait_ns, size);
    };
    if (assume_locks) resolve();
    else liverpool->SendCommand<true>(resolve);
    return true;
}

void TextureCache::CancelReadbacks(VAddr address, u64 size, bool exact) {
    if (!deferred_readbacks) return;
    std::scoped_lock lock{readback_mutex};
    pending_readbacks.Cancel(address, size, [this](auto& entry) {
        if (entry.watched) WatchReadback<false>(entry.address, entry.size);
        ++readbacks_canceled;
    }, exact);
}

// gow3 (GOW3_PERF_DIAG): a request PartialHeightViews answered with a taller cached image.
void TextureCache::NotePartialView(const ImageInfo& request, const ImageInfo& used,
                                   BindingType binding) {
    ++partial_views;
    const auto same = [&](const PartialViewStat& s) {
        return s.address == request.guest_address && s.format == request.pixel_format &&
               s.used_format == used.pixel_format && s.used_height == used.size.height &&
               s.binding == binding;
    };
    auto slot = std::ranges::find_if(partial_view_stats, same);
    if (slot == partial_view_stats.end()) {
        slot = std::ranges::min_element(partial_view_stats, {}, &PartialViewStat::count);
        *slot = {request.guest_address, request.pixel_format, used.pixel_format, used.size.width,
                 used.size.height, request.size.height, request.size.height, 0, binding};
    }
    slot->min_height = std::min(slot->min_height, request.size.height);
    slot->max_height = std::max(slot->max_height, request.size.height);
    ++slot->count;
}

// gow3 (GOW3_PERF_DIAG): every 5 s, the images FindImage created most often. Takes `mutex`
// (held by FindImage), so it runs before ProcessDownloadImages takes readback_mutex.
void TextureCache::ReportImageCreates() {
    const auto now = std::chrono::steady_clock::now();
    if (now - create_report < std::chrono::seconds(5)) {
        return;
    }
    create_report = now;
    std::scoped_lock lock{mutex};
    static constexpr const char* bindings[] = {"texture", "storage", "render target",
                                               "depth target", "video out"};
    std::printf("Image creates: %llu in 5 s\n", static_cast<unsigned long long>(images_created));
    std::ranges::sort(create_stats, std::greater{}, &CreateStat::count);
    for (u32 i = 0; i < 8 && create_stats[i].count; ++i) {
        const auto& s = create_stats[i];
        std::printf("  %u x %#llx %ux%u %s pitch %u tile %u as %s%s: %s", s.count,
                    static_cast<unsigned long long>(s.address), s.width, s.height,
                    vk::to_string(s.format).c_str(), s.pitch, s.tile, bindings[u32(s.binding)],
                    s.needs_native ? "" : " (sampled)", s.reason);
        if (s.overlaps) {
            std::printf(" (%u cached; was %#llx %ux%u %s pitch %u tile %u)", s.overlaps,
                        static_cast<unsigned long long>(s.prev_address), s.prev_width,
                        s.prev_height, vk::to_string(s.prev_format).c_str(), s.prev_pitch,
                        s.prev_tile);
        }
        std::printf("\n");
    }
    create_stats = {};
    images_created = 0;
    std::printf("Partial height views: %llu in 5 s\n", static_cast<unsigned long long>(partial_views));
    std::ranges::sort(partial_view_stats, std::greater{}, &PartialViewStat::count);
    for (u32 i = 0; i < 8 && partial_view_stats[i].count; ++i) {
        const auto& s = partial_view_stats[i];
        std::printf("  %u x %#llx %s rows %u-%u as %s, on %ux%u %s\n", s.count,
                    static_cast<unsigned long long>(s.address), vk::to_string(s.format).c_str(),
                    s.min_height, s.max_height, bindings[u32(s.binding)], s.used_width,
                    s.used_height, vk::to_string(s.used_format).c_str());
    }
    partial_view_stats = {};
    partial_views = 0;
}

// gow3: every copy is recorded first and the GPU is waited for once. A wait per image (as
// before) left the GPU mostly idle in God of War III's gameplay, at about 2 FPS.
void TextureCache::ProcessDownloadImages() {
    if (Vulkan::WaitDiagnostics::Enabled()) {
        ReportImageCreates();
    }
    if (deferred_readbacks) {
        // Upload workers must finish reading RAM before these pages become inaccessible.
        scheduler.WaitHostCopies();
        std::scoped_lock readback_lock{readback_mutex};
        RetireReadbacks();
        std::unique_lock lock{download_images_mutex};
        for (const ImageId image_id : download_images) {
            const auto& image = slot_images[image_id];
            if (False(image.flags & ImageFlagBits::GpuModified)) continue;
            const u64 size = u64{image.info.pitch} * image.info.size.height *
                image.info.size.depth * image.info.resources.layers * (image.info.num_bits / 8);
            if (!pending_readbacks.CanFit(size)) {
                ++readbacks_waited;
                scheduler.Finish();
                RetireReadbacks();
            }
            if (auto copy = RecordImageDownload(image_id)) {
                if (!pending_readbacks.CanFit(copy->size)) {
                    scheduler.Finish();
                    copy->staging.Invalidate();
                    Core::Memory::Instance()->TryWriteBacking(
                        std::bit_cast<u8*>(copy->guest_address), copy->staging.mapped, copy->size);
                    runtime.GetStagingPool().FreeDeferred(copy->staging);
                } else {
                    WatchReadback<true>(copy->guest_address, copy->size);
                    if (Vulkan::WaitDiagnostics::Enabled()) {
                        // Which images go back to RAM each frame (the report lists the busiest).
                        auto* slot = std::ranges::find(readback_images, image.info.guest_address,
                                                       &ReadbackImageStat::address);
                        if (slot == readback_images.end()) {
                            slot = std::ranges::find(readback_images, VAddr{0}, &ReadbackImageStat::address);
                        }
                        if (slot != readback_images.end()) {
                            *slot = {image.info.guest_address, image.info.size.width,
                                     image.info.size.height, image.info.num_bits, slot->copies + 1,
                                     slot->cpu_accesses, image.info.pixel_format};
                        }
                    }
                    pending_readbacks.Push(copy->guest_address, copy->size,
                        scheduler.CurrentTick(), copy->staging);
                    has_pending_readbacks.store(true, std::memory_order_release);
                    ++readbacks_queued;
                    readbacks_peak_bytes = std::max(readbacks_peak_bytes, pending_readbacks.Bytes());
                }
            }
        }
        download_images.clear();
        if (Vulkan::WaitDiagnostics::Enabled()) {
            const auto now = std::chrono::steady_clock::now();
            if (now - readback_report >= std::chrono::seconds(5)) {
                std::printf("Readback profile: queued %llu, completed %llu, demand/budget waits %llu, canceled %llu, pending %.2f MiB, peak %.2f MiB\n",
                    static_cast<unsigned long long>(readbacks_queued),
                    static_cast<unsigned long long>(readbacks_completed),
                    static_cast<unsigned long long>(readbacks_waited),
                    static_cast<unsigned long long>(readbacks_canceled),
                    pending_readbacks.Bytes() / 1048576.0, readbacks_peak_bytes / 1048576.0);
                readbacks_queued = readbacks_completed = readbacks_waited = readbacks_canceled = 0;
                readbacks_peak_bytes = pending_readbacks.Bytes();
                readback_sources.Report(stdout);
                if (stale_readbacks) {
                    std::printf("Readback byte rule: %llu CPU writes went on without waiting\n",
                                static_cast<unsigned long long>(readbacks_released));
                    std::printf("Readback exact waits: %llu image uploads/frees skipped a copy "
                                "on a shared page\n",
                                static_cast<unsigned long long>(readbacks_neighbors));
                    readbacks_released = readbacks_neighbors = 0;
                }
                std::ranges::sort(readback_images, std::greater{}, &ReadbackImageStat::copies);
                for (u32 i = 0; i < 4 && readback_images[i].copies; ++i) {
                    const auto& r = readback_images[i];
                    std::printf("Readback image %#llx: %ux%u, %u bits, %s, %u copies, %u CPU accesses "
                                "while pending\n",
                                static_cast<unsigned long long>(r.address), r.width, r.height, r.bits,
                                vk::to_string(r.format).c_str(), r.copies, r.cpu_accesses);
                }
                readback_images = {};
                readback_report = now;
            }
        }
        return;
    }
    std::unique_lock lk{download_images_mutex};
    boost::container::small_vector<PendingDownload, 16> pending;
    for (const ImageId image_id : download_images) {
        if (auto download = RecordImageDownload(image_id)) {
            pending.push_back(*download);
        }
    }
    download_images.clear();
    if (pending.empty()) {
        return;
    }
    scheduler.Finish();
    for (const PendingDownload& download : pending) {
        download.staging.Invalidate();
        Core::Memory::Instance()->TryWriteBacking(std::bit_cast<u8*>(download.guest_address),
                                                  download.staging.mapped, download.size);
        runtime.GetStagingPool().FreeDeferred(download.staging);
    }
}

std::optional<TextureCache::PendingDownload> TextureCache::RecordImageDownload(ImageId image_id) {
    Image& image = slot_images[image_id];
    if (False(image.flags & ImageFlagBits::GpuModified)) {
        return std::nullopt;
    }
    const u32 download_size = image.info.pitch * image.info.size.height * image.info.size.depth *
                              image.info.resources.layers * (image.info.num_bits / 8);
    ASSERT(download_size <= image.info.guest_size);
    const auto download =
        runtime.GetStagingPool().Request(download_size, MemoryType::HostCached, 16, true);
    const vk::BufferImageCopy image_download = {
        .bufferOffset = download.offset,
        .bufferRowLength = image.info.pitch,
        .bufferImageHeight = image.info.size.height,
        .imageSubresource =
            {
                .aspectMask = image.info.props.is_depth ? vk::ImageAspectFlagBits::eDepth
                                                        : vk::ImageAspectFlagBits::eColor,
                .mipLevel = 0,
                .baseArrayLayer = 0,
                .layerCount = image.info.resources.layers,
            },
        .imageOffset = {0, 0, 0},
        .imageExtent = {image.info.size.width, image.info.size.height, image.info.size.depth},
    };
    runtime.DownloadImage(&image, download.buffer, std::span{&image_download, 1});
    return PendingDownload{image.info.guest_address, download_size, download};
}

void TextureCache::DownloadImageMemory(ImageId image_id, bool sync) {
    Image& image = slot_images[image_id];
    ResolveReadbacks(image.info.guest_address, image.info.guest_size, true,
                     ReadbackSource::ImageDownload);
    if (False(image.flags & ImageFlagBits::GpuModified)) {
        return;
    }
    const u32 download_size = image.info.pitch * image.info.size.height * image.info.size.depth *
                              image.info.resources.layers * (image.info.num_bits / 8);
    ASSERT(download_size <= image.info.guest_size);
    const auto download =
        runtime.GetStagingPool().Request(download_size, MemoryType::HostCached, 16, !sync);
    const vk::BufferImageCopy image_download = {
        .bufferOffset = download.offset,
        .bufferRowLength = image.info.pitch,
        .bufferImageHeight = image.info.size.height,
        .imageSubresource =
            {
                .aspectMask = image.info.props.is_depth ? vk::ImageAspectFlagBits::eDepth
                                                        : vk::ImageAspectFlagBits::eColor,
                .mipLevel = 0,
                .baseArrayLayer = 0,
                .layerCount = image.info.resources.layers,
            },
        .imageOffset = {0, 0, 0},
        .imageExtent = {image.info.size.width, image.info.size.height, image.info.size.depth},
    };
    runtime.DownloadImage(&image, download.buffer, std::span{&image_download, 1});
    if (sync) {
        scheduler.Finish();
        download.Invalidate();
        Core::Memory::Instance()->TryWriteBacking(std::bit_cast<u8*>(image.info.guest_address),
                                                  download.mapped, download_size);
    } else {
        scheduler.DeferPriorityOperation(
            [this, device_addr = image.info.guest_address, download, download_size] {
                download.Invalidate();
                Core::Memory::Instance()->TryWriteBacking(std::bit_cast<u8*>(device_addr),
                                                          download.mapped, download_size);
                runtime.GetStagingPool().FreeDeferred(download);
            });
    }
}

void TextureCache::DiagDumpImage(ImageId image_id, const char* tag) {
    Image& image = slot_images[image_id];
    const u32 bytes_per_pixel = image.info.num_bits / 8;
    if (image.info.props.is_depth || image.info.props.is_block || bytes_per_pixel == 0) {
        return;
    }
    const u32 size = image.info.pitch * image.info.size.height * bytes_per_pixel;
    const auto download = runtime.GetStagingPool().Request(size, MemoryType::HostCached, 16);
    const vk::BufferImageCopy copy = {
        .bufferOffset = download.offset,
        .bufferRowLength = image.info.pitch,
        .bufferImageHeight = image.info.size.height,
        .imageSubresource = {vk::ImageAspectFlagBits::eColor, 0, 0, 1},
        .imageOffset = {0, 0, 0},
        .imageExtent = {image.info.size.width, image.info.size.height, 1},
    };
    runtime.DownloadImage(&image, download.buffer, std::span{&copy, 1});
    scheduler.Finish();
    download.Invalidate();
    static int sequence = 0;
    const char* dir = std::getenv("GOW3_DIAG_DIR");
    const char* dir_path = dir && *dir ? dir : "out/diag";
    std::error_code ec;
    std::filesystem::create_directories(dir_path, ec);
    char path[512];
    std::snprintf(path, sizeof(path), "%s/%03d_%s_%llx_%ux%u_p%u_b%u_f%d.raw", dir_path, sequence++, tag,
                  static_cast<unsigned long long>(image.info.guest_address), image.info.size.width,
                  image.info.size.height, image.info.pitch, bytes_per_pixel,
                  static_cast<int>(image.info.pixel_format));
    if (FILE* f = std::fopen(path, "wb")) {
        std::fwrite(download.mapped, 1, size, f);
        std::fclose(f);
        std::printf("Diag: %s\n", path);
    } else {
        std::printf("Diag: cannot write %s\n", path);
    }
}

// gow3: images sharing guest memory (aliases), ported from cuesta4/shadPS4 (eltutz) commit
// 189ca3b6. A write to one is copied into the others before they are read, as on the PS4,
// where they are the same memory.
static bool Covers(const Extent3D& extent, const ImageInfo& info) {
    return extent.width == info.size.width && extent.height == info.size.height &&
           extent.depth == info.size.depth;
}

static bool CanAlias(const Image& lhs, const Image& rhs) {
    return GetAliasCopyExtent(lhs.info, rhs.info) || GetAliasCopyExtent(rhs.info, lhs.info);
}

template <typename Func>
void TextureCache::ForEachAlias(const Image& image, Func&& func) {
    const auto page = page_table.find(image.info.guest_address >> Traits::PageBits);
    if (page == nullptr) {
        return;
    }
    for (const ImageId candidate_id : *page) {
        Image& candidate = slot_images[candidate_id];
        if (&candidate != &image && candidate.info.guest_address == image.info.guest_address &&
            CanAlias(candidate, image)) {
            func(candidate_id, candidate);
        }
    }
}

void TextureCache::SynchronizeAlias(ImageId image_id) {
    Image& dst = slot_images[image_id];
    if (False(dst.flags & ImageFlagBits::Aliased)) {
        return;
    }
    const auto state_it = alias_states.find(dst.info.guest_address);
    if (state_it == alias_states.end()) {
        return;
    }
    AliasState& state = state_it.value();
    if (!IsLiveImage(state.backing, state.backing_uid)) {
        if (!IsLiveImage(state.writer, state.writer_uid)) {
            state.ResetAuthority();
            return;
        }
        state.backing = state.writer, state.backing_uid = state.writer_uid;
        state.writer = {}, state.writer_uid = 0;
    }
    if (state.writer == image_id && state.writer_uid == dst.image_uid) {
        return;
    }
    if (!CommitAliasWriter(state)) {
        state.ResetAuthority();
        return;
    }
    Image& current = slot_images[state.backing];
    const std::optional extent = GetAliasCopyExtent(current.info, dst.info);
    if (state.backing == image_id || current.alias_generation <= dst.alias_generation || !extent) {
        return;
    }
    CopyAlias(state.backing, image_id, *extent);
    dst.alias_generation = current.alias_generation;
    const std::optional reverse = GetAliasCopyExtent(dst.info, current.info);
    if (reverse && Covers(*reverse, current.info) && !Covers(*extent, dst.info)) {
        state.backing = image_id, state.backing_uid = dst.image_uid;
    }
}

bool TextureCache::CommitAliasWriter(AliasState& state) {
    if (!state.writer) {
        return true;
    }
    if (!IsLiveImage(state.writer, state.writer_uid) ||
        !IsLiveImage(state.backing, state.backing_uid)) {
        return false;
    }
    Image& writer = slot_images[state.writer];
    Image& backing = slot_images[state.backing];
    if (state.writer != state.backing && writer.alias_generation > backing.alias_generation) {
        const std::optional extent = GetAliasCopyExtent(writer.info, backing.info);
        if (!extent) {
            return false;
        }
        CopyAlias(state.writer, state.backing, *extent);
        backing.alias_generation = writer.alias_generation;
    }
    state.writer = {}, state.writer_uid = 0;
    return true;
}

void TextureCache::CopyAlias(ImageId src_id, ImageId dst_id, const Extent3D& extent) {
    Image& dst = slot_images[dst_id];
    runtime.CopyAliasImage(&slot_images[src_id], &dst, {extent.width, extent.height, extent.depth});
    dst.flags |= ImageFlagBits::GpuModified;
    dst.flags &= ~ImageFlagBits::Dirty;
}

void TextureCache::PublishAliasWrite(ImageId image_id) {
    Image& image = slot_images[image_id];
    image.alias_generation = ++alias_generation;
    if (False(image.flags & ImageFlagBits::Aliased)) {
        return;
    }
    std::scoped_lock lock{mutex};
    AliasState& state = alias_states[image.info.guest_address];
    if (IsLiveImage(state.backing, state.backing_uid) &&
        !CanAlias(image, slot_images[state.backing])) {
        state.ResetAuthority();
    }
    state.members = std::max(state.members, 2u);
    if (state.writer == image_id && state.writer_uid == image.image_uid) {
        return;
    }
    if (IsLiveImage(state.backing, state.backing_uid)) {
        const ImageInfo& backing = slot_images[state.backing].info;
        const std::optional extent = GetAliasCopyExtent(image.info, backing);
        if (extent && Covers(*extent, backing)) {
            state.backing = image_id, state.backing_uid = image.image_uid;
        } else {
            state.writer = image_id, state.writer_uid = image.image_uid;
        }
    } else {
        state.backing = image_id, state.backing_uid = image.image_uid;
    }
}

void TextureCache::InvalidateAlias(Image& image) {
    image.alias_generation = 0;
    if (True(image.flags & ImageFlagBits::Aliased)) {
        if (const auto it = alias_states.find(image.info.guest_address); it != alias_states.end()) {
            it.value().ResetAuthority();
        }
    }
}

void TextureCache::MarkAsMaybeDirty(ImageId image_id, Image& image) {
    if (image.hash == 0) {
        // Initialize hash
        const u8* addr = std::bit_cast<u8*>(image.info.guest_address);
        image.hash = XXH3_64bits(addr, image.info.guest_size);
    }
    image.flags |= ImageFlagBits::MaybeCpuDirty;
    UntrackImage(image_id);
}

void TextureCache::InvalidateMemory(VAddr addr, size_t size) {
    std::scoped_lock lock{mutex};
    const auto pages_start = PageManager::GetPageAddr(addr);
    const auto pages_end = PageManager::GetNextPageAddr(addr + size - 1);
    ForEachImageInRegion(pages_start, pages_end - pages_start, [&](ImageId image_id, Image& image) {
        const auto image_begin = image.info.guest_address;
        const auto image_end = image.info.guest_address + image.info.guest_size;
        if (image.Overlaps(addr, size)) {
            // Modified region overlaps image, so the image was definitely accessed by this fault.
            // Untrack the image, so that the range is unprotected and the guest can write freely.
            InvalidateAlias(image);
            image.flags |= ImageFlagBits::CpuDirty;
            UntrackImage(image_id);
        } else if (pages_end < image_end) {
            // This page access may or may not modify the image.
            // We should not mark it as dirty now. If it really was modified
            // it will receive more invalidations on its other pages.
            // Remove tracking from this page only.
            UntrackImageHead(image_id);
        } else if (image_begin < pages_start) {
            // This page access does not modify the image but the page should be untracked.
            // We should not mark this image as dirty now. If it really was modified
            // it will receive more invalidations on its other pages.
            UntrackImageTail(image_id);
        } else {
            // Image begins and ends on this page so it can not receive any more invalidations.
            // We will check it's hash later to see if it really was modified.
            MarkAsMaybeDirty(image_id, image);
        }
    });
}

void TextureCache::NoteBackingWrite(VAddr address, u64 size) {
    std::scoped_lock lock{backing_writes_mutex};
    backing_writes.emplace_back(address, size);
    has_backing_writes.store(true, std::memory_order_release);
}

void TextureCache::ApplyBackingWrites() {
    if (!has_backing_writes.load(std::memory_order_acquire)) {
        return;
    }
    std::vector<std::pair<VAddr, u64>> writes;
    {
        std::scoped_lock lock{backing_writes_mutex};
        writes.swap(backing_writes);
        has_backing_writes.store(false, std::memory_order_release);
    }
    // God of War III streams textures with GPU copies through large storage images over the
    // texture pool; their readbacks put the new texels in memory without a fault. Textures
    // there kept the old contents (yellow ground #138, missing textures #194) until a restart.
    for (const auto& [address, size] : writes) {
        ForEachImageInRegion(address, size, [&](ImageId image_id, Image& image) {
            // Images sharing memory with a GPU writer are synchronized by SynchronizeAlias.
            if (True(image.flags & (ImageFlagBits::GpuModified | ImageFlagBits::Aliased))) {
                return;
            }
            image.flags |= ImageFlagBits::CpuDirty;
            UntrackImage(image_id);
        });
    }
}

void TextureCache::InvalidateMemoryFromGPU(VAddr address, size_t max_size) {
    // Partial buffer writes still need the image's untouched bytes in guest memory.
    ResolveReadbacks(address, max_size, true, ReadbackSource::GpuPartialWrite);
    std::scoped_lock lock{mutex};
    ForEachImageInRegion(address, max_size, [&](ImageId image_id, Image& image) {
        // Only consider images that match base address.
        // TODO: Maybe also consider subresources
        if (image.info.guest_address != address) {
            return;
        }
        // Ensure image is reuploaded when accessed again.
        InvalidateAlias(image);
        image.flags |= ImageFlagBits::GpuDirty;
    });
}

void TextureCache::UnmapMemory(VAddr cpu_addr, size_t size) {
    CancelReadbacks(cpu_addr, size);
    std::scoped_lock lk{mutex};

    ImageIds deleted_images;
    ForEachImageInRegion(cpu_addr, size, [&](ImageId id, Image&) { deleted_images.push_back(id); });
    for (const ImageId id : deleted_images) {
        // TODO: Download image data back to host.
        FreeImage(id);
    }
}

ImageId TextureCache::ResolveDepthOverlap(const ImageInfo& requested_info, BindingType binding,
                                          ImageId cache_image_id) {
    auto& cache_image = slot_images[cache_image_id];

    if (!cache_image.info.props.is_depth && !requested_info.props.is_depth) {
        return {};
    }

    const bool stencil_match =
        requested_info.props.has_stencil == cache_image.info.props.has_stencil;
    const bool bpp_match = requested_info.num_bits == cache_image.info.num_bits;

    // If an image in the cache has less slices we need to expand it
    bool recreate = cache_image.info.resources < requested_info.resources;

    switch (binding) {
    case BindingType::Texture:
        // The guest requires a depth sampled texture, but cache can offer only Rxf. Need to
        // recreate the image.
        recreate |= requested_info.props.is_depth && !cache_image.info.props.is_depth;
        break;
    case BindingType::Storage:
        // If the guest is going to use previously created depth as storage, the image needs to be
        // recreated. (TODO: Probably a case with linear rgba8 aliasing is legit)
        recreate |= cache_image.info.props.is_depth;
        break;
    case BindingType::RenderTarget:
        // Render target can have only Rxf format. If the cache contains only Dx[S8] we need to
        // re-create the image.
        ASSERT(!requested_info.props.is_depth);
        recreate |= cache_image.info.props.is_depth;
        break;
    case BindingType::DepthTarget:
        // The guest has requested previously allocated texture to be bound as a depth target.
        // In this case we need to convert Rx float to a Dx[S8] as requested
        recreate |= !cache_image.info.props.is_depth;

        // The guest is trying to bind a depth target and cache has it. Need to be sure that aspects
        // and bpp match
        recreate |= cache_image.info.props.is_depth && !(stencil_match && bpp_match);
        break;
    default:
        break;
    }

    if (recreate) {
        auto new_info = requested_info;
        new_info.resources = std::max(requested_info.resources, cache_image.info.resources);
        new_info.UpdateSize();
        const auto new_image_id = slot_images.insert(instance, runtime, slot_image_views, new_info);
        RegisterImage(new_image_id);

        // Inherit image usage
        auto& new_image = slot_images[new_image_id];
        new_image.usage = cache_image.usage;
        new_image.flags &= ~ImageFlagBits::Dirty;
        // When creating a depth buffer through overlap resolution don't clear it on first use.
        new_image.info.meta_info.htile_clear_mask = 0;
        runtime.CopyColorAndDepth(&cache_image, &new_image);

        // Free the cache image.
        FreeImage(cache_image_id);
        return new_image_id;
    }

    // Will be handled by view
    return cache_image_id;
}

std::tuple<ImageId, int, int> TextureCache::ResolveOverlap(const ImageInfo& image_info,
                                                           BindingType binding,
                                                           ImageId cache_image_id,
                                                           ImageId merged_image_id,
                                                           bool native_reads) {
    auto& cache_image = slot_images[cache_image_id];
    const bool safe_to_delete =
        scheduler.CurrentTick() - cache_image.tick_accessed_last > NumFramesBeforeRemoval;

    // Equal address
    if (image_info.guest_address == cache_image.info.guest_address) {
        const u32 lhs_block_size = image_info.num_bits * image_info.guest_samples;
        const u32 rhs_block_size = cache_image.info.num_bits * cache_image.info.guest_samples;
        // gow3: the top rows of a cached image with the same layout. God of War III reads its
        // frame buffers as R8G8B8A8Uint with a height that changes every frame during Rage of
        // Sparta and weapon effects; as a "pool allocation" below, each read created and uploaded
        // a new ~8 MB image (FPS 120 -> 40).
        // - Only an integer view of another format: same-format requests here are real pool
        //   reuse (with them, normalized sampling read the wrong rows and blew out exposure).
        // - Only for shaders that read with integer coordinates (ImageResource::needs_native):
        //   normalized sampling of the taller image would read and clamp at the wrong rows.
        // - Known limit: the view keeps the cached size. A size query sees it, and accesses past
        //   the requested extent but inside the cached one reach the frame buffer instead of
        //   being dropped (robustImageAccess2 bounds by the taller image).
        const auto& a = image_info;
        const auto& b = cache_image.info;
        const auto is_integer = [](vk::Format format) {
            const std::string_view numeric = vk::componentNumericFormat(format, 0);
            return numeric == "UINT" || numeric == "SINT";
        };
        // Launcher: Performance, "Fast screen effects" (GOW3_PARTIAL_HEIGHT_VIEWS=0 turns it off).
        static const bool partial_views_enabled = [] {
            const char* value = std::getenv("GOW3_PARTIAL_HEIGHT_VIEWS");
            return !value || value[0] != '0';
        }();
        if (partial_views_enabled && !Gow3Toggle::Disabled(Gow3Toggle::PartialHeightViews) &&
            (binding == BindingType::Texture || binding == BindingType::Storage) && native_reads &&
            a.pixel_format != b.pixel_format && is_integer(a.pixel_format) &&
            lhs_block_size == rhs_block_size && a.pitch == b.pitch && a.tile_mode == b.tile_mode &&
            a.type == b.type && a.size.width <= b.size.width && a.size.height < b.size.height &&
            a.size.depth == 1 && b.size.depth == 1 && a.resources.levels == 1 &&
            a.resources.layers == 1 && b.resources.levels == 1 && b.resources.layers == 1 &&
            !a.props.is_block && !b.props.is_block && !a.props.is_depth && !b.props.is_depth &&
            IsVulkanFormatCompatible(b.pixel_format, a.pixel_format)) {
            // With partial images left from before at this address, FindImage's later iterations
            // compare the merged image against the taller ones, so the loop ends on the full
            // image (a taller non-integer image falls to the pool path and keeps the merged one).
            // Counted only on the request's own comparison (a is the merged image afterwards).
            if (Vulkan::WaitDiagnostics::Enabled() && !merged_image_id) {
                NotePartialView(a, b, binding);
            }
            return {cache_image_id, -1, -1};
        }
        if (image_info.BlockDim() != cache_image.info.BlockDim() ||
            lhs_block_size != rhs_block_size) {
            // Very likely this kind of overlap is caused by allocation from a pool.
            if (safe_to_delete) {
                FreeImage(cache_image_id);
            }
            return {merged_image_id, -1, -1};
        }

        if (const auto depth_image_id = ResolveDepthOverlap(image_info, binding, cache_image_id)) {
            return {depth_image_id, -1, -1};
        }

        // Compressed view of uncompressed image with same block size.
        if (image_info.props.is_block && !cache_image.info.props.is_block) {
            return {ExpandImage(image_info, cache_image_id), -1, -1};
        }

        if (image_info.guest_size == cache_image.info.guest_size &&
            (image_info.type == AmdGpu::ImageType::Color3D ||
             cache_image.info.type == AmdGpu::ImageType::Color3D)) {
            return {ExpandImage(image_info, cache_image_id), -1, -1};
        }

        const bool pow2_padding_only =
            image_info.props.is_pow2 != cache_image.info.props.is_pow2 &&
            image_info.tile_mode == cache_image.info.tile_mode &&
            image_info.size == cache_image.info.size &&
            image_info.pitch == cache_image.info.pitch && image_info.resources.levels == 1 &&
            cache_image.info.resources.levels == 1 && image_info.resources.layers == 1 &&
            cache_image.info.resources.layers == 1;

        // Size and resources are less than or equal, use image view.
        if (image_info.pixel_format != cache_image.info.pixel_format ||
            image_info.guest_size <= cache_image.info.guest_size || pow2_padding_only) {
            auto result_id = merged_image_id ? merged_image_id : cache_image_id;
            const auto& result_image = slot_images[result_id];
            const bool is_compatible =
                IsVulkanFormatCompatible(result_image.info.pixel_format, image_info.pixel_format);
            return {is_compatible ? result_id : ImageId{}, -1, -1};
        }

        // Size and resources are greater, expand the image.
        if (image_info.type == cache_image.info.type &&
            image_info.resources > cache_image.info.resources) {
            return {ExpandImage(image_info, cache_image_id), -1, -1};
        }

        // Size is greater but resources are not, because the tiling mode is different.
        // Likely the address is reused for a image with a different tiling mode.
        if (image_info.tile_mode != cache_image.info.tile_mode) {
            if (safe_to_delete) {
                FreeImage(cache_image_id);
            }
            return {merged_image_id, -1, -1};
        }

        // Enhanced debug logging for unreachable case
        // Calculate expected size based on format and dimensions
        u64 expected_size =
            (static_cast<u64>(image_info.size.width) * static_cast<u64>(image_info.size.height) *
             static_cast<u64>(image_info.size.depth) * static_cast<u64>(image_info.num_bits) / 8);
        LOG_ERROR(Render_Vulkan,
                  "Unresolvable image overlap with equal memory address:\n"
                  "=== OLD IMAGE (cached) ===\n"
                  "  Address:        {:#x}\n"
                  "  Size:           {:#x} bytes\n"
                  "  Format:         {}\n"
                  "  Type:           {}\n"
                  "  Width:          {}\n"
                  "  Height:         {}\n"
                  "  Depth:          {}\n"
                  "  Pitch:          {}\n"
                  "  Mip levels:     {}\n"
                  "  Array layers:   {}\n"
                  "  Samples:        {}\n"
                  "  Tile mode:      {:#x}\n"
                  "  Block size:     {} bits\n"
                  "  Is block-comp:  {}\n"
                  "  Guest size:     {:#x}\n"
                  "  Last accessed:  tick {}\n"
                  "  Safe to delete: {}\n"
                  "  isPow2:         {}\n"
                  "  Alt tile:       {}\n"
                  "\n"
                  "=== NEW IMAGE (requested) ===\n"
                  "  Address:        {:#x}\n"
                  "  Size:           {:#x} bytes\n"
                  "  Format:         {}\n"
                  "  Type:           {}\n"
                  "  Width:          {}\n"
                  "  Height:         {}\n"
                  "  Depth:          {}\n"
                  "  Pitch:          {}\n"
                  "  Mip levels:     {}\n"
                  "  Array layers:   {}\n"
                  "  Samples:        {}\n"
                  "  Tile mode:      {:#x}\n"
                  "  Block size:     {} bits\n"
                  "  Is block-comp:  {}\n"
                  "  Guest size:     {:#x}\n"
                  "  isPow2:         {}\n"
                  "  Alt tile:       {}\n"
                  "\n"
                  "=== COMPARISON ===\n"
                  "  Same format:           {}\n"
                  "  Same type:             {}\n"
                  "  Same tile mode:        {}\n"
                  "  Same block size:       {}\n"
                  "  Same BlockDim:         {}\n"
                  "  Same pitch:            {}\n"
                  "  Same pow2:             {}\n"
                  "  Same alt tile:         {}\n"
                  "  Old resources <= new:  {} (old: {}, new: {})\n"
                  "  Old size <= new size:  {}\n"
                  "  Expected size (calc):  {} bytes\n"
                  "  Size ratio (new/expected): {:.2f}x\n"
                  "  Size ratio (new/old):  {:.2f}x\n"
                  "  Old vs expected diff:  {} bytes ({:+.2f}%)\n"
                  "  New vs expected diff:  {} bytes ({:+.2f}%)\n"
                  "  Merged image ID:       {}\n"
                  "  Binding type:          {}\n"
                  "  Current tick:          {}\n"
                  "  Age (ticks since last access): {}",

                  // Old image details
                  cache_image.info.guest_address, cache_image.info.guest_size,
                  vk::to_string(cache_image.info.pixel_format),
                  static_cast<int>(cache_image.info.type), cache_image.info.size.width,
                  cache_image.info.size.height, cache_image.info.size.depth, cache_image.info.pitch,
                  cache_image.info.resources.levels, cache_image.info.resources.layers,
                  cache_image.info.num_samples, static_cast<u32>(cache_image.info.tile_mode),
                  cache_image.info.num_bits, +cache_image.info.props.is_block,
                  cache_image.info.guest_size, cache_image.tick_accessed_last, safe_to_delete,
                  bool(cache_image.info.props.is_pow2), cache_image.info.alt_tile,

                  // New image details
                  image_info.guest_address, image_info.guest_size,
                  vk::to_string(image_info.pixel_format), static_cast<int>(image_info.type),
                  image_info.size.width, image_info.size.height, image_info.size.depth,
                  image_info.pitch, image_info.resources.levels, image_info.resources.layers,
                  image_info.num_samples, static_cast<u32>(image_info.tile_mode),
                  image_info.num_bits, image_info.props.is_block, image_info.guest_size,
                  bool(image_info.props.is_pow2), image_info.alt_tile,

                  // Comparison
                  (image_info.pixel_format == cache_image.info.pixel_format),
                  (image_info.type == cache_image.info.type),
                  (image_info.tile_mode == cache_image.info.tile_mode),
                  (image_info.num_bits == cache_image.info.num_bits),
                  (image_info.BlockDim() == cache_image.info.BlockDim()),
                  (image_info.pitch == cache_image.info.pitch),
                  (image_info.props.is_pow2 == cache_image.info.props.is_pow2),
                  (image_info.alt_tile == cache_image.info.alt_tile),
                  (cache_image.info.resources <= image_info.resources),
                  cache_image.info.resources.levels, image_info.resources.levels,
                  (cache_image.info.guest_size <= image_info.guest_size), expected_size,

                  // Size ratios
                  static_cast<double>(image_info.guest_size) / expected_size,
                  static_cast<double>(image_info.guest_size) / cache_image.info.guest_size,

                  // Difference between actual and expected sizes with percentages
                  static_cast<s64>(cache_image.info.guest_size) - static_cast<s64>(expected_size),
                  (static_cast<double>(cache_image.info.guest_size) / expected_size - 1.0) * 100.0,

                  static_cast<s64>(image_info.guest_size) - static_cast<s64>(expected_size),
                  (static_cast<double>(image_info.guest_size) / expected_size - 1.0) * 100.0,

                  merged_image_id.index, static_cast<int>(binding), scheduler.CurrentTick(),
                  scheduler.CurrentTick() - cache_image.tick_accessed_last);

        UNREACHABLE_MSG("Encountered unresolvable image overlap with equal memory address.");
    }

    // Right overlap, the image requested is a possible subresource of the image from cache.
    if (image_info.guest_address > cache_image.info.guest_address) {
        if (auto mip = image_info.MipOf(cache_image.info); mip >= 0) {
            if (auto slice = image_info.SliceOf(cache_image.info, mip); slice >= 0) {
                return {cache_image_id, mip, slice};
            }
        }

        // Image isn't a subresource but a chance overlap.
        if (safe_to_delete) {
            FreeImage(cache_image_id);
        }

        return {{}, -1, -1};
    } else {
        // Left overlap, the image from cache is a possible subresource of the image requested
        if (auto mip = cache_image.info.MipOf(image_info); mip >= 0) {
            if (auto slice = cache_image.info.SliceOf(image_info, mip); slice >= 0) {
                // We have a larger image created and a separate one, representing a subres of it
                // bound as render target. In this case we need to rebind render target.
                if (cache_image.binding.is_target) {
                    cache_image.binding.needs_rebind = 1u;
                    if (merged_image_id) {
                        GetImage(merged_image_id).binding.is_target = 1u;
                    }

                    FreeImage(cache_image_id);
                    return {merged_image_id, -1, -1};
                }

                // We need to have a larger, already allocated image to copy this one into
                if (merged_image_id) {
                    auto& merged_image = slot_images[merged_image_id];
                    runtime.CopyMip(&cache_image, &merged_image, mip, slice);
                    FreeImage(cache_image_id);
                }
            }
        }
    }

    return {merged_image_id, -1, -1};
}

ImageId TextureCache::ExpandImage(const ImageInfo& info, ImageId image_id) {
    const auto new_image_id = slot_images.insert(instance, runtime, slot_image_views, info);
    RegisterImage(new_image_id);

    auto& src_image = slot_images[image_id];
    auto& new_image = slot_images[new_image_id];

    RefreshImage(new_image);
    runtime.CopyImage(&src_image, &new_image);

    if (src_image.binding.is_bound || src_image.binding.is_target) {
        src_image.binding.needs_rebind = 1u;
    }

    FreeImage(image_id);
    TrackImage(new_image_id);
    return new_image_id;
}

ImageId TextureCache::FindImage(ImageDesc& desc, bool exact_fmt) {
    const auto& info = desc.info;
    ASSERT(info.guest_address != 0);

    std::scoped_lock lock{mutex};

    u64 key_hash = info.guest_address ^ info.guest_size << 17 ^ u64(info.pixel_format) << 40 ^
                   u64(info.size.width) << 24 ^ info.size.height ^ u64(desc.type) << 58 ^
                   u64(exact_fmt) << 63 ^ u64(desc.needs_native) << 62;
    key_hash = (key_hash ^ key_hash >> 33) * 0xFF51AFD7ED558CCDull;
    key_hash = (key_hash ^ key_hash >> 33) * 0xC4CEB9FE1A85EC53ull;
    key_hash ^= key_hash >> 33;
    auto& cached = find_image_cache[key_hash % find_image_cache.size()];
    // An exact match stays valid while that image is registered with the same description;
    // resolved overlaps (views into other images) only until any image registration changes.
    const auto still_exact = [&] {
        if (cached.view_mip >= 0 || cached.view_slice >= 0 || !cached.image_id) {
            return false;
        }
        const Image& image = slot_images[cached.image_id];
        return True(image.flags & ImageFlagBits::Registered) &&
               image.info.guest_address == info.guest_address &&
               image.info.guest_size == info.guest_size && image.info.size == info.size &&
               image.info.pixel_format == info.pixel_format;
    };
    if ((cached.generation == registry_generation.load(std::memory_order_relaxed) || still_exact()) &&
        cached.address == info.guest_address &&
        cached.size == info.guest_size && cached.extent == info.size &&
        cached.format == info.pixel_format && cached.type == info.type &&
        cached.exact_fmt == exact_fmt && cached.binding == desc.type &&
        cached.needs_native == desc.needs_native && cached.levels == info.resources.levels && cached.layers == info.resources.layers &&
        !Gow3Toggle::Disabled(Gow3Toggle::FindImageCache)) {
        Image& image = slot_images[cached.image_id];
        image.tick_accessed_last = scheduler.CurrentTick();
        TouchImage(image);
        if (cached.view_mip > 0) {
            desc.view_info.range.base.level = cached.view_mip;
        }
        if (cached.view_slice > 0) {
            desc.view_info.range.base.layer = cached.view_slice;
        }
        return cached.image_id;
    }

    ImageIds image_ids;
    ForEachImageInRegion(info.guest_address, info.guest_size,
                         [&](ImageId image_id, Image& image) { image_ids.push_back(image_id); });

    ImageId image_id{};

    // Check for a perfect match first
    for (const auto& cache_id : image_ids) {
        auto& cache_image = slot_images[cache_id];
        if (cache_image.info.guest_address != info.guest_address) {
            continue;
        }
        if (cache_image.info.guest_size != info.guest_size) {
            continue;
        }
        if (cache_image.info.size != info.size) {
            continue;
        }
        if (!IsVulkanFormatCompatible(cache_image.info.pixel_format, info.pixel_format) ||
            (cache_image.info.type != info.type && info.size != Extent3D{1, 1, 1})) {
            continue;
        }
        if (exact_fmt && info.pixel_format != cache_image.info.pixel_format) {
            continue;
        }
        image_id = cache_id;
    }

    // gow3 (GOW3_PERF_DIAG): what was cached here, read before overlap resolution may free it.
    const bool diag = Vulkan::WaitDiagnostics::Enabled();
    CreateStat created{info.guest_address, info.size.width, info.size.height, 1, info.pixel_format,
                       desc.type, image_ids.empty() ? "nothing cached" : "overlap unresolved"};
    if (diag && !image_id && !image_ids.empty()) {
        const auto same = std::ranges::find_if(image_ids, [&](ImageId id) {
            return slot_images[id].info.guest_address == info.guest_address;
        });
        const auto& prev = slot_images[same != image_ids.end() ? *same : image_ids.front()].info;
        created.prev_address = prev.guest_address;
        created.prev_width = prev.size.width;
        created.prev_height = prev.size.height;
        created.prev_format = prev.pixel_format;
        created.overlaps = static_cast<u32>(image_ids.size());
        created.prev_pitch = prev.pitch;
        created.prev_tile = static_cast<u32>(prev.tile_mode);
    }
    created.pitch = info.pitch;
    created.needs_native = desc.needs_native;
    created.tile = static_cast<u32>(info.tile_mode);

    // Try to resolve overlaps (if any)
    int view_mip{-1};
    int view_slice{-1};
    if (!image_id) {
        for (const auto& cache_id : image_ids) {
            view_mip = -1;
            view_slice = -1;

            const auto& merged_info = image_id ? slot_images[image_id].info : info;
            auto [overlap_image_id, overlap_view_mip, overlap_view_slice] =
                ResolveOverlap(merged_info, desc.type, cache_id, image_id, desc.needs_native);
            if (overlap_image_id) {
                image_id = overlap_image_id;
                view_mip = overlap_view_mip;
                view_slice = overlap_view_slice;
            }
        }
    }

    if (image_id) {
        Image& image_resolved = slot_images[image_id];
        if (exact_fmt && info.pixel_format != image_resolved.info.pixel_format) {
            // Cannot reuse this image as we need the exact requested format.
            image_id = {};
            created.reason = "exact format";
        } else if (image_resolved.info.resources < info.resources) {
            // The image was clearly picked up wrong.
            FreeImage(image_id);
            image_id = {};
            LOG_WARNING(Render_Vulkan, "Image overlap resolve failed");
            created.reason = "fewer resources";
        }
    }
    // Create and register a new image
    if (!image_id) {
        image_id = slot_images.insert(instance, runtime, slot_image_views, info);
        RegisterImage(image_id);
        if (diag) {
            ++images_created;
            const auto same = [&](const CreateStat& s) {
                return s.address == created.address && s.width == created.width &&
                       s.height == created.height && s.format == created.format &&
                       s.binding == created.binding;
            };
            auto slot = std::ranges::find_if(create_stats, same);
            if (slot != create_stats.end()) {
                created.count = slot->count + 1;
            } else {
                // A free slot, else the least seen one.
                slot = std::ranges::min_element(create_stats, {}, &CreateStat::count);
            }
            *slot = created;
        }
    }

    Image& image = slot_images[image_id];
    image.tick_accessed_last = scheduler.CurrentTick();
    TouchImage(image);

    // If the image requested is a subresource of the image from cache record its location.
    if (view_mip > 0) {
        desc.view_info.range.base.level = view_mip;
    }
    if (view_slice > 0) {
        desc.view_info.range.base.layer = view_slice;
    }

    cached = FindImageCacheEntry{
        .address = info.guest_address,
        .size = info.guest_size,
        .extent = info.size,
        .format = info.pixel_format,
        .type = info.type,
        .exact_fmt = exact_fmt,
        .binding = desc.type,
        .needs_native = desc.needs_native,
        .levels = info.resources.levels,
        .layers = info.resources.layers,
        .generation = registry_generation.load(std::memory_order_relaxed),
        .image_id = image_id,
        .view_mip = view_mip,
        .view_slice = view_slice,
    };
    return image_id;
}

ImageId TextureCache::FindImageFromRange(VAddr address, size_t size, bool ensure_valid) {
    ImageIds image_ids;
    ForEachImageInRegion(address, size, [&](ImageId image_id, Image& image) {
        if (image.info.guest_address != address) {
            return;
        }
        if (ensure_valid && !image.SafeToDownload()) {
            return;
        }
        image_ids.push_back(image_id);
    });
    if (image_ids.size() == 1) {
        // Sometimes image size might not exactly match with requested buffer size
        // If we only found 1 candidate image use it without too many questions.
        return image_ids.back();
    }
    if (!image_ids.empty()) {
        for (s32 i = 0; i < image_ids.size(); ++i) {
            Image& image = slot_images[image_ids[i]];
            if (image.info.guest_size == size) {
                return image_ids[i];
            }
        }
        LOG_WARNING(Render_Vulkan,
                    "Failed to find exact image match for copy addr={:#x}, size={:#x}", address,
                    size);
    }
    return {};
}

ImageView& TextureCache::FindTexture(ImageId image_id, const ImageDesc& desc, ViewMemo* memo,
                                     bool refresh) {
    Image& image = slot_images[image_id];
    if (desc.type == BindingType::Storage) {
        image.flags |= ImageFlagBits::GpuModified;
        if (readback_linear_images && (!image.info.props.is_tiled || image.info.size.width <= 8) &&
            image.info.guest_address != 0) {
            std::unique_lock lk{download_images_mutex};
            download_images.emplace(image_id);
        }
    }
    if (refresh) {
        UpdateImage(image_id);
    }
    if (desc.type == BindingType::Storage) {
        PublishAliasWrite(image_id);
    }
    if (memo && !Gow3Toggle::Disabled(Gow3Toggle::TextureViewMemo)) {
        if (memo->image_id == image_id && memo->backing == image.backing && memo->view_id) {
            return slot_image_views[memo->view_id];
        }
        ImageView& view = image.FindView(desc.view_info);
        const auto& ids = image.backing->image_view_ids;
        const u32 last = image.backing->last_view;
        memo->image_id = image_id;
        memo->backing = image.backing;
        memo->view_id = last < ids.size() && &slot_image_views[ids[last]] == &view ? ids[last]
                                                                                    : ids.back();
        return view;
    }
    return image.FindView(desc.view_info);
}

ImageView& TextureCache::FindRenderTarget(ImageId image_id, const ImageDesc& desc) {
    Image& image = slot_images[image_id];
    image.flags |= ImageFlagBits::GpuModified;
    if (readback_linear_images && (!image.info.props.is_tiled || image.info.size.width <= 8)) {
        std::unique_lock lk{download_images_mutex};
        download_images.emplace(image_id);
    }
    image.usage.render_target = 1u;
    UpdateImage(image_id);
    PublishAliasWrite(image_id);

    // Register meta data for this color buffer
    if (desc.info.meta_info.cmask_addr) {
        surface_metas.emplace(desc.info.meta_info.cmask_addr,
                              MetaDataInfo{.type = MetaType::CMask});
        image.info.meta_info.cmask_addr = desc.info.meta_info.cmask_addr;
    }

    if (desc.info.meta_info.fmask_addr) {
        surface_metas.emplace(desc.info.meta_info.fmask_addr,
                              MetaDataInfo{.type = MetaType::FMask});
        image.info.meta_info.fmask_addr = desc.info.meta_info.fmask_addr;
    }

    return image.FindView(desc.view_info, false);
}

ImageView& TextureCache::FindDepthTarget(ImageId image_id, const ImageDesc& desc) {
    Image& image = slot_images[image_id];
    image.flags |= ImageFlagBits::GpuModified;
    image.usage.depth_target = 1u;
    UpdateImage(image_id);

    // Register meta data for this depth buffer
    if (desc.info.meta_info.htile_addr) {
        surface_metas.emplace(desc.info.meta_info.htile_addr,
                              MetaDataInfo{.type = MetaType::HTile,
                                           .clear_mask = image.info.meta_info.htile_clear_mask});
        image.info.meta_info.htile_addr = desc.info.meta_info.htile_addr;
    }

    // If there is a stencil attachment, link depth and stencil.
    if (desc.info.stencil_addr != 0) {
        ImageId stencil_id{};
        ForEachImageInRegion(desc.info.stencil_addr, desc.info.stencil_size,
                             [&](ImageId image_id, Image& image) {
                                 if (image.info.guest_address == desc.info.stencil_addr) {
                                     stencil_id = image_id;
                                 }
                             });
        if (!stencil_id) {
            ImageInfo info{};
            info.guest_address = desc.info.stencil_addr;
            info.guest_size = desc.info.stencil_size;
            info.size = desc.info.size;
            stencil_id = slot_images.insert(instance, runtime, slot_image_views, info);
            RegisterImage(stencil_id);
        }
        Image& stencil_image = slot_images[stencil_id];
        TouchImage(stencil_image);
        stencil_image.AssociateDepth(image_id, image.image_uid);
    }

    return image.FindView(desc.view_info, false);
}

void TextureCache::RefreshImage(Image& image) {
    if (False(image.flags & ImageFlagBits::Dirty) || image.info.guest_samples > 1) {
        return;
    }
    ResolveReadbacks(image.info.guest_address, image.info.guest_size, true,
                     ReadbackSource::ImageRefresh);
    Gow3Stats::Timer timer{Gow3Stats::t_refresh};

    RENDERER_TRACE;
    TRACE_HINT(fmt::format("{:x}:{:x}", image.info.guest_address, image.info.guest_size));

    if (True(image.flags & ImageFlagBits::MaybeCpuDirty) &&
        False(image.flags & ImageFlagBits::CpuDirty)) {
        // The image size should be less than page size to be considered MaybeCpuDirty
        // So this calculation should be very uncommon and reasonably fast
        // For now we'll just check up to 64 first pixels
        const auto addr = std::bit_cast<u8*>(image.info.guest_address);
        const u32 w = std::min(image.info.size.width, u32(8));
        const u32 h = std::min(image.info.size.height, u32(8));

        const u32 s_w = image.info.props.is_block ? Common::DivCeil(w, 4u) : w;
        const u32 s_h = image.info.props.is_block ? Common::DivCeil(h, 4u) : h;
        const u32 size = s_w * s_h * (image.info.num_bits / 8);
        const u64 hash = XXH3_64bits(addr, size);
        if (image.hash == hash) {
            image.flags &= ~ImageFlagBits::MaybeCpuDirty;
            return;
        }
        image.hash = hash;
    }

    const u32 num_layers = image.info.resources.layers;
    const u32 num_mips = image.info.resources.levels;
    const bool is_gpu_modified = True(image.flags & ImageFlagBits::GpuModified);
    const bool is_gpu_dirty = True(image.flags & ImageFlagBits::GpuDirty);

    Gow3Stats::image_upload_bytes.fetch_add(image.info.guest_size, std::memory_order_relaxed);
    boost::container::small_vector<vk::BufferImageCopy, 14> image_copies;
    for (u32 m = 0; m < num_mips; m++) {
        const u32 width = std::max(image.info.size.width >> m, 1u);
        const u32 height = std::max(image.info.size.height >> m, 1u);
        const u32 depth =
            image.info.props.is_volume ? std::max(image.info.size.depth >> m, 1u) : 1u;
        const auto [mip_size, mip_pitch, mip_height, mip_offset] = image.info.mips_layout[m];

        // Protect GPU modified resources from accidental CPU reuploads.
        if (is_gpu_modified && !is_gpu_dirty) {
            const u8* addr = std::bit_cast<u8*>(image.info.guest_address);
            const u64 hash = XXH3_64bits(addr + mip_offset, mip_size);
            if (image.mip_hashes[m] == hash) {
                continue;
            }
            image.mip_hashes[m] = hash;
        }

        const u32 extent_width = mip_pitch ? std::min(mip_pitch, width) : width;
        const u32 extent_height = mip_height ? std::min(mip_height, height) : height;
        image_copies.push_back({
            .bufferOffset = mip_offset,
            .bufferRowLength = mip_pitch,
            .bufferImageHeight = mip_height,
            .imageSubresource{
                .aspectMask = image.aspect_mask & ~vk::ImageAspectFlagBits::eStencil,
                .mipLevel = m,
                .baseArrayLayer = 0,
                .layerCount = num_layers,
            },
            .imageOffset = {0, 0, 0},
            .imageExtent = {extent_width, extent_height, depth},
        });
    }

    if (image_copies.empty()) {
        image.flags &= ~ImageFlagBits::Dirty;
        return;
    }

    scheduler.EndRendering();

    const auto [in_buffer, in_offset] =
        buffer_cache.ObtainBufferForImage(image.info.guest_address, image.info.guest_size);
    const auto [buffer, offset] = tile_manager.DetileImage(in_buffer, in_offset, image.info);
    for (auto& copy : image_copies) {
        copy.bufferOffset += offset;
    }

    runtime.UploadImage(&image, buffer, image_copies);
}

vk::Sampler TextureCache::GetSampler(const AmdGpu::Sampler& sampler,
                                     AmdGpu::BorderColorBuffer border_color_base,
                                     const bool is_depth, float extra_lod_bias) {
    // Compare and plain uses of one S# need separate samplers; so do extra LOD biases.
    const u64 hash = HashCombine(HashCombine(XXH3_64bits(&sampler, sizeof(sampler)), is_depth),
                                 u64(std::bit_cast<u32>(extra_lod_bias)));

    std::scoped_lock lock{samplers_mutex};
    const auto [it, new_sampler] = samplers.try_emplace(hash, instance, sampler, border_color_base,
                                                        is_depth, extra_lod_bias);
    if (new_sampler) {
        samplers.at(hash).lru_id = sampler_lru_cache.Insert(hash, gc_tick);
    } else {
        sampler_lru_cache.Touch(it->second.lru_id, gc_tick);
    }

    return it->second.Handle();
}

void TextureCache::RegisterImage(ImageId image_id) {
    Gow3Stats::images_registered.fetch_add(1, std::memory_order_relaxed);
    Image& image = slot_images[image_id];
    ASSERT_MSG(False(image.flags & ImageFlagBits::Registered),
               "Trying to register an already registered image");
    image.flags |= ImageFlagBits::Registered;
    ++registry_generation;
    total_used_memory += Common::AlignUp(image.info.guest_size, 1024);
    image.lru_id = lru_cache.Insert(image_id, gc_tick);
    image.lru_touched_tick = gc_tick;

    AliasState* state{};
    u32 aliases{};
    bool inserted{};
    ForEachAlias(image, [&](ImageId candidate_id, Image& candidate) {
        if (!state) {
            auto result = alias_states.try_emplace(image.info.guest_address);
            state = std::addressof(result.first.value());
            inserted = result.second;
        }
        ++aliases;
        candidate.flags |= ImageFlagBits::Aliased;
        if (!IsLiveImage(state->backing, state->backing_uid) ||
            slot_images[state->backing].alias_generation < candidate.alias_generation) {
            state->backing = candidate_id, state->backing_uid = candidate.image_uid;
        }
    });
    if (state) {
        image.flags |= ImageFlagBits::Aliased;
        state->members = inserted ? aliases + 1 : state->members + 1;
    }

    ForEachPage(image.info.guest_address, image.info.guest_size,
                [this, image_id](u64 page) { page_table[page].push_back(image_id); });
}

void TextureCache::UnregisterImage(ImageId image_id) {
    Image& image = slot_images[image_id];
    ASSERT_MSG(True(image.flags & ImageFlagBits::Registered),
               "Trying to unregister an already unregistered image");
    image.flags &= ~ImageFlagBits::Registered;
    ++registry_generation;
    lru_cache.Free(image.lru_id);
    total_used_memory -= Common::AlignUp(image.info.guest_size, 1024);
    ForEachPage(image.info.guest_address, image.info.guest_size, [this, image_id](u64 page) {
        const auto page_it = page_table.find(page);
        if (page_it == nullptr) {
            UNREACHABLE_MSG("Unregistering unregistered page=0x{:x}", page << PageShift);
            return;
        }
        auto& image_ids = *page_it;
        const auto vector_it = std::ranges::find(image_ids, image_id);
        if (vector_it == image_ids.end()) {
            ASSERT_MSG(false, "Unregistering unregistered image in page=0x{:x}", page << PageShift);
            return;
        }
        image_ids.erase(vector_it);
    });
}

void TextureCache::TrackImage(ImageId image_id) {
    auto& image = slot_images[image_id];
    if (!(image.flags & ImageFlagBits::Registered)) {
        return;
    }
    const auto image_begin = image.info.guest_address;
    const auto image_end = image.info.guest_address + image.info.guest_size;
    if (image_begin == image.track_addr && image_end == image.track_addr_end) {
        return;
    }

    if (!image.IsTracked()) {
        // Re-track the whole image
        image.track_addr = image_begin;
        image.track_addr_end = image_end;
        tracker.UpdatePageWatchers<1>(image_begin, image.info.guest_size);
    } else {
        if (image_begin < image.track_addr) {
            TrackImageHead(image_id);
        }
        if (image.track_addr_end < image_end) {
            TrackImageTail(image_id);
        }
    }
}

void TextureCache::TrackImageHead(ImageId image_id) {
    auto& image = slot_images[image_id];
    if (!(image.flags & ImageFlagBits::Registered)) {
        return;
    }
    const auto image_begin = image.info.guest_address;
    if (image_begin == image.track_addr) {
        return;
    }
    ASSERT(image.track_addr != 0 && image_begin < image.track_addr);
    const auto size = image.track_addr - image_begin;
    image.track_addr = image_begin;
    tracker.UpdatePageWatchers<1>(image_begin, size);
}

void TextureCache::TrackImageTail(ImageId image_id) {
    auto& image = slot_images[image_id];
    if (!(image.flags & ImageFlagBits::Registered)) {
        return;
    }
    const auto image_end = image.info.guest_address + image.info.guest_size;
    if (image_end == image.track_addr_end) {
        return;
    }
    ASSERT(image.track_addr_end != 0 && image.track_addr_end < image_end);
    const auto addr = image.track_addr_end;
    const auto size = image_end - image.track_addr_end;
    image.track_addr_end = image_end;
    tracker.UpdatePageWatchers<1>(addr, size);
}

void TextureCache::UntrackImage(ImageId image_id) {
    auto& image = slot_images[image_id];
    if (!image.IsTracked()) {
        return;
    }
    const auto addr = image.track_addr;
    const auto size = image.track_addr_end - image.track_addr;
    image.track_addr = 0;
    image.track_addr_end = 0;
    if (size != 0) {
        tracker.UpdatePageWatchers<false>(addr, size);
    }
}

void TextureCache::UntrackImageHead(ImageId image_id) {
    auto& image = slot_images[image_id];
    const auto image_begin = image.info.guest_address;
    if (!image.IsTracked() || image_begin < image.track_addr) {
        return;
    }
    const auto addr = tracker.GetNextPageAddr(image_begin);
    const auto size = addr - image_begin;
    image.track_addr = addr;
    if (image.track_addr == image.track_addr_end) {
        // This image spans only 2 pages and both are modified,
        // but the image itself was not directly affected.
        // Cehck its hash later.
        MarkAsMaybeDirty(image_id, image);
    }
    tracker.UpdatePageWatchers<false>(image_begin, size);
}

void TextureCache::UntrackImageTail(ImageId image_id) {
    auto& image = slot_images[image_id];
    const auto image_end = image.info.guest_address + image.info.guest_size;
    if (!image.IsTracked() || image.track_addr_end < image_end) {
        return;
    }
    ASSERT(image.track_addr_end != 0);
    const auto addr = tracker.GetPageAddr(image_end);
    const auto size = image_end - addr;
    image.track_addr_end = addr;
    if (image.track_addr == image.track_addr_end) {
        // This image spans only 2 pages and both are modified,
        // but the image itself was not directly affected.
        // Cehck its hash later.
        MarkAsMaybeDirty(image_id, image);
    }
    tracker.UpdatePageWatchers<false>(addr, size);
}

void TextureCache::GarbageCollectImages() {
    Gow3Stats::Timer gc_timer{Gow3Stats::t_gc};
    if (instance.CanReportMemoryUsage()) {
        total_used_memory = instance.GetDeviceMemoryUsage();
        // gow3: on integrated GPUs (Steam Deck) the usage covers system-memory heaps holding
        // much more than images (buffers backing guest memory), and the startup budget left
        // ~1 GB after its 8 GB system reserve: usage stayed above the critical mark, so the
        // collector evicted images used two or three frames ago on every submission and wrote
        // GPU-written ones back. Compare with the driver's current budget instead.
        // GOW3_GC_BUDGET_MB=N: this rule with a fixed budget on any GPU (tests on a desktop).
        static const u64 forced_budget = [] {
            const char* env = std::getenv("GOW3_GC_BUDGET_MB");
            return env ? std::strtoull(env, nullptr, 10) << 20 : 0;
        }();
        if (instance.IsIntegrated() || forced_budget) {
            const u64 budget = forced_budget ? forced_budget : instance.GetDeviceMemoryBudgetNow();
            if (budget != 0) {
                trigger_gc_memory = budget / 10 * 7;
                pressure_gc_memory = budget / 100 * 85;
                critical_gc_memory = budget / 100 * 95;
            }
        }
    }
    if (total_used_memory < trigger_gc_memory) {
        return;
    }
    std::scoped_lock lock{mutex};
    bool pressured = false;
    bool aggresive = false;
    u64 ticks_to_destroy = 0;
    size_t num_deletions = 0;

    const auto configure = [&](bool allow_aggressive) {
        pressured = total_used_memory >= pressure_gc_memory;
        aggresive = allow_aggressive && total_used_memory >= critical_gc_memory;
        ticks_to_destroy = aggresive ? 160 : pressured ? 80 : 16;
        ticks_to_destroy = std::min(ticks_to_destroy, gc_tick);
        num_deletions = aggresive ? 40 : pressured ? 20 : 10;
    };
    const auto clean_up = [&](ImageId image_id) {
        if (num_deletions == 0) {
            return true;
        }
        --num_deletions;
        auto& image = slot_images[image_id];
        ResolveReadbacks(image.info.guest_address, image.info.guest_size, true,
                         ReadbackSource::CacheCleanup);
        const bool download = image.SafeToDownload();
        const bool tiled = image.info.IsTiled();
        if (tiled && download) {
            // This is a workaround for now. We can't handle non-linear image downloads.
            return false;
        }
        if (download && !pressured) {
            return false;
        }
        if (download) {
            // gow3: synchronously, while the image still protects its pages. A deferred
            // write-back landed after FreeImage had unprotected them, over whatever the game
            // had meanwhile stored there (e.g. its heap after unloading an area).
            DownloadImageMemory(image_id, true);
            ++gc_downloads;
        }
        ++gc_evictions;
        FreeImage(image_id);
        if (total_used_memory < critical_gc_memory) {
            if (aggresive) {
                num_deletions >>= 2;
                aggresive = false;
                return false;
            }
            if (pressured && total_used_memory < pressure_gc_memory) {
                num_deletions >>= 1;
                pressured = false;
            }
        }
        return false;
    };

    // Try to remove anything old enough and not high priority.
    configure(false);
    lru_cache.ForEachItemBelow(gc_tick - ticks_to_destroy, clean_up);

    if (total_used_memory >= critical_gc_memory) {
        // If we are still over the critical limit, run an aggressive GC
        configure(true);
        lru_cache.ForEachItemBelow(gc_tick - ticks_to_destroy, clean_up);
    }
    // gow3: evictions under memory pressure, at most every 5 s (GOW3_FRAME_STATS or not).
    if (pressured || gc_downloads != 0) {
        const auto now = std::chrono::steady_clock::now();
        if (now - gc_report_time >= std::chrono::seconds(5)) {
            std::printf("Texture cache: memory pressure, %llu of %llu MiB (critical %llu): "
                        "%llu images evicted, %llu written back since the last report\n",
                        (unsigned long long)(total_used_memory >> 20),
                        (unsigned long long)(pressure_gc_memory >> 20),
                        (unsigned long long)(critical_gc_memory >> 20),
                        (unsigned long long)gc_evictions, (unsigned long long)gc_downloads);
            gc_report_time = now;
            gc_evictions = gc_downloads = 0;
        }
    }
}

void TextureCache::GarbageCollectSamplers() {
    total_used_samplers = samplers.size();
    if (total_used_samplers < trigger_gc_samplers) {
        return;
    }
    std::scoped_lock lock{samplers_mutex};
    bool pressured = false;
    bool aggresive = false;
    u64 ticks_to_destroy = 0;
    size_t num_deletions = 0;

    const auto configure = [&](bool allow_aggressive) {
        pressured = total_used_samplers >= pressure_gc_samplers;
        aggresive = allow_aggressive && total_used_samplers >= critical_gc_samplers;
        ticks_to_destroy = aggresive ? 160 : pressured ? 80 : 16;
        ticks_to_destroy = std::min(ticks_to_destroy, gc_tick);
        num_deletions = aggresive ? 40 : pressured ? 20 : 10;
    };
    const auto clean_up = [&](u64 hash) {
        if (num_deletions == 0) {
            return true;
        }
        --num_deletions;
        const size_t lru_id = samplers.at(hash).lru_id;
        samplers.erase(hash);
        sampler_lru_cache.Free(lru_id);
        return false;
    };

    // Try to remove anything old enough and not high priority.
    configure(false);
    sampler_lru_cache.ForEachItemBelow(gc_tick - ticks_to_destroy, clean_up);

    if (total_used_samplers >= critical_gc_samplers) {
        // If we are still over the critical limit, run an aggressive GC
        configure(true);
        sampler_lru_cache.ForEachItemBelow(gc_tick - ticks_to_destroy, clean_up);
    }
}

void TextureCache::RunGarbageCollector() {
    SCOPE_EXIT {
        ++gc_tick;
    };
    {
        std::scoped_lock lock{mutex};
        ApplyBackingWrites();
    }

    GarbageCollectImages();
    GarbageCollectSamplers();
}

void TextureCache::TouchImage(const Image& image) {
    if (image.lru_touched_tick == gc_tick) {
        return;
    }
    image.lru_touched_tick = gc_tick;
    lru_cache.Touch(image.lru_id, gc_tick);
}

void TextureCache::DeleteImage(ImageId image_id) {
    Image& image = slot_images[image_id];
    // gow3: only this image's bytes. FreeImage resolved its own copies; with ExactReadbackWaits,
    // copies of neighbors that only share a page are still pending and must reach RAM.
    CancelReadbacks(image.info.guest_address, image.info.guest_size, true);
    ASSERT_MSG(!image.IsTracked(), "Image was not untracked");
    ASSERT_MSG(False(image.flags & ImageFlagBits::Registered), "Image was not unregistered");

    const auto alias_it = True(image.flags & ImageFlagBits::Aliased)
                              ? alias_states.find(image.info.guest_address)
                              : alias_states.end();
    if (alias_it != alias_states.end()) {
        AliasState& state = alias_it.value();
        // ponytail: no copy here (DeleteImage also runs on guest threads through UnmapMemory,
        // which must not record commands); the aliases then re-read guest memory.
        if ((state.writer == image_id && state.writer_uid == image.image_uid) ||
            (state.backing == image_id && state.backing_uid == image.image_uid)) {
            state.ResetAuthority();
        }
        if (state.members <= 2) {
            ForEachAlias(image, [](ImageId, Image& candidate) {
                candidate.flags &= ~ImageFlagBits::Aliased;
            });
            alias_states.erase(alias_it);
        } else {
            --state.members;
        }
    }

    // Remove any registered meta areas.
    const auto& meta_info = image.info.meta_info;
    if (meta_info.cmask_addr) {
        surface_metas.erase(meta_info.cmask_addr);
    }
    if (meta_info.fmask_addr) {
        surface_metas.erase(meta_info.fmask_addr);
    }
    if (meta_info.htile_addr) {
        surface_metas.erase(meta_info.htile_addr);
    }

    {
        std::unique_lock lk{download_images_mutex};
        if (download_images.contains(image_id)) {
            download_images.erase(image_id);
        }
    }

    // Reclaim image and any image views it references.
    scheduler.DeferOperation([this, image_id] {
        Image& image = slot_images[image_id];
        for (auto& backing : image.backing_images) {
            for (const ImageViewId image_view_id : backing.image_view_ids) {
                slot_image_views.erase(image_view_id);
            }
        }
        slot_images.erase(image_id);
    });
}

} // namespace VideoCore
