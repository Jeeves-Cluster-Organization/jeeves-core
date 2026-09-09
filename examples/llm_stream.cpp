#include <jeeves/jeeves.hpp>

#include <iostream>

using namespace jeeves;

Result<void> briefing() {
    auto llm = std::make_shared<MockLlmProvider>(std::vector<std::vector<ModelStreamEvent>>{{
        "Detective Mara ", "found the ledger ", "behind the clock tower."}});
    auto workflow = Workflow::builder("briefing").stage(Stage::llm("speak", LlmAction::text(
        Prompt::text("Summarize the case file in one dramatic sentence.")))).build();
    if (!workflow) return std::unexpected(workflow.error());
    auto registered = Engine::builder().llm(llm).workflow(std::move(*workflow));
    if (!registered) return std::unexpected(registered.error());
    auto engine = registered->build();
    if (!engine) return std::unexpected(engine.error());
    auto handle = engine->start("briefing", "case 41: the missing ledger");
    if (!handle) return std::unexpected(handle.error());
    auto events = handle->take_events();
    while (auto event = events->recv())
        if (const auto * delta = std::get_if<RunEvent::TextDelta>(&event->value)) std::cout << delta->content << std::flush;
    auto outcome = handle->result();
    if (!outcome) return std::unexpected(outcome.error());
    std::cout << "\ncompleted: " << (*outcome)->completed() << "\nmodel calls: " << (*outcome)->result().usage.llm_calls << '\n';
    if (!(*outcome)->completed()) return std::unexpected(Error::permanent("briefing did not complete"));
    return {};
}

int main() {
    auto result = briefing();
    if (!result) { std::cerr << result.error() << '\n'; return 1; }
}
