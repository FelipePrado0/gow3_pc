// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: FSR 4.1.1 replay on Vulkan (fsr411.h). The frame is the DLL's 29 dispatches: SPD auto
// exposure, prepass, pass0_post, model passes 1..12 each followed by its _post pass (tensor border
// clears), postpass, RCAS. The rules for group counts, the tensor size table and the constants
// are the ones tools/fsr4cap/extract.py checks against the recorded D3D12 frames.

#include "fsr411.h"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <map>
#include <vector>

namespace Fsr411 {

namespace {

constexpr const char* kPasses[] = {
    "spd",         "prepass",     "pass0_post",  "pass1",       "pass1_post",  "pass2",
    "pass2_post",  "pass3",       "pass3_post",  "pass4",       "pass4_post",  "pass5",
    "pass5_post",  "pass6",       "pass6_post",  "pass7",       "pass7_post",  "pass8",
    "pass8_post",  "pass9",       "pass9_post",  "pass10",      "pass10_post", "pass11",
    "pass11_post", "pass12",      "pass12_post", "postpass",    "rcas",
};
constexpr uint32_t kPassCount = sizeof(kPasses) / sizeof(kPasses[0]);
/// Tensor level (1/2^level of the aligned output) a model pass runs at, and of its _post pass.
constexpr uint32_t kRunLevel[13] = {1, 1, 1, 2, 2, 2, 3, 3, 3, 3, 2, 2, 1};
constexpr uint32_t kPostLevel[13] = {1, 1, 1, 2, 2, 2, 3, 3, 3, 2, 2, 1, 1};

uint32_t CeilDiv(uint32_t a, uint32_t b) {
    return (a + b - 1) / b;
}

uint16_t FloatToHalf(float f) {
    uint32_t x;
    std::memcpy(&x, &f, 4);
    const uint32_t sign = (x >> 16) & 0x8000u;
    int32_t exp = int32_t((x >> 23) & 0xffu) - 127 + 15;
    uint32_t mant = x & 0x7fffffu;
    if (exp <= 0) {
        return uint16_t(sign);
    }
    if (exp >= 31) {
        return uint16_t(sign | 0x7c00u);
    }
    // Round to nearest even.
    uint32_t h = sign | (uint32_t(exp) << 10) | (mant >> 13);
    const uint32_t rest = mant & 0x1fffu;
    if (rest > 0x1000u || (rest == 0x1000u && (h & 1u))) {
        ++h;
    }
    return uint16_t(h);
}

bool ReadFile(const std::string& path, std::vector<uint8_t>& data) {
    std::ifstream file(path, std::ios::binary | std::ios::ate);
    if (!file) {
        return false;
    }
    data.resize(size_t(file.tellg()));
    file.seekg(0);
    return bool(file.read(reinterpret_cast<char*>(data.data()), std::streamsize(data.size())));
}

// ---- SPIR-V reflection ------------------------------------------------------------------------

struct Binding {
    uint32_t binding;
    VkDescriptorType type;
    std::string name;
};

struct Reflection {
    std::string entry = "main";
    uint32_t local_size[3] = {1, 1, 1};
    std::vector<Binding> bindings;
};

bool Reflect(const std::vector<uint32_t>& words, Reflection& out, std::string& error) {
    if (words.size() < 5 || words[0] != 0x07230203u) {
        error = "not SPIR-V";
        return false;
    }
    std::map<uint32_t, std::string> names;
    std::map<uint32_t, uint32_t> binding_of;
    struct Type {
        uint32_t op = 0;
        uint32_t a = 0, b = 0, c = 0; // image: sampled type, dim, sampled; pointer: class, pointee
    };
    std::map<uint32_t, Type> types;
    struct Var {
        uint32_t id, type, storage;
    };
    std::vector<Var> vars;
    const auto string_at = [&](size_t start, size_t end) {
        std::string s;
        for (size_t i = start; i < end; ++i) {
            for (int k = 0; k < 4; ++k) {
                const char ch = char((words[i] >> (8 * k)) & 0xffu);
                if (!ch) {
                    return s;
                }
                s.push_back(ch);
            }
        }
        return s;
    };
    for (size_t i = 5; i < words.size();) {
        const uint32_t count = words[i] >> 16, op = words[i] & 0xffffu;
        if (count == 0 || i + count > words.size()) {
            error = "malformed SPIR-V";
            return false;
        }
        const uint32_t* w = &words[i];
        switch (op) {
        case 5: // OpName
            names[w[1]] = string_at(i + 2, i + count);
            break;
        case 15: // OpEntryPoint: model, id, name
            out.entry = string_at(i + 3, i + count);
            break;
        case 16: // OpExecutionMode LocalSize
            if (count >= 6 && w[2] == 17) {
                out.local_size[0] = w[3];
                out.local_size[1] = w[4];
                out.local_size[2] = w[5];
            }
            break;
        case 25: // OpTypeImage: result, sampled type, dim, depth, arrayed, ms, sampled, format
            types[w[1]] = {op, w[2], w[3], w[7]};
            break;
        case 26: // OpTypeSampler
            types[w[1]] = {op};
            break;
        case 30: // OpTypeStruct
            types[w[1]] = {op};
            break;
        case 32: // OpTypePointer: result, storage class, type
            types[w[1]] = {op, w[2], w[3]};
            break;
        case 59: // OpVariable: result type, result, storage class
            vars.push_back({w[2], w[1], w[3]});
            break;
        case 71: // OpDecorate
            if (count >= 4 && w[2] == 33) {
                binding_of[w[1]] = w[3];
            }
            break;
        default:
            break;
        }
        i += count;
    }
    for (const Var& v : vars) {
        const auto b = binding_of.find(v.id);
        if (b == binding_of.end()) {
            continue;
        }
        const Type& ptr = types[v.type];
        const Type& pointee = types[ptr.b];
        VkDescriptorType type;
        if (v.storage == 2) {
            type = VK_DESCRIPTOR_TYPE_UNIFORM_BUFFER;
        } else if (v.storage == 12) {
            type = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER;
        } else if (pointee.op == 26) {
            type = VK_DESCRIPTOR_TYPE_SAMPLER;
        } else if (pointee.op == 25) {
            const bool buffer = pointee.b == 5;
            if (pointee.c == 2) {
                type = buffer ? VK_DESCRIPTOR_TYPE_STORAGE_TEXEL_BUFFER : VK_DESCRIPTOR_TYPE_STORAGE_IMAGE;
            } else {
                type = buffer ? VK_DESCRIPTOR_TYPE_UNIFORM_TEXEL_BUFFER : VK_DESCRIPTOR_TYPE_SAMPLED_IMAGE;
            }
        } else {
            continue;
        }
        // dxil-spirv may declare one resource twice (typed views of a buffer): one binding.
        const bool seen = std::any_of(out.bindings.begin(), out.bindings.end(),
                                      [&](const Binding& x) { return x.binding == b->second; });
        if (!seen) {
            std::string name = names[v.id];
            if (const size_t u = name.find("_1"); u != std::string::npos && u + 2 == name.size()) {
                name.resize(u);
            }
            out.bindings.push_back({b->second, type, name});
        }
    }
    return true;
}

} // namespace

// ---- implementation ---------------------------------------------------------------------------

struct Upscaler::Impl {
    VkPhysicalDevice physical;
    VkDevice device;
    std::string dir;
    std::string error;
    VkPhysicalDeviceMemoryProperties memory{};
    VkDeviceSize ubo_align = 256;
    PFN_vkCmdPushDescriptorSetKHR push_descriptors = nullptr;

