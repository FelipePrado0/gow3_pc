// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: MetalFX upscaling (temporal and spatial) over KosmicKrisp's exported MTLHeaps (macOS,
// vk_metalfx.h).
// Objective-C++ with ARC; the Vulkan side uses the port's Vulkan-Hpp dispatcher.

#include "video_core/renderer_vulkan/vk_metalfx.h"

#import <Metal/Metal.h>
#import <MetalFX/MetalFX.h>
#include <mach/mach_time.h>
#include <pthread.h>

#include <array>
#include <atomic>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <deque>
#include <mutex>
#include <string>
#include <thread>

#include "video_core/renderer_vulkan/vk_instance.h"

namespace Vulkan {

namespace {

// VK_EXT_external_memory_metal's query (VkMemoryGetMetalHandleInfoEXT). vulkan_metal.h is left
// out: under ARC its Metal pointer types are __strong, which Vulkan-Hpp's templates reject.
struct MemoryGetMetalHandleInfo {
    VkStructureType sType;
    const void* pNext;
    VkDeviceMemory memory;
    VkExternalMemoryHandleTypeFlagBits handleType;
};
using GetMemoryMetalHandle = VkResult(VKAPI_PTR*)(VkDevice, const MemoryGetMetalHandleInfo*,
                                                  void**);
constexpr auto MetalHeap =
    static_cast<vk::ExternalMemoryHandleTypeFlagBits>(VK_EXTERNAL_MEMORY_HANDLE_TYPE_MTLHEAP_BIT_EXT);

MTLPixelFormat MetalFormat(vk::Format format) {
    switch (format) {
    case vk::Format::eR16G16B16A16Sfloat:
        return MTLPixelFormatRGBA16Float;
    case vk::Format::eR8G8B8A8Unorm:
        return MTLPixelFormatRGBA8Unorm;
    case vk::Format::eR16G16Sfloat:
        return MTLPixelFormatRG16Float;
    case vk::Format::eD32Sfloat:
        return MTLPixelFormatDepth32Float;
    case vk::Format::eD32SfloatS8Uint:
        return MTLPixelFormatDepth32Float_Stencil8;
    // The display image's view formats (the spatial scaler; KosmicKrisp maps them natively).
    case vk::Format::eR8G8B8A8Srgb:
        return MTLPixelFormatRGBA8Unorm_sRGB;
    case vk::Format::eB8G8R8A8Srgb:
        return MTLPixelFormatBGRA8Unorm_sRGB;
    case vk::Format::eA2R10G10B10UnormPack32:
        return MTLPixelFormatBGR10A2Unorm;
    default:
        return MTLPixelFormatInvalid;
    }
}

/// The texture usage KosmicKrisp gives an optimal 2D image (kk_image_init adds the implied
/// usage, then vk_image_usage_flags_to_mtl_texture_usage): a texture placed in the image's heap
/// needs the image's descriptor to share its layout. Configure also compares the sizes.
MTLTextureUsage KosmicKrispUsage(vk::ImageUsageFlags usage, vk::Format format) {
    using Usage = vk::ImageUsageFlagBits;
    const bool depth = format == vk::Format::eD32Sfloat || format == vk::Format::eD32SfloatS8Uint;
    if (usage & Usage::eTransferSrc) usage |= Usage::eSampled;
    if (usage & Usage::eTransferDst) {
        usage |= depth ? Usage::eDepthStencilAttachment : Usage::eColorAttachment;
    }
    MTLTextureUsage out = MTLTextureUsageUnknown;
    if (usage & (Usage::eTransferDst | Usage::eStorage)) out |= MTLTextureUsageShaderWrite;
    if (usage & (Usage::eTransferSrc | Usage::eSampled | Usage::eInputAttachment | Usage::eStorage)) {
        out |= MTLTextureUsageShaderRead;
    }
    if (usage & (Usage::eColorAttachment | Usage::eDepthStencilAttachment | Usage::eTransferDst)) {
        out |= MTLTextureUsageRenderTarget;
    }
    // KosmicKrisp views the depth plane of D32S8 in another format.
    if (format == vk::Format::eD32SfloatS8Uint) out |= MTLTextureUsagePixelFormatView;
    return out;
}

class MetalFx final : public MetalFxUpscaler {
public:
    MetalFx(const Instance& instance, GetMemoryMetalHandle get_heap_)
        : device{instance.GetDevice()}, physical{instance.GetPhysicalDevice()},
          get_heap{get_heap_} {
        const vk::SemaphoreTypeCreateInfo timeline{.semaphoreType = vk::SemaphoreType::eTimeline};
        semaphore = device.createSemaphore({.pNext = &timeline}).value;
        worker = std::thread{[this] { Work(); }};
    }

