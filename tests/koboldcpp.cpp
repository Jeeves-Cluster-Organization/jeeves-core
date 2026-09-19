#include <jeeves/jeeves.hpp>
#include <gtest/gtest.h>

#include <array>
#include <atomic>
#include <condition_variable>
#include <future>
#include <fstream>
#include <mutex>
#include <sstream>
#include <thread>

#ifdef _WIN32
#define NOMINMAX
#include <winsock2.h>
#include <ws2tcpip.h>
#else
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>
#endif

using namespace jeeves;
using namespace std::chrono_literals;

namespace {
#ifdef _WIN32
using TestSocket = SOCKET;
constexpr TestSocket bad_socket = INVALID_SOCKET;
void close_test_socket(TestSocket value) { closesocket(value); }
struct Winsock {
    Winsock() { WSADATA data{}; WSAStartup(MAKEWORD(2, 2), &data); }
    ~Winsock() { WSACleanup(); }
};
#else
using TestSocket = int;
constexpr TestSocket bad_socket = -1;
void close_test_socket(TestSocket value) { close(value); }
#endif

std::string read_request(TestSocket socket) {
    std::string request;
    std::array<char, 4096> buffer{};
    std::size_t expected = 0;
    for (;;) {
        const auto count = recv(socket, buffer.data(), static_cast<int>(buffer.size()), 0);
        if (count <= 0) return request;
        request.append(buffer.data(), static_cast<std::size_t>(count));
        const auto header_end = request.find("\r\n\r\n");
        if (header_end == std::string::npos) continue;
        if (expected == 0) {
            const auto length = request.find("Content-Length:");
            expected = header_end + 4;
            if (length != std::string::npos)
                expected += static_cast<std::size_t>(std::stoul(request.substr(length + 15)));
        }
        if (request.size() >= expected) return request;
    }
}

void send_response(TestSocket socket, int status, std::string body,
                   std::string content_type = "application/json") {
    std::string reason = status == 200 ? "OK" : "Error";
    const auto response = "HTTP/1.1 " + std::to_string(status) + " " + reason +
        "\r\nContent-Type: " + content_type + "\r\nContent-Length: " + std::to_string(body.size()) +
        "\r\nConnection: close\r\n\r\n" + body;
    std::size_t sent = 0;
    while (sent < response.size()) {
        const auto count = send(socket, response.data() + sent, static_cast<int>(response.size() - sent), 0);
        if (count <= 0) return;
        sent += static_cast<std::size_t>(count);
    }
}

std::string sse(std::initializer_list<json> chunks) {
    std::string body;
    for (const auto &chunk : chunks)
        body += "data: " + chunk.dump() + "\n\n";
    body += "data: [DONE]\n\n";
    return body;
}

void send_sse(TestSocket socket, std::initializer_list<json> chunks) {
    send_response(socket, 200, sse(chunks), "text/event-stream");
}

void send_chunked_sse(TestSocket socket, const std::string &body, std::size_t fragment) {
    const std::string head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n"
                             "Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
    send(socket, head.data(), static_cast<int>(head.size()), 0);
    for (std::size_t offset = 0; offset < body.size(); offset += fragment) {
        const auto count = std::min(fragment, body.size() - offset);
        std::ostringstream size;
        size << std::hex << count << "\r\n";
        const auto prefix = size.str();
        send(socket, prefix.data(), static_cast<int>(prefix.size()), 0);
        send(socket, body.data() + offset, static_cast<int>(count), 0);
        send(socket, "\r\n", 2, 0);
    }
    send(socket, "0\r\n\r\n", 5, 0);
}

class FixtureServer {
public:
    using Handler = std::function<void(TestSocket, const std::string &)>;
    explicit FixtureServer(Handler handler, int connections = 1)
        : handler_(std::move(handler)), connections_(connections) {
#ifdef _WIN32
        static Winsock winsock;
#endif
        listener_ = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
        if (listener_ == bad_socket) throw std::runtime_error("fixture socket failed");
        int reuse = 1;
#ifdef _WIN32
        setsockopt(listener_, SOL_SOCKET, SO_REUSEADDR, reinterpret_cast<const char *>(&reuse), sizeof(reuse));
#else
        setsockopt(listener_, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse));
#endif
        sockaddr_in address{};
        address.sin_family = AF_INET;
        address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
        if (bind(listener_, reinterpret_cast<sockaddr *>(&address), sizeof(address)) != 0 ||
            listen(listener_, 4) != 0)
            throw std::runtime_error("fixture bind failed");
#ifdef _WIN32
        int size = sizeof(address);
#else
        socklen_t size = sizeof(address);
#endif
        if (getsockname(listener_, reinterpret_cast<sockaddr *>(&address), &size) != 0)
            throw std::runtime_error("fixture getsockname failed");
        endpoint_ = {"127.0.0.1", ntohs(address.sin_port), "fixture-model"};
        worker_ = std::thread([this] {
            sockaddr_in peer{};
#ifdef _WIN32
            int size = sizeof(peer);
#else
            socklen_t size = sizeof(peer);
#endif
            for (int index = 0; index < connections_; ++index) {
                auto client = accept(listener_, reinterpret_cast<sockaddr *>(&peer), &size);
                if (client == bad_socket) return;
                const auto request = read_request(client);
                handler_(client, request);
                close_test_socket(client);
            }
        });
    }
    ~FixtureServer() {
#ifdef _WIN32
        shutdown(listener_, SD_BOTH);
#else
        shutdown(listener_, SHUT_RDWR);
#endif
        close_test_socket(listener_);
        if (worker_.joinable()) worker_.join();
    }
    const KoboldCppEndpoint & endpoint() const { return endpoint_; }
private:
    Handler handler_;
    TestSocket listener_ = bad_socket;
    KoboldCppEndpoint endpoint_;
    std::thread worker_;
    int connections_ = 1;
};

