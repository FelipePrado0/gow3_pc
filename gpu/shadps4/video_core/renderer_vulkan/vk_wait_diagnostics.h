// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once
#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <mutex>
#include <string_view>

namespace Vulkan {
class WaitDiagnostics {
public:
    struct Entry {
        std::string_view file{};
        unsigned line{}, calls{};
        uint64_t submit_ns{}, wait_ns{}, max_wait_ns{};
    };
    void Add(std::string_view file, unsigned line, uint64_t submit, uint64_t wait) {
        std::scoped_lock lock{mutex};
        auto* entry = &entries.back();
        for (unsigned i = 0; i + 1 < entries.size(); ++i) {
            if (!entries[i].calls || (entries[i].file == file && entries[i].line == line)) {
                entry = &entries[i];
                entry->file = file;
                entry->line = line;
                break;
            }
        }
        if (entry == &entries.back()) entry->file = "other wait sites";
        ++entry->calls;
        entry->submit_ns += submit;
        entry->wait_ns += wait;
        entry->max_wait_ns = std::max(entry->max_wait_ns, wait);
    }
    std::array<Entry, 64> Take() {
        std::scoped_lock lock{mutex};
        const auto result = entries;
        entries = {};
        return result;
    }
    static bool Enabled() {
        static const bool enabled = [] {
            const char* value = std::getenv("GOW3_PERF_DIAG");
            return value && value[0] == '1';
        }();
        return enabled;
    }
    static WaitDiagnostics& Get() {
        static WaitDiagnostics stats;
        return stats;
    }
    static void Report() {
        if (!Enabled()) return;
        auto rows = Get().Take();
        std::sort(rows.begin(), rows.end(), [](const Entry& a, const Entry& b) {
            return a.submit_ns + a.wait_ns > b.submit_ns + b.wait_ns;
        });
        for (const auto& row : rows) {
            if (!row.calls) continue;
            const auto filename = row.file.substr(row.file.find_last_of("/\\") + 1);
            std::printf("Wait profile: %.*s:%u; %u calls, CPU submit %.3f ms, semaphore wait %.3f ms, max wait %.3f ms\n",
                        int(filename.size()), filename.data(), row.line, row.calls,
                        row.submit_ns / 1e6, row.wait_ns / 1e6, row.max_wait_ns / 1e6);
        }
    }
private:
    std::mutex mutex;
    std::array<Entry, 64> entries{};
};
}
