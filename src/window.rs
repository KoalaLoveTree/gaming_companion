use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

use crate::controller::paths::{self, webview_data_dir};
use crate::controller::plugins::{choose, load_plugins};
use crate::controller::prepare::{catalog_script, matching_resource, prepare_page};
use crate::model::Plugin;
use crate::model::bindings::{self, Action};
use crate::model::config::{self, AppConfig, WindowFrame};
use crate::model::hotkey::{self, Hotkey};
use crate::settings;
use crate::{Error, Result};

enum ShellEvent {
    /// `activation_token` comes from the Wayland shortcut portal. It is what
    /// lets a global hotkey focus this window again. Other platforms send none.
    Toggle {
        activation_token: Option<String>,
    },
    Ipc(String),
    /// The webview finished loading this address. Used to follow Back and Forward.
    Navigated(String),
    Bound(String),
    Failed(String),
    /// The window exists, so the portal dialog can be parented to it.
    StartBind,
    Quit,
}

pub fn launch() -> Result<()> {
    // Before GTK and WebKit start, so the renderer inherits it.
    #[cfg(target_os = "linux")]
    linux_scale::prepare_renderer();

    let plugins_path = paths::plugins_dir()?;
    let loaded = load_plugins(&plugins_path)?;
    for err in &loaded.errors {
        eprintln!("gaming_companion: skipped plugin: {err}");
    }
    let config_file = paths::config_path()?;
    let mut config = paths::load_config(&config_file)?;
    let plugins = loaded.plugins;
    let (plugin, resource) = choose(&plugins, &config)?;
    let page = prepare_page(plugin, resource)?;
    config.active_plugin = plugin.id.clone();
    config.active_resource = Some(resource.id.clone());
    let hotkey = hotkey::parse(&config.hotkey)?;
    config.hotkey = hotkey.canonical();
    eprintln!("gaming_companion: {} ({})", page.title, plugin.id);

    // The portal registration has to finish before the display opens.
    // The parent string is filled once the Wayland window can export a handle.
    let parent_handle = Arc::new(Mutex::new(String::new()));
    let prepared = prepare_shortcuts(Arc::clone(&parent_handle));

    // Before the window maps, so Mutter is already watching for it.
    #[cfg(target_os = "linux")]
    crate::gnome_pin::install();

    let event_loop = EventLoopBuilder::<ShellEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let mut shell = Shell::arm(
        prepared,
        proxy.clone(),
        hotkey,
        config,
        config_file,
        plugins,
    );
    let catalog = catalog_script(&shell.plugins)?;

    let startup = active_frame(&shell.config);
    let mut builder = WindowBuilder::new()
        .with_title(page.title.clone())
        .with_inner_size(LogicalSize::new(startup.width, startup.height))
        .with_min_inner_size(LogicalSize::new(320.0, 240.0))
        .with_resizable(true)
        .with_maximizable(false)
        .with_transparent(true)
        .with_decorations(false)
        .with_always_on_top(true)
        .with_focused(!shell.config.overlay);
    if let Some((x, y)) = origin_to_restore(&startup) {
        builder = builder.with_position(LogicalPosition::new(x, y));
    }
    let window = builder
        .build(&event_loop)
        .map_err(|err| Error::new(format!("opening window: {err}")))?;

    let mut web_context = WebContext::new(Some(webview_data_dir()?));
    let ipc_proxy = proxy.clone();
    let webview = build_webview(
        &window,
        &mut web_context,
        &page,
        catalog.as_deref(),
        &shell.plugins,
        &shell.config.active_plugin,
        ipc_proxy,
        &shell.shortcut,
        shell.host.confirms(),
        shell.config.overlay,
        &shell.config.bindings,
    )?;
    OVERLAY.store(shell.config.overlay, Ordering::Relaxed);
    #[cfg(target_os = "linux")]
    crate::frame::set_overlay(shell.config.overlay);
    install_chrome(&window, &webview);
    #[cfg(target_os = "linux")]
    linux_scale::watch(&webview);
    #[cfg(not(target_os = "linux"))]
    eprintln!(
        "gaming_companion: {}",
        monitor_label(&window, window.scale_factor())
    );
    #[cfg(not(target_os = "linux"))]
    drop(parent_handle);
    // Export after the window exists. The fallback fires if the handle never arrives.
    #[cfg(target_os = "linux")]
    let mut await_parent = if shell.host.confirms() {
        arm_portal_parent(&window, &parent_handle, &proxy)
    } else {
        false
    };

    let tray_proxy = proxy.clone();
    let tray = crate::tray::install(move |action| {
        let event = match action {
            crate::tray::Action::Toggle => ShellEvent::Toggle {
                activation_token: None,
            },
            crate::tray::Action::Quit => ShellEvent::Quit,
        };
        let _ = tray_proxy.send_event(event);
    });
    apply_presentation(&window, &shell.config);
    if shell.config.overlay {
        set_shown(&window, true, true, None);
    }
    #[cfg(target_os = "linux")]
    {
        publish_app_id(&window);
        crate::gnome_pin::schedule();
    }

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        let _keep_context = &web_context;
        let _keep_tray = &tray;
        #[cfg(target_os = "linux")]
        if await_parent && export_parent_handle(&window, &parent_handle, &proxy) {
            await_parent = false;
        }
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                shell.save(&window);
                *control_flow = ControlFlow::ExitWithCode(0);
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => shell.pass_margin(&window),
            // Windows and macOS. The webview already paints at this scale.
            // Reapply the user's page zoom. It is not the monitor scale.
            Event::WindowEvent {
                event: WindowEvent::ScaleFactorChanged { scale_factor, .. },
                ..
            } => {
                let _ = webview.zoom(shell.zoom);
                eprintln!("gaming_companion: {}", monitor_label(&window, scale_factor));
            }
            Event::UserEvent(ShellEvent::Quit) => {
                shell.save(&window);
                *control_flow = ControlFlow::ExitWithCode(0);
            }
            Event::UserEvent(shell_event) => shell.on_event(shell_event, &window, &webview),
            _ => {}
        }
    });
}

