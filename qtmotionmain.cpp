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

#include <QCoreApplication>
#include <QDebug>
#include <QStringList>
#include <QRegExp>

#include "CtrlCHandler.h"
#include "QtMotion.h"

extern "C"
{
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavformat/avio.h>
}

int main(int argc, char** argv)
{
  avformat_network_init();

  QCoreApplication app(argc, argv);
  const QStringList args = app.arguments();

  QObject::connect(CtrlCHandler::instance(), SIGNAL(activated()), &app, SLOT(quit()));
  CtrlCHandler::instance()->install();

  const QRegExp rxArgsMirrored("--mirrored");
  bool mirrored = false;

  const QRegExp rxArgsRotated("--rotated");
  bool rotated = false;


  for (int i = 1; i < args.size(); ++i)
  {
    if (rxArgsMirrored.indexIn(args.at(i)) != -1 )
    {
      mirrored = true;
    }
    else if (rxArgsRotated.indexIn(args.at(i)) != -1 )
    {
      rotated = true;
    }
    else
    {
      qDebug() << "Unknown command line argument:" << args.at(i);
    }
  }

  QtMotion qtm(args.at(1), args.at(2));


  const int exitCode = app.exec();

  CtrlCHandler::instance()->uninstall();
  qDebug("Exiting with code %u", exitCode);
  return exitCode;
}
