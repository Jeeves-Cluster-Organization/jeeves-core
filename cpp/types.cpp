#include <jeeves/types.hpp>

#include <array>
#include <iomanip>
#include <limits>
#include <random>
#include <sstream>

namespace jeeves {
namespace {
template <class T> T sat_add(T a, T b) {
    return b > std::numeric_limits<T>::max() - a ? std::numeric_limits<T>::max() : a + b;
}
template <class T> T sat_sub(T a, T b) { return a < b ? 0 : a - b; }
}

RunId::RunId() {
    std::array<unsigned char, 16> bytes{};
    std::random_device random;
    for (auto & byte : bytes) byte = static_cast<unsigned char>(random());
    bytes[6] = static_cast<unsigned char>((bytes[6] & 0x0fU) | 0x40U);
    bytes[8] = static_cast<unsigned char>((bytes[8] & 0x3fU) | 0x80U);
    std::ostringstream out;
    out << std::hex << std::setfill('0');
    for (std::size_t i = 0; i < bytes.size(); ++i) {
        if (i == 4 || i == 6 || i == 8 || i == 10) out << '-';
        out << std::setw(2) << static_cast<unsigned>(bytes[i]);
    }
    value_ = out.str();
}

Result<RunId> RunId::from_string(std::string value) {
    if (value.empty()) return std::unexpected(Error::invalid_input("run id cannot be empty"));
    return RunId(std::move(value));
}

Usage & Usage::operator+=(const Usage & rhs) noexcept {
    llm_calls = sat_add(llm_calls, rhs.llm_calls);
    tool_calls = sat_add(tool_calls, rhs.tool_calls);
    input_tokens = sat_add(input_tokens, rhs.input_tokens);
    output_tokens = sat_add(output_tokens, rhs.output_tokens);
    return *this;
}

Usage operator-(const Usage & lhs, const Usage & rhs) noexcept {
    return {sat_sub(lhs.llm_calls, rhs.llm_calls), sat_sub(lhs.tool_calls, rhs.tool_calls),
            sat_sub(lhs.input_tokens, rhs.input_tokens), sat_sub(lhs.output_tokens, rhs.output_tokens)};
}

const json * RunView::latest_output(const std::string & stage_name) const noexcept {
    for (auto it = history.rbegin(); it != history.rend(); ++it) {
        if (it->stage == stage_name && it->succeeded()) return it->output ? &*it->output : nullptr;
    }
    return nullptr;
}

std::vector<const json *> RunView::output_history(const std::string & stage_name) const {
    std::vector<const json *> values;
    for (const auto & record : history) {
        if (record.stage == stage_name && record.succeeded() && record.output) values.push_back(&*record.output);
    }
    return values;
}

const std::string * RunView::input_text() const noexcept {
    return input.is_string() ? &input.get_ref<const std::string &>() : nullptr;
}

RunOutcome RunOutcome::completed(RunResult result) { return {OutcomeKind::Completed, std::move(result)}; }
RunOutcome RunOutcome::failed(RunResult result, Error error) {
    RunOutcome outcome(OutcomeKind::Failed, std::move(result)); outcome.error_ = std::move(error); return outcome;
}
RunOutcome RunOutcome::cancelled(RunResult result) { return {OutcomeKind::Cancelled, std::move(result)}; }
RunOutcome RunOutcome::limit_exceeded(RunResult result, LimitKind limit) {
    RunOutcome outcome(OutcomeKind::LimitExceeded, std::move(result)); outcome.limit_ = limit; return outcome;
}

} // namespace jeeves
