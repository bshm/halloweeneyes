/* Copyright (c) 2016 Bastian Schmitz
 * Based on code (DIYsc.cpp, surveillanceCap.cpp) written by Kyle Hounslow, March 2014
 * Based on code (motionTracking.cpp) written by Kyle Hounslow, December 2013
 * Based on https://github.com/jrterven/OpenCV-VLC
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

#include "QtMotionTracking.h"

#include <stdint.h>
#include <inttypes.h>

using namespace std;
using namespace cv;

uint8_t* videoBuffer = 0;
unsigned int videoBufferSize = 0;

void QtMotionTracking::cbVideoPrerender(void* p_video_data, uint8_t** pp_pixel_buffer, int size)
{
  fprintf(stderr, "HERE1 p_video_data=% " PRId64 "\n", p_video_data);
  ((QtMotionTracking*)p_video_data)->videoPrerender(pp_pixel_buffer, size);
}


void QtMotionTracking::videoPrerender(uint8_t** pp_pixel_buffer, int size)
{
  fprintf(stderr, "HERE1\n");
  if (size > videoBufferSize || !videoBuffer)
  {
    printf("Reallocate raw video buffer %d bytes\n", size);
    free(videoBuffer);
    videoBuffer = (uint8_t*)malloc(size);
    videoBufferSize = size;
  }

  // videoBuffer = (uint8_t *)malloc(size);
  *pp_pixel_buffer = videoBuffer;
}


void QtMotionTracking::cbVideoPostrender(void* p_video_data, uint8_t* p_pixel_buffer, int width, int height,
                                         int pixel_pitch, int size, int64_t pts)
{
  fprintf(stderr, "HERE2 p_video_data=% " PRId64 "\n", p_video_data);
  ((QtMotionTracking*)p_video_data)->videoPostRender(p_pixel_buffer, width, height, pixel_pitch, size, pts);
}


void QtMotionTracking::videoPostRender(uint8_t* p_pixel_buffer, int width, int height, int pixel_pitch, int size,
                                       int64_t pts)
{
  fprintf(stderr, "HERE2\n");
}


void QtMotionTracking::handleEventMember(const libvlc_event_t* pEvt)
{
  libvlc_time_t time;
  switch (pEvt->type)
  {
    case libvlc_MediaPlayerTimeChanged:
      time = libvlc_media_player_get_time(vlcMediaMplayer.get());
      printf("MediaPlayerTimeChanged %lld ms\n", (long long)time);
      break;

    case libvlc_MediaPlayerEndReached:
      printf("MediaPlayerEndReached\n");
      //done = 1;
      break;

    default:
      printf("%s\n", libvlc_event_type_name(pEvt->type));
  }
}


QtMotionTracking::QtMotionTracking()
  : benchmarkFilter(100)
{
  objectBoundingRectangle = Rect(0, 0, 0, 0);
  objectDetectedCount = 0;
  smallSize = Size(320, 240);
  debugMode = false;
  hasLastImage = false;
  frames = 0;


  motionTrackingTimer.setInterval(80); // slightly faster than 10fps
  connect(&motionTrackingTimer, SIGNAL(timeout()), this, SLOT(step()));
  this->moveToThread(&motionTrackingThread);

  motionTrackingThread.start();
}


QtMotionTracking::~QtMotionTracking()
{ }


bool QtMotionTracking::open(const QString& source_, const QString& dest)
{
  source = source_;

  oVideoWriter  = VideoWriter(qPrintable(dest), CV_FOURCC('X', 'V', 'I', 'D'), 20, smallSize, true);
  if (!oVideoWriter.isOpened())
  {
    qDebug("Error opening video writer.");
    return false;
  }


  // VLC options
  char smem_options[1000];

  // RV24
  sprintf(smem_options,
          "#transcode{vcodec=RV24}:smem{"
          "video-prerender-callback=%" PRId64 ","
          "video-postrender-callback=%" PRId64 ","
          "video-data=%" PRId64 ","
          "no-time-sync},",
          (long long int)(intptr_t)(void*)&cbVideoPrerender,
          (long long int)(intptr_t)(void*)&cbVideoPostrender,
          (long long int)(intptr_t)(void*)this
         );

  const char* const vlc_args[] = {
    "-I", "dummy",                        // Don't use any interface
    "--ignore-config",                    // Don't use VLC's config
    "--no-xlib",
    "--no-audio",
    "--aout=none",
    "--extraintf=logger",                 // Log anything
//    "--verbose=2",                        // Be verbose
    "--sout", smem_options                // Stream to memory
  };

  // Launch VLC
  vlcInstance =
    std::shared_ptr<libvlc_instance_t>(libvlc_new(sizeof(vlc_args) / sizeof(vlc_args[0]), vlc_args),
                                       [=](libvlc_instance_t* ptr)
  {
    libvlc_release(ptr);
  });

  vlcMedia =
    std::shared_ptr<libvlc_media_t>(libvlc_media_new_location(vlcInstance.get(), qPrintable(source)),
                                    [=](libvlc_media_t* ptr)
  {
    libvlc_media_release(ptr);
  });

  vlcMediaMplayer =
    std::shared_ptr<libvlc_media_player_t>(libvlc_media_player_new_from_media(vlcMedia.get()),
                                           [=](libvlc_media_player_t* ptr)
  {
    libvlc_media_player_release(ptr);
  });

  libvlc_event_manager_t* eventManager = libvlc_media_player_event_manager(vlcMediaMplayer.get());

  auto handleEvent([](const libvlc_event_t* pEvt, void* pUserData)
    {
                   ((QtMotionTracking*)pUserData)->handleEventMember(pEvt);
    });
  libvlc_event_attach(eventManager, libvlc_MediaPlayerTimeChanged, handleEvent, this);
  libvlc_event_attach(eventManager, libvlc_MediaPlayerEndReached, handleEvent, this);
  libvlc_event_attach(eventManager, libvlc_MediaPlayerPositionChanged, handleEvent, this);


  motionTrackingTimer.start();
}


bool QtMotionTracking::step()
{
  bool _objectDetected = false;
  debugMode = false;
  trackingEnabled = true;
  pause = false;

  try
  {
    //we can loop the video by re-opening the capture every time the video reaches its last frame

    if (!libvlc_media_player_is_playing(vlcMediaMplayer.get()) && !opening)
    {
      qDebug("opening '%s'", qPrintable(source));
      opening = true;
      libvlc_media_player_play(vlcMediaMplayer.get());
      hasLastImage = false;
    }
#if 0
    if (!hasLastImage)
    {
      //read first frame
      //if (capture.read(lastImage))
      {
        hasLastImage = true;
        frames += 1;
        //convert lastImage to gray scale for frame differencing
        cv::resize(lastImage, lastImageSmall, smallSize, 0, 0, INTER_AREA);
        cv::cvtColor(lastImageSmall, lastGrayImageSmall, COLOR_BGR2GRAY);
      }
      return false;
    }


    const auto start1 = chrono::steady_clock::now();


    //copy second frame
    if (capture.read(currentImage))
    {
      frames += 1;

      //convert currentImage to gray scale for frame differencing
      cv::resize(currentImage, currentImageSmall, smallSize, 0, 0, INTER_AREA);
      cv::cvtColor(currentImageSmall, currentGrayImageSmall, COLOR_BGR2GRAY);


      //perform frame differencing with the sequential images. This will output an "intensity image"
      //do not confuse this with a threshold image, we will need to perform thresholding afterwards.
      cv::absdiff(lastGrayImageSmall, currentGrayImageSmall, differenceImage);
      //threshold intensity image at a given sensitivity value
      cv::threshold(differenceImage, thresholdImage, SENSITIVITY_VALUE, 255, THRESH_BINARY);
      if (debugMode == true)
      {
        //show the difference image and threshold image
        cv::imshow("Difference Image", differenceImage);
        cv::imshow("Threshold Image", thresholdImage);
      }
      else
      {
        //if not in debug mode, destroy the windows so we don't see them anymore
        cv::destroyWindow("Difference Image");
        cv::destroyWindow("Threshold Image");
      }
      //blur the image to get rid of the noise. This will output an intensity image
      cv::blur(thresholdImage, thresholdImage, cv::Size(BLUR_SIZE, BLUR_SIZE));
      //threshold again to obtain binary image from blur output
      cv::threshold(thresholdImage, thresholdImage, SENSITIVITY_VALUE, 255, THRESH_BINARY);
      if (debugMode == true)
      {
        //show the threshold image after it's been "blurred"

        imshow("Final Threshold Image", thresholdImage);

      }
      else
      {
        //if not in debug mode, destroy the windows so we don't see them anymore
        cv::destroyWindow("Final Threshold Image");
      }

      uint32_t x, y;

      //if tracking enabled, search for contours in our thresholded image
      if (trackingEnabled)
      {
        _objectDetected = searchForMovement(thresholdImage, currentImageSmall, x, y);
      }
      else
      {
        _objectDetected = false;
      }


      const auto end = chrono::steady_clock::now();
      const auto diff1 = end - start1;
      benchmarkFilter.add(chrono::duration <double, milli> (diff1).count());

      const auto avg1 = benchmarkFilter.avg();
      const auto fps1 = 1.0/(avg1/1000.0);

      if (_objectDetected)
      {
        objectDetectedCount += 1;

        printf("frames:%d, odc=%u, BM1: %4.2lfms, %3.2lffps, x=%d y=%d\n",
               frames, objectDetectedCount, avg1, fps1, x, y);

        emit objectDetected(x, y);
      }

      //show our captured frame
      if (_objectDetected)
      {
        //imshow("Input Image", currentImageSmall);
        oVideoWriter.write(currentImageSmall);
      }


      lastGrayImageSmall = currentGrayImageSmall.clone();
    }
    else
    {
      printf("capture returned no image\n");
      capture.release();
    }
#endif
  }
  catch (const cv::Exception& e)
  {
    cout<<"Caught Exception:" << e.what() <<endl;
  }

  return _objectDetected;
}


void QtMotionTracking::close()
{
  motionTrackingTimer.stop();
  qDebug("QtMotionTracking::close()");
  motionTrackingThread.quit();
  motionTrackingThread.wait(5000);
  //capture.release();
}


bool QtMotionTracking::searchForMovement(cv::Mat thresholdImage, cv::Mat &cameraFeed, uint32_t& x, uint32_t& y)
{
  //notice how we use the '&' operator for objectDetected and cameraFeed. This is because we wish
  //to take the values passed into the function and manipulate them, rather than just working with a copy.
  //eg. we draw to the cameraFeed to be displayed in the main() function.
  bool objectDetected = false;
  Mat temp;
  thresholdImage.copyTo(temp);
  //these two vectors needed for output of findContours
  vector< vector<Point> > contours;
  vector<Vec4i> hierarchy;
  //find contours of filtered image using openCV findContours function
  //findContours(temp,contours,hierarchy,CV_RETR_CCOMP,CV_CHAIN_APPROX_SIMPLE );// retrieves all contours
  findContours(temp, contours, hierarchy, CV_RETR_EXTERNAL, CV_CHAIN_APPROX_SIMPLE );  // retrieves external contours

  //if contours vector is not empty, we have found some objects
  if (contours.size() > 0)
  {
    objectDetected=true;
  }
  else
  {
    objectDetected = false;
  }

  if (objectDetected)
  {
    //the largest contour is found at the end of the contours vector
    //we will simply assume that the biggest contour is the object we are looking for.
    vector< vector<Point> > largestContourVec;
    largestContourVec.push_back(contours.at(contours.size()-1));
    //make a bounding rectangle around the largest contour then find its centroid
    //this will be the object's final estimated position.
    objectBoundingRectangle = boundingRect(largestContourVec.at(0));
    x = objectBoundingRectangle.x+objectBoundingRectangle.width/2;
    y = objectBoundingRectangle.y+objectBoundingRectangle.height/2;

#if 0
    //draw some crosshairs around the object
    circle(cameraFeed, Point(x, y), 20, Scalar(0, 255, 0), 2);
    line(cameraFeed, Point(x, y), Point(x, y-25), Scalar(0, 255, 0), 2);
    line(cameraFeed, Point(x, y), Point(x, y+25), Scalar(0, 255, 0), 2);
    line(cameraFeed, Point(x, y), Point(x-25, y), Scalar(0, 255, 0), 2);
    line(cameraFeed, Point(x, y), Point(x+25, y), Scalar(0, 255, 0), 2);
#else
    rectangle(cameraFeed, objectBoundingRectangle, Scalar(255, 255, 0));

#endif


  }

  return objectDetected;
}