fn monitor_label(window: &tao::window::Window, scale: f64) -> String {
    let scale = if scale.is_finite() && scale >= 1.0 {
        scale
    } else {
        1.0
    };
    match window.current_monitor().and_then(|monitor| monitor.name()) {
        Some(name) => format!("{name}, scale {scale}"),
        None => format!("display scale {scale}"),
    }
}

#[cfg(target_os = "linux")]
mod linux_scale {
    use std::cell::Cell;
    use std::rc::Rc;

    use gtk::gdk::prelude::MonitorExt;
    use gtk::glib;
    use gtk::prelude::WidgetExt;
    use webkit2gtk::WebViewExt;
    use wry::WebViewExtUnix;

    pub fn prepare_renderer() {
        if !nvidia_loaded() {
            return;
        }
        if wayland_session() {
            // The GPU renderer is the one that paints at the monitor scale.
            // On NVIDIA + Wayland it commits the surface without an acquire
            // point, and this compositor answers with protocol error 71.
            // Turning explicit sync off keeps that renderer. The software
            // fallback stays up, but it draws a 1x bitmap the 4K panel stretches.
            if std::env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none()
                && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none()
            {
                unsafe { std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1") };
            }
        } else if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            // X11 rejects the GPU buffer and the page stays blank.
            unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
        }
    }

    pub fn watch(webview: &wry::WebView) {
        let page = webview.webview();
        // Zoom is layout, not resolution. The widget scale is the pixel ratio.
        page.set_zoom_level(super::user_zoom());
        // 0 so the first real scale, including the startup one, is reported once.
        // The widget reads as 1 until the window is mapped onto a monitor.
        let seen = Rc::new(Cell::new(0));
        let seen_for_signal = Rc::clone(&seen);
        page.connect_scale_factor_notify(move |widget| {
            note_scale(widget, &seen_for_signal);
        });
        // The scale signal fires when the window is mapped, and again when it
        // crosses onto the other monitor. The timeout covers a monitor whose
        // scale is already 1, where GTK may never emit the signal.
        let page_for_later = page.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(400), move || {
            note_scale(&page_for_later, &seen);
            glib::ControlFlow::Break
        });
    }

    fn note_scale(widget: &webkit2gtk::WebView, seen: &Cell<i32>) {
        let next = widget.scale_factor().max(1);
        if next == seen.get() {
            return;
        }
        seen.set(next);
        widget.set_zoom_level(super::user_zoom());
        widget.queue_resize();
        eprintln!("gaming_companion: {}", monitor_line(widget));
    }

    fn monitor_line(widget: &webkit2gtk::WebView) -> String {
        let scale = widget.scale_factor().max(1);
        let Some(surface) = widget.window() else {
            return format!("display scale {scale}");
        };
        let Some(monitor) = widget.display().monitor_at_window(&surface) else {
            return format!("display scale {scale}");
        };
        let name = monitor
            .model()
            .map(|model| model.to_string())
            .filter(|model| !model.is_empty())
            .unwrap_or_else(|| "display".to_string());
        let geo = monitor.geometry();
        format!("{name} {}x{} at scale {scale}", geo.width(), geo.height())
    }

    fn nvidia_loaded() -> bool {
        std::path::Path::new("/proc/driver/nvidia/version").is_file()
            || std::path::Path::new("/sys/module/nvidia").exists()
    }

    pub(super) fn wayland_session() -> bool {
        if let Ok(backend) = std::env::var("GDK_BACKEND") {
            return backend.split(',').any(|item| item.trim() == "wayland");
        }
        std::env::var_os("WAYLAND_DISPLAY").is_some()
            && std::env::var("XDG_SESSION_TYPE").as_deref() != Ok("x11")
    }
}

fn active_frame(config: &AppConfig) -> WindowFrame {
    if config.overlay {
        config
            .overlay_window
            .clone()
            .unwrap_or_else(|| config.window.clone())
    } else {
        config.window.clone()
    }
}

fn remember_frame(window: &tao::window::Window, config: &mut AppConfig) {
    let previous = active_frame(config);
    let scale = window.scale_factor();
    let size = window.inner_size().to_logical::<f64>(scale);
    if !(200.0..=10_000.0).contains(&size.width) || !(200.0..=10_000.0).contains(&size.height) {
        return;
    }
    let mut frame = WindowFrame {
        width: size.width,
        height: size.height,
        x: previous.x,
        y: previous.y,
        placed: previous.placed,
    };
    if let Some((x, y)) = observed_origin(window) {
        frame.x = Some(x);
        frame.y = Some(y);
        frame.placed = true;
    }
    if config.overlay {
        config.overlay_window = Some(frame);
    } else {
        config.window = frame;
    }
}

/// Position the client can prove. On GNOME Wayland the client always reports
/// the origin, so only the shell helper's frame is stored.
fn observed_origin(window: &tao::window::Window) -> Option<(f64, f64)> {
    #[cfg(target_os = "linux")]
    if crate::gnome_pin::gnome_wayland() {
        return crate::gnome_pin::frame_origin();
    }
    let scale = window.scale_factor();
    let Ok(position) = window.outer_position() else {
        return None;
    };
    let position = position.to_logical::<f64>(scale);
    if position.x.is_finite() && position.y.is_finite() {
        Some((position.x, position.y))
    } else {
        None
    }
}

