// SPDX-License-Identifier: GPL-2.0-or-later
#include <cassert>
#include <cstdlib>
#include <cstring>
#include <future>
#include "shader_recompiler/frontend/fetch_shader.h"
#include "common/logging/log.h"

extern "C" { uint32_t runtime_disabled_optimizations = 0; }
void assert_fail_impl() { std::abort(); }
[[noreturn]] void unreachable_impl() { std::abort(); }
namespace Common::Log {
bool ShouldLog(Level) { return false; }
void Write(std::string_view, Level, const char*, int, const char*, std::string&&) {}
}

int main() {
    using namespace Shader::Gcn;
    // s_load_dwordx4 s[8:11], s[2:3], 0; tbuffer_load_format_xyzw; s_setpc_b64.
    std::array<u32, 4> code{0xC0000000u | (2u << 22) | (8u << 15) | (1u << 9) | (1u << 8),
                            0xE8000000u | (3u << 16), 2u << 16, 0xBE802000u};
    AmdGpu::Buffer buffer{};
    buffer.base_address = 0x12340;
    buffer.stride = 16;
    std::array<u32, 4> ud{};
    const auto* code_pointer = code.data();
    const auto* buffer_pointer = &buffer;
    std::memcpy(ud.data(), &code_pointer, sizeof(code_pointer));
    std::memcpy(ud.data() + 2, &buffer_pointer, sizeof(buffer_pointer));
    Shader::Info info;
    info.has_fetch_shader = true;
    info.fetch_shader_sgpr_base = 0;
    info.user_data = ud;
    FetchShaderSnapshot snapshot;
    snapshot.Capture(info);
    assert(snapshot.fetch && snapshot.fetch->attributes.size() == 1);
    assert(snapshot.buffers[0].base_address == 0x12340);

    // Both guest pointers are invalid after capture. A compiler worker must not dereference them.
    ud.fill(0);
    auto worker = std::async(std::launch::async, [&] {
        ScopedFetchShaderSnapshot scope{snapshot};
        const auto fetch = ParseFetchShader(info);
        assert(fetch && fetch->attributes.size() == 1);
        const auto sharp = fetch->attributes[0].GetSharp(info);
        assert(sharp.base_address == 0x12340 && sharp.GetStride() == 16);
        FetchShaderSnapshot nested;
        nested.info = &info;
        {
            ScopedFetchShaderSnapshot inner{nested};
            assert(!ParseFetchShader(info));
        }
        assert(ParseFetchShader(info));
    });
    worker.get();
    info.has_fetch_shader = false;
    assert(!ParseFetchShader(info));
}