json body_from_request(const std::string & request) {
    return json::parse(request.substr(request.find("\r\n\r\n") + 4));
}

std::vector<ModelStreamEvent> collect(ModelStream & stream) {
    std::vector<ModelStreamEvent> events;
    while (auto event = stream.next()) {
        if (!*event) throw std::runtime_error(event->error().message());
        events.push_back(std::move(**event));
    }
    return events;
}
} // namespace

TEST(KoboldCppProvider, SerializesOpenAiChatRequestAndParsesResponse) {
    std::promise<json> captured;
    FixtureServer server([&](TestSocket socket, const std::string & request) {
        captured.set_value(body_from_request(request));
        send_sse(socket, {
            json{{"choices", {{{"delta", {{"role", "assistant"}}}, {"finish_reason", nullptr}}}}},
            json{{"choices", {{{"delta", {{"content", "hel"}}}, {"finish_reason", nullptr}}}}},
            json{{"choices", {{{"delta", {{"content", "lo"}}}, {"finish_reason", nullptr}}}}},
            json{{"choices", {{{"delta", json::object()}, {"finish_reason", "stop"}}}}},
            json{{"choices", json::array()},
                 {"usage", {{"prompt_tokens", 12}, {"completion_tokens", 3}}}}
        });
    });
    KoboldCppProvider provider(server.endpoint());
    ModelRequest request;
    request.messages = {Message::system("rules"), Message::user("question")};
    request.temperature = 0.25;
    request.max_tokens = 77;
    request.model = "role-name";
    request.response_schema = json{{"type", "object"}};
    request.extra_body = json{{"top_p", 0.9}, {"chat_template_kwargs", {{"enable_thinking", false}}}};
    auto stream = provider.stream(request);
    ASSERT_TRUE(stream) << stream.error();
    const auto sent = captured.get_future().get();
    EXPECT_EQ(sent["model"], "role-name");
    EXPECT_EQ(sent["messages"][0], (json{{"role", "system"}, {"content", "rules"}}));
    EXPECT_EQ(sent["messages"][1], (json{{"role", "user"}, {"content", "question"}}));
    EXPECT_EQ(sent["temperature"], 0.25);
    EXPECT_EQ(sent["max_tokens"], 77);
    EXPECT_EQ(sent["stream"], true);
    EXPECT_EQ(sent["stream_options"]["include_usage"], true);
    EXPECT_TRUE(sent["genkey"].get<std::string>().starts_with("jeeves-"));
    EXPECT_EQ(sent["top_p"], 0.9);
    EXPECT_EQ(sent["response_format"]["json_schema"]["schema"], *request.response_schema);
    auto events = collect(**stream);
    ASSERT_EQ(events.size(), 4);
    EXPECT_EQ(std::get<std::string>(events[0].value), "hel");
    EXPECT_EQ(std::get<std::string>(events[1].value), "lo");
    EXPECT_EQ(std::get<TokenUsage>(events[2].value), (TokenUsage{12, 3}));
    EXPECT_EQ(std::get<ModelStopReason>(events[3].value), ModelStopReason::completed("stop"));
}