fn origin_to_restore(frame: &WindowFrame) -> Option<(f64, f64)> {
    #[cfg(target_os = "linux")]
    let require_observed = crate::gnome_pin::gnome_wayland();
    #[cfg(not(target_os = "linux"))]
    let require_observed = false;
    config::restore_origin(frame, require_observed)
}

fn apply_presentation(window: &tao::window::Window, config: &AppConfig) {
    // No system title bar and no system border. The black border around the
    // page moves and resizes the window. Quit lives on the tray icon.
    window.set_decorations(false);
    // The hotkey hides the sheet. While it is visible it stays above the game.
    window.set_always_on_top(true);
    let frame = active_frame(config);
    window.set_inner_size(LogicalSize::new(frame.width, frame.height));
    if let Some((x, y)) = origin_to_restore(&frame) {
        window.set_outer_position(LogicalPosition::new(x, y));
        // Wayland ignores the call above. The shell helper moves the window.
        #[cfg(target_os = "linux")]
        crate::gnome_pin::schedule_place(x, y);
    }
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::GtkWindowExt;
        use tao::platform::unix::WindowExtUnix;
        let gtk_window = window.gtk_window();
        gtk_window.set_keep_above(true);
        gtk_window.set_focus_on_map(!config.overlay);
        gtk_window.set_accept_focus(true);
    }
    pass_margin(window, config.overlay);
}

fn monitor_area(window: &tao::window::Window) -> Option<(f64, f64, f64, f64)> {
    #[cfg(target_os = "linux")]
    {
        use gtk::gdk::prelude::MonitorExt;
        use gtk::prelude::WidgetExt;
        use tao::platform::unix::WindowExtUnix;
        let gtk_window = window.gtk_window();
        let surface = gtk_window.window()?;
        let monitor = gtk_window.display().monitor_at_window(&surface)?;
        let geo = monitor.geometry();
        Some((
            geo.x() as f64,
            geo.y() as f64,
            geo.width() as f64,
            geo.height() as f64,
        ))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let monitor = window.current_monitor()?;
        let scale = window.scale_factor();
        let pos = monitor.position().to_logical::<f64>(scale);
        let size = monitor.size().to_logical::<f64>(scale);
        Some((pos.x, pos.y, size.width, size.height))
    }
}

/// The overlay's outer ring is transparent and ignores the pointer.
/// The black border sits inside that ring, so it still moves and resizes.
fn pass_margin(window: &tao::window::Window, overlay: bool) {
    let margin_px = crate::chrome::MARGIN;
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::WidgetExt;
        use tao::platform::unix::WindowExtUnix;
        let gtk_window = window.gtk_window();
        if !overlay {
            gtk_window.input_shape_combine_region(None);
            return;
        }
        let alloc = gtk_window.allocation();
        let width = alloc.width() - margin_px * 2;
        let height = alloc.height() - margin_px * 2;
        if width < 200 || height < 200 {
            gtk_window.input_shape_combine_region(None);
            return;
        }
        let region = gtk::cairo::Region::create_rectangle(&gtk::cairo::RectangleInt::new(
            margin_px, margin_px, width, height,
        ));
        gtk_window.input_shape_combine_region(Some(&region));
    }
    #[cfg(windows)]
    {
        use tao::platform::windows::WindowExtWindows;
        let scale = window.scale_factor();
        let size = window.inner_size();
        let margin = (margin_px as f64 * scale).round() as i32;
        let width = size.width as i32 - margin * 2;
        let height = size.height as i32 - margin * 2;
        unsafe {
            if !overlay || width < 200 || height < 200 {
                SetWindowRgn(
                    window.hwnd() as *mut core::ffi::c_void,
                    core::ptr::null_mut(),
                    1,
                );
                return;
            }
            let region = CreateRectRgn(margin, margin, margin + width, margin + height);
            if SetWindowRgn(window.hwnd() as *mut core::ffi::c_void, region, 1) == 0 {
                DeleteObject(region);
            }
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (window, overlay);
    }
}

#[cfg(windows)]
unsafe extern "system" {
    fn CreateRectRgn(x1: i32, y1: i32, x2: i32, y2: i32) -> *mut core::ffi::c_void;
    fn SetWindowRgn(
        hwnd: *mut core::ffi::c_void,
        region: *mut core::ffi::c_void,
        redraw: i32,
    ) -> i32;
    fn DeleteObject(object: *mut core::ffi::c_void) -> i32;
}

/// Overlay on or off. Windows hit-testing reads this while the pointer moves.
static OVERLAY: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Page zoom, separate from the monitor scale. Linux resets WebKit's zoom
    /// when the widget scale changes, and this is what it puts back.
    static USER_ZOOM: std::cell::Cell<f64> = const { std::cell::Cell::new(1.0) };
}

#[cfg(target_os = "linux")]
fn user_zoom() -> f64 {
    USER_ZOOM.with(std::cell::Cell::get)
}

fn set_user_zoom(zoom: f64) {
    USER_ZOOM.with(|cell| cell.set(zoom));
}

fn install_chrome(window: &tao::window::Window, webview: &wry::WebView) {
    #[cfg(target_os = "linux")]
    install_linux_chrome(window, webview);
    #[cfg(windows)]
    {
        let _ = webview;
        install_windows_chrome(window);
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (window, webview);
    }
}

