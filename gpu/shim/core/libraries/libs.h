// gow3: shadPS4 registers HLE functions with LIB_FUNCTION(nid, library,
// version, module, fn). Here the registrations fill a table that the C
// loader queries by NID (see gow3gpu.cpp: gow3gpu_resolve).
#pragma once
#include <string>
#include "common/types.h"

namespace Core::Loader {
enum class SymbolType { Function, Object };
class SymbolsResolver {
public:
    void AddSymbol(const char* nid, const char* library, const char* module, SymbolType type, u64 address);
};
} // namespace Core::Loader

#define LIB_FUNCTION(nid, lib, libversion, mod, function)                                          \
    sym->AddSymbol(nid, lib, mod, Core::Loader::SymbolType::Function,                             \
                   reinterpret_cast<u64>(function))
#define LIB_OBJ(nid, lib, libversion, mod, obj)                                                    \
    sym->AddSymbol(nid, lib, mod, Core::Loader::SymbolType::Object, reinterpret_cast<u64>(obj))
