// Continuum Quick Settings tile: the phone's name and battery, and one-click Ring, Messages,
// Photos, Notifications, Phone Storage and Send Files. Everything goes through the Continuum
// daemon (D-Bus) and the Continuum app's actions; this extension holds no state of its own.
// SPDX-License-Identifier: GPL-3.0-or-later

import GObject from 'gi://GObject';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import {QuickMenuToggle, SystemIndicator} from 'resource:///org/gnome/shell/ui/quickSettings.js';

const APP_ID = 'io.github.sharjeelmazhar.Continuum';
const APP_PATH = '/io/github/sharjeelmazhar/Continuum';
const BUS_NAME = 'io.github.sharjeelmazhar.Continuum.Daemon';
const OBJECT_PATH = '/io/github/sharjeelmazhar/Continuum/Daemon';

const DaemonIface = `
<node>
  <interface name="io.github.sharjeelmazhar.Continuum.Daemon1">
    <method name="ListDevices"><arg type="a(sssbib)" direction="out"/></method>
    <method name="DeviceCaps">
      <arg type="s" direction="in"/><arg type="as" direction="out"/>
    </method>
    <method name="Command">
      <arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/>
    </method>
    <signal name="DevicesChanged"/>
  </interface>
</node>`;
const DaemonProxy = Gio.DBusProxy.makeProxyWrapper(DaemonIface);

/** Menu entries: [label, icon, app page or special action, capability needed]. */
const ENTRIES = [
    ['Messages', 'mail-unread-symbolic', 'messages', 'sms'],
    ['Photos', 'image-x-generic-symbolic', 'photos', 'photos'],
    ['Notifications', 'preferences-system-notifications-symbolic', 'notifications', 'notif'],
    ['Phone Storage', 'folder-symbolic', 'storage', 'files'],
    ['Send Files…', 'document-send-symbolic', 'send-files', ''],
];

function pictureFor(id, type) {
    const custom = GLib.build_filenamev([
        GLib.get_user_data_dir(), 'continuum', 'pictures', `${id}.png`]);
    if (GLib.file_test(custom, GLib.FileTest.EXISTS))
        return Gio.FileIcon.new(Gio.File.new_for_path(custom));
    return Gio.ThemedIcon.new(type === 'tablet' ? 'tablet-symbolic' : 'phone-symbolic');
}

function statusText(connected, battery, charging) {
    if (!connected)
        return 'Not connected';
    if (battery < 0)
        return 'Connected';
    return charging ? `${battery}% · Charging` : `${battery}%`;
}

const PhoneToggle = GObject.registerClass(
class PhoneToggle extends QuickMenuToggle {
    _init(extension) {
        super._init({
            title: 'Continuum',
            iconName: 'phone-symbolic',
            toggleMode: false,
        });
        this._ext = extension;
        this._device = null;
        this._caps = [];
        this.connect('clicked', () => this._openPage(null));

        this._ring = new PopupMenu.PopupImageMenuItem('Ring', 'audio-volume-high-symbolic');
        this._ring.connect('activate', () => this._command('find.ring'));
        this.menu.addMenuItem(this._ring);
        this._items = ENTRIES.map(([label, icon, page, cap]) => {
            const item = new PopupMenu.PopupImageMenuItem(label, icon);
            item.connect('activate', () => this._openPage(page));
            this.menu.addMenuItem(item);
            return [item, cap];
        });
        this.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        const open = new PopupMenu.PopupImageMenuItem('Open Continuum', 'go-next-symbolic');
        open.connect('activate', () => this._openPage(null));
        this.menu.addMenuItem(open);
        this.update([]);
    }

    /** Shows the selected (or first connected) device from a ListDevices answer. */
    update(devices) {
        const remembered = this._lastDevice();
        const d = devices.find(x => x[0] === remembered) ??
            devices.find(x => x[3]) ?? devices[0] ?? null;
        this._device = d;
        if (!d) {
            this.title = 'Continuum';
            this.subtitle = 'No phone paired';
            this.checked = false;
            this.menu.setHeader('phone-symbolic', 'Continuum', 'Pair a phone in the Continuum app');
            this._setItemsVisible([]);
            return;
        }
        const [id, name, type, connected, battery, charging] = d;
        this.title = name;
        this.subtitle = statusText(connected, battery, charging);
        this.checked = connected;
        this.menu.setHeader(pictureFor(id, type), name, this.subtitle);
        if (!connected) {
            this._setItemsVisible(null);
            return;
        }
        this._ext.daemon?.DeviceCapsRemote(id, ([caps], error) => {
            if (!error && this._device?.[0] === id)
                this._setItemsVisible(caps);
        });
    }

    /** null = offline: everything shown but greyed out. */
    _setItemsVisible(caps) {
        const offline = caps === null;
        const has = cap => offline || !cap || caps.includes(cap);
        this._ring.visible = has('find') && this._device !== null;
        this._ring.setSensitive(!offline);
        for (const [item, cap] of this._items) {
            item.visible = this._device !== null && has(cap);
            item.setSensitive(!offline || cap === '');
        }
    }

    _lastDevice() {
        const path = GLib.build_filenamev([GLib.get_user_config_dir(), 'continuum', 'last-device']);
        try {
            const [ok, bytes] = GLib.file_get_contents(path);
            return ok ? new TextDecoder().decode(bytes).trim() : null;
        } catch {
            return null;
        }
    }

    _command(kind) {
        if (!this._device)
            return;
        this._ext.daemon?.CommandRemote(this._device[0], kind, '{}', (_, error) => {
            if (error)
                Main.notifyError('Continuum', error.message);
            else if (kind === 'find.ring')
                Main.notify('Continuum', `Ringing ${this._device[1]}…`);
        });
        Main.panel.closeQuickSettings();
    }

    /** Opens the app (on a page, for the selected phone), starting it if needed. */
    _openPage(page) {
        Main.panel.closeQuickSettings();
        const calls = [];
        if (this._device)
            calls.push(['select-device', this._device[0]]);
        if (page === 'send-files')
            calls.push(['send-files', null]);
        else if (page)
            calls.push(['show-page', page]);
        else
            calls.push(['present', null]);
        this._ext.activateApp(calls);
    }
});

