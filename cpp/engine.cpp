#include <jeeves/engine.hpp>

#include "sync.hpp"

#include <algorithm>
#include <condition_variable>
#include <exception>
#include <thread>
#include <unordered_map>

namespace jeeves {
namespace detail {
struct EngineInner {
    std::unordered_map<std::string, std::shared_ptr<Workflow>> workflows;
    std::shared_ptr<LlmProvider> llm;
};

struct RunLifetime {
    std::stop_source stop;
    std::optional<std::jthread> worker;
    ~RunLifetime() {
        stop.request_stop();
        // The worker owns its execution data; dropping a handle requests
        // cancellation without waiting on consumer code that may ignore it.
        if (worker && worker->joinable()) worker->detach();
    }
};
} // namespace detail

namespace detail {

enum class StopKind { Completed, Failed, Cancelled, Limit };
struct Stop {
    StopKind kind;
    std::optional<Error> error;
    std::optional<LimitKind> limit;
    static Stop completed() { return {StopKind::Completed, {}, {}}; }
    static Stop failed(Error e) { return {StopKind::Failed, std::move(e), {}}; }
    static Stop cancelled() { return {StopKind::Cancelled, {}, {}}; }
    static Stop limited(LimitKind l) { return {StopKind::Limit, {}, l}; }
};

enum class ActionStopKind { Error, Cancelled, Limit };
struct ActionStop {
    ActionStopKind kind;
    std::optional<Error> error;
    std::optional<LimitKind> limit;
    static ActionStop failed(Error e) { return {ActionStopKind::Error, std::move(e), {}}; }
    static ActionStop cancelled() { return {ActionStopKind::Cancelled, {}, {}}; }
    static ActionStop limited(LimitKind l) { return {ActionStopKind::Limit, {}, l}; }
};
using ActionResult = std::expected<json, ActionStop>;

std::string exception_message() {
    try { throw; }
    catch (const std::exception & error) { return error.what(); }
    catch (...) { return "non-standard exception"; }
}

std::string duration_debug(Duration duration) {
    const auto nanos = std::chrono::duration_cast<std::chrono::nanoseconds>(duration).count();
    const auto divisor = nanos >= 1000000000 ? 1000000000 : nanos >= 1000000 ? 1000000 : nanos >= 1000 ? 1000 : 1;
    const auto * unit = divisor == 1000000000 ? "s" : divisor == 1000000 ? "ms" : divisor == 1000 ? "µs" : "ns";
    auto text = std::to_string(nanos / divisor);
    if (const auto fraction = nanos % divisor; fraction != 0) {
        auto digits = std::to_string(divisor + fraction).substr(1);
        while (digits.back() == '0') digits.pop_back();
        text += "." + digits;
    }
    return text + unit;
}

std::string limit_name(LimitKind kind) {
    switch (kind) {
        case LimitKind::StageExecutions: return "StageExecutions";
        case LimitKind::LlmCalls: return "LlmCalls";
        case LimitKind::ToolCalls: return "ToolCalls";
        case LimitKind::StageVisits: return "StageVisits";
        case LimitKind::ToolRounds: return "ToolRounds";
        case LimitKind::Deadline: return "Deadline";
    }
    return "Unknown";
}

struct ExecutionState {
    RunId run_id;
    RunInput input;
    json state = nullptr;
    std::vector<StageRecord> history;
    Usage usage;
    std::uint32_t stage_executions = 0;
    std::unordered_map<std::string, std::uint32_t> visits;
    std::chrono::steady_clock::time_point started = std::chrono::steady_clock::now();
    std::stop_token stop;