#[cfg(target_os = "linux")]
fn install_linux_chrome(window: &tao::window::Window, webview: &wry::WebView) {
    use gtk::prelude::{GtkWindowExt, WidgetExt, WidgetExtManual};
    use tao::platform::unix::WindowExtUnix;
    use wry::WebViewExtUnix;

    // The border widgets own move and resize. A click on the page stays on
    // the page. Ctrl+left still moves, using the click's own screen position
    // and time. The page's local position is not the window's.
    use webkit2gtk::WebViewExt;

    let gtk_window = window.gtk_window().clone();
    let page = webview.webview();
    let history = page.clone();
    page.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
    page.connect_button_press_event(move |_, event| {
        // GDK buttons 8 and 9 are browser back and forward. 4 and 5 are the wheel.
        match event.button() {
            8 => {
                history.go_back();
                return gtk::glib::Propagation::Stop;
            }
            9 => {
                history.go_forward();
                return gtk::glib::Propagation::Stop;
            }
            _ => {}
        }
        let ctrl = event.state().contains(gtk::gdk::ModifierType::CONTROL_MASK);
        if event.button() != 1 || !ctrl {
            return gtk::glib::Propagation::Proceed;
        }
        let (x, y) = event.root();
        gtk_window.begin_move_drag(1, x.round() as i32, y.round() as i32, event.time());
        gtk::glib::Propagation::Stop
    });
}

#[cfg(windows)]
fn install_windows_chrome(window: &tao::window::Window) {
    use tao::platform::windows::WindowExtWindows;
    unsafe {
        SetWindowSubclass(
            window.hwnd() as *mut core::ffi::c_void,
            chrome_subclass,
            1,
            0,
        );
    }
}

#[cfg(windows)]
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

#[cfg(windows)]
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

#[cfg(windows)]
#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[cfg(windows)]
#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(windows)]
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

fn build_webview<'a>(
    window: &'a tao::window::Window,
    web_context: &'a mut WebContext,
    page: &crate::model::Page,
    catalog: Option<&str>,
    plugins: &[Plugin],
    active_plugin: &str,
    proxy: EventLoopProxy<ShellEvent>,
    hotkey: &str,
    system_confirms: bool,
    overlay: bool,
    bindings: &crate::model::Bindings,
) -> Result<wry::WebView> {
    #[cfg(target_os = "linux")]
    let frame_proxy = proxy.clone();
    let nav_proxy = proxy.clone();
    let mut builder = WebViewBuilder::new_with_web_context(web_context)
        .with_transparent(true)
        .with_ipc_handler(move |request: wry::http::Request<String>| {
            let body = request.into_body();
            let _ = proxy.send_event(ShellEvent::Ipc(body));
        })
        .with_on_page_load_handler(move |event, url| {
            if let wry::PageLoadEvent::Finished = event {
                let _ = nav_proxy.send_event(ShellEvent::Navigated(url));
            }
        });
    builder = builder.with_initialization_script(settings::script(
        hotkey,
        system_confirms,
        overlay,
        plugins,
        active_plugin,
        bindings,
    ));
    if let Some(script) = catalog {
        builder = builder.with_initialization_script(script);
    }
    if let Some(url) = &page.url {
        builder = builder.with_url(url);
    } else if let Some(html) = &page.html {
        builder = builder.with_html(html);
    } else {
        return Err(Error::new("page has neither a url nor html"));
    }

    #[cfg(target_os = "linux")]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window
            .default_vbox()
            .ok_or_else(|| Error::new("the window has no content area for the webview"))?;
        let center = crate::frame::mount(
            vbox,
            window.gtk_window(),
            hotkey,
            &settings::hint(system_confirms),
            overlay,
            plugins,
            active_plugin,
            bindings,
            move |body| {
                let _ = frame_proxy.send_event(ShellEvent::Ipc(body));
            },
        );
        builder
            .build_gtk(&center)
            .map_err(|err| Error::new(format!("opening webview: {err}")))?
    };
    #[cfg(not(target_os = "linux"))]
    let webview = builder
        .build(window)
        .map_err(|err| Error::new(format!("opening webview: {err}")))?;
    Ok(webview)
}

enum PreparedShortcuts {
    #[cfg(target_os = "linux")]
    Portal(crate::portal::Portal),
    Keys,
    Failed(String),
}

fn prepare_shortcuts(parent: Arc<Mutex<String>>) -> PreparedShortcuts {
    #[cfg(target_os = "linux")]
    if linux_scale::wayland_session() {
        return match crate::portal::prepare(parent) {
            Ok(portal) => PreparedShortcuts::Portal(portal),
            Err(err) => PreparedShortcuts::Failed(err.to_string()),
        };
    }
    let _ = parent;
    PreparedShortcuts::Keys
}

enum BindResult {
    Active,
    Waiting,
    Failed(String),
}

enum ShortcutHost {
    #[cfg(target_os = "linux")]
    Portal(crate::portal::Portal),
    Keys(Grabber),
    Idle,
}

impl ShortcutHost {
    fn confirms(&self) -> bool {
        match self {
            #[cfg(target_os = "linux")]
            ShortcutHost::Portal(_) => true,
            _ => false,
        }
    }

    fn bind(&mut self, hotkey: &Hotkey) -> BindResult {
        match self {
            #[cfg(target_os = "linux")]
            ShortcutHost::Portal(portal) => {
                portal.bind(hotkey.clone());
                BindResult::Waiting
            }
            ShortcutHost::Keys(grabber) => match grabber.bind(hotkey) {
                Ok(()) => BindResult::Active,
                Err(err) => BindResult::Failed(err.to_string()),
            },
            ShortcutHost::Idle => {
                BindResult::Failed("the shortcut service is not available".to_string())
            }
        }
    }
}

struct Grabber {
    manager: GlobalHotKeyManager,
    current: Option<HotKey>,
}

