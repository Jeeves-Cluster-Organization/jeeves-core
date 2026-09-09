#include <jeeves/workflow.hpp>

#include <algorithm>
#include <limits>
#include <sstream>
#include <unordered_set>

namespace jeeves {

Result<void> RunLimits::validate() const {
    if (max_stage_executions == 0)
        return std::unexpected(Error::configuration("max_stage_executions must be positive"));
    return {};
}

RetryPolicy RetryPolicy::exponential(std::uint32_t attempts, Duration backoff) {
    return {attempts, backoff};
}

Duration RetryPolicy::backoff_for_retry(std::uint32_t completed_attempts) const {
    if (initial_backoff == Duration::zero() || max_backoff == Duration::zero()) return Duration::zero();
    if (initial_backoff >= max_backoff) return max_backoff;
    const double initial = std::chrono::duration<double>(initial_backoff).count();
    const double maximum = std::chrono::duration<double>(max_backoff).count();
    const double seconds = initial * std::pow(backoff_multiplier, static_cast<double>(completed_attempts > 0 ? completed_attempts - 1 : 0));
    if (!std::isfinite(seconds) || seconds >= maximum) return max_backoff;
    return std::chrono::duration_cast<Duration>(std::chrono::duration<double>(seconds));
}

Result<void> RetryPolicy::validate() const {
    if (max_attempts == 0)
        return std::unexpected(Error::configuration("retry max_attempts must be positive"));
    if (!std::isfinite(backoff_multiplier) || backoff_multiplier < 1.0)
        return std::unexpected(Error::configuration("retry backoff multiplier must be finite and at least 1"));
    return {};
}

ToolAction::ToolAction(std::shared_ptr<ToolDefinition> value) : tool(std::move(value)) {
    build_arguments = [](const RunView & run) -> Result<json> { return run.input; };
}

Stage Stage::deterministic(std::string name, std::shared_ptr<DeterministicAction> action) {
    StageAction value; value.kind = StageActionKind::Deterministic; value.deterministic = std::move(action);
    return Stage(std::move(name), std::move(value));
}
Stage Stage::llm(std::string name, LlmAction action) {
    StageAction value; value.kind = StageActionKind::Llm; value.llm = std::move(action);
    return Stage(std::move(name), std::move(value));
}
Stage Stage::tool(std::string name, ToolAction action) {
    StageAction value; value.kind = StageActionKind::Tool; value.tool = std::move(action);
    return Stage(std::move(name), std::move(value));
}
Stage Stage::route_only(std::string name) { return Stage(std::move(name), StageAction{}); }

WorkflowBuilder Workflow::builder(std::string name) { return WorkflowBuilder(std::move(name)); }
const Stage * Workflow::stage(const std::string & wanted) const noexcept {
    auto found = std::find_if(stages_.begin(), stages_.end(), [&](const Stage & value) { return value.name_ == wanted; });
    return found == stages_.end() ? nullptr : &*found;
}

Result<Workflow> WorkflowBuilder::build() const {
    if (name_.empty()) return std::unexpected(Error::configuration("workflow name cannot be empty"));
    if (stages_.empty()) return std::unexpected(Error::configuration("workflow must contain at least one stage"));
    std::unordered_set<std::string> names;
    for (const auto & stage : stages_) {
        if (stage.name_.empty()) return std::unexpected(Error::configuration("stage name cannot be empty"));
        if (!names.insert(stage.name_).second)
            return std::unexpected(Error::configuration("duplicate stage '" + stage.name_ + "'"));
        if (stage.max_visits_value == 0)
            return std::unexpected(Error::configuration("stage '" + stage.name_ + "' max_visits must be positive"));
        if (stage.timeout_value == Duration::zero())
            return std::unexpected(Error::configuration("stage '" + stage.name_ + "' timeout must be positive"));
        if (stage.action.kind == StageActionKind::Deterministic && !stage.action.deterministic)
            return std::unexpected(Error::configuration("deterministic action cannot be null"));
        if (stage.action.kind == StageActionKind::Tool && (!stage.action.tool->tool || !stage.action.tool->tool->handler))
            return std::unexpected(Error::configuration("tool definition and handler cannot be null"));
        if (stage.action.kind == StageActionKind::Llm) {
            for (const auto & tool : stage.action.llm->tools)
                if (!tool || !tool->handler)
                    return std::unexpected(Error::configuration("tool definition and handler cannot be null"));
        }
        if (stage.retry_policy) {
            if (auto valid = stage.retry_policy->validate(); !valid) return std::unexpected(valid.error());
            if (!stage.retry_policy->allow_side_effect_replay_value) {
                std::vector<std::string> unsafe;
                if (stage.action.kind == StageActionKind::Tool && stage.action.tool->tool->replay_safety == ReplaySafety::Unsafe)
                    unsafe.push_back(stage.action.tool->tool->spec.name);
                if (stage.action.kind == StageActionKind::Llm) {
                    for (const auto & tool : stage.action.llm->tools)
                        if (tool->replay_safety == ReplaySafety::Unsafe) unsafe.push_back(tool->spec.name);
                }
                if (!unsafe.empty()) {
                    std::ostringstream joined;
                    for (std::size_t i = 0; i < unsafe.size(); ++i) { if (i) joined << ", "; joined << unsafe[i]; }
                    return std::unexpected(Error::configuration("stage '" + stage.name_ + "' retries replay-unsafe tools: " + joined.str()));
                }
            }
        }
        if (stage.action.kind == StageActionKind::Llm) {
            if (auto valid = stage.action.llm->validate(); !valid) return std::unexpected(valid.error());
        }
    }
    for (const auto & stage : stages_) {
        std::vector<std::string> targets;
        if (stage.routing.kind == StageRoutingKind::Next) targets.push_back(stage.routing.next);
        if (stage.error_target) targets.push_back(*stage.error_target);
        for (const auto & target : targets) {
            if (!names.contains(target))
                return std::unexpected(Error::configuration("stage '" + stage.name_ + "' targets missing stage '" + target + "'"));
        }
    }
    if (auto valid = limits_.validate(); !valid) return std::unexpected(valid.error());
    return Workflow{name_, stages_, limits_, initial_state_, reducer_};
}

} // namespace jeeves
