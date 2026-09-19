#include <jeeves/llm/koboldcpp.hpp>

#include "tool_calls.hpp"

#include <algorithm>
#include <array>
#include <atomic>
#include <cctype>
#include <cerrno>
#include <charconv>
#include <chrono>
#include <climits>
#include <cmath>
#include <condition_variable>
#include <deque>
#include <functional>
#include <map>
#include <mutex>
#include <random>
#include <sstream>
#include <thread>
#include <vector>

#ifdef _WIN32
#define NOMINMAX
#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#else
#include <arpa/inet.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <signal.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#ifdef __linux__
#include <sys/prctl.h>
#endif
#endif

namespace jeeves {
namespace {

using namespace std::chrono_literals;
constexpr std::size_t max_header_bytes = 64 * 1024;
constexpr std::size_t max_body_bytes = 1024 * 1024;

#ifdef _WIN32
using Socket = SOCKET;
constexpr Socket invalid_socket = INVALID_SOCKET;
void close_socket(Socket socket) { if (socket != invalid_socket) closesocket(socket); }
int socket_error() { return WSAGetLastError(); }
bool interrupted(int error) { return error == WSAEINTR; }
bool timed_out(int error) { return error == WSAETIMEDOUT || error == WSAEWOULDBLOCK; }
class SocketRuntime {
public:
    SocketRuntime() { WSADATA data{}; valid_ = WSAStartup(MAKEWORD(2, 2), &data) == 0; }
    ~SocketRuntime() { if (valid_) WSACleanup(); }
    [[nodiscard]] bool valid() const noexcept { return valid_; }
private:
    bool valid_ = false;
};
SocketRuntime & socket_runtime() { static SocketRuntime value; return value; }
#else
using Socket = int;
constexpr Socket invalid_socket = -1;
void close_socket(Socket socket) { if (socket != invalid_socket) ::close(socket); }
int socket_error() { return errno; }
bool interrupted(int error) { return error == EINTR; }
bool timed_out(int error) { return error == EAGAIN || error == EWOULDBLOCK; }
#endif

bool loopback_host(const std::string & host) {
    return host == "127.0.0.1" || host == "localhost";
}

class SharedSocket {
public:
    explicit SharedSocket(Socket socket) : socket_(socket) {}
    ~SharedSocket() { close(); }
    [[nodiscard]] Socket get() const { std::lock_guard lock(mutex_); return socket_; }
    void close() {
        std::lock_guard lock(mutex_);
        if (socket_ == invalid_socket) return;
#ifdef _WIN32
        ::shutdown(socket_, SD_BOTH);
#else
        ::shutdown(socket_, SHUT_RDWR);
#endif
        close_socket(socket_);
        socket_ = invalid_socket;
    }
private:
    mutable std::mutex mutex_;
    Socket socket_ = invalid_socket;
};

Result<Socket> connect_loopback(const KoboldCppEndpoint & endpoint) {
#ifdef _WIN32
    if (!socket_runtime().valid())
        return std::unexpected(Error::unavailable("could not initialize Windows sockets"));
#endif
    if (!loopback_host(endpoint.host) || endpoint.port == 0)
        return std::unexpected(Error::configuration(
            "KoboldCpp endpoint must be a loopback host with a nonzero port"));
    Socket socket = ::socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (socket == invalid_socket)
        return std::unexpected(Error::unavailable("could not create a loopback HTTP socket"));
#ifndef _WIN32
#ifdef SO_NOSIGPIPE
    int enabled = 1;
    setsockopt(socket, SOL_SOCKET, SO_NOSIGPIPE, &enabled, sizeof(enabled));
#endif
#endif
    sockaddr_in address{};
    address.sin_family = AF_INET;
    address.sin_port = htons(endpoint.port);
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (::connect(socket, reinterpret_cast<sockaddr *>(&address), sizeof(address)) != 0) {
        close_socket(socket);
        return std::unexpected(Error::unavailable("could not connect to KoboldCpp on loopback"));
    }
#ifdef _WIN32
    DWORD timeout = 500;
    setsockopt(socket, SOL_SOCKET, SO_RCVTIMEO,
               reinterpret_cast<const char *>(&timeout), sizeof(timeout));
    setsockopt(socket, SOL_SOCKET, SO_SNDTIMEO,
               reinterpret_cast<const char *>(&timeout), sizeof(timeout));
#else
    timeval timeout{0, 500000};
    setsockopt(socket, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
    setsockopt(socket, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));
#endif
    return socket;
}

Result<void> send_all(Socket socket, std::string_view data, std::stop_token stop = {}) {
    std::size_t offset = 0;
    while (offset < data.size()) {
        if (stop.stop_requested())
            return std::unexpected(Error::cancelled("KoboldCpp request cancelled"));
#ifdef MSG_NOSIGNAL
        const int flags = MSG_NOSIGNAL;
#else
        const int flags = 0;
#endif
        const auto count = ::send(socket, data.data() + offset,
#ifdef _WIN32
                                  static_cast<int>(std::min<std::size_t>(data.size() - offset, INT_MAX)),
#else
                                  data.size() - offset,
#endif
                                  flags);
        if (count > 0) { offset += static_cast<std::size_t>(count); continue; }
        const int error = socket_error();
        if (interrupted(error) || timed_out(error)) continue;
        if (stop.stop_requested())
            return std::unexpected(Error::cancelled("KoboldCpp request cancelled"));
        return std::unexpected(Error::unavailable("KoboldCpp HTTP request write failed"));
    }
    return {};
}

Result<std::string> receive(
    Socket socket, std::stop_token stop,
    std::optional<std::chrono::steady_clock::time_point> deadline = std::nullopt) {
    std::array<char, 8192> buffer{};
    for (;;) {
        if (stop.stop_requested())
            return std::unexpected(Error::cancelled("KoboldCpp request cancelled"));
        if (deadline && std::chrono::steady_clock::now() >= *deadline)
            return std::unexpected(Error::timeout("KoboldCpp HTTP response timed out"));
        const auto count = ::recv(socket, buffer.data(), static_cast<int>(buffer.size()), 0);
        if (count > 0) return std::string(buffer.data(), static_cast<std::size_t>(count));
        if (count == 0) {
            if (stop.stop_requested())
                return std::unexpected(Error::cancelled("KoboldCpp request cancelled"));
            return std::string{};
        }
        const int error = socket_error();
        if (interrupted(error) || timed_out(error)) continue;
        if (stop.stop_requested())
            return std::unexpected(Error::cancelled("KoboldCpp request cancelled"));
        return std::unexpected(Error::unavailable("KoboldCpp HTTP response read failed"));
    }
}

std::string lowercase(std::string value) {
    std::transform(value.begin(), value.end(), value.begin(), [](unsigned char c) {
        return static_cast<char>(std::tolower(c));
    });
    return value;
}

struct HttpHead {
    int status = 0;
    bool chunked = false;
    std::optional<std::size_t> content_length;
    std::string content_type;
    std::string initial_body;
};

Result<HttpHead> read_http_head(
    Socket socket, std::stop_token stop,
    std::optional<std::chrono::steady_clock::time_point> deadline = std::nullopt) {
    std::string raw;
    std::size_t header_end = std::string::npos;
    while ((header_end = raw.find("\r\n\r\n")) == std::string::npos) {
        auto received = receive(socket, stop, deadline);
        if (!received) return std::unexpected(received.error());
        if (received->empty())
            return std::unexpected(Error::unavailable("KoboldCpp closed an incomplete HTTP response"));
        raw += *received;
        if (raw.size() > max_header_bytes)
            return std::unexpected(Error::unavailable("KoboldCpp HTTP headers exceeded 64 KiB"));
    }
    const auto status_end = raw.find("\r\n");
    const auto first_space = raw.find(' ');
    if (status_end == std::string::npos || first_space == std::string::npos || first_space > status_end)
        return std::unexpected(Error::unavailable("KoboldCpp returned a malformed HTTP status"));
    int status = 0;
    const auto status_text = std::string_view(raw).substr(first_space + 1, 3);
    const auto parsed = std::from_chars(status_text.data(), status_text.data() + status_text.size(), status);
    if (parsed.ec != std::errc{} || parsed.ptr != status_text.data() + status_text.size())
        return std::unexpected(Error::unavailable("KoboldCpp returned a malformed HTTP status"));

    HttpHead head;
    head.status = status;
    head.initial_body = raw.substr(header_end + 4);
    std::size_t position = status_end + 2;
    while (position < header_end) {
        const auto end = raw.find("\r\n", position);
        if (end == std::string::npos || end > header_end) break;
        const auto line = std::string_view(raw).substr(position, end - position);
        const auto colon = line.find(':');
        if (colon == std::string_view::npos)
            return std::unexpected(Error::unavailable("KoboldCpp returned a malformed HTTP header"));
        auto name = lowercase(std::string(line.substr(0, colon)));
        auto value = std::string(line.substr(colon + 1));
        while (!value.empty() && std::isspace(static_cast<unsigned char>(value.front())))
            value.erase(value.begin());
        const auto lower_value = lowercase(value);
        if (name == "transfer-encoding" && lower_value.find("chunked") != std::string::npos)
            head.chunked = true;
        if (name == "content-type") head.content_type = lower_value;
        if (name == "content-length") {
            std::size_t length = 0;
            const auto length_parsed = std::from_chars(value.data(), value.data() + value.size(), length);
            if (length_parsed.ec != std::errc{} || length_parsed.ptr != value.data() + value.size())
                return std::unexpected(Error::unavailable("KoboldCpp returned an invalid Content-Length"));
            head.content_length = length;
        }
        position = end + 2;
    }
    if (head.chunked) head.content_length.reset();
    return head;
}

class HttpBodyReader {
public:
    HttpBodyReader(
        std::shared_ptr<SharedSocket> socket, HttpHead head, std::stop_token stop,
        std::optional<std::chrono::steady_clock::time_point> deadline = std::nullopt)
        : socket_(std::move(socket)), raw_(std::move(head.initial_body)), chunked_(head.chunked),
          remaining_(head.content_length), stop_(stop), deadline_(deadline) {}

