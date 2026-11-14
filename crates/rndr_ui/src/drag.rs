//! In-app item drag and drop.
//!
//! A [`DragSource`] carries a host [`DragPayload`]. A [`DropTarget`] accepts or
//! rejects that payload and reports a [`DropPlace`]: before, after, inside, or
//! an edge reserved for a later split. File drop is a separate path. See
//! `docs/item-drag.md`.

use rndr_core::*;

/// Points the pointer must travel before a press becomes a drag.
///
/// This is the scroll slop. A shorter move stays a click.
pub const DRAG_THRESHOLD: f32 = SCROLL_DRAG_SLOP;

/// Distance from a scrollport edge, in points, that autoscrolls during a drag.
pub const AUTOSCROLL_MARGIN: f32 = 28.0;

/// Points added to the scroll offset on each frame while the pointer stays in
/// [`AUTOSCROLL_MARGIN`].
pub const AUTOSCROLL_STEP: f32 = 16.0;

/// Payload kind reserved by list reorder.
pub const LIST_ROW: DragKind = DragKind(1);

/// Payload kind reserved by tree reorder.
pub const TREE_NODE: DragKind = DragKind(2);

/// Host-defined payload kind. Targets accept or reject by this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DragKind(pub u64);

/// Value copied when a drag starts. The primitive does not interpret `value`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DragPayload {
    /// Which family of drop targets may accept this drag.
    pub kind: DragKind,
    /// Host identity. List and tree reorder store [`crate::collections::ItemId`]
    /// in this field.
    pub value: u64,
    /// View or other owner. A target with an owner accepts only the same one.
    pub owner: Option<Entity>,
}

/// How the focused source responds to the keyboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DragKeyboard {
    /// Alt+Enter picks the source up. Arrows move the highlight. Enter drops.
    /// Escape cancels.
    #[default]
    Pickup,
    /// The owner handles keyboard movement. Alt+Enter does not pick this up.
    Host,
}

/// Entity that can start a drag. The press may land on a descendant.
#[derive(Clone, Debug)]
pub struct DragSource {
    /// Copied at the press. Later edits do not change the gesture in progress.
    pub payload: DragPayload,
    /// Disabled sources do not arm.
    pub enabled: bool,
    /// Name used for the preview and for announcements.
    pub name: String,
    /// Keyboard policy for this source.
    pub keyboard: DragKeyboard,
}
impl Component for DragSource {}

/// Bands inside a target rectangle.
///
/// `axis` is the before/after direction. `edge` is the fraction of that axis,
/// at each end, reported as [`DropPlace::Edge`]. `cross_edge` is the same
/// fraction on the other axis and is tested first, so a later tab strip can
/// treat those bands as drop-to-split. `inside` is the centered fraction of
/// the remaining span reported as [`DropPlace::Inside`]. Zero means the
/// interior is only before and after, split at the midpoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropGeometry {
    /// Before/after axis.
    pub axis: Axis,
    /// End bands along [`Self::axis`]. Zero disables them.
    pub edge: f32,
    /// End bands on the cross axis. Zero disables them. Checked before `edge`.
    pub cross_edge: f32,
    /// Centered interior band. Zero splits the remainder in half.
    pub inside: f32,
}

impl DropGeometry {
    /// Before and after only. Lists use this.
    pub fn reorder(axis: Axis) -> Self {
        Self {
            axis,
            edge: 0.0,
            cross_edge: 0.0,
            inside: 0.0,
        }
    }

    /// Before, a centered inside band, and after. Trees use this.
    pub fn nest(axis: Axis) -> Self {
        Self {
            axis,
            edge: 0.0,
            cross_edge: 0.0,
            inside: 0.34,
        }
    }

    /// Cross-axis edge zones around a before/after interior.
    ///
    /// A later tab strip can map the edge zones to drop-to-split. This
    /// primitive does not split panes.
    pub fn split(axis: Axis) -> Self {
        Self {
            axis,
            edge: 0.0,
            cross_edge: 0.25,
            inside: 0.0,
        }
    }
}

/// Which end of an axis an edge band sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropEnd {
    /// Top or left, in layout points.
    Leading,
    /// Bottom or right, in layout points.
    Trailing,
}

/// An outer band of a target, not a before/after/inside slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropEdge {
    /// Axis the band was measured along.
    pub axis: Axis,
    /// Which end of [`Self::axis`].
    pub end: DropEnd,
}

/// Where inside a target the pointer or keyboard highlight sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPlace {
    /// Insert before the target along its geometry axis.
    Before,
    /// Insert after the target along its geometry axis.
    After,
    /// Drop into the target.
    Inside,
    /// Outer band. List and tree reorder reject this.
    Edge(DropEdge),
}

/// Extra accept check. Called only after kind, owner, and enabled succeed.
/// Return true to accept the place.
pub type DropGate = fn(&World, DragPayload, Entity, DropPlace) -> bool;

