//! Wayland global shortcut via the desktop portal.
//!
//! `global-hotkey` only talks to X11. On Wayland the shortcut has to be bound
//! through org.freedesktop.portal.GlobalShortcuts. The portal rejects a host
//! app until that app id is registered on this same connection, and the id
//! only resolves when the desktop file's Exec is a program that exists.
//! Register before GTK starts: opening the display can call a portal itself.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use dbus::Path as DbusPath;
use dbus::arg::{PropMap, RefArg, Variant};
use dbus::blocking::{Connection, Proxy};
use dbus::message::MatchRule;

use crate::model::hotkey::{self, Hotkey};
use crate::{Error, Result};

pub(crate) const APP_ID: &str = "com.gamingcompanion.Companion";
const DESKTOP_BUS: &str = "org.freedesktop.portal.Desktop";
const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
const SHORTCUT_ID: &str = "toggle";

pub(crate) enum Note {
    /// `activation_token` is the compositor token from this key press, when
    /// the portal sent one. Showing a window with it is allowed to take focus.
    Pressed {
        activation_token: Option<String>,
    },
    Bound(Hotkey),
    Failed(String),
}

enum Command {
    Listen(Box<dyn Fn(Note) + Send>),
    Bind(Hotkey),
}

pub(crate) struct Portal {
    commands: Sender<Command>,
}

impl Portal {
    pub(crate) fn listen<F>(&self, handler: F)
    where
        F: Fn(Note) + Send + 'static,
    {
        let _ = self.commands.send(Command::Listen(Box::new(handler)));
    }

    pub(crate) fn bind(&self, hotkey: Hotkey) {
        let _ = self.commands.send(Command::Bind(hotkey));
    }
}

/// Write the desktop file and register the app id. Returns once that is done,
/// before the caller opens a window. `parent` is the Wayland window handle,
/// filled in once the window exists, so the confirm dialog is placed on it.
pub(crate) fn prepare(parent: Arc<Mutex<String>>) -> Result<Portal> {
    write_desktop_file()?;
    let (commands_tx, commands_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("shortcut-portal".into())
        .spawn(move || portal_loop(commands_rx, ready_tx, parent))
        .map_err(|err| Error::new(format!("shortcut portal: {err}")))?;
    match ready_rx.recv() {
        Ok(result) => result?,
        Err(_) => return Err(Error::new("shortcut portal did not start")),
    }
    Ok(Portal {
        commands: commands_tx,
    })
}

fn write_desktop_file() -> Result<()> {
    let dir = dirs::data_dir()
        .ok_or_else(|| Error::new("could not find a user data directory"))?
        .join("applications");
    std::fs::create_dir_all(&dir)?;
    let icon_path = dir.join(format!("{APP_ID}.png"));
    std::fs::write(&icon_path, crate::mark::png(crate::mark::SWITCHER))
        .map_err(|err| Error::new(format!("writing {}: {err}", icon_path.display())))?;
    let exe = std::env::current_exe()?;
    let text = desktop_entry(&quote_exec(&exe), &icon_path);
    let path = dir.join(format!("{APP_ID}.desktop"));
    std::fs::write(&path, text)
        .map_err(|err| Error::new(format!("writing {}: {err}", path.display())))?;
    Ok(())
}

fn desktop_entry(exec: &str, icon: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Name=Gaming Companion\n\
         Comment=Show and hide the gaming companion\n\
         Type=Application\n\
         Terminal=false\n\
         StartupWMClass={APP_ID}\n\
         Icon={}\n\
         Exec={exec}\n",
        icon.display()
    )
}

fn quote_exec(path: &Path) -> String {
    let text = path.display().to_string();
    if text.contains([' ', '\t', '"', '\\']) {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        text
    }
}