    Result<std::optional<std::string>> line() {
        for (;;) {
            if (const auto end = decoded_.find('\n'); end != std::string::npos) {
                auto value = decoded_.substr(0, end);
                decoded_.erase(0, end + 1);
                if (!value.empty() && value.back() == '\r') value.pop_back();
                return std::optional<std::string>(std::move(value));
            }
            if (body_done_) {
                if (decoded_.empty()) return std::optional<std::string>{};
                auto value = std::move(decoded_);
                decoded_.clear();
                if (!value.empty() && value.back() == '\r') value.pop_back();
                return std::optional<std::string>(std::move(value));
            }
            auto filled = fill();
            if (!filled) return std::unexpected(filled.error());
            if (decoded_.size() > max_body_bytes)
                return std::unexpected(Error::unavailable("KoboldCpp HTTP event exceeded 1 MiB"));
        }
    }

    Result<std::string> all(std::size_t limit) {
        std::string output;
        for (;;) {
            auto next = line();
            if (!next) return std::unexpected(next.error());
            if (!*next) return output;
            if (!output.empty()) output.push_back('\n');
            output += **next;
            if (output.size() > limit) { output.resize(limit); return output; }
        }
    }

private:
    Result<void> read_raw() {
        auto received = receive(socket_->get(), stop_, deadline_);
        if (!received) return std::unexpected(received.error());
        if (received->empty()) { transport_eof_ = true; return {}; }
        raw_ += *received;
        return {};
    }

    Result<void> require_raw(std::size_t count) {
        while (raw_.size() < count && !transport_eof_) {
            if (auto read = read_raw(); !read) return read;
        }
        if (raw_.size() < count)
            return std::unexpected(Error::unavailable("KoboldCpp returned a truncated HTTP body"));
        return {};
    }