/// Entity that can receive a payload.
#[derive(Clone, Debug)]
pub struct DropTarget {
    /// Disabled targets reject every payload.
    pub enabled: bool,
    /// Kinds this target can accept. An empty list accepts none.
    pub kinds: Vec<DragKind>,
    /// When set, the payload owner must be this entity.
    pub owner: Option<Entity>,
    /// How the rectangle divides into places.
    pub geometry: DropGeometry,
    /// Extra reject. `None` means kind and owner are enough.
    pub gate: Option<DropGate>,
}
impl Component for DropTarget {}

/// Hover written on the current target and cleared when the pointer leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropCue {
    /// Band under the pointer or keyboard highlight.
    pub place: DropPlace,
    /// False when the target rejects this payload or place.
    pub accepted: bool,
    /// Payload in flight.
    pub payload: DragPayload,
}
impl Component for DropCue {}

/// A drop the target accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragDrop {
    /// Payload captured at the start of the drag.
    pub payload: DragPayload,
    /// Target under the pointer or keyboard highlight.
    pub target: Entity,
    /// Place on [`Self::target`].
    pub place: DropPlace,
}

/// One hover sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragHover {
    /// Target under the pointer or highlight.
    pub target: Entity,
    /// Place on [`Self::target`].
    pub place: DropPlace,
    /// Whether a release or Enter would drop here.
    pub accepted: bool,
    /// Payload in flight.
    pub payload: DragPayload,
}

/// What changed in the drag this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragNoticeKind {
    /// The pointer crossed [`DRAG_THRESHOLD`], or Alt+Enter picked a source up.
    Started(DragPayload),
    /// The hover target, place, or acceptance changed.
    Hovered(DragHover),
    /// Released or confirmed on an accepted place.
    Dropped(DragDrop),
    /// Escape, a rejected release, or a release with no target.
    Cancelled {
        /// Payload that was in flight.
        payload: DragPayload,
    },
}

/// Observable drag transition. Readable in Update after the Input that sent it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DragNotice {
    /// What happened.
    pub kind: DragNoticeKind,
}
impl Event for DragNotice {}

/// Coarse phase. [`DragNotice`] carries the payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragPhase {
    /// No gesture.
    Idle,
    /// Pressed on a source. The pointer has not crossed [`DRAG_THRESHOLD`].
    Armed,
    /// Pointer drag. A preview is showing.
    Dragging,
    /// Keyboard pickup. Arrows move the highlight.
    Keyboard,
}

/// Place of `point` inside `rect`, or `None` when `point` misses `rect`.
pub fn classify(rect: Rect, point: [f32; 2], geometry: DropGeometry) -> Option<DropPlace> {
    if rect.w <= 0.0 || rect.h <= 0.0 || !rect.contains(point[0], point[1]) {
        return None;
    }
    let edge = band(geometry.edge);
    let cross_edge = band(geometry.cross_edge);
    let inside = portion(geometry.inside);
    let cross_axis = cross(geometry.axis);
    let along = axis_t(geometry.axis, rect, point)?;
    let across = axis_t(cross_axis, rect, point)?;
    if cross_edge > 0.0 && across < cross_edge {
        return Some(edge_place(cross_axis, DropEnd::Leading));
    }
    if cross_edge > 0.0 && across > 1.0 - cross_edge {
        return Some(edge_place(cross_axis, DropEnd::Trailing));
    }
    if edge > 0.0 && along < edge {
        return Some(edge_place(geometry.axis, DropEnd::Leading));
    }
    if edge > 0.0 && along > 1.0 - edge {
        return Some(edge_place(geometry.axis, DropEnd::Trailing));
    }
    let span = (1.0 - edge * 2.0).max(f32::EPSILON);
    let t = ((along - edge) / span).clamp(0.0, 1.0);
    Some(if inside <= 0.0 {
        if t < 0.5 {
            DropPlace::Before
        } else {
            DropPlace::After
        }
    } else {
        let side = (1.0 - inside) * 0.5;
        if t < side {
            DropPlace::Before
        } else if t > 1.0 - side {
            DropPlace::After
        } else {
            DropPlace::Inside
        }
    })
}

/// Current phase. Idle when the plugin has not installed a session.
pub fn phase(world: &World) -> DragPhase {
    let Some(session) = world.resource::<Session>() else {
        return DragPhase::Idle;
    };
    match &session.phase {
        Phase::Idle => DragPhase::Idle,
        Phase::Armed(_) => DragPhase::Armed,
        Phase::Dragging(_) => DragPhase::Dragging,
        Phase::Keyboard(_) => DragPhase::Keyboard,
    }
}

/// Preview entity while a drag or keyboard pickup is showing one.
pub fn preview(world: &World) -> Option<Entity> {
    let session = world.resource::<Session>()?;
    match &session.phase {
        Phase::Dragging(gesture) | Phase::Keyboard(gesture) => gesture.ghost,
        _ => None,
    }
}

/// Last announcement copied into the polite status region. Empty before the
/// first one.
pub fn announcement(world: &World) -> String {
    world
        .resource::<Session>()
        .map(|session| session.announcement.clone())
        .unwrap_or_default()
}

