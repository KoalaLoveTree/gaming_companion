//! Opaque border around the page. The page does not receive these clicks.
//!
//! The gray edge resizes. A light line marks its inner side. The darker bar
//! under the top edge moves the window, and a second light line marks where
//! the page starts. In the overlay, a transparent ring outside this border
//! ignores the pointer.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::gdk::WindowEdge;
use gtk::prelude::{
    BoxExt, ButtonExt, ContainerExt, CssProviderExt, EventBoxExt, LabelExt, MenuButtonExt,
    PopoverExt, ScrolledWindowExt, StyleContextExt, WidgetExt, WidgetExtManual,
};

use crate::chrome::{BORDER, GRIP, MARGIN, TOP};
use crate::model::Plugin;
use crate::model::bindings::{self, Action, Bindings};

struct Pads {
    top: gtk::Box,
    bottom: gtk::Box,
    left: gtk::Box,
    right: gtk::Box,
}

thread_local! {
    static PADS: RefCell<Option<Pads>> = RefCell::new(None);
}

pub fn set_overlay(on: bool) {
    PADS.with(|pads| {
        let pads = pads.borrow();
        let Some(pads) = pads.as_ref() else {
            return;
        };
        let margin = if on { MARGIN } else { 0 };
        pads.top.set_size_request(-1, margin);
        pads.bottom.set_size_request(-1, margin);
        pads.left.set_size_request(margin, -1);
        pads.right.set_size_request(margin, -1);
    });
}

/// Pack the border into the window and return the box that should hold the webview.
/// `on_message` receives the same messages the page used to send: `overlay` and
/// `set:<shortcut>`.
pub fn mount(
    parent: &gtk::Box,
    window: &gtk::ApplicationWindow,
    hotkey: &str,
    hint: &str,
    overlay: bool,
    plugins: &[Plugin],
    active_plugin: &str,
    bindings: &Bindings,
    on_message: impl Fn(String) + 'static,
) -> gtk::Box {
    ensure_css();
    parent.set_app_paintable(true);
    parent.style_context().add_class("gc-clear");

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let top_pad = clear_pad();
    let bottom_pad = clear_pad();
    let left_pad = clear_pad();
    let right_pad = clear_pad();
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.style_context().add_class("gc-border");

    let move_h = (TOP - BORDER).max(BORDER);
    frame.pack_start(
        &edge_row(
            window,
            WindowEdge::NorthWest,
            WindowEdge::North,
            WindowEdge::NorthEast,
        ),
        false,
        false,
        0,
    );
    frame.pack_start(
        &move_bar(
            window, move_h, hotkey, hint, overlay, plugins, bindings, on_message,
        ),
        false,
        false,
        0,
    );

    let middle = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let center = gtk::Box::new(gtk::Orientation::Vertical, 0);
    middle.pack_start(
        &resize(window, WindowEdge::West, BORDER, -1, "gc-w"),
        false,
        false,
        0,
    );
    middle.pack_start(&center, true, true, 0);
    middle.pack_start(
        &resize(window, WindowEdge::East, BORDER, -1, "gc-e"),
        false,
        false,
        0,
    );
    frame.pack_start(&middle, true, true, 0);
    frame.pack_start(
        &edge_row(
            window,
            WindowEdge::SouthWest,
            WindowEdge::South,
            WindowEdge::SouthEast,
        ),
        false,
        false,
        0,
    );

    let band = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    band.pack_start(&left_pad, false, false, 0);
    band.pack_start(&frame, true, true, 0);
    band.pack_start(&right_pad, false, false, 0);

    root.pack_start(&top_pad, false, false, 0);
    root.pack_start(&band, true, true, 0);
    root.pack_start(&bottom_pad, false, false, 0);
    parent.pack_start(&root, true, true, 0);

    PADS.with(|pads| {
        *pads.borrow_mut() = Some(Pads {
            top: top_pad,
            bottom: bottom_pad,
            left: left_pad,
            right: right_pad,
        });
    });
    root.show_all();
    // The window is shown again later, and that walks every widget. Each list
    // opts out of that walk after its rows have already been shown once.
    conceal_popovers();
    select(active_plugin);
    center
}