    Result<void> fill() {
        if (stop_.stop_requested())
            return std::unexpected(Error::cancelled("KoboldCpp request cancelled"));
        if (chunked_) return fill_chunked();
        if (remaining_) {
            if (*remaining_ == 0) { body_done_ = true; return {}; }
            if (raw_.empty()) {
                if (auto read = read_raw(); !read) return read;
                if (transport_eof_)
                    return std::unexpected(Error::unavailable("KoboldCpp returned a truncated HTTP body"));
            }
            const auto count = std::min(*remaining_, raw_.size());
            decoded_.append(raw_, 0, count);
            raw_.erase(0, count);
            *remaining_ -= count;
            if (*remaining_ == 0) body_done_ = true;
            return {};
        }
        if (!raw_.empty()) { decoded_ += std::move(raw_); raw_.clear(); return {}; }
        if (transport_eof_) { body_done_ = true; return {}; }
        if (auto read = read_raw(); !read) return read;
        if (transport_eof_) body_done_ = true;
        return {};
    }

    Result<void> fill_chunked() {
        if (chunk_remaining_ == 0) {
            for (;;) {
                const auto end = raw_.find("\r\n");
                if (end != std::string::npos) {
                    auto size_text = std::string_view(raw_).substr(0, end);
                    if (const auto extension = size_text.find(';'); extension != std::string_view::npos)
                        size_text = size_text.substr(0, extension);
                    std::size_t size = 0;
                    const auto parsed = std::from_chars(size_text.data(), size_text.data() + size_text.size(), size, 16);
                    if (parsed.ec != std::errc{} || parsed.ptr != size_text.data() + size_text.size())
                        return std::unexpected(Error::unavailable("KoboldCpp returned an invalid HTTP chunk size"));
                    raw_.erase(0, end + 2);
                    if (size == 0) { body_done_ = true; return {}; }
                    chunk_remaining_ = size;
                    break;
                }
                if (transport_eof_)
                    return std::unexpected(Error::unavailable("KoboldCpp omitted the final HTTP chunk"));
                if (auto read = read_raw(); !read) return read;
                if (raw_.size() > max_header_bytes)
                    return std::unexpected(Error::unavailable("KoboldCpp HTTP chunk header exceeded 64 KiB"));
            }
        }
        if (auto ready = require_raw(chunk_remaining_ + 2); !ready) return ready;
        if (raw_.substr(chunk_remaining_, 2) != "\r\n")
            return std::unexpected(Error::unavailable("KoboldCpp returned a malformed HTTP chunk"));
        decoded_.append(raw_, 0, chunk_remaining_);
        raw_.erase(0, chunk_remaining_ + 2);
        chunk_remaining_ = 0;
        return {};
    }

