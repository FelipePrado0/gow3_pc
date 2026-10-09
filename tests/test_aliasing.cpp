// SPDX-License-Identifier: GPL-2.0-or-later
// Copy extents between images that share guest memory (texture_cache/aliasing.h); adapted from
// cuesta4/shadPS4 (eltutz) tests/test_aliasing.cpp. No game, Vulkan device or window required.
#include <cassert>
#include <cstdio>

#include "video_core/amdgpu/resource.h"
#include "video_core/texture_cache/aliasing.h"
#include "video_core/texture_cache/image_info.h"

using namespace VideoCore;

static ImageInfo Make(Extent3D size, vk::Format format = vk::Format::eR8G8B8A8Unorm) {
    ImageInfo info{};
    info.guest_address = 0x100000;
    info.num_bits = 32;
    info.pixel_format = format;
    info.type = AmdGpu::ImageType::Color2D;
    info.pitch = size.width;
    info.size = size;
    return info;
}

int main() {
    // Same format: the smaller image is fully covered by the larger one.
    {
        const ImageInfo src = Make({1920, 1088, 1}), dst = Make({1920, 1080, 1});
        assert(GetAliasCopyExtent(src, dst) == dst.size);
        assert(GetAliasCopyExtent(dst, src) == dst.size);
    }
    // Compatible views of other formats: the common rows.
    {
        const ImageInfo src = Make({1920, 1088, 1}, vk::Format::eR8G8B8A8Uint);
        const ImageInfo dst = Make({1920, 1080, 1}, vk::Format::eR8G8B8A8Unorm);
        const Extent3D rows{1920, 1080, 1};
        assert(GetAliasCopyExtent(src, dst) == rows);
        assert(GetAliasCopyExtent(dst, src) == rows);
    }
    // Other memory layouts never alias.
    {
        const ImageInfo src = Make({1920, 1088, 1});
        ImageInfo dst = Make({1920, 1080, 1});
        dst.guest_address += 0x1000;
        assert(!GetAliasCopyExtent(src, dst));
        dst = Make({1920, 1080, 1});
        ++dst.pitch;
        assert(!GetAliasCopyExtent(src, dst));
        dst = Make({1920, 1080, 1});
        dst.props.is_depth = true;
        assert(!GetAliasCopyExtent(src, dst));
    }
    // Incompatible intersections.
    {
        const ImageInfo src = Make({1920, 1088, 1}, vk::Format::eR8G8B8A8Uint);
        ImageInfo dst = Make({1280, 1080, 1}, vk::Format::eR8G8B8A8Unorm);
        assert(!GetAliasCopyExtent(src, dst));
        dst = Make({1920, 1080, 1}, vk::Format::eR16G16B16A16Sfloat);
        assert(!GetAliasCopyExtent(src, dst));
        dst = Make({1920, 1080, 1}, vk::Format::eR8G8B8A8Unorm);
        dst.resources.levels = 2;
        assert(!GetAliasCopyExtent(src, dst));
    }
    std::puts("aliasing-test: OK");
    return 0;
}
