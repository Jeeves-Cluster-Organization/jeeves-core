#include <jeeves/jeeves.hpp>
#include "../cpp/llm/gemma4.hpp"
#include <gtest/gtest.h>

#include <limits>

using namespace jeeves;
using namespace std::chrono_literals;

namespace {
void configuration(const Result<Workflow> & result, const std::string & message) {
    ASSERT_FALSE(result);
    EXPECT_EQ(result.error().kind(), ErrorKind::Configuration);
    EXPECT_EQ(result.error().message(), message);
}

class Echo final : public Tool {
    Result<json> call(const ToolContext &, json value) override { return value; }
};

std::shared_ptr<ToolDefinition> tool() {
    return std::make_shared<ToolDefinition>(*ToolSpec::create("echo", "echo", json::object()), std::make_shared<Echo>());
}
}

TEST(Validation, duplicate_stages_are_rejected_at_build_time) {
    configuration(Workflow::builder("duplicate").stage(Stage::route_only("same"))
        .stage(Stage::route_only("same")).build(), "duplicate stage 'same'");
}

TEST(Validation, missing_static_route_is_rejected_at_build_time) {
    configuration(Workflow::builder("missing").stage(Stage::route_only("entry").next("absent")).build(),
                  "stage 'entry' targets missing stage 'absent'");
    configuration(Workflow::builder("missing").stage(Stage::route_only("entry").on_error("absent")).build(),
                  "stage 'entry' targets missing stage 'absent'");
}

TEST(Validation, llm_workflow_requires_provider_when_engine_is_built) {
    auto workflow = Workflow::builder("llm").stage(Stage::llm("answer", LlmAction::text(Prompt::text("answer")))).build();
    ASSERT_TRUE(workflow);
    auto registered = Engine::builder().workflow(*workflow);
    ASSERT_TRUE(registered);
    auto engine = registered->build();
    ASSERT_FALSE(engine);
    EXPECT_EQ(engine.error().message(), "an LLM provider is required by at least one workflow");
}

TEST(Validation, empty_names_and_workflows_are_rejected) {
    configuration(Workflow::builder("").stage(Stage::route_only("s")).build(), "workflow name cannot be empty");
    configuration(Workflow::builder("w").build(), "workflow must contain at least one stage");
    configuration(Workflow::builder("w").stage(Stage::route_only("")).build(), "stage name cannot be empty");
    EXPECT_EQ(Engine::builder().build().error().message(), "engine must contain at least one workflow");
    auto workflow = Workflow::builder("w").stage(Stage::route_only("s")).build();
    auto builder = Engine::builder();
    ASSERT_TRUE(builder.workflow(*workflow));
    EXPECT_EQ(builder.workflow(*workflow).error().message(), "duplicate workflow 'w'");
}

TEST(Validation, stage_bounds_and_retry_policies_are_validated) {
    configuration(Workflow::builder("w").stage(Stage::route_only("s").max_visits(0)).build(),
                  "stage 's' max_visits must be positive");
    configuration(Workflow::builder("w").stage(Stage::route_only("s").timeout(0ms)).build(),
                  "stage 's' timeout must be positive");
    configuration(Workflow::builder("w").stage(Stage::route_only("s").retry(RetryPolicy::exponential(0, 0ms))).build(),
                  "retry max_attempts must be positive");
    for (double multiplier : {0.5, std::numeric_limits<double>::infinity(), std::numeric_limits<double>::quiet_NaN()}) {
        configuration(Workflow::builder("w").stage(Stage::route_only("s").retry(
            RetryPolicy::exponential(2, 1ms).with_multiplier(multiplier))).build(),
            "retry backoff multiplier must be finite and at least 1");
    }
    configuration(Workflow::builder("w").limits(RunLimits{0, 0, 0, {}}).stage(Stage::route_only("s")).build(),
                  "max_stage_executions must be positive");
    EXPECT_TRUE(Workflow::builder("w").limits(RunLimits{1, 0, 0, 0ms}).stage(Stage::route_only("s")).build());
}

TEST(Validation, llm_settings_are_validated) {
    auto build = [](LlmAction action) { return Workflow::builder("w").stage(Stage::llm("s", action)).build(); };
    configuration(build(LlmAction::text(Prompt::text("p")).with_temperature(std::numeric_limits<double>::infinity())),
                  "LLM temperature must be finite");
    configuration(build(LlmAction::text(Prompt::text("p")).with_max_tokens(0)), "LLM max_tokens must be positive");
    configuration(build(LlmAction::text(Prompt::text("p")).with_extra_body(json::array())), "LLM extra_body must be an object");
    auto echo = tool();
    configuration(build(LlmAction::text(Prompt::text("p")).with_tools({echo, echo})), "duplicate LLM tool 'echo'");
    EXPECT_TRUE(build(LlmAction::text(Prompt::text("p")).with_max_tool_rounds(0)));
}

TEST(Validation, retry_requires_replay_safe_tools_or_explicit_override) {
    auto echo = tool();
    auto policy = RetryPolicy::exponential(2, 0ms);
    configuration(Workflow::builder("w").stage(Stage::tool("s", ToolAction(echo)).retry(policy)).build(),
                  "stage 's' retries replay-unsafe tools: echo");
    configuration(Workflow::builder("w").stage(Stage::llm("s", LlmAction::text(Prompt::text("p"))
        .with_tools({echo})).retry(policy)).build(), "stage 's' retries replay-unsafe tools: echo");
    EXPECT_TRUE(Workflow::builder("w").stage(Stage::tool("s", ToolAction(echo))
        .retry(policy.allow_side_effect_replay())).build());
    echo->idempotent();
    EXPECT_TRUE(Workflow::builder("w").stage(Stage::tool("s", ToolAction(echo))
        .retry(RetryPolicy::exponential(2, 0ms))).build());
}

TEST(Validation, run_ids_and_tool_names_are_validated) {
    EXPECT_EQ(RunId::from_string("").error().message(), "run id cannot be empty");
    EXPECT_EQ(RunId::from_string("external-id")->as_str(), "external-id");
    RunId id;
    EXPECT_EQ(id.as_str().size(), 36);
    EXPECT_EQ(id.as_str()[14], '4');
    EXPECT_NE(std::string("89ab").find(id.as_str()[19]), std::string::npos);
    EXPECT_EQ(ToolSpec::create("", "", nullptr).error().message(), "tool name cannot be empty");
}

TEST(Validation, gemma4_single_turn_text_template) {
    ModelRequest request;
    request.messages = {Message::system(" Answer briefly. "), Message::user(" Hello ")};
    auto prompt = detail::gemma4_text_prompt(request);
    ASSERT_TRUE(prompt);
    EXPECT_EQ(*prompt, "<|turn>system\nAnswer briefly.<turn|>\n<|turn>user\nHello<turn|>\n"
                       "<|turn>model\n<|channel>thought\n<channel|>");
    request.response_schema = json{{"type", "object"}};
    prompt = detail::gemma4_text_prompt(request);
    ASSERT_TRUE(prompt);
    EXPECT_NE(prompt->find("Return JSON matching this schema: {\"type\":\"object\"}"), std::string::npos);
    request.messages.push_back(Message::assistant("history", {}));
    EXPECT_FALSE(detail::gemma4_text_prompt(request));
}
