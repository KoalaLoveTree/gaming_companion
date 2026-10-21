use serde::{Deserialize, Serialize};

use super::bindings::Bindings;
use super::hotkey;
use crate::{Error, Result};

pub const DEFAULT_WIDTH: f64 = 1000.0;
pub const DEFAULT_HEIGHT: f64 = 800.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowFrame {
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    /// Set once x and y came from the compositor. A client guess on Wayland is not restored.
    #[serde(default, skip_serializing_if = "is_false")]
    pub placed: bool,
}

impl Default for WindowFrame {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            x: None,
            y: None,
            placed: false,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Where to open this frame.
/// `require_observed` skips a position the client invented, which on Wayland is not where the window was.
pub(crate) fn restore_origin(frame: &WindowFrame, require_observed: bool) -> Option<(f64, f64)> {
    if require_observed && !frame.placed {
        return None;
    }
    let (Some(x), Some(y)) = (frame.x, frame.y) else {
        return None;
    };
    if x.is_finite() && y.is_finite() {
        Some((x, y))
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    pub active_plugin: String,
    #[serde(default)]
    pub active_resource: Option<String>,
    /// Canonical shortcut, for example `ctrl+alt+space`. Each machine has its own.
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default)]
    pub window: WindowFrame,
    /// Dim panel over the game. Each machine chooses this on its own.
    #[serde(default)]
    pub overlay: bool,
    /// Saved overlay size. Absent until the panel has been opened once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay_window: Option<WindowFrame>,
    /// Page shortcuts. Show or hide stays in `hotkey`.
    #[serde(default)]
    pub bindings: Bindings,
}

fn default_hotkey() -> String {
    hotkey::DEFAULT.to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            active_plugin: "sample".to_string(),
            active_resource: None,
            hotkey: default_hotkey(),
            window: WindowFrame::default(),
            overlay: false,
            overlay_window: None,
            bindings: Bindings::default(),
        }
    }
}

/// Right-hand panel: about 40% of the monitor, leaving the crosshair in the game.
pub(crate) fn overlay_panel(
    origin_x: f64,
    origin_y: f64,
    monitor_w: f64,
    monitor_h: f64,
) -> WindowFrame {
    let width = if monitor_w <= 420.0 {
        monitor_w
    } else {
        (monitor_w * 0.4).clamp(360.0, monitor_w - 48.0)
    };
    let height = if monitor_h <= 300.0 {
        monitor_h
    } else {
        monitor_h - 48.0
    };
    WindowFrame {
        width,
        height,
        x: Some(origin_x + monitor_w - width),
        y: Some(origin_y + 24.0),
        placed: false,
    }
}

impl AppConfig {
    pub fn parse(text: &str) -> Result<Self> {
        let config: Self =
            toml::from_str(text).map_err(|err| Error::new(format!("config.toml: {err}")))?;
        config.check()?;
        Ok(config)
    }

    pub fn to_toml(&self) -> Result<String> {
        self.check()?;
        toml::to_string_pretty(self).map_err(|err| Error::new(format!("config.toml: {err}")))
    }

    fn check(&self) -> Result<()> {
        if self.active_plugin.is_empty() {
            return Err(Error::new("config active_plugin is empty"));
        }
        if let Some(resource) = &self.active_resource
            && resource.is_empty()
        {
            return Err(Error::new("config active_resource is empty"));
        }
        let frame = &self.window;
        if !usable_length(frame.width) || !usable_length(frame.height) {
            return Err(Error::new("config window size is invalid"));
        }
        if !frame.x.map(f64::is_finite).unwrap_or(true)
            || !frame.y.map(f64::is_finite).unwrap_or(true)
        {
            return Err(Error::new("config window position is invalid"));
        }
        hotkey::parse(&self.hotkey).map_err(|err| Error::new(format!("config shortcut: {err}")))?;
        if let Some(frame) = &self.overlay_window {
            if !usable_length(frame.width) || !usable_length(frame.height) {
                return Err(Error::new("config overlay size is invalid"));
            }
            if !frame.x.map(f64::is_finite).unwrap_or(true)
                || !frame.y.map(f64::is_finite).unwrap_or(true)
            {
                return Err(Error::new("config overlay position is invalid"));
            }
        }
        self.bindings.check(&self.hotkey)?;
        Ok(())
    }
}

