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
}
