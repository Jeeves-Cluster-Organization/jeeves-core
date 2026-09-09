#include <jeeves/llm.hpp>

#include <cmath>
#include <unordered_set>

namespace jeeves {

Message Message::system(std::string content) { return {Role::System, std::move(content), std::nullopt, {}}; }
Message Message::user(std::string content) { return {Role::User, std::move(content), std::nullopt, {}}; }
Message Message::assistant(std::string content, std::vector<ToolCall> calls) {
    return {Role::Assistant, std::move(content), std::nullopt, std::move(calls)};
}
Message Message::tool(std::string call_id, std::string content) {
    return {Role::Tool, std::move(content), std::move(call_id), {}};
}

Result<std::string> Prompt::build(const RunView & run) const {
    if (const auto * value = std::get_if<std::string>(&value_)) return *value;
    return std::get<Builder>(value_)(run);
}

LlmAction LlmAction::text(Prompt prompt) { return LlmAction(std::move(prompt)); }
LlmAction LlmAction::structured(Prompt prompt, json schema, OutputValidator validate) {
    LlmAction action(std::move(prompt));
    action.output = {LlmOutputKind::Structured, std::move(schema), std::move(validate)};
    return action;
}

Result<void> LlmAction::validate() const {
    if (temperature && !std::isfinite(*temperature))
        return std::unexpected(Error::configuration("LLM temperature must be finite"));
    if (max_tokens == 0)
        return std::unexpected(Error::configuration("LLM max_tokens must be positive"));
    if (extra_body && !extra_body->is_object())
        return std::unexpected(Error::configuration("LLM extra_body must be an object"));
    std::unordered_set<std::string> names;
    for (const auto & tool : tools) {
        if (!tool || !tool->handler)
            return std::unexpected(Error::configuration("tool definition and handler cannot be null"));
        if (!names.insert(tool->spec.name).second)
            return std::unexpected(Error::configuration("duplicate LLM tool '" + tool->spec.name + "'"));
    }
    for (const auto & hook : hooks)
        if (!hook) return std::unexpected(Error::configuration("LLM hook cannot be null"));
    return {};
}

namespace {
class VectorStream final : public ModelStream {
public:
    explicit VectorStream(std::vector<ModelStreamEvent> events) : events_(std::move(events)) {}
    std::optional<Result<ModelStreamEvent>> next() override {
        if (position_ == events_.size()) return std::nullopt;
        return events_[position_++];
    }
private:
    std::vector<ModelStreamEvent> events_;
    std::size_t position_ = 0;
};
}

MockLlmProvider::MockLlmProvider(std::vector<std::vector<ModelStreamEvent>> responses)
    : responses_(responses.begin(), responses.end()) {}

std::shared_ptr<MockLlmProvider> MockLlmProvider::text(std::string response) {
    return std::make_shared<MockLlmProvider>(
        std::vector<std::vector<ModelStreamEvent>>{{ModelStreamEvent(std::move(response))}});
}

Result<std::unique_ptr<ModelStream>> MockLlmProvider::stream(const ModelRequest &) {
    std::lock_guard lock(mutex_);
    if (responses_.empty())
        return std::unexpected(Error::permanent("mock LLM has no response remaining"));
    auto events = std::move(responses_.front());
    responses_.pop_front();
    return std::unique_ptr<ModelStream>(std::make_unique<VectorStream>(std::move(events)));
}

} // namespace jeeves
