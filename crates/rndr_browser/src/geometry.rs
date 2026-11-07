//! Place a top-left client rect into a parent view's frame coordinates.
//!
//! The host rect's origin is the top-left of the parent bounds. AppKit parents
//! are not all in that space: a flipped view already uses the top-left, and an
//! unflipped view uses the bottom-left. Winit's content view is flipped.

#![cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]

use crate::ClientRect;

/// The parent view's bounds and whether those bounds grow downward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ParentBounds {
    /// `NSView::isFlipped`: true when the bounds origin is the top-left.
    pub flipped: bool,
    /// Bounds origin x, in points.
    pub origin_x: f64,
    /// Bounds origin y, in points.
    pub origin_y: f64,
    /// Bounds width, in points.
    pub width: f64,
    /// Bounds height, in points.
    pub height: f64,
}

/// Frame to give the guest view, in the parent's coordinate system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ViewFrame {
    /// Frame origin x.
    pub x: f64,
    /// Frame origin y, in the parent's own orientation.
    pub y: f64,
    /// Frame width.
    pub width: f64,
    /// Frame height.
    pub height: f64,
}

/// Convert a top-left client rect into the parent view's frame.
pub(crate) fn place_in_parent(parent: ParentBounds, rect: ClientRect) -> ViewFrame {
    let width = f64::from(rect.width);
    let height = f64::from(rect.height);
    let x = parent.origin_x + f64::from(rect.x);
    let y = if parent.flipped {
        parent.origin_y + f64::from(rect.y)
    } else {
        parent.origin_y + parent.height - f64::from(rect.y) - height
    };
    ViewFrame {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> ClientRect {
        ClientRect::try_new(10.0, 20.0, 300.0, 200.0).unwrap()
    }

    fn parent(flipped: bool, height: f64) -> ParentBounds {
        ParentBounds {
            flipped,
            origin_x: 0.0,
            origin_y: 0.0,
            width: 800.0,
            height,
        }
    }

    #[test]
    fn flipped_parent_keeps_the_top_left_and_resize_does_not_move_it() {
        let placed = place_in_parent(parent(true, 600.0), rect());
        assert_eq!(
            placed,
            ViewFrame {
                x: 10.0,
                y: 20.0,
                width: 300.0,
                height: 200.0,
            }
        );
        let resized = place_in_parent(parent(true, 500.0), rect());
        assert_eq!(resized.y, 20.0);
        assert_eq!(resized.x, 10.0);
    }

    #[test]
    fn unflipped_parent_measures_from_the_bottom_and_resize_follows_the_height() {
        let placed = place_in_parent(parent(false, 600.0), rect());
        assert_eq!(placed.y, 380.0);
        assert_eq!(placed.x, 10.0);
        assert_eq!(placed.width, 300.0);
        assert_eq!(placed.height, 200.0);
        let resized = place_in_parent(parent(false, 500.0), rect());
        assert_eq!(resized.y, 280.0);
    }
}
