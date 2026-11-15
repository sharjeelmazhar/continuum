// Reply to a phone notification right in GNOME's notification banner or list (the date/time
// menu): Continuum's "Reply" button opens a text field there instead of a window. Enter
// sends it through the Continuum daemon and the notification goes away. If anything here
// doesn't match this GNOME version, the button keeps its usual behaviour (a reply window).
// SPDX-License-Identifier: GPL-3.0-or-later

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';

import {InjectionManager} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as MessageList from 'resource:///org/gnome/shell/ui/messageList.js';

const APP_ID = 'io.github.sharjeelmazhar.Continuum';
const BUS_NAME = 'io.github.sharjeelmazhar.Continuum.Daemon';
const OBJECT_PATH = '/io/github/sharjeelmazhar/Continuum/Daemon';
const IFACE = 'io.github.sharjeelmazhar.Continuum.Daemon1';
/** The label of our Reply button (notify.rs). */
const REPLY_LABEL = 'Reply';

/** Our freedesktop notification id for a shell notification, or 0. */
function fdoId(notification) {
    const map = Main.notificationDaemon?._fdoNotificationDaemon?._notifications;
    if (!(map instanceof Map))
        return 0;
    for (const [id, n] of map) {
        if (n === notification)
            return id;
    }
    return 0;
}

function isOurs(notification) {
    const app = notification?.source?.app;
    return app?.get_id?.() === `${APP_ID}.desktop`;
}

export class InlineReply {
    enable() {
        const proto = MessageList.NotificationMessage?.prototype;
        if (typeof proto?._addAction !== 'function')
            return;
        this._injections = new InjectionManager();
        const self = this;
        this._injections.overrideMethod(proto, '_addAction', original => function (action) {
            original.call(this, action);
            try {
                self._adopt(this, action);
            } catch (e) {
                console.warn(`Continuum: inline reply unavailable: ${e}`);
            }
        });
    }

    disable() {
        this._injections?.clear();
        this._injections = null;
    }

    /** Takes over the Reply button of one of our phone notifications. */
    _adopt(message, action) {
        if (action?.label !== REPLY_LABEL || !isOurs(message.notification))
            return;
        const button = message._actions?.get(action);
        if (!button || !fdoId(message.notification))
            return;
        // A fresh button in its place (the original's click handler opens the reply window).
        const reply = new St.Button({
            style_class: 'notification-button',
            x_expand: true,
            label: REPLY_LABEL,
            can_focus: true,
        });
        reply.connect('clicked', () => this._openField(message, action));
        button.get_parent()?.replace_child(button, reply);
        message._actions.set(action, reply);
        button.destroy();
    }

    _openField(message, action) {
        const notification = message.notification;
        const id = fdoId(notification);
        if (!id) {
            action.activate();
            return;
        }
        const box = new St.BoxLayout({
            x_expand: true,
            style_class: 'notification-buttons-bin continuum-reply',
        });
        const entry = new St.Entry({
            hint_text: 'Reply…',
            can_focus: true,
            x_expand: true,
            style_class: 'continuum-reply-entry',
            y_align: Clutter.ActorAlign.CENTER,
        });
        const send = new St.Button({
            style_class: 'notification-button continuum-reply-send',
            label: 'Send',
            can_focus: true,
        });
        box.add_child(entry);
        box.add_child(send);
        message.setActionArea(box);
        message.expand?.(true);
        // A banner must hold the keyboard while the user types.
        const tray = Main.messageTray;
        if (tray?._banner === message) {
            tray._expandBanner?.(false);
            tray._ensureBannerFocused?.();
        }
        entry.grab_key_focus();

        const submit = () => {
            const text = entry.get_text().trim();
            if (!text)
                return;
            entry.reactive = false;
            send.reactive = false;
            send.label = 'Sending…';
            Gio.DBus.session.call(BUS_NAME, OBJECT_PATH, IFACE, 'ReplyToNotification',
                new GLib.Variant('(us)', [id, text]), null,
                Gio.DBusCallFlags.NO_AUTO_START, 10000, null, (conn, res) => {
                    try {
                        conn.call_finish(res);
                        // The daemon closes the notification; this covers a slow close.
                        message.notification?.destroy();
                    } catch (e) {
                        entry.reactive = true;
                        send.reactive = true;
                        send.label = 'Send';
                        entry.set_text(text);
                        entry.hint_text = 'Couldn’t send. Try again';
                        console.warn(`Continuum: reply failed: ${e.message}`);
                    }
                });
        };
        entry.clutter_text.connect('activate', submit);
        send.connect('clicked', submit);
    }
}