    RunView view(std::stop_token action_stop = {}) const {
        return {run_id, input.input, input.metadata, state, history, usage,
                action_stop.stop_possible() ? action_stop : stop};
    }
};

class ToolCallGuard {
public:
    ToolCallGuard(std::shared_ptr<detail::QueueState<RunEvent>> events, StageAttempt attempt,
                  std::string call_id, std::string tool)
        : events_(std::move(events)), attempt_(std::move(attempt)), call_id_(std::move(call_id)),
          tool_(std::move(tool)), started_(std::chrono::steady_clock::now()) {}
    ~ToolCallGuard() {
        if (!finished_) events_->push(RunEvent(RunEvent::ToolCallAborted{
            attempt_, call_id_, tool_, std::chrono::steady_clock::now() - started_}));
    }
    void finish(Result<json> result) {
        finished_ = true;
        events_->push(RunEvent(RunEvent::ToolCallFinished{
            attempt_, call_id_, tool_, std::move(result), std::chrono::steady_clock::now() - started_}));
    }
private:
    std::shared_ptr<detail::QueueState<RunEvent>> events_;
    StageAttempt attempt_;
    std::string call_id_;
    std::string tool_;
    std::chrono::steady_clock::time_point started_;
    bool finished_ = false;
};

class Execution {
public:
    Execution(std::shared_ptr<Workflow> workflow, std::shared_ptr<LlmProvider> llm, RunId run_id,
              RunInput input, std::shared_ptr<detail::QueueState<RunEvent>> events,
              std::shared_ptr<detail::QueueState<ApprovalResponse>> approvals, std::stop_token stop,
              std::chrono::steady_clock::time_point started)
        : workflow_(std::move(workflow)), llm_(std::move(llm)),
          state_{std::move(run_id), std::move(input), nullptr, {}, {}, 0, {}, started, stop},
          events_(std::move(events)), approvals_(std::move(approvals)) {}

    Stop run() {
        state_.state = workflow_->initial_state;
        return execute_loop();
    }

