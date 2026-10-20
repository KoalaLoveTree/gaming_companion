//! Shortcuts that work while the sheet is focused.
//!
//! Show or hide stays on [`super::config::AppConfig::hotkey`]. It is the only
//! shortcut that works while the game is focused. These chords do not.

use serde::{Deserialize, Serialize};

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

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|action| *action == self)
            .unwrap_or(0)
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bindings {
    #[serde(default = "back_default")]
    pub back: String,
    #[serde(default = "forward_default")]
    pub forward: String,
    #[serde(default = "reload_default")]
    pub reload: String,
    #[serde(default = "zoom_in_default")]
    pub zoom_in: String,
    #[serde(default = "zoom_out_default")]
    pub zoom_out: String,
    #[serde(default = "zoom_reset_default")]
    pub zoom_reset: String,
    #[serde(default = "fill_default")]
    pub fill: String,
}

fn back_default() -> String {
    "alt+left".to_string()
}
fn forward_default() -> String {
    "alt+right".to_string()
}
fn reload_default() -> String {
    "ctrl+r".to_string()
}
fn zoom_in_default() -> String {
    "ctrl+equal".to_string()
}
fn zoom_out_default() -> String {
    "ctrl+minus".to_string()
}
fn zoom_reset_default() -> String {
    "ctrl+0".to_string()
}
fn fill_default() -> String {
    "f11".to_string()
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            back: back_default(),
            forward: forward_default(),
            reload: reload_default(),
            zoom_in: zoom_in_default(),
            zoom_out: zoom_out_default(),
            zoom_reset: zoom_reset_default(),
            fill: fill_default(),
        }
    }
}

impl Bindings {
    pub fn chord(&self, action: Action) -> &str {
        match action {
            Action::Back => &self.back,
            Action::Forward => &self.forward,
            Action::Reload => &self.reload,
            Action::ZoomIn => &self.zoom_in,
            Action::ZoomOut => &self.zoom_out,
            Action::ZoomReset => &self.zoom_reset,
            Action::Fill => &self.fill,
        }
    }

    pub fn set(&mut self, action: Action, chord: &str) {
        let slot = match action {
            Action::Back => &mut self.back,
            Action::Forward => &mut self.forward,
            Action::Reload => &mut self.reload,
            Action::ZoomIn => &mut self.zoom_in,
            Action::ZoomOut => &mut self.zoom_out,
            Action::ZoomReset => &mut self.zoom_reset,
            Action::Fill => &mut self.fill,
        };
        *slot = chord.to_string();
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
    matches!(hotkey.key, Key::F(5))
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
        assert_eq!(bindings.back, "alt+left");
        assert_eq!(bindings.fill, "f11");
        assert_eq!(display(&bindings.zoom_in), "ctrl+plus");
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
        bindings.forward = bindings.back.clone();
        assert!(
            bindings
                .check(hotkey::DEFAULT)
                .unwrap_err()
                .to_string()
                .contains("Already used by Back")
        );
        bindings = Bindings::default();
        bindings.zoom_in = "f5".to_string();
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
