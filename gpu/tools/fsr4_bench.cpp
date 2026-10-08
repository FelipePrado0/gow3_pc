// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: FSR 4 benchmark outside the game. Runs the v07 INT8 provider (the same assets and
// provider as vk_fsr4.cpp) on synthetic inputs and prints GPU time per pass (GOW3_FSR4_PROFILE)
// and, with --stats, the driver's statistics of every pass (GOW3_FSR4_STATS).
//
//   fsr4-bench [render WxH] [output WxH] [preset 0-4] [frames] [--stats] [--fsr411]
// --fsr411: FSR 4.1.1 replay (fsr411.cpp, assets in GOW3_FSR411_DIR or fsr4_411) instead of v07;
// its frames match tools/fsr4cap (jitter phase, reset on the first frame).
//   defaults: 2260x1272 3840x2160 2 (balanced) 900
// BENCH_NOISE=1: pseudo-random inputs (a fixed seed); BENCH_DUMP=<file>: the output after the
// last frame, raw RGBA16F, for comparing shader variants.
// Assets: GOW3_FSR4_DIR or fsr4_shaders in the working directory.

#include <array>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <memory>
#include <string>
#include <vector>

#include <vulkan/vulkan.h>

#include "ffx_vk_fsr4_v07.h"
#include "ffx_vk_fsr4_v07_assets.h"
#include "fsr411.h"

