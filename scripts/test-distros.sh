#!/bin/bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Installs Continuum with its one-line installer on fresh GNOME VMs (KVM/libvirt), the way a
# new user would, and checks that it works: the binaries load, the service runs, the D-Bus API
# answers, the GNOME extension is installed, in-app video works and the app window opens.
#
#   scripts/test-distros.sh [fedora|arch|ubuntu ...]     # default: all three
#   VM_DIR=/big/disk scripts/test-distros.sh fedora      # where images go (default ~/vms)
#   KEEP=1 ...                                           # leave the VMs running afterwards
#   SITE=http://192.168.122.1:8000 ...                   # test a local build of the website
#
# Resumable (power cuts): each finished step leaves a marker in $VM_DIR/<distro>/, so a rerun
# skips it; a download cut off half-way is redone. Results: $VM_DIR/<distro>/result.txt and a
# screenshot of the VM's screen. Watch the VMs live in virt-manager (connection qemu:///system).
# Needs: virt-install, qemu-img, genisoimage, libvirt access (group libvirt), ~8 GB disk per VM.
set -u
CONN=qemu:///system
VM_DIR=${VM_DIR:-$HOME/vms}/continuum-test
# Where install.sh and the package repositories are (a local copy can be served for testing).
SITE=${SITE:-https://sharjeelmazhar.github.io/continuum}
mkdir -p "$VM_DIR"
chmod o+x "$VM_DIR" "$(dirname "$VM_DIR")" 2>/dev/null # libvirt's qemu user must reach the disks
KEY=$VM_DIR/ssh_key
[ -f "$KEY" ] || ssh-keygen -q -t ed25519 -N '' -f "$KEY" -C continuum-test

image_url() {
  case $1 in
  fedora) echo https://dl.fedoraproject.org/pub/fedora/linux/releases/45/Cloud/x86_64/images/Fedora-Cloud-Base-Generic-45-1.4.x86_64.qcow2 ;;
  arch) echo https://geo.mirror.pkgbuild.com/images/latest/Arch-Linux-x86_64-cloudimg.qcow2 ;;
  ubuntu) echo https://cloud-images.ubuntu.com/releases/resolute/release/ubuntu-26.04-server-cloudimg-amd64.img ;;
  esac
}

# GNOME (shell, login screen, Files, a terminal) and GStreamer's GTK 4 video sink.
packages() {
  case $1 in
  # The video sink is optional (videos then open in the video player), so it can't fail the rest.
  # A stalled mirror can hang dnf for good: give up after 20 minutes and try another mirror.
  fedora) echo "for i in 1 2 3; do timeout 1200 dnf -y --setopt=timeout=60 install gnome-shell gdm gnome-session-wayland-session nautilus ptyxis gnome-extensions-app gstreamer1-plugins-good curl tar && break; done; dnf -y install gstreamer1-plugin-gtk4 || true" ;;
  arch) echo "pacman-key --init && pacman-key --populate archlinux && pacman -Syu --noconfirm gnome-shell gdm nautilus gnome-console gst-plugins-good gst-plugin-gtk4 curl" ;;
  ubuntu) echo "DEBIAN_FRONTEND=noninteractive apt-get install -y gnome-shell gdm3 gnome-session nautilus gnome-terminal gstreamer1.0-plugins-good curl; DEBIAN_FRONTEND=noninteractive apt-get install -y gstreamer1.0-gtk4 || true" ;;
  esac
}

gdm_conf() { [ "$1" = ubuntu ] && echo /etc/gdm3/custom.conf || echo /etc/gdm/custom.conf; }

step() { # $1=distro $2=name; true if already done
  [ -f "$VM_DIR/$1/.done-$2" ]
}
done_step() { touch "$VM_DIR/$1/.done-$2"; }

ssh_vm() { # $1=distro, rest = command (runs as the test user)
  local d=$1; shift
  ssh -q -i "$KEY" -o IdentitiesOnly=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 \
    "tester@$(vm_ip "$d")" "$@"
}

vm_ip() {
  virsh -c $CONN domifaddr "continuum-$1" 2>/dev/null | awk '/ipv4/{sub("/.*","",$4); print $4; exit}'
}

