/****************************************************************************
**
** Copyright (C) 2012 Nokia Corporation and/or its subsidiary(-ies).
** Contact: http://www.qt-project.org/
**
** This file is part of the demonstration applications of the Qt Toolkit.
**
** $QT_BEGIN_LICENSE:LGPL$
** GNU Lesser General Public License Usage
** This file may be used under the terms of the GNU Lesser General Public
** License version 2.1 as published by the Free Software Foundation and
** appearing in the file LICENSE.LGPL included in the packaging of this
** file. Please review the following information to ensure the GNU Lesser
** General Public License version 2.1 requirements will be met:
** http://www.gnu.org/licenses/old-licenses/lgpl-2.1.html.
**
** In addition, as a special exception, Nokia gives you certain additional
** rights. These rights are described in the Nokia Qt LGPL Exception
** version 1.1, included in the file LGPL_EXCEPTION.txt in this package.
**
** GNU General Public License Usage
** Alternatively, this file may be used under the terms of the GNU General
** Public License version 3.0 as published by the Free Software Foundation
** and appearing in the file LICENSE.GPL included in the packaging of this
** file. Please review the following information to ensure the GNU General
** Public License version 3.0 requirements will be met:
** http://www.gnu.org/copyleft/gpl.html.
**
** Other Usage
** Alternatively, this file may be used in accordance with the terms and
** conditions contained in a signed written agreement between you and Nokia.
**
**
**
**
**
**
** $QT_END_LICENSE$
**
****************************************************************************/

#include "arthurwidgets.h"
#include <QApplication>
#include <QPainter>
#include <QPainterPath>
#include <QPixmapCache>
#include <QtEvents>
#include <QTextDocument>
#include <QAbstractTextDocumentLayout>
#include <QFile>
#include <QTextBrowser>
#include <QBoxLayout>

#include <private/qpixmapdata_p.h>

extern QPixmap cached(const QString &img);

ArthurFrame::ArthurFrame(QWidget* parent)
  : QWidget(parent)
  , m_prefer_image(false)
{
#ifdef QT_OPENGL_SUPPORT
  glw = 0;
  m_use_opengl = false;
  QGLFormat f = QGLFormat::defaultFormat();
  f.setSampleBuffers(true);
  f.setStencil(true);
  f.setAlpha(true);
  f.setAlphaBufferSize(8);
  QGLFormat::setDefaultFormat(f);
#endif

#ifdef Q_WS_X11
  QPixmap xRenderPixmap(1, 1);
  m_prefer_image = xRenderPixmap.pixmapData()->classId() == QPixmapData::X11Class && !xRenderPixmap.x11PictureHandle();
#endif
}


#ifdef QT_OPENGL_SUPPORT
void ArthurFrame::enableOpenGL(bool use_opengl)
{
  m_use_opengl = use_opengl;

  if (!glw)
  {
    glw = new GLWidget(this);
    glw->setAutoFillBackground(false);
    glw->disableAutoBufferSwap();
    QApplication::postEvent(this, new QResizeEvent(size(), size()));
  }

  if (use_opengl)
  {
    glw->show();
  }
  else
  {
    glw->hide();
  }

  update();
}


#endif

void ArthurFrame::paintEvent(QPaintEvent* e)
{
#ifdef Q_WS_QWS
  static QPixmap* static_image = 0;
#else
  static QImage* static_image = 0;
#endif
  QPainter painter;
  if (preferImage()
#ifdef QT_OPENGL_SUPPORT
      && !m_use_opengl
#endif
     )
  {
    if (!static_image || static_image->size() != size())
    {
      delete static_image;
#ifdef Q_WS_QWS
      static_image = new QPixmap(size());
#else
      static_image = new QImage(size(), QImage::Format_RGB32);
#endif
    }
    painter.begin(static_image);

    int o = 10;

    QBrush bg = palette().brush(QPalette::Background);
    painter.fillRect(0, 0, o, o, bg);
    painter.fillRect(width() - o, 0, o, o, bg);
    painter.fillRect(0, height() - o, o, o, bg);
    painter.fillRect(width() - o, height() - o, o, o, bg);
  }
  else
  {
#ifdef QT_OPENGL_SUPPORT
    if (m_use_opengl)
    {
      painter.begin(glw);
      painter.fillRect(QRectF(0, 0, glw->width(), glw->height()), palette().color(backgroundRole()));
    }
    else
    {
      painter.begin(this);
    }
#else
    painter.begin(this);
#endif
  }

  painter.setClipRect(e->rect());

  painter.setRenderHint(QPainter::Antialiasing);

  QPainterPath clipPath;

  QRect r = rect();
  qreal left = r.x() + 1;
  qreal top = r.y() + 1;
  qreal right = r.right();
  qreal bottom = r.bottom();
  qreal radius2 = 8 * 2;

  clipPath.moveTo(right - radius2, top);
  clipPath.arcTo(right - radius2, top, radius2, radius2, 90, -90);
  clipPath.arcTo(right - radius2, bottom - radius2, radius2, radius2, 0, -90);
  clipPath.arcTo(left, bottom - radius2, radius2, radius2, 270, -90);
  clipPath.arcTo(left, top, radius2, radius2, 180, -90);
  clipPath.closeSubpath();

  painter.save();
  //painter.setClipPath(clipPath, Qt::IntersectClip);

  //painter.drawTiledPixmap(rect(), m_tile);

  // client painting

  paint(&painter);

  painter.restore();


#if 0
  int level = 180;
  painter.setPen(QPen(QColor(level, level, level), 2));
  painter.setBrush(Qt::NoBrush);
  painter.drawPath(clipPath);
#endif

  if (preferImage()
#ifdef QT_OPENGL_SUPPORT
      && !m_use_opengl
#endif
     )
  {
    painter.end();
    painter.begin(this);
#ifdef Q_WS_QWS
    painter.drawPixmap(e->rect(), *static_image, e->rect());
#else
    painter.drawImage(e->rect(), *static_image, e->rect());
#endif
  }

#ifdef QT_OPENGL_SUPPORT
  if (m_use_opengl &&
      (inherits("PathDeformRenderer") || inherits("PathStrokeRenderer") || inherits("CompositionRenderer")))
  {
    glw->swapBuffers();
  }
#endif
}


void ArthurFrame::resizeEvent(QResizeEvent* e)
{
#ifdef QT_OPENGL_SUPPORT
  if (glw)
  {
    glw->setGeometry(0, 0, e->size().width()-1, e->size().height()-1);
  }
#endif
  QWidget::resizeEvent(e);
}