    std::shared_ptr<RunOutcome> finish(Stop stop) {
        auto result = finish_result();
        std::shared_ptr<RunOutcome> outcome;
        switch (stop.kind) {
            case StopKind::Completed: outcome = std::make_shared<RunOutcome>(RunOutcome::completed(std::move(result))); break;
            case StopKind::Failed: outcome = std::make_shared<RunOutcome>(RunOutcome::failed(std::move(result), *stop.error)); break;
            case StopKind::Cancelled: outcome = std::make_shared<RunOutcome>(RunOutcome::cancelled(std::move(result))); break;
            case StopKind::Limit: outcome = std::make_shared<RunOutcome>(RunOutcome::limit_exceeded(std::move(result), *stop.limit)); break;
        }
        events_->push(RunEvent(RunEvent::Finished{outcome}));
        return outcome;
    }

private:
    Stop execute_loop() {
        std::string current = workflow_->stages_.front().name_;
        for (;;) {
            if (state_.stop.stop_requested()) return Stop::cancelled();
            if (deadline_expired()) return Stop::limited(LimitKind::Deadline);
            const Stage * source = workflow_->stage(current);
            if (!source) return Stop::failed(Error::routing("stage '" + current + "' does not exist"));
            Stage stage = *source;
            auto & visits = state_.visits[current];
            if (visits != UINT32_MAX) ++visits;
            const std::uint32_t visit = visits;
            if (stage.max_visits_value && visit > *stage.max_visits_value) return Stop::limited(LimitKind::StageVisits);

            std::uint32_t attempt_number = 1;
            for (;;) {
                if (state_.stage_executions >= workflow_->limits.max_stage_executions)
                    return Stop::limited(LimitKind::StageExecutions);
                ++state_.stage_executions;
                StageAttempt attempt{stage.name_, visit, attempt_number};
                events_->push(RunEvent(RunEvent::StageStarted{attempt}));
                const Usage before = state_.usage;
                const auto started = std::chrono::steady_clock::now();
                ActionResult action = std::unexpected(ActionStop::failed(Error::internal("action did not execute")));
                try { action = execute_attempt(stage, attempt); }
                catch (...) { action = std::unexpected(ActionStop::failed(Error::panic(
                    "stage '" + stage.name_ + "' action panicked: " + exception_message()))); }
                StageRecord record{stage.name_, visit, attempt_number,
                    action ? std::optional<json>(*action) : std::nullopt, {}, state_.usage - before,
                    std::chrono::steady_clock::now() - started};

                if (action) {
                    state_.history.push_back(record);
                    auto routing = route_success(stage);
                    state_.history.pop_back();
                    std::optional<std::optional<std::string>> route;
                    std::optional<Error> routing_error;
                    if (routing) route = std::move(*routing);
                    else {
                        routing_error = routing.error();
                        record.failures.push_back({StageFailurePhase::Routing, *routing_error});
                    }
                    auto reduction_error = reduce_record(record);
                    publish_record(std::move(record));
                    if (reduction_error || routing_error) {
                        const Error error = reduction_error ? *reduction_error : *routing_error;
                        if (stage.error_target) {
                            events_->push(RunEvent(RunEvent::Routed{stage.name_, stage.error_target, RoutingReason::ErrorRecovery}));
                            current = *stage.error_target; break;
                        }
                        return Stop::failed(error);
                    }
                    if (!route) return Stop::failed(Error::internal("routing result missing without a routing failure"));
                    if (*route) {
                        const auto reason = stage.routing.kind == StageRoutingKind::Dynamic ? RoutingReason::Dynamic : RoutingReason::Success;
                        events_->push(RunEvent(RunEvent::Routed{stage.name_, **route, reason}));
                        current = **route; break;
                    }
                    events_->push(RunEvent(RunEvent::Routed{stage.name_, std::nullopt, RoutingReason::Success}));
                    return Stop::completed();
                }

                const ActionStop action_stop = action.error();
                if (action_stop.kind == ActionStopKind::Cancelled) {
                    record.failures.push_back({StageFailurePhase::Control, Error::cancelled("run cancelled during stage execution")});
                    publish_record(std::move(record)); return Stop::cancelled();
                }
                if (action_stop.kind == ActionStopKind::Limit) {
                    record.failures.push_back({StageFailurePhase::Control,
                        Error::limit("run limit reached: " + limit_name(*action_stop.limit))});
                    publish_record(std::move(record)); return Stop::limited(*action_stop.limit);
                }
                const Error error = *action_stop.error;
                record.failures.push_back({StageFailurePhase::Action, error});
                auto reduction_error = reduce_record(record);
                publish_record(std::move(record));
                if (reduction_error) {
                    if (stage.error_target) {
                        events_->push(RunEvent(RunEvent::Routed{stage.name_, stage.error_target, RoutingReason::ErrorRecovery}));
                        current = *stage.error_target; break;
                    }
                    return Stop::failed(*reduction_error);
                }
                if (should_retry(stage, error, attempt_number)) {
                    auto waited = wait_backoff(stage.retry_policy->backoff_for_retry(attempt_number));
                    if (!waited) {
                        if (waited.error().kind == ActionStopKind::Cancelled) return Stop::cancelled();
                        if (waited.error().kind == ActionStopKind::Limit) return Stop::limited(*waited.error().limit);
                        return Stop::failed(*waited.error().error);
                    }
                    ++attempt_number; continue;
                }
                if (stage.error_target) {
                    events_->push(RunEvent(RunEvent::Routed{stage.name_, stage.error_target, RoutingReason::ErrorRecovery}));
                    current = *stage.error_target; break;
                }
                return Stop::failed(error);
            }
        }
    }

    std::optional<Error> reduce_record(StageRecord & record) {
        if (!workflow_->reducer) return std::nullopt;
        try {
            auto reduced = (*workflow_->reducer)(state_.state, record);
            if (reduced) { state_.state = std::move(*reduced); return std::nullopt; }
            Error error = reduced.error().with_kind(ErrorKind::StateReduction);
            record.failures.push_back({StageFailurePhase::StateReduction, error}); return error;
        } catch (...) {
            Error error = Error::panic("state reducer panicked: " + exception_message());
            record.failures.push_back({StageFailurePhase::StateReduction, error}); return error;
        }
    }

    void publish_record(StageRecord record) {
        state_.history.push_back(record);
        events_->push(RunEvent(RunEvent::StageAttemptFinished{std::make_shared<StageRecord>(std::move(record))}));
    }

    bool should_retry(const Stage & stage, const Error & error, std::uint32_t attempt) const {
        if (!stage.retry_policy || attempt >= stage.retry_policy->max_attempts) return false;
        return stage.retry_policy->retry_on == RetryOn::AnyFailure || error.is_retryable();
    }

