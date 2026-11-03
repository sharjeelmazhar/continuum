#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Turns the packages from build-packages.sh into signed apt, dnf and pacman repositories, the
# folders that "sudo apt upgrade", "sudo dnf upgrade" and "sudo pacman -Syu" read:
#   packaging/build-repos.sh PKG_DIR SITE_DIR
# Signs with the only secret key in $GNUPGHOME (kept outside git; CI imports it from a secret).
# Writes SITE_DIR/apt, SITE_DIR/rpm, SITE_DIR/arch/x86_64 and the public key SITE_DIR/key.asc.
set -euo pipefail
PKGS=$(realpath "$1") SITE=$(realpath -m "$2")
KEY=$(gpg --list-secret-keys --with-colons | awk -F: '/^fpr/{print $10; exit}')
[ -n "$KEY" ] || { echo "no signing key in ${GNUPGHOME:-~/.gnupg}"; exit 1; }
sign() { gpg --batch --yes --local-user "$KEY" "$@"; }
mkdir -p "$SITE"
gpg --batch --yes --armor --export "$KEY" > "$SITE/key.asc"

# --- apt (a flat repository: deb <url>/apt ./) --------------------------------------------------
rm -rf "$SITE/apt" && mkdir -p "$SITE/apt"
cp "$PKGS"/*.deb "$SITE/apt/"
(
  cd "$SITE/apt"
  dpkg-scanpackages --multiversion . > Packages 2>/dev/null
  gzip -9 -k -n Packages
  {
    echo "Origin: Continuum"
    echo "Label: Continuum"
    echo "Suite: stable"
    echo "Architectures: amd64"
    echo "Date: $(date -Ru)"
    echo "SHA256:"
    for f in Packages Packages.gz; do
      printf ' %s %s %s\n' "$(sha256sum "$f" | cut -d' ' -f1)" "$(stat -c %s "$f")" "$f"
    done
  } > Release
  sign --clearsign -o InRelease Release
  sign --armor --detach-sign -o Release.gpg Release
)

# --- dnf: signed packages and signed metadata -----------------------------------------------------
rm -rf "$SITE/rpm" && mkdir -p "$SITE/rpm"
cp "$PKGS"/*.rpm "$SITE/rpm/"
rpmsign --addsign --define "_gpg_name $KEY" --define "_gpg_path ${GNUPGHOME:-$HOME/.gnupg}" "$SITE"/rpm/*.rpm >/dev/null
createrepo_c --quiet "$SITE/rpm"
sign --armor --detach-sign -o "$SITE/rpm/repodata/repomd.xml.asc" "$SITE/rpm/repodata/repomd.xml"

# --- pacman: signed packages and signed database ------------------------------------------------
A=$SITE/arch/x86_64
rm -rf "$SITE/arch" && mkdir -p "$A"
cp "$PKGS"/*.pkg.tar.zst "$A/"
for p in "$A"/*.pkg.tar.zst; do sign --detach-sign -o "$p.sig" "$p"; done
RUN=$(command -v docker || command -v podman)
case $RUN in */podman) OWNER=0:0 ;; *) OWNER="$(id -u):$(id -g)" ;; esac
"$RUN" run --rm -v "$A:/repo:z" docker.io/library/archlinux:latest bash -c \
  'cd /repo && repo-add -q continuum.db.tar.gz *.pkg.tar.zst && chown -R '"$OWNER"' /repo'
# Real files, not the symlinks repo-add makes (web hosting may not follow them).
for f in db files; do
  rm -f "$A/continuum.$f" && cp "$A/continuum.$f.tar.gz" "$A/continuum.$f"
done
sign --detach-sign -o "$A/continuum.db.sig" "$A/continuum.db"
cp "$A/continuum.db.sig" "$A/continuum.db.tar.gz.sig"
echo "Repositories in $SITE (key $KEY)"
