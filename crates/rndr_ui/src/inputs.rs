//! Slider, range, number stepper, tooltip, and toast.
//!
//! Values are controlled. `mount` publishes the value the control shows and
//! exposes to assistive technologies. Keys, pointer drags, step buttons, and
//! assistive set-value send a request and leave that published value in place
//! until the host mounts again. A new mount is the acknowledgement boundary:
//! the same published value restores a stepper that is not mid-edit, and a
//! different value replaces the draft.
//!
//! Date and color pickers are not part of this module.
use crate::lifecycle::{find_keyed_child, require_layout_parent};
use crate::{text, Corner, Token, Typography, UiError, UiId, UiRecipe, UiStyle, UiTheme};
use rndr_core::motion::{self, Curve, Easing, MotionSettings, Prop};
use rndr_core::*;
use std::time::Duration;

/// Inclusive numeric domain shared by sliders and the stepper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberRange {
    /// Inclusive lower bound. Must be finite and less than `max`.
    pub min: f64,
    /// Inclusive upper bound. Must be finite and greater than `min`.
    pub max: f64,
    /// Arrow and button step. Must be finite and greater than zero.
    pub step: f64,
    /// Page Up / Page Down step. Must be finite and greater than zero.
    pub page: f64,
}

impl Default for NumberRange {
    fn default() -> Self {
        Self {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            page: 10.0,
        }
    }
}

impl NumberRange {
    /// `0..=100` with a step of 1 and a page of 10, then `min`/`max` applied.
    pub fn new(min: f64, max: f64) -> Self {
        Self {
            min,
            max,
            ..Self::default()
        }
    }

    fn valid(self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.min < self.max
            && self.step.is_finite()
            && self.step > 0.0
            && self.page.is_finite()
            && self.page > 0.0
    }

    fn access(self, value: f64, horizontal: bool) -> NumericAccess {
        NumericAccess {
            value,
            min: self.min,
            max: self.max,
            step: self.step,
            jump: self.page,
            horizontal,
        }
    }
}

/// Shared label and availability for a numeric control.
#[derive(Clone, Debug, PartialEq)]
pub struct SliderProps {
    /// Nonblank visible and accessible name.
    pub label: String,
    /// Supplementary description. Not a live announcement.
    pub description: String,
    /// Disabled controls cannot focus or change.
    pub enabled: bool,
    /// Readonly controls can focus, but keys and drags do not change them.
    pub readonly: bool,
    /// Domain, step, and page size.
    pub range: NumberRange,
    /// Vertical sliders use the same keys: Up/Right increase, Down/Left decrease.
    pub vertical: bool,
}

impl SliderProps {
    /// Enabled horizontal slider named `label` on the default range.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: String::new(),
            enabled: true,
            readonly: false,
            range: NumberRange::default(),
            vertical: false,
        }
    }
}

/// Properties for a number field with step buttons.
#[derive(Clone, Debug, PartialEq)]
pub struct NumberProps {
    /// Nonblank visible and accessible name.
    pub label: String,
    /// Supplementary description, replaced while a commit is invalid.
    pub description: String,
    /// Disabled fields cannot focus or change.
    pub enabled: bool,
    /// Readonly fields can focus and select text, but cannot change the number.
    pub readonly: bool,
    /// Domain, step, and page size.
    pub range: NumberRange,
}

impl NumberProps {
    /// Enabled field named `label` on the default range.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: String::new(),
            enabled: true,
            readonly: false,
            range: NumberRange::default(),
        }
    }
}

/// Properties for one toast.
#[derive(Clone, Debug, PartialEq)]
pub struct ToastProps {
    /// Nonblank status message.
    pub message: String,
    /// Undo button label. `None` omits the button. Blank is invalid.
    pub undo: Option<String>,
    /// Time until automatic dismiss. Zero stays until Undo, Dismiss, or Escape.
    pub duration: Duration,
}

impl ToastProps {
    /// Sticky toast with no undo action.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            undo: None,
            duration: Duration::ZERO,
        }
    }

    /// Toast that dismisses itself after `duration`.
    pub fn timed(message: impl Into<String>, duration: Duration) -> Self {
        Self {
            message: message.into(),
            undo: None,
            duration,
        }
    }
}

/// Why a toast closed or offered undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastNotice {
    /// The undo button was activated. The toast also closes.
    Undo,
    /// Escape, the dismiss button, or [`ToastHandle::dismiss`].
    Dismissed,
    /// [`ToastProps::duration`] elapsed while the toast was not hovered or focused.
    Expired,
}

/// Published slider value the host has not necessarily accepted yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderChange {
    /// Slider root.
    pub control: Entity,
    /// Complete desired value.
    pub value: f64,
}
impl Event for SliderChange {}

/// Published range the host has not necessarily accepted yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RangeChange {
    /// Range root.
    pub control: Entity,
    /// Complete desired start, kept at or below `end`.
    pub start: f64,
    /// Complete desired end, kept at or above `start`.
    pub end: f64,
}
impl Event for RangeChange {}

/// A clamped number the stepper is asking the host to publish.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberChange {
    /// Stepper root.
    pub control: Entity,
    /// Clamped, snapped value.
    pub value: f64,
}
impl Event for NumberChange {}

/// Enter or blur on a draft that is not a number. The field reverts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberRejected {
    /// Stepper root.
    pub control: Entity,
    /// Draft that failed to parse, before the revert.
    pub draft: String,
}
impl Event for NumberRejected {}

/// Toast notice. Closing starts immediately; reduced motion hides on that frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToastEvent {
    /// Toast root.
    pub toast: Entity,
    /// What happened.
    pub notice: ToastNotice,
}
impl Event for ToastEvent {}

/// Single-value slider root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SliderHandle {
    /// Focusable slider.
    pub root: Entity,
}

/// Two-thumb range. Each thumb is its own slider in the accessibility tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangeHandle {
    /// Group root.
    pub root: Entity,
    /// Lower thumb.
    pub start: Entity,
    /// Upper thumb.
    pub end: Entity,
}

/// Number field root. `field` is the spin button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumberHandle {
    /// Group root.
    pub root: Entity,
    /// Editable spin button.
    pub field: Entity,
}

/// Tooltip attached to a target. The popup is library-owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooltipHandle {
    /// Tooltip popup.
    pub root: Entity,
    /// Control the tooltip describes.
    pub target: Entity,
}

/// Toast root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToastHandle {
    /// Status region.
    pub root: Entity,
}

/// Horizontal or vertical single-value slider.
pub struct Slider;
/// Two-thumb range on the same domain as [`Slider`].
pub struct Range;
/// Number field with decrease and increase buttons.
pub struct NumberField;
/// Hover and focus tooltip linked with [`DescribedBy`].
pub struct Tooltip;
/// Status toast with optional undo and auto-dismiss.
pub struct Toast;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Slider,
    Range,
    Number,
    Toast,
}

#[derive(Clone, Copy)]
struct InputKey {
    id: UiId,
    kind: Family,
}
impl Component for InputKey {}

