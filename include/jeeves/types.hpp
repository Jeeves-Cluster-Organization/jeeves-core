#pragma once

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <expected>
#include <optional>
#include <ostream>
#include <stop_token>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

#include <nlohmann/json.hpp>

namespace jeeves {

using json = nlohmann::json;
using Duration = std::chrono::steady_clock::duration;

enum class ErrorKind {
    Configuration, NotFound, Transient, Timeout, InvalidInput, Denied,
    Permanent, Routing, StateReduction, Unavailable, Limit, Cancelled,
    Panic, Internal,
};

class Error {
public:
    Error(ErrorKind kind, std::string message) : kind_(kind), message_(std::move(message)) {}

    static Error configuration(std::string m) { return {ErrorKind::Configuration, std::move(m)}; }
    static Error not_found(std::string m) { return {ErrorKind::NotFound, std::move(m)}; }
    static Error transient(std::string m) { return {ErrorKind::Transient, std::move(m)}; }
    static Error timeout(std::string m) { return {ErrorKind::Timeout, std::move(m)}; }
    static Error invalid_input(std::string m) { return {ErrorKind::InvalidInput, std::move(m)}; }
    static Error denied(std::string m) { return {ErrorKind::Denied, std::move(m)}; }
    static Error permanent(std::string m) { return {ErrorKind::Permanent, std::move(m)}; }
    static Error routing(std::string m) { return {ErrorKind::Routing, std::move(m)}; }
    static Error state_reduction(std::string m) { return {ErrorKind::StateReduction, std::move(m)}; }
    static Error unavailable(std::string m) { return {ErrorKind::Unavailable, std::move(m)}; }
    static Error limit(std::string m) { return {ErrorKind::Limit, std::move(m)}; }
    static Error cancelled(std::string m) { return {ErrorKind::Cancelled, std::move(m)}; }
    static Error panic(std::string m) { return {ErrorKind::Panic, std::move(m)}; }
    static Error internal(std::string m) { return {ErrorKind::Internal, std::move(m)}; }

    [[nodiscard]] ErrorKind kind() const noexcept { return kind_; }
    [[nodiscard]] const std::string & message() const noexcept { return message_; }
    [[nodiscard]] bool is_retryable() const noexcept {
        return kind_ == ErrorKind::Transient || kind_ == ErrorKind::Timeout || kind_ == ErrorKind::Unavailable;
    }
    [[nodiscard]] Error with_kind(ErrorKind kind) const { return {kind, message_}; }
    friend bool operator==(const Error &, const Error &) = default;

private:
    ErrorKind kind_;
    std::string message_;
};

inline std::ostream & operator<<(std::ostream & out, const Error & error) {
    return out << error.message();
}

template <class T>
using Result = std::expected<T, Error>;

class RunId {
public:
    RunId();
    static Result<RunId> from_string(std::string value);
    [[nodiscard]] const std::string & as_str() const noexcept { return value_; }
    friend bool operator==(const RunId &, const RunId &) = default;

private:
    explicit RunId(std::string value) : value_(std::move(value)) {}
    std::string value_;
};

inline std::ostream & operator<<(std::ostream & out, const RunId & id) { return out << id.as_str(); }

struct RunInput {
    json input;
    json metadata = json::object();

    RunInput(json value) : input(std::move(value)) {}
    RunInput(const char * text) : input(std::string(text)) {}
    RunInput(std::string text) : input(std::move(text)) {}
    static RunInput text(std::string value) { return RunInput(std::move(value)); }
    RunInput & with_metadata(json value) { metadata = std::move(value); return *this; }
};

struct Usage {
    std::uint32_t llm_calls = 0;
    std::uint32_t tool_calls = 0;
    std::uint64_t input_tokens = 0;
    std::uint64_t output_tokens = 0;
    Usage & operator+=(const Usage & other) noexcept;
    friend Usage operator-(const Usage & lhs, const Usage & rhs) noexcept;
    friend bool operator==(const Usage &, const Usage &) = default;
};

struct StageAttempt {
    std::string stage;
    std::uint32_t visit = 0;
    std::uint32_t attempt = 0;
    friend bool operator==(const StageAttempt &, const StageAttempt &) = default;
};

enum class StageFailurePhase { Action, Routing, StateReduction, Control };

struct StageFailure {
    StageFailurePhase phase;
    Error error;
};

struct StageRecord {
    std::string stage;
    std::uint32_t visit = 0;
    std::uint32_t attempt = 0;
    std::optional<json> output;
    std::vector<StageFailure> failures;
    Usage usage;
    Duration duration{};

    [[nodiscard]] bool succeeded() const noexcept { return failures.empty(); }
    [[nodiscard]] const StageFailure * last_failure() const noexcept {
        return failures.empty() ? nullptr : &failures.back();
    }
};

struct RunView {
    const RunId & run_id;
    const json & input;
    const json & metadata;
    const json & state;
    const std::vector<StageRecord> & history;
    Usage usage;
    std::stop_token stop;

    [[nodiscard]] const json * latest_output(const std::string & stage) const noexcept;
    [[nodiscard]] std::vector<const json *> output_history(const std::string & stage) const;
    [[nodiscard]] const std::string * input_text() const noexcept;
};

struct RunResult {
    RunId run_id;
    std::string workflow;
    std::unordered_map<std::string, json> latest_outputs;
    std::vector<StageRecord> history;
    json state;
    Usage usage;
    Duration duration{};
};

enum class LimitKind { StageExecutions, LlmCalls, ToolCalls, StageVisits, ToolRounds, Deadline };
enum class OutcomeKind { Completed, Failed, Cancelled, LimitExceeded };

class RunOutcome {
public:
    static RunOutcome completed(RunResult result);
    static RunOutcome failed(RunResult result, Error error);
    static RunOutcome cancelled(RunResult result);
    static RunOutcome limit_exceeded(RunResult result, LimitKind limit);

    [[nodiscard]] const RunResult & result() const noexcept { return result_; }
    [[nodiscard]] bool completed() const noexcept { return kind_ == OutcomeKind::Completed; }
    [[nodiscard]] OutcomeKind kind() const noexcept { return kind_; }
    [[nodiscard]] const std::optional<Error> & error() const noexcept { return error_; }
    [[nodiscard]] const std::optional<LimitKind> & limit() const noexcept { return limit_; }

private:
    RunOutcome(OutcomeKind kind, RunResult result) : kind_(kind), result_(std::move(result)) {}
    OutcomeKind kind_;
    RunResult result_;
    std::optional<Error> error_;
    std::optional<LimitKind> limit_;
};

} // namespace jeeves