fn usable_length(value: f64) -> bool {
    value.is_finite() && (200.0..=10_000.0).contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_the_open_plugin_and_frame() {
        let config = AppConfig {
            active_plugin: "sample".to_string(),
            active_resource: Some("loot".to_string()),
            hotkey: hotkey::DEFAULT.to_string(),
            window: WindowFrame {
                width: 520.0,
                height: 900.0,
                x: Some(40.0),
                y: None,
                placed: false,
            },
            overlay: false,
            overlay_window: None,
            bindings: Bindings::default(),
        };
        let parsed = AppConfig::parse(&config.to_toml().unwrap()).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn missing_file_shape_uses_defaults_for_optional_fields() {
        let parsed = AppConfig::parse("active_plugin = \"sample\"\n").unwrap();
        assert_eq!(parsed.active_resource, None);
        assert_eq!(parsed.window.width, DEFAULT_WIDTH);
        assert_eq!(parsed.window.x, None);
    }

    #[test]
    fn a_zero_window_is_rejected() {
        let text = "active_plugin = \"sample\"\n[window]\nwidth = 0\nheight = 800\n";
        assert!(AppConfig::parse(text).is_err());
    }

    #[test]
    fn a_missing_shortcut_uses_ctrl_alt_space() {
        let parsed = AppConfig::parse("active_plugin = \"sample\"\n").unwrap();
        assert_eq!(parsed.hotkey, hotkey::DEFAULT);
    }

    #[test]
    fn a_blank_or_broken_shortcut_is_rejected() {
        assert!(AppConfig::parse("active_plugin = \"sample\"\nhotkey = \"\"\n").is_err());
        assert!(AppConfig::parse("active_plugin = \"sample\"\nhotkey = \"nope\"\n").is_err());
    }

    #[test]
    fn round_trip_keeps_the_shortcut() {
        let config = AppConfig {
            hotkey: "ctrl+shift+a".to_string(),
            ..AppConfig::default()
        };
        let parsed = AppConfig::parse(&config.to_toml().unwrap()).unwrap();
        assert_eq!(parsed.hotkey, "ctrl+shift+a");
    }

    #[test]
    fn a_missing_overlay_stays_a_normal_window() {
        let parsed = AppConfig::parse("active_plugin = \"sample\"\n").unwrap();
        assert!(!parsed.overlay);
        assert_eq!(parsed.overlay_window, None);
    }

    #[test]
    fn the_overlay_sits_on_the_right_of_a_wide_monitor() {
        let panel = overlay_panel(0.0, 0.0, 2560.0, 1440.0);
        assert_eq!(panel.width, 1024.0);
        assert_eq!(panel.height, 1392.0);
        assert_eq!(panel.x, Some(1536.0));
        assert_eq!(panel.y, Some(24.0));
        assert!(panel.x.unwrap() > 2560.0 / 2.0);
    }

    #[test]
    fn an_unobserved_origin_is_not_restored() {
        let guessed = WindowFrame {
            x: Some(0.0),
            y: Some(0.0),
            ..WindowFrame::default()
        };
        assert_eq!(restore_origin(&guessed, true), None);
        assert_eq!(restore_origin(&guessed, false), Some((0.0, 0.0)));
        let saved = WindowFrame {
            placed: true,
            ..guessed
        };
        assert_eq!(restore_origin(&saved, true), Some((0.0, 0.0)));
        let config = AppConfig {
            window: saved,
            ..AppConfig::default()
        };
        let parsed = AppConfig::parse(&config.to_toml().unwrap()).unwrap();
        assert!(parsed.window.placed);
        assert_eq!(parsed.window.x, Some(0.0));
    }

    #[test]
    fn a_missing_keymap_uses_the_page_defaults() {
        let parsed = AppConfig::parse("active_plugin = \"sample\"\n").unwrap();
        assert_eq!(parsed.bindings, Bindings::default());
    }

    #[test]
    fn the_keymap_is_stored_as_named_fields() {
        let text = AppConfig::default().to_toml().unwrap();
        assert!(text.contains("back = \"alt+left\""));
        assert!(text.contains("fill = \"f11\""));
        let parsed =
            AppConfig::parse("active_plugin = \"sample\"\n[bindings]\nback = \"alt+a\"\n").unwrap();
        assert_eq!(
            parsed.bindings.chord(crate::model::bindings::Action::Back),
            "alt+a"
        );
        assert_eq!(
            parsed
                .bindings
                .chord(crate::model::bindings::Action::ZoomIn),
            "ctrl+equal"
        );
        assert_eq!(
            parsed.bindings.chord(crate::model::bindings::Action::Fill),
            "f11"
        );
    }

    #[test]
    fn a_page_shortcut_that_repeats_show_or_hide_is_rejected() {
        let text = "active_plugin = \"sample\"\nhotkey = \"alt+left\"\n";
        let err = AppConfig::parse(text).unwrap_err();
        assert!(err.to_string().contains("Already used by Show or hide"));
    }

    #[test]
    fn a_missing_placed_flag_stays_unobserved() {
        let parsed = AppConfig::parse(
            "active_plugin = \"sample\"\n[window]\nwidth = 800\nheight = 600\nx = 0\ny = 0\n",
        )
        .unwrap();
        assert!(!parsed.window.placed);
        assert_eq!(restore_origin(&parsed.window, true), None);
    }
}
