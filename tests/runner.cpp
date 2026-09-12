#include <jeeves/jeeves.hpp>
#include <gtest/gtest.h>
#include "../cpp/llm/tool_calls.hpp"

#include <atomic>
#include <condition_variable>
#include <future>
#include <limits>
#include <stdexcept>

using namespace jeeves;
using namespace std::chrono_literals;

namespace {
template <class T> T must(Result<T> result) {
    if (!result) throw std::runtime_error(result.error().message());
    return std::move(*result);
}
Engine engine(Workflow workflow, std::shared_ptr<LlmProvider> llm = {}) {
    return must(must(Engine::builder().llm(std::move(llm)).workflow(std::move(workflow))).build());
}
const RunResult & completed(const std::shared_ptr<RunOutcome> & outcome) {
    EXPECT_TRUE(outcome->completed());
    if (!outcome->completed()) throw std::runtime_error(outcome->error() ? outcome->error()->message() : "run did not complete");
    return outcome->result();
}
std::shared_ptr<MockLlmProvider> mock(std::vector<std::vector<ModelStreamEvent>> responses) {
    return std::make_shared<MockLlmProvider>(std::move(responses));
}
class EchoTool : public Tool {
public:
    std::atomic_uint calls{0};
    Result<json> call(const ToolContext &, json arguments) override { ++calls; return arguments; }
};
class ApprovalTool final : public EchoTool {
public:
    Result<std::optional<ApprovalPrompt>> approval(const ToolContext &, const json &) override {
        return ApprovalPrompt("approve test tool").with_data(json{{"why", "test"}});
    }
};
std::shared_ptr<ToolDefinition> definition(std::shared_ptr<Tool> handler, std::string name = "echo") {
    return std::make_shared<ToolDefinition>(must(ToolSpec::create(std::move(name), "test", json{{"type", "object"}})), handler);
}
ApprovalRequest approval(EventReceiver & events) {
    while (auto event = events.recv()) {
        if (auto * request = std::get_if<ApprovalRequest>(&event->value)) return *request;
    }
    throw std::runtime_error("no approval request");
}
class PendingAction final : public DeterministicAction {
public:
    std::promise<void> entered;
    Result<json> execute(const RunView & run) override {
        entered.set_value();
        std::mutex mutex; std::condition_variable_any changed; std::unique_lock lock(mutex);
        changed.wait(lock, run.stop, [] { return false; });
        return json(nullptr);
    }
};
}

TEST(Runner, deterministic_pipeline_routes_and_keeps_stage_history) {
    auto workflow = must(Workflow::builder("linear")
        .stage(Stage::deterministic_fn("first", [](const RunView &) -> Result<json> { return json{{"value", 4}}; }).next("second"))
        .stage(Stage::deterministic_fn("second", [](const RunView & run) -> Result<json> {
            return json{{"value", run.latest_output("first")->at("value").get<int>() * 2}};
        })).build());
    auto outcome = must(engine(workflow).run("linear", "hello"));
    const auto & result = completed(outcome);
    EXPECT_EQ(result.latest_outputs.at("second"), (json{{"value", 8}}));
    ASSERT_EQ(result.history.size(), 2);
    EXPECT_EQ(result.history[0].stage, "first");
    EXPECT_EQ(result.history[1].stage, "second");
}

TEST(Runner, exact_stage_and_llm_limits_allow_normal_completion) {
    auto workflow = must(Workflow::builder("exact").limits(RunLimits{1, 1, 1, {}})
        .stage(Stage::llm("only", LlmAction::text(Prompt::text("respond")))).build());
    auto outcome = must(engine(workflow, MockLlmProvider::text("finished")).run("exact", "input"));
    EXPECT_EQ(completed(outcome).latest_outputs.at("only"), "finished");
    EXPECT_EQ(outcome->result().usage.llm_calls, 1);
}

TEST(Runner, retry_any_failure_is_explicit_and_preserves_failed_attempts) {
    std::atomic_uint calls{0};
    auto workflow = must(Workflow::builder("retry").stage(Stage::deterministic_fn("flaky", [&](const RunView &) -> Result<json> {
        if (calls++ == 0) return std::unexpected(Error::invalid_input("bad first result"));
        return json{{"ok", true}};
    }).retry(RetryPolicy::exponential(2, 0ms).retry_on_any_failure())).build());
    auto outcome = must(engine(workflow).run("retry", "input"));
    const auto & result = completed(outcome);
    ASSERT_EQ(result.history.size(), 2);
    EXPECT_FALSE(result.history[0].succeeded());
    EXPECT_TRUE(result.history[1].succeeded());
    EXPECT_EQ(result.history[1].attempt, 2);
    EXPECT_EQ(result.history[1].visit, 1);
    EXPECT_EQ(calls, 2);
}