/// Drop recorded earlier in this Input pass. Cleared on the next Input.
///
/// Update readers use [`DragNotice`] instead. Collection reorder reads this
/// so it can emit before autoscroll recycles a virtual row.
pub fn committed(world: &World) -> Option<DragDrop> {
    world
        .resource::<Session>()
        .and_then(|session| session.dropped)
}

/// Replace the polite live-region text. The node is created on first use and
/// is not painted.
pub fn announce(world: &mut World, text: &str) {
    ensure_session(world);
    let entity = announcer(world);
    if let Some(label) = world.get_mut::<Label>(entity) {
        label.text = text.to_string();
    }
    if let Some(semantics) = world.get_mut::<Semantics>(entity) {
        semantics.name = Some(text.to_string());
    }
    if let Some(session) = world.resource_mut::<Session>() {
        session.announcement = text.to_string();
        session.announcer = Some(entity);
    }
}

pub(crate) fn install(app: &mut App) {
    app.world_mut()
        .insert_resource(PointerDragOwner(claims_press));
    app.world_mut().insert_resource(Session::default());
    app.add_system_front(Schedule::Input, input);
    app.add_system(Schedule::Input, after);
}

pub(crate) fn claims_press(world: &World, hit: Entity) -> bool {
    let Some(input) = world.resource::<Input>() else {
        return false;
    };
    if input.pointer.button != PointerButton::Left {
        return false;
    }
    if ancestor_with::<Editor>(world, hit).is_some() {
        return false;
    }
    let Some(source) = ancestor_with::<DragSource>(world, hit) else {
        return false;
    };
    let Some(drag) = world.get::<DragSource>(source) else {
        return false;
    };
    if !drag.enabled {
        return false;
    }
    if let Some(button) = ancestor_with::<Button>(world, hit) {
        if button != source {
            return false;
        }
    }
    true
}

fn input(world: &mut World) {
    ensure_session(world);
    if let Some(session) = world.resource_mut::<Session>() {
        session.dropped = None;
    }
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    let escape = take_escape(world);
    let pickup = take_pickup(world);
    let keyboard = take_keyboard(world);
    if escape {
        on_escape(world);
    } else {
        step_pointer(world, &input);
        if pickup {
            begin_keyboard(world);
        }
        for event in keyboard {
            on_keyboard(world, &event);
        }
    }
    present(world);
    let dragging = phase(world) == DragPhase::Dragging;
    world.insert_resource(DismissHold {
        hold: phase(world) != DragPhase::Idle,
    });
    if dragging {
        pause_focus_reveal(world);
    }
}

fn after(world: &mut World) {
    if phase(world) != DragPhase::Dragging {
        return;
    }
    cancel_activation(world);
    let point = world
        .resource::<Input>()
        .map(|input| input.pointer.position);
    if let Some(point) = point {
        if autoscroll(world, point) {
            refresh_hover(world, point);
        }
    }
    let rejected = hover_of(world).is_some_and(|hover| !hover.accepted);
    world.insert_resource(if rejected {
        CursorStyle::NoDrop
    } else {
        CursorStyle::Grabbing
    });
}

fn step_pointer(world: &mut World, input: &Input) {
    let left = input.pointer.button == PointerButton::Left;
    if input.pointer.pressed && !input.pointer.released && left && phase(world) == DragPhase::Idle {
        let at = input.pointer.press_at.unwrap_or(input.pointer.position);
        if let Some(hit) = hit_test(world, at[0], at[1]) {
            if claims_press(world, hit) {
                if let Some(source) = ancestor_with::<DragSource>(world, hit) {
                    if let Some(drag) = world.get::<DragSource>(source).cloned() {
                        set_phase(
                            world,
                            Phase::Armed(Armed {
                                source,
                                payload: drag.payload,
                                origin: at,
                                name: drag.name,
                                keyboard: drag.keyboard,
                                pointer: input.pointer.id,
                            }),
                        );
                    }
                }
            }
        }
    }
    if phase(world) == DragPhase::Keyboard && input.pointer.pressed {
        cancel_live(world, true);
        return;
    }
    let Some(armed) = armed_of(world) else {
        if let Some(live) = live_of(world) {
            if live.keyboard_mode {
                return;
            }
            if input.pointer.released || !input.pointer.down || input.pointer.id != live.pointer {
                finish_pointer(world, input.pointer.position);
            } else {
                move_pointer(world, input.pointer.position);
            }
        }
        return;
    };
    if input.pointer.released || !input.pointer.down {
        set_phase(world, Phase::Idle);
        return;
    }
    if input.pointer.id != armed.pointer {
        set_phase(world, Phase::Idle);
        return;
    }
    let distance = travel(armed.origin, input.pointer.position);
    if distance >= DRAG_THRESHOLD {
        begin_pointer(world, input.pointer.position);
    }
}

fn begin_pointer(world: &mut World, point: [f32; 2]) {
    let Some(armed) = armed_of(world) else {
        return;
    };
    let mut live = Live::from_armed(armed, false);
    live.point = point;
    cancel_activation(world);
    send(
        world,
        DragNotice {
            kind: DragNoticeKind::Started(live.payload),
        },
    );
    set_phase(world, Phase::Dragging(live));
    refresh_hover(world, point);
}

