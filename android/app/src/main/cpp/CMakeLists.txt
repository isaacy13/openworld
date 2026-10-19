# SPDX-License-Identifier: Apache-2.0
cmake_minimum_required(VERSION 3.22.1)
project(openworld_jni C)

add_library(openworld_jni SHARED openworld_jni.c)
target_include_directories(openworld_jni PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../../../../../include)
target_compile_options(openworld_jni PRIVATE -Wall -Wextra -Werror=implicit-function-declaration)
# 16 KB pages (Android 15 devices).
target_link_options(openworld_jni PRIVATE -Wl,-z,max-page-size=16384)

# libopenworld_core.so is built by the buildRustCore Gradle task (cargo-ndk) into jniLibs,
# which packages it. It is linked by name, not as an IMPORTED target: AGP would package an
# imported library a second time.
target_link_directories(openworld_jni PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../jniLibs/${ANDROID_ABI})
target_link_libraries(openworld_jni openworld_core)
