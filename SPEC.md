# Gaming Companion

A hotkey shows a sheet beside or over a game. The app is one external process. Plugins are folders on disk. The OS webview loads the page, so a plugin can open an external site.

## Safety

The process does not inject a DLL, read game memory, or send input. Plugins do not get those powers either.

## Plugins

A plugin is a folder with `plugin.toml`. Plugins for one game live in one game folder, each plugin its own folder, so deleting that folder removes only that plugin. A folder that itself contains `plugin.toml` is a single plugin. The shell reads plugins at startup. Adding or editing one does not require a rebuild. No plugin ships with the app. Plugin folders are provided separately.

A resource is a remote URL, a local HTML file, or Markdown. A URL may inject CSS and JS into that document. A path that leaves the plugin folder is rejected. The shell opens one named resource of one named plugin.

## Window

The window has no title bar and no system border. An opaque border drawn by the app surrounds the page. A light line marks the inner edge of that border. The outer 6px of each edge, including the corners, resizes the window. The top of that border is 38px: its outer edge resizes, and the rest is a darker bar that moves the window. A light line under that bar is where the page starts. A small emblem marks the move area. Ctrl plus the left button moves the sheet from the page. Plugins is a button on the left of that bar. It opens a list of games and the plugins in each game, and the open plugin is marked. Settings is a button on the right. It opens the shortcut list. A new shortcut is recorded with Change, then Save.

The tray menu has Show or hide, and Quit. Quit saves and exits. The Settings Close button only hides that list. Alt+F4 also quits.

One shortcut shows and hides the sheet. Hide does not reload the page and leaves the window where the user put it. Clicking the game does not hide the sheet and does not quit. Showing the sheet does not take focus.

Two modes, chosen in Settings and saved per machine. The default is a normal window.

- Normal: the page stays opaque.
- Overlay: a dim panel on the right, about 40% of that monitor. A transparent 16px ring outside the border ignores the pointer. The border itself moves and resizes the same way it does on a normal window. Search, filters, the list, and Settings stay clickable. The pointer does not pass through the sheet.

Each mode keeps its own frame. The first overlay open uses that right-hand panel. Later opens restore the saved overlay frame. Quit saves the open mode's frame. The next start opens that frame at the saved place and size. The page follows the monitor's scale when the window moves.

Exclusive fullscreen covers the sheet. The game needs borderless windowed mode.

While the sheet is visible it stays above other windows. The shortcut is how it is hidden. Windows and X11 honor that directly. On GNOME Wayland the app installs a shell helper that pins only this app and moves it back to the saved place. A helper added or changed during a session starts after the next login. Exclusive fullscreen still covers a pinned window.

A fresh window is 1000 by 800 logical pixels, and no smaller than 320 by 240.

## Shortcut

A fresh machine uses `ctrl+alt+space`. Each person changes it in Settings. Fedora and Windows keep separate config files. On GNOME the system asks to confirm a new shortcut only the first time.

Show or hide is the only global shortcut. Back, forward, reload, zoom, and fill screen work while the sheet is focused. The side mouse buttons go back and forward while the pointer is over the sheet. F5 reloads the page. F11 fills this window to the monitor, and Esc or F11 restores the frame from before that. Fill screen is not an exclusive fullscreen.

## Config

Config lives outside the plugin folder: `~/.config/gaming-companion/` on Linux and `%APPDATA%\gaming-companion\` on Windows. It stores the open plugin, the open resource, the shortcuts, the mode, and both frames. Page data stays in a `webview` directory beside that config and survives hide and show.