impl Grabber {
    fn start(proxy: EventLoopProxy<ShellEvent>) -> Result<Self> {
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state == HotKeyState::Pressed {
                let _ = proxy.send_event(ShellEvent::Toggle {
                    activation_token: None,
                });
            }
        }));
        let manager = GlobalHotKeyManager::new()
            .map_err(|err| Error::new(format!("could not register the shortcut: {err}")))?;
        Ok(Self {
            manager,
            current: None,
        })
    }

    fn bind(&mut self, hotkey: &Hotkey) -> Result<()> {
        let next = hotkey.to_global()?;
        if self.current == Some(next) {
            return Ok(());
        }
        // Register the new keys first. A failure leaves the old shortcut in place.
        self.manager
            .register(next)
            .map_err(|err| Error::new(format!("could not register the shortcut: {err}")))?;
        if let Some(old) = self.current.replace(next) {
            let _ = self.manager.unregister(old);
        }
        Ok(())
    }
}

struct Shell {
    config: AppConfig,
    config_file: PathBuf,
    plugins: Vec<Plugin>,
    shown: bool,
    shortcut: String,
    live: bool,
    status: String,
    host: ShortcutHost,
    /// First shortcut. Bound once the window can parent the confirm dialog.
    initial: Option<Hotkey>,
    bind_started: bool,
    /// Page zoom. 1.0 is 100%. This is not the monitor scale.
    zoom: f64,
    /// The window currently fills the monitor. Config still holds the frame from before that.
    filled: bool,
}

impl Shell {
    fn arm(
        prepared: PreparedShortcuts,
        proxy: EventLoopProxy<ShellEvent>,
        hotkey: Hotkey,
        config: AppConfig,
        config_file: PathBuf,
        plugins: Vec<Plugin>,
    ) -> Self {
        let shortcut = hotkey.canonical();
        let mut initial = None;
        let (host, live, status) = match prepared {
            #[cfg(target_os = "linux")]
            PreparedShortcuts::Portal(portal) => {
                let proxy_for_portal = proxy;
                portal.listen(move |note| {
                    let event = match note {
                        crate::portal::Note::Pressed { activation_token } => {
                            ShellEvent::Toggle { activation_token }
                        }
                        crate::portal::Note::Bound(bound) => ShellEvent::Bound(bound.canonical()),
                        crate::portal::Note::Failed(message) => ShellEvent::Failed(message),
                    };
                    let _ = proxy_for_portal.send_event(event);
                });
                // Bind after the window exports a parent handle. Binding here
                // opens the confirm dialog with no parent, under the top bar.
                initial = Some(hotkey);
                (
                    ShortcutHost::Portal(portal),
                    false,
                    "Confirm the shortcut in the system dialog".to_string(),
                )
            }
            PreparedShortcuts::Keys => match Grabber::start(proxy) {
                Ok(mut grabber) => match grabber.bind(&hotkey) {
                    Ok(()) => (ShortcutHost::Keys(grabber), true, String::new()),
                    Err(err) => {
                        eprintln!("gaming_companion: {err}");
                        (ShortcutHost::Keys(grabber), false, err.to_string())
                    }
                },
                Err(err) => {
                    eprintln!("gaming_companion: {err}");
                    (ShortcutHost::Idle, false, err.to_string())
                }
            },
            PreparedShortcuts::Failed(message) => {
                eprintln!("gaming_companion: {message}");
                (ShortcutHost::Idle, false, message)
            }
        };
        Self {
            config,
            config_file,
            plugins,
            shown: true,
            shortcut,
            live,
            status,
            host,
            initial,
            bind_started: false,
            zoom: 1.0,
            filled: false,
        }
    }

    fn on_event(
        &mut self,
        event: ShellEvent,
        window: &tao::window::Window,
        webview: &wry::WebView,
    ) {
        match event {
            ShellEvent::Toggle { activation_token } => {
                self.shown = !self.shown;
                set_shown(
                    window,
                    self.shown,
                    self.config.overlay,
                    activation_token.as_deref(),
                );
            }
            ShellEvent::Ipc(body) => self.on_ipc(&body, window, webview),
            ShellEvent::Navigated(url) => {
                let _ = webview.zoom(self.zoom);
                self.follow_url(&url, window, webview);
            }
            ShellEvent::Bound(canonical) => self.accept(canonical, window, webview),
            ShellEvent::Failed(message) => {
                self.status = message;
                self.push(webview);
            }
            ShellEvent::StartBind => self.start_bind(window, webview),
            // Quit exits from the event loop so the frame is saved first.
            ShellEvent::Quit => {}
        }
    }

    fn start_bind(&mut self, window: &tao::window::Window, webview: &wry::WebView) {
        if self.bind_started || !self.host.confirms() {
            return;
        }
        let Some(hotkey) = self.initial.clone() else {
            return;
        };
        self.bind_started = true;
        match self.host.bind(&hotkey) {
            BindResult::Active => self.accept(hotkey.canonical(), window, webview),
            BindResult::Waiting => {
                self.status = "Confirm the shortcut in the system dialog".to_string();
                self.push(webview);
            }
            BindResult::Failed(message) => {
                self.status = message;
                self.push(webview);
            }
        }
    }

