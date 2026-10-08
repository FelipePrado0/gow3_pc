// gow3: profiler disabled.
#pragma once
#include "Tracy.hpp"
#define TracyVkCtx void*
#define TracyVkCollect(ctx, cmdbuf)
#define TracyVkNamedZoneC(ctx, var, cmd, name, color, active)
#define TracyVkContextName(ctx, name, size)
#define TracyVkContextHostCalibrated(...) nullptr
#define TracyVkDestroy(ctx)
namespace tracy { struct VkCtxScope { template <class... A> VkCtxScope(A&&...) {} }; }