TEST(KoboldCppProvider, RejectsMalformedAndNonSuccessReplies) {
    {
        FixtureServer server([](TestSocket socket, const std::string &) {
            send_response(socket, 200, "data: not-json\n\ndata: [DONE]\n\n", "text/event-stream");
        });
        auto result = KoboldCppProvider(server.endpoint()).stream(ModelRequest{});
        ASSERT_TRUE(result);
        auto event = (*result)->next();
        ASSERT_TRUE(event);
        ASSERT_FALSE(*event);
        EXPECT_EQ(event->error().kind(), ErrorKind::Unavailable);
        EXPECT_NE(event->error().message().find("malformed SSE JSON"), std::string::npos);
    }
    {
        FixtureServer server([](TestSocket socket, const std::string &) {
            send_response(socket, 503, R"({"error":"loading"})");
        });
        auto result = KoboldCppProvider(server.endpoint()).stream(ModelRequest{});
        ASSERT_FALSE(result);
        EXPECT_EQ(result.error().kind(), ErrorKind::Unavailable);
        EXPECT_NE(result.error().message().find("HTTP 503"), std::string::npos);
    }
    {
        FixtureServer server([](TestSocket socket, const std::string &) {
            send_response(socket, 400, R"({"error":"bad request"})");
        });
        auto result = KoboldCppProvider(server.endpoint()).stream(ModelRequest{});
        ASSERT_FALSE(result);
        EXPECT_EQ(result.error().kind(), ErrorKind::InvalidInput);
    }
}

TEST(KoboldCppProvider, RetriesTransientSingleUserBusyResponse) {
    std::atomic_int calls{0};
    FixtureServer server([&](TestSocket socket, const std::string &) {
        if (calls.fetch_add(1) == 0) {
            send_response(socket, 503,
                          R"({"detail":{"msg":"Server is busy; please try again later."}})");
            return;
        }
        send_sse(socket, {
            json{{"choices", {{{"delta", {{"content", "ready"}}},
                                {"finish_reason", nullptr}}}}},
            json{{"choices", {{{"delta", json::object()}, {"finish_reason", "stop"}}}}}
        });
    }, 2);
    auto stream = KoboldCppProvider(server.endpoint()).stream(ModelRequest{});
    ASSERT_TRUE(stream) << stream.error();
    auto events = collect(**stream);
    ASSERT_EQ(events.size(), 2);
    EXPECT_EQ(std::get<std::string>(events[0].value), "ready");
    EXPECT_EQ(calls, 2);
}

TEST(KoboldCppProvider, ValidatesEndpointBodyAndPreCancelledRequestBeforeConnecting) {
    ModelRequest request;
    request.extra_body = json::array();
    auto result = KoboldCppProvider({"127.0.0.1", 9, "local"}).stream(request);
    ASSERT_FALSE(result);
    EXPECT_EQ(result.error().kind(), ErrorKind::InvalidInput);
    request.extra_body = json{{"messages", json::array()}};
    result = KoboldCppProvider({"127.0.0.1", 9, "local"}).stream(request);
    ASSERT_FALSE(result);
    EXPECT_EQ(result.error().kind(), ErrorKind::InvalidInput);
    request.extra_body.reset();
    std::stop_source stop;
    stop.request_stop();
    request.stop = stop.get_token();
    result = KoboldCppProvider({"127.0.0.1", 9, "local"}).stream(request);
    ASSERT_FALSE(result);
    EXPECT_EQ(result.error().kind(), ErrorKind::Cancelled);
    result = KoboldCppProvider({"0.0.0.0", 5001, "local"}).stream(ModelRequest{});
    ASSERT_FALSE(result);
    EXPECT_EQ(result.error().kind(), ErrorKind::Configuration);
}

