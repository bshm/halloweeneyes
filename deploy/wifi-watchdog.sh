#!/bin/bash
# Keep wlan0 honest: if the gateway stops answering, cycle the WiFi profile.
# Without this a board can stay powered but silently unreachable.
set -u

GW=${GW:-192.168.42.253}
CON=${CON:-TODOFixLater}
INTERVAL=${INTERVAL:-30}
FAILS=${FAILS:-3}

fails=0
while true; do
  sleep "$INTERVAL"
  if ping -c 1 -W 3 -I wlan0 "$GW" >/dev/null 2>&1; then
    fails=0
    continue
  fi
  fails=$((fails + 1))
  echo "wifi-watchdog: gateway unreachable (${fails}/${FAILS})"
  if [ "$fails" -ge "$FAILS" ]; then
    echo "wifi-watchdog: cycling ${CON}"
    nmcli con down "$CON" >/dev/null 2>&1
    sleep 2
    nmcli con up "$CON" >/dev/null 2>&1
    fails=0
  fi
done