    std::shared_ptr<SharedSocket> socket_;
    std::string raw_;
    std::string decoded_;
    bool chunked_ = false;
    std::optional<std::size_t> remaining_;
    std::size_t chunk_remaining_ = 0;
    bool transport_eof_ = false;
    bool body_done_ = false;
    std::stop_token stop_;
    std::optional<std::chrono::steady_clock::time_point> deadline_;
};

struct OpenResponse { std::shared_ptr<SharedSocket> socket; HttpHead head; };
using CancelAction = std::function<void()>;

Result<OpenResponse> open_http(const KoboldCppEndpoint & endpoint, std::string_view method,
                               std::string_view path, std::string_view body,
                               std::stop_token stop = {}, CancelAction on_cancel = {},
                               std::optional<std::chrono::steady_clock::time_point> deadline = std::nullopt) {
    auto connected = connect_loopback(endpoint);
    if (!connected) return std::unexpected(connected.error());
    auto socket = std::make_shared<SharedSocket>(*connected);
    std::stop_callback cancelled(stop, [socket, on_cancel = std::move(on_cancel)] {
        socket->close();
        if (on_cancel) on_cancel();
    });
    if (stop.stop_requested())
        return std::unexpected(Error::cancelled("KoboldCpp request cancelled"));
    std::ostringstream request;
    request << method << ' ' << path << " HTTP/1.1\r\nHost: 127.0.0.1:" << endpoint.port
            << "\r\nConnection: close\r\nAccept: application/json, text/event-stream\r\n";
    if (method == "POST")
        request << "Content-Type: application/json\r\nContent-Length: " << body.size() << "\r\n";
    request << "\r\n" << body;
    if (auto sent = send_all(socket->get(), request.str(), stop); !sent)
        return std::unexpected(sent.error());
    auto head = read_http_head(socket->get(), stop, deadline);
    if (!head) return std::unexpected(head.error());
    return OpenResponse{std::move(socket), std::move(*head)};
}

struct HttpResponse { int status = 0; std::string body; };

Result<HttpResponse> http_request(const KoboldCppEndpoint & endpoint, std::string_view method,
                                  std::string_view path, std::string_view body = {},
                                  std::stop_token stop = {},
                                  std::optional<std::chrono::steady_clock::time_point> deadline = std::nullopt) {
    auto opened = open_http(endpoint, method, path, body, stop, {}, deadline);
    if (!opened) return std::unexpected(opened.error());
    const auto status = opened->head.status;
    HttpBodyReader reader(opened->socket, std::move(opened->head), stop, deadline);
    auto response_body = reader.all(max_body_bytes);
    if (!response_body) return std::unexpected(response_body.error());
    return HttpResponse{status, std::move(*response_body)};
}

void abort_generation(const KoboldCppEndpoint & endpoint, const std::string & key) noexcept {
    try {
        auto connected = connect_loopback(endpoint);
        if (!connected) return;
        const auto body = json{{"genkey", key}}.dump();
        std::ostringstream request;
        request << "POST /api/extra/abort HTTP/1.1\r\nHost: 127.0.0.1:" << endpoint.port
                << "\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: "
                << body.size() << "\r\n\r\n" << body;
        (void)send_all(*connected, request.str());
        close_socket(*connected);
    } catch (...) {}
}

class GenerationGate {
public:
    Result<void> acquire(std::stop_token stop) {
        std::unique_lock lock(mutex_);
        if (!condition_.wait(lock, stop, [this] { return !busy_; }))
            return std::unexpected(Error::cancelled("KoboldCpp request cancelled while waiting"));
        busy_ = true;
        return {};
    }
    void release() noexcept {
        std::lock_guard lock(mutex_);
        busy_ = false;
        condition_.notify_one();
    }
private:
    std::mutex mutex_;
    std::condition_variable_any condition_;
    bool busy_ = false;
};

class GenerationLease {
public:
    explicit GenerationLease(std::shared_ptr<GenerationGate> gate) : gate_(std::move(gate)) {}
    ~GenerationLease() { release(); }
    GenerationLease(GenerationLease && other) noexcept : gate_(std::move(other.gate_)) {}
    GenerationLease & operator=(GenerationLease && other) noexcept {
        if (this != &other) { release(); gate_ = std::move(other.gate_); }
        return *this;
    }
    GenerationLease(const GenerationLease &) = delete;
    GenerationLease & operator=(const GenerationLease &) = delete;
    void release() noexcept { if (gate_) { gate_->release(); gate_.reset(); } }
private:
    std::shared_ptr<GenerationGate> gate_;
};

std::string role_name(Role role) {
    switch (role) {
        case Role::System: return "system";
        case Role::User: return "user";
        case Role::Assistant: return "assistant";
        case Role::Tool: return "tool";
    }
    return "user";
}

json encode_message(const Message & message) {
    json value{{"role", role_name(message.role)}, {"content", message.content}};
    if (message.tool_call_id) value["tool_call_id"] = *message.tool_call_id;
    if (!message.tool_calls.empty()) {
        value["tool_calls"] = json::array();
        for (const auto & call : message.tool_calls)
            value["tool_calls"].push_back({{"id", call.id}, {"type", "function"},
                {"function", {{"name", call.name}, {"arguments", call.arguments.dump()}}}});
    }
    return value;
}

Result<json> request_json(const ModelRequest & request, const KoboldCppEndpoint & endpoint,
                          const std::string & key) {
    if (request.stop.stop_requested())
        return std::unexpected(Error::cancelled("KoboldCpp request cancelled"));
    if (request.extra_body && !request.extra_body->is_object())
        return std::unexpected(Error::invalid_input("LLM extra_body must be an object"));
    if (request.temperature && !std::isfinite(*request.temperature))
        return std::unexpected(Error::invalid_input("LLM temperature must be finite"));
    if (request.max_tokens && *request.max_tokens == 0)
        return std::unexpected(Error::invalid_input("LLM max_tokens must be positive"));
    static constexpr std::array reserved{
        "messages", "model", "stream", "stream_options", "genkey", "temperature",
        "max_tokens", "tools", "response_format"};
    if (request.extra_body) {
        for (const auto * name : reserved)
            if (request.extra_body->contains(name))
                return std::unexpected(Error::invalid_input(
                    std::string("LLM extra_body cannot override '") + name + "'"));
    }
    json body = request.extra_body.value_or(json::object());
    body["model"] = request.model.value_or(endpoint.model);
    body["stream"] = true;
    body["stream_options"] = {{"include_usage", true}};
    body["genkey"] = key;
    body["messages"] = json::array();
    for (const auto & message : request.messages) body["messages"].push_back(encode_message(message));
    if (request.temperature) body["temperature"] = *request.temperature;
    if (request.max_tokens) body["max_tokens"] = *request.max_tokens;
    if (!request.tools.empty()) {
        body["tools"] = json::array();
        for (const auto & tool : request.tools)
            body["tools"].push_back({{"type", "function"}, {"function", {
                {"name", tool.name}, {"description", tool.description}, {"parameters", tool.parameters}}}});
    }
    if (request.response_schema)
        body["response_format"] = {{"type", "json_schema"}, {"json_schema", {
            {"name", "jeeves_output"}, {"strict", true}, {"schema", *request.response_schema}}}};
    return body;
}

ModelStopReason stop_reason(std::string value) {
    if (value == "length") return ModelStopReason::max_tokens(std::move(value));
    if (value == "tool_calls" || value == "function_call") return ModelStopReason::tool_call(std::move(value));
    if (value == "content_filter") return ModelStopReason::content_filter(std::move(value));
    if (value == "stop") return ModelStopReason::completed(std::move(value));
    return ModelStopReason::other(std::move(value));
}

struct PendingToolCall { std::string id, name, arguments; };

class SseModelStream final : public ModelStream {
public:
    SseModelStream(KoboldCppEndpoint endpoint, std::string key, std::stop_token stop,
                   std::shared_ptr<SharedSocket> socket, HttpHead head,
                   GenerationLease lease, bool parse_tagged_tools)
        : endpoint_(std::move(endpoint)), key_(std::move(key)), stop_(stop), socket_(std::move(socket)),
          reader_(socket_, std::move(head), stop_), lease_(std::move(lease)),
          parse_tagged_tools_(parse_tagged_tools) {
        cancellation_.emplace(stop_, std::function<void()>([this] { cancel(); }));
    }
    ~SseModelStream() override { finish(false); }