struct InputOwned;
impl Component for InputOwned {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ThumbEnd {
    Single,
    Start,
    End,
}

#[derive(Clone, Copy)]
struct Thumb {
    root: Entity,
    end: ThumbEnd,
}
impl Component for Thumb {}

#[derive(Clone, Copy)]
struct Track {
    root: Entity,
}
impl Component for Track {}

#[derive(Clone, Copy)]
struct SliderDrag {
    thumb: Option<Entity>,
}

#[derive(Clone)]
struct SliderState {
    props: SliderProps,
    value: f64,
    requested: f64,
    label: Entity,
    track: Entity,
    fill: Entity,
    rest: Entity,
    thumb: Entity,
    face: Entity,
}
impl Component for SliderState {}

#[derive(Clone)]
struct RangeState {
    props: SliderProps,
    start: f64,
    end: f64,
    requested_start: f64,
    requested_end: f64,
    label: Entity,
    track: Entity,
    before: Entity,
    between: Entity,
    after: Entity,
    start_thumb: Entity,
    end_thumb: Entity,
    start_face: Entity,
    end_face: Entity,
}
impl Component for RangeState {}

#[derive(Clone, Copy)]
struct StepField {
    root: Entity,
}
impl Component for StepField {}

#[derive(Clone, Copy)]
struct StepButton {
    root: Entity,
    direction: i8,
}
impl Component for StepButton {}

#[derive(Clone)]
struct NumberState {
    props: NumberProps,
    value: f64,
    requested: f64,
    draft: String,
    editing: bool,
    invalid: bool,
    commit: bool,
    label: Entity,
    field: Entity,
    decrease: Entity,
    increase: Entity,
    message: Entity,
}
impl Component for NumberState {}

#[derive(Clone)]
struct Tip {
    popup: Entity,
    text: String,
    suppressed: bool,
}
impl Component for Tip {}

#[derive(Clone, Copy)]
struct ToastButton {
    toast: Entity,
    notice: ToastNotice,
}
impl Component for ToastButton {}

#[derive(Clone)]
struct ToastState {
    props: ToastProps,
    undo: Option<Entity>,
    dismiss: Entity,
    left: Duration,
    mark: Option<Duration>,
    paused: bool,
    closing: bool,
    notice: Option<ToastNotice>,
    announced: bool,
    entered: bool,
}
impl Component for ToastState {}

impl SliderHandle {
    /// Published value from the last mount.
    pub fn value(self, world: &World) -> Option<f64> {
        world.get::<SliderState>(self.root).map(|state| state.value)
    }

    /// Track that receives pointer drags.
    pub fn track(self, world: &World) -> Option<Entity> {
        world.get::<SliderState>(self.root).map(|state| state.track)
    }

    /// Visual thumb. The accessible slider is [`Self::root`].
    pub fn thumb(self, world: &World) -> Option<Entity> {
        world.get::<SliderState>(self.root).map(|state| state.thumb)
    }

    /// Despawn the slider. In-flight requests are not retracted.
    pub fn unmount(self, world: &mut World) -> Result<(), UiError> {
        unmount_tree(world, self.root)
    }
}

impl RangeHandle {
    /// Published endpoints from the last mount.
    pub fn value(self, world: &World) -> Option<(f64, f64)> {
        world
            .get::<RangeState>(self.root)
            .map(|state| (state.start, state.end))
    }

    /// Despawn the range, including both thumbs.
    pub fn unmount(self, world: &mut World) -> Result<(), UiError> {
        unmount_tree(world, self.root)
    }
}

impl NumberHandle {
    /// Published number from the last mount.
    pub fn value(self, world: &World) -> Option<f64> {
        world.get::<NumberState>(self.root).map(|state| state.value)
    }

    /// Text currently shown in the field, including an uncommitted draft.
    pub fn draft(self, world: &World) -> Option<String> {
        world
            .get::<NumberState>(self.root)
            .map(|state| state.draft.clone())
    }

    /// Despawn the field and its buttons.
    pub fn unmount(self, world: &mut World) -> Result<(), UiError> {
        unmount_tree(world, self.root)
    }
}

impl TooltipHandle {
    /// Remove the tooltip and its described-by link.
    pub fn detach(self, world: &mut World) -> Result<(), UiError> {
        if !world
            .get::<Tip>(self.target)
            .is_some_and(|tip| tip.popup == self.root)
        {
            return Err(UiError::Stale);
        }
        unlink(world, self.target, self.root);
        world.remove::<Tip>(self.target);
        if world.is_alive(self.root) {
            unmount_tree(world, self.root)?;
        }
        Ok(())
    }
}

impl ToastHandle {
    /// Close the toast. Sends [`ToastNotice::Dismissed`] once.
    pub fn dismiss(self, world: &mut World) -> Result<(), UiError> {
        if !world.has::<ToastState>(self.root) {
            return Err(UiError::Stale);
        }
        begin_close(world, self.root, ToastNotice::Dismissed);
        Ok(())
    }

    /// Despawn the toast without a further notice.
    pub fn unmount(self, world: &mut World) -> Result<(), UiError> {
        unmount_tree(world, self.root)
    }
}

impl Slider {
    /// Mount or update a slider. `value` is clamped and snapped.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        props: SliderProps,
        value: f64,
    ) -> Result<SliderHandle, UiError> {
        require_layout_parent(world, parent)?;
        let value = admit_slider(&props, value)?;
        if let Some(root) = find_keyed_child(world, parent, id) {
            expect_kind(world, root, Family::Slider)?;
            let state = world
                .get::<SliderState>(root)
                .cloned()
                .ok_or(UiError::IdentityConflict)?;
            let mut state = state;
            if state.props.vertical != props.vertical {
                return Err(UiError::InvalidProps);
            }
            state.props = props;
            state.value = value;
            state.requested = value;
            world.insert(root, state);
            Ok(SliderHandle { root })
        } else {
            let root = column(world, Some(parent));
            world.insert(
                root,
                InputKey {
                    id,
                    kind: Family::Slider,
                },
            );
            let label = text(world, root, &props.label, Typography::Label)?;
            let horizontal = !props.vertical;
            let track = row_track(world, root, horizontal);
            // Horizontal grows left to right. Vertical grows bottom to top, so
            // the open portion is the first child.
            let (fill, thumb, face, rest) = if horizontal {
                let fill = bar(world, track);
                let (thumb, face) = thumb(world, track);
                let rest = bar(world, track);
                (fill, thumb, face, rest)
            } else {
                let rest = bar(world, track);
                let (thumb, face) = thumb(world, track);
                let fill = bar(world, track);
                (fill, thumb, face, rest)
            };
            world.insert(track, Track { root });
            world.insert(
                root,
                Thumb {
                    root,
                    end: ThumbEnd::Single,
                },
            );
            world.insert(root, Focusable);
            world.insert(root, KeyNavigation(slider_key));
            world.insert(root, AccessMark(AccessRole::Slider));
            world.insert(
                root,
                SliderState {
                    props,
                    value,
                    requested: value,
                    label,
                    track,
                    fill,
                    rest,
                    thumb,
                    face,
                },
            );
            Ok(SliderHandle { root })
        }
    }
}

impl Range {
    /// Mount or update a range. Endpoints are ordered and clamped.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        props: SliderProps,
        start: f64,
        end: f64,
    ) -> Result<RangeHandle, UiError> {
        require_layout_parent(world, parent)?;
        let (start, end) = admit_range(&props, start, end)?;
        if let Some(root) = find_keyed_child(world, parent, id) {
            expect_kind(world, root, Family::Range)?;
            let mut state = world
                .get::<RangeState>(root)
                .cloned()
                .ok_or(UiError::IdentityConflict)?;
            if state.props.vertical != props.vertical {
                return Err(UiError::InvalidProps);
            }
            state.props = props;
            state.start = start;
            state.end = end;
            state.requested_start = start;
            state.requested_end = end;
            let start_thumb = state.start_thumb;
            let end_thumb = state.end_thumb;
            world.insert(root, state);
            Ok(RangeHandle {
                root,
                start: start_thumb,
                end: end_thumb,
            })
        } else {
            let root = column(world, Some(parent));
            world.insert(
                root,
                InputKey {
                    id,
                    kind: Family::Range,
                },
            );
            world.insert(root, AccessMark(AccessRole::Group));
            let label = text(world, root, &props.label, Typography::Label)?;
            let horizontal = !props.vertical;
            let track = row_track(world, root, horizontal);
            let (before, start_thumb, start_face, between, end_thumb, end_face, after) =
                if horizontal {
                    let before = bar(world, track);
                    let (start_thumb, start_face) = thumb(world, track);
                    let between = bar(world, track);
                    let (end_thumb, end_face) = thumb(world, track);
                    let after = bar(world, track);
                    (
                        before,
                        start_thumb,
                        start_face,
                        between,
                        end_thumb,
                        end_face,
                        after,
                    )
                } else {
                    let after = bar(world, track);
                    let (end_thumb, end_face) = thumb(world, track);
                    let between = bar(world, track);
                    let (start_thumb, start_face) = thumb(world, track);
                    let before = bar(world, track);
                    (
                        before,
                        start_thumb,
                        start_face,
                        between,
                        end_thumb,
                        end_face,
                        after,
                    )
                };
            world.insert(track, Track { root });
            for (entity, end) in [(start_thumb, ThumbEnd::Start), (end_thumb, ThumbEnd::End)] {
                world.insert(entity, Thumb { root, end });
                world.insert(entity, Focusable);
                world.insert(entity, KeyNavigation(slider_key));
                world.insert(entity, AccessMark(AccessRole::Slider));
            }
            world.insert(
                root,
                RangeState {
                    props,
                    start,
                    end,
                    requested_start: start,
                    requested_end: end,
                    label,
                    track,
                    before,
                    between,
                    after,
                    start_thumb,
                    end_thumb,
                    start_face,
                    end_face,
                },
            );
            Ok(RangeHandle {
                root,
                start: start_thumb,
                end: end_thumb,
            })
        }
    }
}

