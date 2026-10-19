use global_hotkey::hotkey::{Code, HotKey, Modifiers};

use crate::{Error, Result};

pub const DEFAULT: &str = "ctrl+alt+space";

/// One global shortcut. Modifiers are independent of order in the text form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub key: Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Space,
    Tab,
    Escape,
    Enter,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    F(u8),
    Minus,
    Equal,
    Comma,
    Period,
    Slash,
    Backslash,
    Grave,
    BracketLeft,
    BracketRight,
    Semicolon,
    Quote,
}

impl Hotkey {
    /// `ctrl+alt+space`, modifiers in a fixed order.
    pub fn canonical(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("ctrl");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.shift {
            parts.push("shift");
        }
        if self.super_key {
            parts.push("super");
        }
        parts.push(self.key.canonical());
        parts.join("+")
    }

    /// Shortcut-spec trigger. GNOME converts this into a GTK accelerator.
    /// Sending the GTK form instead makes the dialog drop modifiers.
    pub fn spec_trigger(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("CTRL");
        }
        if self.alt {
            parts.push("ALT");
        }
        if self.shift {
            parts.push("SHIFT");
        }
        if self.super_key {
            parts.push("LOGO");
        }
        parts.push(self.key.gtk_name());
        parts.join("+")
    }

    /// GTK accelerator, which is the form GNOME stores and sends back.
    #[cfg_attr(not(any(test, target_os = "linux")), allow(dead_code))]
    pub fn portal_trigger(&self) -> String {
        let mut out = String::new();
        if self.ctrl {
            out.push_str("<Control>");
        }
        if self.alt {
            out.push_str("<Alt>");
        }
        if self.shift {
            out.push_str("<Shift>");
        }
        if self.super_key {
            out.push_str("<Super>");
        }
        out.push_str(self.key.gtk_name());
        out
    }

    /// The shortcut Windows and X11 register through `global-hotkey`.
    pub fn to_global(&self) -> Result<HotKey> {
        let mut mods = Modifiers::empty();
        if self.ctrl {
            mods |= Modifiers::CONTROL;
        }
        if self.alt {
            mods |= Modifiers::ALT;
        }
        if self.shift {
            mods |= Modifiers::SHIFT;
        }
        if self.super_key {
            mods |= Modifiers::META;
        }
        let code: Code = self.key.code_name().parse().map_err(|_| {
            Error::new(format!(
                "shortcut key {} has no system code",
                self.key.canonical()
            ))
        })?;
        let mods = if mods.is_empty() { None } else { Some(mods) };
        Ok(HotKey::new(mods, code))
    }
}