    // Current set and sizes.
    std::string set;
    uint32_t out_w = 0, out_h = 0;
    bool ready = false;
    bool uploaded = false;

    struct Pass {
        Reflection refl;
        VkDescriptorSetLayout set_layout = VK_NULL_HANDLE;
        VkPipelineLayout layout = VK_NULL_HANDLE;
        VkPipeline pipeline = VK_NULL_HANDLE;
    };
    std::array<Pass, kPassCount> passes{};
    VkSampler linear_clamp = VK_NULL_HANDLE;

    struct Img {
        VkImage image = VK_NULL_HANDLE;
        VkImageView view = VK_NULL_HANDLE;
        VkDeviceMemory memory = VK_NULL_HANDLE;
    };
    Img recurrent, history, reprojected, mlsr_output, exposure, exposure_identity, spd_atomic, spd_mip5;
    struct Buf {
        VkBuffer buffer = VK_NULL_HANDLE;
        VkDeviceMemory memory = VK_NULL_HANDLE;
        VkDeviceSize size = 0;
        void* mapped = nullptr;
    };
    Buf scratch, initializer, staging, ubo, tensor_ubo;
    std::vector<uint8_t> initializer_data;
    uint64_t frame_index = 0;
    float previous_pre_exposure = 0.0f;
    // GOW3_FSR4_PROFILE=1: GPU time per pass, read when a ring slot is reused, printed every 300 frames.
    VkQueryPool profile_pool = VK_NULL_HANDLE;
    float period_ns = 1.0f;
    std::array<uint32_t, kFramesInFlight> profile_count{};
    std::array<std::array<uint8_t, kPassCount>, kFramesInFlight> profile_pass{};
    std::array<double, kPassCount> profile_ms{};
    uint64_t profile_frames = 0;

