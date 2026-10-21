//! Shortcuts that work while the sheet is focused.
//!
//! Show or hide stays on [`super::config::AppConfig::hotkey`]. It is the only
//! shortcut that works while the game is focused. These chords do not.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::hotkey::{self, Hotkey, Key};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Back,
    Forward,
    Reload,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Fill,
}

impl Action {
    pub const ALL: [Action; 7] = [
        Action::Back,
        Action::Forward,
        Action::Reload,
        Action::ZoomIn,
        Action::ZoomOut,
        Action::ZoomReset,
        Action::Fill,
    ];

    pub const fn index(self) -> usize {
        match self {
            Action::Back => 0,
            Action::Forward => 1,
            Action::Reload => 2,
            Action::ZoomIn => 3,
            Action::ZoomOut => 4,
            Action::ZoomReset => 5,
            Action::Fill => 6,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Action::Back => "back",
            Action::Forward => "forward",
            Action::Reload => "reload",
            Action::ZoomIn => "zoom-in",
            Action::ZoomOut => "zoom-out",
            Action::ZoomReset => "zoom-reset",
            Action::Fill => "fill",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::Back => "Back",
            Action::Forward => "Forward",
            Action::Reload => "Reload",
            Action::ZoomIn => "Zoom in",
            Action::ZoomOut => "Zoom out",
            Action::ZoomReset => "Reset zoom",
            Action::Fill => "Fill screen",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.id() == id)
    }
}

/// Page shortcuts, in [`Action::ALL`] order.
///
/// `config.toml` still stores one named field per action (`back`, `zoom_in`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
    chords: [String; 7],
}

const DEFAULTS: [&str; 7] = [
    "alt+left",
    "alt+right",
    "ctrl+r",
    "ctrl+equal",
    "ctrl+minus",
    "ctrl+0",
    "f11",
];

/// The on-disk shape. Field names are the config file.
#[derive(Serialize, Deserialize)]
struct StoredBindings {
    #[serde(default = "default_back")]
    back: String,
    #[serde(default = "default_forward")]
    forward: String,
    #[serde(default = "default_reload")]
    reload: String,
    #[serde(default = "default_zoom_in")]
    zoom_in: String,
    #[serde(default = "default_zoom_out")]
    zoom_out: String,
    #[serde(default = "default_zoom_reset")]
    zoom_reset: String,
    #[serde(default = "default_fill")]
    fill: String,
}

fn default_back() -> String {
    DEFAULTS[Action::Back.index()].to_string()
}
fn default_forward() -> String {
    DEFAULTS[Action::Forward.index()].to_string()
}
fn default_reload() -> String {
    DEFAULTS[Action::Reload.index()].to_string()
}
fn default_zoom_in() -> String {
    DEFAULTS[Action::ZoomIn.index()].to_string()
}
fn default_zoom_out() -> String {
    DEFAULTS[Action::ZoomOut.index()].to_string()
}
fn default_zoom_reset() -> String {
    DEFAULTS[Action::ZoomReset.index()].to_string()
}
fn default_fill() -> String {
    DEFAULTS[Action::Fill.index()].to_string()
}

impl StoredBindings {
    fn from_bindings(bindings: &Bindings) -> Self {
        Self {
            back: bindings.chord(Action::Back).to_string(),
            forward: bindings.chord(Action::Forward).to_string(),
            reload: bindings.chord(Action::Reload).to_string(),
            zoom_in: bindings.chord(Action::ZoomIn).to_string(),
            zoom_out: bindings.chord(Action::ZoomOut).to_string(),
            zoom_reset: bindings.chord(Action::ZoomReset).to_string(),
            fill: bindings.chord(Action::Fill).to_string(),
        }
    }

    fn into_bindings(self) -> Bindings {
        Bindings {
            chords: [
                self.back,
                self.forward,
                self.reload,
                self.zoom_in,
                self.zoom_out,
                self.zoom_reset,
                self.fill,
            ],
        }
    }
}

impl Serialize for Bindings {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        StoredBindings::from_bindings(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Bindings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        StoredBindings::deserialize(deserializer).map(StoredBindings::into_bindings)
    }
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            chords: DEFAULTS.map(str::to_string),
        }
    }
}

impl Bindings {
    pub fn chord(&self, action: Action) -> &str {
        &self.chords[action.index()]
    }

    pub fn set(&mut self, action: Action, chord: &str) {
        self.chords[action.index()] = chord.to_string();
    }

