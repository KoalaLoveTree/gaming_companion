// GNOME Wayland has no client request for always-on-top or for a window move.
// This extension pins only the companion's own windows, and moves them back
// to the place the app saved.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const APP_IDS = ['com.gamingcompanion.Companion', 'gaming_companion'];

const INTERFACE = `
<node>
  <interface name="com.gamingcompanion.Pin">
    <method name="Pin">
      <arg type="s" direction="out" name="status"/>
    </method>
    <method name="Frame">
      <arg type="s" direction="out" name="status"/>
    </method>
    <method name="Place">
      <arg type="i" direction="in" name="x"/>
      <arg type="i" direction="in" name="y"/>
      <arg type="s" direction="out" name="status"/>
    </method>
  </interface>
</node>`;

function metaWindow(actor) {
    if (!actor)
        return null;
    if (actor.meta_window)
        return actor.meta_window;
    if (actor.metaWindow)
        return actor.metaWindow;
    if (typeof actor.get_meta_window === 'function')
        return actor.get_meta_window();
    return null;
}

function idOf(win, name) {
    try {
        if (typeof win[name] !== 'function')
            return '';
        const value = win[name]();
        return value ? String(value) : '';
    } catch (e) {
        return '';
    }
}

function idsOf(win) {
    return [
        idOf(win, 'get_gtk_application_id'),
        idOf(win, 'get_sandboxed_app_id'),
        idOf(win, 'get_wm_class'),
        idOf(win, 'get_wm_class_instance'),
    ].filter(value => value.length > 0);
}

function matches(win) {
    return idsOf(win).some(value => APP_IDS.includes(value));
}

function titleOf(win) {
    try {
        return (typeof win.get_title === 'function' && win.get_title()) || 'companion';
    } catch (e) {
        return 'companion';
    }
}

function companionWindow() {
    for (const actor of global.get_window_actors()) {
        const win = metaWindow(actor);
        if (win && matches(win))
            return win;
    }
    return null;
}

// Shell frame coordinates are the same layout pixels as the saved config.
function frameStatus() {
    const win = companionWindow();
    if (!win)
        return 'missing';
    try {
        const rect = win.get_frame_rect();
        if (!rect || !Number.isFinite(rect.x) || !Number.isFinite(rect.y))
            return 'missing';
        return `${Math.round(rect.x)},${Math.round(rect.y)}`;
    } catch (e) {
        log(`gaming-companion frame: ${e}`);
        return 'failed';
    }
}

function placeWindow(x, y) {
    const win = companionWindow();
    if (!win)
        return 'missing';
    const px = Math.round(Number(x));
    const py = Math.round(Number(y));
    if (!Number.isFinite(px) || !Number.isFinite(py))
        return 'failed';
    try {
        if (typeof win.move_frame !== 'function')
            return 'failed';
        win.move_frame(true, px, py);
        pinWindow(win);
        return 'moved';
    } catch (e) {
        log(`gaming-companion place: ${e}`);
        return 'failed';
    }
}

function pinWindow(win) {
    if (!win || !matches(win))
        return false;
    try {
        if (typeof win.is_above !== 'function' || !win.is_above()) {
            if (typeof win.make_above === 'function')
                win.make_above();
        }
        return typeof win.is_above !== 'function' || win.is_above();
    } catch (e) {
        log(`gaming-companion pin: ${e}`);
        return false;
    }
}

export default class CompanionPinExtension extends Extension {
    enable() {
        this._signals = [];
        this._timeouts = [];
        this._armed = new Set();
        try {
            this._createdId = global.display.connect('window-created', (_display, win) => {
                this._arm(win);
            });
            for (const actor of global.get_window_actors())
                this._arm(metaWindow(actor));
        } catch (e) {
            log(`gaming-companion pin: ${e}`);
        }
        try {
            this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE, {
                Pin: () => this._status(),
                Frame: () => frameStatus(),
                Place: (x, y) => placeWindow(x, y),
            });
            this._dbus.export(Gio.DBus.session, '/com/gamingcompanion/Pin');
        } catch (e) {
            log(`gaming-companion pin export: ${e}`);
            this._dbus = null;
        }
    }

    disable() {
        if (this._createdId) {
            global.display.disconnect(this._createdId);
            this._createdId = 0;
        }
        for (const [win, id] of this._signals) {
            try {
                win.disconnect(id);
            } catch (e) {
                // The window is already gone.
            }
        }
        this._signals = [];
        for (const id of this._timeouts) {
            try {
                GLib.source_remove(id);
            } catch (e) {
                // The timer already ran.
            }
        }
        this._timeouts = [];
        this._armed = new Set();
        if (this._dbus) {
            this._dbus.unexport();
            this._dbus = null;
        }
    }

    _arm(win) {
        if (!win || this._armed.has(win))
            return;
        this._armed.add(win);
        pinWindow(win);
        try {
            const shown = win.connect('shown', () => {
                pinWindow(win);
            });
            this._signals.push([win, shown]);
        } catch (e) {
            log(`gaming-companion pin: ${e}`);
        }
        const later = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 400, () => {
            pinWindow(win);
            return GLib.SOURCE_REMOVE;
        });
        this._timeouts.push(later);
    }

    _status() {
        const lines = [];
        let hinted = false;
        for (const actor of global.get_window_actors()) {
            const win = metaWindow(actor);
            if (!win)
                continue;
            if (matches(win)) {
                const above = pinWindow(win);
                lines.push(`${above ? 'above' : 'not-above'}: ${titleOf(win)}`);
                continue;
            }
            if (hinted)
                continue;
            const title = titleOf(win);
            if (title.includes('Loot sheet') || title.includes('Gaming Companion')) {
                hinted = true;
                lines.push(`saw: ${title} [${idsOf(win).join(', ')}]`);
            }
        }
        if (lines.length === 0)
            return 'missing';
        return lines.join('; ');
    }
}
