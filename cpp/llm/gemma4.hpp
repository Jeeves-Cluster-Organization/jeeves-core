#pragma once
#include <jeeves/llm.hpp>
#include <string_view>

namespace jeeves::detail {

inline std::string trim_ws(std::string value) {
    const auto first = value.find_first_not_of(" \t\r\n\f\v");
    if (first == std::string::npos)
        return {};
    const auto last = value.find_last_not_of(" \t\r\n\f\v");
    return value.substr(first, last - first + 1);
}

// Text-only, single-turn subset of Gemma 4's embedded Jinja, with thinking on.
// The tokenizer adds BOS. Tools/history need the full template implementation.
inline std::string gemma4_visible_text(std::string text) {
    const auto close = text.rfind("<channel|>");
    if (close != std::string::npos)
        text = text.substr(close + 10);
    const std::string response = "<|channel>response\n";
    if (const auto pos = text.find(response); pos != std::string::npos)
        text = text.substr(pos + response.size());
    text = trim_ws(std::move(text));
    if (text.starts_with("```")) {
        const auto newline = text.find('\n');
        if (newline != std::string::npos)
            text = text.substr(newline + 1);
        if (text.ends_with("```"))
            text.resize(text.size() - 3);
        text = trim_ws(std::move(text));
    }
    return text;
}

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
    std::string prompt = "<|turn>system\n<|think|>\n";
    if (const auto body = trim_ws(system); !body.empty())
        prompt += body;
    prompt += "<turn|>\n<|turn>user\n" + trim_ws(request.messages[user].content) + "<turn|>\n";
    prompt += "<|turn>model\n";
    return prompt;
}
} // namespace jeeves::detail
