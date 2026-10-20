//! Where a click on the borderless window moves or resizes it.
//!
//! The overlay's outer `MARGIN` pixels ignore the pointer. Inside that, an
//! opaque border `BORDER` pixels thick resizes the window. The top border is
//! `TOP` pixels: its outer edge resizes, and the rest of it moves the window.

pub const MARGIN: i32 = 16;
pub const BORDER: i32 = 6;
pub const TOP: i32 = 38;
/// The left of the top bar stays a move grip. Plugins sits just to its right.
pub const GRIP: i32 = 36;
/// Plugins and Settings each occupy this much of the top bar.
#[cfg(any(windows, test))]
pub const PLUGINS_BUTTON: i32 = 96;
/// The Settings button occupies this much of the top bar's right side.
#[cfg(any(windows, test))]
pub const SETTINGS_BUTTON: i32 = 96;

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// Inside the pass-through ring. The click is for whatever is behind the sheet.
    Pass,
    /// The page, including Settings.
    Client,
    Move,
    Resize(Edge),
}

/// `width` and `height` are the window's client size in device pixels.
/// `scale_num / scale_den` is the device-pixel ratio (GTK's scale factor is
/// `n/1`; Windows DPI is `dpi/96`).
///
/// Linux does not use this. The border there is a row of widgets, and the
/// page's own coordinates are not the window's.
#[cfg(any(windows, test))]
pub fn hit(
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    ctrl: bool,
    overlay: bool,
    scale_num: i32,
    scale_den: i32,
) -> Hit {
    if width <= 0 || height <= 0 || x < 0 || y < 0 || x >= width || y >= height {
        return Hit::Pass;
    }
    let mut margin = if overlay {
        px(MARGIN, scale_num, scale_den)
    } else {
        0
    };
    if width - margin * 2 < px(200, scale_num, scale_den)
        || height - margin * 2 < px(200, scale_num, scale_den)
    {
        margin = 0;
    }
    let left = margin;
    let top = margin;
    let right = width - margin;
    let bottom = height - margin;
    if x < left || y < top || x >= right || y >= bottom {
        return Hit::Pass;
    }

    let border = px(BORDER, scale_num, scale_den).max(1);
    let top_bar = px(TOP, scale_num, scale_den).max(border);
    let inner_left = left + border;
    let inner_top = top + border;
    let inner_right = right - border;
    let inner_bottom = bottom - border;
    let can_resize = inner_right > inner_left && inner_bottom > inner_top;
    // The top bar is a title strip: its outer edge resizes, and the rest of
    // it moves, including the pixels above the side borders.
    let title_bottom = (top + top_bar).min(inner_bottom);
    let in_title = can_resize && y >= inner_top && y < title_bottom;
    let west = can_resize && !in_title && x < inner_left;
    let east = can_resize && !in_title && x >= inner_right;
    let north = can_resize && y < inner_top;
    let south = can_resize && y >= inner_bottom;
    let edge = match (north, south, west, east) {
        (true, _, true, _) => Some(Edge::NorthWest),
        (true, _, _, true) => Some(Edge::NorthEast),
        (_, true, true, _) => Some(Edge::SouthWest),
        (_, true, _, true) => Some(Edge::SouthEast),
        (true, _, _, _) => Some(Edge::North),
        (_, true, _, _) => Some(Edge::South),
        (_, _, true, _) => Some(Edge::West),
        (_, _, _, true) => Some(Edge::East),
        _ => None,
    };
    if let Some(edge) = edge {
        return Hit::Resize(edge);
    }
    if ctrl || in_title {
        Hit::Move
    } else {
        Hit::Client
    }
}

/// Plugins sits on the left of the top bar and Settings on the right. A click
/// there is for the button. The rest of that bar moves the window. The lists
/// those buttons open sit over the page, so they are ordinary client clicks.
#[cfg(any(windows, test))]
pub fn settings_target(
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    overlay: bool,
    scale_num: i32,
    scale_den: i32,
) -> bool {
    let Some((left, right, top, bottom)) =
        inner_bounds(width, height, overlay, scale_num, scale_den)
    else {
        return false;
    };
    let border = px(BORDER, scale_num, scale_den).max(1);
    let top_bar = px(TOP, scale_num, scale_den).max(border);
    let inner_top = top + border;
    let title_bottom = (top + top_bar).min(bottom - border);
    if y < inner_top || y >= title_bottom {
        return false;
    }
    let grip = px(GRIP, scale_num, scale_den).max(1);
    let plugins = px(PLUGINS_BUTTON, scale_num, scale_den).max(1);
    let settings = px(SETTINGS_BUTTON, scale_num, scale_den).max(1);
    let in_plugins = x >= left + grip && x < left + grip + plugins;
    let in_settings = x >= right - settings;
    in_plugins || in_settings
}

