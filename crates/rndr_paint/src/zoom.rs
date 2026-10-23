//! Wheel zoom that returns to the same camera after the opposite notches.
//!
//! Each notch is one integer step. Step `0` is scale `1`. Positive steps are
//! `1.1` to that power, and negative steps are the reciprocal, so the scale at
//! a step does not depend on the path taken to reach it. While the cursor stays
//! on the same point, pan is recomputed from the pan and step that were current
//! when that point became the anchor. Ten notches in and ten notches out
//! therefore restore both numbers exactly, instead of drifting through
//! `1.1f32.powf`.

use crate::Affine;

/// Largest zoom-out step. Scale is `1 / 1.1^19`, about `0.164`.
pub const ZOOM_MIN_STEP: i32 = -19;
/// Largest zoom-in step. Scale is `1.1^29`, about `15.86`.
pub const ZOOM_MAX_STEP: i32 = 29;

/// Pan and zoom for a canvas camera.
///
/// [`Self::apply_wheel`] reads the same wheel delta the desktop runner stores
/// on `Input.scroll`: one notch is about `32` logical points. [`Self::drag`]
/// pans while the primary contact is down and drops the zoom anchor, so the
/// next notch sticks to the cursor again.
#[derive(Clone, Copy, Debug)]
pub struct ZoomCamera {
    pan: [f32; 2],
    step: i32,
    anchor: Option<ZoomAnchor>,
    grab: Option<[f32; 2]>,
}

#[derive(Clone, Copy, Debug)]
struct ZoomAnchor {
    cursor: [f32; 2],
    pan: [f32; 2],
    step: i32,
}

impl Default for ZoomCamera {
    fn default() -> Self {
        Self::new([0.0, 0.0])
    }
}

impl ZoomCamera {
    /// Identity zoom, camera origin at `pan`.
    pub fn new(pan: [f32; 2]) -> Self {
        Self {
            pan,
            step: 0,
            anchor: None,
            grab: None,
        }
    }

    /// Translation of the camera, in the same units as the scene.
    pub fn pan(self) -> [f32; 2] {
        self.pan
    }

    /// Integer notch offset from scale `1`.
    pub fn step(self) -> i32 {
        self.step
    }

    /// Uniform scale for [`Self::step`].
    pub fn scale(self) -> f32 {
        scale_at(self.step)
    }

    /// Scale then translation. This is the camera [`crate::Canvas::set_camera`]
    /// expects: world units land on screen as `point * scale + pan`.
    pub fn affine(self) -> Affine {
        Affine::scale(self.scale()).then(Affine::translate(self.pan[0], self.pan[1]))
    }

    /// Zoom around `cursor` by `scroll_y` logical points.
    ///
    /// A magnitude at or below `0.5` is ignored. Anything smaller than one
    /// notch still takes one step, and one event takes at most four. The
    /// result is clamped to [`ZOOM_MIN_STEP`] and [`ZOOM_MAX_STEP`]. The world
    /// point under `cursor` stays fixed. Repeating the call with the negated
    /// delta and the same `cursor` restores [`Self::pan`] and [`Self::scale`].
    pub fn apply_wheel(&mut self, cursor: [f32; 2], scroll_y: f32) {
        let notches = notches(scroll_y);
        if notches == 0 || !cursor[0].is_finite() || !cursor[1].is_finite() {
            return;
        }
        let same = self
            .anchor
            .is_some_and(|anchor| same_point(anchor.cursor, cursor));
        if !same {
            self.anchor = Some(ZoomAnchor {
                cursor,
                pan: self.pan,
                step: self.step,
            });
        }
        let Some(anchor) = self.anchor else {
            return;
        };
        let new_step = (self.step + notches).clamp(ZOOM_MIN_STEP, ZOOM_MAX_STEP);
        if new_step == self.step {
            return;
        }
        let from = scale_at(anchor.step);
        let to = scale_at(new_step);
        if from <= 1.0e-8 {
            return;
        }
        let factor = to / from;
        self.pan = [
            cursor[0] - (cursor[0] - anchor.pan[0]) * factor,
            cursor[1] - (cursor[1] - anchor.pan[1]) * factor,
        ];
        self.step = new_step;
    }

    /// Pan with the primary contact. A real movement drops the zoom anchor.
    ///
    /// The first sample is the anchor. A press and a move that arrive together
    /// are kept by [`Self::drag_from`].
    pub fn drag(&mut self, down: bool, position: [f32; 2]) {
        self.drag_from(down, position, None);
    }

    /// Pan with the primary contact, anchored where the button went down.
    ///
    /// `press_at` is that position on a sample that contains the press. The
    /// first delta is `position - press_at`, so content follows the pointer
    /// when the press and the first move arrive together. Later samples pass
    /// `None` and continue from the previous position. A real movement drops
    /// the zoom anchor. [`Self::drag`] is this with no press position.
    pub fn drag_from(&mut self, down: bool, position: [f32; 2], press_at: Option<[f32; 2]>) {
        if !down {
            self.grab = None;
            return;
        }
        if self.grab.is_none() {
            self.grab = Some(press_at.unwrap_or(position));
        }
        if let Some(last) = self.grab {
            let dx = position[0] - last[0];
            let dy = position[1] - last[1];
            if dx != 0.0 || dy != 0.0 {
                self.pan[0] += dx;
                self.pan[1] += dy;
                self.anchor = None;
            }
        }
        self.grab = Some(position);
    }
}

