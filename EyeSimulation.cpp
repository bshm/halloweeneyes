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

#include "EyeSimulation.h"

EyeSimulation::EyeSimulation()
{
  verbose = false;
  state.blinkLevel = 0.0;
  state.requestUpdate = false;
  state.motionDetected = false;

  minEyeMovementPauseMs = 150;
  maxEyeMovementPauseMs = 3000;
  nextEyeMovementTimer.setSingleShot(true);
  nextEyeMovementTimer.setInterval(randomInteger(minEyeMovementPauseMs, maxEyeMovementPauseMs));
  connect(&nextEyeMovementTimer, SIGNAL(timeout()), this, SLOT(startEyeMovement()));


  minEyeBlinkPauseMs = 300;
  maxEyeBlinkPauseMs = 4000;
  nextEyeBlinkTimer.setSingleShot(true);
  nextEyeBlinkTimer.setInterval(randomInteger(minEyeBlinkPauseMs, maxEyeBlinkPauseMs));
  connect(&nextEyeBlinkTimer, SIGNAL(timeout()), this, SLOT(startEyeBlinking()));

  movementSimulationEnabled = true;
  blinkingSimulationEnabled = true;
}


EyeSimulation::~EyeSimulation()
{ }


void EyeSimulation::startEyeMovement()
{
  const uint32_t minEyeMovementTimeMs = 72;
  const uint32_t maxEyeMovementTimeMs = 144;

  eyeMoveTotalTimeMs = randomInteger(minEyeMovementTimeMs, maxEyeMovementTimeMs);
  eyeMoveTimer.restart();

  eyeMovementStart = state.lookPosLeft;
  eyeMovementDestination = QPointF(randomReal(-1.0, 1.0), randomReal(0.0, 1.0)); // don't look upwards

  if (verbose)
  {
    qDebug("eyeMovementStart=(%f,%f)\n", eyeMovementStart.x(), eyeMovementStart.y());
    qDebug("eyeMovementDestination=(%f,%f)\n", eyeMovementDestination.x(), eyeMovementDestination.y());
  }
  eyeIsMoving = true;
  state.requestUpdate = true;
  movementSimulationEnabled = true;
}


void EyeSimulation::stopEyeMovement()
{
  movementSimulationEnabled = false;
}


void EyeSimulation::startEyeBlinking()
{
  const uint32_t minEyeBlinkTimeMs = 40;
  const uint32_t maxEyeBlinkTimeMs = 150;

  eyeBlinkTotalTimeMs = randomInteger(minEyeBlinkTimeMs, maxEyeBlinkTimeMs);
  eyeBlinkTimer.restart();

  eyeBlinkStart = state.blinkLevel;
  // Eyes at most half closed, not more
  eyeBlinkDestination = randomReal(0.0, 0.5);

  if (verbose)
  {
    qDebug("eyeBlinkStart=%f\n", eyeBlinkStart);
    qDebug("eyeBlinkDestination=%f\n", eyeBlinkDestination);
  }

  eyeIsBlinking = true;
  state.requestUpdate = true;
  blinkingSimulationEnabled = true;
}


void EyeSimulation::stopEyeBlinking()
{
  blinkingSimulationEnabled = false;
}


void EyeSimulation::simulationStep()
{
  state.requestUpdate = false;
  state.motionDetected = false;

  if (eyeMovementEnabled())
  {
    if (eyeIsMoving)
    {
      if (eyeMoveTimer.hasExpired(eyeMoveTotalTimeMs))
      {
        eyeIsMoving = false;
        state.lookPosLeft = eyeMovementDestination;
        state.lookPosRight = eyeMovementDestination;
        state.requestUpdate = true;
      }
      else
      {
        const qreal factor = (double) (eyeMoveTimer.nsecsElapsed() / 1000000.0) / eyeMoveTotalTimeMs;
        const qreal smoothedFactor = (3.0*(factor*factor))-(2.0*(factor*factor*factor));
        state.lookPosLeft = linearInterpolate(smoothedFactor, eyeMovementStart, eyeMovementDestination);
        state.lookPosRight = linearInterpolate(smoothedFactor, eyeMovementStart, eyeMovementDestination);
        //printf("factor=%f, lookPos=(%f,%f)\n", factor, lookPos.x(), lookPos.y());
        state.requestUpdate = true;
      }
    }

    if (eyeIsMoving == false)
    {
      if (nextEyeMovementTimer.isActive() == false)
      {
        nextEyeMovementTimer.setInterval(randomInteger(minEyeMovementPauseMs, maxEyeMovementPauseMs));
        nextEyeMovementTimer.start();
      }
    }
  }

  if (eyeBlinkingEnabled())
  {
    if (eyeIsBlinking)
    {
      if (eyeBlinkTimer.hasExpired(eyeBlinkTotalTimeMs))
      {
        eyeIsBlinking = false;
        state.blinkLevel = eyeBlinkDestination;
        state.requestUpdate = true;
      }
      else
      {
        const qreal blinkProgress = (double) (eyeBlinkTimer.nsecsElapsed() / 1000000.0) / eyeBlinkTotalTimeMs;
        if (blinkProgress <= 0.3333333)
        {
          const qreal interpolationFactor = 3.0 * blinkProgress;
          state.blinkLevel = linearInterpolate(interpolationFactor, eyeBlinkStart, (qreal)1.0);
        }
        else
        {
          const qreal interpolationFactor = 1.5 * blinkProgress - 0.5;
          state.blinkLevel = linearInterpolate(interpolationFactor, (qreal)1.0, eyeBlinkDestination);
        }
        //printf("blinkProgress=%f, blinkLevel=%f \n", blinkProgress, blinkLevel);
        state.requestUpdate = true;
      }
    }

    if (eyeIsBlinking == false)
    {
      if (nextEyeBlinkTimer.isActive() == false)
      {
        nextEyeBlinkTimer.start();
      }
    }

    // Add little random uniform noise to blinkLevel
    //TODO make this timer dependent, instead of step depedendant
    state.blinkLevel = between(0.0, state.blinkLevel + randomReal(-0.005, 0.005), 1.0);
  }
  state.requestUpdate = true;
}