fn clear_pad() -> gtk::Box {
    let pad = gtk::Box::new(gtk::Orientation::Vertical, 0);
    pad.set_app_paintable(true);
    pad.style_context().add_class("gc-clear");
    pad
}

fn edge_row(
    window: &gtk::ApplicationWindow,
    west: WindowEdge,
    edge: WindowEdge,
    east: WindowEdge,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.set_size_request(-1, BORDER);
    let side = if matches!(edge, WindowEdge::North) {
        "gc-n"
    } else {
        "gc-s"
    };
    let west_class = if matches!(west, WindowEdge::NorthWest) {
        "gc-nw"
    } else {
        "gc-sw"
    };
    let east_class = if matches!(east, WindowEdge::NorthEast) {
        "gc-ne"
    } else {
        "gc-se"
    };
    row.pack_start(
        &resize(window, west, BORDER, BORDER, west_class),
        false,
        false,
        0,
    );
    row.pack_start(&resize(window, edge, -1, BORDER, side), true, true, 0);
    row.pack_start(
        &resize(window, east, BORDER, BORDER, east_class),
        false,
        false,
        0,
    );
    row
}

fn resize(
    window: &gtk::ApplicationWindow,
    edge: WindowEdge,
    width: i32,
    height: i32,
    class: &str,
) -> gtk::EventBox {
    let widget = gtk::EventBox::new();
    widget.set_size_request(width, height);
    widget.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
    widget.style_context().add_class("gc-border");
    widget.style_context().add_class(class);
    paint(&widget, Shade::Edge, Some(edge));
    hover_cursor(&widget, cursor_name(edge));
    let window = window.clone();
    widget.connect_button_press_event(move |_, event| drag(event, &window, Some(edge)));
    widget
}

fn move_bar(
    window: &gtk::ApplicationWindow,
    height: i32,
    hotkey: &str,
    hint: &str,
    overlay: bool,
    plugins: &[Plugin],
    bindings: &Bindings,
    on_message: impl Fn(String) + 'static,
) -> gtk::EventBox {
    let widget = gtk::EventBox::new();
    widget.set_above_child(false);
    widget.set_size_request(-1, height);
    widget.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
    widget.style_context().add_class("gc-border");
    widget.style_context().add_class("gc-move");
    paint(&widget, Shade::Move, None);
    hover_cursor(&widget, "grab");
    install_settings(
        &widget, window, hotkey, hint, overlay, plugins, bindings, on_message,
    );
    let window = window.clone();
    widget.connect_button_press_event(move |widget, event| {
        // A click on Settings stays on the button. The rest of the bar moves.
        if event.window() != widget.window() {
            return gtk::glib::Propagation::Proceed;
        }
        drag(event, &window, None)
    });
    widget
}

#[derive(Clone, Copy)]
enum Shade {
    Edge,
    Move,
}