    ~MetalFx() override {
        {
            std::scoped_lock lock{mutex};
            stop = true;
        }
        cv.notify_all();
        worker.join(); // after the queued jobs: their Vulkan work is submitted
        WaitIdle();
        Release();
        device.destroySemaphore(semaphore);
    }

    bool Configure(u32 w, u32 h, u32 ow, u32 oh, vk::Format color_format,
                   vk::Format depth_format) override {
        WaitIdle();
        Release();
        failed = false;
        problem.clear();
        if (MetalFormat(color_format) == MTLPixelFormatInvalid ||
            MetalFormat(depth_format) == MTLPixelFormatInvalid) {
            return Fail("unsupported color or depth format");
        }
        using Usage = vk::ImageUsageFlagBits;
        if (!CreateShared(color, "color", color_format, w, h, Usage::eTransferDst | Usage::eSampled) ||
            !CreateShared(depth, "depth", depth_format, w, h, Usage::eTransferDst | Usage::eSampled) ||
            !CreateShared(motion, "motion", vk::Format::eR16G16Sfloat, w, h,
                          Usage::eTransferDst | Usage::eSampled) ||
            !CreateShared(output, "output", vk::Format::eR16G16B16A16Sfloat, ow, oh,
                          Usage::eStorage | Usage::eSampled | Usage::eTransferSrc)) {
            Release();
            return false;
        }
        @autoreleasepool {
            // The device the driver's heaps belong to; one GPU on Apple silicon.
            id<MTLDevice> gpu = color.texture.device;
            const float max_scale = [MTLFXTemporalScalerDescriptor supportedInputContentMaxScaleForDevice:gpu];
            if (ow < w || oh < h || float(ow) > float(w) * max_scale + 0.5f ||
                float(oh) > float(h) * max_scale + 0.5f) {
                Release();
                return Fail("the scale is outside what MetalFX supports on this GPU");
            }
            if (!queue || queue.device != gpu) {
                queue = [gpu newCommandQueue];
                queue.label = @"gow3 MetalFX";
                start = [gpu newSharedEvent];
                start.signaledValue = queued;
            }
            MTLFXTemporalScalerDescriptor* descriptor = [MTLFXTemporalScalerDescriptor new];
            descriptor.colorTextureFormat = MetalFormat(color_format);
            descriptor.depthTextureFormat = MetalFormat(depth_format);
            descriptor.motionTextureFormat = MTLPixelFormatRG16Float;
            descriptor.outputTextureFormat = MTLPixelFormatRGBA16Float;
            descriptor.inputWidth = w;
            descriptor.inputHeight = h;
            descriptor.outputWidth = ow;
            descriptor.outputHeight = oh;
            // HDR scene color: MetalFX measures the exposure itself, as FSR's auto exposure.
            descriptor.autoExposureEnabled = color_format == vk::Format::eR16G16B16A16Sfloat;
            // Created in ~0.1 s either way; without it the first frames take 1-7x as long.
            descriptor.requiresSynchronousInitialization = YES;
            scaler = [descriptor newTemporalScalerWithDevice:gpu];
            if (!scaler) {
                Release();
                return Fail("could not create the temporal scaler");
            }
            if ((color.texture.usage & scaler.colorTextureUsage) != scaler.colorTextureUsage ||
                (depth.texture.usage & scaler.depthTextureUsage) != scaler.depthTextureUsage ||
                (motion.texture.usage & scaler.motionTextureUsage) != scaler.motionTextureUsage) {
                Release();
                return Fail("the shared textures lack the usage MetalFX needs");
            }
            // MetalFX writes private textures only; the blit after it fills the shared output.
            MTLTextureDescriptor* target = [MTLTextureDescriptor
                texture2DDescriptorWithPixelFormat:MTLPixelFormatRGBA16Float
                                             width:ow
                                            height:oh
                                         mipmapped:NO];
            target.storageMode = MTLStorageModePrivate;
            target.usage = scaler.outputTextureUsage;
            private_output = [gpu newTextureWithDescriptor:target];
            scaler.colorTexture = color.texture;
            scaler.depthTexture = depth.texture;
            scaler.motionTexture = motion.texture;
            scaler.outputTexture = private_output;
            scaler.inputContentWidth = w;
            scaler.inputContentHeight = h;
            // CameraMotion's vectors: render pixels from the current to the previous position.
            scaler.motionVectorScaleX = 1.0f;
            scaler.motionVectorScaleY = 1.0f;
            scaler.depthReversed = NO; // D3D depth, 0 at the near plane
            scaler.preExposure = 1.0f;
            std::printf("Upscaler: MetalFX temporal %ux%u -> %ux%u (%s) on %s\n", w, h, ow, oh,
                        descriptor.autoExposureEnabled ? "HDR scene color" : "tonemapped frame",
                        gpu.name.UTF8String);
        }
        config = {w, h, ow, oh, color_format, depth_format};
        return true;
    }

