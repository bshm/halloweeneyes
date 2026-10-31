#!/bin/sh
# halloween eyes kiosk session (X11)
xset s off
xset -dpms
xset s noblank
xrandr --output HDMI-1 --mode 1920x1080 >/dev/null 2>&1
exec /usr/bin/chromium \
  --kiosk --start-fullscreen --window-position=0,0 --window-size=1920,1080 \
  --force-device-scale-factor=1 \
  http://127.0.0.1:8080/ \
  --no-first-run --noerrdialogs --disable-infobars \
  --disable-session-crashed-bubble --password-store=basic \
  --check-for-update-interval=31536000 \
  --disable-features=Translate,MediaRouter,GlobalMediaControls,WebAppInstallation \
  --remote-debugging-port=9222 --remote-allow-origins=*
