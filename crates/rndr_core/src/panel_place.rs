//! Placement for a [`crate::WindowKind::Panel`].
//!
//! The desktop runner turns this into a Wayland layer-shell surface or a Win32
//! popup. Ordinary toplevels do not use it. No live compositor or Win32 session
//! is required to compute the request; whether the OS honors it is recorded on
//! [`crate::platform_support`].

use crate::window::{Position, WindowConfig, WindowKind};

/// Edges the overlay attaches to. Opposite edges stretch; centering uses one
/// corner plus margins instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelAnchor {
    /// Attach to the top edge.
    pub top: bool,
    /// Attach to the bottom edge.
    pub bottom: bool,
    /// Attach to the left edge.
    pub left: bool,
    /// Attach to the right edge.
    pub right: bool,
}

/// Insets from the anchored edges, in logical points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelMargins {
    /// Distance from the top edge.
    pub top: i32,
    /// Distance from the right edge.
    pub right: i32,
    /// Distance from the bottom edge.
    pub bottom: i32,
    /// Distance from the left edge.
    pub left: i32,
}

/// One overlay request. The runner sends it; the support table says whether a
/// live session has shown that the OS applied it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelPlacement {
    /// `true` asks for the topmost overlay layer. `false` is the normal top layer.
    pub above: bool,
    /// Which edges the surface attaches to.
    pub anchor: PanelAnchor,
    /// Insets from those edges, in logical points.
    pub margins: PanelMargins,
    /// Requested width in logical points. At least 1.
    pub width: u32,
    /// Requested height in logical points. At least 1.
    pub height: u32,
    /// `true` asks to take the keyboard. A hidden panel leaves this false.
    pub take_keyboard: bool,
}

/// `WS_EX_NOACTIVATE`. Showing a panel ORs this in so the show itself does not
/// become the foreground window.
pub const WIN32_EX_NOACTIVATE: u32 = 0x0800_0000;
/// `GWL_EXSTYLE`.
pub const WIN32_GWL_EXSTYLE: i32 = -20;
/// `SW_SHOWNOACTIVATE`.
pub const WIN32_SW_SHOWNOACTIVATE: i32 = 4;
/// `HWND_TOPMOST` as a signed pointer-sized value.
pub const WIN32_HWND_TOPMOST: isize = -1;
/// `SWP_NOSIZE`.
pub const WIN32_SWP_NOSIZE: u32 = 0x0001;
/// `SWP_NOMOVE`.
pub const WIN32_SWP_NOMOVE: u32 = 0x0002;
/// `SWP_NOACTIVATE`.
pub const WIN32_SWP_NOACTIVATE: u32 = 0x0010;
/// `SWP_SHOWWINDOW`. Used when a panel show also needs a topmost insert.
pub const WIN32_SWP_SHOWWINDOW: u32 = 0x0040;

/// What the Windows panel path asks Win32 to do.
///
/// Show does not activate. An explicit focus call takes the foreground,
/// because Windows delivers keys to the foreground window and has no
/// non-activating key window. Pinning a window to every virtual desktop is
/// not in this struct: there is no stable public API for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowsPanelAction {
    /// Extended style bit ORed in while the panel should not activate on show.
    pub ex_noactivate: u32,
    /// `ShowWindow` command for a panel show.
    pub show_command: i32,
    /// Raise with `HWND_TOPMOST`.
    pub topmost: bool,
    /// Explicit focus uses `SetForegroundWindow` and clears the no-activate bit.
    pub focus_takes_foreground: bool,
}

/// The Win32 request for every panel, independent of the host this binary was
/// built on.
pub fn windows_panel_action() -> WindowsPanelAction {
    WindowsPanelAction {
        ex_noactivate: WIN32_EX_NOACTIVATE,
        show_command: WIN32_SW_SHOWNOACTIVATE,
        topmost: true,
        focus_takes_foreground: true,
    }
}

/// Layer-shell (or equivalent) request for `config`.
///
/// [`Position::Center`] anchors no edge. A sized layer-shell surface with no
/// anchors is centered on the output, so this does not need the output size.
/// [`Position::Platform`] anchors the top edge only, which centers the surface
/// on that edge. [`Position::At`] anchors the top-left corner and uses margins.
/// Opposite edges are never both set, so the compositor does not stretch the surface.
///
/// `take_keyboard` is set only when the panel is visible and focus was asked
/// for. That grab takes the keyboard; it is not a non-activating focus.
pub fn panel_placement(config: &WindowConfig, visible: bool, focused: bool) -> PanelPlacement {
    let (anchor, margins) = place(config.position);
    PanelPlacement {
        above: config.kind == WindowKind::Panel || config.always_on_top,
        anchor,
        margins,
        width: logical_extent(config.size.width),
        height: logical_extent(config.size.height),
        take_keyboard: visible && focused,
    }
}

fn logical_extent(value: f32) -> u32 {
    if value.is_finite() && value >= 1.0 {
        value.round().clamp(1.0, u32::MAX as f32) as u32
    } else {
        1
    }
}

fn place(position: Position) -> (PanelAnchor, PanelMargins) {
    let none = PanelMargins {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };
    match position {
        Position::Platform => (
            PanelAnchor {
                top: true,
                bottom: false,
                left: false,
                right: false,
            },
            none,
        ),
        Position::At { x, y } => (
            PanelAnchor {
                top: true,
                bottom: false,
                left: true,
                right: false,
            },
            PanelMargins {
                top: margin(y),
                right: 0,
                bottom: 0,
                left: margin(x),
            },
        ),
        Position::Center => (
            PanelAnchor {
                top: false,
                bottom: false,
                left: false,
                right: false,
            },
            none,
        ),
    }
}

fn margin(value: f32) -> i32 {
    if value.is_finite() {
        value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window::{LogicalSize, WindowKind};

    fn config(position: Position) -> WindowConfig {
        WindowConfig {
            size: LogicalSize::new(480.0, 270.0),
            position,
            kind: WindowKind::Panel,
            always_on_top: true,
            ..WindowConfig::default()
        }
    }

    #[test]
    fn center_anchors_nothing_so_the_compositor_centers_it() {
        let placed = panel_placement(&config(Position::Center), true, false);
        assert!(placed.above);
        assert!(!placed.anchor.top && !placed.anchor.bottom);
        assert!(!placed.anchor.left && !placed.anchor.right);
        assert_eq!(placed.margins.left, 0);
        assert_eq!(placed.margins.top, 0);
        assert_eq!(placed.width, 480);
        assert_eq!(placed.height, 270);
        assert!(!placed.take_keyboard);
    }

    #[test]
    fn explicit_origin_is_a_top_left_margin() {
        let placed = panel_placement(&config(Position::At { x: 12.4, y: -3.0 }), false, true);
        assert_eq!(placed.margins.left, 12);
        assert_eq!(placed.margins.top, -3);
        assert!(placed.anchor.top && placed.anchor.left);
        assert!(!placed.take_keyboard);
    }

    #[test]
    fn platform_position_anchors_only_the_top() {
        let placed = panel_placement(&config(Position::Platform), true, true);
        assert!(placed.anchor.top);
        assert!(!placed.anchor.left && !placed.anchor.right && !placed.anchor.bottom);
        assert!(placed.take_keyboard);
    }

    #[test]
    fn windows_show_does_not_activate_and_focus_does() {
        let action = windows_panel_action();
        assert_eq!(action.ex_noactivate, WIN32_EX_NOACTIVATE);
        assert_eq!(action.show_command, WIN32_SW_SHOWNOACTIVATE);
        assert!(action.topmost);
        assert!(action.focus_takes_foreground);
    }
}
