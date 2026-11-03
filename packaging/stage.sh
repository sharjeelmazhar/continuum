#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Lays out the system-wide install tree (/usr/...) that every package (deb, rpm, Arch) contains.
#   packaging/stage.sh BIN_DIR DEST     # BIN_DIR holds continuumd and continuum-ui
set -euo pipefail
BIN=$1 DEST=$2
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="$ROOT/desktop/data"
APP_ID=io.github.sharjeelmazhar.Continuum
EXT=continuum@sharjeelmazhar.github.io

rm -rf "$DEST" && mkdir -p "$DEST"
put() { install -D -m "${3:-644}" "$1" "$DEST/$2"; }

put "$BIN/continuumd" usr/bin/continuumd 755
put "$BIN/continuum-ui" usr/bin/continuum-ui 755
put "$DATA/$APP_ID.desktop" "usr/share/applications/$APP_ID.desktop"
for png in "$DATA"/icons/hicolor/*/apps/$APP_ID.png; do
  size="$(basename "$(dirname "$(dirname "$png")")")"
  put "$png" "usr/share/icons/hicolor/$size/apps/$APP_ID.png"
done
put "$DATA/$APP_ID.Daemon.service" "usr/share/dbus-1/services/$APP_ID.Daemon.service"
sed 's|^ExecStart=.*|ExecStart=/usr/bin/continuumd|' "$DATA/continuumd.service" > "$DEST/.unit"
put "$DEST/.unit" usr/lib/systemd/user/continuumd.service && rm "$DEST/.unit"
put "$ROOT/packaging/continuumd-update.path" usr/lib/systemd/user/continuumd-update.path
put "$ROOT/packaging/continuumd-update.service" usr/lib/systemd/user/continuumd-update.service
for f in "$DATA/gnome-shell/$EXT"/*; do
  put "$f" "usr/share/gnome-shell/extensions/$EXT/$(basename "$f")"
done
put "$DATA/nautilus/continuum-nautilus.py" usr/share/nautilus-python/extensions/continuum-nautilus.py
sed 's|@BIN@|/usr/bin|' "$DATA/kde/continuum-send.desktop" > "$DEST/.kde"
put "$DEST/.kde" usr/share/kio/servicemenus/continuum-send.desktop 755 && rm "$DEST/.kde"
put "$ROOT/LICENSE" usr/share/licenses/continuum/LICENSE
