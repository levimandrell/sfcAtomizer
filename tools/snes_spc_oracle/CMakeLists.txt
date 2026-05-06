cmake_minimum_required(VERSION 3.16)
project(snes_spc_oracle CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# snes_spc 0.9.0 (vendored under vendor/snes_spc/, LGPL-2.1+).
file(GLOB SNES_SPC_SOURCES CONFIGURE_DEPENDS vendor/snes_spc/*.cpp)

add_executable(snes_spc_oracle main.cpp ${SNES_SPC_SOURCES})
target_include_directories(snes_spc_oracle PRIVATE vendor)

if(MSVC)
    target_compile_options(snes_spc_oracle PRIVATE /W3 /EHsc /wd4244 /wd4267)
else()
    target_compile_options(snes_spc_oracle PRIVATE -Wall -O2)
endif()

# snes_spc is LGPL-2.1+; the wrapper binary inherits LGPL distribution
# obligations (source preserved under vendor/, rebuild capability via
# this CMakeLists). Apache-2.0 host links nothing — process-boundary
# integration only. See ../../LICENSING.md and README.md.
