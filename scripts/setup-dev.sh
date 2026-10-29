#!/usr/bin/env bash
# Continuum dev setup for Ubuntu/Debian. Safe to re-run: every step skips what is already done,
# downloads go to a temp name and are renamed only when complete.
#
#   scripts/setup-dev.sh            # everything
#   scripts/setup-dev.sh --no-apt   # skip system packages (no sudo needed)
#   scripts/setup-dev.sh --no-avd   # skip the emulator image and AVD
#   scripts/setup-dev.sh --profile  # also add ANDROID_HOME and PATH to ~/.bashrc and ~/.zshrc
set -euo pipefail

SDK="${ANDROID_HOME:-$HOME/Android/Sdk}"
CMDLINE_ZIP="commandlinetools-linux-16111833_latest.zip"
PLATFORM="${CONTINUUM_PLATFORM:-android-37.0}"
BUILD_TOOLS="${CONTINUUM_BUILD_TOOLS:-37.0.0}"
SYS_IMG="${CONTINUUM_SYS_IMG:-system-images;$PLATFORM;google_apis;x86_64}"
AVD_NAME="continuum-test"

DO_APT=1 DO_AVD=1 DO_PROFILE=0
for a in "$@"; do
  case "$a" in
    --no-apt) DO_APT=0 ;;
    --no-avd) DO_AVD=0 ;;
    --profile) DO_PROFILE=1 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done

step() { printf '\n==> %s\n' "$*"; }

if [ "$DO_APT" = 1 ]; then
  step "System packages (sudo)"
  sudo apt-get update
  sudo apt-get install -y build-essential pkg-config curl unzip git \
    libgtk-4-dev libadwaita-1-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-good \
    libdbus-1-dev openjdk-21-jdk-headless
fi

step "Rust (rustup, stable)"
if ! command -v rustup >/dev/null && [ ! -x "$HOME/.cargo/bin/rustup" ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal \
    --component clippy,rustfmt
fi
# shellcheck disable=SC1091
. "$HOME/.cargo/env"
rustup toolchain install stable --profile minimal --component clippy,rustfmt
rustc --version

step "Android SDK command-line tools ($SDK)"
if [ ! -x "$SDK/cmdline-tools/latest/bin/sdkmanager" ]; then
  mkdir -p "$SDK/cmdline-tools"
  tmp="$(mktemp -d)"
  curl -fL --retry 3 -o "$tmp/$CMDLINE_ZIP.part" "https://dl.google.com/android/repository/$CMDLINE_ZIP"
  mv "$tmp/$CMDLINE_ZIP.part" "$tmp/$CMDLINE_ZIP"
  unzip -q "$tmp/$CMDLINE_ZIP" -d "$tmp"
  rm -rf "$SDK/cmdline-tools/latest"
  mv "$tmp/cmdline-tools" "$SDK/cmdline-tools/latest"
  rm -rf "$tmp"
fi
export ANDROID_HOME="$SDK"
export PATH="$SDK/cmdline-tools/latest/bin:$SDK/platform-tools:$SDK/emulator:$PATH"

step "Android packages ($PLATFORM, build-tools $BUILD_TOOLS)"
yes | sdkmanager --licenses >/dev/null || true
pkgs=("platform-tools" "platforms;$PLATFORM" "build-tools;$BUILD_TOOLS")
[ "$DO_AVD" = 1 ] && pkgs+=("emulator" "$SYS_IMG")
sdkmanager --install "${pkgs[@]}"

if [ "$DO_AVD" = 1 ]; then
  step "Emulator AVD ($AVD_NAME)"
  if ! avdmanager list avd -c | grep -qx "$AVD_NAME"; then
    echo no | avdmanager create avd -n "$AVD_NAME" -k "$SYS_IMG" -d pixel_8
  fi
  [ -w /dev/kvm ] || echo "warning: /dev/kvm not writable, the emulator will be slow (add yourself to the kvm group)"
fi

line='export ANDROID_HOME="$HOME/Android/Sdk"; export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"'
if [ "$DO_PROFILE" = 1 ]; then
  step "Shell profile"
  for rc in "$HOME/.bashrc" "$HOME/.zshrc"; do
    if [ -f "$rc" ] && ! grep -qF 'ANDROID_HOME="$HOME/Android/Sdk"' "$rc"; then echo "$line" >> "$rc"; fi
  done
else
  step "Add this to your shell rc (or re-run with --profile):"
  echo "  . \"\$HOME/.cargo/env\"; $line"
fi

step "Done. Open a new terminal (or source your shell rc) to get cargo and sdkmanager on PATH."
