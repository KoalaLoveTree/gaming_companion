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

    let Ok(icon) = Icon::from_rgba(icon_pixels(), 32, 32) else {
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

fn icon_pixels() -> Vec<u8> {
    const SIZE: usize = 32;
    let mut pixels = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let index = (y * SIZE + x) * 4;
            if x < 2 || y < 2 || x >= SIZE - 2 || y >= SIZE - 2 {
                continue;
            }
            pixels[index] = 28;
            pixels[index + 1] = 28;
            pixels[index + 2] = 32;
            pixels[index + 3] = 255;
            let row = matches!(y, 9 | 10 | 15 | 16 | 21 | 22) && (8..24).contains(&x);
            let mark = matches!(x, 8 | 9) && (8..23).contains(&y);
            if row || mark {
                pixels[index] = 236;
                pixels[index + 1] = 236;
                pixels[index + 2] = 236;
                pixels[index + 3] = 255;
            }
        }
    }
    pixels
}
