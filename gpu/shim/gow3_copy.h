// gow3: parallel copies of guest memory. Streaming a new area uploads 50-400 MB of textures
// and buffers per frame; copied on one thread that is tens of milliseconds (stutter).
#pragma once
#include <cstddef>
#include <functional>

namespace Gow3Copy {

/// Runs `task(i)` for every i in [0, count) on the copy threads and the caller; returns when
/// all are done. Runs inline when count is 1, the pool is disabled, or when called from a copy
/// thread (no nesting).
void ParallelFor(std::size_t count, const std::function<void(std::size_t)>& task);

/// True when ParallelFor would split work (more than one thread available).
bool Enabled();

/// Starts `task` on a copy thread now (inline when the pool is disabled). Tasks run in any
/// order and concurrently.
void Async(std::function<void()> task);

/// A small copy for QueueCopy(): `run(item)` does the work on a copy thread.
struct Item {
    void (*run)(const Item&);
    void* context;
    unsigned long long source, destination, size, extra;
};

/// Queues a small copy in the calling thread's batch; the batch is started with Async() once it
/// holds about 512 KiB or 256 items, or by FlushBatch()/WaitAsync(). One wakeup per batch
/// instead of one per copy (thousands of constant buffer copies per frame).
void QueueCopy(const Item& item);

/// Starts the calling thread's batch.
void FlushBatch();

/// Starts the calling thread's batch, then waits until every task passed to Async() so far
/// has finished.
void WaitAsync();

/// Starts the calling thread's batch and runs `callback` once every task passed to Async() so
/// far has finished (at once when none is pending), on whichever thread finishes last.
/// Callbacks run in the order they were registered.
void AfterCopies(std::function<void()> callback);

} // namespace Gow3Copy
