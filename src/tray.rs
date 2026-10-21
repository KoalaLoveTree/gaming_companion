//! Tray icon. The sheet has no title bar, so Show, Hide, and Quit live here.

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub(crate) enum Action {
    Toggle,
    Quit,
}

pub(crate) fn install<F>(on_action: F) -> Option<TrayIcon>
where
    F: Fn(Action) + Send + Sync + 'static,
{
    let menu = Menu::new();
    let show = MenuItem::with_id("toggle", "Show or hide", true, None);
    let quit = MenuItem::with_id("quit", "Quit", true, None);
    if menu.append(&show).is_err() || menu.append(&quit).is_err() {
        eprintln!("gaming_companion: the tray menu could not be built");
        return None;
    }

    let Ok(icon) = Icon::from_rgba(
        crate::mark::pixels(crate::mark::TRAY),
        crate::mark::TRAY,
        crate::mark::TRAY,
    ) else {
        eprintln!("gaming_companion: the tray icon could not be built");
        return None;
    };

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let action = if event.id.as_ref() == "quit" {
            Action::Quit
        } else {
            Action::Toggle
        };
        on_action(action);
    }));

    match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Gaming Companion")
        .with_icon(icon)
        .build()
    {
        Ok(icon) => {
            eprintln!("gaming_companion: tray icon ready");
            Some(icon)
        }
        Err(err) => {
            eprintln!("gaming_companion: tray icon unavailable: {err}");
            None
        }
    }
}