    bool Matches(u32 w, u32 h, u32 ow, u32 oh, vk::Format color_format,
                 vk::Format depth_format) const override {
        return scaler && config.w == w && config.h == h && config.ow == ow && config.oh == oh &&
               config.color == color_format && config.depth == depth_format;
    }

    bool ConfigureSpatial(u32 w, u32 h, u32 ow, u32 oh, vk::Format format) override {
        WaitIdle();
        Release();
        failed = false;
        problem.clear();
        const MTLPixelFormat pixel_format = MetalFormat(format);
        if (pixel_format == MTLPixelFormatInvalid) {
            return Fail("unsupported image format");
        }
        if (ow < w || oh < h) {
            return Fail("the window is smaller than the image");
        }
        using Usage = vk::ImageUsageFlagBits;
        // Read by the presenter's passes as the display image is: alpha as 1.
        const vk::ComponentMapping opaque{.a = vk::ComponentSwizzle::eOne};
        if (!CreateShared(color, "image", format, w, h, Usage::eTransferDst | Usage::eSampled) ||
            !CreateShared(output, "enlarged image", format, ow, oh, Usage::eSampled, opaque)) {
            Release();
            return false;
        }
        @autoreleasepool {
            id<MTLDevice> gpu = color.texture.device;
            if (!queue || queue.device != gpu) {
                queue = [gpu newCommandQueue];
                queue.label = @"gow3 MetalFX";
                start = [gpu newSharedEvent];
                start.signaledValue = queued;
            }
            MTLFXSpatialScalerDescriptor* descriptor = [MTLFXSpatialScalerDescriptor new];
            descriptor.colorTextureFormat = pixel_format;
            descriptor.outputTextureFormat = pixel_format; // MetalFX keeps sRGB in and out alike
            descriptor.inputWidth = w;
            descriptor.inputHeight = h;
            descriptor.outputWidth = ow;
            descriptor.outputHeight = oh;
            // The game's finished image, gamma encoded. On an sRGB texture MetalFX accepts only
            // this mode, and its result matches the same bytes upscaled as UNORM (checked).
            descriptor.colorProcessingMode = MTLFXSpatialScalerColorProcessingModePerceptual;
            spatial = [descriptor newSpatialScalerWithDevice:gpu];
            if (!spatial) {
                Release();
                return Fail("could not create the spatial scaler");
            }
            if ((color.texture.usage & spatial.colorTextureUsage) != spatial.colorTextureUsage) {
                Release();
                return Fail("the shared texture lacks the usage MetalFX needs");
            }
            // As for the temporal scaler: a private target, blitted into the shared output.
            MTLTextureDescriptor* target =
                [MTLTextureDescriptor texture2DDescriptorWithPixelFormat:pixel_format
                                                                   width:ow
                                                                  height:oh
                                                               mipmapped:NO];
            target.storageMode = MTLStorageModePrivate;
            target.usage = spatial.outputTextureUsage;
            private_output = [gpu newTextureWithDescriptor:target];
            spatial.colorTexture = color.texture;
            spatial.outputTexture = private_output;
            spatial.inputContentWidth = w;
            spatial.inputContentHeight = h;
            std::printf("Screen upscaler: MetalFX spatial %ux%u -> %ux%u (%s) on %s\n", w, h, ow,
                        oh, vk::to_string(format).c_str(), gpu.name.UTF8String);
        }
        config = {w, h, ow, oh, format, vk::Format::eUndefined};
        return true;
    }

