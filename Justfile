# Install `just`: https://github.com/casey/just

default: check

check:
    cmake -S . -B build -DJEEVES_BUILD_TESTS=ON -DJEEVES_BUILD_EXAMPLES=ON
    cmake --build build -j 4
    ctest --test-dir build --output-on-failure

test:
    ctest --test-dir build --output-on-failure
