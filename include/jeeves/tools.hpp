#pragma once

#include <jeeves/types.hpp>

#include <memory>
#include <optional>
#include <string>

namespace jeeves {

struct ToolSpec {
    std::string name;
    std::string description;
    json parameters;
    static Result<ToolSpec> create(std::string name, std::string description, json parameters);
};

struct ApprovalPrompt {
    std::string message;
    std::optional<json> data;
    explicit ApprovalPrompt(std::string message_) : message(std::move(message_)) {}
    static ApprovalPrompt create(std::string message) { return ApprovalPrompt(std::move(message)); }
    ApprovalPrompt & with_data(json value) { data = std::move(value); return *this; }
};

struct ToolContext {
    const RunId & run_id;
    const std::string & workflow;
    const StageAttempt & attempt;
    const std::string & call_id;
    const json & metadata;
    std::stop_token cancellation;
};

class Tool {
public:
    virtual ~Tool() = default;
    virtual Result<json> call(const ToolContext & context, json arguments) = 0;
    virtual Result<std::optional<ApprovalPrompt>> approval(const ToolContext &, const json &) {
        return std::optional<ApprovalPrompt>{};
    }
};

enum class ReplaySafety { Unsafe, Idempotent };

struct ToolDefinition {
    ToolSpec spec;
    std::shared_ptr<Tool> handler;
    ReplaySafety replay_safety = ReplaySafety::Unsafe;
    ToolDefinition(ToolSpec spec_, std::shared_ptr<Tool> handler_)
        : spec(std::move(spec_)), handler(std::move(handler_)) {}
    ToolDefinition & with_replay_safety(ReplaySafety value) { replay_safety = value; return *this; }
    ToolDefinition & idempotent() { replay_safety = ReplaySafety::Idempotent; return *this; }
};

struct ApprovalResponse {
    std::string request_id;
    bool approved;
    static ApprovalResponse approve(std::string id) { return {std::move(id), true}; }
    static ApprovalResponse deny(std::string id) { return {std::move(id), false}; }
};

struct ApprovalRequest {
    std::string request_id;
    StageAttempt attempt;
    std::string call_id;
    std::string tool;
    json arguments;
    ApprovalPrompt prompt;
};

enum class DenialBehavior { Continue, FailStage, CancelRun };

} // namespace jeeves
