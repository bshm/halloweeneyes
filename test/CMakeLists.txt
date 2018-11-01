project(opencvvlc)

cmake_minimum_required(VERSION 2.8)

SET(CMAKE_MODULE_PATH ".")
set(CMAKE_CXX_STANDARD 11)
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++11")
set(CMAKE_INCLUDE_CURRENT_DIR ON)

find_package( OpenCV REQUIRED )
FIND_PACKAGE(LIBVLC REQUIRED)

add_executable(opencvvlc OpenCV_VLC.cpp)
target_link_libraries (opencvvlc ${OpenCV_LIBS} ${VLC_LIBRARIES})
