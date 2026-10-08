// SPDX-License-Identifier: GPL-2.0-or-later
// gow3: a second thread for the GPU command thread's per-draw work (docs/parallel_gpu.md,
// "Texture binding on a helper thread"). The GPU thread forks one task per draw and joins it
// before anything that depends on the task's results; the helper spins for the next task
// while draws flow and sleeps after a short idle period.

#pragma once

#include <atomic>
#include <chrono>
#include <thread>
#include <x86intrin.h>

#include "common/thread.h"
#include "common/types.h"

namespace Vulkan {

class BindHelper {
public:
    using Task = void (*)(void*);

    bool running = false;

public:
    explicit BindHelper(bool enabled) {
        if (enabled) {
            thread = std::jthread([this](std::stop_token stop) { Run(stop); });
            running = true;
        }
    }

    ~BindHelper() {
        if (thread.joinable()) {
            thread.request_stop();
            posted.fetch_add(1, std::memory_order_seq_cst);
            posted.notify_one();
        }
    }

    [[nodiscard]] bool Available() const noexcept {
        return running; // gow3: not thread.joinable(), two system calls on Windows
    }

    /// True on the helper thread (its callees must not join it).
    [[nodiscard]] static bool OnHelper() noexcept {
        return on_helper;
    }

    /// True between Fork() and Join() (GPU thread only).
    [[nodiscard]] bool Active() const noexcept {
        return active;
    }

    /// Starts `task(context)` on the helper.
    void Fork(Task task_, void* context_) {
        task = task_;
        context = context_;
        active = true;
        posted.fetch_add(1, std::memory_order_seq_cst);
        if (sleeping.load(std::memory_order_seq_cst)) {
            posted.notify_one();
        }
    }

    /// Waits for the forked task; no-op when none is running.
    void Join() {
        if (!active) {
            return;
        }
        const u64 target = posted.load(std::memory_order_relaxed);
        if (done.load(std::memory_order_acquire) != target) {
            const u64 start = __rdtsc();
            while (done.load(std::memory_order_acquire) != target) {
                __builtin_ia32_pause();
            }
            wait_cycles += __rdtsc() - start;
        }
        active = false;
    }

    /// Cycles the GPU thread waited in Join() and the helper spent in tasks (GOW3_FRAME_STATS).
    u64 wait_cycles = 0;
    std::atomic<u64> task_cycles{0};

private:
    void Run(std::stop_token stop) {
        Common::SetCurrentThreadName("gow3:TexBind");
        on_helper = true;
        u64 seen = 0;
        while (!stop.stop_requested()) {
            // Draws arrive every few microseconds while a frame is recorded: spin, and sleep
            // only after a pause longer than that (between frames).
            const auto spin_until = std::chrono::steady_clock::now() + std::chrono::microseconds(100);
            u64 now = posted.load(std::memory_order_acquire);
            for (u32 spins = 1; now == seen; ++spins) {
                __builtin_ia32_pause();
                if (!(spins & 255) && std::chrono::steady_clock::now() >= spin_until) {
                    sleeping.store(true, std::memory_order_seq_cst);
                    posted.wait(seen, std::memory_order_seq_cst);
                    sleeping.store(false, std::memory_order_relaxed);
                }
                now = posted.load(std::memory_order_acquire);
            }
            if (stop.stop_requested()) {
                break;
            }
            seen = now;
            const u64 start = __rdtsc();
            task(context);
            task_cycles.fetch_add(__rdtsc() - start, std::memory_order_relaxed);
            done.store(seen, std::memory_order_release);
        }
    }

    static inline thread_local bool on_helper = false;
    Task task = nullptr;
    void* context = nullptr;
    bool active = false;
    alignas(64) std::atomic<u64> posted{0};
    alignas(64) std::atomic<u64> done{0};
    alignas(64) std::atomic<bool> sleeping{false};
    std::jthread thread;
};

} // namespace Vulkan
