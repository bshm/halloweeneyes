#!/bin/bash
sleep 5
systemctl stop lightdm.service
killall -9 Xorg
killall -9 qteye
Xorg  -nocursor -dpms &
export DISPLAY=:0.0
xset -dpms &
xset s noblank &
xset s off &
cd /home/debian/build/augen/motiontrackingtutorial
./qteye