impl NumberField {
    /// Mount or update a stepper. `value` is clamped and snapped.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        props: NumberProps,
        value: f64,
    ) -> Result<NumberHandle, UiError> {
        require_layout_parent(world, parent)?;
        let value = admit_number(&props, value)?;
        if let Some(root) = find_keyed_child(world, parent, id) {
            expect_kind(world, root, Family::Number)?;
            let mut state = world
                .get::<NumberState>(root)
                .cloned()
                .ok_or(UiError::IdentityConflict)?;
            let focused =
                world.resource::<Focus>().and_then(|focus| focus.entity) == Some(state.field);
            let same = (value - state.value).abs() <= 1e-9;
            state.props = props;
            if !same || !state.editing || !focused {
                state.draft = NumericAccess::text(value);
                state.requested = value;
                state.invalid = false;
                state.editing = false;
                let field = state.field;
                let draft = state.draft.clone();
                replace_field_text(world, field, &draft);
            }
            state.value = value;
            let field = state.field;
            world.insert(root, state);
            Ok(NumberHandle { root, field })
        } else {
            let root = column(world, Some(parent));
            world.insert(
                root,
                InputKey {
                    id,
                    kind: Family::Number,
                },
            );
            world.insert(root, AccessMark(AccessRole::Group));
            let label = text(world, root, &props.label, Typography::Label)?;
            let row = world.spawn((Layout {
                width: Size::Fill,
                gap: 4.0,
                ..Layout::row()
            },));
            own(world, row);
            set_parent(world, row, root);
            let decrease = step_button(world, row, "−");
            let field = world.spawn((
                Layout {
                    width: Size::Fill,
                    min_height: 32.0,
                    padding: 8.0,
                    ..Layout::column()
                },
                Label::new(NumericAccess::text(value)),
                TextField,
                CapturesKeys,
                Focusable,
                AccessMark(AccessRole::SpinButton),
            ));
            world.insert(field, KeyNavigation(number_key));
            world.insert(field, FocusBoundary(number_blur));
            own(world, field);
            set_parent(world, field, row);
            world.insert(field, StepField { root });
            let increase = step_button(world, row, "+");
            world.insert(
                decrease,
                StepButton {
                    root,
                    direction: -1,
                },
            );
            world.insert(increase, StepButton { root, direction: 1 });
            let message = text(world, root, "", Typography::Secondary)?;
            world.insert(message, Hidden);
            world.insert(
                root,
                NumberState {
                    props,
                    value,
                    requested: value,
                    draft: NumericAccess::text(value),
                    editing: false,
                    invalid: false,
                    commit: false,
                    label,
                    field,
                    decrease,
                    increase,
                    message,
                },
            );
            Ok(NumberHandle { root, field })
        }
    }
}

impl Tooltip {
    /// Attach `text` to `target`. A second attach updates the same tooltip.
    ///
    /// The tooltip is shown for pointer hover and keyboard focus, hidden on
    /// blur, pointer leave, and Escape. It does not move focus. While it is
    /// visible, [`DescribedBy`] on `target` includes it; hidden targets are
    /// omitted by the accessibility bridge.
    pub fn attach(world: &mut World, target: Entity, text: &str) -> Result<TooltipHandle, UiError> {
        if !world.is_alive(target) {
            return Err(UiError::Stale);
        }
        if text.trim().is_empty() {
            return Err(UiError::InvalidProps);
        }
        if let Some(tip) = world.get::<Tip>(target).cloned() {
            if let Some(label) = world.get_mut::<Label>(tip.popup) {
                label.text = text.to_string();
            }
            if let Some(semantics) = world.get_mut::<Semantics>(tip.popup) {
                semantics.name = Some(text.to_string());
            }
            world.get_mut::<Tip>(target).unwrap().text = text.to_string();
            return Ok(TooltipHandle {
                root: tip.popup,
                target,
            });
        }
        let popup = world.spawn((
            Layout {
                width: Size::Hug,
                padding: 8.0,
                ..Layout::column()
            },
            Label::new(text).overflow(TextOverflow::Wrap),
            Hidden,
            PassivePopup,
            Layer(12),
            AccessMark(AccessRole::Tooltip),
        ));
        world.insert(
            popup,
            Semantics {
                name: Some(text.to_string()),
                ..Semantics::default()
            },
        );
        world.insert(
            popup,
            AnchoredPopup {
                anchor: target,
                max_height: 160.0,
                closed: |_, _| {},
            },
        );
        world.insert(popup, PopupPlacement::Beside);
        own(world, popup);
        world.insert(
            target,
            Tip {
                popup,
                text: text.to_string(),
                suppressed: false,
            },
        );
        link(world, target, popup);
        Ok(TooltipHandle {
            root: popup,
            target,
        })
    }
}

impl Toast {
    /// Mount or update a toast under `parent`. The parent is the host's region;
    /// this toolkit does not introduce a separate overlay stack.
    ///
    /// Hovering or focusing the toast pauses auto-dismiss. Reduced motion shows
    /// and hides it without a fade.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        props: ToastProps,
    ) -> Result<ToastHandle, UiError> {
        require_layout_parent(world, parent)?;
        if props.message.trim().is_empty()
            || props
                .undo
                .as_ref()
                .is_some_and(|undo| undo.trim().is_empty())
        {
            return Err(UiError::InvalidProps);
        }
        if let Some(root) = find_keyed_child(world, parent, id) {
            expect_kind(world, root, Family::Toast)?;
            let mut state = world
                .get::<ToastState>(root)
                .cloned()
                .ok_or(UiError::IdentityConflict)?;
            let message_changed = state.props.message != props.message;
            state.props = props.clone();
            if message_changed {
                state.closing = false;
                state.announced = false;
                state.notice = None;
                state.entered = false;
                state.paused = false;
                state.mark = None;
                state.left = props.duration;
                world.remove::<Hidden>(root);
            }
            sync_toast_actions(world, root, &mut state)?;
            world.insert(root, state);
            if let Some(label) = world.get_mut::<Label>(root) {
                label.text = props.message.clone();
            }
            if let Some(semantics) = world.get_mut::<Semantics>(root) {
                semantics.name = Some(props.message);
            }
            Ok(ToastHandle { root })
        } else {
            let root = world.spawn((
                Layout {
                    width: Size::Fill,
                    padding: 12.0,
                    gap: 8.0,
                    ..Layout::column()
                },
                Label::new(&props.message).overflow(TextOverflow::Wrap),
                AccessMark(AccessRole::Status),
                LiveRegion(LivePoliteness::Polite),
                Semantics {
                    name: Some(props.message.clone()),
                    ..Semantics::default()
                },
                InputKey {
                    id,
                    kind: Family::Toast,
                },
            ));
            own(world, root);
            set_parent(world, root, parent);
            let dismiss = toast_button(world, root, "Dismiss", ToastNotice::Dismissed);
            let undo = props
                .undo
                .as_ref()
                .map(|label| toast_button(world, root, label, ToastNotice::Undo));
            world.insert(
                root,
                ToastState {
                    props: props.clone(),
                    undo,
                    dismiss,
                    left: props.duration,
                    mark: None,
                    paused: false,
                    closing: false,
                    notice: None,
                    announced: false,
                    entered: false,
                },
            );
            Ok(ToastHandle { root })
        }
    }
}

