cmake_minimum_required(VERSION 3.20)
project(jeeves_core_cpp VERSION 0.0.4 LANGUAGES C CXX)

include(FetchContent)
if(POLICY CMP0135)
  cmake_policy(SET CMP0135 NEW)
endif()

set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

find_package(nlohmann_json 3.11.3 QUIET)
if(NOT nlohmann_json_FOUND)
  FetchContent_Declare(nlohmann_json
    URL https://github.com/nlohmann/json/releases/download/v3.11.3/json.tar.xz)
  FetchContent_MakeAvailable(nlohmann_json)
endif()

option(JEEVES_FETCH_LLAMA "Build the pinned llama.cpp source" OFF)
option(JEEVES_REQUIRE_LLAMA "Require a llama.cpp implementation" OFF)
if(JEEVES_FETCH_LLAMA)
  set(LLAMA_BUILD_COMMON OFF CACHE BOOL "" FORCE)
  set(LLAMA_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
  set(LLAMA_BUILD_SERVER OFF CACHE BOOL "" FORCE)
  set(LLAMA_BUILD_TESTS OFF CACHE BOOL "" FORCE)
  set(LLAMA_CURL OFF CACHE BOOL "" FORCE)
  FetchContent_Declare(llama_cpp
    GIT_REPOSITORY https://github.com/ggml-org/llama.cpp.git
    GIT_TAG c1d0e7a004015f23bc0233470b747b596f29b264
    GIT_SHALLOW TRUE)
  FetchContent_MakeAvailable(llama_cpp)
  if(APPLE AND CMAKE_CXX_COMPILER_ID MATCHES "Clang")
    set_property(TARGET llama PROPERTY CXX_STANDARD 17)
    target_compile_options(llama PRIVATE -fdelayed-template-parsing)
  endif()
elseif(JEEVES_REQUIRE_LLAMA)
  find_package(llama CONFIG REQUIRED)
else()
  find_package(llama CONFIG QUIET)
endif()

add_library(jeeves_core_cpp
  cpp/sync.cpp
  cpp/types.cpp
  cpp/events.cpp
  cpp/tools.cpp
  cpp/workflow.cpp
  cpp/llm.cpp
  cpp/engine.cpp)
add_library(jeeves::core ALIAS jeeves_core_cpp)
find_package(Threads REQUIRED)
target_include_directories(jeeves_core_cpp PUBLIC include)
target_link_libraries(jeeves_core_cpp PUBLIC nlohmann_json::nlohmann_json Threads::Threads)
target_compile_features(jeeves_core_cpp PUBLIC cxx_std_23)

if(TARGET llama)
  target_sources(jeeves_core_cpp PRIVATE cpp/llm/llamacpp.cpp)
  target_link_libraries(jeeves_core_cpp PRIVATE llama)
  if(TARGET ggml::all)
    target_link_libraries(jeeves_core_cpp PRIVATE ggml::all)
  endif()
  target_compile_definitions(jeeves_core_cpp PUBLIC JEEVES_HAS_LLAMA=1)
else()
  message(STATUS "llama.cpp not found; building core and mock provider only")
endif()

option(JEEVES_BUILD_TESTS "Build C++ tests" ON)
if(JEEVES_BUILD_TESTS)
  enable_testing()
  find_package(GTest CONFIG QUIET)
  if(NOT GTest_FOUND)
    find_package(GTest QUIET)
  endif()
  if(NOT GTest_FOUND)
    FetchContent_Declare(googletest
      URL https://github.com/google/googletest/archive/refs/tags/v1.15.2.tar.gz)
    set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
    set(BUILD_GMOCK OFF CACHE BOOL "" FORCE)
    FetchContent_MakeAvailable(googletest)
  endif()
  add_executable(jeeves_validation_tests tests/validation.cpp)
  target_link_libraries(jeeves_validation_tests PRIVATE jeeves::core GTest::gtest_main)
  add_executable(jeeves_runner_tests tests/runner.cpp)
  target_link_libraries(jeeves_runner_tests PRIVATE jeeves::core GTest::gtest_main)
  include(GoogleTest)
  gtest_discover_tests(jeeves_validation_tests PROPERTIES TIMEOUT 15)
  gtest_discover_tests(jeeves_runner_tests PROPERTIES TIMEOUT 15)
endif()

option(JEEVES_BUILD_EXAMPLES "Build C++ examples" ON)
if(JEEVES_BUILD_EXAMPLES)
  add_executable(jeeves_pipeline_example examples/pipeline.cpp)
  target_link_libraries(jeeves_pipeline_example PRIVATE jeeves::core)
  add_executable(jeeves_llm_stream_example examples/llm_stream.cpp)
  target_link_libraries(jeeves_llm_stream_example PRIVATE jeeves::core)
endif()
