#include <jeeves/tools.hpp>

namespace jeeves {
Result<ToolSpec> ToolSpec::create(std::string name, std::string description, json parameters) {
    if (name.empty()) return std::unexpected(Error::configuration("tool name cannot be empty"));
    return ToolSpec{std::move(name), std::move(description), std::move(parameters)};
}
} // namespace jeeves
