cmake_minimum_required(VERSION 2.8)
set(CMAKE_BUILD_TYPE Release)
set(CMAKE_CXX_STANDARD 11)
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++11")
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTOUIC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_INCLUDE_CURRENT_DIR ON)

find_package( OpenCV REQUIRED )
find_package(Qt4 REQUIRED)

set(VLCQtCore_DIR "/opt/bshm/packages/vlcqt/lib/cmake/VLCQtCore")
find_package(VLCQtCore REQUIRED)

include_directories(${QT_INCLUDES})

add_library(arthurwidgets_lgpl SHARED arthurwidgets.cpp)
target_link_libraries (arthurwidgets_lgpl Qt4::QtGui Qt4::QtCore)

add_library(eyesimulation STATIC EyeSimulation.cpp)

add_executable(qteye EyeSimulation.cpp QtEye.cpp qteyemain.cpp)
target_link_libraries (qteye Qt4::QtGui Qt4::QtCore Qt4::QtNetwork arthurwidgets_lgpl eyesimulation)

add_executable(qtmotion QtMotion.cpp QtMotionTracking.cpp qtmotionmain.cpp CtrlCHandler.cpp )
target_link_libraries (qtmotion Qt4::QtCore Qt4::QtNetwork ${OpenCV_LIBS} avformat eyesimulation)
