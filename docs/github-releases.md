# GitHub release builds

How to publish a Linux zip and a Windows zip on GitHub Releases. None of this is wired up. Add it when you want a download link. `make cover` stays on your machine. It is not part of a release.

A release is a tag such as `v0.1.0`. The page for it is:

https://github.com/KoalaLoveTree/gaming_companion/releases/tag/v0.1.0

Two files hang on that page:

- `gaming-companion-linux.zip`
- `gaming-companion-windows.zip`

Those files stay there. A workflow artifact is a different upload: it belongs to one Actions run and expires. Use an artifact to look at a trial build. Use a Release for the thing you hand to someone.

This is not a Docker build. The app is a native window. The workflow has jobs in order: check, then build, then attach the zips.

## Before the first release

Add `.github/workflows/ci.yml` and get one green run before you add the release workflow. It runs on pull requests and on pushes to `main`.

The runner is Ubuntu. Install `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libdbus-1-dev`, and `pkg-config`. Install stable Rust with the `rustfmt` and `clippy` components. Then run:

```bash
cargo fmt --all -- --check
cargo clippy --features window --all-targets -- -D warnings
cargo test --features window
```

The first run may fail on formatting or clippy. That log is the fix-up. Leave GitHub's "require status checks" setting off until a run is green, or you cannot merge the fix.

Do that work on a branch and merge it with a pull request. `main` is the line the check is for.

## When the release workflow runs

Create `.github/workflows/release.yml`. It runs when you push a tag that starts with `v`. Starting it by hand builds the zips as artifacts and does not publish a Release.

The publish job waits until the check, the Linux zip, and the Windows zip have succeeded.

In the repo: Settings, Actions, General, Workflow permissions, set read and write. The workflow also sets `contents: write`, so the default token can create the Release.

The Windows cross-compile is slow. Keep it off pull requests.

## Linux zip

`make linux` on this machine links GTK 3 and WebKitGTK 4.1 from Fedora 44. A binary built on GitHub's Ubuntu image is linked to Ubuntu's libraries and often will not start on Fedora.

Run the Linux job in a Fedora 44 container. Change the image tag when the machines you ship to move off Fedora 44. Build on the oldest Fedora you still support. A newer Fedora's binary can refuse to start on an older one.

```yaml
linux:
  runs-on: ubuntu-latest
  container: fedora:44
```

Install the packages first, then check out the repo. The checkout action needs git and certificates, and the container image does not have them yet.

```bash
dnf install -y git tar gzip ca-certificates curl gcc make pkgconf-pkg-config \
  gtk3-devel webkit2gtk4.1-devel dbus-devel zip
```

Then install stable Rust with the `dtolnay/rust-toolchain` action and run `make linux`.

The zip matches the folder you already unpack from `dist/`:

```text
gaming-companion-linux/
  gaming_companion
  HOW-TO-TEST.txt
  plugins/
```

`plugins/` is gitignored here. The app exits when it has no plugin to open. Clone https://github.com/KoalaLoveTree/gaming-companion-plugins into `plugins/` before packing. If that repo is private, the job needs a token that can read it.

`HOW-TO-TEST.txt` exists only under `dist/` today, and `dist/` is gitignored. Commit the two texts you already use, as `packaging/linux/HOW-TO-TEST.txt` and `packaging/windows/HOW-TO-TEST.txt`, and copy them into the zip.

```bash
mkdir -p staging/gaming-companion-linux
cp target/release/gaming_companion staging/gaming-companion-linux/
cp packaging/linux/HOW-TO-TEST.txt staging/gaming-companion-linux/
cp -a plugins staging/gaming-companion-linux/
(cd staging && zip -r ../gaming-companion-linux.zip gaming-companion-linux)
```

The PC that runs the zip still needs the GTK 3 and WebKitGTK 4.1 runtimes (`gtk3` and `webkit2gtk4.1` on Fedora). On GNOME Wayland the first launch writes `~/.local/share/applications/com.gamingcompanion.Companion.desktop`, so search finds Gaming Companion after that. The entry points at the path you launched. An X11 session does not write that file.

## Windows zip

Build on `ubuntu-latest`, the same way `make windows` cross-compiles from Linux. You do not need a Windows runner. `cargo-xwin` downloads the Windows SDK, so the job is large and needs the network.