    std::optional<Result<ModelStreamEvent>> next() override {
        if (!events_.empty()) return pop();
        if (terminal_) return std::nullopt;
        if (stop_.stop_requested()) return fail(Error::cancelled("KoboldCpp request cancelled"));
        for (;;) {
            auto line = reader_.line();
            if (!line) return fail(line.error());
            if (!*line) {
                if (!data_.empty()) {
                    auto processed = process_event(std::exchange(data_, {}));
                    if (!processed) return fail(processed.error());
                    if (!events_.empty()) return pop();
                    if (terminal_) return std::nullopt;
                }
                if (!pending_stop_)
                    return fail(Error::unavailable("KoboldCpp SSE stream ended before a stop reason"));
                    auto finalized = finalize();
                    if (!finalized) return fail(finalized.error());
                    return events_.empty() ? std::nullopt : pop();
            }
            if ((*line)->empty()) {
                if (data_.empty()) continue;
                auto processed = process_event(std::exchange(data_, {}));
                if (!processed) return fail(processed.error());
                if (!events_.empty()) return pop();
                if (terminal_) return std::nullopt;
            } else if ((*line)->starts_with("data:")) {
                auto value = (*line)->substr(5);
                if (!value.empty() && value.front() == ' ') value.erase(value.begin());
                if (!data_.empty()) data_.push_back('\n');
                data_ += value;
                if (data_.size() > max_body_bytes)
                    return fail(Error::unavailable("KoboldCpp SSE event exceeded 1 MiB"));
            }
        }
    }

private:
    std::optional<Result<ModelStreamEvent>> pop() {
        auto event = std::move(events_.front());
        events_.pop_front();
        return Result<ModelStreamEvent>(std::move(event));
    }
    std::optional<Result<ModelStreamEvent>> fail(Error error) {
        finish(false);
        return Result<ModelStreamEvent>(std::unexpected(std::move(error)));
    }
    void cancel() noexcept {
        if (terminal_ || abort_sent_.exchange(true)) return;
        socket_->close();
        abort_generation(endpoint_, key_);
    }
    void finish(bool completed) noexcept {
        if (terminal_ && !lease_active_) return;
        if (!completed && !terminal_) cancel();
        terminal_ = true;
        cancellation_.reset();
        socket_->close();
        if (lease_active_) { lease_.release(); lease_active_ = false; }
    }

    Result<void> process_event(const std::string & data) {
        if (data == "[DONE]") {
            if (!pending_stop_)
                return std::unexpected(Error::unavailable("KoboldCpp SSE stream ended without a stop reason"));
            auto finalized = finalize();
            if (!finalized) return std::unexpected(finalized.error());
            return {};
        }
        json root;
        try { root = json::parse(data); }
        catch (const json::exception & error) {
            return std::unexpected(Error::unavailable(
                std::string("KoboldCpp returned malformed SSE JSON: ") + error.what()));
        }
        try {
            if (root.contains("usage") && root["usage"].is_object()) {
                const auto & usage = root["usage"];
                pending_usage_ = TokenUsage{usage.value("prompt_tokens", std::uint64_t{}),
                                            usage.value("completion_tokens", std::uint64_t{})};
            }
            if (!root.contains("choices") || !root["choices"].is_array())
                return std::unexpected(Error::unavailable("KoboldCpp SSE chunk has no choices array"));
            if (root["choices"].empty()) return {};
            const auto & choice = root["choices"][0];
            if (!choice.is_object() || !choice.contains("delta") || !choice["delta"].is_object())
                return std::unexpected(Error::unavailable("KoboldCpp SSE choice has no delta"));
            const auto & delta = choice["delta"];
            if (delta.contains("content") && !delta["content"].is_null()) {
                if (!delta["content"].is_string())
                    return std::unexpected(Error::unavailable("KoboldCpp SSE content delta is not text"));
                auto text = delta["content"].get<std::string>();
                if (!text.empty()) { all_content_ += text; events_.emplace_back(std::move(text)); }
            }
            if (delta.contains("tool_calls")) {
                if (!delta["tool_calls"].is_array())
                    return std::unexpected(Error::unavailable("KoboldCpp SSE tool_calls delta is not an array"));
                for (const auto & encoded : delta["tool_calls"]) {
                    if (!encoded.is_object())
                        return std::unexpected(Error::unavailable("KoboldCpp SSE tool call delta is invalid"));
                    auto & pending = tools_[encoded.value("index", std::size_t{})];
                    if (encoded.contains("id") && encoded["id"].is_string())
                        pending.id += encoded["id"].get<std::string>();
                    if (encoded.contains("function") && encoded["function"].is_object()) {
                        const auto & function = encoded["function"];
                        if (function.contains("name") && function["name"].is_string())
                            pending.name += function["name"].get<std::string>();
                        if (function.contains("arguments") && function["arguments"].is_string())
                            pending.arguments += function["arguments"].get<std::string>();
                    }
                }
            }
            if (choice.contains("finish_reason") && !choice["finish_reason"].is_null()) {
                if (!choice["finish_reason"].is_string())
                    return std::unexpected(Error::unavailable("KoboldCpp SSE finish_reason is not text"));
                pending_stop_ = stop_reason(choice["finish_reason"].get<std::string>());
            }
        } catch (const json::exception & error) {
            return std::unexpected(Error::unavailable(
                std::string("KoboldCpp returned an invalid SSE chat completion: ") + error.what()));
        }
        return {};
    }

    Result<void> enqueue_tools() {
        for (auto & [index, pending] : tools_) {
            (void)index;
            if (pending.name.empty()) continue;
            try {
                json arguments = pending.arguments.empty() ? json::object() : json::parse(pending.arguments);
                if (!arguments.is_object())
                    return std::unexpected(Error::unavailable("KoboldCpp returned non-object tool arguments"));
                events_.emplace_back(ToolCall{std::move(pending.id), std::move(pending.name),
                                              std::move(arguments)});
                emitted_tools_ = true;
            } catch (const json::exception & error) {
                return std::unexpected(Error::unavailable(
                    std::string("KoboldCpp returned malformed tool arguments: ") + error.what()));
            }
        }
        if (!emitted_tools_ && parse_tagged_tools_) {
            for (auto & call : detail::parse_tool_calls(all_content_)) {
                events_.emplace_back(std::move(call));
                emitted_tools_ = true;
            }
        }
        return {};
    }

    Result<void> finalize() {
        if (terminal_) return {};
        if (auto tools = enqueue_tools(); !tools) return tools;
        if (pending_usage_) events_.emplace_back(*pending_usage_);
        auto reason = pending_stop_.value_or(ModelStopReason::other("missing_finish_reason"));
        if (emitted_tools_) reason = ModelStopReason::tool_call(reason.detail);
        events_.emplace_back(std::move(reason));
        finish(true);
        return {};
    }