fn portal_loop(
    commands: Receiver<Command>,
    ready: Sender<std::result::Result<(), Error>>,
    parent: Arc<Mutex<String>>,
) {
    let conn = match Connection::new_session() {
        Ok(conn) => conn,
        Err(err) => {
            let _ = ready.send(Err(Error::new(format!(
                "could not reach the shortcut portal: {err}"
            ))));
            return;
        }
    };
    if let Err(err) = register(&conn) {
        let _ = ready.send(Err(err));
        return;
    }
    let (incoming_tx, incoming_rx) = mpsc::channel();
    if let Err(err) = subscribe(&conn, &incoming_tx) {
        let _ = ready.send(Err(err));
        return;
    }
    eprintln!("gaming_companion: registered shortcut app {APP_ID}");
    let _ = ready.send(Ok(()));

    let mut state = State {
        parent,
        ..State::default()
    };
    loop {
        if let Err(err) = conn.process(Duration::from_millis(50)) {
            eprintln!("gaming_companion: shortcut portal stopped: {err}");
            state.fail(format!("the shortcut service stopped: {err}"));
            break;
        }
        loop {
            match commands.try_recv() {
                Ok(Command::Listen(handler)) => state.listener = Some(handler),
                Ok(Command::Bind(hotkey)) => state.request_bind(&conn, hotkey),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }
        while let Ok(incoming) = incoming_rx.try_recv() {
            state.on_incoming(&conn, incoming);
        }
    }
}

fn register(conn: &Connection) -> Result<()> {
    let proxy = desktop_proxy(conn);
    let options: PropMap = HashMap::new();
    proxy
        .method_call::<(), _, _, _>(
            "org.freedesktop.host.portal.Registry",
            "Register",
            (APP_ID, options),
        )
        .map_err(|err| Error::new(format!("could not register the shortcut app: {err}")))
}

fn subscribe(conn: &Connection, incoming: &Sender<Incoming>) -> Result<()> {
    let response_tx = incoming.clone();
    conn.add_match(
        MatchRule::new_signal("org.freedesktop.portal.Request", "Response").static_clone(),
        move |args: (u32, PropMap), _: &Connection, message: &dbus::Message| {
            let path = message
                .path()
                .map(|path| path.to_string())
                .unwrap_or_default();
            let _ = response_tx.send(Incoming::Response(path, args.0, args.1));
            true
        },
    )
    .map_err(|err| Error::new(format!("could not listen for shortcut replies: {err}")))?;

    let activated_tx = incoming.clone();
    conn.add_match(
        MatchRule::new_signal("org.freedesktop.portal.GlobalShortcuts", "Activated")
            .with_path(DESKTOP_PATH)
            .static_clone(),
        move |args: (DbusPath<'static>, String, u64, PropMap),
              _: &Connection,
              _: &dbus::Message| {
            let _ = activated_tx.send(Incoming::Activated(
                args.0.to_string(),
                args.1,
                activation_token(&args.3),
            ));
            true
        },
    )
    .map_err(|err| Error::new(format!("could not listen for the shortcut: {err}")))?;

    let changed_tx = incoming.clone();
    conn.add_match(
        MatchRule::new_signal("org.freedesktop.portal.GlobalShortcuts", "ShortcutsChanged")
            .with_path(DESKTOP_PATH)
            .static_clone(),
        move |args: (DbusPath<'static>, Vec<(String, PropMap)>),
              _: &Connection,
              _: &dbus::Message| {
            let _ = changed_tx.send(Incoming::Changed(args.0.to_string(), args.1));
            true
        },
    )
    .map_err(|err| Error::new(format!("could not listen for shortcut changes: {err}")))?;
    Ok(())
}

enum Incoming {
    Response(String, u32, PropMap),
    Activated(String, String, Option<String>),
    Changed(String, Vec<(String, PropMap)>),
}

enum Phase {
    Session,
    Bind,
}

struct Pending {
    request: String,
    hotkey: Hotkey,
    phase: Phase,
    session: Option<String>,
}

struct State {
    listener: Option<Box<dyn Fn(Note) + Send>>,
    /// Session whose shortcut is currently armed.
    session: Option<String>,
    bound: Option<Hotkey>,
    pending: Option<Pending>,
    queued: Option<Hotkey>,
    /// Trigger text seen while a bind dialog is open, if it parses.
    observed: Option<Hotkey>,
    last_press: Instant,
    /// `wayland:…` handle of our window. Empty until the window exports it.
    parent: Arc<Mutex<String>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            listener: None,
            session: None,
            bound: None,
            pending: None,
            queued: None,
            observed: None,
            last_press: Instant::now()
                .checked_sub(Duration::from_secs(10))
                .unwrap_or_else(Instant::now),
            parent: Arc::new(Mutex::new(String::new())),
        }
    }
}

impl State {
    fn request_bind(&mut self, conn: &Connection, hotkey: Hotkey) {
        if self.pending.is_some() {
            self.queued = Some(hotkey);
            return;
        }
        if self
            .bound
            .as_ref()
            .is_some_and(|bound| bound.canonical() == hotkey.canonical())
            && self.session.is_some()
        {
            return;
        }
        self.begin(conn, hotkey);
    }

    fn begin(&mut self, conn: &Connection, hotkey: Hotkey) {
        let handle_token = token("h");
        let session_token = token("s");
        let mut options = PropMap::new();
        options.insert("handle_token".into(), variant_str(&handle_token));
        options.insert("session_handle_token".into(), variant_str(&session_token));
        let proxy = desktop_proxy(conn);
        match proxy.method_call::<(DbusPath<'static>,), _, _, _>(
            "org.freedesktop.portal.GlobalShortcuts",
            "CreateSession",
            (options,),
        ) {
            Ok((path,)) => {
                self.pending = Some(Pending {
                    request: path.to_string(),
                    hotkey,
                    phase: Phase::Session,
                    session: Some(session_path(conn, &session_token)),
                });
            }
            Err(err) => self.fail(format!("could not open a shortcut session: {err}")),
        }
    }

    fn on_incoming(&mut self, conn: &Connection, incoming: Incoming) {
        match incoming {
            Incoming::Response(path, code, results) => {
                self.on_response(conn, &path, code, results);
            }
            Incoming::Activated(session, shortcut_id, activation_token) => {
                self.on_activated(session, shortcut_id, activation_token)
            }
            Incoming::Changed(session, shortcuts) => self.on_changed(session, shortcuts),
        }
    }

    fn on_response(&mut self, conn: &Connection, path: &str, code: u32, results: PropMap) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if pending.request != path {
            self.pending = Some(pending);
            return;
        }
        match pending.phase {
            Phase::Session => self.session_ready(conn, pending, code, &results),
            Phase::Bind => self.bind_ready(conn, pending, code, &results),
        }
    }

    fn session_ready(&mut self, conn: &Connection, pending: Pending, code: u32, results: &PropMap) {
        if code != 0 {
            self.fail(response_message(code));
            self.pump_queue(conn);
            return;
        }
        let session = prop_str(results, "session_handle")
            .filter(|path| path.starts_with('/'))
            .or(pending.session);
        let Some(session) = session else {
            self.fail("the shortcut session did not start".to_string());
            self.pump_queue(conn);
            return;
        };
        let hotkey = pending.hotkey;
        // GNOME keeps the first confirmed shortcut for this id and ignores a
        // later preferred trigger. Write the new keys into that store first,
        // so this bind grabs them instead of the old ones.
        if let Err(err) = publish_stored_shortcut(&hotkey) {
            close_session(conn, &session);
            self.fail(err);
            self.pump_queue(conn);
            return;
        }
        let handle_token = token("b");
        let mut options = PropMap::new();
        options.insert("handle_token".into(), variant_str(&handle_token));
        let mut shortcut = PropMap::new();
        shortcut.insert(
            "description".into(),
            variant_str("Show or hide Gaming Companion"),
        );
        shortcut.insert(
            "preferred_trigger".into(),
            variant_str(&hotkey.spec_trigger()),
        );
        let shortcuts = vec![(SHORTCUT_ID.to_string(), shortcut)];
        let session_path = match DbusPath::new(&session) {
            Ok(path) => path,
            Err(err) => {
                self.fail(format!("the shortcut session path is invalid: {err}"));
                self.pump_queue(conn);
                return;
            }
        };
        let parent_window = self
            .parent
            .lock()
            .map(|slot| slot.clone())
            .unwrap_or_default();
        if parent_window.is_empty() {
            eprintln!("gaming_companion: shortcut dialog has no parent window");
        } else {
            eprintln!("gaming_companion: shortcut dialog attached to this window");
        }
        let proxy = desktop_proxy(conn);
        match proxy.method_call::<(DbusPath<'static>,), _, _, _>(
            "org.freedesktop.portal.GlobalShortcuts",
            "BindShortcuts",
            (session_path, shortcuts, parent_window, options),
        ) {
            Ok((path,)) => {
                eprintln!(
                    "gaming_companion: waiting for confirmation of {}",
                    hotkey.canonical()
                );
                self.pending = Some(Pending {
                    request: path.to_string(),
                    hotkey,
                    phase: Phase::Bind,
                    session: Some(session),
                });
            }
            Err(err) => {
                self.restore_stored_shortcut();
                close_session(conn, &session);
                self.fail(format!("could not bind the shortcut: {err}"));
                self.pump_queue(conn);
            }
        }
    }

    fn bind_ready(&mut self, conn: &Connection, pending: Pending, code: u32, results: &PropMap) {
        let Some(session) = pending.session else {
            self.fail("the shortcut session did not start".to_string());
            self.pump_queue(conn);
            return;
        };
        if code != 0 {
            // The new session never became the armed one. Drop it and keep the old grab.
            self.restore_stored_shortcut();
            close_session(conn, &session);
            self.observed = None;
            self.fail(response_message(code));
            self.pump_queue(conn);
            return;
        }
        // The bind reply is the shortcut GNOME grabbed. An earlier change
        // signal can still be carrying the previous keys.
        let chosen = trigger_in_map(results)
            .or_else(|| self.observed.take())
            .unwrap_or(pending.hotkey);
        if let Some(old) = self.session.take()
            && old != session
        {
            close_session(conn, &old);
        }
        self.session = Some(session);
        self.bound = Some(chosen.clone());
        eprintln!("gaming_companion: shortcut bound {}", chosen.canonical());
        self.notify(Note::Bound(chosen));
        self.pump_queue(conn);
    }

    fn on_activated(
        &mut self,
        session: String,
        shortcut_id: String,
        activation_token: Option<String>,
    ) {
        if shortcut_id != SHORTCUT_ID || self.session.as_deref() != Some(session.as_str()) {
            return;
        }
        if self.last_press.elapsed() < Duration::from_millis(350) {
            return;
        }
        self.last_press = Instant::now();
        self.notify(Note::Pressed { activation_token });
    }

    fn on_changed(&mut self, session: String, shortcuts: Vec<(String, PropMap)>) {
        let Some(parsed) = shortcuts
            .iter()
            .find_map(|(_, props)| trigger_in_map(props))
        else {
            return;
        };
        let pending_session = self
            .pending
            .as_ref()
            .and_then(|pending| pending.session.as_deref());
        if pending_session == Some(session.as_str()) {
            self.observed = Some(parsed);
            return;
        }
        if self.session.as_deref() == Some(session.as_str())
            && self
                .bound
                .as_ref()
                .is_none_or(|bound| bound.canonical() != parsed.canonical())
        {
            self.bound = Some(parsed.clone());
            self.notify(Note::Bound(parsed));
        }
    }

    fn pump_queue(&mut self, conn: &Connection) {
        if self.pending.is_some() {
            return;
        }
        if let Some(hotkey) = self.queued.take() {
            self.request_bind(conn, hotkey);
        }
    }

    fn restore_stored_shortcut(&self) {
        if let Some(previous) = &self.bound {
            let _ = publish_stored_shortcut(previous);
        }
    }

    fn fail(&mut self, message: String) {
        eprintln!("gaming_companion: {message}");
        self.notify(Note::Failed(message));
    }

    fn notify(&self, note: Note) {
        if let Some(listener) = &self.listener {
            listener(note);
        }
    }
}