    fn on_ipc(&mut self, body: &str, window: &tao::window::Window, webview: &wry::WebView) {
        let body = body.trim().trim_matches('"');
        if body == "ready" {
            self.push(webview);
            return;
        }
        if body == "overlay" {
            self.toggle_overlay(window, webview);
            return;
        }
        if body == "back" {
            let _ = webview.go_back();
            return;
        }
        if body == "forward" {
            let _ = webview.go_forward();
            return;
        }
        if body == "reload" {
            let _ = webview.reload();
            return;
        }
        if body == "zoom-in" {
            self.set_zoom(webview, self.zoom + 0.25);
            return;
        }
        if body == "zoom-out" {
            self.set_zoom(webview, self.zoom - 0.25);
            return;
        }
        if body == "zoom-reset" {
            self.set_zoom(webview, 1.0);
            return;
        }
        if body == "fill" {
            self.fill_toggle(window);
            return;
        }
        if body == "escape" {
            if self.filled {
                self.restore_frame(window);
            }
            return;
        }
        if let Some(rest) = body.strip_prefix("bind:") {
            self.bind_action(rest, window, webview);
            return;
        }
        if let Some(id) = body.strip_prefix("open:") {
            self.open_plugin(id.trim(), window, webview);
            return;
        }
        if let Some(id) = body.strip_prefix("game:") {
            self.open_game(id.trim(), window, webview);
            return;
        }
        let Some(text) = body.strip_prefix("set:") else {
            return;
        };
        let parsed = match hotkey::parse(text.trim()) {
            Ok(hotkey) => hotkey,
            Err(err) => {
                self.status = err.to_string();
                self.push(webview);
                return;
            }
        };
        let canonical = parsed.canonical();
        if let Err(err) = self.config.bindings.check(&canonical) {
            self.status = err.to_string();
            self.push(webview);
            return;
        }
        if self.live && canonical == self.shortcut {
            self.status = "Saved".to_string();
            self.push(webview);
            return;
        }
        match self.host.bind(&parsed) {
            BindResult::Active => self.accept(canonical, window, webview),
            BindResult::Waiting => {
                self.status = "Confirm the shortcut in the system dialog".to_string();
                let _ = webview.evaluate_script(&settings::apply_call(
                    &canonical,
                    &self.status,
                    self.config.overlay,
                ));
                #[cfg(target_os = "linux")]
                crate::frame::apply(&canonical, &self.status, self.config.overlay);
            }
            BindResult::Failed(message) => {
                self.status = message;
                self.push(webview);
            }
        }
    }

    fn accept(&mut self, canonical: String, window: &tao::window::Window, webview: &wry::WebView) {
        self.shortcut = canonical;
        self.config.hotkey = self.shortcut.clone();
        self.live = true;
        self.status = "Saved".to_string();
        self.save(window);
        self.push(webview);
        eprintln!("gaming_companion: shortcut saved {}", self.shortcut);
    }

    fn push(&self, webview: &wry::WebView) {
        let _ = webview.evaluate_script(&settings::apply_call(
            &self.shortcut,
            &self.status,
            self.config.overlay,
        ));
        #[cfg(target_os = "linux")]
        {
            crate::frame::apply(&self.shortcut, &self.status, self.config.overlay);
            crate::frame::show_bindings(&self.config.bindings);
        }
        let _ = webview.evaluate_script(&settings::bindings_call(&self.config.bindings));
        self.mark(webview);
    }

    fn mark(&self, webview: &wry::WebView) {
        let _ = webview.evaluate_script(&settings::select_call(&self.config.active_plugin));
        #[cfg(target_os = "linux")]
        crate::frame::select(&self.config.active_plugin);
    }

    fn open_game(&mut self, game_id: &str, window: &tao::window::Window, webview: &wry::WebView) {
        if self
            .plugins
            .iter()
            .any(|plugin| plugin.id == self.config.active_plugin && plugin.game_id == game_id)
        {
            return;
        }
        let Some(id) = self
            .plugins
            .iter()
            .find(|plugin| plugin.game_id == game_id)
            .map(|plugin| plugin.id.clone())
        else {
            return;
        };
        self.open_plugin(&id, window, webview);
    }

    fn open_plugin(&mut self, id: &str, window: &tao::window::Window, webview: &wry::WebView) {
        if id == self.config.active_plugin {
            return;
        }
        let Some(plugin) = self.plugins.iter().find(|plugin| plugin.id == id).cloned() else {
            return;
        };
        let Some(resource) = plugin.resource(&plugin.entry).cloned() else {
            return;
        };
        let page = match prepare_page(&plugin, &resource) {
            Ok(page) => page,
            Err(err) => {
                eprintln!("gaming_companion: {err}");
                return;
            }
        };
        self.config.active_plugin = plugin.id.clone();
        self.config.active_resource = Some(resource.id.clone());
        self.persist();
        window.set_title(&page.title);
        let loaded = if let Some(url) = page.url.as_deref() {
            webview.load_url(url)
        } else if let Some(html) = page.html.as_deref() {
            webview.load_html(html)
        } else {
            eprintln!("gaming_companion: page has neither a url nor html");
            self.mark(webview);
            return;
        };
        if let Err(err) = loaded {
            eprintln!("gaming_companion: {err}");
        }
        self.mark(webview);
        eprintln!("gaming_companion: {} ({})", page.title, plugin.id);
    }

    fn follow_url(&mut self, url: &str, window: &tao::window::Window, webview: &wry::WebView) {
        let found = matching_resource(&self.plugins, url).map(|(plugin, resource)| {
            (
                plugin.id.clone(),
                plugin.name.clone(),
                resource.id.clone(),
                resource.title.clone(),
            )
        });
        if let Some((id, name, resource_id, title)) = found {
            let changed = self.config.active_plugin != id
                || self.config.active_resource.as_deref() != Some(resource_id.as_str());
            if changed {
                let title = format!("{name} - {title}");
                self.config.active_plugin = id.clone();
                self.config.active_resource = Some(resource_id);
                window.set_title(&title);
                self.persist();
                eprintln!("gaming_companion: {title} ({id})");
            }
        }
        self.mark(webview);
    }

    fn set_zoom(&mut self, webview: &wry::WebView, zoom: f64) {
        self.zoom = zoom.clamp(0.5, 3.0);
        set_user_zoom(self.zoom);
        let _ = webview.zoom(self.zoom);
    }