fn expect_kind(world: &World, root: Entity, kind: Family) -> Result<(), UiError> {
    if world
        .get::<InputKey>(root)
        .is_some_and(|key| key.kind == kind)
    {
        Ok(())
    } else {
        Err(UiError::IdentityConflict)
    }
}

/// Identity of a mounted slider, range, stepper, or toast root.
pub(crate) fn key_at(world: &World, entity: Entity) -> Option<UiId> {
    world.get::<InputKey>(entity).map(|key| key.id)
}

pub(crate) fn owns_entity(world: &World, entity: Entity) -> bool {
    world.has::<InputOwned>(entity)
}

/// Pointer drags, assistive set-value, tooltips, stepper commits, and toast
/// timers. Runs in Input after the text field has applied this frame's keys,
/// so the requests are visible to Update.
pub(crate) fn drive(world: &mut World) {
    drive_access(world);
    drive_pointer(world);
    drive_tooltips(world);
    let pending: Vec<Entity> = world
        .entities_with::<NumberState>()
        .into_iter()
        .filter(|root| {
            world
                .get::<NumberState>(*root)
                .is_some_and(|state| state.commit)
        })
        .collect();
    for root in pending {
        if let Some(state) = world.get_mut::<NumberState>(root) {
            state.commit = false;
        }
        commit_number(world, root);
    }
    drive_toasts(world);
}

/// Records an in-progress stepper draft after this frame's field edits.
/// Runs in Update, before a host system registered after [`crate::UiPlugin`].
pub(crate) fn publish(world: &mut World) {
    drive_drafts(world);
}

/// Theme, focus, and thumb geometry. Runs in Layout before style resolution.
pub(crate) fn styles(world: &mut World) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    style_sliders(world, &theme);
    style_ranges(world, &theme);
    style_numbers(world, &theme);
    style_toasts(world, &theme);
    style_tips(world, &theme);
}

fn admit_slider(props: &SliderProps, value: f64) -> Result<f64, UiError> {
    if props.label.trim().is_empty() || !props.range.valid() || !value.is_finite() {
        return Err(UiError::InvalidProps);
    }
    props
        .range
        .access(value, !props.vertical)
        .normalize(value)
        .ok_or(UiError::InvalidProps)
}

fn admit_range(props: &SliderProps, start: f64, end: f64) -> Result<(f64, f64), UiError> {
    let start = admit_slider(props, start)?;
    let end = admit_slider(props, end)?;
    if start <= end {
        Ok((start, end))
    } else {
        Ok((end, start))
    }
}

fn admit_number(props: &NumberProps, value: f64) -> Result<f64, UiError> {
    if props.label.trim().is_empty() || !props.range.valid() || !value.is_finite() {
        return Err(UiError::InvalidProps);
    }
    props
        .range
        .access(value, true)
        .normalize(value)
        .ok_or(UiError::InvalidProps)
}

fn own(world: &mut World, entity: Entity) {
    world.insert(entity, InputOwned);
}

fn column(world: &mut World, parent: Option<Entity>) -> Entity {
    let entity = world.spawn((Layout {
        width: Size::Fill,
        gap: 4.0,
        ..Layout::column()
    },));
    own(world, entity);
    if let Some(parent) = parent {
        set_parent(world, entity, parent);
    }
    entity
}

fn row_track(world: &mut World, parent: Entity, horizontal: bool) -> Entity {
    let entity = world.spawn((Layout {
        width: if horizontal {
            Size::Fill
        } else {
            Size::Px(32.0)
        },
        height: if horizontal {
            Size::Px(32.0)
        } else {
            Size::Px(160.0)
        },
        min_width: if horizontal { 120.0 } else { 32.0 },
        min_height: if horizontal { 32.0 } else { 120.0 },
        ..if horizontal {
            Layout::row()
        } else {
            Layout::column()
        }
    },));
    own(world, entity);
    set_parent(world, entity, parent);
    entity
}

fn bar(world: &mut World, parent: Entity) -> Entity {
    let entity = world.spawn((Layout {
        width: Size::Px(0.0),
        height: Size::Px(4.0),
        ..Layout::column()
    },));
    own(world, entity);
    set_parent(world, entity, parent);
    entity
}

fn thumb(world: &mut World, parent: Entity) -> (Entity, Entity) {
    let entity = world.spawn((Layout {
        width: Size::Px(20.0),
        height: Size::Px(20.0),
        padding: 2.0,
        ..Layout::column()
    },));
    own(world, entity);
    set_parent(world, entity, parent);
    let face = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    own(world, face);
    set_parent(world, face, entity);
    (entity, face)
}

fn step_button(world: &mut World, parent: Entity, glyph: &str) -> Entity {
    let entity = world.spawn((
        Layout {
            width: Size::Px(32.0),
            min_height: 32.0,
            padding: 4.0,
            ..Layout::column()
        },
        Label::new(glyph),
        Button::new(format!("step-{glyph}")),
        ReleaseActivation,
        ActivationHandler(step_from_button),
        Focusable,
    ));
    own(world, entity);
    set_parent(world, entity, parent);
    entity
}

fn sync_toast_actions(
    world: &mut World,
    root: Entity,
    state: &mut ToastState,
) -> Result<(), UiError> {
    match (&state.props.undo, state.undo) {
        (Some(label), Some(entity)) => {
            if let Some(text) = world.get_mut::<Label>(entity) {
                text.text = label.clone();
            }
            if let Some(semantics) = world.get_mut::<Semantics>(entity) {
                semantics.name = Some(label.clone());
            }
        }
        (Some(label), None) => {
            let entity = toast_button(world, root, label, ToastNotice::Undo);
            state.undo = Some(entity);
        }
        (None, Some(entity)) => {
            if world.is_alive(entity) {
                forget_interaction_targets(world, &[entity]);
                despawn(world, entity);
            }
            state.undo = None;
        }
        (None, None) => {}
    }
    if !world.is_alive(state.dismiss) {
        state.dismiss = toast_button(world, root, "Dismiss", ToastNotice::Dismissed);
    }
    Ok(())
}

fn toast_button(world: &mut World, toast: Entity, label: &str, notice: ToastNotice) -> Entity {
    let entity = world.spawn((
        Layout {
            width: Size::Hug,
            min_height: 32.0,
            padding: 8.0,
            ..Layout::row()
        },
        Label::new(label),
        Button::new(format!("toast-{label}")),
        ReleaseActivation,
        ActivationHandler(toast_activate),
        Focusable,
    ));
    world.insert(entity, KeyNavigation(toast_key));
    world.insert(
        entity,
        Semantics {
            name: Some(label.to_string()),
            ..Semantics::default()
        },
    );
    world.insert(entity, ToastButton { toast, notice });
    own(world, entity);
    set_parent(world, entity, toast);
    entity
}

fn unmount_tree(world: &mut World, root: Entity) -> Result<(), UiError> {
    if !world.is_alive(root) {
        return Err(UiError::Stale);
    }
    let mut all = Vec::new();
    collect(world, root, &mut all);
    forget_interaction_targets(world, &all);
    despawn(world, root);
    Ok(())
}

fn collect(world: &World, entity: Entity, out: &mut Vec<Entity>) {
    out.push(entity);
    for child in children_of(world, entity) {
        collect(world, child, out);
    }
}