TEST(KoboldCppProvider, CancellationClosesGenerationAndPostsAbort) {
    std::promise<void> generation_received;
    std::promise<json> abort_received;
    FixtureServer server([&](TestSocket socket, const std::string & request) {
        if (request.starts_with("POST /api/extra/abort ")) {
            abort_received.set_value(body_from_request(request));
            send_response(socket, 200, R"({"success":"true"})");
            return;
        }
        const auto generated = body_from_request(request);
        generation_received.set_value();
        const std::string head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n"
                                 "Connection: close\r\n\r\n";
        send(socket, head.data(), static_cast<int>(head.size()), 0);
        std::array<char, 64> buffer{};
        while (recv(socket, buffer.data(), static_cast<int>(buffer.size()), 0) > 0) {}
        (void)generated;
    }, 2);
    const auto endpoint = server.endpoint();
    KoboldCppProvider provider(endpoint);
    std::stop_source stop;
    ModelRequest request;
    request.stop = stop.get_token();
    auto stream = provider.stream(request);
    ASSERT_TRUE(stream) << stream.error();
    auto future = std::async(std::launch::async, [&] { return (*stream)->next(); });
    generation_received.get_future().wait();
    stop.request_stop();
    auto result = future.get();
    ASSERT_TRUE(result);
    ASSERT_FALSE(*result);
    EXPECT_EQ(result->error().kind(), ErrorKind::Cancelled);
    const auto abort = abort_received.get_future().get();
    EXPECT_TRUE(abort["genkey"].get<std::string>().starts_with("jeeves-"));
}

TEST(KoboldCppProvider, ReassemblesStreamedToolCallsBeforeStop) {
    FixtureServer server([](TestSocket socket, const std::string &) {
        send_sse(socket, {
            json{{"choices", {{{"delta", {{"tool_calls", {{{"index", 0}, {"id", "call-1"},
                {"function", {{"name", "lookup"}, {"arguments", ""}}}}}}}}, {"finish_reason", nullptr}}}}},
            json{{"choices", {{{"delta", {{"tool_calls", {{{"index", 0},
                {"function", {{"arguments", "{\"id\":"}}}}}}}}, {"finish_reason", nullptr}}}}},
            json{{"choices", {{{"delta", {{"tool_calls", {{{"index", 0},
                {"function", {{"arguments", "7}"}}}}}}}}, {"finish_reason", nullptr}}}}},
            json{{"choices", {{{"delta", json::object()}, {"finish_reason", "tool_calls"}}}}}
        });
    });
    ModelRequest request;
    request.tools.push_back(ToolSpec{"lookup", "lookup", json{{"type", "object"}}});
    auto stream = KoboldCppProvider(server.endpoint()).stream(request);
    ASSERT_TRUE(stream) << stream.error();
    auto events = collect(**stream);
    ASSERT_EQ(events.size(), 2);
    const auto &call = std::get<ToolCall>(events[0].value);
    EXPECT_EQ(call.id, "call-1");
    EXPECT_EQ(call.name, "lookup");
    EXPECT_EQ(call.arguments, (json{{"id", 7}}));
    EXPECT_EQ(std::get<ModelStopReason>(events[1].value),
              ModelStopReason::tool_call("tool_calls"));
}

TEST(KoboldCppProvider, ParsesFragmentedChunkedSse) {
    FixtureServer server([](TestSocket socket, const std::string &) {
        const auto body = sse({
            json{{"choices", {{{"delta", {{"content", "fragmented"}}},
                                {"finish_reason", nullptr}}}}},
            json{{"choices", {{{"delta", json::object()}, {"finish_reason", "stop"}}}}},
            json{{"choices", json::array()},
                 {"usage", {{"prompt_tokens", 4}, {"completion_tokens", 1}}}}
        });
        send_chunked_sse(socket, body, 7);
    });
    auto stream = KoboldCppProvider(server.endpoint()).stream(ModelRequest{});
    ASSERT_TRUE(stream) << stream.error();
    auto events = collect(**stream);
    ASSERT_EQ(events.size(), 3);
    EXPECT_EQ(std::get<std::string>(events[0].value), "fragmented");
    EXPECT_EQ(std::get<TokenUsage>(events[1].value), (TokenUsage{4, 1}));
    EXPECT_EQ(std::get<ModelStopReason>(events[2].value), ModelStopReason::completed("stop"));
}

TEST(KoboldCppProvider, CancelsWhileWaitingForSingleGenerationLease) {
    std::promise<void> first_received;
    FixtureServer server([&](TestSocket socket, const std::string &request) {
        if (request.starts_with("POST /api/extra/abort ")) {
            send_response(socket, 200, R"({"success":"true"})");
            return;
        }
        first_received.set_value();
        const std::string head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n"
                                 "Connection: close\r\n\r\n";
        send(socket, head.data(), static_cast<int>(head.size()), 0);
        std::array<char, 64> buffer{};
        while (recv(socket, buffer.data(), static_cast<int>(buffer.size()), 0) > 0) {}
    }, 2);
    KoboldCppProvider provider(server.endpoint());
    auto first = provider.stream(ModelRequest{});
    ASSERT_TRUE(first) << first.error();
    first_received.get_future().wait();
    std::stop_source stop;
    ModelRequest queued;
    queued.stop = stop.get_token();
    auto waiting = std::async(std::launch::async, [&] { return provider.stream(queued); });
    std::this_thread::sleep_for(10ms);
    stop.request_stop();
    auto result = waiting.get();
    ASSERT_FALSE(result);
    EXPECT_EQ(result.error().kind(), ErrorKind::Cancelled);
    first->reset();
}