fn response_message(code: u32) -> String {
    match code {
        1 => "The shortcut was not confirmed".to_string(),
        _ => "The shortcut was rejected".to_string(),
    }
}

fn desktop_proxy(conn: &Connection) -> Proxy<'_, &Connection> {
    conn.with_proxy(DESKTOP_BUS, DESKTOP_PATH, Duration::from_secs(15))
}

fn close_session(conn: &Connection, path: &str) {
    let Ok(path) = DbusPath::new(path) else {
        return;
    };
    let proxy = conn.with_proxy(DESKTOP_BUS, path, Duration::from_secs(2));
    if let Err(err) =
        proxy.method_call::<(), _, _, _>("org.freedesktop.portal.Session", "Close", ())
    {
        eprintln!("gaming_companion: closing shortcut session: {err}");
    }
}

fn session_path(conn: &Connection, token: &str) -> String {
    let sender = conn.unique_name().trim_start_matches(':').replace('.', "_");
    format!("/org/freedesktop/portal/desktop/session/{sender}/{token}")
}

fn token(prefix: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{n}")
}

fn gnome_shortcut_schema() -> String {
    format!(
        "org.gnome.settings-daemon.global-shortcuts.application:/org/gnome/settings-daemon/global-shortcuts/{APP_ID}/"
    )
}