fn link(world: &mut World, target: Entity, popup: Entity) {
    let mut ids = world
        .get::<DescribedBy>(target)
        .map(|described| described.0.clone())
        .unwrap_or_default();
    ids.retain(|entity| world.is_alive(*entity));
    if !ids.contains(&popup) {
        ids.push(popup);
    }
    world.insert(target, DescribedBy(ids));
}

fn unlink(world: &mut World, target: Entity, popup: Entity) {
    if !world.is_alive(target) {
        return;
    }
    let Some(described) = world.get::<DescribedBy>(target).cloned() else {
        return;
    };
    let ids: Vec<_> = described
        .0
        .into_iter()
        .filter(|entity| *entity != popup)
        .collect();
    if ids.is_empty() {
        world.remove::<DescribedBy>(target);
    } else {
        world.insert(target, DescribedBy(ids));
    }
}

fn reduced_motion(world: &World) -> bool {
    world
        .resource::<MotionSettings>()
        .is_some_and(|settings| settings.reduced_motion)
        || system_appearance(world).reduced_motion
}

fn editable(world: &World, entity: Entity) -> bool {
    let details = access_details(world, entity);
    details.focus && !details.readonly
}

fn nearly(left: f64, right: f64) -> bool {
    (left - right).abs() <= 1e-9
}

fn slider_key(world: &mut World, entity: Entity, event: &KeyEvent) -> bool {
    if event.released
        || event.synthetic
        || event.modifiers.ctrl
        || event.modifiers.super_key
        || event.modifiers.alt
    {
        return false;
    }
    let Some(thumb) = world.get::<Thumb>(entity).copied() else {
        return false;
    };
    let page = event.modifiers.shift;
    let adjust = match event.logical {
        LogicalKey::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => {
            if page {
                Adjust::Page(1.0)
            } else {
                Adjust::Steps(1.0)
            }
        }
        LogicalKey::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => {
            if page {
                Adjust::Page(-1.0)
            } else {
                Adjust::Steps(-1.0)
            }
        }
        LogicalKey::Named(NamedKey::PageUp) => Adjust::Page(1.0),
        LogicalKey::Named(NamedKey::PageDown) => Adjust::Page(-1.0),
        LogicalKey::Named(NamedKey::Home) => Adjust::Endpoint(false),
        LogicalKey::Named(NamedKey::End) => Adjust::Endpoint(true),
        _ => return false,
    };
    nudge(world, thumb, adjust);
    true
}

enum Adjust {
    Steps(f64),
    Page(f64),
    Endpoint(bool),
    Absolute(f64),
}

fn nudge(world: &mut World, thumb: Thumb, adjust: Adjust) {
    if !editable(world, thumb_entity(world, &thumb)) {
        return;
    }
    if world.has::<SliderState>(thumb.root) {
        nudge_slider(world, thumb.root, adjust);
    } else {
        nudge_range(world, thumb, adjust);
    }
}

fn thumb_entity(world: &World, thumb: &Thumb) -> Entity {
    if thumb.end == ThumbEnd::Single {
        thumb.root
    } else if let Some(state) = world.get::<RangeState>(thumb.root) {
        if thumb.end == ThumbEnd::Start {
            state.start_thumb
        } else {
            state.end_thumb
        }
    } else {
        thumb.root
    }
}

fn nudge_slider(world: &mut World, root: Entity, adjust: Adjust) {
    let Some(state) = world.get::<SliderState>(root).cloned() else {
        return;
    };
    if !state.props.enabled || state.props.readonly {
        return;
    }
    let access = state
        .props
        .range
        .access(state.requested, !state.props.vertical);
    let next = match adjust {
        Adjust::Steps(steps) => access.normalize(state.requested + steps * state.props.range.step),
        Adjust::Page(sign) => access.normalize(state.requested + sign * state.props.range.page),
        Adjust::Endpoint(max) => Some(if max {
            state.props.range.max
        } else {
            state.props.range.min
        }),
        Adjust::Absolute(value) => access.normalize(value),
    };
    let Some(next) = next else {
        return;
    };
    if nearly(next, state.requested) {
        return;
    }
    world.get_mut::<SliderState>(root).unwrap().requested = next;
    send(
        world,
        SliderChange {
            control: root,
            value: next,
        },
    );
}

fn nudge_range(world: &mut World, thumb: Thumb, adjust: Adjust) {
    let Some(state) = world.get::<RangeState>(thumb.root).cloned() else {
        return;
    };
    if !state.props.enabled || state.props.readonly {
        return;
    }
    let start = thumb.end == ThumbEnd::Start;
    let current = if start {
        state.requested_start
    } else {
        state.requested_end
    };
    let access = state.props.range.access(current, !state.props.vertical);
    let next = match adjust {
        Adjust::Steps(steps) => access.normalize(current + steps * state.props.range.step),
        Adjust::Page(sign) => access.normalize(current + sign * state.props.range.page),
        Adjust::Endpoint(max) => {
            if start {
                Some(if max {
                    state.requested_end
                } else {
                    state.props.range.min
                })
            } else if max {
                Some(state.props.range.max)
            } else {
                Some(state.requested_start)
            }
        }
        Adjust::Absolute(value) => access.normalize(value),
    };
    let Some(mut next) = next else {
        return;
    };
    if start {
        next = next.min(state.requested_end);
    } else {
        next = next.max(state.requested_start);
    }
    if nearly(next, current) {
        return;
    }
    let (requested_start, requested_end) = if start {
        (next, state.requested_end)
    } else {
        (state.requested_start, next)
    };
    if let Some(state) = world.get_mut::<RangeState>(thumb.root) {
        state.requested_start = requested_start;
        state.requested_end = requested_end;
    }
    send(
        world,
        RangeChange {
            control: thumb.root,
            start: requested_start,
            end: requested_end,
        },
    );
}

fn number_key(world: &mut World, entity: Entity, event: &KeyEvent) -> bool {
    if event.released
        || event.synthetic
        || event.modifiers.ctrl
        || event.modifiers.super_key
        || event.modifiers.alt
    {
        return false;
    }
    let Some(field) = world.get::<StepField>(entity).copied() else {
        return false;
    };
    let page = event.modifiers.shift;
    match event.logical {
        LogicalKey::Named(NamedKey::ArrowUp) => step_number(
            world,
            field.root,
            if page { None } else { Some(1.0) },
            if page { 1.0 } else { 0.0 },
        ),
        LogicalKey::Named(NamedKey::ArrowDown) => step_number(
            world,
            field.root,
            if page { None } else { Some(-1.0) },
            if page { -1.0 } else { 0.0 },
        ),
        LogicalKey::Named(NamedKey::PageUp) => step_number(world, field.root, None, 1.0),
        LogicalKey::Named(NamedKey::PageDown) => step_number(world, field.root, None, -1.0),
        LogicalKey::Named(NamedKey::Home) => set_number_endpoint(world, field.root, false),
        LogicalKey::Named(NamedKey::End) => set_number_endpoint(world, field.root, true),
        LogicalKey::Named(NamedKey::Enter) => {
            if let Some(state) = world.get_mut::<NumberState>(field.root) {
                state.commit = true;
            }
            true
        }
        LogicalKey::Named(NamedKey::Escape) => revert_number(world, field.root),
        _ => false,
    }
}

fn step_from_button(world: &mut World, entity: Entity) {
    let Some(button) = world.get::<StepButton>(entity).copied() else {
        return;
    };
    step_number(world, button.root, Some(f64::from(button.direction)), 0.0);
}

fn step_number(world: &mut World, root: Entity, steps: Option<f64>, pages: f64) -> bool {
    let Some(state) = world.get::<NumberState>(root).cloned() else {
        return false;
    };
    if !state.props.enabled || state.props.readonly {
        return true;
    }
    let shown = field_text(world, state.field).unwrap_or(state.draft);
    let base = parse_number(&shown)
        .and_then(|value| state.props.range.access(value, true).normalize(value))
        .unwrap_or(state.requested);
    let delta = steps.map_or(pages * state.props.range.page, |steps| {
        steps * state.props.range.step
    });
    let Some(next) = state.props.range.access(base, true).normalize(base + delta) else {
        return true;
    };
    write_number(world, root, next);
    true
}

