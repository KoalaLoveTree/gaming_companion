# Gaming Companion

A hotkey shows a sheet beside or over a game. The app is one external process. It does not inject a DLL, read game memory, or send input.

Plugins are folders on disk. They live in [gaming-companion-plugins](https://github.com/KoalaLoveTree/gaming-companion-plugins). The OS webview loads the page. No plugin ships with the app. Behavior is specified in [SPEC.md](SPEC.md).

The window has no title bar. Move and Size sit at the bottom right. The tray menu has Show or hide, and Quit. A fresh machine uses `Ctrl+Alt+Space`. Change that in Settings. Exclusive fullscreen covers the sheet, so the game needs borderless windowed mode.

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

The app exits when it has no plugin to open.

## Config

Settings are stored per machine, outside the plugin folder:

- Linux: `~/.config/gaming-companion/`
- Windows: `%APPDATA%\gaming-companion\`

Page data stays in a `webview` directory beside that config and survives hide and show.
