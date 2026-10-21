//! Messages the page and the Linux chrome send into the shell.
//!
//! The wire form stays a string. Parsing it once keeps `Shell::on_ipc` a match
//! on this enum.

use crate::model::bindings::Action;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Message {
    Ready,
    Overlay,
    Navigate(Action),
    Escape,
    Bind { action: Action, chord: String },
    OpenPlugin(String),
    OpenGame(String),
    SetHotkey(String),
}

/// `None` is a message the shell ignores.
pub(crate) fn parse(raw: &str) -> Option<Message> {
    let body = raw.trim().trim_matches('"');
    if let Some(action) = Action::from_id(body) {
        return Some(Message::Navigate(action));
    }
    match body {
        "ready" => Some(Message::Ready),
        "overlay" => Some(Message::Overlay),
        "escape" => Some(Message::Escape),
        _ => parse_prefixed(body),
    }
}

fn parse_prefixed(body: &str) -> Option<Message> {
    if let Some(rest) = body.strip_prefix("bind:") {
        let (id, chord) = rest.split_once(':')?;
        let action = Action::from_id(id.trim())?;
        return Some(Message::Bind {
            action,
            chord: chord.trim().to_string(),
        });
    }
    if let Some(id) = body.strip_prefix("open:") {
        return Some(Message::OpenPlugin(id.trim().to_string()));
    }
    if let Some(id) = body.strip_prefix("game:") {
        return Some(Message::OpenGame(id.trim().to_string()));
    }
    let text = body.strip_prefix("set:")?;
    Some(Message::SetHotkey(text.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_commands_parse() {
        assert_eq!(parse("ready"), Some(Message::Ready));
        assert_eq!(parse("  \"overlay\"  "), Some(Message::Overlay));
        assert_eq!(parse("escape"), Some(Message::Escape));
        assert_eq!(parse("back"), Some(Message::Navigate(Action::Back)));
        assert_eq!(parse("zoom-in"), Some(Message::Navigate(Action::ZoomIn)));
        assert_eq!(parse("fill"), Some(Message::Navigate(Action::Fill)));
        assert_eq!(
            parse("bind:reload:ctrl+r"),
            Some(Message::Bind {
                action: Action::Reload,
                chord: "ctrl+r".to_string(),
            })
        );
        assert_eq!(
            parse("open: loot "),
            Some(Message::OpenPlugin("loot".to_string()))
        );
        assert_eq!(
            parse("game:arc-raiders"),
            Some(Message::OpenGame("arc-raiders".to_string()))
        );
        assert_eq!(
            parse("set: ctrl+alt+space"),
            Some(Message::SetHotkey("ctrl+alt+space".to_string()))
        );
    }

    #[test]
    fn an_unknown_or_incomplete_message_is_ignored() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("nope"), None);
        assert_eq!(parse("bind:back"), None);
        assert_eq!(parse("bind:nope:ctrl+a"), None);
        assert_eq!(parse("set:"), Some(Message::SetHotkey(String::new())));
    }
}