fn set_number_endpoint(world: &mut World, root: Entity, max: bool) -> bool {
    let Some(state) = world.get::<NumberState>(root).cloned() else {
        return false;
    };
    if !state.props.enabled || state.props.readonly {
        return true;
    }
    let next = if max {
        state.props.range.max
    } else {
        state.props.range.min
    };
    write_number(world, root, next);
    true
}

fn write_number(world: &mut World, root: Entity, next: f64) {
    let Some(mut state) = world.get::<NumberState>(root).cloned() else {
        return;
    };
    let text = NumericAccess::text(next);
    state.draft = text.clone();
    state.editing = false;
    state.invalid = false;
    let field = state.field;
    let changed = !nearly(next, state.requested);
    state.requested = next;
    world.insert(root, state);
    replace_field_text(world, field, &text);
    if changed {
        send(
            world,
            NumberChange {
                control: root,
                value: next,
            },
        );
    }
}

fn commit_number(world: &mut World, root: Entity) {
    let Some(state) = world.get::<NumberState>(root).cloned() else {
        return;
    };
    if !state.props.enabled || state.props.readonly {
        return;
    }
    let draft = field_text(world, state.field).unwrap_or_else(|| state.draft.clone());
    if let Some(value) = parse_number(&draft) {
        if let Some(next) = state.props.range.access(value, true).normalize(value) {
            write_number(world, root, next);
            return;
        }
    }
    let revert = NumericAccess::text(state.requested);
    if draft.trim() != revert {
        send(
            world,
            NumberRejected {
                control: root,
                draft,
            },
        );
    }
    let field = world.get::<NumberState>(root).map(|state| state.field);
    if let Some(state) = world.get_mut::<NumberState>(root) {
        state.draft = revert.clone();
        state.editing = false;
        state.invalid = true;
    }
    if let Some(field) = field {
        replace_field_text(world, field, &revert);
    }
}

fn revert_number(world: &mut World, root: Entity) -> bool {
    let Some(state) = world.get::<NumberState>(root).cloned() else {
        return false;
    };
    let canonical = NumericAccess::text(state.requested);
    let shown = field_text(world, state.field).unwrap_or_else(|| state.draft.clone());
    if shown.trim() == canonical {
        return false;
    }
    let field = world.get::<NumberState>(root).map(|state| state.field);
    if let Some(state) = world.get_mut::<NumberState>(root) {
        state.draft = canonical.clone();
        state.editing = false;
        state.invalid = false;
    }
    if let Some(field) = field {
        replace_field_text(world, field, &canonical);
        suppress_tooltip(world, field);
    }
    true
}

fn number_blur(world: &mut World, entity: Entity, _next: Option<Entity>) {
    if let Some(field) = world.get::<StepField>(entity).copied() {
        commit_number(world, field.root);
    }
}

fn replace_field_text(world: &mut World, field: Entity, text: &str) {
    rndr_core::replace_field_text(world, field, text);
}

fn field_text(world: &World, field: Entity) -> Option<String> {
    world.get::<Label>(field).map(|label| label.text.clone())
}

fn parse_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() || partial_number(text) {
        return None;
    }
    let value: f64 = text.parse().ok()?;
    value.is_finite().then_some(value)
}

fn partial_number(text: &str) -> bool {
    let text = text.trim();
    if matches!(text, "" | "+" | "-" | "." | "+." | "-.") {
        return true;
    }
    let lower = text.to_ascii_lowercase();
    lower.ends_with('e') || lower.ends_with("e+") || lower.ends_with("e-") || lower.ends_with('.')
}

fn toast_activate(world: &mut World, entity: Entity) {
    let Some(button) = world.get::<ToastButton>(entity).copied() else {
        return;
    };
    begin_close(world, button.toast, button.notice);
}

fn toast_key(world: &mut World, entity: Entity, event: &KeyEvent) -> bool {
    if event.released
        || event.synthetic
        || event.repeat
        || event.modifiers.ctrl
        || event.modifiers.alt
        || event.modifiers.super_key
        || event.logical != LogicalKey::Named(NamedKey::Escape)
    {
        return false;
    }
    let Some(button) = world.get::<ToastButton>(entity).copied() else {
        return false;
    };
    begin_close(world, button.toast, ToastNotice::Dismissed);
    true
}

fn begin_close(world: &mut World, root: Entity, notice: ToastNotice) {
    let Some(state) = world.get::<ToastState>(root) else {
        return;
    };
    if state.announced {
        return;
    }
    if let Some(state) = world.get_mut::<ToastState>(root) {
        state.closing = true;
        state.notice = Some(notice);
        state.announced = true;
    }
    send(
        world,
        ToastEvent {
            toast: root,
            notice,
        },
    );
    if reduced_motion(world) {
        world.insert(root, Hidden);
        world.insert(root, Opacity(1.0));
    } else {
        motion::animate(
            world,
            root,
            Prop::Opacity,
            0.0,
            Curve::tween(0.16, Easing::EaseOut),
        );
    }
}

fn drive_access(world: &mut World) {
    let sets: Vec<NumericAccessSet> = events::<NumericAccessSet>(world).to_vec();
    for set in sets {
        if let Some(thumb) = world.get::<Thumb>(set.entity).copied() {
            nudge(world, thumb, Adjust::Absolute(set.value));
        } else if let Some(field) = world.get::<StepField>(set.entity).copied() {
            if let Some(state) = world.get::<NumberState>(field.root) {
                if state.props.enabled && !state.props.readonly {
                    write_number(world, field.root, set.value);
                }
            }
        }
    }
}

fn drive_pointer(world: &mut World) {
    let Some(pointer) = world.resource::<Input>().map(|input| input.pointer) else {
        return;
    };
    if pointer.pressed && pointer.button == PointerButton::Left {
        if let Some(hit) = hit_test(world, pointer.position[0], pointer.position[1]) {
            if let Some(thumb) = thumb_for_hit(world, hit, pointer.position) {
                let focus = thumb_entity(world, &thumb);
                world.insert_resource(SliderDrag { thumb: Some(focus) });
                let _ = request_focus(world, focus);
                if let Some(value) = value_at(world, thumb.root, pointer.position) {
                    nudge(world, thumb, Adjust::Absolute(value));
                }
            }
        }
    }
    if pointer.down {
        if let Some(drag) = world.resource::<SliderDrag>().and_then(|drag| drag.thumb) {
            if let Some(thumb) = world.get::<Thumb>(drag).copied() {
                if let Some(value) = value_at(world, thumb.root, pointer.position) {
                    nudge(world, thumb, Adjust::Absolute(value));
                }
            }
        }
    } else if let Some(drag) = world.resource_mut::<SliderDrag>() {
        drag.thumb = None;
    }
}

fn thumb_for_hit(world: &World, mut hit: Entity, at: [f32; 2]) -> Option<Thumb> {
    for _ in 0..32 {
        if world.has::<Button>(hit) && !world.has::<Thumb>(hit) {
            return None;
        }
        if let Some(thumb) = world.get::<Thumb>(hit).copied() {
            return Some(thumb);
        }
        if let Some(track) = world.get::<Track>(hit).copied() {
            return thumb_for_track(world, track.root, at);
        }
        hit = world.get::<Parent>(hit)?.entity;
    }
    None
}

fn thumb_for_track(world: &World, root: Entity, at: [f32; 2]) -> Option<Thumb> {
    if world.has::<SliderState>(root) {
        return Some(Thumb {
            root,
            end: ThumbEnd::Single,
        });
    }
    let state = world.get::<RangeState>(root)?;
    let value = value_at(world, root, at)?;
    let start_distance = (value - state.requested_start).abs();
    let end_distance = (value - state.requested_end).abs();
    Some(Thumb {
        root,
        end: if start_distance <= end_distance {
            ThumbEnd::Start
        } else {
            ThumbEnd::End
        },
    })
}