prepare() { # $1=distro: base image, disk and cloud-init seed
  local d=$1 dir=$VM_DIR/$1
  mkdir -p "$dir"
  if ! step "$d" download; then
    echo "[$d] downloading the cloud image"
    curl -fL --retry 3 -C - -o "$dir/base.part" "$(image_url "$d")" && mv "$dir/base.part" "$dir/base.qcow2" || return 1
    done_step "$d" download
  fi
  if ! step "$d" disk; then
    qemu-img create -q -f qcow2 -F qcow2 -b "$dir/base.qcow2" "$dir/disk.qcow2" 30G || return 1
    cat > "$dir/user-data" <<EOF
#cloud-config
hostname: continuum-$d
users:
  - name: tester
    groups: [wheel, sudo]
    sudo: ALL=(ALL) NOPASSWD:ALL
    shell: /bin/bash
    lock_passwd: false
    plain_text_passwd: tester
    ssh_authorized_keys: ["$(cat "$KEY.pub")"]
runcmd:
  - [sh, -c, "$( [ "$d" = ubuntu ] && echo 'apt-get update &&' ) $(packages "$d")"]
  - [sh, -c, "mkdir -p $(dirname "$(gdm_conf "$d")") && printf '[daemon]\nAutomaticLoginEnable=True\nAutomaticLogin=tester\nWaylandEnable=true\n' > $(gdm_conf "$d")"]
  - [sh, -c, "systemctl set-default graphical.target && touch /var/tmp/continuum-ready && systemctl reboot"]
EOF
    printf 'instance-id: continuum-%s\nlocal-hostname: continuum-%s\n' "$d" "$d" > "$dir/meta-data"
    genisoimage -quiet -o "$dir/seed.iso" -V cidata -r -J "$dir/user-data" "$dir/meta-data" || return 1
    chmod o+r "$dir"/*.qcow2 "$dir/seed.iso"; chmod o+x "$dir"
    done_step "$d" disk
  fi
}

boot() { # $1=distro: create (once) and start the VM, wait for GNOME to be installed
  local d=$1 dir=$VM_DIR/$1
  if ! virsh -c $CONN dominfo "continuum-$d" >/dev/null 2>&1; then
    # The system Python: virt-install needs its gi module, which other Pythons on PATH may lack.
    /usr/bin/python3 /usr/bin/virt-install --connect $CONN --name "continuum-$d" --memory 4096 --vcpus 4 \
      --disk "$dir/disk.qcow2" --disk "$dir/seed.iso,device=cdrom" --import \
      --osinfo detect=on,require=off --network network=default --graphics spice --video virtio \
      --noautoconsole >/dev/null || return 1
  fi
  virsh -c $CONN start "continuum-$d" >/dev/null 2>&1
  echo "[$d] waiting for the VM to install GNOME (first boot takes a while)"
  for _ in $(seq 1 360); do
    ssh_vm "$d" test -f /var/tmp/continuum-ready 2>/dev/null && ssh_vm "$d" systemctl is-active -q display-manager 2>/dev/null && return 0
    sleep 10
  done
  echo "[$d] timed out waiting for GNOME"; return 1
}

check() { # $1=distro: install like a user and record what works
  local d=$1 dir=$VM_DIR/$1 out
  echo "[$d] installing Continuum with the one-line installer from $SITE"
  out=$(ssh_vm "$d" bash -s "$SITE" <<'EOF'
set -u
export XDG_RUNTIME_DIR=/run/user/$(id -u) DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus WAYLAND_DISPLAY=wayland-0
curl -fsSL "$1/install.sh" | CONTINUUM_SITE="$1" sh >/tmp/install.log 2>&1 && echo "install: ok" || { echo "install: FAILED"; tail -8 /tmp/install.log; }
echo "package: $( (dpkg-query -W -f '${Version}' continuum || rpm -q continuum || pacman -Q continuum) 2>/dev/null | tail -1)"
sleep 3
for b in continuumd continuum-ui; do
  missing=$(ldd /usr/bin/$b 2>/dev/null | grep -c "not found")
  [ "$missing" = 0 ] && echo "$b libraries: ok" || echo "$b libraries: MISSING $(ldd /usr/bin/$b | grep 'not found' | tr -s ' ' | tr '\n' ';')"
done
echo "daemon version: $(/usr/bin/continuumd --version 2>&1)"
echo "service: $(systemctl --user is-active continuumd)"
echo "restart after update: $(systemctl --user is-active continuumd-update.path)"
gdbus call --session -d io.github.sharjeelmazhar.Continuum.Daemon -o /io/github/sharjeelmazhar/Continuum/Daemon \
  -m io.github.sharjeelmazhar.Continuum.Daemon1.ListDevices >/dev/null 2>&1 && echo "d-bus api: ok" || echo "d-bus api: FAILED"
gnome-extensions list 2>/dev/null | grep -q continuum@ && echo "gnome extension: installed" || echo "gnome extension: MISSING"
gst-inspect-1.0 gtk4paintablesink >/dev/null 2>&1 && echo "in-app video: ok" || echo "in-app video: no gtk4paintablesink (videos open in the video player)"
echo "gnome shell: $(gnome-shell --version 2>/dev/null)"
(setsid /usr/bin/continuum-ui >/tmp/ui.log 2>&1 &) ; sleep 6
pgrep -x continuum-ui >/dev/null && echo "app window: running" || { echo "app window: CRASHED"; tail -5 /tmp/ui.log; }
EOF
)
  echo "$out" | tee "$dir/result.txt"
  virsh -c $CONN screenshot "continuum-$d" "$dir/screen.ppm" >/dev/null 2>&1 && echo "[$d] screenshot: $dir/screen.ppm"
}

for d in "${@:-fedora arch ubuntu}"; do
  for x in $d; do
    prepare "$x" && boot "$x" && check "$x"
    [ "${KEEP:-0}" = 1 ] || virsh -c $CONN shutdown "continuum-$x" >/dev/null 2>&1
  done
done