TEST(KoboldCppProvider, StructuredOutputStillUsesWorkflowValidator) {
    FixtureServer server([](TestSocket socket, const std::string &) {
        send_sse(socket, {
            json{{"choices", {{{"delta", {{"content", R"({"ready":false})"}}},
                                {"finish_reason", nullptr}}}}},
            json{{"choices", {{{"delta", json::object()}, {"finish_reason", "stop"}}}}}
        });
    });
    auto action = LlmAction::structured(Prompt::text("reply"), json{{"type", "object"}},
        [](const json & value) -> Result<void> {
            if (value != json{{"ready", true}})
                return std::unexpected(Error::invalid_input("not ready"));
            return {};
        });
    auto workflow = Workflow::builder("validation").stage(Stage::llm("check", std::move(action))).build();
    ASSERT_TRUE(workflow);
    auto registered = Engine::builder().llm(std::make_shared<KoboldCppProvider>(server.endpoint()))
                                      .workflow(std::move(*workflow));
    ASSERT_TRUE(registered);
    auto engine = registered->build();
    ASSERT_TRUE(engine);
    auto outcome = engine->run("validation", json::object());
    ASSERT_TRUE(outcome);
    EXPECT_EQ((*outcome)->kind(), OutcomeKind::Failed);
    ASSERT_TRUE((*outcome)->error());
    EXPECT_EQ((*outcome)->error()->message(), "not ready");
}

TEST(KoboldCppProcess, LaunchesPinnedGpuOnlyChildAndVerifiesReadiness) {
    const auto unique = std::chrono::steady_clock::now().time_since_epoch().count();
    const auto root = std::filesystem::temp_directory_path() /
        ("jeeves-koboldcpp-process-test-" + std::to_string(unique));
    std::filesystem::create_directories(root);
    const auto model = root / "model.gguf";
    const auto adapter = root / "adapter.json";
    const auto diagnostics = root / "koboldcpp.log";
    { std::ofstream output(model); output << "fixture model"; }
    { std::ofstream output(adapter); output << "{}"; }
    KoboldCppProcessConfig config;
    config.executable = KOBOLDCPP_PROCESS_FIXTURE;
    config.model = model;
    config.chat_completions_adapter = adapter;
    config.diagnostics_file = diagnostics;
#ifdef __APPLE__
    config.backend = KoboldCppGpuBackend::Metal;
#else
    config.backend = KoboldCppGpuBackend::Vulkan;
#endif
    config.context_size = 4096;
    config.required_version = "fixture-1";
    config.startup_timeout = 5s;
    auto process = KoboldCppProcess::launch(config);
    ASSERT_TRUE(process) << process.error();
    auto ready = (*process)->wait_until_ready();
    ASSERT_TRUE(ready) << ready.error();
    EXPECT_TRUE((*process)->running());
    (*process)->shutdown();
    EXPECT_FALSE((*process)->running());
    {
        std::ifstream log(diagnostics);
        const std::string arguments((std::istreambuf_iterator<char>(log)), {});
        EXPECT_NE(arguments.find("arg:--skiplauncher"), std::string::npos);
        EXPECT_NE(arguments.find("arg:--gpulayers\narg:999"), std::string::npos);
        EXPECT_NE(arguments.find("arg:--multiuser\narg:0"), std::string::npos);
#ifndef __APPLE__
        EXPECT_NE(arguments.find("arg:--usevulkan"), std::string::npos);
#endif
    }
    config.required_version = "not-the-fixture-version";
    auto mismatched = KoboldCppProcess::launch(config);
    ASSERT_TRUE(mismatched) << mismatched.error();
    auto rejected = (*mismatched)->wait_until_ready();
    ASSERT_FALSE(rejected);
    EXPECT_EQ(rejected.error().kind(), ErrorKind::Configuration);
    EXPECT_NE(rejected.error().message().find("version"), std::string::npos);
    (*mismatched)->shutdown();
    std::error_code cleanup_error;
    std::filesystem::remove_all(root, cleanup_error);
}