fn stored_shortcut_value(hotkey: &Hotkey) -> String {
    format!(
        "[('{SHORTCUT_ID}', {{'shortcuts': <['{}']>, 'description': <'Show or hide Gaming Companion'>}})]",
        hotkey.portal_trigger()
    )
}

/// Update GNOME's saved shortcut when this id is already confirmed.
/// A missing store, or a desktop without this setting, leaves the dialog in charge.
fn publish_stored_shortcut(hotkey: &Hotkey) -> std::result::Result<(), String> {
    let schema = gnome_shortcut_schema();
    let current = match std::process::Command::new("gsettings")
        .args(["get", &schema, "shortcuts"])
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => return Ok(()),
    };
    let quoted_id = format!("'{SHORTCUT_ID}'");
    if !current.contains(&quoted_id) {
        return Ok(());
    }
    let next = stored_shortcut_value(hotkey);
    if current == next {
        return Ok(());
    }
    eprintln!(
        "gaming_companion: updating stored shortcut to {}",
        hotkey.canonical()
    );
    let output = std::process::Command::new("gsettings")
        .args(["set", &schema, "shortcuts", &next])
        .output()
        .map_err(|err| format!("could not store the shortcut: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = detail.trim();
    if detail.is_empty() {
        Err("could not store the shortcut".to_string())
    } else {
        Err(format!("could not store the shortcut: {detail}"))
    }
}

fn variant_str(value: &str) -> Variant<Box<dyn RefArg>> {
    Variant(Box::new(value.to_string()))
}

fn prop_str(map: &PropMap, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|value| value.0.as_str().map(|text| text.to_string()))
}

