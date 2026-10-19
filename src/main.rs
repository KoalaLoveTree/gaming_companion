// A console subsystem opens a shell window beside the sheet. Release builds
// are a normal Windows app. Debug builds keep the console so errors stay visible.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    if let Err(err) = gaming_companion::launch() {
        eprintln!("gaming_companion: {err}");
        #[cfg(windows)]
        report(&err.to_string());
        std::process::exit(1);
    }
}

/// The release app has no console, so a failed start is shown in a dialog.
#[cfg(windows)]
fn report(message: &str) {
    use std::os::windows::ffi::OsStrExt;
    let wide = |text: &str| {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let text = wide(message);
    let title = wide("Gaming Companion");
    unsafe {
        let _ = MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
    }
}

#[cfg(windows)]
unsafe extern "system" {
    fn MessageBoxW(
        hwnd: *mut core::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        flags: u32,
    ) -> i32;
}