```bash
rustup target add x86_64-pc-windows-msvc
cargo install cargo-xwin --locked
RUSTFLAGS="-C target-feature=+crt-static" \
  cargo xwin build --release --target x86_64-pc-windows-msvc --features window
```

The static CRT is what `make windows` already sets. The `.exe` does not need the Visual C++ redistributable. The PC that runs it needs WebView2, which is present when Edge is installed.

```text
gaming-companion-windows/
  gaming_companion.exe
  HOW-TO-TEST.txt
  plugins/
```

Start search will not list this exe as an app. That is a later change. An unsigned exe can trip SmartScreen. Signing needs a certificate and can wait.

Upload each zip with `actions/upload-artifact`. A later job downloads both with `actions/download-artifact` and `merge-multiple: true`.

## Publish

```yaml
name: release

on:
  push:
    tags: ["v*"]
  workflow_dispatch:

permissions:
  contents: write

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - name: libraries
        run: |
          sudo apt-get update
          sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev libdbus-1-dev pkg-config
      - run: cargo fmt --all -- --check
      - run: cargo clippy --features window --all-targets -- -D warnings
      - run: cargo test --features window

  linux:
    needs: check
    runs-on: ubuntu-latest
    container: fedora:44
    steps:
      - name: compilers and libraries
        run: |
          dnf install -y git tar gzip ca-certificates curl gcc make \
            pkgconf-pkg-config gtk3-devel webkit2gtk4.1-devel dbus-devel zip
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: plugins
        run: git clone --depth 1 https://github.com/KoalaLoveTree/gaming-companion-plugins.git plugins
      - run: make linux
      - name: pack
        run: |
          mkdir -p staging/gaming-companion-linux
          cp target/release/gaming_companion staging/gaming-companion-linux/
          cp packaging/linux/HOW-TO-TEST.txt staging/gaming-companion-linux/
          cp -a plugins staging/gaming-companion-linux/
          (cd staging && zip -r ../gaming-companion-linux.zip gaming-companion-linux)
      - uses: actions/upload-artifact@v4
        with:
          name: linux-zip
          path: gaming-companion-linux.zip

  windows:
    needs: check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-msvc
      - name: cargo-xwin
        run: cargo install cargo-xwin --locked
      - name: plugins
        run: git clone --depth 1 https://github.com/KoalaLoveTree/gaming-companion-plugins.git plugins
      - name: build
        run: |
          RUSTFLAGS="-C target-feature=+crt-static" cargo xwin build --release \
            --target x86_64-pc-windows-msvc --features window
      - name: pack
        run: |
          sudo apt-get update && sudo apt-get install -y zip
          mkdir -p staging/gaming-companion-windows
          cp target/x86_64-pc-windows-msvc/release/gaming_companion.exe \
            staging/gaming-companion-windows/
          cp packaging/windows/HOW-TO-TEST.txt staging/gaming-companion-windows/
          cp -a plugins staging/gaming-companion-windows/
          (cd staging && zip -r ../gaming-companion-windows.zip gaming-companion-windows)
      - uses: actions/upload-artifact@v4
        with:
          name: windows-zip
          path: gaming-companion-windows.zip

  publish:
    needs: [linux, windows]
    if: startsWith(github.ref, 'refs/tags/v')
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          merge-multiple: true
      - name: publish
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release create "${{ github.ref_name }}" \
            gaming-companion-linux.zip \
            gaming-companion-windows.zip \
            --title "${{ github.ref_name }}" \
            --verify-tag
```

`Cargo.toml` is `0.1.0`. When you tag, use that number, and bump the version in the commit you tag when it changes.

```bash
git tag v0.1.0
git push origin v0.1.0
```

Pushing the tag starts the workflow. When it is green, both zips are on the Releases page.

A hand start still runs `check`, `linux`, and `windows`. Download those zips from the Actions run. They expire. Push a tag when you want them kept.

## Leave out

- A coverage percentage that fails the release. Open `make cover` locally when you want untested lines.
- The Windows exe on every pull request.
- An installer, Flatpak, an rpm, or code signing, until a downloaded zip is not enough.
