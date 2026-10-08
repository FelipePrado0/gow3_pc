/* Independent backend smoke test: command submission and readback, no game graphics. */
#include <vulkan/vulkan.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#define CHECK(call) do { VkResult r = (call); if (r != VK_SUCCESS) { \
    fprintf(stderr, "Vulkan: %s returned %d\n", #call, r); exit(1); } } while (0)

int vulkan_smoke(void) {
    VkApplicationInfo app = {.sType=VK_STRUCTURE_TYPE_APPLICATION_INFO, .pApplicationName="God of War III native probe", .apiVersion=VK_API_VERSION_1_0};
    VkInstanceCreateInfo ici = {.sType=VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO, .pApplicationInfo=&app};
    VkInstance instance; CHECK(vkCreateInstance(&ici, NULL, &instance));
    uint32_t count = 0; CHECK(vkEnumeratePhysicalDevices(instance, &count, NULL));
    if (!count) { fprintf(stderr, "Vulkan: no physical devices\n"); vkDestroyInstance(instance, NULL); return 1; }
    VkPhysicalDevice *devices = calloc(count, sizeof(*devices));
    if (!devices) exit(1);
    CHECK(vkEnumeratePhysicalDevices(instance, &count, devices));
    VkPhysicalDevice physical = devices[0]; free(devices);
    VkPhysicalDeviceProperties properties; vkGetPhysicalDeviceProperties(physical, &properties);
    uint32_t nq = 0; vkGetPhysicalDeviceQueueFamilyProperties(physical, &nq, NULL);
    VkQueueFamilyProperties *families = calloc(nq, sizeof(*families));
    if (!families) exit(1);
    vkGetPhysicalDeviceQueueFamilyProperties(physical, &nq, families);
    uint32_t family = 0;
    while (family < nq && !(families[family].queueCount && (families[family].queueFlags & VK_QUEUE_GRAPHICS_BIT))) ++family;
    free(families);
    if (family == nq) { fprintf(stderr, "Vulkan: no graphics queue\n"); vkDestroyInstance(instance, NULL); return 1; }
    float priority = 1;
    VkDeviceQueueCreateInfo qci = {.sType=VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO, .queueFamilyIndex=family, .queueCount=1, .pQueuePriorities=&priority};
    VkDeviceCreateInfo dci = {.sType=VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO, .queueCreateInfoCount=1, .pQueueCreateInfos=&qci};
    VkDevice device; CHECK(vkCreateDevice(physical, &dci, NULL, &device));
    VkQueue queue; vkGetDeviceQueue(device, family, 0, &queue);
    VkBufferCreateInfo bci = {.sType=VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO, .size=4096, .usage=VK_BUFFER_USAGE_TRANSFER_DST_BIT, .sharingMode=VK_SHARING_MODE_EXCLUSIVE};
    VkBuffer buffer; CHECK(vkCreateBuffer(device, &bci, NULL, &buffer));
    VkMemoryRequirements requirements; vkGetBufferMemoryRequirements(device, buffer, &requirements);
    VkPhysicalDeviceMemoryProperties memory; vkGetPhysicalDeviceMemoryProperties(physical, &memory);
    uint32_t type = 0, wanted = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT;
    while (type < memory.memoryTypeCount && !((requirements.memoryTypeBits & (1u << type)) && (memory.memoryTypes[type].propertyFlags & wanted) == wanted)) ++type;
    if (type == memory.memoryTypeCount) { fprintf(stderr, "Vulkan: no coherent host memory\n"); exit(1); }
    VkMemoryAllocateInfo mai = {.sType=VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO, .allocationSize=requirements.size, .memoryTypeIndex=type};
    VkDeviceMemory allocation; CHECK(vkAllocateMemory(device, &mai, NULL, &allocation));
    CHECK(vkBindBufferMemory(device, buffer, allocation, 0));
    VkCommandPoolCreateInfo pci = {.sType=VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO, .queueFamilyIndex=family};
    VkCommandPool pool; CHECK(vkCreateCommandPool(device, &pci, NULL, &pool));
    VkCommandBufferAllocateInfo cai = {.sType=VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO, .commandPool=pool, .level=VK_COMMAND_BUFFER_LEVEL_PRIMARY, .commandBufferCount=1};
    VkCommandBuffer command; CHECK(vkAllocateCommandBuffers(device, &cai, &command));
    VkCommandBufferBeginInfo begin = {.sType=VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
    CHECK(vkBeginCommandBuffer(command, &begin));
    vkCmdFillBuffer(command, buffer, 0, 4096, 0xb100db0e);
    VkMemoryBarrier barrier = {.sType=VK_STRUCTURE_TYPE_MEMORY_BARRIER, .srcAccessMask=VK_ACCESS_TRANSFER_WRITE_BIT, .dstAccessMask=VK_ACCESS_HOST_READ_BIT};
    vkCmdPipelineBarrier(command, VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_HOST_BIT, 0, 1, &barrier, 0, NULL, 0, NULL);
    CHECK(vkEndCommandBuffer(command));
    VkSubmitInfo submit = {.sType=VK_STRUCTURE_TYPE_SUBMIT_INFO, .commandBufferCount=1, .pCommandBuffers=&command};
    CHECK(vkQueueSubmit(queue, 1, &submit, VK_NULL_HANDLE)); CHECK(vkQueueWaitIdle(queue));
    uint32_t *pixels; CHECK(vkMapMemory(device, allocation, 0, 4096, 0, (void **)&pixels));
    int ok = 1; for (int i=0; i<1024; ++i) if (pixels[i] != 0xb100db0e) ok = 0;
    vkUnmapMemory(device, allocation);
    printf("Vulkan: %s; command submission + 4096-byte readback %s\n", properties.deviceName, ok ? "PASS" : "FAIL");
    vkDestroyCommandPool(device, pool, NULL); vkDestroyBuffer(device, buffer, NULL);
    vkFreeMemory(device, allocation, NULL); vkDestroyDevice(device, NULL); vkDestroyInstance(instance, NULL);
    return ok ? 0 : 1;
}
