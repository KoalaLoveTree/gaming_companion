//! Windows border. The subclass answers hit tests so the outer band resizes
//! and the bar moves the window. The page itself stays a client area.

use std::sync::atomic::{AtomicBool, Ordering};

use tao::platform::windows::WindowExtWindows;

/// The subclass cannot hold the shell, so the overlay flag lives here.
static OVERLAY: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_overlay(on: bool) {
    OVERLAY.store(on, Ordering::Relaxed);
}

pub(crate) fn install(window: &tao::window::Window) {
    unsafe {
        SetWindowSubclass(
            window.hwnd() as *mut core::ffi::c_void,
            chrome_subclass,
            1,
            0,
        );
    }
}

unsafe extern "system" fn chrome_subclass(
    hwnd: *mut core::ffi::c_void,
    msg: u32,
    wparam: usize,
    lparam: isize,
    id: usize,
    data: usize,
) -> isize {
    let _ = data;
    const WM_NCHITTEST: u32 = 0x0084;
    const WM_NCLBUTTONDBLCLK: u32 = 0x00A3;
    const WM_NCDESTROY: u32 = 0x0082;
    const WM_SYSCOMMAND: u32 = 0x0112;
    const SC_MAXIMIZE: usize = 0xF030;
    unsafe {
        if msg == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, chrome_subclass, id);
            return DefSubclassProc(hwnd, msg, wparam, lparam);
        }
        if msg == WM_NCLBUTTONDBLCLK || (msg == WM_SYSCOMMAND && (wparam & 0xFFF0) == SC_MAXIMIZE) {
            return 0;
        }
        if msg == WM_NCHITTEST {
            let mut point = Point {
                x: lparam as i16 as i32,
                y: (lparam >> 16) as i16 as i32,
            };
            let mut rect = Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if ScreenToClient(hwnd, &mut point) != 0 && GetClientRect(hwnd, &mut rect) != 0 {
                let mut dpi = GetDpiForWindow(hwnd) as i32;
                if dpi <= 0 {
                    dpi = 96;
                }
                let ctrl = GetKeyState(0x11) < 0;
                let overlay = OVERLAY.load(Ordering::Relaxed);
                let decision = crate::chrome::hit(
                    rect.right,
                    rect.bottom,
                    point.x,
                    point.y,
                    ctrl,
                    overlay,
                    dpi,
                    96,
                );
                if matches!(decision, crate::chrome::Hit::Move)
                    && crate::chrome::settings_target(
                        rect.right,
                        rect.bottom,
                        point.x,
                        point.y,
                        overlay,
                        dpi,
                        96,
                    )
                {
                    return hit_code(crate::chrome::Hit::Client);
                }
                return hit_code(decision);
            }
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

fn hit_code(hit: crate::chrome::Hit) -> isize {
    match hit {
        crate::chrome::Hit::Pass => -1,
        crate::chrome::Hit::Client => 1,
        crate::chrome::Hit::Move => 2,
        crate::chrome::Hit::Resize(crate::chrome::Edge::West) => 10,
        crate::chrome::Hit::Resize(crate::chrome::Edge::East) => 11,
        crate::chrome::Hit::Resize(crate::chrome::Edge::North) => 12,
        crate::chrome::Hit::Resize(crate::chrome::Edge::NorthWest) => 13,
        crate::chrome::Hit::Resize(crate::chrome::Edge::NorthEast) => 14,
        crate::chrome::Hit::Resize(crate::chrome::Edge::South) => 15,
        crate::chrome::Hit::Resize(crate::chrome::Edge::SouthWest) => 16,
        crate::chrome::Hit::Resize(crate::chrome::Edge::SouthEast) => 17,
    }
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[link(name = "user32")]
#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(
        hwnd: *mut core::ffi::c_void,
        proc: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            u32,
            usize,
            isize,
            usize,
            usize,
        ) -> isize,
        id: usize,
        data: usize,
    ) -> i32;
    fn RemoveWindowSubclass(
        hwnd: *mut core::ffi::c_void,
        proc: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            u32,
            usize,
            isize,
            usize,
            usize,
        ) -> isize,
        id: usize,
    ) -> i32;
    fn DefSubclassProc(
        hwnd: *mut core::ffi::c_void,
        msg: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize;
    fn ScreenToClient(hwnd: *mut core::ffi::c_void, point: *mut Point) -> i32;
    fn GetClientRect(hwnd: *mut core::ffi::c_void, rect: *mut Rect) -> i32;
    fn GetKeyState(key: i32) -> i16;
    fn GetDpiForWindow(hwnd: *mut core::ffi::c_void) -> u32;
}