fn move_pointer(world: &mut World, point: [f32; 2]) {
    if let Some(session) = world.resource_mut::<Session>() {
        if let Phase::Dragging(live) = &mut session.phase {
            live.point = point;
        }
    }
    refresh_hover(world, point);
}

fn finish_pointer(world: &mut World, point: [f32; 2]) {
    refresh_hover(world, point);
    let name = live_of(world).map(|live| live.name).unwrap_or_default();
    match hover_of(world) {
        Some(hover) if hover.accepted => commit(world, hover),
        Some(_) => {
            cancel_live(world, false);
            if !name.is_empty() {
                announce(world, &format!("Cannot drop {name} here."));
            }
        }
        None => cancel_live(world, true),
    }
}

fn begin_keyboard(world: &mut World) {
    if phase(world) != DragPhase::Idle {
        return;
    }
    let Some(focus) = world.resource::<Focus>().and_then(|focus| focus.entity) else {
        return;
    };
    if world.has::<CapturesKeys>(focus) {
        return;
    }
    let Some(source) = ancestor_with::<DragSource>(world, focus) else {
        return;
    };
    let Some(drag) = world.get::<DragSource>(source).cloned() else {
        return;
    };
    if !drag.enabled || drag.keyboard != DragKeyboard::Pickup {
        return;
    }
    let rect = world.get::<Rect>(source).copied();
    let origin = rect
        .map(|rect| [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5])
        .unwrap_or([0.0, 0.0]);
    let live = Live {
        source,
        payload: drag.payload,
        name: drag.name.clone(),
        point: origin,
        pointer: 0,
        hover: None,
        ghost: None,
        slot: None,
        keyboard_mode: true,
        keyboard: DragKeyboard::Pickup,
    };
    send(
        world,
        DragNotice {
            kind: DragNoticeKind::Started(drag.payload),
        },
    );
    set_phase(world, Phase::Keyboard(live));
    announce(
        world,
        &format!(
            "Dragging {}. Arrow keys choose a drop position. Enter drops. Escape cancels.",
            drag.name
        ),
    );
}

fn on_keyboard(world: &mut World, event: &KeyEvent) {
    if phase(world) != DragPhase::Keyboard || event.released || event.synthetic || event.repeat {
        return;
    }
    let LogicalKey::Named(key) = event.logical else {
        return;
    };
    match key {
        NamedKey::Escape => cancel_live(world, true),
        NamedKey::Enter => confirm_keyboard(world),
        NamedKey::ArrowUp | NamedKey::ArrowDown | NamedKey::ArrowLeft | NamedKey::ArrowRight => {
            move_keyboard(world, key);
        }
        _ => {}
    }
}

fn confirm_keyboard(world: &mut World) {
    let Some(hover) = hover_of(world) else {
        cancel_live(world, true);
        return;
    };
    if hover.accepted {
        commit(world, hover);
    } else {
        let name = live_of(world).map(|live| live.name).unwrap_or_default();
        cancel_live(world, false);
        if !name.is_empty() {
            announce(world, &format!("Cannot drop {name} here."));
        }
    }
}

fn move_keyboard(world: &mut World, key: NamedKey) {
    let Some(live) = live_of(world) else {
        return;
    };
    let targets = keyboard_targets(world, live.payload);
    if targets.is_empty() {
        announce(world, &format!("Cannot drop {} here.", live.name));
        return;
    }
    let origin = targets.iter().position(|entity| *entity == live.source);
    let base = live.slot.or(origin);
    let next = match key {
        NamedKey::ArrowDown | NamedKey::ArrowRight => Some(
            base.map(|slot| (slot + 1).min(targets.len() - 1))
                .unwrap_or(0),
        ),
        NamedKey::ArrowUp | NamedKey::ArrowLeft => Some(
            base.map(|slot| slot.saturating_sub(1))
                .unwrap_or(targets.len() - 1),
        ),
        _ => live.slot,
    };
    let Some(slot) = next else {
        return;
    };
    let target = targets[slot];
    let place = keyboard_place(world, target, key);
    let accepted = target_accepts(world, target, live.payload, place);
    let hover = Hover {
        target,
        place,
        accepted,
    };
    let name = live.name.clone();
    let point = world
        .get::<Rect>(target)
        .map(|rect| [rect.x, rect.y])
        .unwrap_or(live.point);
    if let Some(session) = world.resource_mut::<Session>() {
        if let Phase::Keyboard(live) = &mut session.phase {
            live.slot = Some(slot);
            live.hover = Some(hover);
            live.point = point;
        }
    }
    note_hover(world, live.payload, hover);
    let target_name = target_name(world, target);
    if accepted {
        announce(
            world,
            &format!("Drop {name} {} {target_name}.", place_word(place)),
        );
    } else {
        announce(world, &format!("Cannot drop {name} here."));
    }
}

fn on_escape(world: &mut World) {
    match phase(world) {
        DragPhase::Armed => {
            cancel_activation(world);
            set_phase(world, Phase::Idle);
            clear_cue(world);
        }
        DragPhase::Dragging | DragPhase::Keyboard => cancel_live(world, true),
        DragPhase::Idle => {}
    }
}

