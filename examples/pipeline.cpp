#include <jeeves/jeeves.hpp>

#include <atomic>
#include <iostream>

using namespace jeeves;
using namespace std::chrono_literals;

Result<void> pipeline() {
    auto calls = std::make_shared<std::atomic_uint>(0);
    auto workflow = Workflow::builder("newsroom")
        .stage(Stage::deterministic_fn("collect", [](const RunView & run) -> Result<json> {
            const auto * topic = run.input_text();
            if (!topic) return std::unexpected(Error::invalid_input("run input must be text"));
            return json{{"topic", *topic}, {"leads", 3}};
        }).next("verify"))
        .stage(Stage::deterministic_fn("verify", [calls](const RunView & run) -> Result<json> {
            const auto * collected = run.latest_output("collect");
            if (!collected || !collected->contains("topic") || !collected->at("topic").is_string())
                return std::unexpected(Error::invalid_input("collect.topic missing"));
            if (calls->fetch_add(1) < 2) return std::unexpected(Error::transient("sources unavailable"));
            return json{{"verified", true}};
        }).retry(RetryPolicy::exponential(3, 50ms))).build();
    if (!workflow) return std::unexpected(workflow.error());
    auto registered = Engine::builder().workflow(std::move(*workflow));
    if (!registered) return std::unexpected(registered.error());
    auto engine = registered->build();
    if (!engine) return std::unexpected(engine.error());
    auto handle = engine->start("newsroom", "city hall");
    if (!handle) return std::unexpected(handle.error());
    auto events = handle->take_events();
    while (auto event = events->recv()) {
        if (const auto * started = std::get_if<RunEvent::StageStarted>(&event->value))
            std::cout << "stage started: " << started->attempt.stage << " (visit " << started->attempt.visit
                      << ", attempt " << started->attempt.attempt << ")\n";
        if (const auto * routed = std::get_if<RunEvent::Routed>(&event->value))
            std::cout << "routed " << routed->from << " -> " << routed->to.value_or("<complete>") << '\n';
    }
    auto outcome = handle->result();
    if (!outcome) return std::unexpected(outcome.error());
    std::cout << "run finished: completed = " << (*outcome)->completed() << '\n';
    for (const auto & record : (*outcome)->result().history)
        std::cout << "attempt " << record.stage << ':' << record.attempt << " -> "
                  << record.output.value_or(nullptr).dump() << " (" << record.failures.size() << " failure(s))\n";
    if (!(*outcome)->completed()) return std::unexpected(Error::permanent("pipeline did not complete"));
    return {};
}

int main() {
    auto result = pipeline();
    if (!result) { std::cerr << result.error() << '\n'; return 1; }
}
