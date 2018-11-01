cmake_minimum_required(VERSION 2.8)
set(CMAKE_BUILD_TYPE Release)
set(CMAKE_CXX_STANDARD 11)
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++11")
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTOUIC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_INCLUDE_CURRENT_DIR ON)

list(APPEND CMAKE_MODULE_PATH "${CMAKE_SOURCE_DIR}")
find_package(OpenCV REQUIRED )
find_package(Qt4 REQUIRED)
find_package(LIBVLC REQUIRED)

include_directories(${QT_INCLUDES})

add_library(arthurwidgets_lgpl SHARED arthurwidgets.cpp)
target_link_libraries (arthurwidgets_lgpl Qt4::QtGui Qt4::QtCore)

add_library(eyesimulation STATIC EyeSimulation.cpp)

add_executable(qteye EyeSimulation.cpp QtEye.cpp qteyemain.cpp)
target_link_libraries (qteye Qt4::QtGui Qt4::QtCore Qt4::QtNetwork arthurwidgets_lgpl eyesimulation)

add_library(qtmotiontracking STATIC QtMotionTracking.cpp )
target_link_libraries (qtmotiontracking ${OpenCV_LIBS} ${VLC_LIBRARIES} avformat )

add_executable(qtmotion QtMotion.cpp qtmotionmain.cpp CtrlCHandler.cpp )
target_link_libraries (qtmotion Qt4::QtCore Qt4::QtNetwork eyesimulation qtmotiontracking)
