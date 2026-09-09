#pragma once

#include <jeeves/tools.hpp>

#include <deque>
#include <functional>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <variant>
#include <vector>

namespace jeeves {

enum class Role { System, User, Assistant, Tool };

struct ToolCall {
    std::string id;
    std::string name;
    json arguments;
};

struct Message {
    Role role;
    std::string content;
    std::optional<std::string> tool_call_id;
    std::vector<ToolCall> tool_calls;
    static Message system(std::string content);
    static Message user(std::string content);
    static Message assistant(std::string content, std::vector<ToolCall> calls);
    static Message tool(std::string call_id, std::string content);
};

struct TokenUsage {
    std::uint64_t input_tokens = 0;
    std::uint64_t output_tokens = 0;
    friend bool operator==(const TokenUsage &, const TokenUsage &) = default;
};

struct ModelRequest {
    std::vector<Message> messages;
    std::vector<ToolSpec> tools;
    std::optional<double> temperature;
    std::optional<std::uint32_t> max_tokens;
    std::optional<std::string> model;
    std::optional<json> response_schema;
    std::optional<json> extra_body;
    std::stop_token stop;
};

enum class ModelStopKind { Completed, MaxTokens, ToolCall, ContentFilter, StopSequence, Other };
struct ModelStopReason {
    ModelStopKind kind = ModelStopKind::Completed;
    std::string detail;
    static ModelStopReason completed(std::string value = {}) { return {ModelStopKind::Completed, std::move(value)}; }
    static ModelStopReason max_tokens(std::string value = {}) { return {ModelStopKind::MaxTokens, std::move(value)}; }
    static ModelStopReason tool_call(std::string value = {}) { return {ModelStopKind::ToolCall, std::move(value)}; }
    static ModelStopReason content_filter(std::string value = {}) { return {ModelStopKind::ContentFilter, std::move(value)}; }
    static ModelStopReason stop_sequence(std::string value = {}) { return {ModelStopKind::StopSequence, std::move(value)}; }
    static ModelStopReason other(std::string value = {}) { return {ModelStopKind::Other, std::move(value)}; }
    friend bool operator==(const ModelStopReason &, const ModelStopReason &) = default;
};

struct ModelStreamEvent {
    using Value = std::variant<std::string, ToolCall, TokenUsage, ModelStopReason>;
    Value value;
    ModelStreamEvent(std::string text) : value(std::move(text)) {}
    ModelStreamEvent(const char * text) : value(std::string(text)) {}
    ModelStreamEvent(ToolCall call) : value(std::move(call)) {}
    ModelStreamEvent(TokenUsage usage) : value(usage) {}
    ModelStreamEvent(ModelStopReason reason) : value(std::move(reason)) {}
    static ModelStreamEvent text(std::string value) { return ModelStreamEvent(std::move(value)); }
    static ModelStreamEvent tool_call(ToolCall value) { return ModelStreamEvent(std::move(value)); }
    static ModelStreamEvent usage(TokenUsage value) { return ModelStreamEvent(value); }
    static ModelStreamEvent stop_reason(ModelStopReason value) { return ModelStreamEvent(std::move(value)); }
};

class ModelStream {
public:
    virtual ~ModelStream() = default;
    virtual std::optional<Result<ModelStreamEvent>> next() = 0;
};

class LlmProvider {
public:
    virtual ~LlmProvider() = default;
    virtual Result<std::unique_ptr<ModelStream>> stream(const ModelRequest & request) = 0;
};

class PromptBuilder {
public:
    virtual ~PromptBuilder() = default;
    virtual Result<std::string> build(const RunView & run) = 0;
};

class Prompt {
public:
    using Builder = std::function<Result<std::string>(const RunView &)>;
    static Prompt text(std::string value) { return Prompt(std::move(value)); }
    static Prompt dynamic(Builder builder) { return Prompt(std::move(builder)); }
    static Prompt dynamic(std::shared_ptr<PromptBuilder> builder) {
        return dynamic([builder = std::move(builder)](const RunView & run) { return builder->build(run); });
    }
    Result<std::string> build(const RunView & run) const;

private:
    explicit Prompt(std::string value) : value_(std::move(value)) {}
    explicit Prompt(Builder builder) : value_(std::move(builder)) {}
    std::variant<std::string, Builder> value_;
};

struct ModelResponse {
    std::string text;
    std::vector<ToolCall> tool_calls;
    TokenUsage usage;
    std::optional<ModelStopReason> stop_reason;
};

enum class ToolDecisionKind { Continue, Reject, Replace };
struct ToolDecision {
    ToolDecisionKind kind = ToolDecisionKind::Continue;
    std::string reason;
    json replacement;
    static ToolDecision continue_() { return {}; }
    static ToolDecision reject(std::string reason) { return {ToolDecisionKind::Reject, std::move(reason), nullptr}; }
    static ToolDecision replace(json value) { return {ToolDecisionKind::Replace, {}, std::move(value)}; }
};

struct ToolResult { json value; bool succeeded = false; };

class LlmLoopHook {
public:
    virtual ~LlmLoopHook() = default;
    virtual Result<void> before_model(std::vector<Message> &) { return {}; }
    virtual Result<void> after_model(ModelResponse &) { return {}; }
    virtual Result<ToolDecision> before_tool(const ToolCall &) { return ToolDecision::continue_(); }
    virtual Result<void> after_tool(const ToolCall &, ToolResult &) { return {}; }
};

using OutputValidator = std::function<Result<void>(const json &)>;
enum class LlmOutputKind { Text, Structured };
struct LlmOutput {
    LlmOutputKind kind = LlmOutputKind::Text;
    json schema;
    OutputValidator validate;
};

class LlmAction {
public:
    Prompt prompt;
    LlmOutput output;
    std::vector<std::shared_ptr<ToolDefinition>> tools;
    std::vector<std::shared_ptr<LlmLoopHook>> hooks;
    std::optional<double> temperature;
    std::optional<std::uint32_t> max_tokens;
    std::optional<std::string> model;
    std::optional<json> extra_body;
    std::uint32_t max_tool_rounds = 10;
    DenialBehavior on_denied_behavior = DenialBehavior::Continue;

