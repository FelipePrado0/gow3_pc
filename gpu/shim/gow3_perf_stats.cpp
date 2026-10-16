// SPDX-License-Identifier: GPL-2.0-or-later
#include "gow3_perf_stats.h"

#include <chrono>
#include <mutex>
#include <thread>
#include <vector>
#include "video_core/renderer_vulkan/vk_instance.h"

#ifdef _WIN32
#include <windows.h>
#include <pdh.h>
#include <pdhmsg.h>
#include <psapi.h>
#elif defined(__APPLE__)
#include <cstdio>
#include <CoreFoundation/CoreFoundation.h>
#include <IOKit/IOKitLib.h>
#include <mach/mach.h>
#include <sys/resource.h>
#include <sys/sysctl.h>
#endif

namespace Gow3PerfStats {

namespace {

std::mutex mutex;
Sample latest;
std::once_flag started;

#ifdef _WIN32
uint64_t Ticks(const FILETIME& time) {
    return (uint64_t(time.dwHighDateTime) << 32) | time.dwLowDateTime;
}

// This process's use of the GPU's 3D engines, as Task Manager shows it ("GPU Engine" counters,
// any vendor). Negative until the second sample or without the counters.
class GpuUsage {
public:
    GpuUsage() {
        if (PdhOpenQueryW(nullptr, 0, &query) != ERROR_SUCCESS) {
            query = nullptr;
            return;
        }
        if (PdhAddEnglishCounterW(query, L"\\GPU Engine(*)\\Utilization Percentage", 0, &counter) !=
            ERROR_SUCCESS) {
            PdhCloseQuery(query);
            query = nullptr;
            return;
        }
        PdhCollectQueryData(query);
        pid = L"pid_" + std::to_wstring(GetCurrentProcessId()) + L"_";
    }

    float Percent() {
        if (!query || PdhCollectQueryData(query) != ERROR_SUCCESS) {
            return -1.0f;
        }
        DWORD size = 0, count = 0;
        if (PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE | PDH_FMT_NOCAP100, &size, &count,
                                         nullptr) != PDH_MORE_DATA) {
            return -1.0f;
        }
        buffer.resize(size);
        auto* items = reinterpret_cast<PDH_FMT_COUNTERVALUE_ITEM_W*>(buffer.data());
        if (PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE | PDH_FMT_NOCAP100, &size, &count,
                                         items) != ERROR_SUCCESS) {
            return -1.0f;
        }
        double total = 0.0;
        for (DWORD i = 0; i < count; ++i) {
            const wchar_t* name = items[i].szName;
            if (items[i].FmtValue.CStatus == PDH_CSTATUS_VALID_DATA && wcsstr(name, pid.c_str()) &&
                wcsstr(name, L"engtype_3D")) {
                total += items[i].FmtValue.doubleValue;
            }
        }
        return std::clamp(float(total), 0.0f, 100.0f);
    }

private:
    PDH_HQUERY query = nullptr;
    PDH_HCOUNTER counter = nullptr;
    std::wstring pid;
    std::vector<unsigned char> buffer;
};

void Run(const Vulkan::Instance* instance) {
    GpuUsage gpu;
    FILETIME created, ended, kernel, user, now;
    uint64_t last_process = 0, last_wall = 0;
    const unsigned cores = std::max(1u, std::thread::hardware_concurrency());
    constexpr double GiB = 1024.0 * 1024.0 * 1024.0;
    while (true) {
        Sample s;
        s.gpu_name = std::string(instance->GetModelName());
        GetSystemTimeAsFileTime(&now);
        if (GetProcessTimes(GetCurrentProcess(), &created, &ended, &kernel, &user)) {
            const uint64_t process = Ticks(kernel) + Ticks(user), wall = Ticks(now);
            if (last_wall) {
                s.cpu_percent = CpuPercent(process - last_process, wall - last_wall, cores);
            }
            last_process = process;
            last_wall = wall;
        }
        PROCESS_MEMORY_COUNTERS memory{};
        if (K32GetProcessMemoryInfo(GetCurrentProcess(), &memory, sizeof(memory))) {
            s.ram_used_gb = double(memory.WorkingSetSize) / GiB;
        }
        MEMORYSTATUSEX system{.dwLength = sizeof(MEMORYSTATUSEX)};
        if (GlobalMemoryStatusEx(&system)) {
            s.ram_total_gb = double(system.ullTotalPhys) / GiB;
        }
        if (instance->CanReportMemoryUsage()) {
            s.vram_used_gb = double(instance->GetDeviceMemoryUsage()) / GiB;
            s.vram_total_gb = double(instance->GetDeviceMemoryBudgetNow()) / GiB;
        }
        s.gpu_percent = gpu.Percent();
        {
            std::scoped_lock lock{mutex};
            latest = std::move(s);
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(500));
    }
}
#elif defined(__APPLE__)
// gow3 (macOS): the GPU's use as Activity Monitor shows it, from the accelerator's
// PerformanceStatistics. It is the whole GPU's: macOS has no counter per process. Negative
// without the value.
float GpuPercent() {
    io_iterator_t iterator = 0;
    if (IOServiceGetMatchingServices(kIOMainPortDefault, IOServiceMatching("IOAccelerator"), &iterator) !=
        KERN_SUCCESS) {
        return -1.0f;
    }
    float percent = -1.0f;
    while (const io_registry_entry_t entry = IOIteratorNext(iterator)) {
        if (const CFTypeRef stats = IORegistryEntryCreateCFProperty(entry, CFSTR("PerformanceStatistics"),
                                                                   kCFAllocatorDefault, 0)) {
            if (CFGetTypeID(stats) == CFDictionaryGetTypeID()) {
                const void* value =
                    CFDictionaryGetValue(static_cast<CFDictionaryRef>(stats), CFSTR("Device Utilization %"));
                int64_t number = 0;
                if (value && CFGetTypeID(value) == CFNumberGetTypeID() &&
                    CFNumberGetValue(static_cast<CFNumberRef>(value), kCFNumberSInt64Type, &number)) {
                    percent = std::max(percent, float(number));
                }
            }
            CFRelease(stats);
        }
        IOObjectRelease(entry);
    }
    IOObjectRelease(iterator);
    return percent < 0.0f ? -1.0f : std::clamp(percent, 0.0f, 100.0f);
}