TEST(Runner, exhausted_failure_routes_to_recovery_and_can_complete) {
    auto workflow = must(Workflow::builder("recover")
        .stage(Stage::deterministic_fn("work", [](const RunView &) -> Result<json> {
            return std::unexpected(Error::permanent("failed"));
        }).on_error("recover"))
        .stage(Stage::deterministic_fn("recover", [](const RunView &) -> Result<json> { return json{{"recovered", true}}; })).build());
    auto outcome = must(engine(workflow).run("recover", "input"));
    EXPECT_EQ(completed(outcome).latest_outputs.at("recover"), (json{{"recovered", true}}));
    EXPECT_EQ(outcome->result().history.size(), 2);
    EXPECT_FALSE(outcome->result().history.front().succeeded());
}

TEST(Runner, unrecovered_failure_is_not_completed) {
    auto workflow = must(Workflow::builder("fail").stage(Stage::deterministic_fn("work", [](const RunView &) -> Result<json> {
        return std::unexpected(Error::permanent("failed"));
    })).build());
    auto outcome = must(engine(workflow).run("fail", "input"));
    EXPECT_EQ(outcome->kind(), OutcomeKind::Failed);
    EXPECT_EQ(outcome->error(), Error::permanent("failed"));
}

TEST(Runner, reducer_derives_shared_state_from_attempt_history) {
    auto workflow = must(Workflow::builder("state").state(json{{"successes", 0}}, [](const json & state, const StageRecord & record) -> Result<json> {
        auto next = state;
        if (record.succeeded()) next["successes"] = next["successes"].get<int>() + 1;
        return next;
    }).stage(Stage::deterministic_fn("one", [](const RunView &) -> Result<json> { return json(1); }).next("two"))
      .stage(Stage::deterministic_fn("two", [](const RunView &) -> Result<json> { return json(2); })).build());
    auto outcome = must(engine(workflow).run("state", "input"));
    EXPECT_EQ(completed(outcome).state, (json{{"successes", 2}}));
}

TEST(Runner, dynamic_routing_reads_latest_stage_output) {
    auto workflow = must(Workflow::builder("route")
        .stage(Stage::deterministic_fn("choose", [](const RunView &) -> Result<json> { return json{{"target", "right"}}; })
            .route([](const RunView & run) -> Result<Route> { return Route::next(run.latest_output("choose")->at("target")); }))
        .stage(Stage::route_only("left")).stage(Stage::route_only("right")).build());
    auto outcome = must(engine(workflow).run("route", "input"));
    EXPECT_TRUE(completed(outcome).latest_outputs.contains("right"));
    EXPECT_FALSE(outcome->result().latest_outputs.contains("left"));
}

TEST(Runner, text_output_streams_deltas_and_finishes_with_same_result) {
    auto workflow = must(Workflow::builder("stream").stage(Stage::llm("speak", LlmAction::text(Prompt::text("speak")))).build());
    auto handle = must(engine(workflow, mock({{"hello ", "world"}})).start("stream", "input"));
    auto events = handle.take_events(); ASSERT_TRUE(events);
    auto clone = handle;
    EXPECT_FALSE(clone.take_events()); EXPECT_FALSE(handle.take_events());
    auto outcome = must(handle.result());
    EXPECT_EQ(completed(outcome).latest_outputs.at("speak"), "hello world");
    std::string deltas;
    std::shared_ptr<RunOutcome> finished;
    while (auto event = events->recv()) {
        if (auto * text = std::get_if<RunEvent::TextDelta>(&event->value)) {
            EXPECT_EQ(text->model_call, 1); deltas += text->content;
        }
        if (auto * value = std::get_if<RunEvent::Finished>(&event->value)) finished = value->outcome;
    }
    EXPECT_EQ(deltas, "hello world");
    EXPECT_EQ(outcome.get(), finished.get()); EXPECT_EQ(must(clone.result()).get(), outcome.get());
}

TEST(Runner, structured_output_is_collected_validated_and_retried) {
    auto llm = mock({{"not-json"}, {"{\"answer\":\"bad\"}"}, {"{\"answer\":", "42}"}});
    auto action = LlmAction::structured(Prompt::text("answer"), json{{"type", "object"}}, [](const json & value) -> Result<void> {
        if (!value.contains("answer") || !value["answer"].is_number_integer())
            return std::unexpected(Error::invalid_input("answer must be an integer"));
        return {};
    });
    auto workflow = must(Workflow::builder("structured").stage(Stage::llm("answer", action)
        .retry(RetryPolicy::exponential(3, 0ms).retry_on_any_failure())).build());
    auto handle = must(engine(workflow, llm).start("structured", "input"));
    auto events = handle.take_events();
    auto outcome = must(handle.result());
    EXPECT_EQ(completed(outcome).latest_outputs.at("answer"), (json{{"answer", 42}}));
    EXPECT_EQ(outcome->result().history.size(), 3);
    EXPECT_EQ(outcome->result().usage.llm_calls, 3);
    while (auto event = events->recv()) EXPECT_FALSE(std::holds_alternative<RunEvent::TextDelta>(event->value));
}