    std::expected<void, ActionStop> wait_backoff(Duration duration) const {
        bool deadline_limited = false;
        if (workflow_->limits.deadline) {
            const auto elapsed = std::chrono::steady_clock::now() - state_.started;
            if (elapsed >= *workflow_->limits.deadline) return std::unexpected(ActionStop::limited(LimitKind::Deadline));
            const auto remaining = *workflow_->limits.deadline - elapsed;
            if (remaining <= duration) { duration = remaining; deadline_limited = true; }
        }
        std::mutex mutex; std::condition_variable_any changed; std::unique_lock lock(mutex);
        if (changed.wait_for(lock, state_.stop, duration, [] { return false; })) {}
        if (state_.stop.stop_requested()) return std::unexpected(ActionStop::cancelled());
        if (deadline_limited) return std::unexpected(ActionStop::limited(LimitKind::Deadline));
        return {};
    }

    Result<std::optional<std::string>> route_success(const Stage & stage) const {
        Route route = Route::complete();
        if (stage.routing.kind == StageRoutingKind::Next) route = Route::next(stage.routing.next);
        else if (stage.routing.kind == StageRoutingKind::Dynamic) {
            try {
                auto selected = stage.routing.dynamic(state_.view());
                if (!selected) return std::unexpected(selected.error().with_kind(ErrorKind::Routing));
                route = std::move(*selected);
            } catch (...) {
                return std::unexpected(Error::panic("router for stage '" + stage.name_ + "' panicked: " + exception_message()));
            }
        }
        if (route.target() && !workflow_->stage(*route.target()))
            return std::unexpected(Error::routing("routing selected missing stage '" + *route.target() + "'"));
        return route.target();
    }

    ActionResult execute_attempt(const Stage & stage, const StageAttempt & attempt) {
        auto timeout = effective_timeout(stage.timeout_value);
        if (!timeout) return std::unexpected(timeout.error());
        action_deadline_.reset();
        action_stage_ = stage.name_;
        if (*timeout) {
            action_duration_ = (*timeout)->first;
            deadline_limited_ = (*timeout)->second;
            action_deadline_ = std::chrono::steady_clock::now() + action_duration_;
        }
        std::stop_source action_cancel;
        std::stop_callback forward_cancel(state_.stop, [&] { action_cancel.request_stop(); });
        action_stop_ = action_cancel.get_token();
        // Only timed attempts need a timer. It signals the same token used by
        // blocking actions, tools and providers, and is joined on scope exit.
        std::optional<std::jthread> timer;
        if (action_deadline_) {
            timer.emplace([until = *action_deadline_, action_cancel](std::stop_token done) mutable {
                std::mutex mutex;
                std::condition_variable_any changed;
                std::unique_lock lock(mutex);
                changed.wait_until(lock, done, until, [] { return false; });
                if (!done.stop_requested()) action_cancel.request_stop();
            });
        }
        try {
            check_control();
            auto result = execute_action(stage, attempt);
            check_control();
            return result;
        } catch (const ActionStop & stop) {
            return std::unexpected(stop);
        }
    }

    void check_control() const {
        if (state_.stop.stop_requested()) throw ActionStop::cancelled();
        if (action_deadline_ && std::chrono::steady_clock::now() >= *action_deadline_) {
            if (deadline_limited_) throw ActionStop::limited(LimitKind::Deadline);
            throw ActionStop::failed(Error::timeout(
                "stage '" + action_stage_ + "' timed out after " + duration_debug(action_duration_)));
        }
    }

    std::expected<std::optional<std::pair<Duration, bool>>, ActionStop>
    effective_timeout(std::optional<Duration> stage_timeout) const {
        std::optional<Duration> remaining;
        if (workflow_->limits.deadline) {
            const auto elapsed = std::chrono::steady_clock::now() - state_.started;
            remaining = elapsed >= *workflow_->limits.deadline ? Duration::zero() : *workflow_->limits.deadline - elapsed;
        }
        if (remaining == Duration::zero()) return std::unexpected(ActionStop::limited(LimitKind::Deadline));
        if (stage_timeout && remaining) return std::pair{std::min(*stage_timeout, *remaining), *remaining <= *stage_timeout};
        if (stage_timeout) return std::pair{*stage_timeout, false};
        if (remaining) return std::pair{*remaining, true};
        return std::nullopt;
    }

    bool deadline_expired() const {
        return workflow_->limits.deadline && std::chrono::steady_clock::now() - state_.started >= *workflow_->limits.deadline;
    }

