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

#ifndef QT_EYE_H_INCLUDED
#define QT_EYE_H_INCLUDED

#include "arthurwidgets.h"

#include <QBasicTimer>
#include <QPolygonF>
#include <QTime>
#include <QTimer>
#include <QElapsedTimer>
#include <QHostAddress>
#include <QtNetwork>
#include "EyeSimulation.h"

class QtEyeView : public ArthurFrame
{
  Q_OBJECT

public:
  QtEyeView(QWidget* parent, bool leftEye_, bool rotated_, const QString& iris);
  virtual ~QtEyeView() { }

  virtual void paint(QPainter*);

  QSize sizeHint() const
  {
    return QSize(800, 600);
  }


  bool animation() const
  {
    return timer.isActive();
  }


public slots:
  void setAnimation(bool animate);
  void reset();
  void simulationStep();
  void processPendingDatagrams();

protected:


  virtual void mousePressEvent(QMouseEvent* e);
  virtual void mouseMoveEvent(QMouseEvent* event);
  virtual void resizeEvent(QResizeEvent* e);


  void wheelEvent(QWheelEvent*);

private:
  QPixmap m_bgPixmap;
  QPixmap m_irisPixmap;
  QTimer timer;

  bool rotated;
  bool leftEye;
  QPointF viewSize;
  QPointF center;
  qreal viewRotationDeg;
  qreal viewMirrorFactor;
  qreal viewScaleFactor;
  QTransform basicTrandform;

  QPointF irisCenter;


  QTime m_time;
  int m_frameCount;

  QUdpSocket udpSocket;
  QHostAddress groupAddress;
  EyeSimulation es;
};


class QtEyeWidget : public QWidget
{
  Q_OBJECT
public:
  QtEyeWidget(QWidget* parent, bool leftEye, bool rotated, const QString& iris);

private:
  QtEyeView* view;
};


#endif  // QtEye_H
