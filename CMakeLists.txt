cmake_minimum_required(VERSION 3.21)
project(JeevesCore VERSION 0.1.0 LANGUAGES C CXX)

include(CMakePackageConfigHelpers)
include(GNUInstallDirs)

include(FetchContent)
if(POLICY CMP0135)
  cmake_policy(SET CMP0135 NEW)
endif()

if(PROJECT_IS_TOP_LEVEL)
  set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
endif()

set(JEEVES_MINIMUM_NLOHMANN_VERSION 3.11.3)
find_package(nlohmann_json ${JEEVES_MINIMUM_NLOHMANN_VERSION} QUIET)
if(NOT nlohmann_json_FOUND)
  FetchContent_Declare(nlohmann_json
    URL https://github.com/nlohmann/json/releases/download/v${JEEVES_MINIMUM_NLOHMANN_VERSION}/json.tar.xz)
  FetchContent_MakeAvailable(nlohmann_json)
  set(JEEVES_NLOHMANN_VERSION ${JEEVES_MINIMUM_NLOHMANN_VERSION})
else()
  set(JEEVES_NLOHMANN_VERSION ${nlohmann_json_VERSION})
endif()

add_library(jeeves_core_cpp STATIC
  cpp/sync.cpp
  cpp/types.cpp
  cpp/events.cpp
  cpp/tools.cpp
  cpp/workflow.cpp
  cpp/llm.cpp
  cpp/engine.cpp
  cpp/llm/koboldcpp.cpp)
add_library(jeeves::core ALIAS jeeves_core_cpp)
find_package(Threads REQUIRED)
target_include_directories(jeeves_core_cpp PUBLIC
  $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
  $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>)
target_link_libraries(jeeves_core_cpp PUBLIC Threads::Threads
  $<BUILD_INTERFACE:nlohmann_json::nlohmann_json>)
target_compile_features(jeeves_core_cpp PUBLIC cxx_std_23)
set_target_properties(jeeves_core_cpp PROPERTIES CXX_EXTENSIONS OFF)
set_target_properties(jeeves_core_cpp PROPERTIES EXPORT_NAME core)
if(WIN32)
  target_link_libraries(jeeves_core_cpp PRIVATE ws2_32)
endif()

install(TARGETS jeeves_core_cpp EXPORT JeevesCoreTargets
  ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR}
  LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
  RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
  INCLUDES DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
install(DIRECTORY include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
install(EXPORT JeevesCoreTargets
  FILE JeevesCoreTargets.cmake
  NAMESPACE jeeves::
  DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/JeevesCore)
configure_package_config_file(cmake/JeevesCoreConfig.cmake.in
  "${CMAKE_CURRENT_BINARY_DIR}/JeevesCoreConfig.cmake"
  INSTALL_DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/JeevesCore)
write_basic_package_version_file(
  "${CMAKE_CURRENT_BINARY_DIR}/JeevesCoreConfigVersion.cmake"
  VERSION ${PROJECT_VERSION}
  COMPATIBILITY SameMajorVersion)
install(FILES
  "${CMAKE_CURRENT_BINARY_DIR}/JeevesCoreConfig.cmake"
  "${CMAKE_CURRENT_BINARY_DIR}/JeevesCoreConfigVersion.cmake"
  DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/JeevesCore)

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
  add_executable(jeeves_koboldcpp_tests tests/koboldcpp.cpp)
  target_link_libraries(jeeves_koboldcpp_tests PRIVATE jeeves::core GTest::gtest_main)
  add_executable(koboldcpp_process_fixture tests/koboldcpp_process_fixture.cpp)
  target_compile_features(koboldcpp_process_fixture PRIVATE cxx_std_23)
  if(WIN32)
    target_link_libraries(koboldcpp_process_fixture PRIVATE ws2_32)
  endif()
  target_compile_definitions(jeeves_koboldcpp_tests PRIVATE
    KOBOLDCPP_PROCESS_FIXTURE="$<TARGET_FILE:koboldcpp_process_fixture>")
  add_dependencies(jeeves_koboldcpp_tests koboldcpp_process_fixture)
  include(GoogleTest)
  gtest_discover_tests(jeeves_validation_tests PROPERTIES TIMEOUT 15)
  gtest_discover_tests(jeeves_runner_tests PROPERTIES TIMEOUT 15)
  gtest_discover_tests(jeeves_koboldcpp_tests PROPERTIES TIMEOUT 15)
  add_test(NAME JeevesCore.package_consumer COMMAND "${CMAKE_COMMAND}"
    -DTEST_ROOT=${CMAKE_CURRENT_BINARY_DIR}/package-consumer-test
    -DJEEVES_BUILD=${CMAKE_CURRENT_BINARY_DIR}
    -DJEEVES_SOURCE=${CMAKE_CURRENT_SOURCE_DIR}
    -DNLOHMANN_SOURCE=${nlohmann_json_SOURCE_DIR}
    -DTEST_GENERATOR=${CMAKE_GENERATOR}
    -P ${CMAKE_CURRENT_SOURCE_DIR}/tests/package_consumer.cmake)
  set_tests_properties(JeevesCore.package_consumer PROPERTIES TIMEOUT 120)
endif()

option(JEEVES_BUILD_EXAMPLES "Build C++ examples" ${PROJECT_IS_TOP_LEVEL})
if(JEEVES_BUILD_EXAMPLES)
  add_executable(jeeves_pipeline_example examples/pipeline.cpp)
  target_link_libraries(jeeves_pipeline_example PRIVATE jeeves::core)
  add_executable(jeeves_llm_stream_example examples/llm_stream.cpp)
  target_link_libraries(jeeves_llm_stream_example PRIVATE jeeves::core)
endif()
