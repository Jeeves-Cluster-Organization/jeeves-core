#include <jeeves/events.hpp>

#include "sync.hpp"

namespace jeeves {
EventReceiver & EventReceiver::operator=(EventReceiver && other) noexcept {
    if (this != &other) { reset(); state_ = std::move(other.state_); }
    return *this;
}
EventReceiver::~EventReceiver() { reset(); }
void EventReceiver::reset() { if (state_) state_->drop_receiver(); state_.reset(); }
std::optional<RunEvent> EventReceiver::next() { return state_ ? state_->pop() : std::nullopt; }
} // namespace jeeves
