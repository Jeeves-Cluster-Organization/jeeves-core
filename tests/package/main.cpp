#include <jeeves/jeeves.hpp>

int main() {
    auto provider = jeeves::MockLlmProvider::text("installed");
    return provider ? 0 : 1;
}
