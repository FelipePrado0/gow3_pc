// gow3: fault-handler registry. The C loader's SIGSEGV handler calls
// gow3gpu_handle_fault() first; registered handlers run in priority order.
#pragma once
#include <compare>
#include <set>
#include <signal.h>
#include "common/singleton.h"
#include "common/types.h"

namespace Core {
using AccessViolationHandler = bool (*)(void* context, void* fault_address);
using IllegalInstructionHandler = bool (*)(void* context);

class SignalDispatch {
public:
    void RemoveHandlers() { access_violation_handlers.clear(); illegal_instruction_handlers.clear(); }
    void RegisterAccessViolationHandler(const AccessViolationHandler& handler, u32 priority) {
        access_violation_handlers.emplace(handler, priority);
    }
    void RegisterIllegalInstructionHandler(const IllegalInstructionHandler& handler, u32 priority) {
        illegal_instruction_handlers.emplace(handler, priority);
    }
    bool DispatchAccessViolation(void* context, void* fault_address) const {
        for (const auto& entry : access_violation_handlers)
            if (entry.handler(context, fault_address)) return true;
        return false;
    }
    bool DispatchIllegalInstruction(void* context) const {
        for (const auto& entry : illegal_instruction_handlers)
            if (entry.handler(context)) return true;
        return false;
    }

private:
    template <typename T>
    struct HandlerEntry {
        T handler;
        u32 priority;
        std::strong_ordering operator<=>(const HandlerEntry& right) const { return priority <=> right.priority; }
    };
    std::set<HandlerEntry<AccessViolationHandler>> access_violation_handlers;
    std::set<HandlerEntry<IllegalInstructionHandler>> illegal_instruction_handlers;
};
using Signals = Common::Singleton<SignalDispatch>;
} // namespace Core