// Activity Monitor's "Memory" for this process (its physical footprint), in bytes; 0 without it.
uint64_t Footprint() {
    task_vm_info_data_t info{};
    mach_msg_type_number_t count = TASK_VM_INFO_COUNT;
    if (task_info(mach_task_self(), TASK_VM_INFO, reinterpret_cast<task_info_t>(&info), &count) != KERN_SUCCESS) {
        return 0;
    }
    return info.phys_footprint;
}

uint64_t CpuMicros() {
    rusage usage{};
    getrusage(RUSAGE_SELF, &usage);
    const auto micros = [](const timeval& t) { return uint64_t(t.tv_sec) * 1000000 + uint64_t(t.tv_usec); };
    return micros(usage.ru_utime) + micros(usage.ru_stime);
}

void Run(const Vulkan::Instance* instance) {
    uint64_t last_cpu = 0;
    std::chrono::steady_clock::time_point last_wall{};
    const unsigned cores = std::max(1u, std::thread::hardware_concurrency());
    constexpr double GiB = 1024.0 * 1024.0 * 1024.0;
    uint64_t installed = 0;
    size_t size = sizeof(installed);
    sysctlbyname("hw.memsize", &installed, &size, nullptr, 0);
    bool reported = false;
    while (true) {
        Sample s;
        s.gpu_name = std::string(instance->GetModelName());
        const auto now = std::chrono::steady_clock::now();
        const uint64_t cpu = CpuMicros();
        if (last_wall != std::chrono::steady_clock::time_point{}) {
            const auto wall = std::chrono::duration_cast<std::chrono::microseconds>(now - last_wall).count();
            s.cpu_percent = CpuPercent(cpu - last_cpu, uint64_t(wall), cores);
        }
        last_cpu = cpu;
        last_wall = now;
        if (const uint64_t used = Footprint()) {
            s.ram_used_gb = double(used) / GiB;
        }
        if (installed) {
            s.ram_total_gb = double(installed) / GiB;
        }
        if (instance->CanReportMemoryUsage()) {
            s.vram_used_gb = double(instance->GetDeviceMemoryUsage()) / GiB;
            s.vram_total_gb = double(instance->GetDeviceMemoryBudgetNow()) / GiB;
        }
        s.gpu_percent = GpuPercent();
        if (!reported && s.cpu_percent >= 0.0f) {
            // Once, for reports: what the overlay's measured items read.
            std::printf("Performance overlay: GPU %.0f%% (whole GPU), CPU %.0f%%, RAM %.1f / %.1f GB, VRAM %.1f / %.1f GB\n",
                        s.gpu_percent, s.cpu_percent, s.ram_used_gb, s.ram_total_gb, s.vram_used_gb,
                        s.vram_total_gb);
            std::fflush(stdout);
            reported = true;
        }
        {
            std::scoped_lock lock{mutex};
            latest = std::move(s);
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(500));
    }
}
#else
void Run(const Vulkan::Instance* instance) {
    while (true) {
        Sample s;
        s.gpu_name = std::string(instance->GetModelName());
        {
            std::scoped_lock lock{mutex};
            latest = std::move(s);
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(500));
    }
}
#endif

} // namespace

void Start(const Vulkan::Instance& instance) {
    // ponytail: never joined; a thread that ended while the game ran crashed in ntdll
    // (vk_pipeline_serialization.cpp, StopWarmUp). It sleeps between samples.
    std::call_once(started, [&] { std::thread(Run, &instance).detach(); });
}

Sample Latest() {
    std::scoped_lock lock{mutex};
    return latest;
}

} // namespace Gow3PerfStats
