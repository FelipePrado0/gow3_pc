// Regression: a pending readback (read watcher) must deny every access. Read denied with write
// allowed is an invalid protection on Windows and would let the CPU write before the readback.
#include <cassert>
#include "video_core/page_state.h"

using Core::MemoryPermission;
using VideoCore::PageState;

int main() {
    PageState state{};
    assert(state.Perms() == MemoryPermission::ReadWrite);

    state.AddDelta<1, true>();
    assert(state.Perms() == MemoryPermission::None);
    state.AddDelta<-1, true>();
    assert(state.Perms() == MemoryPermission::ReadWrite);

    state.AddDelta<1, false>();
    assert(state.Perms() == MemoryPermission::Read);
    for (int i = 1; i <= 65; ++i) {
        assert((state.AddDelta<1, true>() == i));
        assert(state.Perms() == MemoryPermission::None);
    }
    for (int i = 64; i >= 0; --i) {
        assert((state.AddDelta<-1, true>() == i));
        assert(i == 0 || state.Perms() == MemoryPermission::None);
    }
    assert(state.Perms() == MemoryPermission::Read);
    state.AddDelta<-1, false>();
    assert(state.Perms() == MemoryPermission::ReadWrite);
}
