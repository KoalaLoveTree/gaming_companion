# Gaming Companion

A hotkey shows a sheet beside or over a game. The app is one external process. It does not inject a DLL, read game memory, or send input.

Plugins are folders on disk. They live in [gaming-companion-plugins](https://github.com/KoalaLoveTree/gaming-companion-plugins). The OS webview loads the page. No plugin ships with the app. Behavior is specified in [SPEC.md](SPEC.md).

The window has no title bar and no system border. A solid gray border resizes it, and a light line marks the inner edge of that border. The darker bar above the page moves the window. Plugins, on the left of that bar, opens the list of games and plugins. Settings, on the right, opens the shortcuts. Back, forward, reload, zoom, and fill screen are shortcuts while the sheet is focused. The side mouse buttons go back and forward. F11 fills the monitor and Esc restores that frame. A second light line shows where the page starts. Ctrl plus the left button moves it from the page. The same border is there in overlay mode, inside a transparent ring the pointer ignores. The tray menu has Show or hide, and Quit. A fresh machine uses `Ctrl+Alt+Space`. Change that in Settings. Exclusive fullscreen covers the sheet, so the game needs borderless windowed mode.

## Build

On Fedora, install the libraries before the first build:

```bash
sudo dnf install gtk3-devel webkit2gtk4.1-devel dbus-devel
```

Other distributions need the development packages for GTK 3, WebKitGTK 4.1, and dbus.

```bash
make linux      # target/release/gaming_companion
make windows    # target/x86_64-pc-windows-msvc/release/gaming_companion.exe
make run        # debug build, then open the window
```

`make windows` cross-compiles from Linux. It needs [cargo-xwin](https://github.com/rust-cross/cargo-xwin) and the `x86_64-pc-windows-msvc` target. The `.exe` uses a static CRT, so the Visual C++ redistributable is not required. The Windows PC needs WebView2, which is already present when Edge is installed.

A running Linux build needs GTK 3 and WebKitGTK 4.1.

## Plugins

Put plugin folders in a `plugins` directory. The app checks these locations, in order:

1. `GAMING_COMPANION_PLUGINS`, when that variable names a directory
2. `./plugins`
3. `plugins` next to the executable
4. `plugins` in this repo, for a build made from this checkout

Plugins in use are in [gaming-companion-plugins](https://github.com/KoalaLoveTree/gaming-companion-plugins). `plugins/` in this checkout is a clone of that repo, and it is gitignored here, so this app repo does not store them.

A plugin is a folder with `plugin.toml`. `testdata/sample` is a generic one:

```toml
id = "sample"
name = "Sample"
entry = "page"

[[resource]]
id = "page"
title = "Page"
kind = "url"
src = "https://example.com/sheet"
inject = ["inject.css"]

[[resource]]
id = "notes"
title = "Notes"
kind = "markdown"
src = "notes.md"
```

`kind` is `url`, `html`, or `markdown`. `entry` names the resource that opens first. `inject` lists CSS files inside the plugin folder. A path that leaves the folder is rejected. Adding or editing a plugin does not require a rebuild.

Plugins for one game can share a folder. Each plugin is still its own directory with `plugin.toml`, so deleting that directory removes only that plugin. `game.toml` in the game folder sets the name the list shows:

```text
plugins/arc-raiders/game.toml
plugins/arc-raiders/loot/plugin.toml
plugins/arc-raiders/maps/plugin.toml
```

A folder that itself contains `plugin.toml` is one plugin. Plugins, on the left of the top bar, lists each game and the plugins under it.

The app exits when it has no plugin to open.

## Config

Settings are stored per machine, outside the plugin folder:

- Linux: `~/.config/gaming-companion/`
- Windows: `%APPDATA%\gaming-companion\`

Page data stays in a `webview` directory beside that config and survives hide and show.
