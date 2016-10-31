/* Copyright (c) 2016 Bastian Schmitz
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

#ifndef QTMOTION_H_INCLUDED
#define QTMOTION_H_INCLUDED

#include <QObject>
#include <QTimer>
#include <QHostAddress>
#include <QtNetwork>

#include "EyeSimulation.h"
#include "QtMotionTracking.h"

class QtMotion : public QObject
{
  Q_OBJECT

public:

  QtMotion(const QString& source_, const QString& dest_);
  virtual ~QtMotion();

public slots:

  void simulationStep();
  void switchToSimulation();

  void close();
  void objectDetected(int x, int y);

private:

  QTimer simulationTimer;
  EyeSimulation es;

  QtMotionTracking mt;

  QTimer switchToSimulationTimer;
  QUdpSocket udpSocket;
  QHostAddress groupAddress;
};


#endif
