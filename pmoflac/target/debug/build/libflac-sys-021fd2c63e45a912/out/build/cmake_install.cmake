# Install script for directory: /Users/coissac/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libflac-sys-0.3.4/flac

# Set the install prefix
if(NOT DEFINED CMAKE_INSTALL_PREFIX)
  set(CMAKE_INSTALL_PREFIX "/Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoflac/target/debug/build/libflac-sys-021fd2c63e45a912/out")
endif()
string(REGEX REPLACE "/$" "" CMAKE_INSTALL_PREFIX "${CMAKE_INSTALL_PREFIX}")

# Set the install configuration name.
if(NOT DEFINED CMAKE_INSTALL_CONFIG_NAME)
  if(BUILD_TYPE)
    string(REGEX REPLACE "^[^A-Za-z0-9_]+" ""
           CMAKE_INSTALL_CONFIG_NAME "${BUILD_TYPE}")
  else()
    set(CMAKE_INSTALL_CONFIG_NAME "Debug")
  endif()
  message(STATUS "Install configuration: \"${CMAKE_INSTALL_CONFIG_NAME}\"")
endif()

# Set the component getting installed.
if(NOT CMAKE_INSTALL_COMPONENT)
  if(COMPONENT)
    message(STATUS "Install component: \"${COMPONENT}\"")
    set(CMAKE_INSTALL_COMPONENT "${COMPONENT}")
  else()
    set(CMAKE_INSTALL_COMPONENT)
  endif()
endif()

# Is this installation the result of a crosscompile?
if(NOT DEFINED CMAKE_CROSSCOMPILING)
  set(CMAKE_CROSSCOMPILING "FALSE")
endif()

# Set path to fallback-tool for dependency-resolution.
if(NOT DEFINED CMAKE_OBJDUMP)
  set(CMAKE_OBJDUMP "/usr/bin/objdump")
endif()

if(NOT CMAKE_INSTALL_LOCAL_ONLY)
  # Include the install script for the subdirectory.
  include("/Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoflac/target/debug/build/libflac-sys-021fd2c63e45a912/out/build/src/cmake_install.cmake")
endif()

if(CMAKE_INSTALL_COMPONENT STREQUAL "Unspecified" OR NOT CMAKE_INSTALL_COMPONENT)
  file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/include/FLAC" TYPE FILE FILES
    "/Users/coissac/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libflac-sys-0.3.4/flac/include/FLAC/all.h"
    "/Users/coissac/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libflac-sys-0.3.4/flac/include/FLAC/assert.h"
    "/Users/coissac/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libflac-sys-0.3.4/flac/include/FLAC/callback.h"
    "/Users/coissac/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libflac-sys-0.3.4/flac/include/FLAC/export.h"
    "/Users/coissac/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libflac-sys-0.3.4/flac/include/FLAC/format.h"
    "/Users/coissac/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libflac-sys-0.3.4/flac/include/FLAC/metadata.h"
    "/Users/coissac/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libflac-sys-0.3.4/flac/include/FLAC/ordinals.h"
    "/Users/coissac/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libflac-sys-0.3.4/flac/include/FLAC/stream_decoder.h"
    "/Users/coissac/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/libflac-sys-0.3.4/flac/include/FLAC/stream_encoder.h"
    )
endif()

string(REPLACE ";" "\n" CMAKE_INSTALL_MANIFEST_CONTENT
       "${CMAKE_INSTALL_MANIFEST_FILES}")
if(CMAKE_INSTALL_LOCAL_ONLY)
  file(WRITE "/Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoflac/target/debug/build/libflac-sys-021fd2c63e45a912/out/build/install_local_manifest.txt"
     "${CMAKE_INSTALL_MANIFEST_CONTENT}")
endif()
if(CMAKE_INSTALL_COMPONENT)
  if(CMAKE_INSTALL_COMPONENT MATCHES "^[a-zA-Z0-9_.+-]+$")
    set(CMAKE_INSTALL_MANIFEST "install_manifest_${CMAKE_INSTALL_COMPONENT}.txt")
  else()
    string(MD5 CMAKE_INST_COMP_HASH "${CMAKE_INSTALL_COMPONENT}")
    set(CMAKE_INSTALL_MANIFEST "install_manifest_${CMAKE_INST_COMP_HASH}.txt")
    unset(CMAKE_INST_COMP_HASH)
  endif()
else()
  set(CMAKE_INSTALL_MANIFEST "install_manifest.txt")
endif()

if(NOT CMAKE_INSTALL_LOCAL_ONLY)
  file(WRITE "/Users/coissac/Sync/maison/Petite_maisons/src/pmomusic/pmoflac/target/debug/build/libflac-sys-021fd2c63e45a912/out/build/${CMAKE_INSTALL_MANIFEST}"
     "${CMAKE_INSTALL_MANIFEST_CONTENT}")
endif()
