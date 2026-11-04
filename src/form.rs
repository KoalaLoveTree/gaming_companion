//! Linux page form. The dialog checks a link and shows it before Save writes the folder.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::{
    BoxExt, ButtonExt, ComboBoxExt, ComboBoxExtManual, ComboBoxTextExt, ContainerExt, DialogExt,
    EditableSignals, EntryExt, GtkWindowExt, LabelExt, MessageDialogExt, ScrolledWindowExt,
    TextBufferExt, TextViewExt, WidgetExt,
};
use webkit2gtk::WebViewExt;
use webkit2gtk::SettingsExt;

use crate::controller::pages::{self, Draft, DraftKind, SaveBody, SaveRequest};
use crate::controller::paths;
use crate::model::Plugin;

pub fn open_add(
    window: &gtk::ApplicationWindow,
    plugins: &[Plugin],
    on_message: Rc<dyn Fn(String)>,
) {
    present(window, plugins, None, on_message);
}

pub fn open_edit(
    window: &gtk::ApplicationWindow,
    plugins: &[Plugin],
    plugin: &Plugin,
    on_message: Rc<dyn Fn(String)>,
) {
    match pages::read_draft(plugin) {
        Ok(draft) => present(window, plugins, Some(draft), on_message),
        Err(err) => {
            message(
                window,
                gtk::MessageType::Warning,
                gtk::ButtonsType::Ok,
                &err.to_string(),
            );
        }
    }
}

