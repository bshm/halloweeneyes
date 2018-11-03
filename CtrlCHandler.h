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


#ifndef CTRL_C_HANDLER_H_INCLUDED
#define CTRL_C_HANDLER_H_INCLUDED

#include <QtGlobal>
#include <QObject>
#include <QProcess>

#ifdef Q_OS_WIN32
// forward declarations, can use the pointer now...
class QWinEventNotifier;
#include <windows.h>
#endif

#ifdef Q_OS_UNIX
class QSocketNotifier;
#include <csignal>
#endif

#include <QLocalServer>

/**
   This class encapsulates catching UNIX signals SIGINT and SIGTERM and WIN32
   ConsoleControlEvents. Its purpose is to allow a daemon application to
   terminate in a clean fashion when these signals occur. This is especially
   important if there is unwritten data pending in memory buffers. It is
   singleton class because the OS signals can be only used once per process.
   Additionally it provides a static method to signal another process to
   terminate.
 */
class CtrlCHandler : public QObject
{
  Q_OBJECT
public:

  static CtrlCHandler* instance();
  virtual ~CtrlCHandler ();

  bool install ();
  void uninstall ();

  static void requestProcessTermination(qint64 applicationPid);

signals:

  /** This signal is emitted when CtrlC/CtrlBreak/Logout/CloseWin32/SIGINT/SIGTERM are caught. */
  void activated();

private:

  /** Singleton instance pointer, initialized to NULL. */
  static CtrlCHandler* instancePtr;

  /** Singleton constructor */
  explicit CtrlCHandler ();
  bool is_installed;

private:

  /** Termination socket for external notification. */
  QLocalServer localServer;

#ifdef Q_OS_WIN32

  /**
     Bridge object that translates the Windows event sent by the control
     handler into a Qt signal.
   */
  QWinEventNotifier* ctrlCReceivedNotifier;

  /** Windows style quit event. */
  HANDLE quitEvent;

  /** Windows style ctrlHandler that intercepts CtrlC. */
  static BOOL WINAPI ctrlHandler (DWORD dwCtrlType);
#endif

#ifdef Q_OS_UNIX
  /**
     Bridge object that translates data available on the socket pair into a Qt
     signal.
   */
  QSocketNotifier* socketNotifier;


  /**
     File descriptor pair that is used by socketpair().
     Initialized to -1.
   */
  int sigFd[2];


  /**
     Previously installed signal handler for SIGTERM.
     It is restored on uninstall().
   */
  struct sigaction previous_SIGTERM;


  /**
     Previously installed signal handler for SIGTERM.
     It is restored on uninstall().
   */
  struct sigaction previous_SIGINT;


  /**
     Previously installed signal handler for SIGHUP.
     It is restored on uninstall().
   */
  struct sigaction previous_SIGHUP;


  /** Signal handler function */
  static void signalHandler (int signal);
#endif

};


#endif // CTRL_C_HANDLER_H_INCLUDED