TEST(Runner, token_limit_stop_rejects_valid_but_truncated_structured_output) {
    auto llm = mock({{"{\"answer\":41}", ModelStopReason::max_tokens("length")},
                     {"{\"answer\":42}", ModelStopReason::completed("stop")}});
    auto action = LlmAction::structured(Prompt::text("answer"), json{{"type", "object"}}, [](const json &) -> Result<void> { return {}; });
    auto workflow = must(Workflow::builder("truncation").stage(Stage::llm("answer", action)
        .retry(RetryPolicy::exponential(2, 0ms).retry_on_any_failure())).build());
    auto outcome = must(engine(workflow, llm).run("truncation", "input"));
    EXPECT_EQ(completed(outcome).latest_outputs.at("answer"), (json{{"answer", 42}}));
    EXPECT_EQ(outcome->result().history.size(), 2);
    EXPECT_EQ(outcome->result().history[0].failures[0].error.message(), "LLM output was truncated at its token limit");
}

TEST(Runner, direct_tool_arguments_can_be_derived_from_run_history) {
    auto tool = std::make_shared<EchoTool>();
    auto workflow = must(Workflow::builder("direct-tool").limits(RunLimits{2, 0, 1, {}})
        .stage(Stage::deterministic_fn("prepare", [](const RunView &) -> Result<json> { return json{{"x", 7}}; }).next("echo"))
        .stage(Stage::tool("echo", ToolAction(definition(tool)).arguments([](const RunView & run) -> Result<json> {
            return *run.latest_output("prepare");
        }))).build());
    auto outcome = must(engine(workflow).run("direct-tool", "ignored"));
    EXPECT_EQ(completed(outcome).latest_outputs.at("echo"), (json{{"x", 7}})); EXPECT_EQ(tool->calls, 1);
}

TEST(Runner, zero_tool_budget_prevents_tool_execution) {
    auto tool = std::make_shared<EchoTool>();
    auto workflow = must(Workflow::builder("bounded-tool").limits(RunLimits{1, 0, 0, {}})
        .stage(Stage::tool("echo", ToolAction(definition(tool)))).build());
    auto outcome = must(engine(workflow).run("bounded-tool", json{{"x", 1}}));
    EXPECT_EQ(outcome->kind(), OutcomeKind::LimitExceeded); EXPECT_EQ(outcome->limit(), LimitKind::ToolCalls);
    EXPECT_EQ(tool->calls, 0);
}

TEST(Runner, approval_pauses_in_place_and_resumes_the_same_llm_loop) {
    auto tool = std::make_shared<ApprovalTool>();
    auto llm = mock({{ToolCall{"call-1", "echo", json{{"x", 1}}}}, {"done"}});
    auto workflow = must(Workflow::builder("approval").stage(Stage::llm("act",
        LlmAction::text(Prompt::text("act")).with_tools({definition(tool)}))).build());
    auto handle = must(engine(workflow, llm).start("approval", "input"));
    auto events = handle.take_events();
    auto request = approval(*events);
    EXPECT_EQ(tool->calls, 0); EXPECT_EQ(request.call_id, "call-1");
    EXPECT_EQ(request.arguments, (json{{"x", 1}})); EXPECT_EQ(request.prompt.data, (json{{"why", "test"}}));
    ASSERT_TRUE(handle.respond_to_approval(ApprovalResponse::approve("unrelated-id")));
    ASSERT_TRUE(handle.respond_to_approval(ApprovalResponse::approve(request.request_id)));
    auto outcome = must(handle.result());
    EXPECT_EQ(completed(outcome).latest_outputs.at("act"), "done");
    EXPECT_EQ(tool->calls, 1); EXPECT_EQ(outcome->result().usage.llm_calls, 2);
}

TEST(Runner, denied_llm_tool_returns_to_model_without_executing) {
    auto tool = std::make_shared<ApprovalTool>();
    auto llm = mock({{ToolCall{"call-1", "echo", json{{"x", 1}}}}, {"alternative"}});
    auto workflow = must(Workflow::builder("denied").stage(Stage::llm("act",
        LlmAction::text(Prompt::text("act")).with_tools({definition(tool)}))).build());
    auto handle = must(engine(workflow, llm).start("denied", "input"));
    auto events = handle.take_events();
    auto request = approval(*events);
    ASSERT_TRUE(handle.respond_to_approval(ApprovalResponse::deny(request.request_id)));
    auto outcome = must(handle.result());
    EXPECT_EQ(completed(outcome).latest_outputs.at("act"), "alternative"); EXPECT_EQ(tool->calls, 0);
}