fn commit(world: &mut World, hover: Hover) {
    let Some(live) = live_of(world) else {
        return;
    };
    let drop = DragDrop {
        payload: live.payload,
        target: hover.target,
        place: hover.place,
    };
    send(
        world,
        DragNotice {
            kind: DragNoticeKind::Dropped(drop),
        },
    );
    if let Some(session) = world.resource_mut::<Session>() {
        session.dropped = Some(drop);
    }
    if live.keyboard == DragKeyboard::Pickup {
        let target_name = target_name(world, hover.target);
        announce(
            world,
            &format!(
                "Dropped {} {} {target_name}.",
                live.name,
                place_word(hover.place)
            ),
        );
    }
    cleanup(world);
    resume_focus_reveal(world);
    world.insert_resource(CursorStyle::Arrow);
}

fn cancel_live(world: &mut World, standard: bool) {
    let Some(live) = live_of(world) else {
        set_phase(world, Phase::Idle);
        return;
    };
    send(
        world,
        DragNotice {
            kind: DragNoticeKind::Cancelled {
                payload: live.payload,
            },
        },
    );
    if standard {
        announce(world, &format!("Cancelled drag of {}.", live.name));
    }
    cleanup(world);
    resume_focus_reveal(world);
    world.insert_resource(CursorStyle::Arrow);
}

fn cleanup(world: &mut World) {
    despawn_ghost(world);
    clear_cue(world);
    set_phase(world, Phase::Idle);
}

fn present(world: &mut World) {
    sync_ghost(world);
    sync_bar(world);
}

fn refresh_hover(world: &mut World, point: [f32; 2]) {
    let Some(live) = live_of(world) else {
        return;
    };
    if live.keyboard_mode {
        return;
    }
    let hover = hover_at(world, point, live.payload);
    let changed = live.hover != hover;
    if let Some(session) = world.resource_mut::<Session>() {
        if let Phase::Dragging(live) = &mut session.phase {
            live.hover = hover;
            live.point = point;
        }
    }
    if changed {
        if let Some(hover) = hover {
            note_hover(world, live.payload, hover);
        }
    }
    sync_bar(world);
}

fn note_hover(world: &mut World, payload: DragPayload, hover: Hover) {
    send(
        world,
        DragNotice {
            kind: DragNoticeKind::Hovered(DragHover {
                target: hover.target,
                place: hover.place,
                accepted: hover.accepted,
                payload,
            }),
        },
    );
}

fn hover_at(world: &World, point: [f32; 2], payload: DragPayload) -> Option<Hover> {
    let hit = hit_test(world, point[0], point[1])?;
    let entity = ancestor_with::<DropTarget>(world, hit)?;
    let place = {
        let target = world.get::<DropTarget>(entity)?;
        let rect = world.get::<Rect>(entity).copied()?;
        classify(rect, point, target.geometry)?
    };
    let accepted = target_accepts(world, entity, payload, place);
    Some(Hover {
        target: entity,
        place,
        accepted,
    })
}

fn target_accepts(world: &World, entity: Entity, payload: DragPayload, place: DropPlace) -> bool {
    let Some(target) = world.get::<DropTarget>(entity) else {
        return false;
    };
    if !target.enabled || !target.kinds.contains(&payload.kind) {
        return false;
    }
    if target.owner.is_some() && target.owner != payload.owner {
        return false;
    }
    target
        .gate
        .is_none_or(|gate| gate(world, payload, entity, place))
}

fn sync_ghost(world: &mut World) {
    let Some(live) = live_of(world) else {
        despawn_ghost(world);
        return;
    };
    if !world.is_alive(live.source) {
        cancel_live(world, true);
        return;
    }
    let ghost = match live.ghost.filter(|entity| world.is_alive(*entity)) {
        Some(ghost) => ghost,
        None => spawn_ghost(world, &live),
    };
    if let Some(session) = world.resource_mut::<Session>() {
        match &mut session.phase {
            Phase::Dragging(live) | Phase::Keyboard(live) => live.ghost = Some(ghost),
            _ => {}
        }
    }
    let point = live_of(world).map(|live| live.point).unwrap_or([0.0, 0.0]);
    let at = if matches!(phase(world), DragPhase::Dragging) {
        [point[0] + 12.0, point[1] + 12.0]
    } else {
        point
    };
    world.insert(ghost, PopupPlacement::At(at));
}

fn spawn_ghost(world: &mut World, live: &Live) -> Entity {
    let ghost = world.spawn((
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            padding: 8.0,
            ..Layout::column()
        },
        Label {
            color: Some(Color::rgb(1.0, 1.0, 1.0)),
            ..Label::new(&live.name)
        },
        Color::rgba(0.16, 0.2, 0.28, 0.94),
        Radius(8.0),
        Opacity(0.94),
        Layer(24),
    ));
    world.insert(ghost, IgnorePointer);
    world.insert(ghost, PassivePopup);
    world.insert(
        ghost,
        AnchoredPopup {
            anchor: live.source,
            max_height: 72.0,
            closed: ghost_closed,
        },
    );
    world.insert(ghost, PopupPlacement::At(live.point));
    ghost
}

