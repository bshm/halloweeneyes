# halloweeneyes

[![Build Status](https://www.travis-ci.com/bshm/halloweeneyes.svg?branch=master)](https://www.travis-ci.com/bshm/halloweeneyes)
[![BCH compliance](https://bettercodehub.com/edge/badge/bshm/halloweeneyes?branch=master)](https://bettercodehub.com/)


Code for projecting moving eyes to windows

This is code to project scary looking moving eyes to the inside of windows.
A camera tracks the movement in front of the house and makes the eyes
follow people moving in front of the house

![alt text](demo.jpg "Example Image")

## Architecture

The components are
- IP RTSP camera (e.g. AVTECH AVN812)
- motion detection software (qtmotion)
- eye rendering software (qteye)
- 2 single board computers with HDMI connector (Raspberry Pi or Beaglebone Black)
- 2 LED beamers used for projection

The qtmotion software obtains an RTSP video stream from the camera and tries to detect motion in successive vide frames.
It also contains an EyeSimulation that generates eye movement and lid movement if no motion is detected.
qtmotion broadcasts the eye movement data using UDP multicast to all running qteye instances in the local ethernet.
qteye picks up this data and renders the eye accoringly.
The eye rendering consist of simple overlying of the background image and an iris picture to generate the impression of
upper and lower lids and a moving eye.

I used this with Beaglebone Black and Raspberry PI (rendering) and on a Debian wheezy server (motion detection).

## Prerequisites
- IP webcam (RTSP) that works reasonably well at night
- Two Linux computers (Beaglebone, Raspberry PI, x86) with video outputs. Probably a single PC with multiple video outputs works as well.
- Two projectors

## Setup

### Setup of qtmotion
- start a screen session
- execute
./qtmotion rtsp://admin:admin@192.168.42.33/live/h264/QVGA capture-2016-10-30-22-30.av
The first parameter is the URL of the RTSP stream as understood by opencv, the second parameter is the name of an
output file for debuuging motion detection

### Setup on beaglebone black
- download bone-debian-8.4-lxqt-4gb-armhf-2016-05-13-4gb.img
- remove unnecessary stuff from image:
- sudo apt purge locales ti-llvm-3.3 chromium-browser firmware-am57xx-opencl-monitor nodejs oxygen-icon-theme ti-opencl chromium-browser-l10n adwaita-icon-theme apache2 apache2-bin apache2-data apache2-utils
- sudo apt-get autoremove
- sudo apt-get update
- sudo apt-get install build-essential cmake libopencv-dev libqt4-private-dev x11-xserver-utils
- sudo apt-get upgrade

#### Compilation
- clone this repository
- cd /home/debian
- git clone https://github.com/bshm/halloweeneyes.git
- cd halloweeneyes
- cmake .
- make

#### Autostart, Resolution, ...
uEnv.txt change kernel command line to 800x600 and don't start X server
- cmdline=coherent_pool=1M quiet cape_universal=enable video=HDMI-A-1:800x600@60 text

- Autostart
/etc/rc.local
Add 
sudo /home/default/halloweeneyes/start.sh 
before "exit 0"

- /etc/X11/xorg.conf
disable screensaver and dpms by setting timeouts to zero

### Tuning

The code contains quite a lot hardcoded information
- The background images
- The size of the eye images (BG, IRIS) are expected to be 800x600. Lots of hardcoded coordinates inside qteye.
- The multicast group is hardcoded somewhere
- The mapping from camera coordinates (320x200) to eye coordinates ([-1..1], [-1..1]) is hardcoded in QtMotion.cpp

For coordinates pairs inside the image file have been picked to map to the extreme positions of the eye balls.

Those functions have been generated using wolfram alpha:
- http://www.wolframalpha.com/input/?i=xfun%3DInterpolatingPolynomial%5B%7B%7B%7B285.0,116%7D,+-1%7D,%7B%7B940,77%7D,+1%7D,%7B%7B110,587%7D,+-1%7D,+%7B%7B1087,504%7D,+1%7D,+%7B%7B600,298%7D,0%7D%7D,%7Bx,y%7D%5D%2F%2FN
- http://www.wolframalpha.com/input/?i=yfun%3DInterpolatingPolynomial%5B%7B%7B%7B285,116%7D,+0%7D,%7B%7B940,77%7D,+0%7D,%7B%7B110,587%7D,+1%7D,+%7B%7B1087,504%7D,+1%7D,+%7B%7B600,298%7D,0.5%7D%7D,%7Bx,y%7D%5D%2F%2FN

The camera coordinates used for this mapping still use (1024x1280)


```C++
const double xEye = -2.10554+x_1280 * (0.00362959 -2.89324E-7 * x_1280-2.24893E-6 * y_1024)+0.00145656 * y_1024;
const double yEye = -0.383554+x_1280 * (0.000500975 -3.04759E-7 * x_1280+1.12333E-7 * y_1024)+0.00225704 * y_1024;
```

![alt text](BG.png "Background picture")
![alt text](IRIS.png "Iris picture")