fn present(
    window: &gtk::ApplicationWindow,
    plugins: &[Plugin],
    draft: Option<Draft>,
    on_message: Rc<dyn Fn(String)>,
) {
    let dialog = gtk::Dialog::new();
    dialog.set_title(if draft.is_some() { "Edit page" } else { "Add page" });
    dialog.set_transient_for(Some(window));
    dialog.set_modal(true);
    dialog.set_default_size(560, 640);

    let content = dialog.content_area();
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_spacing(8);

    let games = game_choices(plugins);
    let game = gtk::ComboBoxText::new();
    let new_game = gtk::Entry::new();
    new_game.set_placeholder_text(Some("Path of Exile"));
    let editing = draft.is_some();
    if let Some(draft) = &draft {
        game.append(Some(&draft.game_id), &draft.game_name);
        game.set_active_id(Some(&draft.game_id));
        game.set_sensitive(false);
        new_game.set_visible(false);
    } else {
        for (id, name) in &games {
            game.append(Some(id), name);
        }
        game.append(Some("new"), "New game");
        if games.is_empty() {
            game.set_active_id(Some("new"));
            new_game.set_visible(true);
        } else {
            game.set_active(Some(0));
            new_game.set_visible(false);
        }
    }

    let kind = gtk::ComboBoxText::new();
    kind.append(Some("link"), "Link");
    kind.append(Some("notes"), "Notes");
    let notes_draft = matches!(draft.as_ref().map(|item| &item.kind), Some(DraftKind::Notes(_)));
    kind.set_active_id(Some(if notes_draft { "notes" } else { "link" }));
    if editing {
        kind.set_sensitive(false);
    }

    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("League notes"));
    if let Some(draft) = &draft {
        name.set_text(&draft.name);
    }

    let url = gtk::Entry::new();
    url.set_placeholder_text(Some("maxroll.gg/build"));
    let original_url = match draft.as_ref().map(|item| &item.kind) {
        Some(DraftKind::Link(stored)) => {
            url.set_text(stored);
            Some(stored.clone())
        }
        _ => None,
    };
    let check = gtk::Button::with_label("Check");
    let url_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    url.set_hexpand(true);
    url_row.pack_start(&url, true, true, 0);
    url_row.pack_start(&check, false, false, 0);

    let notes = gtk::TextView::new();
    notes.set_wrap_mode(gtk::WrapMode::WordChar);
    if let Some(DraftKind::Notes(text)) = draft.as_ref().map(|item| &item.kind) {
        if let Some(buffer) = notes.buffer() {
            buffer.set_text(text);
        }
    }
    let notes_scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    notes_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    notes_scroll.set_min_content_height(160);
    notes_scroll.add(&notes);

    let status = gtk::Label::new(Some(""));
    status.set_xalign(0.0);
    status.set_line_wrap(true);
    status.set_max_width_chars(52);
    let final_url = gtk::Label::new(Some(""));
    final_url.set_xalign(0.0);
    final_url.set_line_wrap(true);
    final_url.set_selectable(true);
    final_url.set_max_width_chars(52);
    let warning = gtk::Label::new(Some(
        "This link is not protected. It was only available over http.",
    ));
    warning.set_xalign(0.0);
    warning.set_line_wrap(true);
    warning.set_max_width_chars(52);

    let context = webkit2gtk::WebContext::new();
    let preview = webkit2gtk::WebView::with_context(&context);
    preview.set_size_request(520, 260);
    if let Some(settings) = WebViewExt::settings(&preview) {
        settings.set_javascript_can_open_windows_automatically(false);
    }
    let preview_frame = gtk::Frame::new(None);
    preview_frame.add(&preview);
    preview_frame.set_visible(false);

    content.pack_start(&field_label("Game"), false, false, 0);
    content.pack_start(&game, false, false, 0);
    content.pack_start(&new_game, false, false, 0);
    content.pack_start(&field_label("Type"), false, false, 0);
    content.pack_start(&kind, false, false, 0);
    content.pack_start(&field_label("Name"), false, false, 0);
    content.pack_start(&name, false, false, 0);
    content.pack_start(&url_row, false, false, 0);
    content.pack_start(&notes_scroll, false, false, 0);
    content.pack_start(&status, false, false, 0);
    content.pack_start(&final_url, false, false, 0);
    content.pack_start(&warning, false, false, 0);
    content.pack_start(&preview_frame, true, true, 0);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let cancel = gtk::Button::with_label("Cancel");
    let save = gtk::Button::with_label("Save");
    buttons.pack_end(&save, false, false, 0);
    buttons.pack_end(&cancel, false, false, 0);
    content.pack_end(&buttons, false, false, 0);

    let form = Rc::new(Form {
        plugins: plugins.to_vec(),
        replace_id: draft.as_ref().map(|item| item.replace_id.clone()),
        game,
        new_game,
        kind,
        name,
        url,
        url_row,
        notes,
        notes_scroll,
        check,
        status,
        final_url,
        warning,
        preview,
        preview_frame,
        save,
        state: Rc::new(RefCell::new(CheckState {
            original_url,
            ..CheckState::default()
        })),
        closed: Rc::new(Cell::new(false)),
        generation: Rc::new(Cell::new(0)),
    });

    {
        let form = form.clone();
        let game = form.game.clone();
        game.connect_changed(move |_| {
            let new = form.game.active_id().as_deref() == Some("new");
            form.new_game.set_visible(new);
            form.paint();
        });
    }
    {
        let form = form.clone();
        let kind = form.kind.clone();
        kind.connect_changed(move |_| {
            form.apply_kind();
            form.paint();
        });
    }
    {
        let form = form.clone();
        let name = form.name.clone();
        name.connect_changed(move |_| form.paint());
    }
    {
        let form = form.clone();
        let new_game = form.new_game.clone();
        new_game.connect_changed(move |_| form.paint());
    }
    {
        let form = form.clone();
        let notes = form.notes.clone();
        if let Some(buffer) = notes.buffer() {
            buffer.connect_changed(move |_| form.paint());
        }
    }
    {
        let form = form.clone();
        let url = form.url.clone();
        url.connect_changed(move |_| {
            form.generation.set(form.generation.get().wrapping_add(1));
            form.state.borrow_mut().clear();
            form.preview.stop_loading();
            form.preview_frame.set_visible(false);
            form.paint();
        });
    }
    {
        let form = form.clone();
        let check = form.check.clone();
        let parent = dialog.clone();
        check.connect_clicked(move |_| form.start_check(&parent));
    }
    {
        let form = form.clone();
        let save = form.save.clone();
        let parent = dialog.clone();
        let on_message = on_message.clone();
        save.connect_clicked(move |_| {
            if let Some(id) = form.commit(&parent) {
                on_message(format!("refresh:{id}"));
                parent.close();
            }
        });
    }
    {
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| {
            dialog.close();
        });
    }
    {
        let form = form.clone();
        dialog.connect_destroy(move |_| {
            form.closed.set(true);
            form.generation.set(form.generation.get().wrapping_add(1));
        });
    }

    wire_preview(&form);
    form.apply_kind();
    form.paint();
    dialog.show_all();
    // show_all marks every child visible, including fields paint had hidden.
    form.apply_kind();
    form.new_game
        .set_visible(form.game.active_id().as_deref() == Some("new"));
    form.preview_frame.set_visible(false);
    form.paint();
    // The view keeps the context, and so does the dialog, until the dialog is destroyed.
    let context = context;
    dialog.connect_destroy(move |_| {
        let _ = &context;
    });
}

