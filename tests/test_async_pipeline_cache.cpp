// SPDX-License-Identifier: GPL-2.0-or-later
#include <array>
#include <atomic>
#include <barrier>
#include <cassert>
#include <cstdio>
#include <vector>
#include <vulkan/vulkan.h>
#include "video_core/renderer_vulkan/vk_async_compiler.h"

int main() {
    VkInstance instance{};
    VkInstanceCreateInfo instance_info{VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO};
    assert(vkCreateInstance(&instance_info, nullptr, &instance) == VK_SUCCESS);
    uint32_t count = 0;
    assert(vkEnumeratePhysicalDevices(instance, &count, nullptr) == VK_SUCCESS && count);
    std::vector<VkPhysicalDevice> physical(count);
    assert(vkEnumeratePhysicalDevices(instance, &count, physical.data()) == VK_SUCCESS);
    VkPhysicalDeviceProperties properties;
    vkGetPhysicalDeviceProperties(physical[0], &properties);
    vkGetPhysicalDeviceQueueFamilyProperties(physical[0], &count, nullptr);
    std::vector<VkQueueFamilyProperties> families(count);
    vkGetPhysicalDeviceQueueFamilyProperties(physical[0], &count, families.data());
    uint32_t family = 0;
    while (family < count && !(families[family].queueFlags & VK_QUEUE_COMPUTE_BIT)) {
        ++family;
    }
    assert(family < count);
    float priority = 1;
    VkDeviceQueueCreateInfo queue_info{VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO};
    queue_info.queueFamilyIndex = family;
    queue_info.queueCount = 1;
    queue_info.pQueuePriorities = &priority;
    VkDeviceCreateInfo device_info{VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO};
    device_info.queueCreateInfoCount = 1;
    device_info.pQueueCreateInfos = &queue_info;
    VkDevice device{};
    assert(vkCreateDevice(physical[0], &device_info, nullptr, &device) == VK_SUCCESS);
    // GLSL 450: layout(local_size_x=1) in; void main() {}. No dispatch or game resources.
    const uint32_t code[]{
        0x7230203, 0x10000, 0x8000b, 0xa, 0x0, 0x20011, 0x1, 0x6000b, 0x1,
        0x4c534c47, 0x6474732e, 0x3035342e, 0x0, 0x3000e, 0x0, 0x1, 0x5000f,
        0x5, 0x4, 0x6e69616d, 0x0, 0x60010, 0x4, 0x11, 0x1, 0x1, 0x1, 0x30003,
        0x2, 0x1c2, 0x40005, 0x4, 0x6e69616d, 0x0, 0x40047, 0x9, 0xb, 0x19,
        0x20013, 0x2, 0x30021, 0x3, 0x2, 0x40015, 0x6, 0x20, 0x0, 0x40017,
        0x7, 0x6, 0x3, 0x4002b, 0x6, 0x8, 0x1, 0x6002c, 0x7, 0x9, 0x8, 0x8,
        0x8, 0x50036, 0x2, 0x4, 0x0, 0x3, 0x200f8, 0x5, 0x100fd, 0x10038};
    VkShaderModuleCreateInfo shader_info{VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO};
    shader_info.codeSize = sizeof(code);
    shader_info.pCode = code;
    VkShaderModule shader{};
    assert(vkCreateShaderModule(device, &shader_info, nullptr, &shader) == VK_SUCCESS);
    VkPipelineLayoutCreateInfo layout_info{VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO};
    VkPipelineLayout layout{};
    assert(vkCreatePipelineLayout(device, &layout_info, nullptr, &layout) == VK_SUCCESS);
    VkPipelineCacheCreateInfo cache_info{VK_STRUCTURE_TYPE_PIPELINE_CACHE_CREATE_INFO};
    std::array<VkPipelineCache, 6> caches{};
    for (auto& cache : caches) {
        assert(vkCreatePipelineCache(device, &cache_info, nullptr, &cache) == VK_SUCCESS);
    }
    std::array<Vulkan::AsyncJob<VkPipeline>, 6> jobs;
    std::barrier start(6);
    std::atomic<unsigned> mask{};
    Vulkan::AsyncCompiler compiler(6);
    for (auto& job : jobs) {
        assert(job.Start(compiler, [&] {
            const auto index = Vulkan::AsyncCompiler::WorkerIndex();
            mask.fetch_or(1u << index);
            start.arrive_and_wait();
            VkComputePipelineCreateInfo info{VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO};
            info.stage.sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
            info.stage.stage = VK_SHADER_STAGE_COMPUTE_BIT;
            info.stage.module = shader;
            info.stage.pName = "main";
            info.layout = layout;
            VkPipeline pipeline{};
            assert(vkCreateComputePipelines(device, caches[index], 1, &info, nullptr, &pipeline) == VK_SUCCESS);
            size_t size = 0;
            assert(vkGetPipelineCacheData(device, caches[index], &size, nullptr) == VK_SUCCESS && size);
            return pipeline;
        }));
    }
    compiler.Stop();
    assert(mask == 63);
    VkPipelineCache merged{};
    assert(vkCreatePipelineCache(device, &cache_info, nullptr, &merged) == VK_SUCCESS);
    assert(vkMergePipelineCaches(device, merged, caches.size(), caches.data()) == VK_SUCCESS);
    for (auto& job : jobs) {
        auto pipeline = job.Poll();
        assert(pipeline && *pipeline);
        vkDestroyPipeline(device, *pipeline, nullptr);
    }
    for (auto cache : caches) {
        vkDestroyPipelineCache(device, cache, nullptr);
    }
    vkDestroyPipelineCache(device, merged, nullptr);
    vkDestroyPipelineLayout(device, layout, nullptr);
    vkDestroyShaderModule(device, shader, nullptr);
    vkDestroyDevice(device, nullptr);
    vkDestroyInstance(instance, nullptr);
    std::printf("async-pipeline-cache-test: OK (%s, 6 workers)\n", properties.deviceName);
}
