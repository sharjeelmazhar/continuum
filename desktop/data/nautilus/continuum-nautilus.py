# SPDX-License-Identifier: GPL-3.0-or-later
"""Continuum: adds "Send to <phone>" to the right-click menu of GNOME Files (Nautilus)."""

import gi

try:
    gi.require_version("Nautilus", "4.0")
except ValueError:
    gi.require_version("Nautilus", "3.0")
from gi.repository import Gio, GLib, GObject, Nautilus  # noqa: E402

BUS = "io.github.sharjeelmazhar.Continuum.Daemon"
PATH = "/io/github/sharjeelmazhar/Continuum/Daemon"
IFACE = "io.github.sharjeelmazhar.Continuum.Daemon1"


def _call(method, args, reply_type, timeout_ms):
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    return bus.call_sync(BUS, PATH, IFACE, method, args, reply_type,
                         Gio.DBusCallFlags.NO_AUTO_START, timeout_ms, None)


class ContinuumMenu(GObject.GObject, Nautilus.MenuProvider):
    def get_file_items(self, *args):
        files = args[-1]  # Nautilus 3 passes (window, files), Nautilus 4 just (files)
        if not files or any(f.is_directory() for f in files):
            return []
        paths = [f.get_location().get_path() for f in files]
        if None in paths:  # not local files (e.g. a network share without a path)
            return []
        try:
            devices = _call("ListDevices", None, GLib.VariantType("(a(sssbib))"), 300).unpack()[0]
        except GLib.Error:
            return []  # Continuum isn't running
        items = []
        for dev_id, name, kind, _connected, _level, _charging in devices:
            item = Nautilus.MenuItem(
                name="Continuum::send-" + dev_id,
                label="Send to " + name,
                icon="computer-symbolic" if kind in ("laptop", "desktop") else "phone-symbolic",
            )
            item.connect("activate", self._send, dev_id, paths)
            items.append(item)
        return items

    def _send(self, _item, dev_id, paths):
        try:
            _call("SendFiles", GLib.Variant("(sas)", (dev_id, paths)), None, 5000)
        except GLib.Error as e:
            print("Continuum: could not send:", e.message)
