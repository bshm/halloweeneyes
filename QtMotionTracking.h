/* Copyright (c) 2016 Bastian Schmitz
 * Based on code (DIYsc.cpp, surveillanceCap.cpp) written by Kyle Hounslow, March 2014
 * Based on code (motionTracking.cpp) written by Kyle Hounslow, December 2013
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */

#ifndef QT_MOTION_TRACKING_H_INCLUDED
#define QT_MOTION_TRACKING_H_INCLUDED

#include <QObject>
#include <opencv/cv.h>
#include <opencv/highgui.h>
#include <chrono>
#include "SMA.h"
#include <cstdint>
#include <ctime>
#include <memory>
#include <QString>
#include <QDebug>
#include <QThread>
#include <QTimer>
#include <vlc/vlc.h>

class QtMotionTracking : public QObject
{
  Q_OBJECT
public:

  QtMotionTracking();
  virtual ~QtMotionTracking();

  bool open(const QString& source_, const QString& dest);


  void close();

  bool searchForMovement(cv::Mat thresholdImage, cv::Mat &cameraFeed, uint32_t& x, uint32_t& y);

  std::shared_ptr<libvlc_instance_t> vlcInstance;
  std::shared_ptr<libvlc_media_t> vlcMedia;
  std::shared_ptr<libvlc_media_player_t> vlcMediaMplayer;


  QThread motionTrackingThread;
  QTimer motionTrackingTimer;

//our sensitivity value to be used in the absdiff() function
  static const int SENSITIVITY_VALUE = 40;
//size of blur used to smooth the intensity image output from absdiff() function
  const static int BLUR_SIZE = 10;

//bounding rectangle of the object, we will use the center of this as its position.
  cv::Rect objectBoundingRectangle;

  uint32_t objectDetectedCount;
  cv::Size smallSize;
  QString source;

  //these two can be toggled by pressing 'd' or 't'
  bool debugMode;
  bool trackingEnabled;
  //pause and resume code
  bool pause;
  //set up the matrices that we will need
  //the two frames we will be comparing

  bool hasLastImage;
  cv::Mat lastImage;
  cv::Mat currentImage;
  //their grayscale images (needed for absdiff() function)

  cv::Mat currentGrayImageSmall;
  cv::Mat lastGrayImageSmall;
  cv::Mat lastImageSmall;
  cv::Mat currentImageSmall;


  //resulting difference image
  cv::Mat differenceImage;
  //thresholded difference image (for use in findContours() function)
  cv::Mat thresholdImage;
  //video capture object.
  cv::VideoCapture capture;
  cv::VideoWriter oVideoWriter;

  SMA benchmarkFilter;
  uint32_t frames;

  double xEye;
  double yEye;


public slots:
  bool step();

signals:

  void objectDetected(int x, int y);

private:
  bool opening = false;
  void handleEventMember(const libvlc_event_t* pEvt);
};


#endif