    ActionResult execute_action(const Stage & stage, const StageAttempt & attempt) {
        switch (stage.action.kind) {
            case StageActionKind::RouteOnly: return json(nullptr);
            case StageActionKind::Deterministic: {
                auto value = stage.action.deterministic->execute(state_.view(action_stop_));
                if (!value) return std::unexpected(ActionStop::failed(value.error())); return std::move(*value);
            }
            case StageActionKind::Tool: return execute_direct_tool(attempt, *stage.action.tool);
            case StageActionKind::Llm: return execute_llm(attempt, *stage.action.llm);
        }
        return std::unexpected(ActionStop::failed(Error::internal("unknown stage action")));
    }

    ActionResult execute_direct_tool(const StageAttempt & attempt, const ToolAction & action) {
        auto arguments = action.build_arguments(state_.view(action_stop_));
        check_control();
        if (!arguments) return std::unexpected(ActionStop::failed(arguments.error()));
        const std::string call_id = detail::uuid_v4();
        auto approval = action.tool->handler->approval(tool_context(attempt, call_id), *arguments);
        check_control();
        if (!approval) return std::unexpected(ActionStop::failed(approval.error()));
        if (*approval) {
            auto approved = request_approval(attempt, call_id, action.tool, *arguments, std::move(**approval));
            if (!approved) return std::unexpected(approved.error());
            if (!*approved) {
                if (action.on_denied_behavior == DenialBehavior::Continue) return json{{"denied", true}};
                if (action.on_denied_behavior == DenialBehavior::CancelRun) return std::unexpected(ActionStop::cancelled());
                return std::unexpected(ActionStop::failed(Error::denied("tool call denied")));
            }
        }
        return call_tool(attempt, action.tool, call_id, std::move(*arguments));
    }

    ActionResult execute_llm(const StageAttempt & attempt, const LlmAction & action) {
        if (!llm_) return std::unexpected(ActionStop::failed(Error::configuration("LLM provider missing")));
        auto prompt = action.prompt.build(state_.view(action_stop_));
        check_control();
        if (!prompt) return std::unexpected(ActionStop::failed(prompt.error()));
        const std::string user_input = state_.input.input.is_string()
            ? state_.input.input.get<std::string>() : state_.input.input.dump();
        std::vector<Message> messages{Message::system(*prompt), Message::user(user_input)};
        std::uint32_t tool_rounds = 0, model_call = 0;
        for (;;) {
            if (auto consumed = consume_llm_call(); !consumed) return std::unexpected(consumed.error());
            ++model_call;
            for (const auto & hook : action.hooks) {
                auto result = hook->before_model(messages); if (!result) return std::unexpected(ActionStop::failed(result.error()));
                check_control();
            }
            ModelRequest request;
            request.messages = messages;
            for (const auto & tool : action.tools) request.tools.push_back(tool->spec);
            request.temperature = action.temperature; request.max_tokens = action.max_tokens;
            request.model = action.model; request.extra_body = action.extra_body; request.stop = action_stop_;
            if (action.output.kind == LlmOutputKind::Structured) request.response_schema = action.output.schema;
            auto opened = llm_->stream(request);
            check_control();
            if (!opened) return std::unexpected(ActionStop::failed(opened.error()));
            ModelResponse response;
            for (;;) {
                check_control();
                auto event = (*opened)->next();
                check_control();
                if (!event) break;
                if (!*event) return std::unexpected(ActionStop::failed(event->error()));
                auto & value = event->value().value;
                if (auto * text = std::get_if<std::string>(&value)) {
                    response.text += *text;
                    if (action.output.kind == LlmOutputKind::Text)
                        events_->push(RunEvent(RunEvent::TextDelta{attempt, model_call, *text}));
                } else if (auto * call = std::get_if<ToolCall>(&value)) merge_tool_call(response.tool_calls, *call);
                else if (auto * usage = std::get_if<TokenUsage>(&value)) response.usage = *usage;
                else response.stop_reason = std::get<ModelStopReason>(value);
            }
            state_.usage += Usage{0, 0, response.usage.input_tokens, response.usage.output_tokens};
            for (const auto & hook : action.hooks) {
                auto result = hook->after_model(response); if (!result) return std::unexpected(ActionStop::failed(result.error()));
                check_control();
            }
            if (response.stop_reason && response.stop_reason->kind == ModelStopKind::MaxTokens)
                return std::unexpected(ActionStop::failed(Error::invalid_input("LLM output was truncated at its token limit")));
            if (response.tool_calls.empty()) return interpret_llm_output(action, std::move(response.text));
            if (tool_rounds >= action.max_tool_rounds) return std::unexpected(ActionStop::limited(LimitKind::ToolRounds));
            ++tool_rounds;
            messages.push_back(Message::assistant(response.text, response.tool_calls));
            for (const auto & call : response.tool_calls) {
                ToolDecision decision;
                for (const auto & hook : action.hooks) {
                    auto hook_result = hook->before_tool(call);
                    check_control();
                    if (!hook_result) return std::unexpected(ActionStop::failed(hook_result.error()));
                    if (hook_result->kind != ToolDecisionKind::Continue) { decision = std::move(*hook_result); break; }
                }
                ToolResult tool_result;
                if (decision.kind == ToolDecisionKind::Reject)
                    tool_result = {json{{"error", "tool_call_rejected"}, {"reason", decision.reason}}, false};
                else if (decision.kind == ToolDecisionKind::Replace) tool_result = {std::move(decision.replacement), true};
                else {
                    auto executed = execute_llm_tool(attempt, action, call);
                    if (!executed) return std::unexpected(executed.error());
                    tool_result = std::move(*executed);
                }
                for (const auto & hook : action.hooks) {
                    auto result = hook->after_tool(call, tool_result); if (!result) return std::unexpected(ActionStop::failed(result.error()));
                    check_control();
                }
                messages.push_back(Message::tool(call.id, tool_result.value.dump()));
            }
        }
    }