fn ghost_closed(_world: &mut World, _root: Entity) {}

fn despawn_ghost(world: &mut World) {
    let ghost = live_of(world).and_then(|live| live.ghost);
    if let Some(ghost) = ghost {
        if world.is_alive(ghost) {
            despawn(world, ghost);
        }
    }
    if let Some(session) = world.resource_mut::<Session>() {
        match &mut session.phase {
            Phase::Dragging(live) | Phase::Keyboard(live) => live.ghost = None,
            _ => {}
        }
    }
}

fn sync_bar(world: &mut World) {
    clear_cue(world);
    let Some(live) = live_of(world) else {
        return;
    };
    let Some(hover) = live.hover else {
        return;
    };
    if !world.is_alive(hover.target) {
        return;
    }
    world.insert(
        hover.target,
        DropCue {
            place: hover.place,
            accepted: hover.accepted,
            payload: live.payload,
        },
    );
    if !hover.accepted {
        return;
    }
    let axis = world
        .get::<DropTarget>(hover.target)
        .map(|target| target.geometry.axis)
        .unwrap_or(Axis::Column);
    let bar = spawn_bar(
        world,
        hover.target,
        hover.place,
        axis,
        hover.place == DropPlace::Inside,
    );
    if let Some(session) = world.resource_mut::<Session>() {
        session.bar = Some(bar);
    }
}

fn spawn_bar(
    world: &mut World,
    parent: Entity,
    place: DropPlace,
    axis: Axis,
    fill: bool,
) -> Entity {
    let (pin, layout) = bar_shape(place, axis);
    let color = if fill {
        Color::rgba(0.2, 0.45, 0.95, 0.28)
    } else {
        Color::rgba(0.2, 0.45, 0.95, 0.95)
    };
    let bar = world.spawn((layout, pin, color, IgnorePointer));
    set_parent(world, bar, parent);
    bar
}

fn bar_shape(place: DropPlace, axis: Axis) -> (Absolute, Layout) {
    let horizontal = matches!(
        (place, axis),
        (DropPlace::Before, Axis::Column)
            | (DropPlace::After, Axis::Column)
            | (
                DropPlace::Edge(DropEdge {
                    axis: Axis::Column,
                    ..
                }),
                _
            )
    );
    let leading = matches!(
        place,
        DropPlace::Before
            | DropPlace::Edge(DropEdge {
                end: DropEnd::Leading,
                ..
            })
    );
    if matches!(place, DropPlace::Inside) {
        return (
            Absolute {
                left: Some(1.0),
                top: Some(1.0),
                right: Some(1.0),
                bottom: Some(1.0),
            },
            Layout {
                width: Size::Hug,
                height: Size::Hug,
                ..Layout::column()
            },
        );
    }
    if horizontal {
        let pin = if leading {
            Absolute {
                left: Some(0.0),
                right: Some(0.0),
                top: Some(0.0),
                bottom: None,
            }
        } else {
            Absolute {
                left: Some(0.0),
                right: Some(0.0),
                top: None,
                bottom: Some(0.0),
            }
        };
        (
            pin,
            Layout {
                width: Size::Hug,
                height: Size::Px(3.0),
                ..Layout::column()
            },
        )
    } else {
        let pin = if leading {
            Absolute {
                left: Some(0.0),
                right: None,
                top: Some(0.0),
                bottom: Some(0.0),
            }
        } else {
            Absolute {
                left: None,
                right: Some(0.0),
                top: Some(0.0),
                bottom: Some(0.0),
            }
        };
        (
            pin,
            Layout {
                width: Size::Px(3.0),
                height: Size::Hug,
                ..Layout::column()
            },
        )
    }
}

fn clear_cue(world: &mut World) {
    let bar = world.resource::<Session>().and_then(|session| session.bar);
    if let Some(bar) = bar {
        if world.is_alive(bar) {
            despawn(world, bar);
        }
    }
    if let Some(session) = world.resource_mut::<Session>() {
        session.bar = None;
    }
    let marked: Vec<_> = world.entities_with::<DropCue>();
    for entity in marked {
        world.remove::<DropCue>(entity);
    }
}

fn autoscroll(world: &mut World, point: [f32; 2]) -> bool {
    let Some(hit) = hit_test(world, point[0], point[1]) else {
        return false;
    };
    let mut current = Some(hit);
    while let Some(entity) = current {
        if world.has::<Scroll>(entity) && nudge(world, entity, point) {
            layout_propagate(world);
            return true;
        }
        current = world.get::<Parent>(entity).map(|parent| parent.entity);
    }
    false
}

