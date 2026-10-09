// SPDX-License-Identifier: GPL-2.0-or-later
#include <atomic>
#include <cassert>
#include <future>
#include <stdexcept>
#include "video_core/renderer_vulkan/vk_async_compiler.h"

int main() {
    using Vulkan::AsyncCompiler;
    using Vulkan::AsyncJob;
    std::atomic<int> calls{};
    {
        AsyncCompiler compiler(1, 2);
        std::promise<void> release;
        auto gate = release.get_future().share();
        AsyncJob<int> job;
        assert(job.Start(compiler, [&] { gate.wait(); ++calls; return 42; }));
        assert(!job.Start(compiler, [&] { ++calls; return 99; }));
        assert(!job.Poll());
        assert(job.Pending());
        release.set_value();
        job.Wait();
        auto result = job.Poll();
        assert(result && *result == 42);
        assert(!job.Poll());
        assert(!job.Start(compiler, [] { return 0; }));
        assert(calls == 1);

        AsyncJob<int> failed;
        assert(failed.Start(compiler, []() -> int { throw std::runtime_error("compile failed"); }));
        failed.Wait();
        assert(!failed.Poll());
        assert(failed.Failed());
        assert(failed.Error() == "compile failed");
        assert(!failed.Start(compiler, [] { return 0; }));
        compiler.Stop();
        AsyncJob<int> stopped;
        assert(!stopped.Start(compiler, [] { return 0; }));
    }
    {
        AsyncCompiler compiler(1, 1);
        std::promise<void> entered, release;
        auto gate = release.get_future().share();
        assert(compiler.Submit([&] { entered.set_value(); gate.wait(); }));
        entered.get_future().wait();
        assert(compiler.Submit([&] { ++calls; }));
        AsyncJob<int> full;
        assert(!full.Start(compiler, [] { return 7; }));
        release.set_value();
        compiler.Stop();
        assert(calls == 2);
        AsyncCompiler retry(1);
        assert(full.Start(retry, [] { return 7; }));
        full.Wait();
        assert(full.Poll() == 7);
    }
    {
        AsyncCompiler compiler(1, 2);
        assert(compiler.Submit([&] { ++calls; }));
    }
    assert(calls == 3);
    {
        AsyncCompiler compiler(1, 4);
        std::promise<void> entered, release;
        auto gate = release.get_future().share();
        std::vector<int> order;
        assert(compiler.Submit([&] { entered.set_value(); gate.wait(); }));
        entered.get_future().wait();
        assert(compiler.Submit([&] { order.push_back(1); }));
        assert(compiler.Submit([&] { order.push_back(2); }, true));
        assert(compiler.Depth() == 2);
        release.set_value();
        compiler.Stop();
        assert((order == std::vector<int>{2, 1}));
        assert(compiler.QueueWaitNs() > 0);
    }
    {
        std::atomic<unsigned> initialized{};
        AsyncCompiler compiler(2, 4, [&](unsigned) { ++initialized; });
        std::promise<void> entered[2], release;
        auto gate = release.get_future().share();
        std::atomic<unsigned> mask{};
        for (unsigned i = 0; i < 2; ++i) {
            assert(compiler.Submit([&, i] {
                mask.fetch_or(1u << AsyncCompiler::WorkerIndex());
                entered[i].set_value();
                gate.wait();
            }));
        }
        entered[0].get_future().wait();
        entered[1].get_future().wait();
        release.set_value();
        compiler.Stop();
        assert(initialized == 2 && mask == 3);
    }
    {
        struct Storage { int used = 0; void ReleaseContents() { used = 0; } };
        Vulkan::CompilerPool<Storage> pool(1);
        auto [first, reused] = pool.Acquire();
        assert(!reused);
        auto* address = first.get();
        first->used = 7;
        pool.Recycle(std::move(first));
        auto [second, reused_again] = pool.Acquire();
        assert(reused_again && second.get() == address && second->used == 0);
        auto [third, reused_third] = pool.Acquire();
        assert(!reused_third && third.get() != second.get());
        pool.Recycle(std::move(second));
        pool.Recycle(std::move(third));
        assert(pool.Retained() == 1);
    }
    {
        struct Storage {
            void ReleaseContents() { throw std::runtime_error("pool reset failed"); }
        };
        Vulkan::CompilerPool<Storage> pool(1);
        auto [storage, reused] = pool.Acquire();
        pool.Recycle(std::move(storage));
        assert(pool.Retained() == 0);
    }
    {
        AsyncCompiler heavy(1), capture(1);
        std::promise<void> heavy_entered, release;
        auto gate = release.get_future().share();
        assert(heavy.Submit([&] { heavy_entered.set_value(); gate.wait(); }));
        heavy_entered.get_future().wait();
        std::vector<int> guest{1, 2, 3}, snapshot;
        AsyncJob<bool> job;
        assert(job.Start(capture, [&] { snapshot = guest; return true; }));
        job.Wait();
        assert(job.Poll() == true);
        guest.assign(3, 0);
        assert((snapshot == std::vector<int>{1, 2, 3}));
        release.set_value();
        // A capture failure cannot strand a waiter or prevent the next capture.
        AsyncJob<bool> failed;
        assert(failed.Start(capture, []() -> bool { throw std::runtime_error("capture failed"); }));
        failed.Wait();
        assert(!failed.Poll() && failed.Failed());
        AsyncJob<bool> next;
        assert(next.Start(capture, [] { return true; }));
        next.Wait();
        assert(next.Poll() == true);
    }
}
