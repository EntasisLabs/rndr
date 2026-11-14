//! Focusable split sash built on [`Splitter`](rndr_core::Splitter).
//!
//! Pointer drag and clamping stay in core [`apply_split`](rndr_core::apply_split).
//! This module adds focus, a slider role, arrow keys, Home/End, double-click
//! reset, and a [`SplitSashChange`] event the host can persist.
//!
//! The ratio is the leading pane's share of the length the two panes share,
//! after the handle, gaps, and padding. Minimums on both sides clamp that
//! share. A later mount with a different ratio replaces it. A mount with the
//! ratio already on screen leaves a drag or key change in place. Resizing the
//! parent keeps the ratio and writes new pane sizes.
//!
//! ```
//! use rndr_core::{Layout, Sim, Size, Viewport};
//! use rndr_ui::split::{SplitSash, SplitSashProps};
//! use rndr_ui::{UiId, UiPlugin};
//!
//! let mut sim = Sim::new();
//! sim.app.add_plugin(UiPlugin);
//! sim.world().insert_resource(Viewport::new(800.0, 400.0));
//! let parent = sim.world().spawn((Layout {
//!     width: Size::Px(800.0),
//!     height: Size::Px(400.0),
//!     ..Layout::row()
//! },));
//! let split = SplitSash::mount(
//!     sim.world(),
//!     parent,
//!     UiId(1),
//!     SplitSashProps::new("Panes"),
//!     0.5,
//! )
//! .unwrap();
//! sim.update();
//! sim.update();
//! let ratio = split.ratio(sim.world()).unwrap();
//! assert!((ratio - 0.5).abs() < 0.02);
//! ```
use crate::lifecycle::{find_keyed_child, require_layout_parent};
use crate::{Token, UiError, UiId, UiTheme};
use rndr_core::*;
use std::time::Duration;

const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const RATIO_EPSILON: f64 = 1e-6;

/// Label, axis, minimums, and keyboard step for one sash.
#[derive(Clone, Debug, PartialEq)]
pub struct SplitSashProps {
    /// Nonblank accessible name. The handle does not show this text.
    pub label: String,
    /// Supplementary description. Not a live announcement.
    pub description: String,
    /// [`Axis::Row`] moves left and right. [`Axis::Column`] moves up and down.
    /// Fixed for the life of the id.
    pub axis: Axis,
    /// Minimum leading pane, in logical points. Finite and at least zero.
    pub min_leading: f32,
    /// Minimum trailing pane, in logical points. Finite and at least zero.
    pub min_trailing: f32,
    /// Ratio added or removed by one arrow key. Finite and greater than zero.
    pub step: f64,
    /// Ratio added or removed by Page Up, Page Down, and Shift+arrow.
    /// Finite and greater than zero.
    pub page: f64,
    /// Ratio restored by a double-click. Finite; stored clamped to `0..=1`.
    pub default_ratio: f64,
    /// Disabled handles are not in the tab order and do not resize.
    pub enabled: bool,
    /// Readonly handles can focus. Keys, drags, and double-click do not resize.
    pub readonly: bool,
}

impl SplitSashProps {
    /// Enabled horizontal sash named `label`.
    ///
    /// Minimums are 48 points. Arrow step is `0.05`. Page step is `0.2`.
    /// Double-click returns to `0.5`.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: String::new(),
            axis: Axis::Row,
            min_leading: 48.0,
            min_trailing: 48.0,
            step: 0.05,
            page: 0.2,
            default_ratio: 0.5,
            enabled: true,
            readonly: false,
        }
    }
}

/// Ratio a sash is asking the host to persist.
///
/// Sent when the clamped ratio changes: the first settled value, each arrow
/// or page key, Home/End, a double-click, a drag frame that moved, and an
/// assistive set-value. The layout already shows `ratio`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitSashChange {
    /// Sash entity. This is the focusable slider, not the split root.
    pub sash: Entity,
    /// Leading share of the two panes, from 0 to 1, after clamping.
    pub ratio: f64,
}
impl Event for SplitSashChange {}

