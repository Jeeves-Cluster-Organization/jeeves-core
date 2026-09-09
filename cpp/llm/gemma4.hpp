#pragma once
#include <jeeves/llm.hpp>
#include <string_view>

namespace jeeves::detail {
// Text-only, single-turn subset of the GGUF's Gemma 4 Jinja template, with
// enable_thinking=false. Used only when libllama's built-in templates reject it.
// The tokenizer adds BOS. Tools/history need the full template implementation.
inline Result<std::string> gemma4_text_prompt(const ModelRequest &request) {
    if (!request.tools.empty() || request.messages.empty() || request.messages.size() > 2)
        return std::unexpected(Error::configuration("Gemma 4 fallback supports single-turn text only"));
    std::string system;
    std::size_t user = 0;
    if (request.messages.front().role == Role::System) {
        system = request.messages.front().content;
        user = 1;
    }
    if (user + 1 != request.messages.size() || request.messages[user].role != Role::User)
        return std::unexpected(Error::configuration("Gemma 4 fallback requires one user message"));
    for (const auto &message : request.messages)
        if (!message.tool_calls.empty() || message.tool_call_id)
            return std::unexpected(Error::configuration("Gemma 4 fallback does not support tool messages"));
    if (request.response_schema)
        system += "\nReturn JSON matching this schema: " + request.response_schema->dump();
    auto trim = [](std::string_view value) {
        const auto first = value.find_first_not_of(" \t\r\n\f\v");
        if (first == std::string_view::npos) return std::string{};
        const auto last = value.find_last_not_of(" \t\r\n\f\v");
        return std::string(value.substr(first, last - first + 1));
    };
    std::string prompt;
    if (user || !system.empty()) prompt = "<|turn>system\n" + trim(system) + "<turn|>\n";
    prompt += "<|turn>user\n" + trim(request.messages[user].content) + "<turn|>\n";
    prompt += "<|turn>model\n<|channel>thought\n<channel|>";
    return prompt;
}
} // namespace jeeves::detail