TEST(Runner, approval_without_an_event_consumer_fails_instead_of_waiting_forever) {
    auto tool = std::make_shared<ApprovalTool>();
    auto workflow = must(Workflow::builder("unobserved-approval").stage(Stage::tool("act", ToolAction(definition(tool)))).build());
    auto outcome = must(engine(workflow).run("unobserved-approval", json{{"x", 1}}));
    EXPECT_EQ(outcome->kind(), OutcomeKind::Failed); EXPECT_EQ(outcome->error()->kind(), ErrorKind::Configuration);
    EXPECT_EQ(tool->calls, 0);
}

TEST(Runner, explicit_cancellation_finishes_as_cancelled) {
    auto action = std::make_shared<PendingAction>();
    auto entered = action->entered.get_future();
    auto workflow = must(Workflow::builder("cancel").stage(Stage::deterministic("wait", action)).build());
    auto handle = must(engine(workflow).start("cancel", "input"));
    ASSERT_EQ(entered.wait_for(2s), std::future_status::ready);
    handle.cancel();
    auto outcome = must(handle.result());
    EXPECT_EQ(outcome->kind(), OutcomeKind::Cancelled);
    ASSERT_EQ(outcome->result().history.size(), 1);
    EXPECT_EQ(outcome->result().history[0].failures[0].phase, StageFailurePhase::Control);
}

TEST(Examples, double_pipeline) {
    auto workflow = must(Workflow::builder("double")
        .stage(Stage::deterministic_fn("read", [](const RunView & run) -> Result<json> { return run.input.at("value"); }).next("double"))
        .stage(Stage::deterministic_fn("double", [](const RunView & run) -> Result<json> { return json(run.latest_output("read")->get<int>() * 2); })).build());
    auto outcome = must(engine(workflow).run("double", json{{"value", 21}}));
    EXPECT_EQ(completed(outcome).latest_outputs.at("double"), 42);
}

TEST(Examples, mock_greet_stream) {
    auto workflow = must(Workflow::builder("greet").stage(Stage::llm("speak", LlmAction::text(Prompt::text("Greet warmly.")))).build());
    auto outcome = must(engine(workflow, mock({{"Hello", ", world!"}})).run("greet", "input"));
    EXPECT_EQ(completed(outcome).latest_outputs.at("speak"), "Hello, world!");
}

TEST(Parity, default_retry_only_retries_transient_timeout_and_unavailable) {
    for (auto kind : {ErrorKind::Transient, ErrorKind::Timeout, ErrorKind::Unavailable, ErrorKind::InvalidInput, ErrorKind::Panic}) {
        std::atomic_uint calls{0};
        auto workflow = must(Workflow::builder("w").stage(Stage::deterministic_fn("s", [&](const RunView &) -> Result<json> {
            if (calls++ == 0) return std::unexpected(Error(kind, "first"));
            return json("ok");
        }).retry(RetryPolicy::exponential(2, 0ms))).build());
        auto outcome = must(engine(workflow).run("w", "input"));
        const bool retryable = Error(kind, "").is_retryable();
        EXPECT_EQ(outcome->completed(), retryable);
        EXPECT_EQ(calls, retryable ? 2 : 1);
    }
    auto policy = RetryPolicy::exponential(10, 10ms).with_max_backoff(25ms);
    EXPECT_EQ(policy.backoff_for_retry(1), 10ms);
    EXPECT_EQ(policy.backoff_for_retry(2), 20ms);
    EXPECT_EQ(policy.backoff_for_retry(3), 25ms);
    EXPECT_EQ(policy.backoff_for_retry(UINT32_MAX), 25ms);
}

TEST(Parity, stage_execution_visit_and_llm_limits_are_distinct) {
    auto bounded = must(Workflow::builder("w").limits(RunLimits{2, 0, 0, {}})
        .stage(Stage::route_only("s").next("s")).build());
    auto outcome = must(engine(bounded).run("w", "input"));
    EXPECT_EQ(outcome->limit(), LimitKind::StageExecutions);
    ASSERT_EQ(outcome->result().history.size(), 2);
    EXPECT_EQ(outcome->result().history[1].visit, 2);
    EXPECT_EQ(outcome->result().history[1].attempt, 1);
    auto visits = must(Workflow::builder("w").stage(Stage::route_only("s").next("s").max_visits(1)).build());
    outcome = must(engine(visits).run("w", "input"));
    EXPECT_EQ(outcome->limit(), LimitKind::StageVisits); EXPECT_EQ(outcome->result().history.size(), 1);
    auto llm = MockLlmProvider::text("unspent");
    auto zero = must(Workflow::builder("w").limits(RunLimits{1, 0, 0, {}})
        .stage(Stage::llm("s", LlmAction::text(Prompt::text("p")))).build());
    outcome = must(engine(zero, llm).run("w", "input"));
    EXPECT_EQ(outcome->limit(), LimitKind::LlmCalls); EXPECT_EQ(outcome->result().usage.llm_calls, 0);
    EXPECT_TRUE(llm->stream(ModelRequest{}));
}