/// One mounted split. Content belongs in [`Self::leading`] and [`Self::trailing`].
///
/// Nest by mounting another [`SplitSash`] into either pane. [`Self::unmount`]
/// despawns the root and everything parented under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SplitSashHandle {
    /// Split root. Its axis is the sash axis.
    pub root: Entity,
    /// Pane before the handle.
    pub leading: Entity,
    /// Focusable slider. Carries the [`Splitter`](rndr_core::Splitter).
    pub sash: Entity,
    /// Pane after the handle. It fills the space the leading pane does not use.
    pub trailing: Entity,
}

/// Semantic split: two panes and a [`Splitter`](rndr_core::Splitter) sash.
pub struct SplitSash;

impl SplitSash {
    /// Mount or update a split under `parent`.
    ///
    /// `ratio` is the leading share, clamped into `0..=1` and then into the
    /// minimums once the parent has a size. Changing [`SplitSashProps::axis`]
    /// on an existing id returns [`UiError::InvalidProps`].
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        props: SplitSashProps,
        ratio: f64,
    ) -> Result<SplitSashHandle, UiError> {
        require_layout_parent(world, parent)?;
        let (props, ratio) = admit(&props, ratio)?;
        if let Some(root) = find_keyed_child(world, parent, id) {
            let key = world
                .get::<SplitKey>(root)
                .copied()
                .ok_or(UiError::IdentityConflict)?;
            let mut state = world
                .get::<SashState>(key.sash)
                .cloned()
                .ok_or(UiError::IdentityConflict)?;
            if state.props.axis != props.axis {
                return Err(UiError::InvalidProps);
            }
            state.props = props.clone();
            state.force = Some(ratio);
            let sash = key.sash;
            world.insert(sash, state);
            write_chrome(world, sash, &props);
            Ok(SplitSashHandle {
                root,
                leading: key.leading,
                sash,
                trailing: key.trailing,
            })
        } else {
            let root = world.spawn((Layout {
                axis: props.axis,
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::column()
            },));
            set_parent(world, root, parent);
            let leading = world.spawn((pane(props.axis, Size::Px(0.0)),));
            set_parent(world, leading, root);
            let (mut bar, mut spec) = splitter();
            spec.min_leading = props.min_leading;
            spec.min_trailing = props.min_trailing;
            let thickness = match bar.width {
                Size::Px(px) => px,
                _ => 8.0,
            };
            if props.axis == Axis::Column {
                bar.width = Size::Fill;
                bar.height = Size::Px(thickness);
            }
            let sash = world.spawn((bar, spec, ResolvedColor));
            set_parent(world, sash, root);
            let trailing = world.spawn((pane(props.axis, Size::Fill),));
            set_parent(world, trailing, root);
            world.insert(
                root,
                SplitKey {
                    id,
                    sash,
                    leading,
                    trailing,
                },
            );
            world.insert(sash, KeyNavigation(sash_key));
            world.insert(sash, AccessMark(AccessRole::Slider));
            world.insert(
                sash,
                SashState {
                    props: props.clone(),
                    ratio,
                    force: Some(ratio),
                    pending: None,
                    reset: false,
                    published: None,
                    press_frame: 0,
                    press_ms: 0,
                    armed: false,
                    leading,
                    trailing,
                },
            );
            write_chrome(world, sash, &props);
            paint(world, leading, Token::Content);
            paint(world, trailing, Token::Content);
            paint(world, sash, Token::StrongStroke);
            Ok(SplitSashHandle {
                root,
                leading,
                sash,
                trailing,
            })
        }
    }
}

impl SplitSashHandle {
    /// Leading share currently shown, from 0 to 1.
    ///
    /// `None` when the sash has been despawned.
    pub fn ratio(self, world: &World) -> Option<f64> {
        world.get::<SashState>(self.sash).map(|state| state.ratio)
    }

