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

#include "QtMotion.h"

QtMotion::QtMotion(const QString& source_, const QString& dest_)
{
  groupAddress = QHostAddress("239.255.43.21");

  simulationTimer.setInterval(10);
  connect(&simulationTimer, SIGNAL(timeout()), this, SLOT(simulationStep()));

  switchToSimulationTimer.setInterval(2000);
  switchToSimulationTimer.setSingleShot(true);
  connect(&switchToSimulationTimer, SIGNAL(timeout()), this, SLOT(switchToSimulation()));

  connect(QCoreApplication::instance(), SIGNAL(aboutToQuit()), this, SLOT(close()));
  connect(&mt, SIGNAL(objectDetected(int, int)), this, SLOT(objectDetected(int, int)));

  const QDateTime now = QDateTime::currentDateTime();
  const QString timestamp = now.toString(QLatin1String("yyyyMMdd-hhmmss"));
  const QString outFilename = dest_.arg(timestamp);

  mt.open(source_, outFilename);

  simulationTimer.start();
}


void QtMotion::close()
{
  mt.close();
}


QtMotion::~QtMotion()
{ }


void QtMotion::switchToSimulation()
{
  es.startEyeMovement();
}


void QtMotion::simulationStep()
{
  es.simulationStep();


  QByteArray buffer;
  {
    QDataStream stream( &buffer, QIODevice::WriteOnly );
    stream << es.state;
  }
  udpSocket.writeDatagram(buffer, groupAddress, 45454);
}


void QtMotion::objectDetected(int x, int y)
{
  es.stopEyeMovement();
  switchToSimulationTimer.start();   // restarts timer

  // transform from 320x240 -> 1280x1024
  const double x_1280 = x * 4.0;
  const double y_1024 = (y + 8.0) * 4.0;


  //transform to global coordinate system x: [-3..3] [m], y [0..12] [m]
  const double xEye = -2.10554+x_1280 * (0.00362959 -2.89324E-7 * x_1280-2.24893E-6 * y_1024)+0.00145656 * y_1024;
  const double yEye = -0.383554+x_1280 * (0.000500975 -3.04759E-7 * x_1280+1.12333E-7 * y_1024)+0.00225704 * y_1024;


  es.state.lookPosLeft = QPointF(between(-1.0, xEye, 1.0), between(-1.0, yEye, 1.0));
  es.state.lookPosRight = QPointF(between(-1.0, xEye, 1.0), between(-1.0, yEye, 1.0));
  es.state.requestUpdate = true;
  es.state.motionDetected = true;


  QByteArray buffer;
  {
    QDataStream stream( &buffer, QIODevice::WriteOnly );
    stream << es.state;
  }
  udpSocket.writeDatagram(buffer, groupAddress, 45454);
}