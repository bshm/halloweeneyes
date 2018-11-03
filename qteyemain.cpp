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

#include <QApplication>
#include <QDebug>

#include "QtEye.h"

int main(int argc, char** argv)
{
  QApplication app(argc, argv);
  const QStringList args = app.arguments();
  bool leftEye = true;
  bool mirrored = false;
  QString iris("IRIS_RED.png");

  const QRegExp rxArgsLeft("--left");
  const QRegExp rxArgsRight("--right");
  const QRegExp rxArgsRotated("--rotated");
  const QRegExp rxArgsIris("--iris");
  bool rotated = false;


  for (int i = 1; i < args.size(); ++i)
  {
    if (rxArgsLeft.indexIn(args.at(i)) != -1 )
    {
      leftEye = true;
    }
    else if (rxArgsRight.indexIn(args.at(i)) != -1 )
    {
      leftEye = false;
    }
    else if (rxArgsRotated.indexIn(args.at(i)) != -1 )
    {
      rotated = true;
    }
    else if (rxArgsIris.indexIn(args.at(i)) != -1 )
    {
      iris = args.at(i+1);
    }
    else
    {
      qDebug() << "Unknown command line argument:" << args.at(i);
    }
  }

  QtEyeWidget QtEyeWidget(NULL, leftEye, rotated, iris);
  QtEyeWidget.show();

  return app.exec();
}