    /// Despawn the split, both panes, and every descendant.
    pub fn unmount(self, world: &mut World) -> Result<(), UiError> {
        if !world.is_alive(self.root) || !world.has::<SplitKey>(self.root) {
            return Err(UiError::Stale);
        }
        let mut all = Vec::new();
        collect(world, self.root, &mut all);
        forget_interaction_targets(world, &all);
        despawn(world, self.root);
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct SplitKey {
    id: UiId,
    sash: Entity,
    leading: Entity,
    trailing: Entity,
}
impl Component for SplitKey {}

#[derive(Clone)]
struct SashState {
    props: SplitSashProps,
    ratio: f64,
    /// Host ratio waiting for a measured parent. Cleared once it is written.
    force: Option<f64>,
    /// Assistive or double-click target for this frame.
    pending: Option<f64>,
    /// Drop the pointer drag after rewriting the default ratio.
    reset: bool,
    published: Option<f64>,
    press_frame: u64,
    press_ms: u64,
    armed: bool,
    leading: Entity,
    trailing: Entity,
}
impl Component for SashState {}

pub(crate) fn key_at(world: &World, entity: Entity) -> Option<UiId> {
    world.get::<SplitKey>(entity).map(|key| key.id)
}

/// Keys land before this runs. Pointer drag uses [`apply_split`]. When
/// [`Shell`](rndr_core::Shell) also runs that system, the later call sees no
/// further pointer delta.
pub(crate) fn drive(world: &mut World) {
    note_pointer(world);
    note_access(world);
    let sashes = world.entities_with::<SashState>();
    if sashes.is_empty() {
        return;
    }
    for sash in &sashes {
        absorb(world, *sash);
    }
    let frozen = frozen_sizes(world, &sashes);
    if world.resource::<Drag>().is_none() {
        world.insert_resource(Drag::default());
    }
    apply_split(world);
    restore_frozen(world, &frozen);
    for sash in &sashes {
        finish_reset(world, *sash);
        readback(world, *sash);
        publish(world, *sash);
    }
}

/// Focus and drag color. Direct [`Color`](rndr_core::Color) on the handle.
pub(crate) fn styles(world: &mut World) {
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let drag = world.resource::<Drag>().and_then(|drag| drag.entity);
    for sash in world.entities_with::<SashState>() {
        let disabled = world.has::<ControlDisabled>(sash);
        let token = if disabled {
            Token::SubtleStroke
        } else if focus == Some(sash) || drag == Some(sash) {
            Token::Focus
        } else {
            Token::StrongStroke
        };
        paint(world, sash, token);
    }
}

fn admit(props: &SplitSashProps, ratio: f64) -> Result<(SplitSashProps, f64), UiError> {
    if props.label.trim().is_empty()
        || !props.min_leading.is_finite()
        || props.min_leading < 0.0
        || !props.min_trailing.is_finite()
        || props.min_trailing < 0.0
        || !props.step.is_finite()
        || props.step <= 0.0
        || !props.page.is_finite()
        || props.page <= 0.0
        || !props.default_ratio.is_finite()
        || !ratio.is_finite()
    {
        return Err(UiError::InvalidProps);
    }
    let mut props = props.clone();
    props.default_ratio = props.default_ratio.clamp(0.0, 1.0);
    Ok((props, ratio.clamp(0.0, 1.0)))
}

fn write_chrome(world: &mut World, sash: Entity, props: &SplitSashProps) {
    world.insert(
        sash,
        Splitter {
            min_leading: props.min_leading,
            min_trailing: props.min_trailing,
        },
    );
    world.insert(
        sash,
        Semantics {
            name: Some(props.label.clone()),
            description: props.description.clone(),
            readonly: props.readonly,
        },
    );
    // Focusable stays so a disabled handle remains in the access tree.
    // ControlDisabled removes it from the tab order and from set-value.
    world.insert(sash, Focusable);
    if props.enabled {
        world.remove::<ControlDisabled>(sash);
    } else {
        world.insert(sash, ControlDisabled);
    }
    let horizontal = props.axis == Axis::Row;
    let value = world
        .get::<SashState>(sash)
        .map(|state| state.ratio)
        .unwrap_or(props.default_ratio);
    world.insert(
        sash,
        NumericAccess {
            value,
            min: 0.0,
            max: 1.0,
            step: props.step,
            jump: props.page,
            horizontal,
        },
    );
}

fn pane(axis: Axis, main: Size) -> Layout {
    let (width, height) = match axis {
        Axis::Row => (main, Size::Fill),
        Axis::Column => (Size::Fill, main),
    };
    Layout {
        axis: Axis::Column,
        width,
        height,
        ..Layout::column()
    }
}

fn paint(world: &mut World, entity: Entity, token: Token) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    world.insert(entity, theme.color(token));
}

fn note_pointer(world: &mut World) {
    let Some(pointer) = world.resource::<Input>().map(|input| input.pointer) else {
        return;
    };
    if let Some(sash) = sash_at(world, pointer.position) {
        if access_details(world, sash).set_value {
            let axis = world
                .get::<SashState>(sash)
                .map(|state| state.props.axis)
                .unwrap_or(Axis::Row);
            world.insert_resource(match axis {
                Axis::Row => CursorStyle::ResizeColumn,
                Axis::Column => CursorStyle::ResizeRow,
            });
            world.insert_resource(MotionWatch { active: true });
        }
    }
    if !pointer.pressed || pointer.button != PointerButton::Left {
        return;
    }
    let Some(sash) = sash_at(world, pointer.press_at.unwrap_or(pointer.position)) else {
        return;
    };
    if access_details(world, sash).focus {
        let _ = request_focus(world, sash);
    }
    let Some(mut state) = world.get::<SashState>(sash).cloned() else {
        return;
    };
    let time = world.resource::<Time>().copied().unwrap_or_default();
    let frame = time.frame;
    let ms = millis(time.now);
    let editable = access_details(world, sash).set_value;
    let double = editable && double_press(&state, frame, ms);
    state.armed = true;
    state.press_frame = frame;
    state.press_ms = ms;
    if double {
        state.pending = Some(state.props.default_ratio);
        state.force = None;
        state.reset = true;
    }
    world.insert(sash, state);
}

fn double_press(state: &SashState, frame: u64, ms: u64) -> bool {
    if !state.armed || frame.saturating_sub(state.press_frame) == 0 {
        return false;
    }
    let dt = ms.saturating_sub(state.press_ms);
    (dt == 0 && frame == state.press_frame.saturating_add(1))
        || (dt > 0 && dt <= u64::try_from(DOUBLE_CLICK.as_millis()).unwrap_or(400))
}

fn note_access(world: &mut World) {
    let sets: Vec<NumericAccessSet> = events::<NumericAccessSet>(world).to_vec();
    for set in sets {
        if !world.has::<SashState>(set.entity) || !access_details(world, set.entity).set_value {
            continue;
        }
        if let Some(state) = world.get_mut::<SashState>(set.entity) {
            state.pending = Some(set.value.clamp(0.0, 1.0));
            state.force = None;
        }
    }
}

fn absorb(world: &mut World, sash: Entity) {
    let Some(mut state) = world.get::<SashState>(sash).cloned() else {
        return;
    };
    if let Some(pending) = state.pending.take() {
        state.ratio = pending;
        state.force = None;
    } else if let Some(force) = state.force {
        state.ratio = force;
    }
    world.insert(sash, state);
    if write_ratio(world, sash) {
        if let Some(state) = world.get_mut::<SashState>(sash) {
            state.force = None;
        }
    }
}

fn frozen_sizes(world: &World, sashes: &[Entity]) -> Vec<(Entity, f32)> {
    let mut frozen = Vec::new();
    for sash in sashes {
        if access_details(world, *sash).set_value {
            continue;
        }
        let Some(state) = world.get::<SashState>(*sash) else {
            continue;
        };
        if let Some(px) = main_px(world, state.leading, axis_of(world, *sash)) {
            frozen.push((*sash, px));
        }
    }
    frozen
}

fn restore_frozen(world: &mut World, frozen: &[(Entity, f32)]) {
    let Some(drag) = world.resource::<Drag>().and_then(|drag| drag.entity) else {
        return;
    };
    let Some((_, px)) = frozen.iter().find(|(sash, _)| *sash == drag) else {
        return;
    };
    let axis = axis_of(world, drag);
    let leading = world.get::<SashState>(drag).map(|state| state.leading);
    if let Some(leading) = leading {
        write_main(world, leading, axis, *px);
    }
    if let Some(drag) = world.resource_mut::<Drag>() {
        drag.entity = None;
    }
}

fn finish_reset(world: &mut World, sash: Entity) {
    let reset = world
        .get::<SashState>(sash)
        .is_some_and(|state| state.reset);
    if !reset {
        return;
    }
    if let Some(state) = world.get_mut::<SashState>(sash) {
        state.reset = false;
    }
    let _ = write_ratio(world, sash);
    if world.resource::<Drag>().and_then(|drag| drag.entity) == Some(sash) {
        if let Some(drag) = world.resource_mut::<Drag>() {
            drag.entity = None;
        }
    }
}

fn readback(world: &mut World, sash: Entity) {
    let Some(pair) = pair_of(world, sash) else {
        return;
    };
    let Some(state) = world.get::<SashState>(sash).cloned() else {
        return;
    };
    let Some(px) = main_px(world, state.leading, axis_of(world, sash)) else {
        return;
    };
    let ratio = ratio_of(px, pair);
    if let Some(state) = world.get_mut::<SashState>(sash) {
        state.ratio = ratio;
    }
}

fn publish(world: &mut World, sash: Entity) {
    let Some(pair) = pair_of(world, sash) else {
        return;
    };
    let Some(state) = world.get::<SashState>(sash).cloned() else {
        return;
    };
    let (lo, hi) = bounds(pair, state.props.min_leading, state.props.min_trailing);
    world.insert(
        sash,
        NumericAccess {
            value: state.ratio,
            min: lo,
            max: hi,
            step: state.props.step,
            jump: state.props.page,
            horizontal: state.props.axis == Axis::Row,
        },
    );
    if state
        .published
        .is_some_and(|previous| (previous - state.ratio).abs() <= RATIO_EPSILON)
    {
        return;
    }
    let ratio = state.ratio;
    if let Some(state) = world.get_mut::<SashState>(sash) {
        state.published = Some(ratio);
    }
    send(world, SplitSashChange { sash, ratio });
}

fn sash_key(world: &mut World, entity: Entity, event: &KeyEvent) -> bool {
    if event.released || event.synthetic || event.modifiers.command() || event.modifiers.alt {
        return false;
    }
    let Some(state) = world.get::<SashState>(entity).cloned() else {
        return false;
    };
    let adjustment = match event.logical {
        LogicalKey::Named(NamedKey::Home) => Adjust::End(false),
        LogicalKey::Named(NamedKey::End) => Adjust::End(true),
        LogicalKey::Named(NamedKey::PageUp) => Adjust::Delta(state.props.page),
        LogicalKey::Named(NamedKey::PageDown) => Adjust::Delta(-state.props.page),
        LogicalKey::Named(key) => {
            let Some(sign) = arrow_sign(state.props.axis, key) else {
                return false;
            };
            let amount = if event.modifiers.shift {
                state.props.page
            } else {
                state.props.step
            };
            Adjust::Delta(sign * amount)
        }
        _ => return false,
    };
    if !access_details(world, entity).set_value {
        return true;
    }
    let Some(pair) = pair_of(world, entity) else {
        return true;
    };
    let (lo, hi) = bounds(pair, state.props.min_leading, state.props.min_trailing);
    let next = match adjustment {
        Adjust::End(high) => {
            if high {
                hi
            } else {
                lo
            }
        }
        Adjust::Delta(delta) => (state.ratio + delta).clamp(lo, hi),
    };
    if (next - state.ratio).abs() <= RATIO_EPSILON {
        return true;
    }
    // The drive pass writes the leading pane and emits one event for the
    // ratio that remains after every key in this frame.
    if let Some(stored) = world.get_mut::<SashState>(entity) {
        stored.ratio = next;
        stored.force = None;
    }
    true
}

enum Adjust {
    Delta(f64),
    End(bool),
}

fn arrow_sign(axis: Axis, key: NamedKey) -> Option<f64> {
    match (axis, key) {
        (Axis::Row, NamedKey::ArrowRight | NamedKey::ArrowUp) => Some(1.0),
        (Axis::Row, NamedKey::ArrowLeft | NamedKey::ArrowDown) => Some(-1.0),
        (Axis::Column, NamedKey::ArrowDown | NamedKey::ArrowRight) => Some(1.0),
        (Axis::Column, NamedKey::ArrowUp | NamedKey::ArrowLeft) => Some(-1.0),
        _ => None,
    }
}

fn write_ratio(world: &mut World, sash: Entity) -> bool {
    let Some(pair) = pair_of(world, sash) else {
        return false;
    };
    let Some(state) = world.get::<SashState>(sash).cloned() else {
        return false;
    };
    let (lo, hi) = bounds(pair, state.props.min_leading, state.props.min_trailing);
    let ratio = state.ratio.clamp(lo, hi);
    let max_px = (pair - state.props.min_trailing).max(state.props.min_leading);
    let px = (f64::from(pair) * ratio) as f32;
    let px = px.clamp(state.props.min_leading, max_px);
    write_main(world, state.leading, axis_of(world, sash), px);
    if let Some(state) = world.get_mut::<SashState>(sash) {
        state.ratio = ratio_of(px, pair);
    }
    true
}

fn bounds(pair: f32, min_leading: f32, min_trailing: f32) -> (f64, f64) {
    let pair = f64::from(pair);
    if !pair.is_finite() || pair <= f64::EPSILON {
        return (0.0, 1.0);
    }
    let lo = (f64::from(min_leading) / pair).clamp(0.0, 1.0);
    let hi_px = (pair - f64::from(min_trailing)).max(f64::from(min_leading));
    let hi = (hi_px / pair).clamp(lo, 1.0);
    (lo, hi)
}

fn ratio_of(px: f32, pair: f32) -> f64 {
    if !pair.is_finite() || pair <= f32::EPSILON || !px.is_finite() {
        return 0.0;
    }
    (f64::from(px) / f64::from(pair)).clamp(0.0, 1.0)
}

fn pair_of(world: &World, sash: Entity) -> Option<f32> {
    if let Some(budget) = split_budget(world, sash) {
        if budget.is_finite() && budget > 1.0 {
            return Some(budget);
        }
    }
    let estimate = estimated_pair(world, sash)?;
    (estimate.is_finite() && estimate > 1.0).then_some(estimate)
}

fn estimated_pair(world: &World, sash: Entity) -> Option<f32> {
    let state = world.get::<SashState>(sash)?;
    let parent = world.get::<Parent>(sash)?.entity;
    let axis = axis_of(world, sash);
    let span = estimated_span(world, parent, axis, 0)?;
    let layout = world.get::<Layout>(parent).copied()?;
    let inner = (span - layout.padding * 2.0).max(0.0);
    let kids = children_of(world, parent);
    let gaps = if kids.len() > 1 {
        layout.gap * (kids.len() as f32 - 1.0)
    } else {
        0.0
    };
    let mut extras = 0.0;
    for child in kids {
        if child == state.leading || child == state.trailing {
            continue;
        }
        extras += bar_px(world, child, axis);
    }
    Some((inner - extras - gaps).max(0.0))
}

fn estimated_span(world: &World, entity: Entity, axis: Axis, depth: u8) -> Option<f32> {
    if depth > 32 {
        return None;
    }
    if let Some(rect) = world.get::<Rect>(entity).copied() {
        let measured = match axis {
            Axis::Row => rect.w,
            Axis::Column => rect.h,
        };
        if measured.is_finite() && measured > 1.0 {
            return Some(measured);
        }
    }
    let layout = world.get::<Layout>(entity).copied()?;
    match main_size(layout, axis) {
        Size::Px(px) if px.is_finite() && px > 0.0 => Some(px),
        Size::Fill => {
            if let Some(parent) = world.get::<Parent>(entity).map(|parent| parent.entity) {
                let span = estimated_span(world, parent, axis, depth + 1)?;
                let parent_layout = world.get::<Layout>(parent).copied()?;
                Some((span - parent_layout.padding * 2.0).max(0.0))
            } else {
                let viewport = world.resource::<Viewport>().copied().unwrap_or_default();
                Some(match axis {
                    Axis::Row => viewport.content_width(),
                    Axis::Column => viewport.content_height(),
                })
            }
        }
        _ => None,
    }
}

fn bar_px(world: &World, entity: Entity, axis: Axis) -> f32 {
    if let Some(layout) = world.get::<Layout>(entity).copied() {
        if let Size::Px(px) = main_size(layout, axis) {
            if px.is_finite() && px > 0.0 {
                return px;
            }
        }
    }
    if let Some(rect) = world.get::<Rect>(entity).copied() {
        let measured = match axis {
            Axis::Row => rect.w,
            Axis::Column => rect.h,
        };
        if measured.is_finite() && measured > 0.0 {
            return measured;
        }
    }
    8.0
}

fn axis_of(world: &World, sash: Entity) -> Axis {
    world
        .get::<Parent>(sash)
        .and_then(|parent| world.get::<Layout>(parent.entity))
        .map(|layout| layout.axis)
        .or_else(|| world.get::<SashState>(sash).map(|state| state.props.axis))
        .unwrap_or(Axis::Row)
}

fn main_size(layout: Layout, axis: Axis) -> Size {
    match axis {
        Axis::Row => layout.width,
        Axis::Column => layout.height,
    }
}

fn main_px(world: &World, entity: Entity, axis: Axis) -> Option<f32> {
    let layout = world.get::<Layout>(entity).copied()?;
    match main_size(layout, axis) {
        Size::Px(px) if px.is_finite() => Some(px),
        _ => world.get::<Rect>(entity).map(|rect| match axis {
            Axis::Row => rect.w,
            Axis::Column => rect.h,
        }),
    }
}

fn write_main(world: &mut World, entity: Entity, axis: Axis, px: f32) {
    let Some(layout) = world.get_mut::<Layout>(entity) else {
        return;
    };
    match axis {
        Axis::Row => layout.width = Size::Px(px),
        Axis::Column => layout.height = Size::Px(px),
    }
}

fn sash_at(world: &World, at: [f32; 2]) -> Option<Entity> {
    let hit = hit_test(world, at[0], at[1])?;
    ancestor_with::<SashState>(world, hit)
}

fn millis(now: Duration) -> u64 {
    u64::try_from(now.as_millis()).unwrap_or(u64::MAX)
}

fn collect(world: &World, entity: Entity, out: &mut Vec<Entity>) {
    out.push(entity);
    for child in children_of(world, entity) {
        collect(world, child, out);
    }
}