fn value_at(world: &World, root: Entity, at: [f32; 2]) -> Option<f64> {
    let (track, range, vertical) = if let Some(state) = world.get::<SliderState>(root) {
        (state.track, state.props.range, state.props.vertical)
    } else {
        let state = world.get::<RangeState>(root)?;
        (state.track, state.props.range, state.props.vertical)
    };
    let rect = world.get::<Rect>(track).copied()?;
    let (origin, span, cursor) = if vertical {
        (rect.y, rect.h, at[1])
    } else {
        (rect.x, rect.w, at[0])
    };
    if span <= f32::EPSILON {
        return Some(range.min);
    }
    let mut t = ((cursor - origin) / span).clamp(0.0, 1.0) as f64;
    if vertical {
        t = 1.0 - t;
    }
    Some(range.min + t * (range.max - range.min))
}

fn drive_tooltips(world: &mut World) {
    let escape = world.resource::<Input>().is_some_and(|input| {
        input.keys.iter().any(|event| {
            event.logical == LogicalKey::Named(NamedKey::Escape)
                && !event.released
                && !event.synthetic
                && !event.modifiers.ctrl
                && !event.modifiers.alt
                && !event.modifiers.super_key
        })
    });
    let pointer = world
        .resource::<Input>()
        .map(|input| input.pointer.position)
        .unwrap_or([0.0, 0.0]);
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let targets: Vec<Entity> = world.entities_with::<Tip>();
    for target in targets {
        let Some(mut tip) = world.get::<Tip>(target).cloned() else {
            continue;
        };
        if !world.is_alive(target) || !world.is_alive(tip.popup) {
            unlink(world, target, tip.popup);
            world.remove::<Tip>(target);
            continue;
        }
        let hovered = covers(world, target, pointer) || covers(world, tip.popup, pointer);
        let focused = focus.is_some_and(|entity| within(world, target, entity));
        if !hovered && !focused {
            tip.suppressed = false;
        } else if escape {
            tip.suppressed = true;
        }
        let show = (hovered || focused) && !tip.suppressed;
        if show {
            world.remove::<Hidden>(tip.popup);
        } else {
            world.insert(tip.popup, Hidden);
        }
        if let Some(label) = world.get_mut::<Label>(tip.popup) {
            if label.text != tip.text {
                label.text = tip.text.clone();
            }
        }
        world.insert(target, tip);
        link(world, target, world.get::<Tip>(target).unwrap().popup);
    }
}

fn suppress_tooltip(world: &mut World, target: Entity) {
    let Some(mut tip) = world.get::<Tip>(target).cloned() else {
        return;
    };
    tip.suppressed = true;
    world.insert(tip.popup, Hidden);
    world.insert(target, tip);
}

fn covers(world: &World, root: Entity, at: [f32; 2]) -> bool {
    let Some(hit) = hit_test(world, at[0], at[1]) else {
        return false;
    };
    within(world, root, hit)
}

fn within(world: &World, root: Entity, mut entity: Entity) -> bool {
    for _ in 0..64 {
        if entity == root {
            return true;
        }
        match world.get::<Parent>(entity).map(|parent| parent.entity) {
            Some(parent) if world.is_alive(parent) => entity = parent,
            _ => return false,
        }
    }
    false
}

fn drive_drafts(world: &mut World) {
    let changes: Vec<TextChanged> = events::<TextChanged>(world).to_vec();
    for change in changes {
        let Some(field) = world.get::<StepField>(change.entity).copied() else {
            continue;
        };
        let Some(mut state) = world.get::<NumberState>(field.root).cloned() else {
            continue;
        };
        state.draft = change.text;
        state.editing = true;
        state.invalid = parse_number(&state.draft).is_none() && !partial_number(&state.draft);
        world.insert(field.root, state);
    }
}

fn drive_toasts(world: &mut World) {
    let now = world
        .resource::<Time>()
        .map(|time| time.now)
        .unwrap_or_default();
    let pointer = world
        .resource::<Input>()
        .map(|input| input.pointer.position)
        .unwrap_or([0.0, 0.0]);
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let toasts: Vec<Entity> = world.entities_with::<ToastState>();
    for toast in toasts {
        if !world.is_alive(toast) {
            continue;
        }
        if !world
            .get::<ToastState>(toast)
            .is_some_and(|state| state.entered)
        {
            world.remove::<Hidden>(toast);
            if reduced_motion(world) {
                world.insert(toast, Opacity(1.0));
            } else {
                motion::set(world, toast, Prop::Opacity, 0.0);
                motion::animate(
                    world,
                    toast,
                    Prop::Opacity,
                    1.0,
                    Curve::tween(0.16, Easing::EaseOut),
                );
            }
            if let Some(state) = world.get_mut::<ToastState>(toast) {
                state.entered = true;
            }
        }
        let Some(mut state) = world.get::<ToastState>(toast).cloned() else {
            continue;
        };
        if state.closing {
            let gone = reduced_motion(world)
                || motion::value(world, toast, Prop::Opacity).is_none_or(|opacity| opacity <= 0.02);
            if gone {
                world.insert(toast, Hidden);
            }
            continue;
        }
        if state.props.duration.is_zero() {
            world.insert(toast, state);
            continue;
        }
        let engaged = covers(world, toast, pointer)
            || focus.is_some_and(|entity| within(world, toast, entity));
        if engaged && !state.paused {
            if let Some(mark) = state.mark {
                state.left = state.left.saturating_sub(now.saturating_sub(mark));
            }
            state.mark = None;
            state.paused = true;
        } else if !engaged && state.paused {
            state.paused = false;
            state.mark = Some(now);
        } else if !state.paused && state.mark.is_none() {
            state.mark = Some(now);
        }
        let due = state
            .mark
            .is_some_and(|mark| now.saturating_sub(mark) >= state.left);
        world.insert(toast, state);
        if due {
            begin_close(world, toast, ToastNotice::Expired);
        }
    }
}

fn style_sliders(world: &mut World, theme: &UiTheme) {
    let focused = world.resource::<Focus>().and_then(|focus| focus.entity);
    let sliders: Vec<Entity> = world.entities_with::<SliderState>();
    for root in sliders {
        let Some(state) = world.get::<SliderState>(root).cloned() else {
            continue;
        };
        paint_availability(world, root, state.props.enabled);
        let fraction = fraction(state.value, state.props.range);
        place_segments(
            world,
            state.track,
            &[(state.fill, fraction), (state.rest, 1.0 - fraction)],
            &[state.thumb],
            !state.props.vertical,
            theme,
        );
        recipe(world, state.track, Token::Navigation, Corner::Compact);
        paint_thumb(
            world,
            state.thumb,
            state.face,
            focused == Some(root),
            hovered(world, root),
            state.props.enabled,
            theme,
        );
        paint_fill(world, state.fill, state.props.enabled);
        label_text(world, state.label, &state.props.label, state.props.enabled);
        semantics(
            world,
            root,
            &state.props.label,
            &state.props.description,
            state.props.readonly,
        );
        world.insert(
            root,
            state.props.range.access(state.value, !state.props.vertical),
        );
    }
}

