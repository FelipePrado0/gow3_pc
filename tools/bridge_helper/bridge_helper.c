/* gow3: Wine side of the memory bridge test (tools/bridge_helper). Built with MinGW, run under
 * Proton's Wine. Imports a Vulkan buffer the native process exported as an opaque fd (inherited
 * fd number on the command line) through VK_KHR_external_memory_win32, checks the native
 * pattern and writes its own.
 *
 * usage: bridge_helper.exe <fd> <size> <device uuid hex> */
#define VK_USE_PLATFORM_WIN32_KHR
#define VK_NO_PROTOTYPES
#include <windows.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <vulkan/vulkan.h>

typedef LONG(WINAPI *FdToHandle)(int fd, unsigned int access, unsigned int attributes,
                                 HANDLE *handle);

static PFN_vkGetInstanceProcAddr get_instance_proc;
#define LOAD(instance, name) PFN_##name name = (PFN_##name)get_instance_proc(instance, #name)

static int fail(const char *what, int code) {
    printf("bridge_helper: %s (%d)\n", what, code);
    fflush(stdout);
    return 1;
}

int main(int argc, char **argv) {
    if (argc < 4) {
        return fail("usage: bridge_helper <fd> <size> <uuid>", 0);
    }
    const int fd = atoi(argv[1]);
    const VkDeviceSize size = strtoull(argv[2], NULL, 10);
    uint8_t uuid[VK_UUID_SIZE];
    for (int i = 0; i < VK_UUID_SIZE; ++i) {
        unsigned value = 0;
        sscanf(argv[3] + 2 * i, "%2x", &value);
        uuid[i] = (uint8_t)value;
    }

    HMODULE ntdll = GetModuleHandleA("ntdll.dll");
    FdToHandle fd_to_handle = (FdToHandle)GetProcAddress(ntdll, "wine_server_fd_to_handle");
    if (!fd_to_handle) {
        return fail("ntdll has no wine_server_fd_to_handle", 0);
    }
    HANDLE handle = NULL;
    LONG status = fd_to_handle(fd, GENERIC_ALL, 0, &handle);
    if (status != 0 || !handle) {
        return fail("wine_server_fd_to_handle failed", (int)status);
    }

    HMODULE vulkan = LoadLibraryA("vulkan-1.dll");
    if (!vulkan) {
        return fail("cannot load vulkan-1.dll", (int)GetLastError());
    }
    get_instance_proc = (PFN_vkGetInstanceProcAddr)GetProcAddress(vulkan, "vkGetInstanceProcAddr");
    LOAD(NULL, vkCreateInstance);
    const VkApplicationInfo app = {.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO,
                                   .apiVersion = VK_API_VERSION_1_3};
    const VkInstanceCreateInfo instance_ci = {.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
                                              .pApplicationInfo = &app};
    VkInstance instance;
    VkResult r = vkCreateInstance(&instance_ci, NULL, &instance);
    if (r) return fail("vkCreateInstance", r);
    LOAD(instance, vkEnumeratePhysicalDevices);
    LOAD(instance, vkGetPhysicalDeviceProperties2);
    LOAD(instance, vkGetPhysicalDeviceMemoryProperties);
    LOAD(instance, vkCreateDevice);
    LOAD(instance, vkGetDeviceProcAddr);

    VkPhysicalDevice devices[8];
    uint32_t count = 8;
    vkEnumeratePhysicalDevices(instance, &count, devices);
    VkPhysicalDevice physical = VK_NULL_HANDLE;
    for (uint32_t i = 0; i < count; ++i) {
        VkPhysicalDeviceIDProperties ids = {.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_ID_PROPERTIES};
        VkPhysicalDeviceProperties2 props = {.sType = VK_STRUCTURE_TYPE_PHYSICAL_DEVICE_PROPERTIES_2,
                                             .pNext = &ids};
        vkGetPhysicalDeviceProperties2(devices[i], &props);
        if (!memcmp(ids.deviceUUID, uuid, VK_UUID_SIZE)) {
            physical = devices[i];
            printf("bridge_helper: device %s\n", props.properties.deviceName);
        }
    }
    if (!physical) return fail("no device with the native UUID", (int)count);

    const float priority = 1.0f;
    const VkDeviceQueueCreateInfo queue_ci = {.sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
                                              .queueFamilyIndex = 0,
                                              .queueCount = 1,
                                              .pQueuePriorities = &priority};
    const char *extensions[] = {VK_KHR_EXTERNAL_MEMORY_WIN32_EXTENSION_NAME};
    const VkDeviceCreateInfo device_ci = {.sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
                                          .queueCreateInfoCount = 1,
                                          .pQueueCreateInfos = &queue_ci,
                                          .enabledExtensionCount = 1,
                                          .ppEnabledExtensionNames = extensions};
    VkDevice device;
    r = vkCreateDevice(physical, &device_ci, NULL, &device);
    if (r) return fail("vkCreateDevice (external_memory_win32)", r);
#define DLOAD(name) PFN_##name name = (PFN_##name)vkGetDeviceProcAddr(device, #name)
    DLOAD(vkCreateBuffer);
    DLOAD(vkGetBufferMemoryRequirements);
    DLOAD(vkAllocateMemory);
    DLOAD(vkBindBufferMemory);
    DLOAD(vkMapMemory);
    DLOAD(vkGetDeviceQueue);
    DLOAD(vkCreateCommandPool);
    DLOAD(vkAllocateCommandBuffers);
    DLOAD(vkBeginCommandBuffer);
    DLOAD(vkCmdCopyBuffer);
    DLOAD(vkCmdFillBuffer);
    DLOAD(vkCmdPipelineBarrier);
    DLOAD(vkEndCommandBuffer);
    DLOAD(vkQueueSubmit);
    DLOAD(vkQueueWaitIdle);

    const VkExternalMemoryBufferCreateInfo external_buffer = {
        .sType = VK_STRUCTURE_TYPE_EXTERNAL_MEMORY_BUFFER_CREATE_INFO,
        .handleTypes = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_WIN32_BIT};
    const VkBufferCreateInfo shared_ci = {.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
                                          .pNext = &external_buffer,
                                          .size = size,
                                          .usage = VK_BUFFER_USAGE_TRANSFER_SRC_BIT |
                                                   VK_BUFFER_USAGE_TRANSFER_DST_BIT};
    VkBuffer shared;
    vkCreateBuffer(device, &shared_ci, NULL, &shared);
    VkMemoryRequirements requirements;
    vkGetBufferMemoryRequirements(device, shared, &requirements);
    VkPhysicalDeviceMemoryProperties memory_props;
    vkGetPhysicalDeviceMemoryProperties(physical, &memory_props);
    uint32_t type = 0;
    while (!(requirements.memoryTypeBits & (1u << type))) ++type;
    const VkImportMemoryWin32HandleInfoKHR import = {
        .sType = VK_STRUCTURE_TYPE_IMPORT_MEMORY_WIN32_HANDLE_INFO_KHR,
        .handleType = VK_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_WIN32_BIT,
        .handle = handle};
    const VkMemoryAllocateInfo shared_alloc = {.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
                                               .pNext = &import,
                                               .allocationSize = requirements.size,
                                               .memoryTypeIndex = type};
    VkDeviceMemory shared_memory;
    r = vkAllocateMemory(device, &shared_alloc, NULL, &shared_memory);
    if (r) return fail("vkAllocateMemory import", r);
    vkBindBufferMemory(device, shared, shared_memory, 0);

    /* Readback buffer (host visible). */
    const VkBufferCreateInfo read_ci = {.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
                                        .size = size,
                                        .usage = VK_BUFFER_USAGE_TRANSFER_DST_BIT};
    VkBuffer readback;
    vkCreateBuffer(device, &read_ci, NULL, &readback);
    vkGetBufferMemoryRequirements(device, readback, &requirements);
    type = 0;
    while (!((requirements.memoryTypeBits & (1u << type)) &&
             (memory_props.memoryTypes[type].propertyFlags &
              (VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)) ==
                 (VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)))
        ++type;
    const VkMemoryAllocateInfo read_alloc = {.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
                                             .allocationSize = requirements.size,
                                             .memoryTypeIndex = type};
    VkDeviceMemory read_memory;
    vkAllocateMemory(device, &read_alloc, NULL, &read_memory);
    vkBindBufferMemory(device, readback, read_memory, 0);

    VkQueue queue;
    vkGetDeviceQueue(device, 0, 0, &queue);
    const VkCommandPoolCreateInfo pool_ci = {.sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO};
    VkCommandPool pool;
    vkCreateCommandPool(device, &pool_ci, NULL, &pool);
    const VkCommandBufferAllocateInfo cb_ai = {.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
                                               .commandPool = pool,
                                               .level = VK_COMMAND_BUFFER_LEVEL_PRIMARY,
                                               .commandBufferCount = 1};
    VkCommandBuffer cmd;
    vkAllocateCommandBuffers(device, &cb_ai, &cmd);
    const VkCommandBufferBeginInfo begin = {.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
    vkBeginCommandBuffer(cmd, &begin);
    const VkBufferCopy region = {.size = size};
    vkCmdCopyBuffer(cmd, shared, readback, 1, &region);
    const VkMemoryBarrier barrier = {.sType = VK_STRUCTURE_TYPE_MEMORY_BARRIER,
                                     .srcAccessMask = VK_ACCESS_TRANSFER_READ_BIT,
                                     .dstAccessMask = VK_ACCESS_TRANSFER_WRITE_BIT};
    vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT, 0, 1,
                         &barrier, 0, NULL, 0, NULL);
    vkCmdFillBuffer(cmd, shared, 0, size, 0x5EED1234u);
    vkEndCommandBuffer(cmd);
    const VkSubmitInfo submit = {.sType = VK_STRUCTURE_TYPE_SUBMIT_INFO,
                                 .commandBufferCount = 1,
                                 .pCommandBuffers = &cmd};
    r = vkQueueSubmit(queue, 1, &submit, VK_NULL_HANDLE);
    if (r) return fail("vkQueueSubmit", r);
    vkQueueWaitIdle(queue);

    uint32_t *words;
    vkMapMemory(device, read_memory, 0, size, 0, (void **)&words);
    size_t bad = 0;
    for (size_t i = 0; i < size / 4; ++i) {
        bad += words[i] != 0xB12D6E00u + (uint32_t)(i & 0xFF);
    }
    printf("bridge_helper: native pattern %s (%zu bad words of %zu); wrote 0x5EED1234\n",
           bad ? "WRONG" : "OK", bad, (size_t)(size / 4));
    fflush(stdout);
    return bad ? 2 : 0;
}