    bool MatchesSpatial(u32 w, u32 h, u32 ow, u32 oh, vk::Format format) const override {
        return spatial && config.w == w && config.h == h && config.ow == ow && config.oh == oh &&
               config.color == format;
    }

    Image Color() const override {
        return {color.image, color.view};
    }
    Image Depth() const override {
        return {depth.image, depth.view};
    }
    Image Motion() const override {
        return {motion.image, motion.view};
    }
    Image Output() const override {
        return {output.image, output.view};
    }

    u64 Submit(vk::Semaphore timeline, u64 tick, const Frame& frame) override {
        u64 value;
        {
            std::scoped_lock lock{mutex};
            value = ++queued;
        }
        if (!queue) { // never configured: no Metal work can be pending either
            failed = true;
            Signal(value);
            return value;
        }
        // Encoded and committed here, behind a wait for `start`: once Vulkan has written the
        // inputs the worker only releases it, so the GPU waits for that thread's wake-up alone.
        @autoreleasepool {
            id<MTLCommandBuffer> command = [queue commandBuffer];
            [command encodeWaitForEvent:start value:value];
            if (scaler) {
                scaler.jitterOffsetX = frame.jitter_x;
                scaler.jitterOffsetY = frame.jitter_y;
                scaler.reset = frame.reset;
                [scaler encodeToCommandBuffer:command];
                id<MTLBlitCommandEncoder> blit = [command blitCommandEncoder];
                [blit copyFromTexture:private_output toTexture:output.texture];
                [blit endEncoding];
            } else if (spatial) {
                [spatial encodeToCommandBuffer:command];
                id<MTLBlitCommandEncoder> blit = [command blitCommandEncoder];
                [blit copyFromTexture:private_output toTexture:output.texture];
                [blit endEncoding];
            } else {
                failed = true; // not configured: the command buffer only keeps the order
            }
            [command addCompletedHandler:^(id<MTLCommandBuffer> done) {
              if (done.status != MTLCommandBufferStatusCompleted) {
                  std::printf("MetalFX: upscale %llu failed: %s\n",
                              static_cast<unsigned long long>(value),
                              done.error.localizedDescription.UTF8String);
                  failed = true;
              }
              Signal(value);
              if (frame_stats) {
                  Count(done.GPUStartTime - released[value % released.size()],
                        done.GPUEndTime - done.GPUStartTime, HostSeconds() - done.GPUEndTime);
              }
            }];
            [command commit];
        }
        {
            std::scoped_lock lock{mutex};
            jobs.push_back({timeline, tick, value});
        }
        cv.notify_one();
        return value;
    }

    vk::Semaphore Semaphore() const override {
        return semaphore;
    }