/// Fill the widget ourselves. The window is cleared to transparent, so a CSS
/// background on this border does not show.
fn paint(widget: &gtk::EventBox, shade: Shade, edge: Option<WindowEdge>) {
    widget.set_app_paintable(true);
    widget.connect_draw(move |widget, cr| {
        let width = widget.allocated_width() as f64;
        let height = widget.allocated_height() as f64;
        if width < 1.0 || height < 1.0 {
            return gtk::glib::Propagation::Stop;
        }
        let (red, green, blue) = match shade {
            Shade::Edge => (0.290, 0.290, 0.290),
            Shade::Move => (0.141, 0.141, 0.141),
        };
        cr.set_operator(gtk::cairo::Operator::Source);
        cr.set_source_rgb(red, green, blue);
        cr.rectangle(0.0, 0.0, width, height);
        let _ = cr.fill();
        cr.set_operator(gtk::cairo::Operator::Over);
        cr.set_source_rgb(0.949, 0.949, 0.949);
        cr.set_line_width(1.0);
        match edge {
            Some(WindowEdge::North) | None => hline(cr, height - 0.5, width),
            Some(WindowEdge::South) => hline(cr, 0.5, width),
            Some(WindowEdge::West) => vline(cr, width - 0.5, height),
            Some(WindowEdge::East) => vline(cr, 0.5, height),
            Some(WindowEdge::NorthWest) => {
                hline(cr, height - 0.5, width);
                vline(cr, width - 0.5, height);
            }
            Some(WindowEdge::NorthEast) => {
                hline(cr, height - 0.5, width);
                vline(cr, 0.5, height);
            }
            Some(WindowEdge::SouthWest) => {
                hline(cr, 0.5, width);
                vline(cr, width - 0.5, height);
            }
            Some(WindowEdge::SouthEast) => {
                hline(cr, 0.5, width);
                vline(cr, 0.5, height);
            }
            Some(_) => {}
        }
        if matches!(shade, Shade::Move) {
            cr.set_source_rgb(0.86, 0.86, 0.86);
            let top = (height - 10.0) / 2.0;
            for (x, y) in [
                (12.0, top),
                (18.0, top),
                (12.0, top + 6.0),
                (18.0, top + 6.0),
            ] {
                cr.arc(x + 2.0, y + 2.0, 1.7, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }
            for child in widget.children() {
                widget.propagate_draw(&child, cr);
            }
        }
        gtk::glib::Propagation::Stop
    });
}

fn hline(cr: &gtk::cairo::Context, y: f64, width: f64) {
    cr.move_to(0.0, y);
    cr.line_to(width, y);
    let _ = cr.stroke();
}

fn vline(cr: &gtk::cairo::Context, x: f64, height: f64) {
    cr.move_to(x, 0.0);
    cr.line_to(x, height);
    let _ = cr.stroke();
}

#[derive(Clone)]
struct ChordRow {
    field: gtk::Label,
    change: gtk::Button,
    saved: Rc<RefCell<String>>,
}

struct SettingsUi {
    popover: gtk::Popover,
    rows: Vec<ChordRow>,
    recording: Rc<Cell<Option<usize>>>,
    pending: Rc<Cell<Option<usize>>>,
    status: gtk::Label,
    mode: gtk::Button,
    keymap: Rc<RefCell<Vec<(Action, String)>>>,
}

struct NavPlugin {
    id: String,
    button: gtk::Button,
}

struct NavUi {
    popover: gtk::Popover,
    plugins: Vec<NavPlugin>,
}

struct Keys {
    recording: Rc<Cell<Option<usize>>>,
    pending: Rc<Cell<Option<usize>>>,
    draft: Rc<RefCell<Option<String>>>,
    hint: Rc<RefCell<String>>,
    rows: Vec<ChordRow>,
    status: gtk::Label,
    keymap: Rc<RefCell<Vec<(Action, String)>>>,
    on_message: Rc<dyn Fn(String)>,
    plugins_popover: gtk::Popover,
    settings_popover: gtk::Popover,
    /// Key events can arrive on the window and on an open list. One event is handled once.
    handled: Cell<u32>,
}

thread_local! {
    static SETTINGS: RefCell<Option<SettingsUi>> = const { RefCell::new(None) };
    static NAV: RefCell<Option<NavUi>> = const { RefCell::new(None) };
}

/// Mark the open plugin in the list.
pub fn select(id: &str) {
    NAV.with(|nav| {
        let nav = nav.borrow();
        let Some(ui) = nav.as_ref() else {
            return;
        };
        for plugin in &ui.plugins {
            if plugin.id == id {
                plugin.button.style_context().add_class("gc-chrome-on");
            } else {
                plugin.button.style_context().remove_class("gc-chrome-on");
            }
        }
    });
}

/// Show the saved Show or hide shortcut, the status line, and which mode the button offers.
pub fn apply(hotkey: &str, status: &str, overlay: bool) {
    SETTINGS.with(|settings| {
        let ui = settings.borrow();
        let Some(ui) = ui.as_ref() else {
            return;
        };
        if ui.recording.get() != Some(0) && ui.pending.get() != Some(0) {
            if let Some(row) = ui.rows.first() {
                *row.saved.borrow_mut() = hotkey.to_string();
                paint_saved(row);
            }
        }
        ui.status.set_text(status);
        ui.mode
            .set_label(if overlay { "Window" } else { "Overlay" });
    });
}

/// Show the saved page shortcuts. Show or hide is updated by [`apply`].
pub fn show_bindings(next: &Bindings) {
    SETTINGS.with(|settings| {
        let ui = settings.borrow();
        let Some(ui) = ui.as_ref() else {
            return;
        };
        let pairs = bindings::pairs(next);
        *ui.keymap.borrow_mut() = pairs.clone();
        for (action, canon) in pairs {
            let index = action.index() + 1;
            if ui.recording.get() == Some(index) || ui.pending.get() == Some(index) {
                continue;
            }
            let Some(row) = ui.rows.get(index) else {
                continue;
            };
            *row.saved.borrow_mut() = canon;
            paint_saved(row);
        }
    });
}

fn paint_saved(row: &ChordRow) {
    row.field.set_text(&bindings::display(&row.saved.borrow()));
    row.change.set_label("Change");
}

fn conceal_popovers() {
    let conceal = |popover: &gtk::Popover| {
        popover.show_all();
        popover.set_no_show_all(true);
        popover.popdown();
    };
    NAV.with(|nav| {
        if let Some(ui) = nav.borrow().as_ref() {
            conceal(&ui.popover);
        }
    });
    SETTINGS.with(|settings| {
        if let Some(ui) = settings.borrow().as_ref() {
            conceal(&ui.popover);
        }
    });
}

fn install_settings(
    bar: &gtk::EventBox,
    window: &gtk::ApplicationWindow,
    hotkey: &str,
    hint: &str,
    overlay: bool,
    plugins: &[Plugin],
    bindings: &Bindings,
    on_message: impl Fn(String) + 'static,
) {
    let on_message: Rc<dyn Fn(String)> = Rc::new(on_message);
    let recording: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let pending: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
    let draft: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let hint = Rc::new(RefCell::new(hint.to_string()));
    let keymap = Rc::new(RefCell::new(bindings::pairs(bindings)));

    let slot = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    slot.set_hexpand(true);
    slot.set_valign(gtk::Align::Center);
    slot.set_margin_start(GRIP);
    slot.set_margin_end(8);
    slot.set_margin_top(2);
    slot.set_margin_bottom(2);

    let plugins_button = menu_button("Plugins");
    let settings_button = menu_button("Settings");
    settings_button.set_halign(gtk::Align::End);

    let plugins_popover = gtk::Popover::new(Some(&plugins_button));
    plugins_popover.set_position(gtk::PositionType::Bottom);
    plugins_button.set_popover(Some(&plugins_popover));
    fill_plugin_list(&plugins_popover, plugins, &on_message);

    let settings_popover = gtk::Popover::new(Some(&settings_button));
    settings_popover.set_position(gtk::PositionType::Bottom);
    settings_button.set_popover(Some(&settings_popover));
    let (rows, status, mode) = fill_settings_list(
        &settings_popover,
        hotkey,
        &hint.borrow(),
        overlay,
        bindings,
        &on_message,
        &recording,
        &pending,
        &draft,
    );

    slot.pack_start(&plugins_button, false, false, 0);
    slot.pack_end(&settings_button, false, false, 0);
    bar.add(&slot);

    {
        let settings_popover = settings_popover.clone();
        plugins_button.connect_clicked(move |_| {
            settings_popover.popdown();
        });
    }
    {
        let plugins_popover = plugins_popover.clone();
        settings_button.connect_clicked(move |_| {
            plugins_popover.popdown();
        });
    }

    let keys = Rc::new(Keys {
        recording: recording.clone(),
        pending: pending.clone(),
        draft,
        hint: hint.clone(),
        rows: rows.clone(),
        status: status.clone(),
        keymap: keymap.clone(),
        on_message,
        plugins_popover: plugins_popover.clone(),
        settings_popover: settings_popover.clone(),
        handled: Cell::new(0),
    });
    listen_for_shortcut(window, &settings_popover, &plugins_popover, keys);

    {
        let recording = recording.clone();
        let pending = pending.clone();
        let rows = rows.clone();
        let status = status.clone();
        let hint = hint.clone();
        settings_popover.connect_closed(move |_| {
            recording.set(None);
            pending.set(None);
            for row in &rows {
                paint_saved(row);
            }
            status.set_text(&hint.borrow());
        });
    }

    SETTINGS.with(|settings| {
        *settings.borrow_mut() = Some(SettingsUi {
            popover: settings_popover,
            rows,
            recording,
            pending,
            status,
            mode,
            keymap,
        });
    });
}

fn fill_plugin_list(popover: &gtk::Popover, plugins: &[Plugin], on_message: &Rc<dyn Fn(String)>) {
    let scrolled = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_propagate_natural_width(true);
    scrolled.set_propagate_natural_height(true);
    scrolled.set_max_content_height(360);
    scrolled.set_min_content_width(220);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    list.set_margin_top(4);
    list.set_margin_bottom(6);
    list.set_margin_start(8);
    list.set_margin_end(8);

    let mut nav_plugins = Vec::new();
    let mut last_game = None;
    for plugin in plugins {
        if last_game.as_deref() != Some(plugin.game_id.as_str()) {
            let title = gtk::Label::new(Some(&plugin.game_name));
            title.set_xalign(0.0);
            title.set_margin_top(6);
            title.style_context().add_class("gc-game");
            list.pack_start(&title, false, false, 0);
            last_game = Some(plugin.game_id.clone());
        }
        let button = gtk::Button::new();
        let label = gtk::Label::new(Some(&plugin.name));
        label.set_xalign(0.0);
        label.set_halign(gtk::Align::Start);
        button.add(&label);
        button.set_halign(gtk::Align::Fill);
        button.set_margin_start(12);
        style_chrome(&button);
        button.set_halign(gtk::Align::Fill);
        let message = on_message.clone();
        let id = plugin.id.clone();
        let popover = popover.clone();
        button.connect_clicked(move |_| {
            message(format!("open:{id}"));
            popover.popdown();
        });
        list.pack_start(&button, false, false, 0);
        nav_plugins.push(NavPlugin {
            id: plugin.id.clone(),
            button,
        });
    }

    scrolled.add(&list);
    popover.add(&scrolled);
    NAV.with(|nav| {
        *nav.borrow_mut() = Some(NavUi {
            popover: popover.clone(),
            plugins: nav_plugins,
        });
    });
}

fn fill_settings_list(
    popover: &gtk::Popover,
    hotkey: &str,
    hint: &str,
    overlay: bool,
    bindings: &Bindings,
    on_message: &Rc<dyn Fn(String)>,
    recording: &Rc<Cell<Option<usize>>>,
    pending: &Rc<Cell<Option<usize>>>,
    draft: &Rc<RefCell<Option<String>>>,
) -> (Vec<ChordRow>, gtk::Label, gtk::Button) {
    let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    list.set_margin_top(8);
    list.set_margin_bottom(8);
    list.set_margin_start(8);
    list.set_margin_end(8);

    let mut specs = vec![("Show or hide".to_string(), hotkey.to_string(), None)];
    for action in Action::ALL {
        specs.push((
            action.label().to_string(),
            bindings.chord(action).to_string(),
            Some(action),
        ));
    }

    let status = gtk::Label::new(Some(hint));
    status.set_xalign(0.0);
    status.set_line_wrap(true);
    status.set_max_width_chars(42);
    status.style_context().add_class("gc-status");

    let mut rows = Vec::new();
    let mut save_buttons = Vec::new();
    for (title, chord, _action) in &specs {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let name = gtk::Label::new(Some(title));
        name.set_xalign(0.0);
        name.set_width_chars(12);
        let field = gtk::Label::new(Some(&bindings::display(chord)));
        field.set_xalign(0.0);
        field.set_width_chars(16);
        field.set_ellipsize(gtk::pango::EllipsizeMode::End);
        field.style_context().add_class("gc-field");
        let change = gtk::Button::with_label("Change");
        style_chrome(&change);
        let save = gtk::Button::with_label("Save");
        style_chrome(&save);
        row_box.pack_start(&name, false, false, 0);
        row_box.pack_start(&field, true, true, 0);
        row_box.pack_start(&change, false, false, 0);
        row_box.pack_start(&save, false, false, 0);
        list.pack_start(&row_box, false, false, 0);
        rows.push(ChordRow {
            field,
            change,
            saved: Rc::new(RefCell::new(chord.clone())),
        });
        save_buttons.push(save);
    }

    for (index, row) in rows.iter().enumerate() {
        wire_row(
            index,
            specs[index].2,
            row,
            &save_buttons[index],
            &rows,
            recording,
            pending,
            draft,
            hint,
            &status,
            on_message,
        );
    }

    let note = gtk::Label::new(Some("Only Show or hide works while the game is focused."));
    note.set_xalign(0.0);
    note.set_line_wrap(true);
    note.set_max_width_chars(42);
    note.style_context().add_class("gc-note");
    list.pack_start(&note, false, false, 0);
    list.pack_start(&status, false, false, 0);

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let mode = gtk::Button::with_label(if overlay { "Window" } else { "Overlay" });
    style_chrome(&mode);
    let close = gtk::Button::with_label("Close");
    style_chrome(&close);
    footer.pack_start(&mode, false, false, 0);
    footer.pack_end(&close, false, false, 0);
    list.pack_start(&footer, false, false, 0);
    popover.add(&list);

    {
        let on_message = on_message.clone();
        mode.connect_clicked(move |_| on_message("overlay".to_string()));
    }
    {
        let popover = popover.clone();
        close.connect_clicked(move |_| {
            popover.popdown();
        });
    }

    (rows, status, mode)
}

fn wire_row(
    index: usize,
    action: Option<Action>,
    row: &ChordRow,
    save: &gtk::Button,
    rows: &[ChordRow],
    recording: &Rc<Cell<Option<usize>>>,
    pending: &Rc<Cell<Option<usize>>>,
    draft: &Rc<RefCell<Option<String>>>,
    hint: &str,
    status: &gtk::Label,
    on_message: &Rc<dyn Fn(String)>,
) {
    let rows_for_change = rows.to_vec();
    let recording_for_change = recording.clone();
    let pending_for_change = pending.clone();
    let draft_for_change = draft.clone();
    let hint_for_change = hint.to_string();
    let status_for_change = status.clone();
    row.change.connect_clicked(move |button| {
        if recording_for_change.get() == Some(index) {
            recording_for_change.set(None);
            pending_for_change.set(None);
            *draft_for_change.borrow_mut() = None;
            for row in &rows_for_change {
                paint_saved(row);
            }
            status_for_change.set_text(&hint_for_change);
            return;
        }
        recording_for_change.set(Some(index));
        pending_for_change.set(None);
        *draft_for_change.borrow_mut() = None;
        for (n, row) in rows_for_change.iter().enumerate() {
            if n == index {
                row.field.set_text("Press a shortcut");
                row.change.set_label("Cancel");
            } else {
                paint_saved(row);
            }
        }
        status_for_change.set_text("Press the keys");
        button.grab_focus();
    });

    let recording = recording.clone();
    let pending = pending.clone();
    let draft = draft.clone();
    let status = status.clone();
    let on_message = on_message.clone();
    let message = action.map(Action::id);
    save.connect_clicked(move |_| {
        if pending.get() != Some(index) {
            status.set_text("Press Change, then the keys");
            return;
        }
        let Some(value) = draft.borrow().clone() else {
            status.set_text("Press Change, then the keys");
            return;
        };
        pending.set(None);
        recording.set(None);
        *draft.borrow_mut() = None;
        if let Some(message) = message {
            on_message(format!("bind:{message}:{value}"));
        } else {
            on_message(format!("set:{value}"));
        }
    });
}

fn menu_button(label: &str) -> gtk::MenuButton {
    let button = gtk::MenuButton::new();
    let text = gtk::Label::new(Some(label));
    button.add(&text);
    style_chrome(&button);
    button
}

fn style_chrome(widget: &impl gtk::prelude::WidgetExt) {
    widget.set_halign(gtk::Align::Center);
    widget.set_valign(gtk::Align::Center);
    widget.style_context().add_class("gc-chrome-button");
    hover_cursor(widget, "pointer");
}

impl Keys {
    fn on_key(&self, event: &gtk::gdk::EventKey) -> gtk::glib::Propagation {
        let serial = event.time();
        if serial != 0 && self.handled.get() == serial {
            return gtk::glib::Propagation::Stop;
        }
        let result = self.dispatch(event);
        if matches!(result, gtk::glib::Propagation::Stop) {
            self.handled.set(serial);
        }
        result
    }

    fn dispatch(&self, event: &gtk::gdk::EventKey) -> gtk::glib::Propagation {
        if let Some(index) = self.recording.get() {
            return self.capture(event, index);
        }
        let (ctrl, alt, shift, super_key, token) = chord_parts(event);
        if token == "escape" && !ctrl && !alt && !shift && !super_key {
            if self.plugins_popover.is_visible() || self.settings_popover.is_visible() {
                self.plugins_popover.popdown();
                self.settings_popover.popdown();
                return gtk::glib::Propagation::Stop;
            }
            (self.on_message)("escape".to_string());
            return gtk::glib::Propagation::Proceed;
        }
        if event.is_modifier() || token.is_empty() {
            return gtk::glib::Propagation::Proceed;
        }
        let chord = chord_text(ctrl, alt, shift, super_key, &token);
        let pairs = self.keymap.borrow();
        if let Some(action) = bindings::find(&pairs, &chord) {
            (self.on_message)(action.id().to_string());
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    }

    fn capture(&self, event: &gtk::gdk::EventKey, index: usize) -> gtk::glib::Propagation {
        if event.is_modifier() {
            return gtk::glib::Propagation::Stop;
        }
        let (ctrl, alt, shift, super_key, token) = chord_parts(event);
        if token == "escape" && !ctrl && !alt && !shift && !super_key {
            self.recording.set(None);
            self.pending.set(None);
            *self.draft.borrow_mut() = None;
            for row in &self.rows {
                paint_saved(row);
            }
            self.status.set_text(&self.hint.borrow());
            return gtk::glib::Propagation::Stop;
        }
        if token.is_empty() {
            return gtk::glib::Propagation::Stop;
        }
        let chord = chord_text(ctrl, alt, shift, super_key, &token);
        self.recording.set(None);
        self.pending.set(Some(index));
        *self.draft.borrow_mut() = Some(chord.clone());
        if let Some(row) = self.rows.get(index) {
            row.field.set_text(&bindings::display(&chord));
            row.change.set_label("Change");
        }
        self.status.set_text("Press Save");
        gtk::glib::Propagation::Stop
    }
}

fn chord_parts(event: &gtk::gdk::EventKey) -> (bool, bool, bool, bool, String) {
    let state = event.state();
    let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
    let alt = state.contains(gtk::gdk::ModifierType::MOD1_MASK);
    let shift = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
    let super_key = state.contains(gtk::gdk::ModifierType::MOD4_MASK);
    let token = event
        .keyval()
        .to_lower()
        .name()
        .map(|name| name.to_ascii_lowercase().replace('_', ""))
        .unwrap_or_default();
    (ctrl, alt, shift, super_key, token)
}

fn chord_text(ctrl: bool, alt: bool, shift: bool, super_key: bool, token: &str) -> String {
    let mut parts = Vec::new();
    if ctrl {
        parts.push("ctrl");
    }
    if alt {
        parts.push("alt");
    }
    if shift {
        parts.push("shift");
    }
    if super_key {
        parts.push("super");
    }
    parts.push(token);
    parts.join("+")
}

fn listen_for_shortcut(
    window: &gtk::ApplicationWindow,
    settings_popover: &gtk::Popover,
    plugins_popover: &gtk::Popover,
    keys: Rc<Keys>,
) {
    fn attach(widget: &impl gtk::prelude::WidgetExt, keys: Rc<Keys>) {
        widget.add_events(gtk::gdk::EventMask::KEY_PRESS_MASK);
        widget.connect_key_press_event(move |_, event| keys.on_key(event));
    }
    attach(window, keys.clone());
    attach(settings_popover, keys.clone());
    attach(plugins_popover, keys);
}
fn cursor_name(edge: WindowEdge) -> &'static str {
    match edge {
        WindowEdge::North | WindowEdge::South => "ns-resize",
        WindowEdge::East | WindowEdge::West => "ew-resize",
        WindowEdge::NorthWest => "nw-resize",
        WindowEdge::SouthEast => "se-resize",
        WindowEdge::NorthEast => "ne-resize",
        WindowEdge::SouthWest => "sw-resize",
        _ => "default",
    }
}

/// The border does not pick up a CSS cursor on its own. Set it when the pointer
/// enters, before the click.
fn hover_cursor(widget: &impl gtk::prelude::WidgetExt, name: &'static str) {
    widget.add_events(
        gtk::gdk::EventMask::ENTER_NOTIFY_MASK | gtk::gdk::EventMask::LEAVE_NOTIFY_MASK,
    );
    widget.connect_enter_notify_event(move |widget, _| {
        set_widget_cursor(widget, Some(name));
        gtk::glib::Propagation::Proceed
    });
    widget.connect_leave_notify_event(move |widget, event| {
        if event.detail() != gtk::gdk::NotifyType::Inferior {
            set_widget_cursor(widget, None);
        }
        gtk::glib::Propagation::Proceed
    });
}

fn set_widget_cursor(widget: &impl gtk::prelude::WidgetExt, name: Option<&str>) {
    let Some(window) = widget.window() else {
        return;
    };
    let cursor = name.and_then(|name| {
        gtk::gdk::Cursor::from_name(&widget.display(), name)
            .or_else(|| cursor_fallback(widget, name))
    });
    window.set_cursor(cursor.as_ref());
}

fn cursor_fallback(widget: &impl gtk::prelude::WidgetExt, name: &str) -> Option<gtk::gdk::Cursor> {
    let kind = match name {
        "ns-resize" => gtk::gdk::CursorType::SbVDoubleArrow,
        "ew-resize" => gtk::gdk::CursorType::SbHDoubleArrow,
        "nw-resize" => gtk::gdk::CursorType::TopLeftCorner,
        "se-resize" => gtk::gdk::CursorType::BottomRightCorner,
        "ne-resize" => gtk::gdk::CursorType::TopRightCorner,
        "sw-resize" => gtk::gdk::CursorType::BottomLeftCorner,
        "grab" => gtk::gdk::CursorType::Fleur,
        "pointer" => gtk::gdk::CursorType::Hand2,
        _ => return None,
    };
    gtk::gdk::Cursor::for_display(&widget.display(), kind)
}

fn drag(
    event: &gtk::gdk::EventButton,
    window: &gtk::ApplicationWindow,
    edge: Option<WindowEdge>,
) -> gtk::glib::Propagation {
    use gtk::prelude::GtkWindowExt;
    if event.button() != 1 {
        return gtk::glib::Propagation::Proceed;
    }
    let (x, y) = event.root();
    let (x, y) = (x.round() as i32, y.round() as i32);
    match edge {
        Some(edge) => window.begin_resize_drag(edge, 1, x, y, event.time()),
        None => window.begin_move_drag(1, x, y, event.time()),
    }
    gtk::glib::Propagation::Stop
}

fn ensure_css() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let provider = gtk::CssProvider::new();
        if provider
            .load_from_data(
                b".gc-border { background-color: #4a4a4a; }
                  .gc-move { background-color: #242424; }
                  .gc-clear { background-color: transparent; }
                  .gc-chrome-button {
                    min-height: 0;
                    padding: 2px 8px;
                    border-radius: 6px;
                    border: 1px solid #4a4a4a;
                    background-image: none;
                    background-color: #2a2a2a;
                    color: #f2f2f2;
                    box-shadow: none;
                    font-size: 13px;
                  }
                  .gc-chrome-button:hover { background-color: #3a3a3a; }
                  .gc-chrome-on {
                    background-color: #111111;
                    border-color: #f2f2f2;
                  }
                  .gc-field {
                    color: #f2f2f2;
                    background-color: #111111;
                    border-radius: 6px;
                    padding: 2px 8px;
                    font-size: 13px;
                  }
                  .gc-status, .gc-note, .gc-game { color: #d7d7d7; font-size: 12px; }
                  .gc-game { font-weight: bold; }
                  popover, popover box, popover scrolledwindow, popover viewport {
                    background-color: #242424;
                    color: #f2f2f2;
                  }
                  menubutton.gc-chrome-button arrow, .gc-chrome-button arrow {
                    opacity: 0;
                    min-width: 0;
                    min-height: 0;
                  }
                  .gc-n, .gc-s { cursor: ns-resize; }
                  .gc-e, .gc-w { cursor: ew-resize; }
                  .gc-nw, .gc-se { cursor: nwse-resize; }
                  .gc-ne, .gc-sw { cursor: nesw-resize; }
                  .gc-move { cursor: grab; }",
            )
            .is_err()
        {
            return;
        }
        let Some(screen) = gtk::gdk::Screen::default() else {
            return;
        };
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}
