#include <cassert>
#include <cstdio>
#include <cstring>
#include "video_core/texture_cache/readback_sources.h"

using VideoCore::ReadbackSource;
using VideoCore::ReadbackSourceStats;

int main() {
    assert(std::strcmp(VideoCore::ReadbackSourceName(ReadbackSource::ImageFree), "image-free") == 0);
    ReadbackSourceStats stats;
    stats.Record(ReadbackSource::CpuRead, true, 3'000'000, 4096);
    stats.Record(ReadbackSource::CpuRead, true, 1'000'000, 4096);
    stats.Record(ReadbackSource::CpuRead, false, 0, 8192);
    stats.Record(ReadbackSource::BufferUpload, false, 0, 64);
    auto read = stats.Get(ReadbackSource::CpuRead);
    assert(read.hits == 3 && read.waits == 2);
    assert(read.wait_ns == 4'000'000 && read.max_wait_ns == 3'000'000);
    assert(read.bytes == 16384);
    assert(stats.Get(ReadbackSource::BufferUpload).waits == 0);
    // Out-of-range sources fold into "unknown" instead of writing past the table.
    stats.Record(static_cast<ReadbackSource>(200), true, 5, 1);
    assert(stats.Get(ReadbackSource::Unknown).hits == 1);

    std::FILE* out = std::fopen("readback-sources-test.txt", "w+b");
    assert(out);
    stats.Report(out);
    std::rewind(out);
    char text[1024] = {};
    std::fread(text, 1, sizeof(text) - 1, out);
    assert(std::strstr(text, "Readback source cpu-read: hits 3, waits 2, wait 4.000 ms, max 3.000 ms"));
    assert(!std::strstr(text, "image-free"));
    assert(stats.Get(ReadbackSource::CpuRead).hits == 0);
    std::fclose(out);
    std::remove("readback-sources-test.txt");
}