    Impl(VkPhysicalDevice p, VkDevice d, std::string dir_)
        : physical{p}, device{d}, dir{std::move(dir_)} {
        vkGetPhysicalDeviceMemoryProperties(physical, &memory);
        VkPhysicalDeviceProperties props;
        vkGetPhysicalDeviceProperties(physical, &props);
        ubo_align = std::max<VkDeviceSize>(256, props.limits.minUniformBufferOffsetAlignment);
        push_descriptors = reinterpret_cast<PFN_vkCmdPushDescriptorSetKHR>(
            vkGetDeviceProcAddr(device, "vkCmdPushDescriptorSetKHR"));
        period_ns = props.limits.timestampPeriod;
        const char* profile = std::getenv("GOW3_FSR4_PROFILE");
        if (profile && profile[0] == '1') {
            VkQueryPoolCreateInfo qci{VK_STRUCTURE_TYPE_QUERY_POOL_CREATE_INFO};
            qci.queryType = VK_QUERY_TYPE_TIMESTAMP;
            qci.queryCount = kFramesInFlight * (kPassCount + 1);
            if (vkCreateQueryPool(device, &qci, nullptr, &profile_pool) != VK_SUCCESS) {
                profile_pool = VK_NULL_HANDLE;
            }
        }
    }

    void CollectProfile(uint32_t slot) {
        const uint32_t count = profile_count[slot];
        if (!profile_pool || !count) {
            return;
        }
        profile_count[slot] = 0;
        std::array<uint64_t, kPassCount + 1> ts{};
        if (vkGetQueryPoolResults(device, profile_pool, slot * (kPassCount + 1), count + 1,
                                  sizeof(ts), ts.data(), 8, VK_QUERY_RESULT_64_BIT) != VK_SUCCESS) {
            return;
        }
        for (uint32_t i = 0; i < count; ++i) {
            profile_ms[profile_pass[slot][i]] += double(ts[i + 1] - ts[i]) * period_ns * 1e-6;
        }
        if (++profile_frames % 300) {
            return;
        }
        double total = 0.0;
        for (double ms : profile_ms) total += ms;
        std::printf("FSR 4.1.1 profile: %.3f ms/frame\n", total / 300.0);
        for (uint32_t p = 0; p < kPassCount; ++p) {
            if (profile_ms[p] > 0.0) {
                std::printf("  %6.3f ms/frame  %s\n", profile_ms[p] / 300.0, kPasses[p]);
            }
            profile_ms[p] = 0.0;
        }
        std::fflush(stdout);
    }

    ~Impl() {
        Destroy();
        if (profile_pool) vkDestroyQueryPool(device, profile_pool, nullptr);
    }

    uint32_t MemoryType(uint32_t bits, VkMemoryPropertyFlags flags) const {
        for (uint32_t i = 0; i < memory.memoryTypeCount; ++i) {
            if ((bits & (1u << i)) && (memory.memoryTypes[i].propertyFlags & flags) == flags) {
                return i;
            }
        }
        return UINT32_MAX;
    }