fn style_ranges(world: &mut World, theme: &UiTheme) {
    let focused = world.resource::<Focus>().and_then(|focus| focus.entity);
    let ranges: Vec<Entity> = world.entities_with::<RangeState>();
    for root in ranges {
        let Some(state) = world.get::<RangeState>(root).cloned() else {
            continue;
        };
        paint_availability(world, root, state.props.enabled);
        let start = fraction(state.start, state.props.range);
        let end = fraction(state.end, state.props.range);
        let horizontal = !state.props.vertical;
        place_segments(
            world,
            state.track,
            &[
                (state.before, start),
                (state.between, (end - start).max(0.0)),
                (state.after, (1.0 - end).max(0.0)),
            ],
            &[state.start_thumb, state.end_thumb],
            horizontal,
            theme,
        );
        recipe(world, state.track, Token::Navigation, Corner::Compact);
        paint_fill(world, state.between, state.props.enabled);
        for (thumb_entity, face, value, name_suffix) in [
            (state.start_thumb, state.start_face, state.start, "start"),
            (state.end_thumb, state.end_face, state.end, "end"),
        ] {
            paint_thumb(
                world,
                thumb_entity,
                face,
                focused == Some(thumb_entity),
                hovered(world, thumb_entity),
                state.props.enabled,
                theme,
            );
            let name = format!("{} {name_suffix}", state.props.label);
            semantics(
                world,
                thumb_entity,
                &name,
                &state.props.description,
                state.props.readonly,
            );
            world.insert(thumb_entity, state.props.range.access(value, horizontal));
        }
        label_text(world, state.label, &state.props.label, state.props.enabled);
        semantics(
            world,
            root,
            &state.props.label,
            &state.props.description,
            state.props.readonly,
        );
    }
}

fn style_numbers(world: &mut World, theme: &UiTheme) {
    let numbers: Vec<Entity> = world.entities_with::<NumberState>();
    for root in numbers {
        let Some(state) = world.get::<NumberState>(root).cloned() else {
            continue;
        };
        paint_availability(world, root, state.props.enabled);
        let height = theme.target_min();
        for entity in [state.decrease, state.increase, state.field] {
            if let Some(layout) = world.get_mut::<Layout>(entity) {
                layout.min_height = height;
                if entity != state.field {
                    layout.width = Size::Px(height);
                    layout.height = Size::Px(height);
                }
            }
        }
        let message = if state.invalid {
            format!(
                "Enter a number from {} to {}",
                NumericAccess::text(state.props.range.min),
                NumericAccess::text(state.props.range.max)
            )
        } else {
            state.props.description.clone()
        };
        if message.is_empty() {
            world.insert(state.message, Hidden);
        } else {
            world.remove::<Hidden>(state.message);
            label_text(world, state.message, &message, true);
        }
        label_text(world, state.label, &state.props.label, state.props.enabled);
        semantics(
            world,
            root,
            &state.props.label,
            &message,
            state.props.readonly,
        );
        semantics(
            world,
            state.field,
            &state.props.label,
            &message,
            state.props.readonly,
        );
        world.insert(state.field, state.props.range.access(state.value, true));
        for (entity, verb) in [(state.decrease, "Decrease"), (state.increase, "Increase")] {
            semantics(
                world,
                entity,
                &format!("{verb} {}", state.props.label),
                "",
                state.props.readonly,
            );
            if let Some(button) = world.get_mut::<Button>(entity) {
                button.enabled = state.props.enabled;
            }
            recipe(world, entity, Token::Content, Corner::Control);
        }
        recipe(
            world,
            state.field,
            if state.props.enabled {
                Token::Content
            } else {
                Token::Navigation
            },
            Corner::Control,
        );
    }
}

fn style_toasts(world: &mut World, theme: &UiTheme) {
    let toasts: Vec<Entity> = world.entities_with::<ToastState>();
    for toast in toasts {
        let Some(state) = world.get::<ToastState>(toast).cloned() else {
            continue;
        };
        recipe(world, toast, Token::Overlay, Corner::Overlay);
        if let Some(layout) = world.get_mut::<Layout>(toast) {
            layout.padding = theme.space(crate::Space::X12);
            layout.gap = theme.space(crate::Space::X8);
        }
        for entity in state.undo.into_iter().chain(Some(state.dismiss)) {
            if world.is_alive(entity) {
                recipe(world, entity, Token::Content, Corner::Control);
                if let Some(layout) = world.get_mut::<Layout>(entity) {
                    layout.min_height = theme.target_min();
                }
            }
        }
    }
}

fn style_tips(world: &mut World, _theme: &UiTheme) {
    let tips: Vec<Entity> = world.entities_with::<Tip>();
    for target in tips {
        let Some(tip) = world.get::<Tip>(target).cloned() else {
            continue;
        };
        if world.is_alive(tip.popup) {
            recipe(world, tip.popup, Token::Overlay, Corner::Overlay);
        }
    }
}

fn fraction(value: f64, range: NumberRange) -> f64 {
    let span = range.max - range.min;
    if span <= f64::EPSILON {
        0.0
    } else {
        ((value - range.min) / span).clamp(0.0, 1.0)
    }
}

fn place_segments(
    world: &mut World,
    track: Entity,
    segments: &[(Entity, f64)],
    thumbs: &[Entity],
    horizontal: bool,
    theme: &UiTheme,
) {
    let thumb = (20.0 * theme.text_scale.clamp(0.5, 4.0)).clamp(16.0, 28.0_f32);
    let rect = world.get::<Rect>(track).copied().unwrap_or(Rect {
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    });
    let span = if horizontal { rect.w } else { rect.h };
    let flexible = (span - thumb * thumbs.len() as f32).max(0.0);
    for (entity, weight) in segments {
        if let Some(layout) = world.get_mut::<Layout>(*entity) {
            let size = flexible * (*weight as f32).max(0.0);
            if horizontal {
                layout.width = Size::Px(size);
                layout.height = Size::Px(4.0);
            } else {
                layout.width = Size::Px(4.0);
                layout.height = Size::Px(size);
            }
        }
    }
    for thumb_entity in thumbs {
        if let Some(layout) = world.get_mut::<Layout>(*thumb_entity) {
            layout.width = Size::Px(thumb);
            layout.height = Size::Px(thumb);
        }
    }
}

fn paint_fill(world: &mut World, entity: Entity, enabled: bool) {
    recipe(
        world,
        entity,
        if enabled {
            Token::Accent
        } else {
            Token::Disabled
        },
        Corner::Compact,
    );
}

fn paint_thumb(
    world: &mut World,
    thumb: Entity,
    face: Entity,
    focused: bool,
    hovered: bool,
    enabled: bool,
    _theme: &UiTheme,
) {
    recipe(
        world,
        thumb,
        if !enabled {
            Token::Disabled
        } else if focused {
            Token::Focus
        } else {
            Token::StrongStroke
        },
        Corner::Control,
    );
    recipe(
        world,
        face,
        if !enabled {
            Token::Navigation
        } else if hovered {
            Token::ControlHover
        } else {
            Token::Content
        },
        Corner::Compact,
    );
}

fn paint_availability(world: &mut World, root: Entity, enabled: bool) {
    if enabled {
        world.remove::<ControlDisabled>(root);
    } else {
        world.insert(root, ControlDisabled);
    }
}

fn semantics(world: &mut World, entity: Entity, name: &str, description: &str, readonly: bool) {
    world.insert(
        entity,
        Semantics {
            name: Some(name.to_string()),
            description: description.to_string(),
            readonly,
        },
    );
}

fn label_text(world: &mut World, entity: Entity, text: &str, enabled: bool) {
    if let Some(label) = world.get_mut::<Label>(entity) {
        if label.text != text {
            label.text = text.to_string();
        }
    }
    world.insert(
        entity,
        UiRecipe(UiStyle {
            foreground: Some(
                if enabled {
                    Token::Text
                } else {
                    Token::Disabled
                }
                .into(),
            ),
            typography: Some(Typography::Label),
            ..UiStyle::default()
        }),
    );
}

fn recipe(world: &mut World, entity: Entity, token: Token, corner: Corner) {
    world.insert(
        entity,
        UiRecipe(UiStyle {
            background: Some(token.into()),
            corner: Some(corner),
            ..UiStyle::default()
        }),
    );
}

fn hovered(world: &World, entity: Entity) -> bool {
    let Some(pointer) = world
        .resource::<Input>()
        .map(|input| input.pointer.position)
    else {
        return false;
    };
    covers(world, entity, pointer)
}
