#pragma once

#include <jeeves/llm.hpp>

#include <cmath>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <variant>
#include <vector>

namespace jeeves {
namespace detail { class Execution; }

struct RunLimits {
    std::uint32_t max_stage_executions = 100;
    std::uint32_t max_llm_calls = 100;
    std::uint32_t max_tool_calls = 100;
    std::optional<Duration> deadline;
    Result<void> validate() const;
};

enum class RetryOn { Retryable, AnyFailure };
struct RetryPolicy {
    std::uint32_t max_attempts;
    Duration initial_backoff;
    double backoff_multiplier = 2.0;
    Duration max_backoff = std::chrono::seconds(30);
    RetryOn retry_on = RetryOn::Retryable;
    bool allow_side_effect_replay_value = false;

    static RetryPolicy exponential(std::uint32_t max_attempts, Duration initial_backoff);
    RetryPolicy & retry_on_any_failure() { retry_on = RetryOn::AnyFailure; return *this; }
    RetryPolicy & allow_side_effect_replay() { allow_side_effect_replay_value = true; return *this; }
    RetryPolicy & with_max_backoff(Duration value) { max_backoff = value; return *this; }
    RetryPolicy & with_multiplier(double value) { backoff_multiplier = value; return *this; }
    [[nodiscard]] Duration backoff_for_retry(std::uint32_t completed_attempts) const;
    Result<void> validate() const;
};

class DeterministicAction {
public:
    virtual ~DeterministicAction() = default;
    virtual Result<json> execute(const RunView & run) = 0;
};

template <class F>
class DeterministicFn final : public DeterministicAction {
public:
    explicit DeterministicFn(F function) : function_(std::move(function)) {}
    Result<json> execute(const RunView & run) override { return function_(run); }
private:
    F function_;
};

class Route {
public:
    static Route next(std::string stage) { return Route(std::move(stage)); }
    static Route complete() { return Route(std::nullopt); }
    [[nodiscard]] const std::optional<std::string> & target() const noexcept { return target_; }
private:
    explicit Route(std::optional<std::string> target) : target_(std::move(target)) {}
    std::optional<std::string> target_;
};

class Router {
public:
    virtual ~Router() = default;
    virtual Result<Route> route(const RunView & run) = 0;
};
class StateReducer {
public:
    virtual ~StateReducer() = default;
    virtual Result<json> reduce(const json & state, const StageRecord & record) = 0;
};
using RouterFn = std::function<Result<Route>(const RunView &)>;
using StateReducerFn = std::function<Result<json>(const json &, const StageRecord &)>;
using ToolArguments = std::function<Result<json>(const RunView &)>;

struct ToolAction {
    std::shared_ptr<ToolDefinition> tool;
    DenialBehavior on_denied_behavior = DenialBehavior::FailStage;
    ToolArguments build_arguments;
    explicit ToolAction(std::shared_ptr<ToolDefinition> value);
    ToolAction & on_denied(DenialBehavior value) { on_denied_behavior = value; return *this; }
    ToolAction & arguments(ToolArguments value) { build_arguments = std::move(value); return *this; }
};

enum class StageActionKind { Deterministic, Llm, Tool, RouteOnly };
struct StageAction {
    StageActionKind kind = StageActionKind::RouteOnly;
    std::shared_ptr<DeterministicAction> deterministic;
    std::optional<LlmAction> llm;
    std::optional<ToolAction> tool;
};

enum class StageRoutingKind { Complete, Next, Dynamic };
struct StageRouting {
    StageRoutingKind kind = StageRoutingKind::Complete;
    std::string next;
    RouterFn dynamic;
};

class Stage {
public:
    [[nodiscard]] const std::string & name() const noexcept { return name_; }

private:
    friend class Workflow;
    friend class WorkflowBuilder;
    friend class EngineBuilder;
    friend class detail::Execution;
    std::string name_;
    StageAction action;
    StageRouting routing;
    std::optional<std::string> error_target;
    std::optional<RetryPolicy> retry_policy;
    std::optional<Duration> timeout_value;
    std::optional<std::uint32_t> max_visits_value;

public:
    static Stage deterministic(std::string name, std::shared_ptr<DeterministicAction> action);
    template <class F>
    static Stage deterministic_fn(std::string name, F function) {
        return deterministic(std::move(name), std::make_shared<DeterministicFn<F>>(std::move(function)));
    }
    static Stage llm(std::string name, LlmAction action);
    static Stage tool(std::string name, ToolAction action);
    static Stage route_only(std::string name);
    Stage & next(std::string target) { routing = {StageRoutingKind::Next, std::move(target), {}}; return *this; }
    Stage & route(RouterFn value) { routing = {StageRoutingKind::Dynamic, {}, std::move(value)}; return *this; }
    Stage & route(std::shared_ptr<Router> value) {
        return route([value = std::move(value)](const RunView & run) { return value->route(run); });
    }
    Stage & on_error(std::string target) { error_target = std::move(target); return *this; }
    Stage & retry(RetryPolicy value) { retry_policy = std::move(value); return *this; }
    Stage & timeout(Duration value) { timeout_value = value; return *this; }
    Stage & max_visits(std::uint32_t value) { max_visits_value = value; return *this; }

private:
    Stage(std::string name, StageAction action_) : name_(std::move(name)), action(std::move(action_)) {}
};

class WorkflowBuilder;
class Workflow {
public:
    static WorkflowBuilder builder(std::string name);
    [[nodiscard]] const std::string & name() const noexcept { return name_; }
    [[nodiscard]] const std::vector<Stage> & stages() const noexcept { return stages_; }
    [[nodiscard]] const Stage * stage(const std::string & name) const noexcept;

private:
    friend class WorkflowBuilder;
    friend class detail::Execution;
    Workflow(std::string name, std::vector<Stage> stages, RunLimits bounds, json initial,
             std::optional<StateReducerFn> reduce)
        : name_(std::move(name)), stages_(std::move(stages)), limits(bounds),
          initial_state(std::move(initial)), reducer(std::move(reduce)) {}
    std::string name_;
    std::vector<Stage> stages_;
    RunLimits limits;
    json initial_state;
    std::optional<StateReducerFn> reducer;
};

class WorkflowBuilder {
public:
    explicit WorkflowBuilder(std::string name) : name_(std::move(name)) {}
    WorkflowBuilder & stage(Stage value) { stages_.push_back(std::move(value)); return *this; }
    WorkflowBuilder & limits(RunLimits value) { limits_ = value; return *this; }
    WorkflowBuilder & state(json initial, StateReducerFn reducer) {
        initial_state_ = std::move(initial); reducer_ = std::move(reducer); return *this;
    }
    WorkflowBuilder & state(json initial, std::shared_ptr<StateReducer> reducer) {
        return state(std::move(initial), [reducer = std::move(reducer)](const json & value, const StageRecord & record) {
            return reducer->reduce(value, record);
        });
    }
    Result<Workflow> build() const;

private:
    std::string name_;
    std::vector<Stage> stages_;
    RunLimits limits_;
    json initial_state_ = nullptr;
    std::optional<StateReducerFn> reducer_;
};

} // namespace jeeves