/// GNOME names this `activation_token`. Older portal builds used a hyphen.
fn activation_token(map: &PropMap) -> Option<String> {
    ["activation_token", "activation-token"]
        .iter()
        .find_map(|key| {
            let token = prop_str(map, key)?;
            let token = token.trim();
            if token.is_empty() {
                None
            } else {
                Some(token.to_string())
            }
        })
}

/// The portal names the keys in `trigger_description` (`Press <ctrl><alt>space`).
/// Labels in the same dict are ignored.
fn trigger_in_map(map: &PropMap) -> Option<Hotkey> {
    if let Some(text) = prop_str(map, "trigger_description") {
        return hotkey::parse(&text).ok();
    }
    for value in map.values() {
        if let Some(found) = press_text(&*value.0, 0) {
            return Some(found);
        }
    }
    None
}

fn press_text(arg: &dyn RefArg, depth: u8) -> Option<Hotkey> {
    if depth > 8 {
        return None;
    }
    if let Some(text) = arg.as_str()
        && (text.starts_with("Press ") || text.starts_with("press ") || text.contains('<'))
        && let Ok(hotkey) = hotkey::parse(text)
    {
        return Some(hotkey);
    }
    let children = arg.as_iter()?;
    for child in children {
        if let Some(found) = press_text(child, depth + 1) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{desktop_entry, stored_shortcut_value};
    use crate::model::hotkey;

    #[test]
    fn a_changed_shortcut_uses_the_gnome_store_form() {
        let hotkey = hotkey::parse("ctrl+alt+space").unwrap();
        assert_eq!(
            stored_shortcut_value(&hotkey),
            "[('toggle', {'shortcuts': <['<Control><Alt>space']>, 'description': <'Show or hide Gaming Companion'>})]"
        );
    }

    #[test]
    fn the_desktop_entry_names_the_icon() {
        let icon =
            Path::new("/home/me/.local/share/applications/com.gamingcompanion.Companion.png");
        let text = desktop_entry("/usr/bin/gaming_companion", icon);
        assert!(text.contains(
            "Icon=/home/me/.local/share/applications/com.gamingcompanion.Companion.png\n"
        ));
        assert!(text.contains("StartupWMClass=com.gamingcompanion.Companion\n"));
        assert!(text.contains("Exec=/usr/bin/gaming_companion\n"));
    }
}