    KoboldCppEndpoint endpoint_;
    std::string key_;
    std::stop_token stop_;
    std::shared_ptr<SharedSocket> socket_;
    HttpBodyReader reader_;
    GenerationLease lease_;
    bool lease_active_ = true;
    bool parse_tagged_tools_ = false;
    bool emitted_tools_ = false;
    bool terminal_ = false;
    std::atomic_bool abort_sent_{false};
    std::optional<std::stop_callback<std::function<void()>>> cancellation_;
    std::string data_, all_content_;
    std::map<std::size_t, PendingToolCall> tools_;
    std::optional<TokenUsage> pending_usage_;
    std::optional<ModelStopReason> pending_stop_;
    std::deque<ModelStreamEvent> events_;
};

Result<std::uint16_t> private_port() {
#ifdef _WIN32
    if (!socket_runtime().valid())
        return std::unexpected(Error::unavailable("could not initialize Windows sockets"));
#endif
    Socket socket = ::socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (socket == invalid_socket)
        return std::unexpected(Error::unavailable("could not allocate a KoboldCpp port"));
    sockaddr_in address{};
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    address.sin_port = 0;
    if (::bind(socket, reinterpret_cast<sockaddr *>(&address), sizeof(address)) != 0) {
        close_socket(socket);
        return std::unexpected(Error::unavailable("could not bind a private KoboldCpp port"));
    }
#ifdef _WIN32
    int size = sizeof(address);
#else
    socklen_t size = sizeof(address);
#endif
    if (::getsockname(socket, reinterpret_cast<sockaddr *>(&address), &size) != 0) {
        close_socket(socket);
        return std::unexpected(Error::unavailable("could not inspect the private KoboldCpp port"));
    }
    const auto port = ntohs(address.sin_port);
    close_socket(socket);
    return port;
}

Result<void> validate_process_config(const KoboldCppProcessConfig & config) {
    std::error_code error;
    if (!std::filesystem::is_regular_file(config.executable, error))
        return std::unexpected(Error::configuration("KoboldCpp executable is missing: " + config.executable.string()));
    if (!std::filesystem::is_regular_file(config.model, error))
        return std::unexpected(Error::configuration("KoboldCpp model is missing: " + config.model.string()));
    if (!std::filesystem::is_regular_file(config.chat_completions_adapter, error))
        return std::unexpected(Error::configuration(
            "KoboldCpp chat-completions adapter is missing: " + config.chat_completions_adapter.string()));
    if (config.context_size < 256 || config.context_size > 524288)
        return std::unexpected(Error::configuration("KoboldCpp context size must be between 256 and 524288"));
#if defined(__APPLE__)
    if (config.backend != KoboldCppGpuBackend::Metal)
        return std::unexpected(Error::configuration("this macOS build requires the Metal KoboldCpp backend"));
#else
    if (config.backend == KoboldCppGpuBackend::Metal)
        return std::unexpected(Error::configuration("the Metal KoboldCpp backend is available only on macOS"));
#endif
    if (config.startup_timeout <= 0ms)
        return std::unexpected(Error::configuration("KoboldCpp startup timeout must be positive"));
    return {};
}

std::vector<std::string> process_arguments(const KoboldCppProcessConfig & config, std::uint16_t port) {
    std::vector<std::string> arguments{
        config.executable.string(), "--model", config.model.string(), "--host", "127.0.0.1",
        "--port", std::to_string(port), "--contextsize", std::to_string(config.context_size),
        "--gpulayers", "999", "--chatcompletionsadapter", config.chat_completions_adapter.string(),
        "--multiuser", "0", "--skiplauncher", "--quiet"};
    if (config.backend == KoboldCppGpuBackend::Cuda) arguments.push_back("--usecuda");
    if (config.backend == KoboldCppGpuBackend::Vulkan) arguments.push_back("--usevulkan");
    return arguments;
}

#ifdef _WIN32
std::wstring widen(const std::string & value) {
    if (value.empty()) return {};
    const int size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, value.data(),
                                         static_cast<int>(value.size()), nullptr, 0);
    if (size <= 0) return {};
    std::wstring result(static_cast<std::size_t>(size), L'\0');
    MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, value.data(),
                        static_cast<int>(value.size()), result.data(), size);
    return result;
}
std::wstring quote_windows(const std::string & value) {
    auto input = widen(value);
    std::wstring output = L"\"";
    std::size_t slashes = 0;
    for (wchar_t c : input) {
        if (c == L'\\') { ++slashes; continue; }
        if (c == L'\"') { output.append(slashes * 2 + 1, L'\\'); output.push_back(c); slashes = 0; continue; }
        output.append(slashes, L'\\'); slashes = 0; output.push_back(c);
    }
    output.append(slashes * 2, L'\\');
    output.push_back(L'\"');
    return output;
}
#endif

} // namespace

struct KoboldCppProvider::State {
    std::shared_ptr<GenerationGate> gate = std::make_shared<GenerationGate>();
    std::atomic_uint64_t next{0};
    std::string nonce;
    State() {
        std::random_device random;
        std::ostringstream value;
        value << std::hex << random() << random();
        nonce = value.str();
    }
};

KoboldCppProvider::KoboldCppProvider(KoboldCppEndpoint endpoint)
    : endpoint_(std::move(endpoint)), state_(std::make_shared<State>()) {}

