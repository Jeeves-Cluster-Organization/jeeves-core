#pragma once

#include <jeeves/events.hpp>
#include <jeeves/workflow.hpp>

#include <memory>
#include <optional>
#include <string>

namespace jeeves {
namespace detail {
struct EngineInner;
struct RunLifetime;
template <class T> struct QueueState;
template <class T> struct ResultState;
}

class RunHandle {
public:
    RunHandle(const RunHandle & other);
    RunHandle & operator=(const RunHandle & other);
    RunHandle(RunHandle &&) noexcept = default;
    RunHandle & operator=(RunHandle &&) noexcept = default;
    ~RunHandle() = default;

    [[nodiscard]] const RunId & id() const noexcept { return run_id_; }
    void cancel() const;
    Result<void> respond_to_approval(ApprovalResponse response) const;
    std::optional<EventReceiver> take_events();
    void discard_events();
    Result<std::shared_ptr<RunOutcome>> result() const;

private:
    friend class Engine;
    RunHandle(RunId id, std::shared_ptr<detail::RunLifetime> lifetime,
              std::shared_ptr<detail::QueueState<ApprovalResponse>> approvals,
              std::shared_ptr<detail::ResultState<RunOutcome>> result,
              std::optional<EventReceiver> events);
    RunId run_id_;
    std::shared_ptr<detail::RunLifetime> lifetime_;
    std::shared_ptr<detail::QueueState<ApprovalResponse>> approvals_;
    std::shared_ptr<detail::ResultState<RunOutcome>> result_;
    std::optional<EventReceiver> events_;
};

class EngineBuilder;
class Engine {
public:
    static EngineBuilder builder();
    Result<RunHandle> start(const std::string & workflow, RunInput input) const;
    Result<std::shared_ptr<RunOutcome>> run(const std::string & workflow, RunInput input) const;

private:
    friend class EngineBuilder;
    Result<RunHandle> start_impl(const std::string & workflow, RunInput input, bool events) const;
    explicit Engine(std::shared_ptr<detail::EngineInner> inner) : inner_(std::move(inner)) {}
    std::shared_ptr<detail::EngineInner> inner_;
};

class EngineBuilder {
public:
    EngineBuilder & llm(std::shared_ptr<LlmProvider> provider) { llm_ = std::move(provider); return *this; }
    Result<EngineBuilder> workflow(Workflow workflow);
    Result<Engine> build() const;

private:
    std::unordered_map<std::string, std::shared_ptr<Workflow>> workflows_;
    std::shared_ptr<LlmProvider> llm_;
};

} // namespace jeeves