struct Form {
    plugins: Vec<Plugin>,
    replace_id: Option<String>,
    game: gtk::ComboBoxText,
    new_game: gtk::Entry,
    kind: gtk::ComboBoxText,
    name: gtk::Entry,
    url: gtk::Entry,
    url_row: gtk::Box,
    notes: gtk::TextView,
    notes_scroll: gtk::ScrolledWindow,
    check: gtk::Button,
    status: gtk::Label,
    final_url: gtk::Label,
    warning: gtk::Label,
    preview: webkit2gtk::WebView,
    preview_frame: gtk::Frame,
    save: gtk::Button,
    state: Rc<RefCell<CheckState>>,
    closed: Rc<Cell<bool>>,
    generation: Rc<Cell<u64>>,
}

#[derive(Default)]
struct CheckState {
    checking: bool,
    preview: bool,
    checked_input: Option<String>,
    resolved: Option<pages::Resolved>,
    /// The stored address when editing. Clearing a check does not drop this.
    original_url: Option<String>,
}

impl CheckState {
    fn clear(&mut self) {
        self.checking = false;
        self.preview = false;
        self.checked_input = None;
        self.resolved = None;
    }

    fn chosen(&self, typed: &str) -> Option<String> {
        if self.checked_input.as_deref() == Some(typed) {
            return self.resolved.as_ref().map(|item| item.url.clone());
        }
        if self.original_url.as_deref() == Some(typed) {
            return Some(typed.to_string());
        }
        None
    }
}

impl Form {
    fn is_link(&self) -> bool {
        self.kind.active_id().as_deref() != Some("notes")
    }

    fn apply_kind(&self) {
        let link = self.is_link();
        self.url_row.set_visible(link);
        self.notes_scroll.set_visible(!link);
        if !link {
            self.preview_frame.set_visible(false);
            self.warning.set_visible(false);
        }
    }

    fn typed_url(&self) -> String {
        self.url.text().trim().to_string()
    }

    fn chosen_url(&self) -> Option<String> {
        self.state.borrow().chosen(&self.typed_url())
    }

    fn paint(&self) {
        if self.closed.get() {
            return;
        }
        let state = self.state.borrow();
        let chosen = self.chosen_url();
        let unprotected = chosen
            .as_deref()
            .is_some_and(|url| url.starts_with("http://"));
        self.warning.set_visible(self.is_link() && unprotected);
        if self.is_link() {
            if let Some(url) = chosen.as_deref() {
                self.final_url.set_text(url);
            } else if !state.checking && !state.preview {
                self.final_url.set_text("");
            }
        } else if !state.checking && !state.preview {
            self.final_url.set_text("");
        }
        let ready = if state.checking || state.preview {
            false
        } else if !self.is_link() {
            !self.name.text().trim().is_empty() && self.game_ready()
        } else {
            chosen.is_some() && !self.name.text().trim().is_empty() && self.game_ready()
        };
        self.save.set_sensitive(ready);
        self.check.set_sensitive(self.is_link() && !state.checking && !state.preview);
    }