    ActionResult interpret_llm_output(const LlmAction & action, std::string text) const {
        if (action.output.kind == LlmOutputKind::Text) return json(std::move(text));
        json value;
        try { value = json::parse(text); }
        catch (const json::exception & error) {
            return std::unexpected(ActionStop::failed(Error::invalid_input(
                "structured LLM output is invalid JSON: " + std::string(error.what()))));
        }
        auto validated = action.output.validate(value);
        if (!validated) return std::unexpected(ActionStop::failed(validated.error()));
        return value;
    }

    std::expected<ToolResult, ActionStop> execute_llm_tool(
        const StageAttempt & attempt, const LlmAction & action, const ToolCall & call) {
        auto found = std::find_if(action.tools.begin(), action.tools.end(),
            [&](const auto & tool) { return tool->spec.name == call.name; });
        if (found == action.tools.end()) return ToolResult{json{{"error", "tool_not_available"}, {"tool", call.name}}, false};
        auto tool = *found;
        auto approval = tool->handler->approval(tool_context(attempt, call.id), call.arguments);
        check_control();
        if (!approval) return std::unexpected(ActionStop::failed(approval.error()));
        if (*approval) {
            auto approved = request_approval(attempt, call.id, tool, call.arguments, std::move(**approval));
            if (!approved) return std::unexpected(approved.error());
            if (!*approved) {
                if (action.on_denied_behavior == DenialBehavior::Continue)
                    return ToolResult{json{{"error", "approval_denied"}, {"tool", call.name}}, false};
                if (action.on_denied_behavior == DenialBehavior::CancelRun) return std::unexpected(ActionStop::cancelled());
                return std::unexpected(ActionStop::failed(Error::denied("tool call denied")));
            }
        }
        auto result = call_tool(attempt, tool, call.id, call.arguments);
        if (result) return ToolResult{std::move(*result), true};
        if (result.error().kind == ActionStopKind::Error)
            return ToolResult{json{{"error", result.error().error->message()}}, false};
        return std::unexpected(result.error());
    }

