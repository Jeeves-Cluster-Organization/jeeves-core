#pragma once

#include <jeeves/llm.hpp>

#include <memory>
#include <string>

namespace jeeves {

#ifdef JEEVES_HAS_LLAMA

class LlamaCppProvider final : public LlmProvider {
public:
    explicit LlamaCppProvider(std::string gguf_path);
    ~LlamaCppProvider() override;
    LlamaCppProvider(LlamaCppProvider &&) noexcept;
    LlamaCppProvider & operator=(LlamaCppProvider &&) noexcept;
    LlamaCppProvider(const LlamaCppProvider &) = delete;
    LlamaCppProvider & operator=(const LlamaCppProvider &) = delete;

    LlamaCppProvider & with_model_role(std::string role, std::string gguf_path);
    Result<std::unique_ptr<ModelStream>> stream(const ModelRequest & request) override;

private:
    struct Impl;
    std::unique_ptr<Impl> impl_;
};

#endif

} // namespace jeeves