    static LlmAction text(Prompt prompt);
    static LlmAction structured(Prompt prompt, json schema, OutputValidator validate);
    LlmAction & with_tools(std::vector<std::shared_ptr<ToolDefinition>> value) { tools = std::move(value); return *this; }
    LlmAction & with_hook(std::shared_ptr<LlmLoopHook> value) { hooks.push_back(std::move(value)); return *this; }
    LlmAction & with_model(std::string value) { model = std::move(value); return *this; }
    LlmAction & with_temperature(double value) { temperature = value; return *this; }
    LlmAction & with_max_tokens(std::uint32_t value) { max_tokens = value; return *this; }
    LlmAction & with_extra_body(json value) { extra_body = std::move(value); return *this; }
    LlmAction & with_max_tool_rounds(std::uint32_t value) { max_tool_rounds = value; return *this; }
    LlmAction & on_denied(DenialBehavior value) { on_denied_behavior = value; return *this; }
    Result<void> validate() const;

private:
    explicit LlmAction(Prompt value) : prompt(std::move(value)) {}
};

class MockLlmProvider final : public LlmProvider {
public:
    explicit MockLlmProvider(std::vector<std::vector<ModelStreamEvent>> responses);
    MockLlmProvider(std::initializer_list<std::vector<ModelStreamEvent>> responses)
        : MockLlmProvider(std::vector<std::vector<ModelStreamEvent>>(responses)) {}
    static std::shared_ptr<MockLlmProvider> text(std::string response);
    Result<std::unique_ptr<ModelStream>> stream(const ModelRequest &) override;

private:
    std::mutex mutex_;
    std::deque<std::vector<ModelStreamEvent>> responses_;
};

} // namespace jeeves