namespace {

#define CHECK(call)                                                                            \
    do {                                                                                       \
        const VkResult r_ = (call);                                                            \
        if (r_ != VK_SUCCESS) {                                                                \
            std::fprintf(stderr, "%s failed: %d\n", #call, int(r_));                           \
            std::exit(1);                                                                      \
        }                                                                                      \
    } while (0)

bool ReadFile(const std::string& path, std::vector<unsigned char>& data) {
    std::ifstream file(path, std::ios::binary | std::ios::ate);
    if (!file) {
        return false;
    }
    data.resize(size_t(file.tellg()));
    file.seekg(0);
    return bool(file.read(reinterpret_cast<char*>(data.data()), std::streamsize(data.size())));
}

struct Gpu {
    VkInstance instance{};
    VkPhysicalDevice physical{};
    VkDevice device{};
    VkQueue queue{};
    uint32_t family = 0;
    VkPhysicalDeviceMemoryProperties memory{};
};

uint32_t MemoryType(const Gpu& gpu, uint32_t bits, VkMemoryPropertyFlags flags) {
    for (uint32_t i = 0; i < gpu.memory.memoryTypeCount; ++i) {
        if ((bits & (1u << i)) && (gpu.memory.memoryTypes[i].propertyFlags & flags) == flags) {
            return i;
        }
    }
    std::fprintf(stderr, "no memory type\n");
    std::exit(1);
}

Gpu CreateGpu(bool stats) {
    Gpu gpu;
    VkApplicationInfo app{VK_STRUCTURE_TYPE_APPLICATION_INFO};
    app.pApplicationName = "fsr4-bench";
    app.apiVersion = VK_API_VERSION_1_3;
    VkInstanceCreateInfo ici{VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO};
    ici.pApplicationInfo = &app;
    CHECK(vkCreateInstance(&ici, nullptr, &gpu.instance));
    uint32_t count = 0;
    CHECK(vkEnumeratePhysicalDevices(gpu.instance, &count, nullptr));
    std::vector<VkPhysicalDevice> devices(count);
    CHECK(vkEnumeratePhysicalDevices(gpu.instance, &count, devices.data()));
    gpu.physical = devices.at(0);
    for (const auto device : devices) {
        VkPhysicalDeviceProperties props;
        vkGetPhysicalDeviceProperties(device, &props);
        if (props.deviceType == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU) {
            gpu.physical = device;
            break;
        }
    }
    VkPhysicalDeviceProperties props;
    vkGetPhysicalDeviceProperties(gpu.physical, &props);
    std::printf("GPU: %s\n", props.deviceName);
    vkGetPhysicalDeviceMemoryProperties(gpu.physical, &gpu.memory);

    uint32_t families = 0;
    vkGetPhysicalDeviceQueueFamilyProperties(gpu.physical, &families, nullptr);
    std::vector<VkQueueFamilyProperties> family_props(families);
    vkGetPhysicalDeviceQueueFamilyProperties(gpu.physical, &families, family_props.data());
    while (gpu.family < families && !(family_props[gpu.family].queueFlags & VK_QUEUE_COMPUTE_BIT)) {
        ++gpu.family;
    }

    // Everything the device supports, as the game's device enables what FSR 4 needs.
    VkPhysicalDevicePipelineExecutablePropertiesFeaturesKHR executable{
        VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PIPELINE_EXECUTABLE_PROPERTIES_FEATURES_KHR};
    VkPhysicalDeviceComputeShaderDerivativesFeaturesKHR derivatives{
        VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_COMPUTE_SHADER_DERIVATIVES_FEATURES_KHR};
    // Cooperative matrix (WMMA) for experimental model passes, when the device has it.
    VkPhysicalDeviceCooperativeMatrixFeaturesKHR coopmat{
        VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_COOPERATIVE_MATRIX_FEATURES_KHR};
    // FSR 4.1.1 passes use mixed float dot products (dot2 of halves into float), as vkd3d-proton.
    VkPhysicalDeviceShaderMixedFloatDotProductFeaturesVALVE mixed_dot{
        VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_SHADER_MIXED_FLOAT_DOT_PRODUCT_FEATURES_VALVE};
    VkPhysicalDeviceVulkan13Features f13{VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_3_FEATURES};
    VkPhysicalDeviceVulkan12Features f12{VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_2_FEATURES};
    VkPhysicalDeviceVulkan11Features f11{VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_VULKAN_1_1_FEATURES};
    VkPhysicalDeviceFeatures2 features{VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_FEATURES_2};
    features.pNext = &f11;
    f11.pNext = &f12;
    f12.pNext = &f13;
    f13.pNext = &derivatives;
    derivatives.pNext = &coopmat;
    coopmat.pNext = &mixed_dot;
    if (stats) {
        mixed_dot.pNext = &executable;
    }
    vkGetPhysicalDeviceFeatures2(gpu.physical, &features);
    features.features.robustBufferAccess = VK_FALSE; // as the game: no robustness cost
    f13.robustImageAccess = VK_FALSE;
    std::vector<const char*> extensions{VK_KHR_COMPUTE_SHADER_DERIVATIVES_EXTENSION_NAME};
    extensions.push_back(VK_KHR_PUSH_DESCRIPTOR_EXTENSION_NAME);
    if (mixed_dot.shaderMixedFloatDotProductFloat16AccFloat32) {
        extensions.push_back(VK_VALVE_SHADER_MIXED_FLOAT_DOT_PRODUCT_EXTENSION_NAME);
    }
    if (coopmat.cooperativeMatrix) {
        extensions.push_back(VK_KHR_COOPERATIVE_MATRIX_EXTENSION_NAME);
    }
    if (stats) {
        extensions.push_back(VK_KHR_PIPELINE_EXECUTABLE_PROPERTIES_EXTENSION_NAME);
    }
    const float priority = 1.0f;
    VkDeviceQueueCreateInfo qci{VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO};
    qci.queueFamilyIndex = gpu.family;
    qci.queueCount = 1;
    qci.pQueuePriorities = &priority;
    VkDeviceCreateInfo dci{VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO};
    dci.pNext = &features;
    dci.queueCreateInfoCount = 1;
    dci.pQueueCreateInfos = &qci;
    dci.enabledExtensionCount = uint32_t(extensions.size());
    dci.ppEnabledExtensionNames = extensions.data();
    CHECK(vkCreateDevice(gpu.physical, &dci, nullptr, &gpu.device));
    vkGetDeviceQueue(gpu.device, gpu.family, 0, &gpu.queue);
    return gpu;
}

struct Buffer {
    VkBuffer buffer{};
    VkDeviceMemory memory{};
    void* data = nullptr;
};

Buffer CreateHostBuffer(const Gpu& gpu, VkDeviceSize size, VkBufferUsageFlags usage) {
    Buffer b;
    VkBufferCreateInfo ci{VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO};
    ci.size = size;
    ci.usage = usage;
    CHECK(vkCreateBuffer(gpu.device, &ci, nullptr, &b.buffer));
    VkMemoryRequirements req;
    vkGetBufferMemoryRequirements(gpu.device, b.buffer, &req);
    VkMemoryAllocateInfo ai{VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
    ai.allocationSize = req.size;
    ai.memoryTypeIndex = MemoryType(gpu, req.memoryTypeBits,
                                    VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT |
                                        VK_MEMORY_PROPERTY_HOST_COHERENT_BIT);
    CHECK(vkAllocateMemory(gpu.device, &ai, nullptr, &b.memory));
    CHECK(vkBindBufferMemory(gpu.device, b.buffer, b.memory, 0));
    CHECK(vkMapMemory(gpu.device, b.memory, 0, VK_WHOLE_SIZE, 0, &b.data));
    return b;
}

uint16_t Half(float f) {
    // Inputs in [0, 2): exact enough for test data (truncating conversion).
    uint32_t x;
    std::memcpy(&x, &f, 4);
    const uint32_t sign = (x >> 16) & 0x8000u;
    const int exp = int((x >> 23) & 0xffu) - 127 + 15;
    if (exp <= 0) {
        return uint16_t(sign);
    }
    return uint16_t(sign | (uint32_t(exp) << 10) | ((x >> 13) & 0x3ffu));
}

struct Image {
    VkImage image{};
    VkImageView view{};
    VkDeviceMemory memory{};
    uint32_t width = 0, height = 0;
};

Image CreateImage(const Gpu& gpu, VkFormat format, uint32_t w, uint32_t h, VkImageUsageFlags usage,
                  VkImageAspectFlags aspect) {
    Image image;
    image.width = w;
    image.height = h;
    VkImageCreateInfo ci{VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO};
    ci.imageType = VK_IMAGE_TYPE_2D;
    ci.format = format;
    ci.extent = {w, h, 1};
    ci.mipLevels = 1;
    ci.arrayLayers = 1;
    ci.samples = VK_SAMPLE_COUNT_1_BIT;
    ci.tiling = VK_IMAGE_TILING_OPTIMAL;
    ci.usage = usage;
    CHECK(vkCreateImage(gpu.device, &ci, nullptr, &image.image));
    VkMemoryRequirements req;
    vkGetImageMemoryRequirements(gpu.device, image.image, &req);
    VkMemoryAllocateInfo ai{VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
    ai.allocationSize = req.size;
    ai.memoryTypeIndex = MemoryType(gpu, req.memoryTypeBits, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT);
    CHECK(vkAllocateMemory(gpu.device, &ai, nullptr, &image.memory));
    CHECK(vkBindImageMemory(gpu.device, image.image, image.memory, 0));
    VkImageViewCreateInfo vi{VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO};
    vi.image = image.image;
    vi.viewType = VK_IMAGE_VIEW_TYPE_2D;
    vi.format = format;
    vi.subresourceRange = {aspect, 0, 1, 0, 1};
    CHECK(vkCreateImageView(gpu.device, &vi, nullptr, &image.view));
    return image;
}

FfxFsr4ModelPreset ModelPreset(int preset) {
    switch (preset) {
    case 0:
        return FFX_FSR4_MODEL_PRESET_NATIVE_AA;
    case 1:
        return FFX_FSR4_MODEL_PRESET_QUALITY;
    case 2:
        return FFX_FSR4_MODEL_PRESET_BALANCED;
    case 3:
        return FFX_FSR4_MODEL_PRESET_PERFORMANCE;
    default:
        return FFX_FSR4_MODEL_PRESET_ULTRA_PERFORMANCE;
    }
}

} // namespace

int main(int argc, char** argv) {
    uint32_t rw = 2260, rh = 1272, ow = 3840, oh = 2160;
    int preset = 2, frames = 900;
    bool stats = false;
    bool fsr411 = false;
    int positional = 0;
    for (int i = 1; i < argc; ++i) {
        if (std::strcmp(argv[i], "--fsr411") == 0) {
            fsr411 = true;
            continue;
        }
        if (std::strcmp(argv[i], "--stats") == 0) {
            stats = true;
            continue;
        }
        switch (positional++) {
        case 0:
            std::sscanf(argv[i], "%ux%u", &rw, &rh);
            break;
        case 1:
            std::sscanf(argv[i], "%ux%u", &ow, &oh);
            break;
        case 2:
            preset = std::atoi(argv[i]);
            break;
        case 3:
            frames = std::atoi(argv[i]);
            break;
        }
    }
    setenv("GOW3_FSR4_PROFILE", "1", 0);
    if (stats) {
        setenv("GOW3_FSR4_STATS", "1", 1);
    }
    const Gpu gpu = CreateGpu(stats);

    FfxInterface backend{};
    ffxContext context{};
    std::vector<unsigned char> scratch;
    if (!fsr411) {
    // Assets, as vk_fsr4.cpp loads them.
    FfxFsr4V07AssetSet assets{};
    if (!ffxFsr4V07BuildAssetSet(ModelPreset(preset), ow, oh, &assets)) {
        std::fprintf(stderr, "unsupported output size\n");
        return 1;
    }
    const char* dir_env = std::getenv("GOW3_FSR4_DIR");
    const std::string dir = std::string{dir_env && dir_env[0] ? dir_env : "fsr4_shaders"} + "/";
    std::array<std::vector<unsigned char>, FFX_FSR4_VK_PASS_COUNT> code;
    std::vector<unsigned char> initializer, weights;
    // As vk_fsr4.cpp: passes from opt/ (tools/fsr4_optimize.sh) unless GOW3_FSR4_OPT=0.
    const char* opt_env = std::getenv("GOW3_FSR4_OPT");
    const bool use_opt = !(opt_env && opt_env[0] == '0');
    int optimized = 0;
    const auto load = [&](const char* name, std::vector<unsigned char>& data) {
        if (use_opt && ReadFile(dir + "opt/" + name, data)) {
            ++optimized;
            return;
        }
        if (!ReadFile(dir + name, data)) {
            std::fprintf(stderr, "missing %s%s\n", dir.c_str(), name);
            std::exit(1);
        }
    };
    load(assets.pre, code[0]);
    for (uint32_t pass = 0; pass < FFX_FSR4_MODEL_PASS_COUNT; ++pass) {
        load(assets.model[pass], code[1 + pass]);
    }
    load(assets.post, code[13]);
    load(assets.rcas, code[14]);
    load(assets.spdAutoExposure, code[15]);
    load(assets.initializer, initializer);
    load(assets.prePassWeights, weights);
    std::printf("%d passes from %sopt/\n", optimized, dir.c_str());
    std::array<std::string, FFX_FSR4_VK_PASS_COUNT> entries;
    entries.fill("main");
    for (uint32_t pass = 1; pass <= FFX_FSR4_MODEL_PASS_COUNT; ++pass) {
        entries[pass] = "fsr4_model_v07_i8_pass" + std::to_string(pass);
    }
    FfxFsr4VkCreateInfo ci{};
    ci.device = gpu.device;
    ci.physicalDevice = gpu.physical;
    for (uint32_t i = 0; i < FFX_FSR4_VK_PASS_COUNT; ++i) {
        ci.shaders[i] = {reinterpret_cast<const uint32_t*>(code[i].data()), code[i].size(),
                         entries[i].c_str()};
    }
    ci.modelInitializer = initializer.data();
    ci.modelInitializerSize = initializer.size();
    ci.prePassWeights = weights.data();
    ci.prePassWeightsSize = weights.size();
    scratch.assign(ffxFsr4VkGetScratchMemorySize(), 0);
    ci.scratchBuffer = scratch.data();
    ci.scratchBufferSize = scratch.size();
    CHECK(ffxFsr4VkCreateContext(&ci, &backend));

    ffxCreateContextDescUpscale desc{};
    desc.header.type = FFX_API_CREATE_CONTEXT_DESC_TYPE_UPSCALE;
    desc.flags = FFX_UPSCALE_ENABLE_HIGH_DYNAMIC_RANGE | FFX_UPSCALE_ENABLE_AUTO_EXPOSURE;
    desc.maxRenderSize = {(ow + 7) & ~7u, (oh + 7) & ~7u};
    desc.maxUpscaleSize = {(ow + 7) & ~7u, (oh + 7) & ~7u};
    ffxFsr4V07SetBackendInterface(&backend);
    if (ffxFsr4V07CreateContext(&context, &desc.header, nullptr) != FFX_API_RETURN_OK) {
        std::fprintf(stderr, "provider context creation failed\n");
        return 1;
    }
    ffxFsr4V07SetBackendInterface(nullptr);
    std::printf("FSR 4 %s %ux%u -> %ux%u, %d frames\n",
                ffxFsr4ModelPresetName(ModelPreset(preset)), rw, rh, ow, oh, frames);
    }

    const VkImageUsageFlags rw_usage = VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_STORAGE_BIT |
                                       VK_IMAGE_USAGE_TRANSFER_DST_BIT;
    Image color = CreateImage(gpu, VK_FORMAT_R16G16B16A16_SFLOAT, rw, rh, rw_usage,
                              VK_IMAGE_ASPECT_COLOR_BIT);
    Image depth = CreateImage(gpu, VK_FORMAT_D32_SFLOAT, rw, rh,
                              VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT,
                              VK_IMAGE_ASPECT_DEPTH_BIT);
    Image motion = CreateImage(gpu, VK_FORMAT_R16G16_SFLOAT, rw, rh, rw_usage,
                               VK_IMAGE_ASPECT_COLOR_BIT);
    Image output = CreateImage(gpu, VK_FORMAT_R16G16B16A16_SFLOAT, ow, oh, rw_usage,
                               VK_IMAGE_ASPECT_COLOR_BIT);

    VkCommandPoolCreateInfo pci{VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO};
    pci.flags = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT;
    pci.queueFamilyIndex = gpu.family;
    VkCommandPool pool;
    CHECK(vkCreateCommandPool(gpu.device, &pci, nullptr, &pool));
    VkCommandBufferAllocateInfo cai{VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO};
    cai.commandPool = pool;
    cai.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
    cai.commandBufferCount = 1;
    VkCommandBuffer cmd;
    CHECK(vkAllocateCommandBuffers(gpu.device, &cai, &cmd));
    VkFenceCreateInfo fci{VK_STRUCTURE_TYPE_FENCE_CREATE_INFO};
    VkFence fence;
    CHECK(vkCreateFence(gpu.device, &fci, nullptr, &fence));

    const auto submit = [&] {
        CHECK(vkEndCommandBuffer(cmd));
        VkSubmitInfo si{VK_STRUCTURE_TYPE_SUBMIT_INFO};
        si.commandBufferCount = 1;
        si.pCommandBuffers = &cmd;
        CHECK(vkQueueSubmit(gpu.queue, 1, &si, fence));
        CHECK(vkWaitForFences(gpu.device, 1, &fence, VK_TRUE, UINT64_MAX));
        CHECK(vkResetFences(gpu.device, 1, &fence));
    };
    const VkCommandBufferBeginInfo begin{VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};

    // Inputs: a mid-grey frame, far depth, small motion; all in General.
    CHECK(vkBeginCommandBuffer(cmd, &begin));
    const auto to_general = [&](const Image& image, VkImageAspectFlags aspect) {
        VkImageMemoryBarrier b{VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER};
        b.dstAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
        b.oldLayout = VK_IMAGE_LAYOUT_UNDEFINED;
        b.newLayout = VK_IMAGE_LAYOUT_GENERAL;
        b.srcQueueFamilyIndex = b.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
        b.image = image.image;
        b.subresourceRange = {aspect, 0, 1, 0, 1};
        vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT,
                             VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 0, nullptr, 0, nullptr, 1, &b);
    };
    to_general(color, VK_IMAGE_ASPECT_COLOR_BIT);
    to_general(depth, VK_IMAGE_ASPECT_DEPTH_BIT);
    to_general(motion, VK_IMAGE_ASPECT_COLOR_BIT);
    to_general(output, VK_IMAGE_ASPECT_COLOR_BIT);
    const VkImageSubresourceRange color_range{VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
    const VkClearColorValue grey{{0.4f, 0.35f, 0.3f, 1.0f}}, mv{{0.25f, -0.5f, 0.0f, 0.0f}};
    vkCmdClearColorImage(cmd, color.image, VK_IMAGE_LAYOUT_GENERAL, &grey, 1, &color_range);
    vkCmdClearColorImage(cmd, motion.image, VK_IMAGE_LAYOUT_GENERAL, &mv, 1, &color_range);
    const VkClearDepthStencilValue far{0.5f, 0};
    const VkImageSubresourceRange depth_range{VK_IMAGE_ASPECT_DEPTH_BIT, 0, 1, 0, 1};
    vkCmdClearDepthStencilImage(cmd, depth.image, VK_IMAGE_LAYOUT_GENERAL, &far, 1, &depth_range);
    VkMemoryBarrier mb{VK_STRUCTURE_TYPE_MEMORY_BARRIER};
    mb.srcAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
    mb.dstAccessMask = VK_ACCESS_SHADER_READ_BIT;
    vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,
                         0, 1, &mb, 0, nullptr, 0, nullptr);
    submit();

    const char* noise_env = std::getenv("BENCH_NOISE");
    if (noise_env && noise_env[0] == '1') {
        // Pseudo-random color, motion within +-2 pixels, depth; one upload.
        const VkDeviceSize color_bytes = VkDeviceSize(rw) * rh * 8, motion_bytes = VkDeviceSize(rw) * rh * 4,
                           depth_bytes = VkDeviceSize(rw) * rh * 4;
        Buffer staging = CreateHostBuffer(gpu, color_bytes + motion_bytes + depth_bytes,
                                          VK_BUFFER_USAGE_TRANSFER_SRC_BIT);
        uint32_t seed = 12345u;
        const auto next = [&] {
            seed = seed * 1664525u + 1013904223u;
            return float(seed >> 8) / float(1u << 24);
        };
        auto* bytes = static_cast<unsigned char*>(staging.data);
        auto* c16 = reinterpret_cast<uint16_t*>(bytes);
        for (VkDeviceSize i = 0; i < VkDeviceSize(rw) * rh; ++i) {
            c16[i * 4 + 0] = Half(next() * 1.5f);
            c16[i * 4 + 1] = Half(next() * 1.5f);
            c16[i * 4 + 2] = Half(next() * 1.5f);
            c16[i * 4 + 3] = Half(1.0f);
        }
        auto* m16 = reinterpret_cast<uint16_t*>(bytes + color_bytes);
        for (VkDeviceSize i = 0; i < VkDeviceSize(rw) * rh * 2; ++i) {
            const float v = next() * 4.0f - 2.0f;
            m16[i] = Half(v);
        }
        auto* d32 = reinterpret_cast<float*>(bytes + color_bytes + motion_bytes);
        for (VkDeviceSize i = 0; i < VkDeviceSize(rw) * rh; ++i) {
            d32[i] = 0.9f + next() * 0.1f;
        }
        CHECK(vkBeginCommandBuffer(cmd, &begin));
        VkBufferImageCopy region{};
        region.imageSubresource = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 0, 1};
        region.imageExtent = {rw, rh, 1};
        vkCmdCopyBufferToImage(cmd, staging.buffer, color.image, VK_IMAGE_LAYOUT_GENERAL, 1, &region);
        region.bufferOffset = color_bytes;
        vkCmdCopyBufferToImage(cmd, staging.buffer, motion.image, VK_IMAGE_LAYOUT_GENERAL, 1, &region);
        region.bufferOffset = color_bytes + motion_bytes;
        region.imageSubresource.aspectMask = VK_IMAGE_ASPECT_DEPTH_BIT;
        vkCmdCopyBufferToImage(cmd, staging.buffer, depth.image, VK_IMAGE_LAYOUT_GENERAL, 1, &region);
        vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TRANSFER_BIT,
                             VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, 0, 1, &mb, 0, nullptr, 0,
                             nullptr);
        submit();
    }

