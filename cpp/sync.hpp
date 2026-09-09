#pragma once

#include <condition_variable>
#include <deque>
#include <memory>
#include <mutex>
#include <optional>
#include <stop_token>
#include <string>

namespace jeeves::detail {

template <class T>
struct QueueState {
    mutable std::mutex mutex;
    std::condition_variable_any changed;
    std::deque<T> values;
    bool closed = false;
    bool receiver_alive = true;

    bool push(T value) {
        std::lock_guard lock(mutex);
        if (closed || !receiver_alive) return false;
        values.push_back(std::move(value));
        changed.notify_all();
        return true;
    }
    std::optional<T> pop(std::stop_token stop = {}) {
        std::unique_lock lock(mutex);
        changed.wait(lock, stop, [&] { return !values.empty() || closed; });
        if (values.empty()) return std::nullopt;
        T value = std::move(values.front()); values.pop_front(); return value;
    }
    std::optional<T> try_pop() {
        std::lock_guard lock(mutex);
        if (values.empty()) return std::nullopt;
        T value = std::move(values.front()); values.pop_front(); return value;
    }
    void close() {
        std::lock_guard lock(mutex); closed = true; changed.notify_all();
    }
    void drop_receiver() {
        std::lock_guard lock(mutex); receiver_alive = false; values.clear(); changed.notify_all();
    }
    bool has_receiver() const {
        std::lock_guard lock(mutex); return receiver_alive;
    }
};

template <class T>
struct ResultState {
    std::mutex mutex;
    std::condition_variable changed;
    std::shared_ptr<T> value;
};

std::string uuid_v4();

} // namespace jeeves::detail