fn nudge(world: &mut World, entity: Entity, point: [f32; 2]) -> bool {
    let Some(port) = world.get::<Rect>(entity).copied() else {
        return false;
    };
    let bounds = world
        .get::<Clip>(entity)
        .copied()
        .map(|clip| Rect {
            x: clip.x,
            y: clip.y,
            w: clip.w,
            h: clip.h,
        })
        .filter(|rect| rect.w > 0.0 && rect.h > 0.0)
        .unwrap_or(port);
    if !bounds.contains(point[0], point[1]) {
        return false;
    }
    let Some(scroll) = world.get::<Scroll>(entity).copied() else {
        return false;
    };
    let min_x = (port.w - scroll.content[0]).min(0.0);
    let min_y = (port.h - scroll.content[1]).min(0.0);
    let mut delta = [0.0, 0.0];
    if point[0] <= bounds.x + AUTOSCROLL_MARGIN && scroll.offset[0] < -0.5 {
        delta[0] = AUTOSCROLL_STEP.min(-scroll.offset[0]);
    } else if point[0] >= bounds.x + bounds.w - AUTOSCROLL_MARGIN && scroll.offset[0] > min_x + 0.5
    {
        delta[0] = -(AUTOSCROLL_STEP.min(scroll.offset[0] - min_x));
    }
    if point[1] <= bounds.y + AUTOSCROLL_MARGIN && scroll.offset[1] < -0.5 {
        delta[1] = AUTOSCROLL_STEP.min(-scroll.offset[1]);
    } else if point[1] >= bounds.y + bounds.h - AUTOSCROLL_MARGIN && scroll.offset[1] > min_y + 0.5
    {
        delta[1] = -(AUTOSCROLL_STEP.min(scroll.offset[1] - min_y));
    }
    if delta == [0.0, 0.0] {
        return false;
    }
    if let Some(scroll) = world.get_mut::<Scroll>(entity) {
        scroll.offset[0] += delta[0];
        scroll.offset[1] += delta[1];
        scroll.clamp_to(port.w, port.h);
    }
    true
}

fn keyboard_targets(world: &World, payload: DragPayload) -> Vec<Entity> {
    let mut found: Vec<(Entity, f32, f32)> = world
        .entities_with::<DropTarget>()
        .into_iter()
        .filter_map(|entity| {
            let target = world.get::<DropTarget>(entity)?;
            if !target.enabled || !target.kinds.contains(&payload.kind) {
                return None;
            }
            if target.owner.is_some() && target.owner != payload.owner {
                return None;
            }
            let rect = world.get::<Rect>(entity).copied()?;
            if rect.w <= 0.0 || rect.h <= 0.0 {
                return None;
            }
            Some((entity, rect.y, rect.x))
        })
        .collect();
    found.sort_by(|left, right| left.1.total_cmp(&right.1).then(left.2.total_cmp(&right.2)));
    found.into_iter().map(|(entity, _, _)| entity).collect()
}

fn keyboard_place(world: &World, target: Entity, key: NamedKey) -> DropPlace {
    let geometry = world
        .get::<DropTarget>(target)
        .map(|target| target.geometry)
        .unwrap_or_else(|| DropGeometry::reorder(Axis::Column));
    if geometry.inside > 0.0 && key == NamedKey::ArrowRight {
        return DropPlace::Inside;
    }
    match (geometry.axis, key) {
        (Axis::Column, NamedKey::ArrowUp) | (Axis::Row, NamedKey::ArrowLeft) => DropPlace::Before,
        _ => DropPlace::After,
    }
}

fn target_name(world: &World, entity: Entity) -> String {
    world
        .get::<DragSource>(entity)
        .map(|source| source.name.clone())
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            world
                .get::<Semantics>(entity)
                .and_then(|semantics| semantics.name.clone())
        })
        .filter(|name| !name.trim().is_empty())
        .or_else(|| world.get::<Label>(entity).map(|label| label.text.clone()))
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "target".to_string())
}

pub(crate) fn place_word(place: DropPlace) -> &'static str {
    match place {
        DropPlace::Before => "before",
        DropPlace::After => "after",
        DropPlace::Inside => "inside",
        DropPlace::Edge(_) => "on the edge of",
    }
}

fn announcer(world: &mut World) -> Entity {
    if let Some(entity) = world
        .resource::<Session>()
        .and_then(|session| session.announcer)
    {
        if world.is_alive(entity) {
            return entity;
        }
    }
    world.spawn((
        Label::new(""),
        AccessMark(AccessRole::Status),
        LiveRegion(LivePoliteness::Polite),
        Semantics::default(),
        Opacity(0.0),
        IgnorePointer,
    ))
}

fn take_escape(world: &mut World) -> bool {
    if phase(world) == DragPhase::Idle {
        return false;
    }
    let Some(input) = world.resource::<Input>().cloned() else {
        return false;
    };
    let hit = input.key == Some(Key::Escape) || input.keys.iter().any(is_escape);
    if !hit {
        return false;
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.keys.retain(|event| !is_escape(event));
        if input.key == Some(Key::Escape) {
            input.key = None;
        }
    }
    true
}

fn take_pickup(world: &mut World) -> bool {
    if phase(world) != DragPhase::Idle || !pickup_focus(world) {
        return false;
    }
    let Some(input) = world.resource_mut::<Input>() else {
        return false;
    };
    let index = input.keys.iter().position(is_pickup);
    let Some(index) = index else {
        return false;
    };
    input.keys.remove(index);
    true
}