    void WaitIdle() override {
        u64 target;
        {
            std::scoped_lock lock{mutex};
            target = queued;
        }
        const vk::SemaphoreWaitInfo wait{
            .semaphoreCount = 1, .pSemaphores = &semaphore, .pValues = &target};
        while (target && device.waitSemaphores(wait, 2'000'000'000) == vk::Result::eTimeout) {
            std::printf("MetalFX: still waiting for upscale %llu\n",
                        static_cast<unsigned long long>(target));
        }
    }

    bool Failed() const override {
        return failed;
    }

    const char* Problem() const override {
        return problem.c_str();
    }

private:
    struct Shared {
        vk::Image image;
        vk::DeviceMemory memory;
        vk::ImageView view;
        id<MTLTexture> texture;
    };
    struct Job {
        vk::Semaphore timeline;
        u64 tick, value;
    };

    bool Fail(const std::string& reason) {
        problem = reason;
        return false;
    }

    /// A Vulkan image with memory of its own, exported as an MTLHeap, and the texture at the
    /// start of that heap with the image's descriptor.
    bool CreateShared(Shared& out, const char* name, vk::Format format, u32 w, u32 h,
                      vk::ImageUsageFlags usage, vk::ComponentMapping components = {}) {
        const bool is_depth =
            format == vk::Format::eD32Sfloat || format == vk::Format::eD32SfloatS8Uint;
        const vk::ExternalMemoryImageCreateInfo external{
            .handleTypes = MetalHeap};
        auto image = device.createImage({
            .pNext = &external,
            .imageType = vk::ImageType::e2D,
            .format = format,
            .extent = {w, h, 1},
            .mipLevels = 1,
            .arrayLayers = 1,
            .samples = vk::SampleCountFlagBits::e1,
            .tiling = vk::ImageTiling::eOptimal,
            .usage = usage,
            .initialLayout = vk::ImageLayout::eUndefined,
        });
        if (image.result != vk::Result::eSuccess) {
            return Fail(std::string{"cannot create the shared "} + name + " image");
        }
        out.image = image.value;
        const auto requirements = device.getImageMemoryRequirements(out.image);
        const auto types = physical.getMemoryProperties();
        u32 type = 0;
        while (type < types.memoryTypeCount &&
               (!(requirements.memoryTypeBits & (1u << type)) ||
                !(types.memoryTypes[type].propertyFlags & vk::MemoryPropertyFlagBits::eDeviceLocal))) {
            ++type;
        }
        if (type == types.memoryTypeCount) {
            return Fail(std::string{"no device-local memory type for "} + name);
        }
        const vk::MemoryDedicatedAllocateInfo dedicated{.image = out.image};
        const vk::ExportMemoryAllocateInfo exported{
            .pNext = &dedicated, .handleTypes = MetalHeap};
        auto memory = device.allocateMemory({
            .pNext = &exported,
            .allocationSize = requirements.size,
            .memoryTypeIndex = type,
        });
        if (memory.result != vk::Result::eSuccess) {
            return Fail(std::string{"cannot allocate exportable memory for "} + name);
        }
        out.memory = memory.value;
        if (device.bindImageMemory(out.image, out.memory, 0) != vk::Result::eSuccess) {
            return Fail(std::string{"cannot bind the memory of "} + name);
        }
        const MemoryGetMetalHandleInfo get{
            .sType = VK_STRUCTURE_TYPE_MEMORY_GET_METAL_HANDLE_INFO_EXT,
            .memory = static_cast<VkDeviceMemory>(out.memory),
            .handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_MTLHEAP_BIT_EXT,
        };
        void* handle = nullptr;
        if (get_heap(static_cast<VkDevice>(device), &get, &handle) != VK_SUCCESS || !handle) {
            return Fail(std::string{"the driver does not export the heap of "} + name);
        }
        id<MTLHeap> heap = (__bridge id<MTLHeap>)handle; // lives as long as the memory
        if (heap.type != MTLHeapTypePlacement) {
            return Fail("the driver's heaps are not placement heaps");
        }
        MTLTextureDescriptor* descriptor = [MTLTextureDescriptor new];
        descriptor.textureType = MTLTextureType2D;
        descriptor.pixelFormat = MetalFormat(format);
        descriptor.width = w;
        descriptor.height = h;
        descriptor.allowGPUOptimizedContents = YES;
        descriptor.usage = KosmicKrispUsage(usage, format);
        descriptor.resourceOptions = heap.resourceOptions;
        const MTLSizeAndAlign size = [heap.device heapTextureSizeAndAlignWithDescriptor:descriptor];
        if (size.size != requirements.size) {
            char text[160];
            std::snprintf(text, sizeof(text),
                          "the Metal texture of %s does not match the Vulkan image (%llu vs %llu "
                          "bytes)",
                          name, static_cast<unsigned long long>(size.size),
                          static_cast<unsigned long long>(requirements.size));
            return Fail(text);
        }
        out.texture = [heap newTextureWithDescriptor:descriptor offset:0];
        if (!out.texture) {
            return Fail(std::string{"cannot place the texture of "} + name + " in its heap");
        }
        out.texture.label = @(name);
        auto view = device.createImageView({
            .image = out.image,
            .viewType = vk::ImageViewType::e2D,
            .format = format,
            .components = components,
            .subresourceRange = {is_depth ? vk::ImageAspectFlagBits::eDepth
                                          : vk::ImageAspectFlagBits::eColor,
                                 0, 1, 0, 1},
        });
        if (view.result != vk::Result::eSuccess) {
            return Fail(std::string{"cannot create the view of "} + name);
        }
        out.view = view.value;
        return true;
    }

    void Release() {
        scaler = nil;
        spatial = nil;
        private_output = nil;
        for (Shared* shared : {&color, &depth, &motion, &output}) {
            shared->texture = nil; // before the memory that holds its heap
            if (shared->view) device.destroyImageView(shared->view);
            if (shared->image) device.destroyImage(shared->image);
            if (shared->memory) device.freeMemory(shared->memory);
            *shared = {};
        }
        config = {};
    }

    /// Completion handlers can run on any thread and out of order: the timeline only moves up.
    void Signal(u64 value) {
        std::scoped_lock lock{signal_mutex};
        if (value <= signaled) return;
        signaled = value;
        if (device.signalSemaphore({.semaphore = semaphore, .value = value}) !=
            vk::Result::eSuccess) {
            std::printf("MetalFX: cannot signal upscale %llu\n",
                        static_cast<unsigned long long>(value));
        }
    }

    void Work() {
        pthread_setname_np("gow3:MetalFX");
        // The GPU idles between the inputs and the upscale: wake up first.
        pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0);
        std::unique_lock lock{mutex};
        for (;;) {
            cv.wait(lock, [this] { return stop || !jobs.empty(); });
            if (jobs.empty()) return;
            const Job job = jobs.front();
            jobs.pop_front();
            lock.unlock();
            Start(job);
            lock.lock();
        }
    }