    fn game_ready(&self) -> bool {
        if self.replace_id.is_some() {
            return true;
        }
        if self.game.active_id().as_deref() == Some("new") {
            return !self.new_game.text().trim().is_empty();
        }
        self.game.active_id().is_some()
    }

    fn start_check(&self, parent: &gtk::Dialog) {
        if self.closed.get() || !self.is_link() {
            return;
        }
        let input = self.typed_url();
        self.generation.set(self.generation.get().wrapping_add(1));
        let generation = self.generation.get();
        {
            let mut state = self.state.borrow_mut();
            state.clear();
            state.checking = true;
        }
        self.status.set_text("Checking the link…");
        self.preview_frame.set_visible(false);
        self.paint();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(pages::resolve_link(&input));
        });
        let _ = parent;
        let status = self.status.clone();
        let generation_cell = self.generation.clone();
        let closed = self.closed.clone();
        let preview = self.preview.clone();
        let preview_frame = self.preview_frame.clone();
        let state = self.state.clone();
        let save = self.save.clone();
        let check = self.check.clone();
        let warning = self.warning.clone();
        let final_url = self.final_url.clone();
        let url_entry = self.url.clone();
        let name = self.name.clone();
        let game = self.game.clone();
        let new_game = self.new_game.clone();
        let kind = self.kind.clone();
        let replace = self.replace_id.clone();
        glib_wait(rx, move |result| {
            if closed.get() || generation_cell.get() != generation {
                return;
            }
            state.borrow_mut().checking = false;
            match result {
                Ok(resolved) => {
                    state.borrow_mut().preview = true;
                    status.set_text("Loading preview…");
                    preview_frame.set_visible(true);
                    preview.load_uri(&resolved.url);
                    state.borrow_mut().resolved = Some(resolved);
                    arm_timeout(
                        generation,
                        generation_cell.clone(),
                        closed.clone(),
                        state.clone(),
                        status.clone(),
                        save.clone(),
                        check.clone(),
                        warning.clone(),
                        final_url.clone(),
                        url_entry.clone(),
                        name.clone(),
                        game.clone(),
                        new_game.clone(),
                        kind.clone(),
                        replace.clone(),
                        preview.clone(),
                        preview_frame.clone(),
                    );
                }
                Err(err) => {
                    status.set_text(&err.to_string());
                    state.borrow_mut().clear();
                }
            }
            paint_parts(
                &state,
                &url_entry,
                &name,
                &game,
                &new_game,
                &kind,
                &replace,
                &save,
                &check,
                &warning,
                &final_url,
                &closed,
            );
        });
    }

    fn commit(&self, parent: &gtk::Dialog) -> Option<String> {
        let name = self.name.text().to_string();
        let (game_id, game_name) = if let Some(replace) = &self.replace_id {
            let plugin = self.plugins.iter().find(|plugin| &plugin.id == replace);
            match plugin {
                Some(plugin) => (plugin.game_id.clone(), plugin.game_name.clone()),
                None => {
                    self.status.set_text("That page is not in the list");
                    return None;
                }
            }
        } else if self.game.active_id().as_deref() == Some("new") {
            (String::new(), self.new_game.text().to_string())
        } else {
            match self.game.active_id() {
                Some(id) => {
                    let name = self
                        .plugins
                        .iter()
                        .find(|plugin| plugin.game_id == id.as_str())
                        .map(|plugin| plugin.game_name.clone())
                        .unwrap_or_default();
                    (id.to_string(), name)
                }
                None => {
                    self.status.set_text("Enter a game name");
                    return None;
                }
            }
        };
        let body = if self.is_link() {
            let Some(url) = self.chosen_url() else {
                self.status.set_text("Check the link first");
                return None;
            };
            if url.starts_with("http://")
                && !message(
                    parent,
                    gtk::MessageType::Warning,
                    gtk::ButtonsType::OkCancel,
                    pages::UNPROTECTED_CONFIRM,
                )
            {
                return None;
            }
            SaveBody::Link(url)
        } else {
            SaveBody::Notes(notes_text(&self.notes))
        };
        let user = match paths::user_plugins_dir() {
            Ok(path) => path,
            Err(err) => {
                self.status.set_text(&err.to_string());
                return None;
            }
        };
        match pages::save_page(
            &user,
            &self.plugins,
            &SaveRequest {
                game_id,
                game_name,
                name,
                replace_id: self.replace_id.clone(),
                body,
            },
        ) {
            Ok(id) => Some(id),
            Err(err) => {
                self.status.set_text(&err.to_string());
                None
            }
        }
    }
}

