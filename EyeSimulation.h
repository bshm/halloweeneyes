/* Copyright (c) 2016 Bastian Schmitz
 *
 * Eye Simulation based on code from Arduino Teensy Bowler Head project.
 * Written by Phil Burgess / Paint Your Dragon for Adafruit Industries.
 * MIT license.  SPI FIFO insight from Paul Stoffregen's ILI9341_t3 library.
 * Inspired by David Boccabella's (Marcwolf) hybrid servo/OLED eye concept.
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

#ifndef EYE_SIMULATION_H_INCLUDED
#define EYE_SIMULATION_H_INCLUDED

#include <QPointF>
#include <QElapsedTimer>
#include <QTimer>
#include <QDataStream>

template <typename T>
inline T randomInteger(const T& min, const T& max)
{
  const T range = max - min;
  return (qrand() % (range + 1)) + min;
}


inline qreal randomReal(const qreal& min, const qreal& max)
{
  const qreal range = max - min;
  const qreal normalized = (qreal)qrand() / (qreal)RAND_MAX;
  return (normalized * range) + min;
}


inline qreal between(const qreal& min, const qreal& value, const qreal& max)
{
  if (value > max)
  {
    return max;
  }
  if (value < min)
  {
    return min;
  }
  return value;
}


template <typename T>
inline T linearInterpolate(const qreal& factor, const T& from, const T& to)
{
  return ((1.0 - factor) * from) + (factor * to);
}


class EyeSimulation : public QObject
{
  Q_OBJECT
public:

  struct State
  {
    QPointF lookPosLeft;
    QPointF lookPosRight;
    qreal blinkLevel;
    bool requestUpdate;
    bool motionDetected;
  };


  EyeSimulation();
  virtual ~EyeSimulation();


public:

  void simulationStep();
  bool eyeBlinkingEnabled() const
  {
    return blinkingSimulationEnabled;
  }


  bool eyeMovementEnabled() const
  {
    return movementSimulationEnabled;
  }


  State state;

private:

  bool verbose;
  bool movementSimulationEnabled;
  bool eyeIsMoving;
  qreal eyeMoveTotalTimeMs;
  QElapsedTimer eyeMoveTimer;
  QPointF eyeMovementStart;
  QPointF eyeMovementDestination;
  QTimer nextEyeMovementTimer;
  uint32_t minEyeMovementPauseMs;
  uint32_t maxEyeMovementPauseMs;

  bool blinkingSimulationEnabled;
  bool eyeIsBlinking;
  qreal eyeBlinkTotalTimeMs;
  QElapsedTimer eyeBlinkTimer;
  qreal eyeBlinkStart;
  qreal eyeBlinkDestination;
  QTimer nextEyeBlinkTimer;
  uint32_t minEyeBlinkPauseMs;
  uint32_t maxEyeBlinkPauseMs;

public slots:

  void startEyeBlinking();
  void stopEyeBlinking();

  void startEyeMovement();
  void stopEyeMovement();
};


// ostream, << overloading
inline QDataStream& operator<<(QDataStream &out, const EyeSimulation::State& s)
{
  out << s.lookPosLeft.x() << s.lookPosLeft.y();
  out << s.lookPosRight.x() << s.lookPosRight.y();
  out << s.blinkLevel << s.requestUpdate << s.motionDetected;
  return out;
}


// istream, >> overloading
inline QDataStream& operator>>(QDataStream &in, EyeSimulation::State& s)
{
  in >> s.lookPosLeft.rx() >> s.lookPosLeft.ry();
  in >> s.lookPosRight.rx() >> s.lookPosRight.ry();
  in >> s.blinkLevel >> s.requestUpdate >> s.motionDetected;
  return in;
}


#endif