TEST(Parity, routing_precedes_reduction_and_failed_reduction_is_transactional) {
    auto workflow = must(Workflow::builder("w").state(json(7), [](const json & state, const StageRecord & record) -> Result<json> {
        EXPECT_EQ(state, 7);
        if (record.stage == "choose") return std::unexpected(Error::permanent("reducer failed"));
        return json(9);
    }).stage(Stage::deterministic_fn("choose", [](const RunView &) -> Result<json> { return json(42); })
        .route([](const RunView & run) -> Result<Route> {
            EXPECT_EQ(run.state, 7); EXPECT_EQ(*run.latest_output("choose"), 42);
            return std::unexpected(Error::permanent("router failed"));
        }).on_error("recover"))
        .stage(Stage::deterministic_fn("recover", [](const RunView & run) -> Result<json> {
            EXPECT_EQ(run.state, 7); EXPECT_EQ(run.latest_output("choose"), nullptr);
            return json("recovered");
        })).build());
    auto outcome = must(engine(workflow).run("w", "input"));
    const auto & result = completed(outcome);
    EXPECT_EQ(result.state, 9);
    ASSERT_EQ(result.history[0].failures.size(), 2);
    EXPECT_EQ(result.history[0].failures[0].phase, StageFailurePhase::Routing);
    EXPECT_EQ(result.history[0].failures[0].error.kind(), ErrorKind::Routing);
    EXPECT_EQ(result.history[0].failures[1].phase, StageFailurePhase::StateReduction);
    EXPECT_EQ(result.history[0].failures[1].error.kind(), ErrorKind::StateReduction);
}

TEST(Parity, reduction_failure_after_action_failure_prevents_retry_and_retains_both_causes) {
    std::atomic_uint calls{0};
    auto workflow = must(Workflow::builder("w").state(json(1), [](const json &, const StageRecord &) -> Result<json> {
        return std::unexpected(Error::permanent("reduction"));
    }).stage(Stage::deterministic_fn("s", [&](const RunView &) -> Result<json> {
        ++calls; return std::unexpected(Error::transient("action"));
    }).retry(RetryPolicy::exponential(2, 0ms))).build());
    auto outcome = must(engine(workflow).run("w", "input"));
    EXPECT_EQ(outcome->error()->kind(), ErrorKind::StateReduction); EXPECT_EQ(calls, 1);
    EXPECT_EQ(outcome->result().state, 1); EXPECT_EQ(outcome->result().history[0].failures.size(), 2);
}

TEST(Parity, callbacks_throw_as_typed_panic_errors) {
    for (int phase = 0; phase < 3; ++phase) {
        auto stage = Stage::deterministic_fn("s", [phase](const RunView &) -> Result<json> {
            if (phase == 0) throw std::runtime_error("action panic");
            return json(1);
        }).route([phase](const RunView &) -> Result<Route> {
            if (phase == 1) throw std::runtime_error("router panic");
            return Route::complete();
        });
        auto workflow = must(Workflow::builder("w").stage(stage).state(nullptr,
            [phase](const json &, const StageRecord &) -> Result<json> {
                if (phase == 2) throw 42;
                return json(nullptr);
            }).build());
        auto outcome = must(engine(workflow).run("w", "input"));
        EXPECT_EQ(outcome->kind(), OutcomeKind::Failed); EXPECT_EQ(outcome->error()->kind(), ErrorKind::Panic);
        EXPECT_EQ(outcome->result().history.size(), 1);
    }
}

TEST(Parity, dynamic_route_missing_target_is_a_routing_failure) {
    auto workflow = must(Workflow::builder("w").stage(Stage::route_only("s")
        .route([](const RunView &) -> Result<Route> { return Route::next("missing"); })).build());
    auto outcome = must(engine(workflow).run("w", "input"));
    EXPECT_EQ(outcome->error(), Error::routing("routing selected missing stage 'missing'"));
    EXPECT_TRUE(outcome->result().latest_outputs.empty());
}

TEST(Parity, timed_action_unblocks_and_retry_gets_a_fresh_stop_token) {
    std::atomic_uint calls{0};
    auto workflow = must(Workflow::builder("w").stage(Stage::deterministic_fn("s", [&](const RunView & run) -> Result<json> {
        EXPECT_FALSE(run.stop.stop_requested());
        if (calls++ == 0) {
            std::mutex mutex; std::condition_variable_any changed; std::unique_lock lock(mutex);
            changed.wait(lock, run.stop, [] { return false; });
        }
        return json("ok");
    }).timeout(30ms).retry(RetryPolicy::exponential(2, 0ms))).build());
    auto outcome = must(engine(workflow).run("w", "input"));
    EXPECT_EQ(completed(outcome).latest_outputs.at("s"), "ok");
    EXPECT_EQ(outcome->result().history[0].failures[0].error.kind(), ErrorKind::Timeout);
    EXPECT_EQ(outcome->result().history[0].failures[0].error.message(), "stage 's' timed out after 30ms");
    EXPECT_EQ(calls, 2);
}