fn wire_preview(form: &Rc<Form>) {
    let state = form.state.clone();
    let status = form.status.clone();
    let save = form.save.clone();
    let check = form.check.clone();
    let warning = form.warning.clone();
    let final_url = form.final_url.clone();
    let url_entry = form.url.clone();
    let name = form.name.clone();
    let game = form.game.clone();
    let new_game = form.new_game.clone();
    let kind = form.kind.clone();
    let replace = form.replace_id.clone();
    let closed = form.closed.clone();
    let preview_frame = form.preview_frame.clone();
    form.preview.connect_load_changed(move |view, event| {
        if event != webkit2gtk::LoadEvent::Finished || closed.get() {
            return;
        }
        let uri = view.uri().map(|uri| uri.to_string()).unwrap_or_default();
        {
            let mut current = state.borrow_mut();
            if !current.preview {
                return;
            }
            current.preview = false;
            if pages::candidates(&uri).is_ok()
                && (uri.starts_with("https://") || uri.starts_with("http://"))
            {
                let typed = url_entry.text().trim().to_string();
                current.checked_input = Some(typed);
                current.resolved = Some(pages::Resolved {
                    unprotected: uri.starts_with("http://"),
                    url: uri,
                });
                status.set_text("");
            } else {
                current.resolved = None;
                current.checked_input = None;
                status.set_text("This link could not be opened");
                preview_frame.set_visible(false);
            }
        }
        paint_parts(
            &state,
            &url_entry,
            &name,
            &game,
            &new_game,
            &kind,
            &replace,
            &save,
            &check,
            &warning,
            &final_url,
            &closed,
        );
    });

    let state = form.state.clone();
    let status = form.status.clone();
    let save = form.save.clone();
    let check = form.check.clone();
    let warning = form.warning.clone();
    let final_url = form.final_url.clone();
    let url_entry = form.url.clone();
    let name = form.name.clone();
    let game = form.game.clone();
    let new_game = form.new_game.clone();
    let kind = form.kind.clone();
    let replace = form.replace_id.clone();
    let closed = form.closed.clone();
    let preview_frame = form.preview_frame.clone();
    form.preview.connect_load_failed(move |_, _, _, _| {
        if closed.get() {
            return true;
        }
        {
            let mut current = state.borrow_mut();
            current.preview = false;
            current.resolved = None;
            current.checked_input = None;
        }
        status.set_text("This link could not be opened");
        preview_frame.set_visible(false);
        paint_parts(
            &state,
            &url_entry,
            &name,
            &game,
            &new_game,
            &kind,
            &replace,
            &save,
            &check,
            &warning,
            &final_url,
            &closed,
        );
        true
    });
}

