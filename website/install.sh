#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Installs Continuum on this computer:
#   curl -fsSL https://sharjeelmazhar.github.io/continuum/install.sh | sh
# It adds Continuum's signed package repository, so later versions arrive with the system's own
# updates (Software / "sudo apt upgrade", "sudo dnf upgrade", "sudo pacman -Syu").
# Run it as yourself, not with sudo: it asks for your password when it needs it.
set -eu
SITE=${CONTINUUM_SITE:-https://sharjeelmazhar.github.io/continuum}
FPR=493630795BE42031D6ECF8E2D79AB8A38FAFE6AC
EXT=continuum@sharjeelmazhar.github.io
PORT=47820

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }
[ "$(id -u)" != 0 ] || { echo "Run this as yourself, without sudo (it asks for your password when needed)."; exit 1; }
[ "$(uname -m)" = x86_64 ] || { echo "Continuum is only built for 64-bit PCs (x86_64) so far."; exit 1; }
command -v systemctl >/dev/null || { echo "Continuum needs systemd, which this computer doesn't use."; exit 1; }
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
curl -fsSL -o "$T/key.asc" "$SITE/key.asc"
# The key must be the one this script expects, whatever the server sent.
command -v gpg >/dev/null && ! gpg --show-keys --with-colons "$T/key.asc" 2>/dev/null | grep -q "fpr:::::::::$FPR:" &&
  { echo "The signing key from $SITE isn't the expected one. Stopping."; exit 1; }

if command -v apt-get >/dev/null; then
  say "Adding the Continuum repository (apt)"
  sudo install -D -m 644 "$T/key.asc" /etc/apt/keyrings/continuum.asc
  # The deb822 format (the one-line .list format is deprecated).
  printf 'Types: deb\nURIs: %s/apt/\nSuites: ./\nSigned-By: /etc/apt/keyrings/continuum.asc\n' "$SITE" |
    sudo tee /etc/apt/sources.list.d/continuum.sources >/dev/null
  sudo apt-get update -qq
  say "Installing"
  sudo apt-get install -y continuum
elif command -v dnf >/dev/null; then
  say "Adding the Continuum repository (dnf)"
  printf '[continuum]\nname=Continuum\nbaseurl=%s/rpm\nenabled=1\ngpgcheck=1\nrepo_gpgcheck=1\ngpgkey=%s/key.asc\n' "$SITE" "$SITE" |
    sudo tee /etc/yum.repos.d/continuum.repo >/dev/null
  sudo rpm --import "$T/key.asc"
  say "Installing"
  sudo dnf install -y continuum
elif command -v pacman >/dev/null; then
  say "Adding the Continuum repository (pacman)"
  sudo pacman-key --add "$T/key.asc" >/dev/null
  sudo pacman-key --lsign-key "$FPR" >/dev/null
  grep -q '^\[continuum\]' /etc/pacman.conf ||
    printf '\n[continuum]\nSigLevel = Required\nServer = %s/arch/$arch\n' "$SITE" | sudo tee -a /etc/pacman.conf >/dev/null
  say "Installing"
  sudo pacman -Syu --noconfirm --needed continuum
  command -v nautilus >/dev/null && sudo pacman -S --noconfirm --needed --asdeps python-nautilus gst-plugin-gtk4 gst-plugins-base >/dev/null 2>&1 || true
else
  echo "This system has no apt, dnf or pacman, so Continuum can't be installed here yet."
  exit 1
fi

# A copy installed from an older download or a source checkout lives in your home folder and
# would hide the system one: remove it (pairing data is kept).
if [ -e "$HOME/.local/bin/continuumd" ] || [ -e "$HOME/.config/systemd/user/continuumd.service" ]; then
  say "Removing the older copy in your home folder (your pairing is kept)"
  systemctl --user disable --now continuumd.service 2>/dev/null || true
  D=${XDG_DATA_HOME:-$HOME/.local/share}
  rm -f "$HOME/.local/bin/continuumd" "$HOME/.local/bin/continuum-ui" "$HOME/.config/systemd/user/continuumd.service" \
    "$D/applications/io.github.sharjeelmazhar.Continuum.desktop" "$D"/icons/hicolor/*/apps/io.github.sharjeelmazhar.Continuum.png \
    "$D/dbus-1/services/io.github.sharjeelmazhar.Continuum.Daemon.service" \
    "$D/nautilus-python/extensions/continuum-nautilus.py" "$D/kio/servicemenus/continuum-send.desktop"
  rm -rf "$D/gnome-shell/extensions/$EXT"
fi

say "Starting the background service (and at every login)"
systemctl --user daemon-reload
systemctl --user enable --now continuumd.service continuumd-update.path >/dev/null 2>&1 || true
systemctl --user restart continuumd.service 2>/dev/null || true

NEED_LOGIN=
if command -v gnome-shell >/dev/null && command -v gsettings >/dev/null; then
  if gnome-extensions info "$EXT" >/dev/null 2>&1; then
    gnome-extensions enable "$EXT" 2>/dev/null || true
  fi
  # Also for the next login, when GNOME first sees a newly installed extension.
  current=$(gsettings get org.gnome.shell enabled-extensions 2>/dev/null || echo "@as []")
  case $current in
    *"$EXT"*) ;;
    "@as []") gsettings set org.gnome.shell enabled-extensions "['$EXT']"; NEED_LOGIN=1 ;;
    *) gsettings set org.gnome.shell enabled-extensions "${current%]}, '$EXT']"; NEED_LOGIN=1 ;;
  esac
fi

# The phone connects to this port. Only home and office networks, never public ones.
NETS="10.0.0.0/8 172.16.0.0/12 192.168.0.0/16 169.254.0.0/16 100.64.0.0/10 fe80::/10 fc00::/7"
if command -v ufw >/dev/null && sudo ufw status 2>/dev/null | grep -q "Status: active"; then
  say "Letting the phone through the firewall (local networks only)"
  for n in $NETS; do sudo ufw allow proto tcp from "$n" to any port $PORT comment Continuum >/dev/null; done
elif command -v firewall-cmd >/dev/null && sudo firewall-cmd --state >/dev/null 2>&1; then
  say "Letting the phone through the firewall (local networks only)"
  for n in $NETS; do
    case $n in *:*) fam=ipv6 ;; *) fam=ipv4 ;; esac
    sudo firewall-cmd --permanent --add-rich-rule="rule family=$fam source address=$n port port=$PORT protocol=tcp accept" >/dev/null
  done
  sudo firewall-cmd --reload >/dev/null
fi

echo
echo "Done. Open “Continuum” from your apps and click “Pair a Phone”."
[ -z "$NEED_LOGIN" ] || echo "Log out and back in once to see your phone in Quick Settings."
