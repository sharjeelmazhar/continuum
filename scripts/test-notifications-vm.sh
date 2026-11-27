#!/bin/bash
# SPDX-License-Identifier: GPL-3.0-or-later
# End-to-end test of transfer progress notifications on a GNOME VM (from scripts/test-distros.sh)
# with the Android emulator as the phone:
#   - transfers both ways over a throttled link (a realistic Wi-Fi speed),
#   - the shown speed matches the real one,
#   - clearing the notification in GNOME Shell (one, or "Clear all", several times) brings back
#     exactly one progress notification while the transfer runs,
#   - when it ends: one "Sent …"/"Received …" notification, no progress left behind.
#
#   scripts/test-notifications-vm.sh ubuntu|fedora     (VM running, emulator on adb)
#   VM_DIR=/big/disk ADB="/path/to/adb -s emulator-5562" BIN=desktop/target/release RATE_MBIT=32 SIZE_MB=300 ...
#
# Installs the given continuumd/continuum-ui into the VM's /usr/bin, and a test-only GNOME Shell
# extension that turns on unsafe mode there (so the test can read and clear notifications through
# org.gnome.Shell.Eval). Rerunnable: every step checks its state first. Results: stdout and
# $VM_DIR/<distro>/notifications.txt.
set -u
D=${1:?distro}
CONN=qemu:///system
VM_DIR=${VM_DIR:-$HOME/vms}/continuum-test
KEY=$VM_DIR/ssh_key
BIN=${BIN:-desktop/target/release}
RATE_MBIT=${RATE_MBIT:-32}
SIZE_MB=${SIZE_MB:-300}
PKG=io.github.sharjeelmazhar.continuum
ADB=${ADB:-adb -s ${ANDROID_SERIAL:-emulator-5554}}
OUT=$VM_DIR/$D/notifications.txt
: >"$OUT"

ip=$(virsh -c $CONN domifaddr "continuum-$D" | awk '/ipv4/{sub("/.*","",$4); print $4; exit}')
[ -n "$ip" ] || { echo "VM continuum-$D has no address (running?)"; exit 1; }
SSHO=(-q -i "$KEY" -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5)
vm() { ssh "${SSHO[@]}" "tester@$ip" "export XDG_RUNTIME_DIR=/run/user/\$(id -u) DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/\$(id -u)/bus; $*"; }
pass=0 fail=0
ok() { echo "PASS $*" | tee -a "$OUT"; pass=$((pass + 1)); }
bad() { echo "FAIL $*" | tee -a "$OUT"; fail=$((fail + 1)); }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

