# source me. Mouse helpers in logical coords of the right monitor (2048x1152 at x=1920).
export YDOTOOL_SOCKET=/run/user/1000/.ydotool_socket
CX=2047; CY=1151
corner(){ for i in 1 2 3; do ydotool mousemove -x 4000 -y 4000; done; CX=2047; CY=1151; }
jump(){ corner; ydotool mousemove -x $(( $1-2047 )) -y $(( $2-1151 )); CX=$1; CY=$2; }
# glide X Y [steps]: smooth move from current
glide(){ local n=${3:-30} i dx dy px=$CX py=$CY; for i in $(seq 1 $n); do
  nx=$(python3 -c "import math;t=$i/$n;e=(1-math.cos(math.pi*t))/2;print(round($px+($1-$px)*e))"); ny=$(python3 -c "import math;t=$i/$n;e=(1-math.cos(math.pi*t))/2;print(round($py+($2-$py)*e))")
  ydotool mousemove -x $((nx-CX)) -y $((ny-CY)); CX=$nx; CY=$ny; sleep 0.012; done; }
click(){ ydotool click 0xC0; }
down(){ ydotool click 0x40; }
up(){ ydotool click 0x80; }
cap(){ gdbus call --session --dest org.gnome.Shell.Screencast --object-path /org/gnome/Shell/Screencast --method org.gnome.Shell.Screencast.ScreencastArea 1920 0 2048 1152 "$S/c.webm" "{'draw-cursor': <true>, 'framerate': <int32 10>}" >/dev/null; sleep 1; gdbus call --session --dest org.gnome.Shell.Screencast --object-path /org/gnome/Shell/Screencast --method org.gnome.Shell.Screencast.StopScreencast >/dev/null; ffmpeg -loglevel error -y -i $S/c.webm -frames:v 1 -vf scale=1280:-1 $S/$1.png; }
