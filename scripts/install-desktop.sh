#!/usr/bin/env bash
# Builds and installs Continuum for the current user (no root needed, except to open the
# firewall port if a firewall is active). Safe to re-run; each file is replaced atomically.
#
#   scripts/install-desktop.sh              # build, install, start
#   scripts/install-desktop.sh --uninstall  # stop and remove (keeps your pairing data)
#
# Until there's a Flatpak (M11), this is how to install from source.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="$ROOT/desktop/data"
APP_ID="io.github.sharjeelmazhar.Continuum"
BIN="$HOME/.local/bin"
SHARE="${XDG_DATA_HOME:-$HOME/.local/share}"
UNITS="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
PORT=47820

put() { # put SRC DEST [MODE]: atomic copy
  mkdir -p "$(dirname "$2")"
  install -m "${3:-644}" "$1" "$2.tmp"
  mv -f "$2.tmp" "$2"
}

if [ "${1:-}" = "--uninstall" ]; then
  systemctl --user disable --now continuumd.service 2>/dev/null || true
  rm -f "$BIN/continuumd" "$BIN/continuum-ui" "$UNITS/continuumd.service" \
    "$SHARE/applications/$APP_ID.desktop" "$SHARE/icons/hicolor/scalable/apps/$APP_ID.svg" \
    "$SHARE/dbus-1/services/$APP_ID.Daemon.service"
  systemctl --user daemon-reload
  echo "Continuum removed. Your pairing data is still in $SHARE/continuum (delete it to forget all devices)."
  exit 0
fi

# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
echo "==> Building (release)"
(cd "$ROOT/desktop" && cargo build --release --locked -p continuumd -p continuum-ui)

echo "==> Installing"
put "$ROOT/desktop/target/release/continuumd" "$BIN/continuumd" 755
put "$ROOT/desktop/target/release/continuum-ui" "$BIN/continuum-ui" 755
put "$DATA/$APP_ID.desktop" "$SHARE/applications/$APP_ID.desktop"
put "$DATA/$APP_ID.svg" "$SHARE/icons/hicolor/scalable/apps/$APP_ID.svg"
put "$DATA/$APP_ID.Daemon.service" "$SHARE/dbus-1/services/$APP_ID.Daemon.service"
put "$DATA/continuumd.service" "$UNITS/continuumd.service"
update-desktop-database "$SHARE/applications" 2>/dev/null || true
gtk-update-icon-cache -q -t "$SHARE/icons/hicolor" 2>/dev/null || true

echo "==> Starting the background service (and at every login)"
systemctl --user daemon-reload
systemctl --user enable continuumd.service >/dev/null
systemctl --user restart continuumd.service

if command -v ufw >/dev/null && sudo -n ufw status 2>/dev/null | grep -q "Status: active"; then
  echo "==> Opening port $PORT in the firewall (ufw)"
  sudo -n ufw allow "$PORT/tcp" comment Continuum >/dev/null
elif command -v firewall-cmd >/dev/null && firewall-cmd --state >/dev/null 2>&1; then
  echo "==> Opening port $PORT in the firewall (firewalld)"
  sudo -n firewall-cmd --add-port="$PORT/tcp" --permanent >/dev/null && sudo -n firewall-cmd --reload >/dev/null
fi

case ":$PATH:" in *":$BIN:"*) ;; *) echo "note: $BIN is not on your PATH; the app menu entry works anyway." ;; esac
echo "Done. Open “Continuum” from your apps and click “Pair a Phone”."
