// gow3: the parts of shadPS4's kernel.h used by vendored libraries.
#pragma once
#include "common/types.h"
#include "core/libraries/kernel/orbis_error.h"
namespace Libraries::Kernel {
int* PS4_SYSV_ABI __Error();
} // namespace Libraries::Kernel