    fn fill_toggle(&mut self, window: &tao::window::Window) {
        if self.filled {
            self.restore_frame(window);
            return;
        }
        let Some((x, y, width, height)) = monitor_area(window) else {
            return;
        };
        remember_frame(window, &mut self.config);
        self.filled = true;
        window.set_outer_position(LogicalPosition::new(x, y));
        window.set_inner_size(LogicalSize::new(width, height));
        #[cfg(target_os = "linux")]
        crate::gnome_pin::schedule_place(x, y);
        self.persist();
        eprintln!("gaming_companion: fill");
    }

    fn restore_frame(&mut self, window: &tao::window::Window) {
        self.filled = false;
        apply_presentation(window, &self.config);
        eprintln!("gaming_companion: restore");
    }

    fn bind_action(&mut self, rest: &str, window: &tao::window::Window, webview: &wry::WebView) {
        let Some((id, chord)) = rest.split_once(':') else {
            return;
        };
        let Some(action) = Action::from_id(id) else {
            return;
        };
        let canon = match bindings::normalized(chord.trim()) {
            Ok(value) => value,
            Err(err) => {
                self.status = err.to_string();
                self.push(webview);
                return;
            }
        };
        let mut next = self.config.bindings.clone();
        next.set(action, &canon);
        if let Err(err) = next.check(&self.config.hotkey) {
            self.status = err.to_string();
            self.push(webview);
            return;
        }
        self.config.bindings = next;
        self.status = "Saved".to_string();
        self.save(window);
        self.push(webview);
    }

    fn toggle_overlay(&mut self, window: &tao::window::Window, webview: &wry::WebView) {
        if !self.filled {
            remember_frame(window, &mut self.config);
        }
        self.filled = false;
        self.config.overlay = !self.config.overlay;
        OVERLAY.store(self.config.overlay, Ordering::Relaxed);
        #[cfg(target_os = "linux")]
        crate::frame::set_overlay(self.config.overlay);
        if self.config.overlay
            && self.config.overlay_window.is_none()
            && let Some((x, y, width, height)) = monitor_area(window)
        {
            self.config.overlay_window = Some(config::overlay_panel(x, y, width, height));
        }
        apply_presentation(window, &self.config);
        if self.shown {
            set_shown(window, true, self.config.overlay, None);
        }
        self.status = if self.config.overlay {
            "Overlay on".to_string()
        } else {
            "Window".to_string()
        };
        eprintln!(
            "gaming_companion: {}",
            if self.config.overlay {
                "overlay on"
            } else {
                "window"
            }
        );
        self.persist();
        self.push(webview);
    }

    fn pass_margin(&self, window: &tao::window::Window) {
        // A resize while the sheet is concealed would fill the input region
        // again, and the invisible window would start eating clicks.
        if !self.shown {
            return;
        }
        pass_margin(window, self.config.overlay);
    }

    fn persist(&self) {
        if let Err(err) = paths::save_config(&self.config_file, &self.config) {
            eprintln!("gaming_companion: {err}");
        }
    }

    fn save(&mut self, window: &tao::window::Window) {
        if !self.filled {
            remember_frame(window, &mut self.config);
        }
        self.persist();
    }
}

/// Ask GDK for a Wayland parent handle, then bind on a timer if it never arrives.
#[cfg(target_os = "linux")]
fn arm_portal_parent(
    window: &tao::window::Window,
    parent: &Arc<Mutex<String>>,
    proxy: &EventLoopProxy<ShellEvent>,
) -> bool {
    if !linux_scale::wayland_session() {
        return false;
    }
    let fallback = proxy.clone();
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(800), move || {
        let _ = fallback.send_event(ShellEvent::StartBind);
        gtk::glib::ControlFlow::Break
    });
    !export_parent_handle(window, parent, proxy)
}

#[cfg(target_os = "linux")]
struct ExportCtx {
    parent: Arc<Mutex<String>>,
    proxy: EventLoopProxy<ShellEvent>,
}

#[cfg(target_os = "linux")]
unsafe extern "C" fn on_exported(
    _window: *mut gdk_wayland_sys::GdkWaylandWindow,
    handle: *const libc::c_char,
    user_data: *mut libc::c_void,
) {
    if user_data.is_null() {
        return;
    }
    let ctx = unsafe { &*user_data.cast::<ExportCtx>() };
    if !handle.is_null() {
        let text = unsafe { std::ffi::CStr::from_ptr(handle) };
        if let Ok(text) = text.to_str()
            && !text.is_empty()
            && let Ok(mut slot) = ctx.parent.lock()
        {
            *slot = format!("wayland:{text}");
        }
    }
    let _ = ctx.proxy.send_event(ShellEvent::StartBind);
}

/// Returns whether GDK accepted the export. The context stays for the process:
/// the callback can run on a later frame, and a dropped box would be a use after free.
#[cfg(target_os = "linux")]
fn export_parent_handle(
    window: &tao::window::Window,
    parent: &Arc<Mutex<String>>,
    proxy: &EventLoopProxy<ShellEvent>,
) -> bool {
    use gtk::glib::translate::ToGlibPtr;
    use gtk::prelude::WidgetExt;
    use tao::platform::unix::WindowExtUnix;

    let Some(gdk) = window.gtk_window().window() else {
        return false;
    };
    let ctx = Box::new(ExportCtx {
        parent: Arc::clone(parent),
        proxy: proxy.clone(),
    });
    let raw = Box::into_raw(ctx);
    let gdk_ptr: *mut gtk::gdk::ffi::GdkWindow = gdk.to_glib_none().0;
    let accepted = unsafe {
        gdk_wayland_sys::gdk_wayland_window_export_handle(
            gdk_ptr.cast(),
            Some(on_exported),
            raw.cast(),
            None,
        )
    };
    if accepted == 0 {
        drop(unsafe { Box::from_raw(raw) });
        return false;
    }
    true
}

