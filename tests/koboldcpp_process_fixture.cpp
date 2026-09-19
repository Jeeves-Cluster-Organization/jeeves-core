#include <atomic>
#include <charconv>
#include <csignal>
#include <cstdint>
#include <iostream>
#include <string>

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

namespace {
std::atomic_bool running{true};
void stop(int) { running = false; }

#ifdef _WIN32
using Socket = SOCKET;
constexpr Socket invalid_socket = INVALID_SOCKET;
void close_socket(Socket value) { closesocket(value); }
#else
using Socket = int;
constexpr Socket invalid_socket = -1;
void close_socket(Socket value) { close(value); }
#endif

void reply(Socket client, std::string body) {
    const auto response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: " +
        std::to_string(body.size()) + "\r\nConnection: close\r\n\r\n" + body;
    std::size_t offset = 0;
    while (offset < response.size()) {
        const auto sent = send(client, response.data() + offset,
                               static_cast<int>(response.size() - offset), 0);
        if (sent <= 0) break;
        offset += static_cast<std::size_t>(sent);
    }
}
}

int main(int argc, char **argv) {
#ifdef _WIN32
    WSADATA data{};
    if (WSAStartup(MAKEWORD(2, 2), &data) != 0) return 2;
#endif
    std::uint16_t port = 0;
    std::uint32_t context = 0;
    for (int index = 1; index < argc; ++index) {
        std::cout << "arg:" << argv[index] << '\n';
        const std::string argument = argv[index];
        if ((argument == "--port" || argument == "--contextsize") && index + 1 < argc) {
            unsigned value = 0;
            const std::string text = argv[++index];
            std::cout << "arg:" << text << '\n';
            std::from_chars(text.data(), text.data() + text.size(), value);
            if (argument == "--port") port = static_cast<std::uint16_t>(value);
            else context = value;
        } else if ((argument == "--model" || argument == "--host" ||
                    argument == "--chatcompletionsadapter" || argument == "--gpulayers" ||
                    argument == "--multiuser") && index + 1 < argc) {
            std::cout << "arg:" << argv[++index] << '\n';
        }
    }
    std::cout.flush();
    if (port == 0 || context == 0) return 3;
    std::signal(SIGTERM, stop);
    std::signal(SIGINT, stop);
    const Socket listener = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (listener == invalid_socket) return 4;
    int reuse = 1;
#ifdef _WIN32
    setsockopt(listener, SOL_SOCKET, SO_REUSEADDR, reinterpret_cast<const char *>(&reuse), sizeof(reuse));
#else
    setsockopt(listener, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse));
#endif
    sockaddr_in address{};
    address.sin_family = AF_INET;
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    address.sin_port = htons(port);
    if (bind(listener, reinterpret_cast<sockaddr *>(&address), sizeof(address)) != 0 ||
        listen(listener, 8) != 0) return 5;
    while (running) {
        fd_set readable;
        FD_ZERO(&readable);
        FD_SET(listener, &readable);
        timeval timeout{0, 100000};
        const auto selected = select(static_cast<int>(listener) + 1, &readable, nullptr, nullptr,
                                     &timeout);
        if (!running) break;
        if (selected <= 0) continue;
        sockaddr_in peer{};
#ifdef _WIN32
        int size = sizeof(peer);
#else
        socklen_t size = sizeof(peer);
#endif
        const Socket client = accept(listener, reinterpret_cast<sockaddr *>(&peer), &size);
        if (client == invalid_socket) break;
        std::string request;
        char buffer[2048];
        while (request.find("\r\n\r\n") == std::string::npos) {
            const auto count = recv(client, buffer, sizeof(buffer), 0);
            if (count <= 0) break;
            request.append(buffer, static_cast<std::size_t>(count));
        }
        if (request.starts_with("GET /api/extra/version "))
            reply(client, R"({"result":"KoboldCpp","version":"fixture-1","llm":true})");
        else if (request.starts_with("GET /api/v1/model "))
            reply(client, R"({"result":"koboldcpp/fixture"})");
        else if (request.starts_with("GET /api/extra/true_max_context_length "))
            reply(client, "{\"value\":" + std::to_string(context) + "}");
        else
            reply(client, R"({"error":"unexpected"})");
        close_socket(client);
    }
    close_socket(listener);
#ifdef _WIN32
    WSACleanup();
#endif
    return 0;
}