const PhoneIndicator = GObject.registerClass(
class PhoneIndicator extends SystemIndicator {
    _init(extension) {
        super._init();
        this._icon = this._addIndicator();
        this._icon.iconName = 'phone-symbolic';
        this._icon.visible = false;
        this.toggle = new PhoneToggle(extension);
        this.quickSettingsItems.push(this.toggle);
    }

    update(devices) {
        this.toggle.update(devices);
        this._icon.visible = devices.some(d => d[3]);
    }
});

export default class ContinuumExtension extends Extension {
    enable() {
        this._indicator = new PhoneIndicator(this);
        Main.panel.statusArea.quickSettings.addExternalIndicator(this._indicator);
        this._watch = Gio.bus_watch_name(Gio.BusType.SESSION, BUS_NAME,
            Gio.BusNameWatcherFlags.NONE,
            () => this._connectDaemon(),
            () => {
                this._dropDaemon();
                this._indicator?.update([]);
            });
    }

    disable() {
        Gio.bus_unwatch_name(this._watch);
        this._dropDaemon();
        if (this._appWait)
            GLib.source_remove(this._appWait);
        this._appWait = 0;
        this._indicator.quickSettingsItems.forEach(i => i.destroy());
        this._indicator.destroy();
        this._indicator = null;
    }

    _connectDaemon() {
        this._dropDaemon();
        this.daemon = new DaemonProxy(Gio.DBus.session, BUS_NAME, OBJECT_PATH, (proxy, error) => {
            if (error || this.daemon !== proxy)
                return;
            this._changed = proxy.connectSignal('DevicesChanged', () => this._refresh());
            this._refresh();
        }, null, Gio.DBusProxyFlags.DO_NOT_AUTO_START);
    }

    _dropDaemon() {
        if (this.daemon && this._changed)
            this.daemon.disconnectSignal(this._changed);
        this._changed = 0;
        this.daemon = null;
    }

    _refresh() {
        this.daemon?.ListDevicesRemote(([devices], error) => {
            if (!error)
                this._indicator?.update(devices);
        });
    }

    /** Runs app actions over D-Bus; if the app isn't running, starts it and waits for it. */
    activateApp(calls, attempt = 0) {
        const call = i => {
            if (i >= calls.length)
                return;
            const [name, arg] = calls[i];
            const params = arg === null ? [] : [new GLib.Variant('s', arg)];
            Gio.DBus.session.call(APP_ID, APP_PATH, 'org.gtk.Actions', 'Activate',
                new GLib.Variant('(sava{sv})', [name, params, {}]),
                null, Gio.DBusCallFlags.NO_AUTO_START, 2000, null, (conn, res) => {
                    try {
                        conn.call_finish(res);
                        call(i + 1);
                    } catch {
                        if (attempt === 0)
                            Gio.DesktopAppInfo.new(`${APP_ID}.desktop`)?.launch([], null);
                        if (attempt < 20) {
                            this._appWait = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 250, () => {
                                this._appWait = 0;
                                this.activateApp(calls.slice(i), attempt + 1);
                                return GLib.SOURCE_REMOVE;
                            });
                        }
                    }
                });
        };
        call(0);
    }
}
