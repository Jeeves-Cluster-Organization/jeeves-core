#pragma once

#include <jeeves/llm.hpp>
#include "../sync.hpp"

namespace jeeves::detail {

// Best-effort llama text protocol; malformed candidates remain ordinary text.
inline std::vector<ToolCall> parse_tool_calls(const std::string & text) {
    std::vector<ToolCall> calls;
    auto parse = [&](const std::string & encoded) {
        try {
            auto value = json::parse(encoded);
            const auto & function = value.contains("function") ? value.at("function") : value;
            if (!function.is_object() || !function.contains("name") || !function.at("name").is_string()
                || !function.contains("arguments")) return;
            json arguments = function.at("arguments");
            if (arguments.is_string()) arguments = json::parse(arguments.get<std::string>());
            auto id = value.value("id", std::string{});
            if (id.empty()) id = uuid_v4();
            calls.push_back({std::move(id), function.at("name").get<std::string>(), std::move(arguments)});
        } catch (const json::exception &) {}
    };
    std::size_t position = 0;
    bool tagged = false;
    while ((position = text.find("<tool_call>", position)) != std::string::npos) {
        tagged = true;
        position += 11;
        auto end = text.find("</tool_call>", position);
        if (end == std::string::npos) break;
        parse(text.substr(position, end - position));
        position = end + 12;
    }
    if (!tagged) parse(text);
    return calls;
}

} // namespace jeeves::detail