TEST(Parity, deadline_and_cancel_skip_reducer) {
    for (bool cancel : {false, true}) {
        std::atomic_uint reductions{0};
        auto action = std::make_shared<PendingAction>();
        auto entered = action->entered.get_future();
        RunLimits limits;
        if (!cancel) limits.deadline = 40ms;
        auto workflow = must(Workflow::builder("w").limits(limits).state(json(0),
            [&](const json &, const StageRecord &) -> Result<json> { ++reductions; throw std::runtime_error("should not reduce"); })
            .stage(Stage::deterministic("wait", action)).build());
        auto handle = must(engine(workflow).start("w", "input"));
        ASSERT_EQ(entered.wait_for(2s), std::future_status::ready);
        if (cancel) handle.cancel();
        auto outcome = must(handle.result());
        EXPECT_EQ(outcome->kind(), cancel ? OutcomeKind::Cancelled : OutcomeKind::LimitExceeded);
        if (!cancel) EXPECT_EQ(outcome->limit(), LimitKind::Deadline);
        EXPECT_EQ(reductions, 0); EXPECT_EQ(outcome->result().state, 0);
    }
}

TEST(Parity, approval_receiver_drop_and_timeout_finish_the_run) {
    for (bool drop : {false, true}) {
        auto tool = std::make_shared<ApprovalTool>();
        auto workflow = must(Workflow::builder("w").stage(Stage::tool("s", ToolAction(definition(tool)))
            .timeout(100ms)).build());
        auto handle = must(engine(workflow).start("w", "input"));
        auto events = handle.take_events();
        approval(*events);
        if (drop) events.reset();
        auto outcome = must(handle.result());
        EXPECT_EQ(outcome->kind(), OutcomeKind::Failed);
        EXPECT_EQ(outcome->error()->kind(), drop ? ErrorKind::Configuration : ErrorKind::Timeout);
        EXPECT_EQ(tool->calls, 0);
    }
}

TEST(Parity, denied_direct_tool_policies) {
    for (auto behavior : {DenialBehavior::Continue, DenialBehavior::FailStage, DenialBehavior::CancelRun}) {
        auto tool = std::make_shared<ApprovalTool>();
        auto workflow = must(Workflow::builder("w").stage(Stage::tool("s", ToolAction(definition(tool)).on_denied(behavior))).build());
        auto handle = must(engine(workflow).start("w", "input"));
        auto events = handle.take_events(); auto request = approval(*events);
        ASSERT_TRUE(handle.respond_to_approval(ApprovalResponse::deny(request.request_id)));
        auto outcome = must(handle.result());
        if (behavior == DenialBehavior::Continue) EXPECT_EQ(completed(outcome).latest_outputs.at("s"), (json{{"denied", true}}));
        if (behavior == DenialBehavior::FailStage) EXPECT_EQ(outcome->error()->kind(), ErrorKind::Denied);
        if (behavior == DenialBehavior::CancelRun) EXPECT_EQ(outcome->kind(), OutcomeKind::Cancelled);
        EXPECT_EQ(tool->calls, 0); EXPECT_EQ(outcome->result().usage.tool_calls, 0);
    }
}

TEST(Parity, last_handle_drop_cancels_but_dropping_a_clone_does_not) {
    auto action = std::make_shared<PendingAction>();
    auto entered = action->entered.get_future();
    auto workflow = must(Workflow::builder("w").stage(Stage::deterministic("s", action)).build());
    std::optional<RunHandle> handle = must(engine(workflow).start("w", "input"));
    auto events = handle->take_events();
    { auto clone = *handle; EXPECT_FALSE(clone.take_events()); }
    ASSERT_EQ(entered.wait_for(2s), std::future_status::ready);
    handle.reset();
    std::shared_ptr<RunOutcome> outcome;
    while (auto event = events->recv())
        if (auto * finished = std::get_if<RunEvent::Finished>(&event->value)) outcome = finished->outcome;
    ASSERT_TRUE(outcome); EXPECT_EQ(outcome->kind(), OutcomeKind::Cancelled);
}

TEST(Parity, deadline_bounds_retry_backoff) {
    auto workflow = must(Workflow::builder("w").limits(RunLimits{10, 0, 0, 30ms})
        .stage(Stage::deterministic_fn("s", [](const RunView &) -> Result<json> {
            return std::unexpected(Error::transient("retry"));
        }).retry(RetryPolicy::exponential(3, 1s))).build());
    auto outcome = must(engine(workflow).run("w", "input"));
    EXPECT_EQ(outcome->limit(), LimitKind::Deadline); EXPECT_EQ(outcome->result().history.size(), 1);
}