Result<std::unique_ptr<ModelStream>> KoboldCppProvider::stream(const ModelRequest & request) {
    if (!loopback_host(endpoint_.host) || endpoint_.port == 0)
        return std::unexpected(Error::configuration("KoboldCpp provider accepts only a loopback endpoint"));
    if (auto acquired = state_->gate->acquire(request.stop); !acquired)
        return std::unexpected(acquired.error());
    GenerationLease lease(state_->gate);
    const auto key = "jeeves-" + state_->nonce + "-" + std::to_string(state_->next.fetch_add(1));
    auto body = request_json(request, endpoint_, key);
    if (!body) return std::unexpected(body.error());
    const auto encoded_body = body->dump();
    const auto busy_deadline = std::chrono::steady_clock::now() + 5s;
    Result<OpenResponse> opened = std::unexpected(Error::unavailable("KoboldCpp request not sent"));
    for (;;) {
        opened = open_http(endpoint_, "POST", "/v1/chat/completions", encoded_body, request.stop,
                           [endpoint = endpoint_, key] { abort_generation(endpoint, key); });
        if (!opened) return std::unexpected(opened.error());
        if (opened->head.status >= 200 && opened->head.status < 300) break;
        const auto status = opened->head.status;
        HttpBodyReader reader(opened->socket, std::move(opened->head), request.stop);
        auto error_body = reader.all(max_body_bytes);
        const bool single_user_busy = error_body &&
            error_body->find("Server is busy") != std::string::npos;
        if (status == 503 && single_user_busy &&
            std::chrono::steady_clock::now() < busy_deadline) {
            std::mutex mutex;
            std::condition_variable_any delay;
            std::unique_lock lock(mutex);
            (void)delay.wait_for(lock, request.stop, 50ms, [] { return false; });
            if (request.stop.stop_requested())
                return std::unexpected(Error::cancelled("KoboldCpp request cancelled"));
            continue;
        }
        std::string message = "KoboldCpp returned HTTP " + std::to_string(status);
        if (error_body && !error_body->empty()) message += ": " + error_body->substr(0, 1024);
        const auto kind = status >= 500 || status == 408 || status == 429
            ? ErrorKind::Unavailable : ErrorKind::InvalidInput;
        return std::unexpected(Error(kind, std::move(message)));
    }
    if (opened->head.content_type.find("text/event-stream") == std::string::npos)
        return std::unexpected(Error::unavailable("KoboldCpp streaming response is not text/event-stream"));
    return std::unique_ptr<ModelStream>(std::make_unique<SseModelStream>(
        endpoint_, key, request.stop, std::move(opened->socket), std::move(opened->head),
        std::move(lease), !request.tools.empty()));
}

struct KoboldCppProcess::Impl {
    KoboldCppProcessConfig config;
    KoboldCppEndpoint endpoint;
    std::atomic_bool stopped{false};
#ifdef _WIN32
    PROCESS_INFORMATION process{};
    HANDLE job = nullptr;
#else
    pid_t pid = -1;
#endif
};

KoboldCppProcess::KoboldCppProcess(std::unique_ptr<Impl> impl) : impl_(std::move(impl)) {}
KoboldCppProcess::~KoboldCppProcess() { shutdown(); }

