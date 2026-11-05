# Build the gaming companion.
#
#   make linux     release binary for this machine
#   make windows   release .exe, cross-compiled from Linux
#   make run       debug build, then open the window
#   make cover     HTML coverage report for the library tests
#
# Linux needs:  sudo dnf install gtk3-devel webkit2gtk4.1-devel dbus-devel
# Windows needs cargo-xwin, the x86_64-pc-windows-msvc target, and WebView2
# Evergreen on the PC that runs the .exe.
# Coverage needs cargo-llvm-cov and the llvm-tools-preview rustup component.
# How to publish zips on GitHub is in docs/github-releases.md.

.PHONY: help linux windows run cover

.DEFAULT_GOAL := help

help:
	@echo "make linux     target/release/gaming_companion"
	@echo "make windows   target/x86_64-pc-windows-msvc/release/gaming_companion.exe"
	@echo "make run       debug build and open the window"
	@echo "make cover     target/llvm-cov/html/index.html"

linux:
	cargo build --release --features window

# GUI subsystem comes from the release cfg in src/main.rs. The static CRT
# keeps the .exe from needing the Visual C++ redistributable.
windows:
	RUSTFLAGS="-C target-feature=+crt-static" cargo xwin build --release --target x86_64-pc-windows-msvc --features window

# This NVIDIA Wayland session blanks the page unless explicit sync is off.
run:
	__NV_DISABLE_EXPLICIT_SYNC=1 cargo run --features window

# Library tests only. The report is this crate, written under target/.
cover:
	cargo llvm-cov --lib --features window --html --output-dir target/llvm-cov
	@echo "report: target/llvm-cov/html/index.html"