TEST(Parity, tool_panic_and_cancellation_emit_aborted_events) {
    class FailingTool final : public Tool {
    public:
        bool panic;
        explicit FailingTool(bool value) : panic(value) {}
        Result<json> call(const ToolContext & context, json) override {
            if (panic) throw std::runtime_error("tool panic");
            std::mutex mutex; std::condition_variable_any changed; std::unique_lock lock(mutex);
            changed.wait(lock, context.cancellation, [] { return false; });
            return json(nullptr);
        }
    };
    for (bool panic : {false, true}) {
        auto workflow = must(Workflow::builder("w").stage(Stage::tool("s", ToolAction(definition(std::make_shared<FailingTool>(panic))))).build());
        auto handle = must(engine(workflow).start("w", "input"));
        auto events = handle.take_events();
        unsigned aborted = 0, finished = 0;
        while (auto event = events->recv()) {
            if (!panic && std::holds_alternative<RunEvent::ToolCallStarted>(event->value)) handle.cancel();
            aborted += std::holds_alternative<RunEvent::ToolCallAborted>(event->value);
            finished += std::holds_alternative<RunEvent::ToolCallFinished>(event->value);
        }
        auto outcome = must(handle.result());
        EXPECT_EQ(aborted, 1); EXPECT_EQ(finished, 0);
        if (panic) EXPECT_EQ(outcome->error()->kind(), ErrorKind::Panic);
        else EXPECT_EQ(outcome->kind(), OutcomeKind::Cancelled);
    }
}

TEST(Parity, llm_hooks_requests_usage_and_merged_tool_calls) {
    class Hook final : public LlmLoopHook {
    public:
        unsigned model_calls = 0;
        unsigned tool_results = 0;
        Result<void> before_model(std::vector<Message> & messages) override {
            if (model_calls++ == 1) {
                EXPECT_EQ(messages.size(), 5);
                EXPECT_EQ(messages[3].tool_call_id, "same");
                EXPECT_EQ(json::parse(messages[3].content), (json{{"x", 2}}));
                EXPECT_EQ(json::parse(messages[4].content), (json{{"error", "tool_not_available"}, {"tool", "missing"}}));
            }
            return {};
        }
        Result<void> after_model(ModelResponse & response) override {
            // Token accounting happens before this hook, as in Rust.
            response.usage = {999, 999}; return {};
        }
        Result<void> after_tool(const ToolCall &, ToolResult &) override { ++tool_results; return {}; }
    };
    class Capture final : public LlmProvider {
    public:
        std::shared_ptr<MockLlmProvider> source = mock({{
            ToolCall{"same", "echo", json{{"x", 1}}}, ToolCall{"same", "echo", json{{"x", 2}}},
            ToolCall{"", "missing", nullptr}, TokenUsage{10, 3}, TokenUsage{12, 4}}, {"done", TokenUsage{5, 2}}});
        Result<std::unique_ptr<ModelStream>> stream(const ModelRequest & request) override {
            EXPECT_EQ(request.temperature, 0.25); EXPECT_EQ(request.max_tokens, 123);
            EXPECT_EQ(request.model, "writer"); EXPECT_EQ(request.extra_body, (json{{"top_k", 5}}));
            EXPECT_EQ(request.messages[0].content, "input-derived");
            EXPECT_EQ(request.tools.size(), 1);
            return source->stream(request);
        }
    };
    auto hook = std::make_shared<Hook>(); auto echo = std::make_shared<EchoTool>();
    auto action = LlmAction::text(Prompt::dynamic([](const RunView & run) -> Result<std::string> {
        return run.input.get<std::string>() + "-derived";
    })).with_hook(hook).with_tools({definition(echo)}).with_model("writer").with_temperature(0.25)
        .with_max_tokens(123).with_extra_body(json{{"top_k", 5}});
    auto workflow = must(Workflow::builder("w").stage(Stage::llm("s", action)).build());
    auto outcome = must(engine(workflow, std::make_shared<Capture>()).run("w", "input"));
    EXPECT_EQ(completed(outcome).usage, (Usage{2, 1, 17, 6}));
    EXPECT_EQ(echo->calls, 1); EXPECT_EQ(hook->tool_results, 2);
}

