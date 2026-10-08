// gow3: devtools GPU memory viewer is not part of the port.
#pragma once
#include <string>
#include <vector>
#include "common/types.h"
namespace Core::Devtools::Widget {
struct GpuMemoryRow { std::string name; u64 bytes; };
struct GpuMemoryGroup { std::string name; std::vector<GpuMemoryRow> rows; };
struct GpuMemoryViewer {
    static bool IsEnabled() { return false; }
    static void Publish(GpuMemoryGroup&&) {}
};
} // namespace Core::Devtools::Widget