#[cfg(any(windows, test))]
fn inner_bounds(
    width: i32,
    height: i32,
    overlay: bool,
    scale_num: i32,
    scale_den: i32,
) -> Option<(i32, i32, i32, i32)> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let mut margin = if overlay {
        px(MARGIN, scale_num, scale_den)
    } else {
        0
    };
    if width - margin * 2 < px(200, scale_num, scale_den)
        || height - margin * 2 < px(200, scale_num, scale_den)
    {
        margin = 0;
    }
    let left = margin;
    let top = margin;
    let right = width - margin;
    let bottom = height - margin;
    if right <= left || bottom <= top {
        return None;
    }
    Some((left, right, top, bottom))
}

#[cfg(any(windows, test))]
fn px(logical: i32, num: i32, den: i32) -> i32 {
    let den = den.max(1);
    let num = num.max(1);
    (logical.saturating_mul(num) + den / 2) / den
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_resize_and_the_strip_moves() {
        assert_eq!(
            hit(1000, 800, 2, 2, false, false, 1, 1),
            Hit::Resize(Edge::NorthWest)
        );
        assert_eq!(
            hit(1000, 800, 500, 3, false, false, 1, 1),
            Hit::Resize(Edge::North)
        );
        assert_eq!(
            hit(1000, 800, 998, 400, false, false, 1, 1),
            Hit::Resize(Edge::East)
        );
        assert_eq!(hit(1000, 800, 500, 16, false, false, 1, 1), Hit::Move);
        assert_eq!(hit(1000, 800, 2, 16, false, false, 1, 1), Hit::Move);
        assert_eq!(
            hit(1000, 800, 2, 50, false, false, 1, 1),
            Hit::Resize(Edge::West)
        );
        assert_eq!(hit(1000, 800, 500, 50, false, false, 1, 1), Hit::Client);
        assert_eq!(hit(1000, 800, 500, 400, true, false, 1, 1), Hit::Move);
        assert_eq!(
            hit(1000, 800, 2, 400, true, false, 1, 1),
            Hit::Resize(Edge::West)
        );
    }

    #[test]
    fn overlay_keeps_a_pass_through_ring_outside_the_band() {
        assert_eq!(hit(1000, 800, 10, 10, false, true, 1, 1), Hit::Pass);
        assert_eq!(
            hit(1000, 800, 18, 18, false, true, 1, 1),
            Hit::Resize(Edge::NorthWest)
        );
        assert_eq!(
            hit(1000, 800, 500, 18, false, true, 1, 1),
            Hit::Resize(Edge::North)
        );
        assert_eq!(hit(1000, 800, 500, 30, false, true, 1, 1), Hit::Move);
        assert_eq!(hit(1000, 800, 500, 60, false, true, 1, 1), Hit::Client);
        assert_eq!(
            hit(1000, 800, 980, 400, false, true, 1, 1),
            Hit::Resize(Edge::East)
        );
    }

    #[test]
    fn a_higher_scale_thickens_the_band() {
        assert_eq!(
            hit(1000, 800, 10, 10, false, false, 2, 1),
            Hit::Resize(Edge::NorthWest)
        );
        assert_eq!(hit(1000, 800, 40, 40, false, false, 2, 1), Hit::Move);
        assert_eq!(hit(1000, 800, 40, 100, false, false, 2, 1), Hit::Client);
    }

    #[test]
    fn the_settings_button_is_on_the_right_of_the_title() {
        assert!(!settings_target(1000, 800, 500, 16, false, 1, 1));
        assert!(settings_target(1000, 800, 950, 16, false, 1, 1));
        assert!(!settings_target(1000, 800, 8, 16, false, 1, 1));
        assert!(!settings_target(1000, 800, 950, 3, false, 1, 1));
    }

    #[test]
    fn the_plugins_button_is_on_the_left_of_the_title() {
        assert!(settings_target(1000, 800, 80, 16, false, 1, 1));
        assert!(!settings_target(1000, 800, 20, 16, false, 1, 1));
        assert!(!settings_target(1000, 800, 200, 16, false, 1, 1));
    }
}
