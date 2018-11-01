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

#include "CtrlCHandler.h"

#include <QMutex>
#include <QDebug>
#include <cassert>
#include <stdint.h>

#ifdef Q_OS_WIN32
#include "qwineventnotifier_p.h"
#include <windows.h>
#endif

#ifdef Q_OS_UNIX
#include <QSocketNotifier>
#include <sys/socket.h>
#include <unistd.h>
#endif

#include <QCoreApplication>
#include <QLocalSocket>

// Definition of singelton pointer.
CtrlCHandler* CtrlCHandler::instancePtr = NULL;


CtrlCHandler* CtrlCHandler::instance ()
{
  static QMutex mutex;
  if (instancePtr == NULL)
  {
    mutex.lock();

    if (instancePtr == NULL)
    {
      instancePtr = new CtrlCHandler ();
    }

    mutex.unlock();
  }

  return instancePtr;
}


CtrlCHandler::~CtrlCHandler ()
{
  uninstall ();
}


#ifdef Q_OS_WIN32

BOOL WINAPI CtrlCHandler::ctrlHandler (DWORD dwCtrlType)
{
  BOOL result = TRUE;

  switch ( dwCtrlType )
  {
    // Handle all kinds of signals
    case CTRL_C_EVENT:
    case CTRL_CLOSE_EVENT:
    case CTRL_BREAK_EVENT:
    case CTRL_LOGOFF_EVENT:
    case CTRL_SHUTDOWN_EVENT:

      if (::WaitForSingleObject (CtrlCHandler::instance()->quitEvent, 0) != WAIT_OBJECT_0)
      {
        (void)::SetEvent (CtrlCHandler::instance()->quitEvent);
      }
      break;

    default:
      assert (false);
      break;
  }

  return result;
}


#endif


#ifdef Q_OS_UNIX

void CtrlCHandler::signalHandler (int signal)
{
  const uint8_t dummy = 0;

  switch (signal)
  {
    case SIGINT:
    case SIGTERM:
    case SIGHUP:

      (void)::write(CtrlCHandler::instance()->sigFd[0], &dummy, sizeof(dummy));

      break;

    default:
      break;
  }
}


#endif


bool CtrlCHandler::install ()
{
  qDebug("ApplicationPid is %u", QCoreApplication::applicationPid());
  localServer.listen (QString::fromLatin1("PID%1").arg(QCoreApplication::applicationPid()));
  QObject::connect (&localServer, SIGNAL(newConnection()), this, SIGNAL(activated()));

#ifdef Q_OS_WIN32
  quitEvent = ::CreateEvent (NULL, TRUE, FALSE, NULL);
  ctrlCReceivedNotifier = new QWinEventNotifier (quitEvent, NULL);

  QObject::connect (ctrlCReceivedNotifier, SIGNAL(activated(HANDLE)), this, SIGNAL(activated()));
  ctrlCReceivedNotifier->setEnabled (true);
  if (::SetConsoleCtrlHandler (&ctrlHandler, TRUE) == FALSE)
  {
    qDebug("Failed to register console control handler.");
    return false;
  }
#endif

#ifdef Q_OS_UNIX
  if (::socketpair(AF_UNIX, SOCK_STREAM, 0, sigFd))
  {
    qDebug("Couldn't create signalling socketpair.");
    return false;
  }

  socketNotifier = new QSocketNotifier(sigFd[1], QSocketNotifier::Read, this);
  if (!connect (socketNotifier, SIGNAL(activated(int)), this, SIGNAL(activated())))
  {
    qDebug("Couldn't connect socket notifier.");
    return false;
  }

  struct sigaction term;
  term.sa_handler = &CtrlCHandler::signalHandler;
  term.sa_flags = 0;
  term.sa_restorer = NULL;
  sigemptyset (&term.sa_mask);
  term.sa_flags |= SA_RESTART;

  if (::sigaction(SIGINT, &term, &CtrlCHandler::instance()->previous_SIGINT) > 0)
  {
    qDebug ("Unable to install SIGINT signal handler.");
    return false;
  }
  if (::sigaction(SIGTERM, &term, &CtrlCHandler::instance()->previous_SIGTERM) > 0)
  {
    qDebug ("Unable to install SIGTERM signal handler.");
    // in this case, try to restore already installed SIGINT handler
    (void) sigaction(SIGINT, &previous_SIGINT, NULL);
    return false;
  }
  if (::sigaction(SIGHUP, &term, &CtrlCHandler::instance()->previous_SIGHUP) > 0)
  {
    qDebug ("Unable to install SIGHUB signal handler.");
    // in this case, try to restore already installed SIGINT and SIGTERM handlers
    (void) sigaction(SIGINT, &previous_SIGINT, NULL);
    (void) sigaction(SIGINT, &previous_SIGTERM, NULL);
    return false;
  }
#endif
  is_installed = true;
  return true;
}


void CtrlCHandler::uninstall()
{
  if (is_installed)
  {
#ifdef Q_OS_WIN32
    Q_UNUSED (::SetConsoleCtrlHandler (&CtrlCHandler::ctrlHandler, FALSE));
    Q_UNUSED (::CloseHandle (quitEvent));
    ctrlCReceivedNotifier->disconnect (SIGNAL(activated(HANDLE)));
    delete ctrlCReceivedNotifier;
    ctrlCReceivedNotifier = NULL;
#endif

#ifdef Q_OS_UNIX
    delete socketNotifier;
    socketNotifier = NULL;
    (void) close (sigFd[0]);
    sigFd[0] = -1;
    (void) close (sigFd[1]);
    sigFd[1] = -1;

    if (::sigaction (SIGTERM, &previous_SIGINT, NULL) != 0)
    {
      qDebug ("Unable to restore SIGINT signal handler.");
    }
    if (::sigaction (SIGTERM, &previous_SIGTERM, NULL) != 0)
    {
      qDebug ("Unable to restore SIGTERM signal handler.");
    }
    if (::sigaction (SIGTERM, &previous_SIGHUP, NULL) != 0)
    {
      qDebug ("Unable to restore SIGHUP signal handler.");
    }
#endif
    is_installed = false;
  }
}


CtrlCHandler::CtrlCHandler() : is_installed (false)
{
#ifdef Q_OS_WIN32
  ctrlCReceivedNotifier = NULL;
  quitEvent = NULL;
#endif
#ifdef Q_OS_UNIX
  socketNotifier = NULL;
  sigFd[0] = -1;
  sigFd[1] = -1;
#endif
}


void CtrlCHandler::requestProcessTermination(qint64 applicationPid)
{
  QLocalSocket socket;
  socket.connectToServer(QString::fromLatin1("PID%1").arg(applicationPid), QIODevice::WriteOnly);
  socket.close();
}
