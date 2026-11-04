Name:           continuum
Version:        @VERSION@
Release:        1
Summary:        @SUMMARY@
License:        GPL-3.0-or-later
URL:            @URL@
Recommends:     gstreamer1-plugin-gtk4
Recommends:     gstreamer1-plugin-openh264
Recommends:     gstreamer1-plugins-base
Recommends:     gstreamer1-plugins-good
Recommends:     nautilus-python

%global debug_package %{nil}
%global __strip /bin/true

%description
Continuum links an Android phone with a GNOME desktop: send files, browse the phone's
gallery and storage, read and answer messages and notifications, and share the clipboard.

%install
cp -a %{stage}/. %{buildroot}/

%post
systemctl --global enable continuumd.service continuumd-update.path >/dev/null 2>&1 || :

%preun
if [ "$1" -eq 0 ]; then
  systemctl --global disable continuumd.service continuumd-update.path >/dev/null 2>&1 || :
fi

%files
%license /usr/share/licenses/continuum/LICENSE
/usr/bin/continuumd
/usr/bin/continuum-ui
/usr/share/applications/io.github.sharjeelmazhar.Continuum.desktop
/usr/share/icons/hicolor/*/apps/io.github.sharjeelmazhar.Continuum.png
/usr/share/applications/io.github.sharjeelmazhar.Continuum.Mirror.desktop
/usr/share/icons/hicolor/*/apps/io.github.sharjeelmazhar.Continuum.Mirror.png
/usr/share/icons/hicolor/scalable/actions/continuum*-symbolic.svg
/usr/share/dbus-1/services/io.github.sharjeelmazhar.Continuum.Daemon.service
/usr/lib/systemd/user/continuumd.service
/usr/lib/systemd/user/continuumd-update.path
/usr/lib/systemd/user/continuumd-update.service
/usr/share/gnome-shell/extensions/continuum@sharjeelmazhar.github.io
/usr/share/nautilus-python/extensions/continuum-nautilus.py
/usr/share/kio/servicemenus/continuum-send.desktop