    std::expected<bool, ActionStop> request_approval(
        const StageAttempt & attempt, const std::string & call_id,
        const std::shared_ptr<ToolDefinition> & tool, json arguments, ApprovalPrompt prompt) {
        const std::string request_id = detail::uuid_v4();
        check_control();
        if (!events_->push(RunEvent(ApprovalRequest{request_id, attempt, call_id, tool->spec.name,
                                                    std::move(arguments), std::move(prompt)})))
            return std::unexpected(ActionStop::failed(Error::configuration(
                "tool approval requires a retained event receiver")));
        for (;;) {
            check_control();
            if (!events_->has_receiver()) return std::unexpected(ActionStop::failed(Error::configuration(
                "tool approval event receiver was dropped")));
            if (auto response = approvals_->try_pop()) {
                if (response->request_id == request_id) return response->approved;
            }
            std::unique_lock lock(approvals_->mutex);
            approvals_->changed.wait_for(lock, action_stop_, std::chrono::milliseconds(10), [&] {
                return !approvals_->values.empty() || approvals_->closed;
            });
            if (approvals_->closed && approvals_->values.empty()) return std::unexpected(ActionStop::cancelled());
        }
    }

    ActionResult call_tool(const StageAttempt & attempt, const std::shared_ptr<ToolDefinition> & tool,
                           std::string call_id, json arguments) {
        if (auto consumed = consume_tool_call(); !consumed) return std::unexpected(consumed.error());
        events_->push(RunEvent(RunEvent::ToolCallStarted{attempt, call_id, tool->spec.name, arguments}));
        ToolCallGuard guard(events_, attempt, call_id, tool->spec.name);
        auto result = tool->handler->call(tool_context(attempt, call_id), std::move(arguments));
        check_control();
        guard.finish(result);
        if (!result) return std::unexpected(ActionStop::failed(result.error()));
        return std::move(*result);
    }

    ToolContext tool_context(const StageAttempt & attempt, const std::string & call_id) const {
        return {state_.run_id, workflow_->name_, attempt, call_id, state_.input.metadata, action_stop_};
    }

    std::expected<void, ActionStop> consume_llm_call() {
        check_control();
        if (state_.usage.llm_calls >= workflow_->limits.max_llm_calls)
            return std::unexpected(ActionStop::limited(LimitKind::LlmCalls));
        ++state_.usage.llm_calls; return {};
    }
    std::expected<void, ActionStop> consume_tool_call() {
        check_control();
        if (state_.usage.tool_calls >= workflow_->limits.max_tool_calls)
            return std::unexpected(ActionStop::limited(LimitKind::ToolCalls));
        ++state_.usage.tool_calls; return {};
    }

    RunResult finish_result() const {
        std::unordered_map<std::string, json> latest;
        for (const auto & record : state_.history)
            if (record.succeeded() && record.output) latest[record.stage] = *record.output;
        return {state_.run_id, workflow_->name_, std::move(latest), state_.history, state_.state,
                state_.usage, std::chrono::steady_clock::now() - state_.started};
    }

    static void merge_tool_call(std::vector<ToolCall> & calls, const ToolCall & incoming) {
        auto found = std::find_if(calls.begin(), calls.end(), [&](const ToolCall & value) {
            return !incoming.id.empty() && value.id == incoming.id;
        });
        if (found == calls.end()) calls.push_back(incoming); else *found = incoming;
    }

