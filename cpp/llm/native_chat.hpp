#pragma once

#include "gemma4.hpp"

#include <string_view>

namespace jeeves::detail {

// llama.cpp chat-template adapters for GGUFs whose embedded Jinja is not in
// libllama's builtins. Match on architecture + a template marker, then own
// prompt rendering, visible-text extraction, and whether a JSON token grammar
// is safe. Streaming stays per-token unless defer_visible_text is set.
struct NativeChat {
    std::string_view architecture;
    std::string_view template_mark;
    Result<std::string> (*prompt)(const ModelRequest &);
    std::string (*visible_text)(std::string);
    bool allow_default_schema_grammar;
    bool defer_visible_text;
};

inline constexpr NativeChat kGemma4Chat{
    "gemma4",
    "<|turn>",
    gemma4_text_prompt,
    gemma4_visible_text,
    false,
    true,
};

inline const NativeChat *native_chat_for(std::string_view architecture, std::string_view embedded) {
    for (const auto *chat : {&kGemma4Chat})
        if (architecture == chat->architecture && embedded.find(chat->template_mark) != std::string_view::npos)
            return chat;
    return nullptr;
}

} // namespace jeeves::detail