fn arm_timeout(
    generation: u64,
    generation_cell: Rc<Cell<u64>>,
    closed: Rc<Cell<bool>>,
    state: Rc<RefCell<CheckState>>,
    status: gtk::Label,
    save: gtk::Button,
    check: gtk::Button,
    warning: gtk::Label,
    final_url: gtk::Label,
    url_entry: gtk::Entry,
    name: gtk::Entry,
    game: gtk::ComboBoxText,
    new_game: gtk::Entry,
    kind: gtk::ComboBoxText,
    replace: Option<String>,
    preview: webkit2gtk::WebView,
    preview_frame: gtk::Frame,
) {
    glib::timeout_add_local(Duration::from_secs(15), move || {
        if !closed.get() && generation_cell.get() == generation && state.borrow().preview {
            preview.stop_loading();
            preview_frame.set_visible(false);
            state.borrow_mut().clear();
            status.set_text("This link could not be opened");
            paint_parts(
                &state,
                &url_entry,
                &name,
                &game,
                &new_game,
                &kind,
                &replace,
                &save,
                &check,
                &warning,
                &final_url,
                &closed,
            );
        }
        glib::ControlFlow::Break
    });
}

fn paint_parts(
    state: &RefCell<CheckState>,
    url_entry: &gtk::Entry,
    name: &gtk::Entry,
    game: &gtk::ComboBoxText,
    new_game: &gtk::Entry,
    kind: &gtk::ComboBoxText,
    replace: &Option<String>,
    save: &gtk::Button,
    check: &gtk::Button,
    warning: &gtk::Label,
    final_url: &gtk::Label,
    closed: &Cell<bool>,
) {
    if closed.get() {
        return;
    }
    let link = kind.active_id().as_deref() != Some("notes");
    let typed = url_entry.text().trim().to_string();
    let state = state.borrow();
    let chosen = state.chosen(&typed);
    let unprotected = chosen
        .as_deref()
        .is_some_and(|url| url.starts_with("http://"));
    warning.set_visible(link && unprotected);
    if let Some(url) = &chosen {
        final_url.set_text(url);
    }
    let game_ready = if replace.is_some() {
        true
    } else if game.active_id().as_deref() == Some("new") {
        !new_game.text().trim().is_empty()
    } else {
        game.active_id().is_some()
    };
    let ready = !state.checking
        && !state.preview
        && !name.text().trim().is_empty()
        && game_ready
        && (!link || chosen.is_some());
    save.set_sensitive(ready);
    check.set_sensitive(link && !state.checking && !state.preview);
}

fn notes_text(view: &gtk::TextView) -> String {
    let Some(buffer) = view.buffer() else {
        return String::new();
    };
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer
        .text(&start, &end, false)
        .map(|text| text.to_string())
        .unwrap_or_default()
}

fn glib_wait<T: 'static>(rx: std::sync::mpsc::Receiver<T>, apply: impl Fn(T) + 'static) {
    let apply = Rc::new(RefCell::new(Some(apply)));
    glib::timeout_add_local(Duration::from_millis(150), move || {
        match rx.try_recv() {
            Ok(value) => {
                if let Some(apply) = apply.borrow_mut().take() {
                    apply(value);
                }
                glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        }
    });
}

fn field_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label
}

fn game_choices(plugins: &[Plugin]) -> Vec<(String, String)> {
    let mut games = Vec::new();
    for plugin in plugins {
        if games.iter().any(|(id, _)| id == &plugin.game_id) {
            continue;
        }
        games.push((plugin.game_id.clone(), plugin.game_name.clone()));
    }
    games
}

fn message(
    parent: &impl gtk::prelude::IsA<gtk::Window>,
    kind: gtk::MessageType,
    buttons: gtk::ButtonsType,
    text: &str,
) -> bool {
    let dialog = gtk::MessageDialog::new(
        Some(parent),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        kind,
        buttons,
        "",
    );
    dialog.set_text(Some(text));
    let response = dialog.run();
    dialog.close();
    response == gtk::ResponseType::Ok || response == gtk::ResponseType::Accept
}