    /// Releases the committed upscale once the Vulkan submission that wrote its inputs has
    /// completed (KosmicKrisp signals its timeline after that Metal command buffer).
    void Start(const Job& job) {
        const vk::SemaphoreWaitInfo wait{
            .semaphoreCount = 1, .pSemaphores = &job.timeline, .pValues = &job.tick};
        vk::Result result;
        while ((result = device.waitSemaphores(wait, 2'000'000'000)) == vk::Result::eTimeout) {
        }
        if (result != vk::Result::eSuccess) {
            // Run it anyway on whatever the inputs hold: the Vulkan queue waits for it.
            std::printf("MetalFX: inputs of upscale %llu not confirmed (%s)\n",
                        static_cast<unsigned long long>(job.value), vk::to_string(result).c_str());
            failed = true;
        }
        released[job.value % released.size()] = HostSeconds();
        start.signaledValue = job.value;
    }

    /// Seconds on the clock of GPUStartTime and GPUEndTime (mach absolute time).
    static double HostSeconds() {
        static const double scale = [] {
            mach_timebase_info_data_t info;
            mach_timebase_info(&info);
            return double(info.numer) / double(info.denom) * 1e-9;
        }();
        return double(mach_absolute_time()) * scale;
    }

    /// GOW3_FRAME_STATS=1: every 5 s, the upscales' GPU time and the gaps the relay leaves around
    /// them (upscale released -> MetalFX starts on the GPU, MetalFX done -> Vulkan signalled).
    void Count(double to_start, double gpu, double to_signal) {
        std::scoped_lock lock{stats_mutex};
        const double now = HostSeconds();
        if (!stats.count) stats.since = now;
        ++stats.count;
        stats.to_start += to_start;
        stats.gpu += gpu;
        stats.to_signal += to_signal;
        if (now - stats.since >= 5.0) {
            const double ms = 1e3 / stats.count;
            std::printf("MetalFX: %u upscales in %.1f s, GPU %.2f ms each; relay %.2f ms to "
                        "start, %.2f ms to signal\n",
                        stats.count, now - stats.since, stats.gpu * ms, stats.to_start * ms,
                        stats.to_signal * ms);
            stats = {};
        }
    }

    vk::Device device;
    vk::PhysicalDevice physical;
    GetMemoryMetalHandle get_heap;
    id<MTLCommandQueue> queue;
    id<MTLSharedEvent> start; ///< upscale n may start at value n (inputs written)
    std::array<std::atomic<double>, 8> released{}; ///< when upscale n was started (stats)
    id<MTLFXTemporalScaler> scaler;
    id<MTLFXSpatialScaler> spatial; ///< instead of `scaler` (ConfigureSpatial)
    id<MTLTexture> private_output;
    Shared color{}, depth{}, motion{}, output{};
    struct {
        u32 w, h, ow, oh;
        vk::Format color, depth;
    } config{};
    std::string problem;
    std::atomic<bool> failed{false};
    const bool frame_stats = std::getenv("GOW3_FRAME_STATS") != nullptr;
    std::mutex stats_mutex;
    struct {
        u32 count;
        double since, to_start, gpu, to_signal;
    } stats{};

    vk::Semaphore semaphore; ///< timeline, host-signalled when an upscale has completed
    std::mutex signal_mutex;
    u64 signaled = 0;
    std::mutex mutex;
    std::condition_variable cv;
    std::deque<Job> jobs;
    u64 queued = 0;
    bool stop = false;
    std::thread worker;
};

} // namespace

std::unique_ptr<MetalFxUpscaler> MetalFxUpscaler::Create(const Instance& instance,
                                                         std::string& problem, Kind kind) {
    if (!instance.IsExternalMemoryMetalSupported()) {
        problem = "the Vulkan driver lacks VK_EXT_external_memory_metal";
        return nullptr;
    }
    const auto get_heap = reinterpret_cast<GetMemoryMetalHandle>(
        instance.GetDevice().getProcAddr("vkGetMemoryMetalHandleEXT"));
    if (!get_heap) {
        problem = "vkGetMemoryMetalHandleEXT is missing";
        return nullptr;
    }
    @autoreleasepool {
        id<MTLDevice> metal = MTLCreateSystemDefaultDevice();
        if (!metal) {
            problem = "no Metal device";
            return nullptr;
        }
        if (kind == Spatial) {
            if (![MTLFXSpatialScalerDescriptor supportsDevice:metal]) {
                problem = std::string{"MetalFX spatial scaling does not support "} +
                          metal.name.UTF8String;
                return nullptr;
            }
            std::printf("Screen upscaler: MetalFX available on %s\n", metal.name.UTF8String);
            return std::make_unique<MetalFx>(instance, get_heap);
        }
        if (![MTLFXTemporalScalerDescriptor supportsDevice:metal]) {
            problem = std::string{"MetalFX temporal scaling does not support "} +
                      metal.name.UTF8String;
            return nullptr;
        }
        std::printf("Upscaler: MetalFX available on %s (scale %.1f to %.1f)\n",
                    metal.name.UTF8String,
                    [MTLFXTemporalScalerDescriptor supportedInputContentMinScaleForDevice:metal],
                    [MTLFXTemporalScalerDescriptor supportedInputContentMaxScaleForDevice:metal]);
        return std::make_unique<MetalFx>(instance, get_heap);
    }
}

} // namespace Vulkan