    bool Allocate(VkMemoryRequirements req, VkMemoryPropertyFlags flags, VkDeviceMemory& out) {
        VkMemoryAllocateInfo ai{VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
        ai.allocationSize = req.size;
        ai.memoryTypeIndex = MemoryType(req.memoryTypeBits, flags);
        if (ai.memoryTypeIndex == UINT32_MAX) {
            error = "no memory type";
            return false;
        }
        if (vkAllocateMemory(device, &ai, nullptr, &out) != VK_SUCCESS) {
            error = "out of memory";
            return false;
        }
        return true;
    }

    bool MakeBuffer(Buf& b, VkDeviceSize size, VkBufferUsageFlags usage, bool host) {
        VkBufferCreateInfo ci{VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO};
        ci.size = size;
        ci.usage = usage;
        if (vkCreateBuffer(device, &ci, nullptr, &b.buffer) != VK_SUCCESS) {
            error = "buffer creation failed";
            return false;
        }
        VkMemoryRequirements req;
        vkGetBufferMemoryRequirements(device, b.buffer, &req);
        const VkMemoryPropertyFlags flags =
            host ? VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT
                 : VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT;
        if (!Allocate(req, flags, b.memory) ||
            vkBindBufferMemory(device, b.buffer, b.memory, 0) != VK_SUCCESS) {
            return false;
        }
        b.size = size;
        if (host && vkMapMemory(device, b.memory, 0, VK_WHOLE_SIZE, 0, &b.mapped) != VK_SUCCESS) {
            error = "map failed";
            return false;
        }
        return true;
    }

    bool MakeImage(Img& img, VkFormat format, uint32_t w, uint32_t h) {
        VkImageCreateInfo ci{VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO};
        ci.imageType = VK_IMAGE_TYPE_2D;
        ci.format = format;
        ci.extent = {w, h, 1};
        ci.mipLevels = 1;
        ci.arrayLayers = 1;
        ci.samples = VK_SAMPLE_COUNT_1_BIT;
        ci.tiling = VK_IMAGE_TILING_OPTIMAL;
        ci.usage = VK_IMAGE_USAGE_STORAGE_BIT | VK_IMAGE_USAGE_SAMPLED_BIT |
                   VK_IMAGE_USAGE_TRANSFER_DST_BIT | VK_IMAGE_USAGE_TRANSFER_SRC_BIT;
        if (vkCreateImage(device, &ci, nullptr, &img.image) != VK_SUCCESS) {
            error = "image creation failed";
            return false;
        }
        VkMemoryRequirements req;
        vkGetImageMemoryRequirements(device, img.image, &req);
        if (!Allocate(req, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT, img.memory) ||
            vkBindImageMemory(device, img.image, img.memory, 0) != VK_SUCCESS) {
            return false;
        }
        VkImageViewCreateInfo vi{VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO};
        vi.image = img.image;
        vi.viewType = VK_IMAGE_VIEW_TYPE_2D;
        vi.format = format;
        vi.subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
        if (vkCreateImageView(device, &vi, nullptr, &img.view) != VK_SUCCESS) {
            error = "image view creation failed";
            return false;
        }
        return true;
    }

    void FreeImage(Img& img) {
        if (img.view) vkDestroyImageView(device, img.view, nullptr);
        if (img.image) vkDestroyImage(device, img.image, nullptr);
        if (img.memory) vkFreeMemory(device, img.memory, nullptr);
        img = {};
    }

    void FreeBuffer(Buf& b) {
        if (b.buffer) vkDestroyBuffer(device, b.buffer, nullptr);
        if (b.memory) vkFreeMemory(device, b.memory, nullptr);
        b = {};
    }

    void Destroy() {
        for (Pass& p : passes) {
            if (p.pipeline) vkDestroyPipeline(device, p.pipeline, nullptr);
            if (p.layout) vkDestroyPipelineLayout(device, p.layout, nullptr);
            if (p.set_layout) vkDestroyDescriptorSetLayout(device, p.set_layout, nullptr);
            p = {};
        }
        if (linear_clamp) vkDestroySampler(device, linear_clamp, nullptr);
        profile_count = {};
        linear_clamp = VK_NULL_HANDLE;
        for (Img* img : {&recurrent, &history, &reprojected, &mlsr_output, &exposure,
                         &exposure_identity, &spd_atomic, &spd_mip5}) {
            FreeImage(*img);
        }
        for (Buf* b : {&scratch, &initializer, &staging, &ubo, &tensor_ubo}) {
            FreeBuffer(*b);
        }
        ready = false;
        uploaded = false;
        set.clear();
    }

    bool LoadPass(uint32_t index) {
        std::vector<uint8_t> bytes;
        const std::string path = dir + "/" + set + "/" + kPasses[index] + ".spv";
        if (!ReadFile(path, bytes) || bytes.size() % 4) {
            error = "missing " + path + " (tools/fsr4cap: capture and extract the FSR 4.1.1 assets)";
            return false;
        }
        std::vector<uint32_t> words(bytes.size() / 4);
        std::memcpy(words.data(), bytes.data(), bytes.size());
        Pass& p = passes[index];
        if (!Reflect(words, p.refl, error)) {
            error = path + ": " + error;
            return false;
        }
        std::vector<VkDescriptorSetLayoutBinding> bindings;
        for (const Binding& b : p.refl.bindings) {
            VkDescriptorSetLayoutBinding lb{};
            lb.binding = b.binding;
            lb.descriptorType = b.type;
            lb.descriptorCount = 1;
            lb.stageFlags = VK_SHADER_STAGE_COMPUTE_BIT;
            if (b.type == VK_DESCRIPTOR_TYPE_SAMPLER) {
                lb.pImmutableSamplers = &linear_clamp;
            }
            bindings.push_back(lb);
        }
        VkDescriptorSetLayoutCreateInfo sci{VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO};
        sci.flags = VK_DESCRIPTOR_SET_LAYOUT_CREATE_PUSH_DESCRIPTOR_BIT_KHR;
        sci.bindingCount = uint32_t(bindings.size());
        sci.pBindings = bindings.data();
        VkPipelineLayoutCreateInfo lci{VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO};
        lci.setLayoutCount = 1;
        lci.pSetLayouts = &p.set_layout;
        VkShaderModuleCreateInfo mci{VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO};
        mci.codeSize = bytes.size();
        mci.pCode = words.data();
        VkShaderModule module = VK_NULL_HANDLE;
        if (vkCreateDescriptorSetLayout(device, &sci, nullptr, &p.set_layout) != VK_SUCCESS ||
            vkCreatePipelineLayout(device, &lci, nullptr, &p.layout) != VK_SUCCESS ||
            vkCreateShaderModule(device, &mci, nullptr, &module) != VK_SUCCESS) {
            error = path + ": layout or module creation failed";
            return false;
        }
        VkComputePipelineCreateInfo pci{VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO};
        pci.stage.sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
        pci.stage.stage = VK_SHADER_STAGE_COMPUTE_BIT;
        pci.stage.module = module;
        pci.stage.pName = p.refl.entry.c_str();
        pci.layout = p.layout;
        const VkResult r = vkCreateComputePipelines(device, VK_NULL_HANDLE, 1, &pci, nullptr, &p.pipeline);
        vkDestroyShaderModule(device, module, nullptr);
        if (r != VK_SUCCESS) {
            error = path + ": pipeline creation failed (" + std::to_string(int(r)) + ")";
            return false;
        }
        return true;
    }

    bool Create(const std::string& wanted, uint32_t ow, uint32_t oh, bool tier2160) {
        Destroy();
        if (!push_descriptors) {
            error = "VK_KHR_push_descriptor is required";
            return false;
        }
        set = wanted;
        out_w = ow;
        out_h = oh;
        VkSamplerCreateInfo si{VK_STRUCTURE_TYPE_SAMPLER_CREATE_INFO};
        si.magFilter = si.minFilter = VK_FILTER_LINEAR;
        si.mipmapMode = VK_SAMPLER_MIPMAP_MODE_LINEAR;
        si.addressModeU = si.addressModeV = si.addressModeW = VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE;
        si.maxLod = VK_LOD_CLAMP_NONE;
        if (vkCreateSampler(device, &si, nullptr, &linear_clamp) != VK_SUCCESS) {
            error = "sampler creation failed";
            return false;
        }
        for (uint32_t i = 0; i < kPassCount; ++i) {
            if (!LoadPass(i)) {
                return false;
            }
        }
        if (!ReadFile(dir + "/" + set + "/initializer.bin", initializer_data) ||
            initializer_data.size() != 131072) {
            error = "missing or wrong " + dir + "/" + set + "/initializer.bin";
            return false;
        }
        const VkDeviceSize scratch_size = tier2160 ? 83232256u : 20880256u;
        if (!MakeImage(recurrent, VK_FORMAT_R8G8B8A8_UNORM, ow, oh) ||
            !MakeImage(history, VK_FORMAT_R16G16B16A16_SFLOAT, ow, oh) ||
            !MakeImage(reprojected, VK_FORMAT_R16G16B16A16_SFLOAT, ow, oh) ||
            !MakeImage(mlsr_output, VK_FORMAT_R16G16B16A16_SFLOAT, ow, oh) ||
            !MakeImage(exposure, VK_FORMAT_R32_SFLOAT, 2, 1) ||
            !MakeImage(exposure_identity, VK_FORMAT_R32_SFLOAT, 2, 1) ||
            !MakeImage(spd_atomic, VK_FORMAT_R32_UINT, 1, 1) ||
            !MakeImage(spd_mip5, VK_FORMAT_R32_SFLOAT, CeilDiv(ow, 16), CeilDiv(oh, 16)) ||
            !MakeBuffer(scratch, scratch_size,
                        VK_BUFFER_USAGE_STORAGE_BUFFER_BIT | VK_BUFFER_USAGE_TRANSFER_DST_BIT, false) ||
            !MakeBuffer(initializer, 131072,
                        VK_BUFFER_USAGE_STORAGE_BUFFER_BIT | VK_BUFFER_USAGE_TRANSFER_DST_BIT, false) ||
            !MakeBuffer(staging, 131072, VK_BUFFER_USAGE_TRANSFER_SRC_BIT, true) ||
            !MakeBuffer(ubo, kFramesInFlight * 4 * ubo_align, VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT, true) ||
            !MakeBuffer(tensor_ubo, 512, VK_BUFFER_USAGE_UNIFORM_BUFFER_BIT, true)) {
            return false;
        }
        std::memcpy(staging.mapped, initializer_data.data(), initializer_data.size());
        // Tensor sizes (17 x uint4): levels of the output aligned to 8.
        const uint32_t aw = (ow + 7) & ~7u, ah = (oh + 7) & ~7u;
        static constexpr uint32_t kShift[17] = {1, 0, 1, 1, 2, 2, 2, 3, 3, 3, 2, 2, 1, 1, 0, 0, 0};
        auto* sizes = static_cast<uint32_t*>(tensor_ubo.mapped);
        std::memset(sizes, 0, 512);
        for (uint32_t i = 0; i < 17; ++i) {
            sizes[i * 4 + 0] = aw >> kShift[i];
            sizes[i * 4 + 1] = ah >> kShift[i];
        }
        ready = true;
        previous_pre_exposure = 0.0f;
        return true;
    }

    /// First frame after creation: weights, cleared state.
    void Upload(VkCommandBuffer cmd) {
        std::vector<VkImageMemoryBarrier> to_general;
        for (Img* img : {&recurrent, &history, &reprojected, &mlsr_output, &exposure,
                         &exposure_identity, &spd_atomic, &spd_mip5}) {
            VkImageMemoryBarrier b{VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER};
            b.dstAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
            b.oldLayout = VK_IMAGE_LAYOUT_UNDEFINED;
            b.newLayout = VK_IMAGE_LAYOUT_GENERAL;
            b.srcQueueFamilyIndex = b.dstQueueFamilyIndex = VK_QUEUE_FAMILY_IGNORED;
            b.image = img->image;
            b.subresourceRange = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
            to_general.push_back(b);
        }
        vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT,
                             0, 0, nullptr, 0, nullptr, uint32_t(to_general.size()), to_general.data());
        const VkImageSubresourceRange range{VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
        const VkClearColorValue zero{}, one{{1.0f, 1.0f, 1.0f, 1.0f}};
        for (Img* img : {&recurrent, &history, &reprojected, &mlsr_output, &exposure, &spd_atomic, &spd_mip5}) {
            vkCmdClearColorImage(cmd, img->image, VK_IMAGE_LAYOUT_GENERAL, &zero, 1, &range);
        }
        vkCmdClearColorImage(cmd, exposure_identity.image, VK_IMAGE_LAYOUT_GENERAL, &one, 1, &range);
        vkCmdFillBuffer(cmd, scratch.buffer, 0, VK_WHOLE_SIZE, 0);
        const VkBufferCopy copy{0, 0, 131072};
        vkCmdCopyBuffer(cmd, staging.buffer, initializer.buffer, 1, &copy);
        VkMemoryBarrier mb{VK_STRUCTURE_TYPE_MEMORY_BARRIER};
        mb.srcAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT;
        mb.dstAccessMask = VK_ACCESS_SHADER_READ_BIT | VK_ACCESS_SHADER_WRITE_BIT;
        vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,
                             0, 1, &mb, 0, nullptr, 0, nullptr);
        uploaded = true;
    }