fn pickup_focus(world: &World) -> bool {
    let Some(focus) = world.resource::<Focus>().and_then(|focus| focus.entity) else {
        return false;
    };
    if world.has::<CapturesKeys>(focus) {
        return false;
    }
    let Some(source) = ancestor_with::<DragSource>(world, focus) else {
        return false;
    };
    world
        .get::<DragSource>(source)
        .is_some_and(|drag| drag.enabled && drag.keyboard == DragKeyboard::Pickup)
}

fn take_keyboard(world: &mut World) -> Vec<KeyEvent> {
    if phase(world) != DragPhase::Keyboard {
        return Vec::new();
    }
    let Some(input) = world.resource_mut::<Input>() else {
        return Vec::new();
    };
    let mut kept = Vec::new();
    let mut taken = Vec::new();
    for event in input.keys.drain(..) {
        if is_keyboard_command(&event) {
            taken.push(event);
        } else {
            kept.push(event);
        }
    }
    input.keys = kept;
    taken
}

fn is_escape(event: &KeyEvent) -> bool {
    !event.released
        && !event.synthetic
        && matches!(event.logical, LogicalKey::Named(NamedKey::Escape))
        && !event.modifiers.alt
        && !event.modifiers.ctrl
        && !event.modifiers.super_key
}

fn is_pickup(event: &KeyEvent) -> bool {
    !event.released
        && !event.repeat
        && !event.synthetic
        && event.modifiers.alt
        && !event.modifiers.command()
        && !event.modifiers.shift
        && matches!(event.logical, LogicalKey::Named(NamedKey::Enter))
}

fn is_keyboard_command(event: &KeyEvent) -> bool {
    if event.released || event.synthetic || event.modifiers.command() || event.modifiers.alt {
        return false;
    }
    matches!(
        event.logical,
        LogicalKey::Named(
            NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::Enter
                | NamedKey::Escape
        )
    )
}

fn ensure_session(world: &mut World) {
    if !world.has_resource::<Session>() {
        world.insert_resource(Session::default());
    }
}

fn set_phase(world: &mut World, phase: Phase) {
    ensure_session(world);
    if let Some(session) = world.resource_mut::<Session>() {
        session.phase = phase;
    }
}

fn armed_of(world: &World) -> Option<Armed> {
    match &world.resource::<Session>()?.phase {
        Phase::Armed(armed) => Some(armed.clone()),
        _ => None,
    }
}

fn live_of(world: &World) -> Option<Live> {
    match &world.resource::<Session>()?.phase {
        Phase::Dragging(live) | Phase::Keyboard(live) => Some(live.clone()),
        _ => None,
    }
}

fn hover_of(world: &World) -> Option<Hover> {
    live_of(world).and_then(|live| live.hover)
}

fn band(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value.min(0.49)
    } else {
        0.0
    }
}

fn portion(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn cross(axis: Axis) -> Axis {
    match axis {
        Axis::Row => Axis::Column,
        Axis::Column => Axis::Row,
    }
}

fn axis_t(axis: Axis, rect: Rect, point: [f32; 2]) -> Option<f32> {
    let (start, size, value) = match axis {
        Axis::Row => (rect.x, rect.w, point[0]),
        Axis::Column => (rect.y, rect.h, point[1]),
    };
    if size <= 0.0 {
        return None;
    }
    Some(((value - start) / size).clamp(0.0, 1.0))
}

fn edge_place(axis: Axis, end: DropEnd) -> DropPlace {
    DropPlace::Edge(DropEdge { axis, end })
}

fn travel(origin: [f32; 2], point: [f32; 2]) -> f32 {
    let dx = point[0] - origin[0];
    let dy = point[1] - origin[1];
    if dx.is_finite() && dy.is_finite() {
        dx.hypot(dy)
    } else {
        0.0
    }
}

#[derive(Clone)]
struct Armed {
    source: Entity,
    payload: DragPayload,
    origin: [f32; 2],
    name: String,
    keyboard: DragKeyboard,
    pointer: u64,
}

#[derive(Clone)]
struct Live {
    source: Entity,
    payload: DragPayload,
    name: String,
    point: [f32; 2],
    pointer: u64,
    hover: Option<Hover>,
    ghost: Option<Entity>,
    slot: Option<usize>,
    keyboard_mode: bool,
    keyboard: DragKeyboard,
}

impl Live {
    fn from_armed(armed: Armed, keyboard_mode: bool) -> Self {
        Self {
            source: armed.source,
            payload: armed.payload,
            name: armed.name,
            point: armed.origin,
            pointer: armed.pointer,
            hover: None,
            ghost: None,
            slot: None,
            keyboard_mode,
            keyboard: armed.keyboard,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Hover {
    target: Entity,
    place: DropPlace,
    accepted: bool,
}

enum Phase {
    Idle,
    Armed(Armed),
    Dragging(Live),
    Keyboard(Live),
}

struct Session {
    phase: Phase,
    dropped: Option<DragDrop>,
    announcer: Option<Entity>,
    announcement: String,
    bar: Option<Entity>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            dropped: None,
            announcer: None,
            announcement: String::new(),
            bar: None,
        }
    }
}