TEST(Parity, hooks_can_reject_or_replace_tools_without_spending_budget) {
    class Hook final : public LlmLoopHook {
        Result<ToolDecision> before_tool(const ToolCall & call) override {
            return call.id == "reject" ? ToolDecision::reject("no") : ToolDecision::replace(json("replacement"));
        }
        Result<void> after_tool(const ToolCall & call, ToolResult & result) override {
            if (call.id == "reject") { EXPECT_FALSE(result.succeeded); EXPECT_EQ(result.value["reason"], "no"); }
            else { EXPECT_TRUE(result.succeeded); EXPECT_EQ(result.value, "replacement"); }
            return {};
        }
    };
    auto tool = std::make_shared<EchoTool>();
    auto action = LlmAction::text(Prompt::text("p")).with_tools({definition(tool)}).with_hook(std::make_shared<Hook>());
    auto workflow = must(Workflow::builder("w").limits(RunLimits{1, 2, 0, {}}).stage(Stage::llm("s", action)).build());
    auto outcome = must(engine(workflow, mock({{ToolCall{"reject", "echo", nullptr}, ToolCall{"replace", "echo", nullptr}}, {"ok"}})).run("w", "input"));
    EXPECT_EQ(completed(outcome).latest_outputs.at("s"), "ok"); EXPECT_EQ(tool->calls, 0);
}

TEST(Parity, tool_round_limit_precedes_invocation) {
    auto tool = std::make_shared<EchoTool>();
    auto workflow = must(Workflow::builder("w").stage(Stage::llm("s", LlmAction::text(Prompt::text("p"))
        .with_tools({definition(tool)}).with_max_tool_rounds(0))).build());
    auto outcome = must(engine(workflow, mock({{ToolCall{"id", "echo", nullptr}}})).run("w", "input"));
    EXPECT_EQ(outcome->limit(), LimitKind::ToolRounds); EXPECT_EQ(tool->calls, 0);
}

TEST(Parity, usage_saturates_and_run_metadata_is_preserved) {
    Usage usage{UINT32_MAX, UINT32_MAX, UINT64_MAX, UINT64_MAX};
    usage += Usage{1, 1, 1, 1};
    EXPECT_EQ(usage, (Usage{UINT32_MAX, UINT32_MAX, UINT64_MAX, UINT64_MAX}));
    EXPECT_EQ(Usage{} - usage, Usage{});
    auto workflow = must(Workflow::builder("w").stage(Stage::deterministic_fn("s", [](const RunView & run) -> Result<json> {
        EXPECT_EQ(run.metadata, (json{{"owner", "test"}})); EXPECT_EQ(*run.input_text(), "hello");
        EXPECT_TRUE(run.output_history("absent").empty());
        return json(run.run_id.as_str());
    })).build());
    auto handle = must(engine(workflow).start("w", RunInput::text("hello").with_metadata(json{{"owner", "test"}})));
    auto outcome = must(handle.result());
    EXPECT_EQ(completed(outcome).run_id, handle.id());
    EXPECT_EQ(outcome->result().latest_outputs.at("s"), handle.id().as_str());
}

TEST(LlamaCpp, parses_completed_tool_arguments_and_multiple_calls) {
    auto calls = detail::parse_tool_calls(R"(<tool_call>{"id":"one","function":{"name":"echo","arguments":"{\"x\":1}"}}</tool_call>
        <tool_call>{"id":"two","name":"echo","arguments":{"x":2}}</tool_call>)");
    ASSERT_EQ(calls.size(), 2);
    EXPECT_EQ(calls[0].id, "one"); EXPECT_EQ(calls[0].name, "echo"); EXPECT_EQ(calls[0].arguments, (json{{"x", 1}}));
    EXPECT_EQ(calls[1].id, "two"); EXPECT_EQ(calls[1].arguments, (json{{"x", 2}}));
    calls = detail::parse_tool_calls(R"({"name":"echo","arguments":{}})");
    ASSERT_EQ(calls.size(), 1); EXPECT_FALSE(calls[0].id.empty());
    EXPECT_TRUE(detail::parse_tool_calls(R"({"name":"a person's name"})").empty());
    EXPECT_TRUE(detail::parse_tool_calls(R"(<tool_call>{"name":"echo","arguments":{}})").empty());
    EXPECT_TRUE(detail::parse_tool_calls(R"({"name":"echo","arguments":"not json"})").empty());
}

TEST(LlamaCpp, invalid_settings_and_cancelled_requests_do_not_load_models) {
    LlamaCppProvider provider("missing-test-model.gguf");
    ModelRequest request;
    for (auto setting : {json{{"n_ctx", -1}}, json{{"n_threads", 0}}, json{{"n_predict", 0}},
                         json{{"top_p", "wrong"}}, json{{"grammar", 3}},
                         json{{"chat_template_kwargs", false}},
                         json{{"chat_template_kwargs", {{"enable_thinking", "no"}}}}, json::array()}) {
        request.extra_body = setting;
        auto result = provider.stream(request);
        ASSERT_FALSE(result); EXPECT_EQ(result.error().kind(), ErrorKind::InvalidInput);
    }
    request.extra_body.reset();
    std::stop_source stop; stop.request_stop(); request.stop = stop.get_token();
    auto result = provider.stream(request);
    ASSERT_FALSE(result); EXPECT_EQ(result.error().kind(), ErrorKind::Cancelled);
}
