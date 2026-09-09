#include "sync.hpp"

#include <jeeves/types.hpp>

namespace jeeves::detail {
std::string uuid_v4() { return RunId().as_str(); }
} // namespace jeeves::detail
