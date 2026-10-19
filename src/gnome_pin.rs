//! GNOME Wayland ignores `gtk_window_set_keep_above` and a client window move.
//! Mutter still pins and moves a window when a shell extension asks, so the
//! companion installs one that watches only its own windows.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use dbus::arg::PropMap;
use dbus::blocking::{Connection, Proxy};

use crate::{Error, Result};

const UUID: &str = "gaming-companion@local";
const SHELL_BUS: &str = "org.gnome.Shell";
const SHELL_PATH: &str = "/org/gnome/Shell";
const PIN_PATH: &str = "/com/gamingcompanion/Pin";
const PIN_IFACE: &str = "com.gamingcompanion.Pin";
const EXTENSION_JS: &str = include_str!("gnome_pin.js");

static WAITING_FOR_LOGIN: AtomicBool = AtomicBool::new(false);

pub(crate) fn install() {
    if !gnome_wayland() {
        return;
    }
    if let Err(err) = install_extension() {
        eprintln!("gaming_companion: {err}");
    }
}

/// Ask the extension again. Hide and show drops Mutter's pin on this session.
pub(crate) fn request() {
    if !gnome_wayland() {
        return;
    }
    match pin_status() {
        Ok(status) => note(&status),
        Err(err) => note(&err.to_string()),
    }
}

pub(crate) fn schedule() {
    if !gnome_wayland() {
        return;
    }
    for delay_ms in [200_u64, 700, 1600] {
        gtk::glib::timeout_add_local(Duration::from_millis(delay_ms), || {
            request();
            gtk::glib::ControlFlow::Break
        });
    }
}

pub(crate) fn gnome_wayland() -> bool {
    let gnome = std::env::var("XDG_CURRENT_DESKTOP")
        .map(|desktop| desktop.to_ascii_uppercase().contains("GNOME"))
        .unwrap_or(false);
    gnome
        && std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var("XDG_SESSION_TYPE").as_deref() != Ok("x11")
}

fn install_extension() -> Result<()> {
    let dir = extension_dir()?;
    std::fs::create_dir_all(&dir)
        .map_err(|err| Error::new(format!("creating {}: {err}", dir.display())))?;
    let script_changed = write_if_changed(&dir.join("extension.js"), EXTENSION_JS)?;
    let meta_changed = write_if_changed(&dir.join("metadata.json"), &metadata()?)?;
    let changed = script_changed || meta_changed;

    let conn = Connection::new_session()
        .map_err(|err| Error::new(format!("could not reach GNOME Shell: {err}")))?;
    let proxy = conn.with_proxy(SHELL_BUS, SHELL_PATH, Duration::from_secs(5));
    let (enabled,): (bool,) = proxy
        .method_call("org.gnome.Shell.Extensions", "EnableExtension", (UUID,))
        .map_err(|err| Error::new(format!("could not enable the pin extension: {err}")))?;
    // The shell only reads new extension folders at login. Remembering the
    // id makes the next session pin the window. This session cannot.
    remember_enabled()?;
    if !enabled {
        if extension_known(&proxy) {
            return Err(Error::new(format!(
                "GNOME did not enable the pin extension ({})",
                extension_errors(&proxy)
            )));
        }
        WAITING_FOR_LOGIN.store(true, Ordering::Relaxed);
        eprintln!(
            "gaming_companion: GNOME will keep this window above others after the next login"
        );
        return Ok(());
    }
    if changed {
        eprintln!("gaming_companion: GNOME picks up window placement on the next login");
    }
    Ok(())
}

fn extension_dir() -> Result<PathBuf> {
    Ok(dirs::data_dir()
        .ok_or_else(|| Error::new("could not find a user data directory"))?
        .join("gnome-shell/extensions")
        .join(UUID))
}

fn metadata() -> Result<String> {
    let major = shell_major()?;
    Ok(format!(
        "{{\n  \
         \"uuid\": \"{UUID}\",\n  \
         \"name\": \"Gaming Companion\",\n  \
         \"description\": \"Keeps the Gaming Companion window above other windows and restores its place.\",\n  \
         \"shell-version\": [\"{major}\"],\n  \
         \"session-modes\": [\"user\"],\n  \
         \"version\": 2\n\
         }}\n"
    ))
}

fn shell_major() -> Result<String> {
    let output = std::process::Command::new("gnome-shell")
        .arg("--version")
        .output()
        .map_err(|err| Error::new(format!("could not read the GNOME version: {err}")))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let version = text.split_whitespace().last().unwrap_or("");
    let major = version.split('.').next().unwrap_or("");
    if major.is_empty() || !major.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::new(format!(
            "could not read the GNOME version from {text:?}"
        )));
    }
    Ok(major.to_string())
}

fn write_if_changed(path: &Path, body: &str) -> Result<bool> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(body) {
        return Ok(false);
    }
    std::fs::write(path, body)
        .map_err(|err| Error::new(format!("writing {}: {err}", path.display())))?;
    Ok(true)
}

fn extension_errors(proxy: &Proxy<'_, &Connection>) -> String {
    proxy
        .method_call::<(Vec<String>,), _, _, _>(
            "org.gnome.Shell.Extensions",
            "GetExtensionErrors",
            (UUID,),
        )
        .map(|(errors,)| {
            if errors.is_empty() {
                "no detail".to_string()
            } else {
                errors.join("; ")
            }
        })
        .unwrap_or_else(|err| err.to_string())
}