    std::shared_ptr<Workflow> workflow_;
    std::shared_ptr<LlmProvider> llm_;
    ExecutionState state_;
    std::shared_ptr<detail::QueueState<RunEvent>> events_;
    std::shared_ptr<detail::QueueState<ApprovalResponse>> approvals_;
    std::stop_token action_stop_;
    std::optional<std::chrono::steady_clock::time_point> action_deadline_;
    Duration action_duration_{};
    bool deadline_limited_ = false;
    std::string action_stage_;
};

} // namespace detail

using detail::Execution;
using detail::Stop;
using detail::exception_message;

RunHandle::RunHandle(RunId id, std::shared_ptr<detail::RunLifetime> lifetime,
                     std::shared_ptr<detail::QueueState<ApprovalResponse>> approvals,
                     std::shared_ptr<detail::ResultState<RunOutcome>> result,
                     std::optional<EventReceiver> events)
    : run_id_(std::move(id)), lifetime_(std::move(lifetime)), approvals_(std::move(approvals)),
      result_(std::move(result)), events_(std::move(events)) {}

RunHandle::RunHandle(const RunHandle & other)
    : run_id_(other.run_id_), lifetime_(other.lifetime_), approvals_(other.approvals_), result_(other.result_) {}
RunHandle & RunHandle::operator=(const RunHandle & other) {
    if (this != &other) {
        events_.reset(); run_id_ = other.run_id_; lifetime_ = other.lifetime_;
        approvals_ = other.approvals_; result_ = other.result_;
    }
    return *this;
}
void RunHandle::cancel() const { lifetime_->stop.request_stop(); }
Result<void> RunHandle::respond_to_approval(ApprovalResponse response) const {
    if (!approvals_->push(std::move(response)))
        return std::unexpected(Error::cancelled("run is no longer awaiting control messages"));
    return {};
}
std::optional<EventReceiver> RunHandle::take_events() {
    auto events = std::move(events_); events_.reset(); return events;
}
void RunHandle::discard_events() { events_.reset(); }
Result<std::shared_ptr<RunOutcome>> RunHandle::result() const {
    std::unique_lock lock(result_->mutex);
    result_->changed.wait(lock, [&] { return static_cast<bool>(result_->value); });
    return result_->value;
}

EngineBuilder Engine::builder() { return {}; }
Result<RunHandle> Engine::start(const std::string & name, RunInput input) const {
    return start_impl(name, std::move(input), true);
}

Result<RunHandle> Engine::start_impl(const std::string & name, RunInput input, bool retain_events) const try {
    auto found = inner_->workflows.find(name);
    if (found == inner_->workflows.end())
        return std::unexpected(Error::not_found("workflow '" + name + "' not found"));
    RunId run_id;
    auto lifetime = std::make_shared<detail::RunLifetime>();
    auto events = std::make_shared<detail::QueueState<RunEvent>>();
    if (!retain_events) events->drop_receiver();
    auto approvals = std::make_shared<detail::QueueState<ApprovalResponse>>();
    auto result = std::make_shared<detail::ResultState<RunOutcome>>();
    auto workflow = found->second;
    auto llm = inner_->llm;
    const auto stop = lifetime->stop.get_token();
    const auto started = std::chrono::steady_clock::now();
    lifetime->worker.emplace([workflow, llm, run_id, input = std::move(input), events, approvals, result, stop, started]() mutable {
        Execution execution(workflow, llm, run_id, std::move(input), events, approvals, stop, started);
        Stop stopped = Stop::failed(Error::internal("workflow did not execute"));
        try { stopped = execution.run(); }
        catch (...) { stopped = Stop::failed(Error::panic("workflow execution panicked: " + exception_message())); }
        auto outcome = execution.finish(std::move(stopped));
        {
            std::lock_guard lock(result->mutex); result->value = std::move(outcome);
        }
        approvals->close();
        events->close();
        result->changed.notify_all();
    });
    std::optional<EventReceiver> receiver;
    if (retain_events) receiver.emplace(events);
    return RunHandle(run_id, std::move(lifetime), approvals, result, std::move(receiver));
} catch (const std::system_error & error) {
    return std::unexpected(Error::unavailable("could not start workflow: " + std::string(error.what())));
}

Result<std::shared_ptr<RunOutcome>> Engine::run(const std::string & workflow, RunInput input) const {
    auto started = start_impl(workflow, std::move(input), false);
    if (!started) return std::unexpected(started.error());
    return started->result();
}

Result<EngineBuilder> EngineBuilder::workflow(Workflow workflow) {
    const std::string name = workflow.name();
    if (workflows_.contains(name)) return std::unexpected(Error::configuration("duplicate workflow '" + name + "'"));
    workflows_[name] = std::make_shared<Workflow>(std::move(workflow)); return *this;
}
Result<Engine> EngineBuilder::build() const {
    if (workflows_.empty()) return std::unexpected(Error::configuration("engine must contain at least one workflow"));
    bool needs_llm = false;
    for (const auto & [_, workflow] : workflows_)
        for (const auto & stage : workflow->stages()) needs_llm |= stage.action.kind == StageActionKind::Llm;
    if (needs_llm && !llm_) return std::unexpected(Error::configuration("an LLM provider is required by at least one workflow"));
    return Engine(std::make_shared<detail::EngineInner>(detail::EngineInner{workflows_, llm_}));
}

} // namespace jeeves
