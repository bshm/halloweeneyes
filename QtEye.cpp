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

#include <QLayout>
#include <QPainter>
#include <QPainterPath>
#include <QWheelEvent>
#include <QtCore/qmath.h>
#include <QColor>

#include "QtEye.h"

int* unused = new int;

QtEyeView::QtEyeView(QWidget* parent, bool leftEye_, bool rotated_, const QString& iris)
  : ArthurFrame(parent),
  m_frameCount(0)
{
  setAttribute(Qt::WA_MouseTracking);
  leftEye = leftEye_;
  rotated = rotated_;


  m_bgPixmap = QPixmap("BG.png");
  m_irisPixmap = QPixmap(QString::fromLatin1("%1").arg(iris));
  irisCenter = QPointF(998, 998);


  viewScaleFactor = 1.0;
  viewMirrorFactor = 1.0;
  if (rotated)
  {
    viewRotationDeg = 90.0;
    viewSize = QPointF(600, 800);
  }
  else
  {
    viewRotationDeg = 0.0;
    viewSize = QPointF(800, 600);
  }
  if (leftEye)
  {
    viewMirrorFactor = 1.0;
  }
  else
  {
    viewMirrorFactor = -1.0;
  }
  center = viewSize / 2;


  setMinimumSize(viewSize.rx(), viewSize.ry() );
  setMaximumSize(viewSize.rx(), viewSize.ry() );
  setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);


  QPalette palette = this->palette();
  palette.setColor( backgroundRole(), QColor( 0, 0, 0 ) );
  setPalette( palette );
  setAutoFillBackground( true );

  // this is ignored
  //basicTrandform.translate( center.x(), center.y());
  basicTrandform.translate( irisCenter.x(), irisCenter.y());
  basicTrandform.scale(viewMirrorFactor, 1.0);
  basicTrandform.rotate(viewRotationDeg);
  basicTrandform.scale(viewScaleFactor, viewScaleFactor);
  basicTrandform.translate( -irisCenter.x(), -irisCenter.y());

  m_bgPixmap = m_bgPixmap.transformed(basicTrandform);
  m_irisPixmap = m_irisPixmap.transformed(basicTrandform);


  connect(&timer, SIGNAL(timeout()), this, SLOT(simulationStep()));

  groupAddress = QHostAddress("239.255.43.21");

  udpSocket.bind(45454, QUdpSocket::ShareAddress);
  udpSocket.joinMulticastGroup(groupAddress);
  connect(&udpSocket, SIGNAL(readyRead()),
          this, SLOT(processPendingDatagrams()));
}


void QtEyeView::processPendingDatagrams()
{
  while (udpSocket.hasPendingDatagrams())
  {
    QByteArray datagram;
    datagram.resize(udpSocket.pendingDatagramSize());
    udpSocket.readDatagram(datagram.data(), datagram.size());   //TODO error checking

    {
      QDataStream stream( &datagram, QIODevice::ReadOnly );
      stream >> es.state;
      if (es.state.motionDetected)
      {
        qDebug("motionDetected at %lf, %lf", es.state.lookPosLeft.rx(), es.state.lookPosLeft.ry());
      }
    }

    setAnimation(false);
  }

  if (es.state.requestUpdate)
  {
    update();
  }
}


void QtEyeView::mousePressEvent(QMouseEvent* event)
{
  if (event->button() == Qt::LeftButton)
  {
    setAnimation(false);
    es.state.lookPosLeft = es.state.lookPosRight = QPointF(event->posF().rx() / viewSize.rx() * 2.0 - 1.0,
                                                           event->posF().ry() / viewSize.ry() * 2.0 - 1.0);
    es.state.requestUpdate = true;
    update();
  }
  if (event->button() == Qt::RightButton)
  {
    setAnimation(false);
    es.state.blinkLevel = event->posF().ry() / viewSize.ry();
    es.state.requestUpdate = true;
    update();
  }
  if (event->button() == Qt::MiddleButton)
  {
    setAnimation(true);
  }
}


void QtEyeView::mouseMoveEvent(QMouseEvent* event)
{
  //printf("mouseMoveEvent\n");
}


void QtEyeView::resizeEvent(QResizeEvent* e)
{
  ArthurFrame::resizeEvent(e);
}


void QtEyeView::paint(QPainter* painter)
{
  painter->save();
  painter->setRenderHint(QPainter::Antialiasing);
  painter->setRenderHint(QPainter::SmoothPixmapTransform);


  const QPointF origin;
  QPointF irisMoveFactor(250.0, 150.0);
  QPointF bgMoveFactor(75.0, 50.0);
  QPointF lookPosTransformed;

  QPointF upperLidMovement;
  QPointF lowerLidMovement;
  if (leftEye)
  {
    lookPosTransformed = es.state.lookPosLeft;
  }
  else
  {
    lookPosTransformed = es.state.lookPosRight;
  }
  if (rotated)
  {
    upperLidMovement = QPointF(375, 0);
    lowerLidMovement = QPointF(-125, 0);
  }
  else
  {
    upperLidMovement = QPointF(0, 375);
    lowerLidMovement = QPointF(0, -125);
  }

  const QPointF imageTranslation = -irisCenter + center;
  const QPointF irisLookTranslation =  imageTranslation + QPointF(
    lookPosTransformed.x() * irisMoveFactor.x(), lookPosTransformed.y() * irisMoveFactor.y());
  const QPointF bgLookTranslation = imageTranslation +  QPointF(
    lookPosTransformed.x() * bgMoveFactor.x(), lookPosTransformed.y() * bgMoveFactor.y());
  const QPointF upperLidTranslation = bgLookTranslation + upperLidMovement * es.state.blinkLevel;
  const QPointF lowerLidTranslation = bgLookTranslation + lowerLidMovement * es.state.blinkLevel;

  painter->save();
  painter->drawPixmap(irisLookTranslation, m_irisPixmap);
  painter->drawPixmap(upperLidTranslation, m_bgPixmap);
  painter->drawPixmap(lowerLidTranslation, m_bgPixmap);
  painter->drawPixmap(bgLookTranslation, m_bgPixmap);
  painter->restore();

  if (m_frameCount == 0)
  {
    m_time.start();
  }
  else
  {
    if (m_frameCount % 100 == 0)
    {
      printf("FPS: %f\n", float(m_frameCount)/m_time.elapsed()*1000);
    }
  }
  m_frameCount++;

  painter->restore();
}


void QtEyeView::setAnimation(bool animate)
{
  timer.stop();
  if (animate)
  {
    timer.start(10);
  }
}


void QtEyeView::simulationStep()
{
  es.simulationStep();


  if (es.state.requestUpdate)
  {
    update();
  }
}


void QtEyeView::wheelEvent(QWheelEvent* e)
{ }


void QtEyeView::reset()
{
  update();
}


QtEyeWidget::QtEyeWidget(QWidget* parent, bool leftEye, bool rotated, const QString& iris)
  : QWidget(parent)
{
  view = new QtEyeView(this, leftEye, rotated, iris);


  QHBoxLayout* viewLayout = new QHBoxLayout(this);
  viewLayout->addWidget(view);
  viewLayout->setMargin(0);

  view->setAnimation(true);
  view->setMouseTracking(true);


  // defaults
  view->reset();
}