    const auto resource = [](const Image& image, uint32_t format, uint32_t state) {
        FfxApiResource r{};
        r.resource = reinterpret_cast<void*>(image.view);
        r.description.type = FFX_RESOURCE_TYPE_TEXTURE2D;
        r.description.format = format;
        r.description.width = image.width;
        r.description.height = image.height;
        r.description.depth = 1;
        r.description.mipCount = 1;
        r.state = state;
        return r;
    };
    const auto register_image = [&](const Image& image, VkAccessFlags access) {
        const FfxFsr4VkExternalImageState state{
            sizeof(FfxFsr4VkExternalImageState), image.image, image.view,
            VK_IMAGE_LAYOUT_GENERAL, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, access,
            VK_IMAGE_LAYOUT_GENERAL, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, access};
        CHECK(ffxFsr4VkSetExternalImageState(&backend, &state));
    };
    std::unique_ptr<Fsr411::Upscaler> upscaler411;
    VkQueryPool timestamps = VK_NULL_HANDLE;
    double gpu_ms = 0.0;
    float period_ns = 1.0f;
    if (fsr411) {
        const char* dir411 = std::getenv("GOW3_FSR411_DIR");
        upscaler411 = std::make_unique<Fsr411::Upscaler>(gpu.physical, gpu.device,
                                                         dir411 && dir411[0] ? dir411 : "fsr4_411");
        VkQueryPoolCreateInfo qci{VK_STRUCTURE_TYPE_QUERY_POOL_CREATE_INFO};
        qci.queryType = VK_QUERY_TYPE_TIMESTAMP;
        qci.queryCount = 2;
        CHECK(vkCreateQueryPool(gpu.device, &qci, nullptr, &timestamps));
        VkPhysicalDeviceProperties props;
        vkGetPhysicalDeviceProperties(gpu.physical, &props);
        period_ns = props.limits.timestampPeriod;
        std::printf("FSR 4.1.1 replay %ux%u -> %ux%u, %d frames\n", rw, rh, ow, oh, frames);
    }
    for (int frame = 1; fsr411 && frame <= frames; ++frame) {
        CHECK(vkBeginCommandBuffer(cmd, &begin));
        vkCmdResetQueryPool(cmd, timestamps, 0, 2);
        vkCmdWriteTimestamp(cmd, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, timestamps, 0);
        Fsr411::Frame f;
        f.cmdbuf = cmd;
        f.color = {color.image, color.view, rw, rh};
        f.depth = {depth.image, depth.view, rw, rh};
        f.motion = {motion.image, motion.view, rw, rh};
        f.output = {output.image, output.view, ow, oh};
        f.render_width = rw;
        f.render_height = rh;
        f.ultra_performance = preset >= 4;
        f.jitter[0] = 0.25f * float((frame - 1) % 4) - 0.375f; // as fsr4cap
        f.jitter[1] = 0.125f;
        f.sharpen = true;
        f.sharpness = 0.5f;
        f.reset = frame == 1;
        f.auto_exposure = true;
        if (!upscaler411->Record(f)) {
            std::fprintf(stderr, "FSR 4.1.1: %s\n", upscaler411->Error().c_str());
            return 1;
        }
        vkCmdWriteTimestamp(cmd, VK_PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT, timestamps, 1);
        submit();
        uint64_t ts[2];
        CHECK(vkGetQueryPoolResults(gpu.device, timestamps, 0, 2, sizeof(ts), ts, 8, VK_QUERY_RESULT_64_BIT));
        gpu_ms += double(ts[1] - ts[0]) * period_ns * 1e-6;
        if (frame % 300 == 0) {
            std::printf("FSR 4.1.1 replay: %.3f ms/frame (%s)\n", gpu_ms / 300.0, upscaler411->Describe().c_str());
            gpu_ms = 0.0;
        }
    }
    for (int frame = 1; !fsr411 && frame <= frames; ++frame) {
        CHECK(ffxFsr4VkBeginFrame(&backend, uint64_t(frame)));
        CHECK(vkBeginCommandBuffer(cmd, &begin));
        register_image(color, VK_ACCESS_SHADER_READ_BIT);
        register_image(depth, VK_ACCESS_SHADER_READ_BIT);
        register_image(motion, VK_ACCESS_SHADER_READ_BIT);
        register_image(output, VK_ACCESS_SHADER_READ_BIT | VK_ACCESS_SHADER_WRITE_BIT);
        ffxDispatchDescUpscale d{};
        d.header.type = FFX_API_DISPATCH_DESC_TYPE_UPSCALE;
        d.commandList = cmd;
        d.color = resource(color, FFX_SURFACE_FORMAT_R16G16B16A16_FLOAT,
                           FFX_API_RESOURCE_STATE_COMPUTE_READ);
        d.depth = resource(depth, FFX_SURFACE_FORMAT_R32_FLOAT, FFX_API_RESOURCE_STATE_COMPUTE_READ);
        d.motionVectors = resource(motion, FFX_SURFACE_FORMAT_R16G16_FLOAT,
                                   FFX_API_RESOURCE_STATE_COMPUTE_READ);
        d.output = resource(output, FFX_SURFACE_FORMAT_R16G16B16A16_FLOAT,
                            FFX_API_RESOURCE_STATE_UNORDERED_ACCESS);
        d.jitterOffset = {0.25f * float(frame % 4) - 0.375f, 0.125f};
        d.motionVectorScale = {1.0f, 1.0f};
        d.renderSize = {rw, rh};
        d.upscaleSize = {ow, oh};
        d.enableSharpening = true;
        d.sharpness = 0.5f;
        d.enableAutoExposure = true;
        d.frameTimeDelta = 10.0f;
        d.preExposure = 1.0f;
        d.reset = frame == 1;
        d.cameraNear = 0.05f;
        d.cameraFar = 3000.0f;
        d.cameraFovAngleVertical = 0.75f;
        d.viewSpaceToMetersFactor = 1.0f;
        if (ffxFsr4V07Dispatch(&context, &d.header) != FFX_API_RETURN_OK) {
            std::fprintf(stderr, "dispatch failed\n");
            return 1;
        }
        submit();
        CHECK(ffxFsr4VkRetireFrame(&backend, uint64_t(frame)));
    }
    CHECK(vkDeviceWaitIdle(gpu.device));
    if (const char* dump = std::getenv("BENCH_DUMP")) {
        const VkDeviceSize bytes = VkDeviceSize(ow) * oh * 8;
        Buffer readback = CreateHostBuffer(gpu, bytes, VK_BUFFER_USAGE_TRANSFER_DST_BIT);
        CHECK(vkBeginCommandBuffer(cmd, &begin));
        VkMemoryBarrier rb{VK_STRUCTURE_TYPE_MEMORY_BARRIER};
        rb.srcAccessMask = VK_ACCESS_SHADER_WRITE_BIT;
        rb.dstAccessMask = VK_ACCESS_TRANSFER_READ_BIT;
        vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,
                             VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 1, &rb, 0, nullptr, 0, nullptr);
        VkBufferImageCopy region{};
        region.imageSubresource = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 0, 1};
        region.imageExtent = {ow, oh, 1};
        vkCmdCopyImageToBuffer(cmd, output.image, VK_IMAGE_LAYOUT_GENERAL, readback.buffer, 1,
                               &region);
        submit();
        if (FILE* f = std::fopen(dump, "wb")) {
            std::fwrite(readback.data, 1, size_t(bytes), f);
            std::fclose(f);
        }
    }
    upscaler411.reset();
    if (!fsr411) {
        ffxFsr4V07DestroyContext(&context, nullptr);
        ffxFsr4VkDestroyContext(reinterpret_cast<FfxFsr4VkContext*>(scratch.data()));
    }
    return 0;
}