/// The Wayland app id starts as the executable name. Publishing the desktop id
/// lets GNOME group the window, and the pin extension accepts either name.
#[cfg(target_os = "linux")]
fn publish_app_id(window: &tao::window::Window) {
    use gtk::glib::translate::ToGlibPtr;
    use gtk::prelude::WidgetExt;
    use tao::platform::unix::WindowExtUnix;

    let Some(gdk) = window.gtk_window().window() else {
        return;
    };
    let Ok(id) = std::ffi::CString::new(crate::portal::APP_ID) else {
        return;
    };
    let gdk_ptr: *mut gtk::gdk::ffi::GdkWindow = gdk.to_glib_none().0;
    unsafe {
        gdk_wayland_window_set_application_id(gdk_ptr.cast(), id.as_ptr());
    }
}

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn gdk_wayland_window_set_application_id(
        window: *mut gtk::gdk::ffi::GdkWindow,
        application_id: *const libc::c_char,
    );
}

/// Set when a focused normal window was minimized instead of unmapped.
/// The toplevel stays, so GNOME still has its rectangle.
#[cfg(target_os = "linux")]
static WAYLAND_MINIMIZED: AtomicBool = AtomicBool::new(false);

fn set_shown(
    window: &tao::window::Window,
    shown: bool,
    overlay: bool,
    activation_token: Option<&str>,
) {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::{GtkWindowExt, WidgetExt};
        use tao::platform::unix::WindowExtUnix;
        let gtk_window = window.gtk_window();
        gtk_window.set_focus_on_map(!overlay);
        gtk_window.set_keep_above(true);
        if linux_scale::wayland_session() {
            wayland_set_shown(window, shown, overlay, activation_token);
            return;
        }
        if shown {
            gtk_window.show_all();
            publish_app_id(window);
            crate::gnome_pin::request();
            // A focused sheet hides game item names. The overlay waits for a click.
            if !overlay {
                gtk_window.present();
            }
        } else {
            gtk_window.hide();
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = activation_token;
        window.set_always_on_top(true);
        if shown && overlay {
            window.set_focusable(false);
        }
        window.set_visible(shown);
        if shown && overlay {
            window.set_focusable(true);
        } else if shown {
            window.set_focus();
        }
    }
}

/// `gtk_window.hide()` on Wayland destroys the xdg toplevel. The next show is a
/// new window, and GNOME puts a large one in the center. Keep this toplevel
/// mapped: hide the page, paint the frame with alpha 0, and ignore the pointer.
#[cfg(target_os = "linux")]
fn wayland_set_shown(
    window: &tao::window::Window,
    shown: bool,
    overlay: bool,
    activation_token: Option<&str>,
) {
    use gtk::prelude::{GtkWindowExt, WidgetExt};
    use tao::platform::unix::WindowExtUnix;

    let gtk_window = window.gtk_window();
    if shown {
        let minimized = WAYLAND_MINIMIZED.swap(false, Ordering::Relaxed);
        reveal_page(window);
        gtk_window.show_all();
        pass_margin(window, overlay);
        publish_app_id(window);
        crate::gnome_pin::request();
        // Overlay stays unfocused so the game keeps item names. A minimized
        // window is the exception: only activation brings it back.
        if !overlay || minimized {
            if let Some(token) = activation_token.filter(|token| !token.is_empty()) {
                set_wayland_startup_id(window, token);
            }
            gtk_window.present();
        }
        return;
    }

    conceal_page(window);
    // Minimizing hands the keyboard back to the game. The next shortcut press
    // carries a fresh token, which is what GNOME accepts for unminimizing.
    // Skip it without a token, or the sheet could stay stuck in the overview.
    // Overlay is never minimized: showing it must not take focus.
    let can_restore = activation_token.is_some_and(|token| !token.is_empty());
    if !overlay && can_restore && gtk_window.is_active() {
        WAYLAND_MINIMIZED.store(true, Ordering::Relaxed);
        gtk_window.iconify();
    }
}

/// Hide the page without unmapping the toplevel. A size request is not pinned:
/// pinning the current allocation makes GTK resend the minimum size forever.
#[cfg(target_os = "linux")]
fn conceal_page(window: &tao::window::Window) {
    use gtk::prelude::{ContainerExt, WidgetExt};
    use tao::platform::unix::WindowExtUnix;

    let gtk_window = window.gtk_window();
    if let Some(vbox) = window.default_vbox() {
        vbox.foreach(|child| {
            child.set_visible(false);
        });
    }
    gtk_window.set_opacity(0.0);
    gtk_window.input_shape_combine_region(Some(&gtk::cairo::Region::create()));
    gtk_window.queue_draw();
}

#[cfg(target_os = "linux")]
fn reveal_page(window: &tao::window::Window) {
    use gtk::prelude::{ContainerExt, WidgetExt};
    use tao::platform::unix::WindowExtUnix;

    let gtk_window = window.gtk_window();
    gtk_window.set_opacity(1.0);
    if let Some(vbox) = window.default_vbox() {
        vbox.foreach(|child| {
            child.set_visible(true);
        });
        vbox.show_all();
    }
}

#[cfg(target_os = "linux")]
fn set_wayland_startup_id(window: &tao::window::Window, token: &str) {
    use gtk::glib::translate::ToGlibPtr;
    use gtk::prelude::WidgetExt;
    use tao::platform::unix::WindowExtUnix;

    let Ok(token) = std::ffi::CString::new(token) else {
        return;
    };
    let display = window.gtk_window().display();
    let display_ptr = display.to_glib_none().0;
    unsafe {
        gdk_wayland_display_set_startup_notification_id(display_ptr, token.as_ptr());
    }
}

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn gdk_wayland_display_set_startup_notification_id(
        display: *mut gtk::gdk::ffi::GdkDisplay,
        startup_id: *const libc::c_char,
    );
}
