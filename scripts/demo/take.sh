#!/bin/bash
# Needs: ydotoold running as root with --socket-path=$XDG_RUNTIME_DIR/.ydotool_socket --socket-own=$(id -u):$(id -g),
# mouse accel-profile flat + speed 0 (restore after), emulator paired and mirrored, windows placed by hand.
# Coordinates are for a 2048x1152 logical monitor at x=1920; adjust for other screens.
# take.sh light|dark : one real screen recording of Continuum + phone mirroring.
S=${S:-/tmp/continuum-demo}
source $S/m.sh >/dev/null
T=$1; export ANDROID_SERIAL=emulator-5562
if [ $T = dark ]; then gsettings set org.gnome.desktop.interface color-scheme prefer-dark; adb shell cmd uimode night yes >/dev/null
else gsettings set org.gnome.desktop.interface color-scheme default; adb shell cmd uimode night no >/dev/null; fi
adb shell input keyevent KEYCODE_HOME; sleep 0.5; adb shell monkey -p com.google.android.apps.photos -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1; sleep 3; adb shell uiautomator dump /sdcard/u.xml >/dev/null; adb shell cat /sdcard/u.xml | grep -q 'Not now' && adb shell input tap 672 2794 && sleep 2; adb shell input keyevent KEYCODE_BACK; sleep 1; adb shell input keyevent KEYCODE_HOME
jump 600 700 >/dev/null; sleep 3
OUT=$S/take-$T.webm; rm -f $OUT
/usr/bin/python3 $S/rec.py 2020 80 1780 1001 "$OUT" & REC=$!
sleep 1.5
# desktop gallery: scroll down and back up
for i in 1 2 3 4; do ydotool mousemove -w -x 0 -y -1 >/dev/null; sleep 0.35; done; sleep 0.8
for i in 1 2 3 4; do ydotool mousemove -w -x 0 -y 1 >/dev/null; sleep 0.3; done; sleep 0.8
# to the phone: open Photos
glide 1722 728 40 >/dev/null; sleep 0.5; click >/dev/null; sleep 3
# open the first photo
glide 1544 342 30 >/dev/null; sleep 0.4; click >/dev/null; sleep 2.5
# swipe to the next photos with mouse drags
for k in 1 2; do glide 1800 565 20 >/dev/null; down >/dev/null; glide 1560 565 18 >/dev/null; up >/dev/null; sleep 2; done
# swipe up from the bottom: home
glide 1680 950 25 >/dev/null; down >/dev/null; glide 1680 640 6 >/dev/null; up >/dev/null; sleep 2.5
glide 1300 800 30 >/dev/null; sleep 1
kill -TERM $REC; wait $REC
echo "take $T: $(ffprobe -v error -show_entries format=duration -of csv=p=0 $OUT)s"