Result<std::shared_ptr<KoboldCppProcess>> KoboldCppProcess::launch(KoboldCppProcessConfig config) {
    if (auto valid = validate_process_config(config); !valid) return std::unexpected(valid.error());
    if (!config.diagnostics_file.empty()) {
        std::error_code error;
        if (!config.diagnostics_file.parent_path().empty())
            std::filesystem::create_directories(config.diagnostics_file.parent_path(), error);
        if (error)
            return std::unexpected(Error::configuration(
                "could not create KoboldCpp diagnostics directory: " + error.message()));
    }
    // A selected ephemeral port cannot stay reserved while an unrelated child
    // binds it. Retry only children that exit immediately, which covers that
    // narrow race without hiding readiness or model-loading failures.
    for (int attempt = 0; attempt < 3; ++attempt) {
        auto selected = private_port();
        if (!selected) return std::unexpected(selected.error());
        auto impl = std::make_unique<Impl>();
        impl->config = config;
        impl->endpoint = {"127.0.0.1", *selected, "local"};
        auto arguments = process_arguments(impl->config, *selected);
#ifdef _WIN32
        std::wstring command;
        for (const auto & argument : arguments) {
            if (!command.empty()) command.push_back(L' ');
            command += quote_windows(argument);
        }
        SECURITY_ATTRIBUTES inherited{};
        inherited.nLength = sizeof(inherited);
        inherited.bInheritHandle = TRUE;
        HANDLE log = INVALID_HANDLE_VALUE;
        HANDLE input = INVALID_HANDLE_VALUE;
        if (!impl->config.diagnostics_file.empty())
            log = CreateFileW(impl->config.diagnostics_file.wstring().c_str(), GENERIC_WRITE,
                              FILE_SHARE_READ | FILE_SHARE_WRITE, &inherited, CREATE_ALWAYS,
                              FILE_ATTRIBUTE_NORMAL, nullptr);
        if (!impl->config.diagnostics_file.empty() && log == INVALID_HANDLE_VALUE)
            return std::unexpected(Error::configuration("could not open KoboldCpp diagnostics file"));
        STARTUPINFOW startup{};
        startup.cb = sizeof(startup);
        startup.dwFlags = STARTF_USESHOWWINDOW;
        startup.wShowWindow = SW_HIDE;
        if (log != INVALID_HANDLE_VALUE) {
            input = CreateFileW(L"NUL", GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                &inherited, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
            startup.dwFlags |= STARTF_USESTDHANDLES;
            startup.hStdOutput = log; startup.hStdError = log; startup.hStdInput = input;
        }
        impl->job = CreateJobObjectW(nullptr, nullptr);
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if (!impl->job || !SetInformationJobObject(impl->job, JobObjectExtendedLimitInformation,
                                                    &limits, sizeof(limits))) {
            if (impl->job) CloseHandle(impl->job);
            if (log != INVALID_HANDLE_VALUE) CloseHandle(log);
            if (input != INVALID_HANDLE_VALUE) CloseHandle(input);
            return std::unexpected(Error::unavailable("could not configure KoboldCpp process job"));
        }
        const BOOL created = CreateProcessW(nullptr, command.data(), nullptr, nullptr,
                                            log != INVALID_HANDLE_VALUE,
                                            CREATE_NO_WINDOW | CREATE_SUSPENDED, nullptr, nullptr,
                                            &startup, &impl->process);
        if (log != INVALID_HANDLE_VALUE) CloseHandle(log);
        if (input != INVALID_HANDLE_VALUE) CloseHandle(input);
        if (!created || !AssignProcessToJobObject(impl->job, impl->process.hProcess)) {
            if (created) {
                TerminateProcess(impl->process.hProcess, 1);
                CloseHandle(impl->process.hThread);
                CloseHandle(impl->process.hProcess);
            }
            CloseHandle(impl->job);
            return std::unexpected(Error::unavailable("could not launch and own KoboldCpp"));
        }
        ResumeThread(impl->process.hThread);
#else
        std::vector<char *> argv;
        for (auto & argument : arguments) argv.push_back(argument.data());
        argv.push_back(nullptr);
        const pid_t pid = fork();
        if (pid < 0) return std::unexpected(Error::unavailable("could not fork KoboldCpp"));
        if (pid == 0) {
            (void)setpgid(0, 0);
#ifdef __linux__
            (void)prctl(PR_SET_PDEATHSIG, SIGTERM);
            if (getppid() == 1) _exit(125);
#endif
            if (!impl->config.diagnostics_file.empty()) {
                const int log = ::open(impl->config.diagnostics_file.c_str(), O_WRONLY | O_CREAT | O_TRUNC, 0600);
                if (log < 0) _exit(126);
                dup2(log, STDOUT_FILENO); dup2(log, STDERR_FILENO); close(log);
            }
            execv(argv[0], argv.data());
            _exit(127);
        }
        (void)setpgid(pid, pid);
        impl->pid = pid;
#endif
        auto process = std::shared_ptr<KoboldCppProcess>(new KoboldCppProcess(std::move(impl)));
        std::this_thread::sleep_for(25ms);
        if (process->running()) return process;
        process->shutdown();
    }
    return std::unexpected(Error::unavailable(
        "KoboldCpp exited immediately after three launch attempts; see " +
        config.diagnostics_file.string()));
}

const KoboldCppEndpoint & KoboldCppProcess::endpoint() const noexcept { return impl_->endpoint; }
const std::filesystem::path & KoboldCppProcess::diagnostics_file() const noexcept {
    return impl_->config.diagnostics_file;
}

bool KoboldCppProcess::running() const noexcept {
    if (!impl_ || impl_->stopped.load()) return false;
#ifdef _WIN32
    return impl_->process.hProcess && WaitForSingleObject(impl_->process.hProcess, 0) == WAIT_TIMEOUT;
#else
    if (impl_->pid <= 0) return false;
    int status = 0;
    const auto result = waitpid(impl_->pid, &status, WNOHANG);
    if (result == impl_->pid) impl_->pid = -1;
    return result == 0;
#endif
}

Result<void> KoboldCppProcess::wait_until_ready(std::stop_token stop) {
    const auto deadline = std::chrono::steady_clock::now() + impl_->config.startup_timeout;
    while (std::chrono::steady_clock::now() < deadline) {
        if (stop.stop_requested())
            return std::unexpected(Error::cancelled("KoboldCpp startup cancelled"));
        if (!running())
            return std::unexpected(Error::unavailable(
                "KoboldCpp exited before becoming ready; see " + diagnostics_file().string()));
        const auto probe_deadline = std::min(deadline, std::chrono::steady_clock::now() + 1s);
        auto version_response = http_request(impl_->endpoint, "GET", "/api/extra/version", {},
                                             stop, probe_deadline);
        if (version_response && version_response->status >= 200 && version_response->status < 300) {
            try {
                const auto version = json::parse(version_response->body);
                if (version.value("result", std::string{}) != "KoboldCpp" || !version.value("llm", false))
                    return std::unexpected(Error::configuration(
                        "loopback process is not a model-ready KoboldCpp server"));
                if (!impl_->config.required_version.empty() &&
                    version.value("version", std::string{}) != impl_->config.required_version)
                    return std::unexpected(Error::configuration(
                        "bundled KoboldCpp version does not match the manifest"));
                auto model = http_request(impl_->endpoint, "GET", "/api/v1/model", {}, stop,
                                          probe_deadline);
                auto context = http_request(impl_->endpoint, "GET",
                    "/api/extra/true_max_context_length", {}, stop, probe_deadline);
                if (model && context && model->status >= 200 && model->status < 300 &&
                    context->status >= 200 && context->status < 300) {
                    const auto model_json = json::parse(model->body);
                    const auto context_json = json::parse(context->body);
                    if (model_json.value("result", std::string{}).empty())
                        return std::unexpected(Error::configuration("KoboldCpp reported no loaded model"));
                    if (context_json.value("value", std::uint32_t{}) != impl_->config.context_size)
                        return std::unexpected(Error::configuration(
                            "KoboldCpp loaded a different context size than requested"));
                    return {};
                }
            } catch (const json::exception & error) {
                return std::unexpected(Error::unavailable(
                    std::string("KoboldCpp readiness response was malformed: ") + error.what()));
            }
        }
        std::this_thread::sleep_for(50ms);
    }
    return std::unexpected(Error::timeout(
        "KoboldCpp model startup timed out; see " + diagnostics_file().string()));
}

void KoboldCppProcess::shutdown() noexcept {
    if (!impl_ || impl_->stopped.exchange(true)) return;
#ifdef _WIN32
    if (impl_->process.hProcess) {
        if (WaitForSingleObject(impl_->process.hProcess, 0) == WAIT_TIMEOUT) {
            TerminateJobObject(impl_->job, 0);
            WaitForSingleObject(impl_->process.hProcess, 2000);
        }
        CloseHandle(impl_->process.hThread);
        CloseHandle(impl_->process.hProcess);
        impl_->process = {};
    }
    if (impl_->job) { CloseHandle(impl_->job); impl_->job = nullptr; }
#else
    if (impl_->pid > 0) {
        int status = 0;
        if (waitpid(impl_->pid, &status, WNOHANG) == 0) {
            kill(-impl_->pid, SIGTERM);
            const auto deadline = std::chrono::steady_clock::now() + 2s;
            while (std::chrono::steady_clock::now() < deadline) {
                if (waitpid(impl_->pid, &status, WNOHANG) == impl_->pid) break;
                std::this_thread::sleep_for(20ms);
            }
            if (waitpid(impl_->pid, &status, WNOHANG) == 0) {
                kill(-impl_->pid, SIGKILL);
                (void)waitpid(impl_->pid, &status, 0);
            }
        }
        impl_->pid = -1;
    }
#endif
}

} // namespace jeeves
