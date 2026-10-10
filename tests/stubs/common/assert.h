// Test stand-in for shadps4/common/assert.h: plain assert, no logging backend to link.
#pragma once
#include <cassert>
#define ASSERT(condition) assert(condition)
#define ASSERT_MSG(condition, ...) assert(condition)
