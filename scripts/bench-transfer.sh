#!/bin/bash
# PC -> phone transfer benchmark: GSConnect vs Continuum, alternating, 2.5 GB each.
# Resumable: each finished run is appended to results.csv and skipped on rerun.
# The phone copy is deleted after every run (phone storage is small).
# Usage (from anywhere): ROUNDS=5 scripts/bench-transfer.sh   -> results in bench-out/results.csv
# Device ids below are for the current pairing; update them after re-pairing.
set -u
DIR="$(cd "$(dirname "$0")/.." && pwd)/bench-out"
mkdir -p "$DIR"
ROUNDS=${ROUNDS:-5}
SIZE_MB=2560
F=$DIR/bench-2.5GB.bin
CSV=$DIR/results.csv
# Set these for your setup (GSConnect SFTP mount of the phone, both device ids, the NIC).
PHONE="${PHONE:?set PHONE to the GSConnect mount, e.g. /run/user/1000/gvfs/sftp:host=PHONE-IP,port=1739/storage/emulated/0/Download}"
B=org.gnome.Shell.Extensions.GSConnect
GS_DEV=/org/gnome/Shell/Extensions/GSConnect/Device/${GS_ID:?set GS_ID to the GSConnect device id}
CT_DEV=${CT_ID:?set CT_ID to the Continuum device id}
TX=/sys/class/net/${NIC:-eno1}/statistics/tx_bytes
BYTES=$((SIZE_MB*1048576))

[ -f "$CSV" ] || echo "run,app,mb,seconds_total,mb_s_total,seconds_data,mb_s_data,phone_bytes" > "$CSV"
# Test file: rebuild if missing or the wrong size (e.g. cut off by a power cut).
if [ "$(stat -c%s "$F" 2>/dev/null)" != "$BYTES" ]; then
  head -c ${SIZE_MB}M /dev/urandom > "$F.tmp" && mv "$F.tmp" "$F"
fi

phone_path() { [ "$1" = gsconnect ] && echo "$PHONE/bench-2.5GB.bin" || echo "$PHONE/Continuum/bench-2.5GB.bin"; }

run_one() {  # $1=run number, $2=app
  local n=$1 app=$2 pf; pf=$(phone_path "$app")
  grep -q "^$n,$app," "$CSV" && { echo "skip run $n $app (done)"; return; }
  rm -f "$pf"; sleep 3
  local start prev now d t0 first="" last idle=0
  start=$(cat $TX); prev=$start; t0=$(date +%s.%N)
  if [ "$app" = gsconnect ]; then
    gdbus call --session --dest $B --object-path $GS_DEV --method org.gtk.Actions.Activate \
      shareFile "[<('file://$F', false)>]" "{}" >/dev/null
  else
    ~/.local/bin/continuumd send --to $CT_DEV "$F" >/dev/null
  fi
  last=$t0
  for i in $(seq 1 600); do
    sleep 1; now=$(cat $TX); d=$((now-prev)); prev=$now
    if [ $d -gt 1000000 ]; then [ -z "$first" ] && first=$(date +%s.%N); last=$(date +%s.%N); idle=0
    elif [ $((now-start)) -gt $((BYTES/2)) ]; then idle=$((idle+1)); fi
    [ $((now-start)) -lt 50000000 ] && [ $i -ge 45 ] && { echo "run $n $app: no data after 45s"; return 1; }
    [ $idle -ge 5 ] && break
  done
  sleep 5
  local pb; pb=$(stat -c%s "$pf" 2>/dev/null || echo 0)
  local tt td; tt=$(echo "$last - $t0" | bc); td=$(echo "$last - ${first:-$t0}" | bc)
  echo "$n,$app,$SIZE_MB,$tt,$(echo "scale=1; $SIZE_MB/$tt" | bc),$td,$(echo "scale=1; $SIZE_MB/$td" | bc),$pb" >> "$CSV"
  echo "run $n $app: $(tail -1 "$CSV")"
  rm -f "$pf"; sleep 5
}

for n in $(seq 1 $ROUNDS); do
  # Alternate which app goes first so neither always gets the "cold" slot.
  if [ $((n%2)) -eq 1 ]; then run_one $n gsconnect; run_one $n continuum
  else run_one $n continuum; run_one $n gsconnect; fi
done
rm -f "$F"
echo DONE
