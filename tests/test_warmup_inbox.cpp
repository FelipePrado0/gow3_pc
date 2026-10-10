// Background pipeline cache warm-up helpers (vk_warmup_inbox.h).
#include <cassert>
#include <cstdio>
#include <map>
#include <memory>
#include <thread>
#include <vector>
#include "video_core/renderer_vulkan/vk_warmup_inbox.h"

using namespace Vulkan;

int main() {
    assert(WarmupThreadCount(1) == 1 && WarmupThreadCount(2) == 1 && WarmupThreadCount(4) == 1);
    assert(WarmupThreadCount(6) == 4 && WarmupThreadCount(8) == 6 && WarmupThreadCount(32) == 6);

    // Batches: never more than asked, oldest first, nothing lost across threads.
    WarmupInbox<int> inbox;
    std::vector<std::thread> producers;
    for (int t = 0; t < 4; ++t) {
        producers.emplace_back([&inbox, t] {
            for (int i = 0; i < 1000; ++i) {
                inbox.Push(t * 1000 + i);
            }
        });
    }
    for (auto& p : producers) {
        p.join();
    }
    std::vector<int> seen(4000, 0);
    size_t batches = 0;
    while (!inbox.Empty()) {
        const auto batch = inbox.Take(64);
        assert(!batch.empty() && batch.size() <= 64);
        for (int v : batch) {
            ++seen[v];
        }
        ++batches;
    }
    for (int count : seen) {
        assert(count == 1);
    }
    assert(batches == (4000 + 63) / 64);
    inbox.Push(1);
    inbox.Push(2);
    assert((inbox.Take(1) == std::vector<int>{1}) && (inbox.Take(8) == std::vector<int>{2}));
    assert(inbox.Take(8).empty());

    // Conflict: the first pipeline for a key wins, the later one is left to the caller.
    std::map<int, std::unique_ptr<int>> pipelines;
    auto runtime = std::make_unique<int>(1);
    auto warm = std::make_unique<int>(2);
    assert(PublishFirst(pipelines, 7, std::move(runtime)));
    assert(!PublishFirst(pipelines, 7, std::move(warm)));
    assert(*pipelines[7] == 1 && warm && *warm == 2);
    assert(PublishFirst(pipelines, 8, std::make_unique<int>(3)) && *pipelines[8] == 3);

    std::puts("PASS: warm-up inbox");
}