# JavaScript in GNOME Shell (needs unsafe mode); prints the returned string.
shell_js() {
  # gdbus reads arguments as GVariant text: pass the code as a quoted GVariant string.
  local js
  js=$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$1")
  vm "gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval $(printf %q "$js")" |
    python3 -c '
import sys, ast, re, json
r = re.sub(r"^\((true|false),", lambda m: "(" + m.group(1).title() + ",", sys.stdin.read().strip())
ok, v = ast.literal_eval(r) if r else (False, "no reply")
if not ok: print("ERR " + v); sys.exit()
try: v = json.loads(v)  # Eval returns its result JSON-encoded
except ValueError: pass
print(v if isinstance(v, str) else json.dumps(v))'
}
# Continuum's notifications in the shell: one JSON object per line {t, b}.
LIST_JS='JSON.stringify(Main.messageTray.getSources().filter(s => /Continuum/.test(s.title)).flatMap(s => s.notifications.map(n => ({t: n.title, b: n.body}))))'
notes() { shell_js "$LIST_JS"; }
progress_count() { notes | python3 -c 'import sys,json; print(sum("%" in n["b"] for n in json.loads(sys.stdin.read() or "[]")))' 2>/dev/null || echo -1; }
# Marks the progress notifications on screen now; "fresh" ones appeared after the mark.
mark_old() { shell_js 'for (const s of Main.messageTray.getSources()) for (const n of s.notifications) n._ctOld = true; "ok"' >/dev/null; }
fresh_count() { shell_js 'String(Main.messageTray.getSources().filter(s => /Continuum/.test(s.title)).flatMap(s => s.notifications).filter(n => /%/.test(n.body) && !n._ctOld).length)'; }
old_count() { shell_js 'String(Main.messageTray.getSources().flatMap(s => s.notifications).filter(n => n._ctOld).length)'; }
# The user clears: opens the date menu and clicks its "Clear" button; or dismisses one notification
# (what its close button and a swipe do).
clear_all() { shell_js 'const dm = Main.panel.statusArea.dateMenu; dm.menu.open(); dm._messageList._clearButton.emit("clicked", 1); dm.menu.close(); "cleared"'; }
clear_one() { shell_js 'let k = 0; for (const s of Main.messageTray.getSources()) if (/Continuum/.test(s.title)) for (const n of [...s.notifications]) if (/%/.test(n.body)) { n.destroy(2); k++; } String(k)'; }
# Bytes done in a progress body ("31% · 60.0 MB of …"), and the time now.
done_of() { python3 -c '
import sys,json,re,time
for n in json.loads(sys.stdin.read() or "[]"):
    m = re.search(r"% · ([\d.]+) (bytes|KB|MB|GB) of", n["b"])
    if m: print(float(m.group(1)) * {"bytes":1e-6,"KB":1e-3,"MB":1,"GB":1e3}[m.group(2)], time.time()); break'; }
# MB/s in a progress body ("… · 5.8 MB/s · …"), 0 if none.
speed_of() { python3 -c '
import sys,json,re
for n in json.loads(sys.stdin.read() or "[]"):
    m = re.search(r"([\d.]+) (bytes|KB|MB|GB)/s", n["b"])
    if m: print(float(m.group(1)) * {"bytes":1e-6,"KB":1e-3,"MB":1,"GB":1e3}[m.group(2)]); break
else: print(0)'; }

 # Centre of the first on-screen node whose text or description is $1.
ui_find() {
  $ADB exec-out uiautomator dump /dev/tty 2>/dev/null | python3 -c '
import sys,re
t = sys.stdin.read(); w = re.escape(sys.argv[1])
for m in re.finditer(r"<node [^>]*>", t):
    n = m.group(0)
    if re.search(r"(text|content-desc)=\"%s\"" % w, n):
        a,b,c,d = map(int, re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", n).groups())
        print((a+c)//2, (b+d)//2); break' "$1"
}
# Taps the on-screen button labelled $1 (a confirmation the user would tap).
tap_text() {
  local xy
  for _ in $(seq 10); do
    sleep 1
    xy=$($ADB exec-out uiautomator dump /dev/tty 2>/dev/null | python3 -c '
import sys,re
t = sys.stdin.read()
m = re.search(r"text=\"%s\"[^>]*bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]" % re.escape(sys.argv[1]), t)
if m: a,b,c,d = map(int, m.groups()); print((a+c)//2, (b+d)//2)' "$1")
    [ -n "$xy" ] && { $ADB shell input tap $xy; return 0; }
  done
  return 1
}

echo "== $D ($ip)" | tee -a "$OUT"

# 1. Install the build under test.
for b in continuumd continuum-ui; do
  [ -x "$BIN/$b" ] || { echo "missing $BIN/$b (cargo build --release)"; exit 1; }
done
if [ "$(vm "sha256sum /usr/bin/continuumd" | cut -d' ' -f1)" != "$(sha256sum "$BIN/continuumd" | cut -d' ' -f1)" ]; then
  scp "${SSHO[@]}" "$BIN/continuumd" "$BIN/continuum-ui" "tester@$ip:/tmp/" &&
    vm "sudo install -m755 /tmp/continuumd /tmp/continuum-ui /usr/bin/ && systemctl --user restart continuumd"
fi
check "daemon under test runs" '[ "$(vm "systemctl --user is-active continuumd")" = active ]'

# 2. Unsafe mode through a test-only extension (needs one shell restart, autologin brings it back).
vm "loginctl unlock-sessions; gsettings set org.gnome.desktop.screensaver lock-enabled false; gsettings set org.gnome.desktop.session idle-delay 0" 2>/dev/null
sleep 1
if [ "$(shell_js '"yes"')" != yes ]; then
  vm 'd=~/.local/share/gnome-shell/extensions/continuum-test@local; mkdir -p $d
cat > $d/metadata.json <<EOF
{"uuid": "continuum-test@local", "name": "Continuum test helper", "description": "Unsafe mode for tests", "shell-version": ["45","46","47","48","49","50","51","52"], "session-modes": ["user", "unlock-dialog"]}
EOF
cat > $d/extension.js <<EOF
import {Extension} from "resource:///org/gnome/shell/extensions/extension.js";
export default class T extends Extension { enable() { global.context.unsafe_mode = true; } disable() { global.context.unsafe_mode = false; } }
EOF
gsettings set org.gnome.shell disable-user-extensions false
cur=$(gsettings get org.gnome.shell enabled-extensions)
case $cur in *continuum-test@local*) ;; "@as []") gsettings set org.gnome.shell enabled-extensions "[\"continuum-test@local\"]" ;; *) gsettings set org.gnome.shell enabled-extensions "${cur%]*}, \"continuum-test@local\"]" ;; esac'
  # Automatic login happens at boot only (a gdm restart shows the login screen).
  vm 'sudo systemctl reboot' 2>/dev/null
  sleep 15
  for _ in $(seq 80); do sleep 3; [ "$(shell_js '"yes"' 2>/dev/null)" = yes ] && break; done
fi
# A locked screen (idle) would switch extensions off in the middle of the test.
vm "gsettings set org.gnome.desktop.screensaver lock-enabled false; gsettings set org.gnome.desktop.session idle-delay 0; loginctl unlock-sessions" 2>/dev/null
check "shell eval available" '[ "$(shell_js "\"yes\"")" = yes ]'
vm "systemctl --user is-active continuumd >/dev/null || systemctl --user start continuumd"

# 3. Paired and connected with the emulator.
devices() { vm "gdbus call --session --dest io.github.sharjeelmazhar.Continuum.Daemon --object-path /io/github/sharjeelmazhar/Continuum/Daemon --method io.github.sharjeelmazhar.Continuum.Daemon1.ListDevices"; }
connected_id() { devices | python3 -c '
import sys,re
t = sys.stdin.read()
for m in re.finditer(r"\(\x27([0-9a-f]+)\x27, \x27[^\x27]*\x27, \x27[^\x27]*\x27, true", t): print(m.group(1)); break' ; }
id=$(connected_id)
if [ -z "$id" ]; then
  uri=$(vm "gdbus call --session --dest io.github.sharjeelmazhar.Continuum.Daemon --object-path /io/github/sharjeelmazhar/Continuum/Daemon --method io.github.sharjeelmazhar.Continuum.Daemon1.StartPairing" | grep -o "continuum://[^']*")
  echo "pairing: $uri"
  $ADB shell am start -a android.intent.action.VIEW -d "$(printf %q "$uri")" "$PKG" >/dev/null
  tap_text Pair
  for _ in $(seq 40); do sleep 2; id=$(connected_id); [ -n "$id" ] && break; done
fi
check "emulator connected" '[ -n "$id" ]'
[ -n "$id" ] || { echo "devices: $(devices)"; exit 1; }

# 4. Throttle the VM's link: a realistic Wi-Fi speed, so the speed can be checked.
dev=$(vm "ip -o route get 192.168.122.1 | grep -o 'dev [^ ]*' | cut -d' ' -f2")
# Fedora's cloud image has no tc (package iproute-tc).
vm "command -v tc >/dev/null || sudo dnf -y -q install iproute-tc >/dev/null 2>&1 || sudo apt-get install -y -qq iproute2 >/dev/null 2>&1"
vm "sudo tc qdisc replace dev $dev root tbf rate ${RATE_MBIT}mbit burst 64kb latency 50ms"
# Incoming too (phone → computer): police it to the same rate.
vm "sudo tc qdisc del dev $dev ingress 2>/dev/null; sudo tc qdisc add dev $dev handle ffff: ingress && sudo tc filter add dev $dev parent ffff: protocol ip u32 match u32 0 0 police rate ${RATE_MBIT}mbit burst 1m drop flowid :1"
check "link throttled to $RATE_MBIT Mbit/s" '[ -n "$(vm "tc qdisc show dev $dev" | grep tbf)" ]'
want=$(python3 -c "print($RATE_MBIT/8)")

watch_transfer() { # $1 = label, $2 = title when done; watches progress, clears it 4 times, checks speed; until it ends
  local label=$1 seen=0 rounds=0 maxp=0 spd=0 n cur t0=0 truth
  # Ground truth on the side: the daemon's bytes done and speed twice a second.
  vm "for i in \$(seq 400); do echo \"\$(date +%s.%N) \$(gdbus call --session --dest io.github.sharjeelmazhar.Continuum.Daemon --object-path /io/github/sharjeelmazhar/Continuum/Daemon --method io.github.sharjeelmazhar.Continuum.Daemon1.Transfers)\"; sleep 0.5; done" >/tmp/ct-truth.txt 2>/dev/null &
  truth=$!
  for i in $(seq 1 240); do
    n=$(progress_count)
    [ "$n" -gt "$maxp" ] && maxp=$n
    [ "$n" -gt 1 ] && echo "  $label: $n progress notifications: $(notes)" | tee -a "$OUT"
    if [ "$n" = 1 ]; then
      seen=1
      [ "$t0" = 0 ] && t0=$SECONDS
      [ "$spd" = 0 ] && spd=$(notes | speed_of)
      if [ "$i" -gt 4 ] && [ $((i % 6)) = 0 ] && [ "$rounds" -lt 4 ]; then
        mark_old
        if [ $((rounds % 2)) = 0 ]; then clear_all >/dev/null; else clear_one >/dev/null; fi
        rounds=$((rounds + 1))
        # (The transfer may end right then: its result replaces the old one in place.)
        [ "$(old_count)" = 0 ] || notes | grep -q "\"t\":\"$2\"" || bad "$label: clear $rounds left the old notification"
        local back=0
        for _ in $(seq 1 10); do sleep 0.25; [ "$(fresh_count)" = 1 ] && [ "$(progress_count)" = 1 ] && { back=1; break; }; done
        cur=$(notes)
        if [ $back = 1 ]; then ok "$label: cleared ($rounds) → back as one new notification"
        elif echo "$cur" | grep -q "\"t\":\"$2\""; then ok "$label: cleared ($rounds) as it ended → its result shows"
        else bad "$label: cleared ($rounds) → not back in 2.5 s: $cur"; fi
      fi
    elif [ "$seen" = 1 ] && [ "$n" = 0 ]; then
      sleep 1.5
      [ "$(progress_count)" = 0 ] && break
    fi
    sleep 0.5
  done
  kill $truth 2>/dev/null
  # The speed shown must be the real one: compared with the bytes actually moved over the same
  # few seconds, at every sample (the average and the worst sample).
  local verdict
  verdict=$(python3 - /tmp/ct-truth.txt <<'PY'
import re, json, sys
rows = []
for l in open(sys.argv[1]):
    m = re.match(r"([\d.]+) \('(.*)',\)", l.strip())
    if m:
        j = json.loads(m.group(2).replace("\\'", "'"))
        if j: rows.append((float(m.group(1)), j[0]["done"], j[0]["rate"], j[0]["total"]))
ratios = []
for i, (t, d, r, tot) in enumerate(rows):
    prev = [x for x in rows[:i] if 3.0 <= t - x[0] <= 3.6]
    if not prev or r == 0 or d >= tot: continue
    true = (d - prev[-1][1]) / (t - prev[-1][0])
    if true > 0: ratios.append(r / true)
if len(ratios) < 10: print("FEW", len(ratios)); sys.exit()
mean = sum(ratios) / len(ratios)
inside = sum(0.75 <= x <= 1.33 for x in ratios) / len(ratios)
print("OK" if 0.9 <= mean <= 1.1 and inside >= 0.9 else "BAD", f"{len(ratios)} samples, mean ratio {mean:.3f}, {inside*100:.0f}% within 25%")
PY
)
  check "$label: speed is the real one ($verdict)" '[ "${verdict%% *}" = OK ]'
  check "$label: progress shown" '[ $seen = 1 ]'
  check "$label: never more than one progress notification (max $maxp)" '[ $maxp -le 1 ]'
  check "$label: cleared 4 times" '[ $rounds -ge 4 ]'
  check "$label: speed in the notification (${spd} MB/s, link ${want} MB/s)" "python3 -c 'import sys; sys.exit(0 if 0 < $spd <= 1.3*$want else 1)'"
}

# 5. Computer → phone.
vm "rm -f /tmp/big-up.bin; head -c ${SIZE_MB}M /dev/urandom > /tmp/big-up.bin"
$ADB shell rm -f /sdcard/Download/Continuum/big-up*.bin
clear_all >/dev/null
vm "continuumd send --to $id /tmp/big-up.bin" >/dev/null
watch_transfer "PC → phone" "Sent big-up.bin"
sleep 2
final=$(notes)
echo "after: $final" >>"$OUT"
check "PC → phone: one Sent notification" "echo '$final' | grep -q 'Sent big-up.bin'"
check "PC → phone: no progress left" '[ "$(progress_count)" = 0 ]'
check "PC → phone: file intact on phone" '[ "$($ADB shell sha256sum /sdcard/Download/Continuum/big-up.bin | cut -d" " -f1)" = "$(vm "sha256sum /tmp/big-up.bin" | cut -d" " -f1)" ]'

# 6. Phone → computer, shared the way a person does (Files → long-press → Share → Continuum):
#    Continuum only takes files the sharing app granted it, so adb can't share directly.
head -c "${SIZE_MB}M" /dev/urandom >/tmp/big-down.mp4
$ADB shell rm -f /sdcard/Movies/big-down.mp4
$ADB push /tmp/big-down.mp4 /sdcard/Movies/big-down.mp4 >/dev/null
rm -f /tmp/big-down.mp4
vm "rm -f ~/Downloads/Continuum/big-down*.mp4"
clear_all >/dev/null
$ADB shell am start -a android.intent.action.VIEW -t vnd.android.document/directory -d "content://com.android.externalstorage.documents/document/primary%3AMovies" >/dev/null
sleep 2
xy=""; for _ in $(seq 10); do xy=$(ui_find big-down.mp4); [ -n "$xy" ] && break; sleep 1; done
$ADB shell input swipe $xy $xy 800
sleep 1.5
$ADB shell input tap $(ui_find Share)
sleep 2.5
target=$(vm "hostname")
others=$($ADB exec-out uiautomator dump /dev/tty 2>/dev/null | grep -o 'content-desc="Send to [^"]*  Continuum"' | grep -v "Send to $target  Continuum")
check "share sheet offers only the paired computer" '[ -z "$others" ]'
$ADB shell input tap $(ui_find "Send to $target")
watch_transfer "phone → PC" "Received big-down.mp4"
sleep 2
final=$(notes)
echo "after: $final" >>"$OUT"
check "phone → PC: one Received notification" "echo '$final' | grep -q 'Received big-down.mp4'"
check "phone → PC: no progress left" '[ "$(progress_count)" = 0 ]'
check "phone → PC: file intact" '[ "$($ADB shell sha256sum /sdcard/Movies/big-down.mp4 | cut -d" " -f1)" = "$(vm "sha256sum ~/Downloads/Continuum/big-down.mp4" | cut -d" " -f1)" ]'

vm "sudo tc qdisc del dev $dev root; sudo tc qdisc del dev $dev ingress" 2>/dev/null
echo "== $D: $pass passed, $fail failed" | tee -a "$OUT"
[ "$fail" = 0 ]