impl Key {
    fn canonical(self) -> &'static str {
        match self {
            Key::Char(ch) => leak_char(ch),
            Key::Space => "space",
            Key::Tab => "tab",
            Key::Escape => "escape",
            Key::Enter => "enter",
            Key::Backspace => "backspace",
            Key::Delete => "delete",
            Key::Insert => "insert",
            Key::Home => "home",
            Key::End => "end",
            Key::PageUp => "pageup",
            Key::PageDown => "pagedown",
            Key::Up => "up",
            Key::Down => "down",
            Key::Left => "left",
            Key::Right => "right",
            Key::F(n) => leak_f(n),
            Key::Minus => "minus",
            Key::Equal => "equal",
            Key::Comma => "comma",
            Key::Period => "period",
            Key::Slash => "slash",
            Key::Backslash => "backslash",
            Key::Grave => "grave",
            Key::BracketLeft => "bracketleft",
            Key::BracketRight => "bracketright",
            Key::Semicolon => "semicolon",
            Key::Quote => "quote",
        }
    }

    fn gtk_name(self) -> &'static str {
        match self {
            Key::Char(ch) => leak_char(ch),
            Key::Space => "space",
            Key::Tab => "Tab",
            Key::Escape => "Escape",
            Key::Enter => "Return",
            Key::Backspace => "BackSpace",
            Key::Delete => "Delete",
            Key::Insert => "Insert",
            Key::Home => "Home",
            Key::End => "End",
            Key::PageUp => "Page_Up",
            Key::PageDown => "Page_Down",
            Key::Up => "Up",
            Key::Down => "Down",
            Key::Left => "Left",
            Key::Right => "Right",
            Key::F(n) => leak_gtk_f(n),
            Key::Minus => "minus",
            Key::Equal => "equal",
            Key::Comma => "comma",
            Key::Period => "period",
            Key::Slash => "slash",
            Key::Backslash => "backslash",
            Key::Grave => "grave",
            Key::BracketLeft => "bracketleft",
            Key::BracketRight => "bracketright",
            Key::Semicolon => "semicolon",
            Key::Quote => "apostrophe",
        }
    }

    fn code_name(self) -> String {
        match self {
            Key::Char(ch) if ch.is_ascii_digit() => format!("Digit{ch}"),
            Key::Char(ch) => format!("Key{}", ch.to_ascii_uppercase()),
            Key::Space => "Space".to_string(),
            Key::Tab => "Tab".to_string(),
            Key::Escape => "Escape".to_string(),
            Key::Enter => "Enter".to_string(),
            Key::Backspace => "Backspace".to_string(),
            Key::Delete => "Delete".to_string(),
            Key::Insert => "Insert".to_string(),
            Key::Home => "Home".to_string(),
            Key::End => "End".to_string(),
            Key::PageUp => "PageUp".to_string(),
            Key::PageDown => "PageDown".to_string(),
            Key::Up => "ArrowUp".to_string(),
            Key::Down => "ArrowDown".to_string(),
            Key::Left => "ArrowLeft".to_string(),
            Key::Right => "ArrowRight".to_string(),
            Key::F(n) => format!("F{n}"),
            Key::Minus => "Minus".to_string(),
            Key::Equal => "Equal".to_string(),
            Key::Comma => "Comma".to_string(),
            Key::Period => "Period".to_string(),
            Key::Slash => "Slash".to_string(),
            Key::Backslash => "Backslash".to_string(),
            Key::Grave => "Backquote".to_string(),
            Key::BracketLeft => "BracketLeft".to_string(),
            Key::BracketRight => "BracketRight".to_string(),
            Key::Semicolon => "Semicolon".to_string(),
            Key::Quote => "Quote".to_string(),
        }
    }
}

/// `leak_char` returns a static name for one ascii letter or digit.
fn leak_char(ch: char) -> &'static str {
    match ch {
        'a' => "a",
        'b' => "b",
        'c' => "c",
        'd' => "d",
        'e' => "e",
        'f' => "f",
        'g' => "g",
        'h' => "h",
        'i' => "i",
        'j' => "j",
        'k' => "k",
        'l' => "l",
        'm' => "m",
        'n' => "n",
        'o' => "o",
        'p' => "p",
        'q' => "q",
        'r' => "r",
        's' => "s",
        't' => "t",
        'u' => "u",
        'v' => "v",
        'w' => "w",
        'x' => "x",
        'y' => "y",
        'z' => "z",
        '0' => "0",
        '1' => "1",
        '2' => "2",
        '3' => "3",
        '4' => "4",
        '5' => "5",
        '6' => "6",
        '7' => "7",
        '8' => "8",
        '9' => "9",
        _ => "?",
    }
}

fn leak_f(n: u8) -> &'static str {
    match n {
        1 => "f1",
        2 => "f2",
        3 => "f3",
        4 => "f4",
        5 => "f5",
        6 => "f6",
        7 => "f7",
        8 => "f8",
        9 => "f9",
        10 => "f10",
        11 => "f11",
        12 => "f12",
        _ => "f1",
    }
}

fn leak_gtk_f(n: u8) -> &'static str {
    match n {
        1 => "F1",
        2 => "F2",
        3 => "F3",
        4 => "F4",
        5 => "F5",
        6 => "F6",
        7 => "F7",
        8 => "F8",
        9 => "F9",
        10 => "F10",
        11 => "F11",
        12 => "F12",
        _ => "F1",
    }
}

pub fn parse(text: &str) -> Result<Hotkey> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Error::new("shortcut is empty"));
    }
    let text = text
        .strip_prefix("Press ")
        .or_else(|| text.strip_prefix("press "))
        .unwrap_or(text)
        .trim();
    let hotkey = if text.starts_with('<') {
        parse_accelerator(text)?
    } else {
        parse_chord(text)?
    };
    if !hotkey.ctrl
        && !hotkey.alt
        && !hotkey.shift
        && !hotkey.super_key
        && !matches!(hotkey.key, Key::F(_))
    {
        return Err(Error::new(
            "add a modifier, or use a function key such as F8",
        ));
    }
    Ok(hotkey)
}