    /// Each chord parses, none repeat, and none steal Show or hide or F5.
    pub fn check(&self, show_hide: &str) -> Result<()> {
        let show = normalize(&hotkey::parse(show_hide)?.canonical());
        let mut used = vec![("Show or hide".to_string(), show)];
        for action in Action::ALL {
            let parsed = hotkey::parse(self.chord(action))?;
            if reserved_reload(&parsed) {
                return Err(Error::new("F5 reloads the page"));
            }
            let canon = normalize(&parsed.canonical());
            if let Some((name, _)) = used.iter().find(|(_, existing)| existing == &canon) {
                return Err(Error::new(format!("Already used by {name}")));
            }
            used.push((action.label().to_string(), canon));
        }
        Ok(())
    }
}

/// Collapse Ctrl+Plus and Ctrl+Equal onto one chord. The stored form stays
/// `equal`, which is what the parser already calls that key.
pub fn normalize(canonical: &str) -> String {
    let mut parts: Vec<&str> = canonical
        .split('+')
        .filter(|part| !part.is_empty())
        .collect();
    let zoom_key = parts
        .last()
        .is_some_and(|key| matches!(*key, "equal" | "plus" | "kpadd" | "add"));
    if zoom_key {
        parts.retain(|part| *part != "shift");
        if let Some(last) = parts.last_mut() {
            *last = "equal";
        }
    }
    parts.join("+")
}

/// `ctrl+equal` is shown as `ctrl+plus`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn display(canonical: &str) -> String {
    canonical.replace("equal", "plus")
}

/// Bare F5 always reloads. It is not a shortcut someone can rebind.
pub fn reserved_reload(hotkey: &Hotkey) -> bool {
    matches!(hotkey.key, Key::F(key) if key.number() == 5)
        && !hotkey.ctrl
        && !hotkey.alt
        && !hotkey.shift
        && !hotkey.super_key
}

/// Parse a typed chord into the form we store. F5 is refused.
pub fn normalized(text: &str) -> Result<String> {
    let parsed = hotkey::parse(text)?;
    if reserved_reload(&parsed) {
        return Err(Error::new("F5 reloads the page"));
    }
    let canon = normalize(&parsed.canonical());
    hotkey::parse(&canon)?;
    Ok(canon)
}

/// Saved chords are already normalized. `chord` is whatever the key event built.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn find(pairs: &[(Action, String)], chord: &str) -> Option<Action> {
    let parsed = hotkey::parse(chord).ok()?;
    if reserved_reload(&parsed) {
        return Some(Action::Reload);
    }
    let canon = normalize(&parsed.canonical());
    pairs
        .iter()
        .find(|(_, saved)| saved == &canon)
        .map(|(action, _)| *action)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn pairs(bindings: &Bindings) -> Vec<(Action, String)> {
    Action::ALL
        .into_iter()
        .filter_map(|action| {
            let saved = normalize(&hotkey::parse(bindings.chord(action)).ok()?.canonical());
            Some((action, saved))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_do_not_repeat_show_or_hide() {
        let bindings = Bindings::default();
        bindings.check(hotkey::DEFAULT).unwrap();
        assert_eq!(bindings.chord(Action::Back), "alt+left");
        assert_eq!(bindings.chord(Action::Fill), "f11");
        assert_eq!(display(bindings.chord(Action::ZoomIn)), "ctrl+plus");
        for action in Action::ALL {
            assert_eq!(Action::ALL[action.index()], action);
        }
    }

    #[test]
    fn plus_and_shift_equal_match_the_zoom_in_default() {
        let pairs = pairs(&Bindings::default());
        assert_eq!(find(&pairs, "ctrl+plus"), Some(Action::ZoomIn));
        assert_eq!(find(&pairs, "ctrl+shift+equal"), Some(Action::ZoomIn));
        assert_eq!(find(&pairs, "ctrl+equal"), Some(Action::ZoomIn));
        assert_eq!(find(&pairs, "f5"), Some(Action::Reload));
        assert_eq!(find(&pairs, "f11"), Some(Action::Fill));
        assert_eq!(normalize("ctrl+shift+equal"), "ctrl+equal");
        assert_eq!(normalize("ctrl+shift+a"), "ctrl+shift+a");
    }

    #[test]
    fn a_repeated_chord_and_bare_f5_are_rejected() {
        let mut bindings = Bindings::default();
        let back = bindings.chord(Action::Back).to_string();
        bindings.set(Action::Forward, &back);
        assert!(
            bindings
                .check(hotkey::DEFAULT)
                .unwrap_err()
                .to_string()
                .contains("Already used by Back")
        );
        bindings = Bindings::default();
        bindings.set(Action::ZoomIn, "f5");
        assert!(
            bindings
                .check(hotkey::DEFAULT)
                .unwrap_err()
                .to_string()
                .contains("F5 reloads the page")
        );
        bindings = Bindings::default();
        assert!(
            bindings
                .check("alt+left")
                .unwrap_err()
                .to_string()
                .contains("Already used by Show or hide")
        );
    }
}
