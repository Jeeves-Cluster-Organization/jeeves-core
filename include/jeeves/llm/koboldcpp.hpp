#pragma once

#include <jeeves/llm.hpp>

#include <chrono>
#include <cstdint>
#include <filesystem>
#include <memory>
#include <stop_token>
#include <string>

namespace jeeves {

struct KoboldCppEndpoint {
    std::string host = "127.0.0.1";
    std::uint16_t port = 0;
    std::string model = "local";
};

// Loopback-only OpenAI chat-completions adapter. The provider does not own or
// launch KoboldCpp; applications may use KoboldCppProcess for that lifecycle.
class KoboldCppProvider final : public LlmProvider {
public:
    explicit KoboldCppProvider(KoboldCppEndpoint endpoint);
    Result<std::unique_ptr<ModelStream>> stream(const ModelRequest & request) override;

    [[nodiscard]] const KoboldCppEndpoint & endpoint() const noexcept { return endpoint_; }

private:
    struct State;
    KoboldCppEndpoint endpoint_;
    std::shared_ptr<State> state_;
};

enum class KoboldCppGpuBackend { Metal, Cuda, Vulkan };

struct KoboldCppProcessConfig {
    std::filesystem::path executable;
    std::filesystem::path model;
    std::filesystem::path chat_completions_adapter;
    std::filesystem::path diagnostics_file;
    KoboldCppGpuBackend backend;
    std::uint32_t context_size = 4096;
    std::string required_version;
    std::chrono::milliseconds startup_timeout{300000};
};

// A small cross-platform companion process. It always binds KoboldCpp to
// 127.0.0.1 on an OS-selected private port and never enables its launcher UI.
class KoboldCppProcess final {
public:
    static Result<std::shared_ptr<KoboldCppProcess>> launch(KoboldCppProcessConfig config);
    ~KoboldCppProcess();
    KoboldCppProcess(const KoboldCppProcess &) = delete;
    KoboldCppProcess & operator=(const KoboldCppProcess &) = delete;

    [[nodiscard]] const KoboldCppEndpoint & endpoint() const noexcept;
    [[nodiscard]] const std::filesystem::path & diagnostics_file() const noexcept;
    [[nodiscard]] bool running() const noexcept;
    Result<void> wait_until_ready(std::stop_token stop = {});
    void shutdown() noexcept;

private:
    struct Impl;
    explicit KoboldCppProcess(std::unique_ptr<Impl> impl);
    std::unique_ptr<Impl> impl_;
};

} // namespace jeeves
