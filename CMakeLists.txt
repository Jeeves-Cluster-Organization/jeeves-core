cmake_minimum_required(VERSION 3.21)
project(jeeves_core_cpp VERSION 0.0.4 LANGUAGES C CXX)

include(FetchContent)
if(POLICY CMP0135)
  cmake_policy(SET CMP0135 NEW)
endif()

if(PROJECT_IS_TOP_LEVEL)
  set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
endif()

find_package(nlohmann_json 3.11.3 QUIET)
if(NOT nlohmann_json_FOUND)
  FetchContent_Declare(nlohmann_json
    URL https://github.com/nlohmann/json/releases/download/v3.11.3/json.tar.xz)
  FetchContent_MakeAvailable(nlohmann_json)
endif()

set(LLAMA_BUILD_COMMON ON CACHE BOOL "" FORCE)
set(LLAMA_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_SERVER OFF CACHE BOOL "" FORCE)
set(LLAMA_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(LLAMA_CURL OFF CACHE BOOL "" FORCE)
set(BUILD_SHARED_LIBS OFF)
FetchContent_Declare(llama_cpp
  GIT_REPOSITORY https://github.com/ggml-org/llama.cpp.git
  GIT_TAG c1d0e7a004015f23bc0233470b747b596f29b264)
FetchContent_MakeAvailable(llama_cpp)
if(NOT TARGET llama)
  message(FATAL_ERROR "The pinned llama.cpp source did not provide the llama target")
endif()
if(APPLE AND CMAKE_CXX_COMPILER_ID MATCHES "Clang")
  set_property(TARGET llama PROPERTY CXX_STANDARD 17)
  target_compile_options(llama PRIVATE -fdelayed-template-parsing)
endif()

add_library(jeeves_core_cpp STATIC
  cpp/sync.cpp
  cpp/types.cpp
  cpp/events.cpp
  cpp/tools.cpp
  cpp/workflow.cpp
  cpp/llm.cpp
  cpp/engine.cpp
  cpp/llm/llamacpp.cpp)
add_library(jeeves::core ALIAS jeeves_core_cpp)
find_package(Threads REQUIRED)
target_include_directories(jeeves_core_cpp PUBLIC include)
target_link_libraries(jeeves_core_cpp PUBLIC nlohmann_json::nlohmann_json Threads::Threads PRIVATE llama-common)
target_compile_features(jeeves_core_cpp PUBLIC cxx_std_23)
set_target_properties(jeeves_core_cpp PROPERTIES CXX_EXTENSIONS OFF)
if(TARGET ggml::all)
  target_link_libraries(jeeves_core_cpp PRIVATE ggml::all)
endif()

option(JEEVES_BUILD_TESTS "Build C++ tests" ${PROJECT_IS_TOP_LEVEL})
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

option(JEEVES_BUILD_EXAMPLES "Build C++ examples" ${PROJECT_IS_TOP_LEVEL})
if(JEEVES_BUILD_EXAMPLES)
  add_executable(jeeves_pipeline_example examples/pipeline.cpp)
  target_link_libraries(jeeves_pipeline_example PRIVATE jeeves::core)
  add_executable(jeeves_llm_stream_example examples/llm_stream.cpp)
  target_link_libraries(jeeves_llm_stream_example PRIVATE jeeves::core)
endif()