/// Scale at `step`, clamped to the supported range.
///
/// Positive steps use `1.1.powi`. Negative steps use the reciprocal of that
/// power, so `scale_at(n) * scale_at(-n)` is not required: `scale_at(0)` is
/// the constant `1.0`.
pub fn scale_at(step: i32) -> f32 {
    let step = step.clamp(ZOOM_MIN_STEP, ZOOM_MAX_STEP);
    if step == 0 {
        return 1.0;
    }
    let mag = 1.1f32.powi(step.abs());
    if step > 0 {
        mag
    } else {
        1.0 / mag
    }
}

fn notches(scroll_y: f32) -> i32 {
    if !scroll_y.is_finite() || scroll_y.abs() <= 0.5 {
        return 0;
    }
    let raw = scroll_y / 32.0;
    let notches = if raw.abs() < 1.0 {
        if raw > 0.0 {
            1
        } else {
            -1
        }
    } else {
        raw.round() as i32
    };
    notches.clamp(-4, 4)
}

fn same_point(a: [f32; 2], b: [f32; 2]) -> bool {
    a[0].to_bits() == b[0].to_bits() && a[1].to_bits() == b[1].to_bits()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_notches_each_way_restore_pan_and_scale() {
        let mut camera = ZoomCamera::new([24.0, -8.0]);
        let cursor = [180.5, 96.25];
        let pan = camera.pan();
        let scale = camera.scale();
        for _ in 0..10 {
            camera.apply_wheel(cursor, 32.0);
        }
        assert!(camera.scale() > scale);
        assert_ne!(camera.pan(), pan);
        for _ in 0..10 {
            camera.apply_wheel(cursor, -32.0);
        }
        assert_eq!(camera.scale().to_bits(), scale.to_bits());
        assert_eq!(camera.pan()[0].to_bits(), pan[0].to_bits());
        assert_eq!(camera.pan()[1].to_bits(), pan[1].to_bits());
        assert_eq!(camera.step(), 0);
    }

    #[test]
    fn one_notch_keeps_the_point_under_the_cursor() {
        let mut camera = ZoomCamera::new([10.0, -4.0]);
        let cursor = [80.0, 50.0];
        let before = [
            (cursor[0] - camera.pan()[0]) / camera.scale(),
            (cursor[1] - camera.pan()[1]) / camera.scale(),
        ];
        camera.apply_wheel(cursor, 32.0);
        let after = [
            (cursor[0] - camera.pan()[0]) / camera.scale(),
            (cursor[1] - camera.pan()[1]) / camera.scale(),
        ];
        assert!(
            (before[0] - after[0]).abs() < 1.0e-4,
            "{before:?} {after:?}"
        );
        assert!(
            (before[1] - after[1]).abs() < 1.0e-4,
            "{before:?} {after:?}"
        );
        assert!((camera.scale() - 1.1).abs() < 1.0e-6);
        camera.apply_wheel(cursor, 0.0);
        assert!((camera.scale() - 1.1).abs() < 1.0e-6);
    }

    #[test]
    fn a_drag_drops_the_zoom_anchor() {
        let mut camera = ZoomCamera::new([0.0, 0.0]);
        let cursor = [40.0, 40.0];
        camera.apply_wheel(cursor, 32.0);
        camera.drag(true, cursor);
        camera.drag(true, [48.0, 40.0]);
        camera.drag(false, [48.0, 40.0]);
        let dragged = camera.pan();
        let world = [
            (cursor[0] - dragged[0]) / camera.scale(),
            (cursor[1] - dragged[1]) / camera.scale(),
        ];
        camera.apply_wheel(cursor, -32.0);
        assert_eq!(camera.step(), 0);
        let after = [
            (cursor[0] - camera.pan()[0]) / camera.scale(),
            (cursor[1] - camera.pan()[1]) / camera.scale(),
        ];
        assert!((world[0] - after[0]).abs() < 1.0e-4);
        assert!((world[1] - after[1]).abs() < 1.0e-4);
        assert_ne!(camera.pan(), [0.0, 0.0]);
    }

    #[test]
    fn a_fast_drag_keeps_the_press_delta() {
        let mut camera = ZoomCamera::new([0.0, 0.0]);
        camera.drag_from(true, [40.0, 0.0], Some([0.0, 0.0]));
        assert_eq!(camera.pan()[0], 40.0);
        assert_eq!(camera.pan()[1], 0.0);
        camera.drag_from(true, [80.0, 10.0], None);
        assert_eq!(camera.pan()[0], 80.0);
        assert_eq!(camera.pan()[1], 10.0);
        camera.drag_from(false, [80.0, 10.0], None);
        assert_eq!(camera.pan()[0], 80.0);

        let mut plain = ZoomCamera::new([0.0, 0.0]);
        plain.drag(true, [40.0, 0.0]);
        assert_eq!(plain.pan(), [0.0, 0.0]);
        plain.drag(true, [80.0, 0.0]);
        assert_eq!(plain.pan()[0], 40.0);
    }
}
