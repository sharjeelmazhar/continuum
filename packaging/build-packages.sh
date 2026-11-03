#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Builds the desktop packages from release binaries:
#   packaging/build-packages.sh VERSION BIN_DIR OUT_DIR
# Makes OUT_DIR/continuum_VERSION_amd64.deb, continuum-VERSION-1.x86_64.rpm and
# continuum-VERSION-1-x86_64.pkg.tar.zst. Needs dpkg-deb + dpkg-shlibdeps, rpmbuild, and docker
# or podman (the Arch package is built with makepkg in an archlinux container).
set -euo pipefail
VERSION=$1 BIN=$(realpath "$2") OUT=$(realpath -m "$3")
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK=$(mktemp -d); trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT"
"$HERE/stage.sh" "$BIN" "$WORK/root"
SUMMARY="Connect your Android phone to this computer (files, photos, messages, notifications, clipboard)"
URL=https://github.com/sharjeelmazhar/continuum

# --- Debian / Ubuntu ---------------------------------------------------------------------------
deb=$WORK/deb
cp -a "$WORK/root" "$deb"
mkdir -p "$deb/DEBIAN" "$WORK/shlibs/debian"
echo "Source: continuum" > "$WORK/shlibs/debian/control"
depends=$(cd "$WORK/shlibs" && dpkg-shlibdeps -O -e "$deb/usr/bin/continuumd" -e "$deb/usr/bin/continuum-ui" 2>/dev/null |
  sed -n 's/^shlibs:Depends=//p')
cat > "$deb/DEBIAN/control" <<EOF
Package: continuum
Version: $VERSION
Architecture: amd64
Maintainer: Continuum <noreply@github.com>
Depends: $depends
Recommends: gstreamer1.0-gtk4, gstreamer1.0-plugins-base, gstreamer1.0-plugins-good, python3-nautilus
Section: net
Priority: optional
Homepage: $URL
Description: $SUMMARY
 Continuum links an Android phone with a GNOME desktop: send files, browse the phone's
 gallery and storage, read and answer messages and notifications, and share the clipboard.
EOF
install -m 755 "$HERE/scripts/postinst" "$deb/DEBIAN/postinst"
install -m 755 "$HERE/scripts/prerm" "$deb/DEBIAN/prerm"
dpkg-deb --root-owner-group -Zxz --build "$deb" "$OUT/continuum_${VERSION}_amd64.deb" >/dev/null

# --- Fedora / RHEL -----------------------------------------------------------------------------
rpmtop=$WORK/rpm
mkdir -p "$rpmtop"/{BUILD,RPMS,SPECS}
sed -e "s|@VERSION@|$VERSION|" -e "s|@SUMMARY@|$SUMMARY|" -e "s|@URL@|$URL|" "$HERE/continuum.spec" > "$rpmtop/SPECS/continuum.spec"
rpmbuild -bb --quiet --define "_topdir $rpmtop" --define "stage $WORK/root" \
  --define "_build_id_links none" "$rpmtop/SPECS/continuum.spec"
cp "$rpmtop"/RPMS/x86_64/continuum-"$VERSION"-1.x86_64.rpm "$OUT/"

# --- Arch ----------------------------------------------------------------------------------------
arch=$WORK/arch
mkdir -p "$arch"
cp -a "$WORK/root" "$arch/stage"
sed -e "s|@VERSION@|$VERSION|" -e "s|@SUMMARY@|$SUMMARY|" -e "s|@URL@|$URL|" "$HERE/PKGBUILD" > "$arch/PKGBUILD"
cp "$HERE/scripts/continuum.install" "$arch/"
RUN=$(command -v docker || command -v podman)
# Hand the files back: rootless podman maps container root to us, docker needs our own uid.
case $RUN in */podman) OWNER=0:0 ;; *) OWNER="$(id -u):$(id -g)" ;; esac
"$RUN" run --rm -v "$arch:/build:z" docker.io/library/archlinux:latest bash -c '
  set -e
  pacman -Sy --noconfirm --needed fakeroot >/dev/null
  useradd -m b && chown -R b /build
  su b -c "cd /build && PKGEXT=.pkg.tar.zst makepkg --nodeps --noconfirm >/dev/null"
  chown -R '"$OWNER"' /build'
cp "$arch"/continuum-"$VERSION"-1-x86_64.pkg.tar.zst "$OUT/"
ls -1 "$OUT"
