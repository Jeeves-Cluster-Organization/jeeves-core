#pragma once

#include <jeeves/tools.hpp>

#include <memory>
#include <optional>
#include <variant>

namespace jeeves {
namespace detail { template <class T> struct QueueState; }

enum class RoutingReason { Success, Dynamic, ErrorRecovery };

struct RunEvent {
    struct StageStarted { StageAttempt attempt; };
    struct TextDelta { StageAttempt attempt; std::uint32_t model_call; std::string content; };
    struct ToolCallStarted { StageAttempt attempt; std::string call_id; std::string tool; json arguments; };
    struct ToolCallFinished { StageAttempt attempt; std::string call_id; std::string tool; Result<json> result; Duration duration; };
    struct ToolCallAborted { StageAttempt attempt; std::string call_id; std::string tool; Duration duration; };
    struct StageAttemptFinished { std::shared_ptr<StageRecord> record; };
    struct Routed { std::string from; std::optional<std::string> to; RoutingReason reason; };
    struct Finished { std::shared_ptr<RunOutcome> outcome; };

    using Value = std::variant<StageStarted, TextDelta, ToolCallStarted, ToolCallFinished,
                               ToolCallAborted, ApprovalRequest, StageAttemptFinished, Routed, Finished>;
    Value value;

    template <class T> RunEvent(T event) : value(std::move(event)) {}
};

class EventReceiver {
public:
    EventReceiver() = default;
    explicit EventReceiver(std::shared_ptr<detail::QueueState<RunEvent>> state) : state_(std::move(state)) {}
    EventReceiver(const EventReceiver &) = delete;
    EventReceiver & operator=(const EventReceiver &) = delete;
    EventReceiver(EventReceiver && other) noexcept : state_(std::move(other.state_)) {}
    EventReceiver & operator=(EventReceiver && other) noexcept;
    ~EventReceiver();
    std::optional<RunEvent> next();
    std::optional<RunEvent> recv() { return next(); }
    [[nodiscard]] explicit operator bool() const noexcept { return static_cast<bool>(state_); }

private:
    void reset();
    std::shared_ptr<detail::QueueState<RunEvent>> state_;
};

} // namespace jeeves