    bool Record(const Frame& f) {
        error.clear();
        const uint32_t ow = f.output.width, oh = f.output.height;
        const uint32_t rw = f.render_width, rh = f.render_height;
        if (!ow || !oh || !rw || !rh || rw > ow || rh > oh || ow > 3840 || oh > 2160) {
            error = "unsupported size " + std::to_string(rw) + "x" + std::to_string(rh) + " -> " +
                    std::to_string(ow) + "x" + std::to_string(oh);
            return false;
        }
        const bool tier2160 = ow > 1920 || oh > 1080;
        const std::string wanted = std::string(tier2160 ? "t2160" : "t1080") +
                                   (f.ultra_performance ? "_m1" : "_m0");
        if (!ready || wanted != set || ow != out_w || oh != out_h) {
            if (!Create(wanted, ow, oh, tier2160)) {
                Destroy();
                return false;
            }
        }
        const VkCommandBuffer cmd = f.cmdbuf;
        if (!uploaded) {
            Upload(cmd);
        }
        const uint32_t aw = (ow + 7) & ~7u, ah = (oh + 7) & ~7u;
        const float pre_exposure = f.pre_exposure > 0.0f ? f.pre_exposure : 1.0f;

        // Constant buffers of this frame: MLSR, SPD, RCAS.
        const uint32_t ring = uint32_t(frame_index++ % kFramesInFlight);
        const VkDeviceSize slot = ring * 4 * ubo_align;
        CollectProfile(ring);
        const uint32_t query_base = ring * (kPassCount + 1);
        uint32_t profiled = 0;
        if (profile_pool) {
            vkCmdResetQueryPool(cmd, profile_pool, query_base, kPassCount + 1);
            vkCmdWriteTimestamp(cmd, VK_PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT, profile_pool, query_base);
        }
        auto* base = static_cast<uint8_t*>(ubo.mapped) + slot;
        struct Mlsr {
            float inv_size[2], scale[2], inv_scale[2], jitter[2], mv_scale[2], tex_size[2],
                max_render_size[2], mv_jitter_cancellation[2];
            uint32_t width, height, reset, width_lr, height_lr;
            float pre_exposure, previous_pre_exposure;
            uint32_t rcas_enabled;
            float rcas_sharpness, pad;
        } mlsr{};
        mlsr.inv_size[0] = 1.0f / float(ow);
        mlsr.inv_size[1] = 1.0f / float(oh);
        mlsr.scale[0] = float(ow) / float(rw);
        mlsr.scale[1] = float(oh) / float(rh);
        // As the DLL: the reciprocal of the scale (not rw / ow, which differs in the last bit).
        mlsr.inv_scale[0] = 1.0f / mlsr.scale[0];
        mlsr.inv_scale[1] = 1.0f / mlsr.scale[1];
        mlsr.jitter[0] = f.jitter[0];
        mlsr.jitter[1] = f.jitter[1];
        mlsr.mv_scale[0] = f.motion_scale[0] / float(rw);
        mlsr.mv_scale[1] = f.motion_scale[1] / float(rh);
        mlsr.tex_size[0] = mlsr.max_render_size[0] = float(ow);
        mlsr.tex_size[1] = mlsr.max_render_size[1] = float(oh);
        mlsr.width = ow;
        mlsr.height = oh;
        mlsr.reset = f.reset ? 1u : 0u;
        mlsr.width_lr = rw;
        mlsr.height_lr = rh;
        mlsr.pre_exposure = pre_exposure;
        mlsr.previous_pre_exposure = previous_pre_exposure;
        mlsr.rcas_enabled = f.sharpen ? 1u : 0u;
        mlsr.rcas_sharpness = f.sharpness;
        std::memcpy(base, &mlsr, sizeof(mlsr));
        struct Spd {
            uint32_t mips, work_groups, offset[2];
            float inv_input_size[2], pre_exposure, pad;
        } spd{};
        for (uint32_t m = std::max(rw, rh); m > 1 && spd.mips < 12; m >>= 1) {
            ++spd.mips;
        }
        spd.work_groups = CeilDiv(rw, 64) * CeilDiv(rh, 64);
        spd.inv_input_size[0] = 1.0f / float(rw);
        spd.inv_input_size[1] = 1.0f / float(rh);
        spd.pre_exposure = pre_exposure;
        std::memcpy(base + ubo_align, &spd, sizeof(spd));
        struct Rcas {
            uint32_t config[4];
            float pre_exposure;
            uint32_t pad[3];
        } rcas{};
        const float amount = std::clamp(f.sharpness, 0.0f, 1.0f);
        const float linear = std::exp2(-(2.0f - 2.0f * amount));
        std::memcpy(&rcas.config[0], &linear, 4);
        rcas.config[1] = uint32_t(FloatToHalf(linear)) | (uint32_t(FloatToHalf(linear)) << 16);
        rcas.pre_exposure = pre_exposure;
        std::memcpy(base + 2 * ubo_align, &rcas, sizeof(rcas));
        previous_pre_exposure = pre_exposure;

        const Img& exposure_image = f.auto_exposure ? exposure : exposure_identity;
        const Image* const final_output = &f.output;

        VkMemoryBarrier between{VK_STRUCTURE_TYPE_MEMORY_BARRIER};
        between.srcAccessMask = VK_ACCESS_SHADER_WRITE_BIT;
        between.dstAccessMask = VK_ACCESS_SHADER_READ_BIT | VK_ACCESS_SHADER_WRITE_BIT;
        // Earlier work of the caller on the inputs (render, copies) is the caller's barrier.
        for (uint32_t index = 0; index < kPassCount; ++index) {
            const std::string name = kPasses[index];
            if ((name == "spd" && !f.auto_exposure) || (name == "rcas" && !f.sharpen)) {
                continue;
            }
            const Pass& p = passes[index];
            std::vector<VkWriteDescriptorSet> writes;
            std::vector<VkDescriptorImageInfo> images;
            std::vector<VkDescriptorBufferInfo> buffers;
            images.reserve(p.refl.bindings.size());
            buffers.reserve(p.refl.bindings.size());
            for (const Binding& b : p.refl.bindings) {
                VkWriteDescriptorSet w{VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET};
                w.dstBinding = b.binding;
                w.descriptorCount = 1;
                w.descriptorType = b.type;
                const auto image = [&](VkImageView view) {
                    images.push_back({VK_NULL_HANDLE, view, VK_IMAGE_LAYOUT_GENERAL});
                    w.pImageInfo = &images.back();
                };
                const auto buffer = [&](VkBuffer buf, VkDeviceSize offset, VkDeviceSize range) {
                    buffers.push_back({buf, offset, range});
                    w.pBufferInfo = &buffers.back();
                };
                const std::string& n = b.name;
                if (b.type == VK_DESCRIPTOR_TYPE_SAMPLER) {
                    continue; // immutable
                } else if (n == "r_input_color") image(f.color.view);
                else if (n == "r_velocity") image(f.motion.view);
                else if (n == "r_depth") image(f.depth.view);
                else if (n == "r_history_color" || n == "rw_history_color") image(history.view);
                else if (n == "r_reprojected_color" || n == "rw_reprojected_color") image(reprojected.view);
                else if (n == "r_recurrent_0" || n == "rw_recurrent_0") image(recurrent.view);
                else if (n == "r_auto_exposure_texture") image(exposure_image.view);
                else if (n == "rw_auto_exposure_texture") image(exposure.view);
                else if (n == "rw_spd_global_atomic") image(spd_atomic.view);
                else if (n == "rw_autoexp_mip_5") image(spd_mip5.view);
                else if (n == "rw_mlsr_output_color") image(f.sharpen ? mlsr_output.view : final_output->view);
                else if (n == "r_rcas_input") image(mlsr_output.view);
                else if (n == "rw_rcas_output") image(final_output->view);
                else if (n == "ScratchBuffer") buffer(scratch.buffer, 0, VK_WHOLE_SIZE);
                else if (n == "InitializerBuffer") buffer(initializer.buffer, 0, VK_WHOLE_SIZE);
                else if (n == "MLSR_Optimized_Constants") buffer(ubo.buffer, slot, sizeof(Mlsr));
                else if (n == "AutoExposureSPDConstants") buffer(ubo.buffer, slot + ubo_align, sizeof(Spd));
                else if (n == "cbRCAS") buffer(ubo.buffer, slot + 2 * ubo_align, sizeof(Rcas));
                else if (n == "CsTensorSizes") buffer(tensor_ubo.buffer, 0, 272);
                else {
                    error = name + ": unknown resource " + n;
                    return false;
                }
                writes.push_back(w);
            }
            vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, p.pipeline);
            push_descriptors(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, p.layout, 0, uint32_t(writes.size()),
                             writes.data());
            uint32_t gx = 1, gy = 1;
            if (name == "spd") {
                gx = CeilDiv(rw, 64);
                gy = CeilDiv(rh, 64);
            } else if (name == "prepass" || name == "rcas") {
                gx = CeilDiv(aw, 16);
                gy = CeilDiv(ah, 16);
            } else if (name == "postpass") {
                gx = CeilDiv(aw, 32);
                gy = CeilDiv(ah, 32);
            } else if (name.ends_with("_post")) {
                // Clears the border of a tensor: threads as the shader counts them (it stops the
                // rest) from the tensor size and the tier's allocation.
                const uint32_t k = name == "pass0_post" ? 0 : uint32_t(std::stoi(name.substr(4)));
                const uint32_t level = k ? kPostLevel[k] : 1;
                const uint32_t w = aw >> level, h = ah >> level;
                const uint32_t tw = (tier2160 ? 3840u : 1920u) >> level;
                const uint32_t th = (tier2160 ? 2160u : 1080u) >> level;
                const uint32_t kx = std::min(tw + 1 - w, 5u), ky = std::min(th + 1 - h, 1u);
                const uint32_t threads = (w + 1 + kx) + h + h * kx + (w + 1 + kx) * ky;
                gx = CeilDiv(threads, p.refl.local_size[0]);
            } else {
                const uint32_t k = uint32_t(std::stoi(name.substr(4)));
                const uint32_t level = kRunLevel[k];
                gx = CeilDiv(aw >> level, 64);
                gy = ah >> level;
            }
            vkCmdDispatch(cmd, gx, gy, 1);
            if (profile_pool) {
                vkCmdWriteTimestamp(cmd, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, profile_pool,
                                    query_base + 1 + profiled);
                profile_pass[ring][profiled++] = uint8_t(index);
            }
            vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,
                                 VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, 0, 1, &between, 0, nullptr, 0,
                                 nullptr);
        }
        profile_count[ring] = profiled;
        return true;
    }
};

Upscaler::Upscaler(VkPhysicalDevice physical, VkDevice device, std::string dir)
    : impl{std::make_unique<Impl>(physical, device, std::move(dir))} {}

Upscaler::~Upscaler() = default;

bool Upscaler::Record(const Frame& frame) {
    return impl->Record(frame);
}

const std::string& Upscaler::Error() const noexcept {
    return impl->error;
}

std::string Upscaler::Describe() const {
    return impl->set + " output " + std::to_string(impl->out_w) + "x" + std::to_string(impl->out_h);
}

} // namespace Fsr411