fn parse_chord(text: &str) -> Result<Hotkey> {
    let mut hotkey = blank();
    let mut seen_key = false;
    for raw in text.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            return Err(Error::new("shortcut is empty"));
        }
        if apply_modifier(&mut hotkey, token) {
            continue;
        }
        if seen_key {
            return Err(Error::new("shortcut can only have one key"));
        }
        hotkey.key = key_token(token)?;
        seen_key = true;
    }
    if !seen_key {
        return Err(Error::new("shortcut is missing a key"));
    }
    Ok(hotkey)
}

fn parse_accelerator(text: &str) -> Result<Hotkey> {
    let mut rest = text.trim();
    let mut hotkey = blank();
    while let Some(end) = rest.strip_prefix('<').and_then(|body| body.find('>')) {
        let name = &rest[1..end + 1];
        if !apply_modifier(&mut hotkey, name) {
            return Err(Error::new(format!("unknown modifier {name}")));
        }
        rest = rest[end + 2..].trim_start();
    }
    if rest.is_empty() {
        return Err(Error::new("shortcut is missing a key"));
    }
    hotkey.key = key_token(rest)?;
    Ok(hotkey)
}

fn blank() -> Hotkey {
    Hotkey {
        ctrl: false,
        alt: false,
        shift: false,
        super_key: false,
        key: Key::Space,
    }
}

fn apply_modifier(hotkey: &mut Hotkey, token: &str) -> bool {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" | "primary" => hotkey.ctrl = true,
        "alt" | "option" => hotkey.alt = true,
        "shift" => hotkey.shift = true,
        "super" | "meta" | "win" | "cmd" | "command" => hotkey.super_key = true,
        _ => return false,
    }
    true
}

fn key_token(raw: &str) -> Result<Key> {
    let token = raw.trim().to_ascii_lowercase().replace('_', "");
    let key = match token.as_str() {
        "space" => Key::Space,
        "tab" => Key::Tab,
        "escape" | "esc" => Key::Escape,
        "enter" | "return" => Key::Enter,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "insert" => Key::Insert,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "minus" => Key::Minus,
        "equal" => Key::Equal,
        "comma" => Key::Comma,
        "period" => Key::Period,
        "slash" => Key::Slash,
        "backslash" => Key::Backslash,
        "grave" | "backquote" => Key::Grave,
        "bracketleft" => Key::BracketLeft,
        "bracketright" => Key::BracketRight,
        "semicolon" => Key::Semicolon,
        "quote" | "apostrophe" => Key::Quote,
        other if other.len() == 1 => {
            let ch = other.chars().next().unwrap();
            if ch.is_ascii_alphanumeric() {
                Key::Char(ch)
            } else {
                return Err(Error::new(format!("unknown key {raw}")));
            }
        }
        other
            if other.starts_with('f')
                && other[1..]
                    .parse::<u8>()
                    .ok()
                    .is_some_and(|n| (1..=12).contains(&n)) =>
        {
            Key::F(other[1..].parse().unwrap())
        }
        _ => return Err(Error::new(format!("unknown key {raw}"))),
    };
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_alt_space_has_a_portal_trigger_and_a_windows_hotkey() {
        let hotkey = parse("Ctrl+Alt+Space").unwrap();
        assert_eq!(hotkey.canonical(), "ctrl+alt+space");
        assert_eq!(hotkey.spec_trigger(), "CTRL+ALT+space");
        assert_eq!(hotkey.portal_trigger(), "<Control><Alt>space");
        assert_eq!(
            hotkey.to_global().unwrap(),
            HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space)
        );
    }

    #[test]
    fn modifier_order_and_the_portal_form_parse_to_the_same_shortcut() {
        let typed = parse("shift + ctrl + a").unwrap();
        let portal = parse("Press <Control><Shift>a").unwrap();
        assert_eq!(typed, portal);
        assert_eq!(typed.canonical(), "ctrl+shift+a");
        assert_eq!(typed.portal_trigger(), "<Control><Shift>a");
    }

    #[test]
    fn a_function_key_can_stand_alone_and_a_bare_letter_cannot() {
        assert_eq!(parse("F8").unwrap().canonical(), "f8");
        assert!(parse("").is_err());
        assert!(parse("ctrl").is_err());
        assert!(parse("a").is_err());
        assert!(parse("ctrl+nope").is_err());
        assert!(parse("ctrl+alt+space+x").is_err());
    }
}
