#!/usr/bin/env bash
# Start the Continuum test emulator and wait until Android has fully booted. Safe to re-run:
# if the AVD is already running, it just waits for boot and exits.
#
#   scripts/emulator.sh            # headless (no window), for tests
#   scripts/emulator.sh --window   # with a window, to look at the UI
#   scripts/emulator.sh --stop     # shut it down
#
# Networking: inside the emulator the host PC is 10.0.2.2, so the app reaches continuumd at
# 10.0.2.2:47820. mDNS from the host does not reach the emulator (see docs/design/07-build-test-ci.md).
set -euo pipefail

SDK="${ANDROID_HOME:-$HOME/Android/Sdk}"
AVD="${CONTINUUM_AVD:-continuum-test}"
ADB="$SDK/platform-tools/adb"
EMU="$SDK/emulator/emulator"
LOG="${XDG_CACHE_HOME:-$HOME/.cache}/continuum/emulator.log"
BOOT_TIMEOUT="${CONTINUUM_BOOT_TIMEOUT:-300}"

WINDOW=0
case "${1:-}" in
  "") ;;
  --window) WINDOW=1 ;;
  --stop)
    for s in $("$ADB" devices | awk '/^emulator-/{print $1}'); do "$ADB" -s "$s" emu kill || true; done
    exit 0 ;;
  *) echo "usage: $0 [--window|--stop]" >&2; exit 2 ;;
esac

running_serial() {
  for s in $("$ADB" devices | awk '/^emulator-[0-9]+\tdevice/{print $1}'); do
    if [ "$("$ADB" -s "$s" emu avd name 2>/dev/null | head -1 | tr -d '\r')" = "$AVD" ]; then
      echo "$s"; return 0
    fi
  done
  return 1
}

"$ADB" start-server >/dev/null
if ! serial="$(running_serial)"; then
  [ -w /dev/kvm ] || echo "warning: /dev/kvm not writable, the emulator will be very slow" >&2
  mkdir -p "$(dirname "$LOG")"
  args=(-avd "$AVD" -no-snapshot-save -no-boot-anim -netdelay none -netspeed full)
  [ "$WINDOW" = 1 ] || args+=(-no-window -no-audio -gpu swiftshader_indirect)
  echo "Starting $AVD (log: $LOG)"
  nohup "$EMU" "${args[@]}" >"$LOG" 2>&1 &
  for _ in $(seq 60); do serial="$(running_serial)" && break; sleep 2; done
  [ -n "${serial:-}" ] || { echo "emulator did not appear in adb; see $LOG" >&2; exit 1; }
fi

echo "Waiting for $serial to boot..."
deadline=$((SECONDS + BOOT_TIMEOUT))
until [ "$("$ADB" -s "$serial" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do
  [ "$SECONDS" -lt "$deadline" ] || { echo "boot timed out after ${BOOT_TIMEOUT}s; see $LOG" >&2; exit 1; }
  sleep 3
done
echo "$serial ready"