/// Layout position of this app's window, as the shell reports it.
/// Absent when the helper is not the one from this build, or the window is not mapped.
pub(crate) fn frame_origin() -> Option<(f64, f64)> {
    if !gnome_wayland() {
        return None;
    }
    let conn = Connection::new_session().ok()?;
    let proxy = conn.with_proxy(SHELL_BUS, PIN_PATH, Duration::from_millis(800));
    let (status,): (String,) = proxy.method_call(PIN_IFACE, "Frame", ()).ok()?;
    parse_frame(&status)
}

/// Move the window back after it maps. Wayland ignores a client position.
/// Later tries cover the moment before the shell has seen the window.
pub(crate) fn schedule_place(x: f64, y: f64) {
    if !gnome_wayland() {
        return;
    }
    let (Some(x), Some(y)) = (place_coord(x), place_coord(y)) else {
        return;
    };
    // Repeat after map. An early move can land before Mutter has placed the window.
    for delay_ms in [0_u64, 250, 700] {
        gtk::glib::timeout_add_local(Duration::from_millis(delay_ms), move || {
            let _ = place_now(x, y);
            gtk::glib::ControlFlow::Break
        });
    }
}

fn place_now(x: i32, y: i32) -> bool {
    let Ok(conn) = Connection::new_session() else {
        return false;
    };
    let proxy = conn.with_proxy(SHELL_BUS, PIN_PATH, Duration::from_millis(800));
    let Ok((status,)) = proxy.method_call::<(String,), _, _, _>(PIN_IFACE, "Place", (x, y)) else {
        return false;
    };
    status == "moved"
}

fn parse_frame(status: &str) -> Option<(f64, f64)> {
    let (x, y) = status.split_once(',')?;
    let x: f64 = x.trim().parse().ok()?;
    let y: f64 = y.trim().parse().ok()?;
    if x.is_finite() && y.is_finite() {
        Some((x, y))
    } else {
        None
    }
}

fn place_coord(value: f64) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    let rounded = value.round();
    if !(i32::MIN as f64..=i32::MAX as f64).contains(&rounded) {
        return None;
    }
    Some(rounded as i32)
}

fn pin_status() -> Result<String> {
    let conn = Connection::new_session()
        .map_err(|err| Error::new(format!("could not reach GNOME Shell: {err}")))?;
    let proxy = conn.with_proxy(SHELL_BUS, PIN_PATH, Duration::from_millis(800));
    let (status,): (String,) = proxy
        .method_call(PIN_IFACE, "Pin", ())
        .map_err(|err| Error::new(format!("pin: {err}")))?;
    Ok(status)
}

fn extension_known(proxy: &Proxy<'_, &Connection>) -> bool {
    proxy
        .method_call::<(PropMap,), _, _, _>(
            "org.gnome.Shell.Extensions",
            "GetExtensionInfo",
            (UUID,),
        )
        .map(|(info,)| !info.is_empty())
        .unwrap_or(false)
}

fn remember_enabled() -> Result<()> {
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.shell", "enabled-extensions"])
        .output()
        .map_err(|err| Error::new(format!("could not read enabled extensions: {err}")))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(Error::new(format!(
            "could not read enabled extensions: {}",
            detail.trim()
        )));
    }
    let current = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if current.contains(UUID) {
        return Ok(());
    }
    let next = if current == "@as []" || current == "[]" {
        format!("['{UUID}']")
    } else if let Some(body) = current.strip_suffix(']') {
        format!("{body}, '{UUID}']")
    } else {
        return Err(Error::new(format!(
            "could not read enabled extensions: {current}"
        )));
    };
    let set = std::process::Command::new("gsettings")
        .args(["set", "org.gnome.shell", "enabled-extensions", &next])
        .output()
        .map_err(|err| Error::new(format!("could not enable the pin extension: {err}")))?;
    if set.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&set.stderr);
    Err(Error::new(format!(
        "could not enable the pin extension: {}",
        detail.trim()
    )))
}

fn note(status: &str) {
    use std::sync::Mutex;
    static LOGGED_OK: AtomicBool = AtomicBool::new(false);
    static LAST_FAILURE: Mutex<String> = Mutex::new(String::new());
    if WAITING_FOR_LOGIN.load(Ordering::Relaxed) {
        return;
    }
    if status.starts_with("above:") {
        if !LOGGED_OK.swap(true, Ordering::Relaxed) {
            eprintln!("gaming_companion: window stays above other windows");
        }
        return;
    }
    let Ok(mut last) = LAST_FAILURE.lock() else {
        return;
    };
    if *last == status {
        return;
    }
    *last = status.to_string();
    eprintln!("gaming_companion: could not keep the window above others ({status})");
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_pin_extension_targets_this_app() {
        let source = include_str!("gnome_pin.js");
        assert!(source.contains("make_above"));
        assert!(source.contains("move_frame"));
        assert!(source.contains("com.gamingcompanion.Companion"));
        assert!(source.contains("gaming_companion"));
        assert!(source.contains("com.gamingcompanion.Pin"));
        assert!(source.contains("name=\"Frame\""));
        assert!(source.contains("name=\"Place\""));
    }

    #[test]
    fn a_shell_frame_is_two_coordinates() {
        assert_eq!(super::parse_frame("12,-4"), Some((12.0, -4.0)));
        assert_eq!(super::parse_frame("missing"), None);
        assert_eq!(super::parse_frame("nope,1"), None);
        assert_eq!(super::place_coord(40.4), Some(40));
        assert_eq!(super::place_coord(f64::NAN), None);
    }
}
