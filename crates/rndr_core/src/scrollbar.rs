//! Interactive scroll tracks. Paint the returned geometry with application styles.
use crate::{
    Axis, Clip, Component, Editor, Entity, Focus, Input, NamedKey, PointerButton, Rect, Scroll,
    World,
};

/// A focusable track controlling one canonical viewport. Editor targets support
/// vertical soft-wrapped scrolling only; ordinary Scroll targets support both axes.
#[derive(Clone, Copy, Debug)]
pub struct Scrollbar {
    /// Live viewport entity. A stale generation disables the control.
    /// Give the track a Layout at least 24 points thick for a usable pointer
    /// target; paint can draw a narrower visual inside that hit region.
    pub target: Entity,
    /// Horizontal or vertical movement.
    pub axis: Axis,
}
impl Component for Scrollbar {}

/// Geometry and range shared by paint, pointer and accessibility projections.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarGeometry {
    /// Track after ancestor clipping.
    pub track: Rect,
    /// Thumb, at least 24 points long when the track allows it.
    pub thumb: Rect,
    /// Positive distance from the start in points.
    pub value: f32,
    /// Maximum positive distance.
    pub max: f32,
    /// Viewport length in points.
    pub page: f32,
}
#[derive(Clone, Copy)]
struct Capture {
    entity: Entity,
    target: Entity,
    axis: Axis,
    id: u64,
    grab: f32,
    focus: Option<Entity>,
}
#[derive(Default)]
struct CaptureState(Option<Capture>);

/// Whether this thumb currently owns a held pointer drag (not keyboard focus).
pub fn scrollbar_dragging(world: &World, entity: Entity) -> bool {
    world
        .resource::<CaptureState>()
        .is_some_and(|s| s.0.is_some_and(|c| c.entity == entity))
}

fn index(axis: Axis) -> usize {
    if axis == Axis::Row {
        0
    } else {
        1
    }
}
fn start(rect: Rect, axis: Axis) -> f32 {
    if index(axis) == 0 {
        rect.x
    } else {
        rect.y
    }
}
fn length(rect: Rect, axis: Axis) -> f32 {
    if index(axis) == 0 {
        rect.w
    } else {
        rect.h
    }
}
fn visible(world: &World, entity: Entity) -> bool {
    crate::interaction_visible(world, entity)
        && crate::interaction::policy(world, entity).0
        && crate::overlay::modal_floor(world)
            .is_none_or(|floor| crate::overlay::layer_of(world, entity) >= floor)
}

/// Current bounded geometry, or None for a hidden, stale or unsupported target.
pub fn scrollbar_geometry(world: &World, entity: Entity) -> Option<ScrollbarGeometry> {
    let bar = world.get::<Scrollbar>(entity)?;
    if !visible(world, entity) || !visible(world, bar.target) {
        return None;
    }
    let rect = *world.get::<Rect>(entity)?;
    let clip = world
        .get::<Clip>(entity)
        .copied()
        .unwrap_or(Clip::from_rect(rect))
        .intersect(Clip::from_rect(rect));
    let track = Rect {
        x: clip.x,
        y: clip.y,
        w: clip.w,
        h: clip.h,
    };
    if track.w <= 0.
        || track.h <= 0.
        || ![track.x, track.y, track.w, track.h]
            .iter()
            .all(|v| v.is_finite())
    {
        return None;
    }
    let port = world.get::<Rect>(bar.target)?;
    let i = index(bar.axis);
    let (content, page, value) = if let Some(editor) = world.get::<Editor>(bar.target) {
        if i != 1 {
            return None;
        }
        let (content, page) = crate::editor::editor_scroll_metrics(world, bar.target)?;
        (content, page, editor.offset)
    } else {
        let scroll = world.get::<Scroll>(bar.target)?;
        (
            scroll.content[i],
            if i == 0 { port.w } else { port.h },
            -scroll.offset[i],
        )
    };
    if !content.is_finite() || !page.is_finite() || !value.is_finite() || page <= 0. {
        return None;
    }
    let max = (content - page).max(0.);
    let value = value.clamp(0., max);
    let span = length(track, bar.axis);
    let thumb_len = (span * page / content.max(page)).max(24.).min(span);
    let pos = start(track, bar.axis)
        + if max > 0. {
            value / max * (span - thumb_len)
        } else {
            0.
        };
    let mut thumb = track;
    if i == 0 {
        thumb.x = pos;
        thumb.w = thumb_len;
    } else {
        thumb.y = pos;
        thumb.h = thumb_len;
    }
    Some(ScrollbarGeometry {
        track,
        thumb,
        value,
        max,
        page,
    })
}

/// Set the positive viewport offset without changing text, selection or history.
/// Returns false for non-finite values or unsupported/stale controls.
pub fn set_scrollbar_value(world: &mut World, entity: Entity, value: f32) -> bool {
    if !value.is_finite() {
        return false;
    }
    let Some(g) = scrollbar_geometry(world, entity) else {
        return false;
    };
    let bar = *world.get::<Scrollbar>(entity).unwrap();
    let value = value.clamp(0., g.max);
    crate::editor::reset_editor_wheel(world, bar.target);
    if let Some(editor) = world.get_mut::<Editor>(bar.target) {
        editor.offset = value;
    } else if let Some(scroll) = world.get_mut::<Scroll>(bar.target) {
        scroll.offset[index(bar.axis)] = -value;
    }
    crate::focus::pause_focus_reveal(world);
    crate::layout_propagate(world);
    true
}

/// Handle tracks before parent scrolling, selection and focus. Captured movement
/// continues outside the track; release/lost focus/stale ownership cancels it.
pub fn apply_scrollbars(world: &mut World) {
    let Some(input) = world.resource::<Input>().cloned() else {
        world.insert_resource(CaptureState::default());
        return;
    };
    let p = input.pointer;
    if crate::events::<crate::WindowFocus>(world).contains(&crate::WindowFocus::Blurred) {
        world.insert_resource(CaptureState::default());
        return;
    }
    let mut capture = world.resource::<CaptureState>().and_then(|s| s.0);
    let hit = crate::hit_test(world, p.position[0], p.position[1])
        .and_then(|e| crate::ancestor_with::<Scrollbar>(world, e));
    let press = p.press_at.unwrap_or(p.position);
    let press_hit = crate::hit_test(world, press[0], press[1])
        .and_then(|e| crate::ancestor_with::<Scrollbar>(world, e));
    let ime_busy = !input.ime_preedit.is_empty()
        || !input.ime_commit.is_empty()
        || world
            .resource::<Focus>()
            .and_then(|f| f.entity)
            .and_then(|e| crate::editing::attached_source(world, e))
            .and_then(|d| world.resource::<crate::editing::SourceDocuments>()?.get(d))
            .is_some_and(|session| session.is_composing());
    if ime_busy {
        // A new focus target must not make source preparation discard a native
        // commit delivered in the same frame as a track press. Candidate keys
        // and composition remain with their original text owner.
        world.insert_resource(CaptureState::default());
        if p.pressed && press_hit.is_some() {
            if let Some(input) = world.resource_mut::<Input>() {
                input.pointer.pressed = false;
            }
        }
        return;
    }
    let pointer_owned = capture.is_some() || (p.pressed && press_hit.is_some());
    let mut owned = pointer_owned || hit.is_some();
    if let Some(c) = capture {
        let valid = world.resource::<Focus>().and_then(|f| f.entity) == c.focus
            && p.id == c.id
            && p.button == PointerButton::Left
            && world
                .get::<Scrollbar>(c.entity)
                .is_some_and(|b| b.target == c.target && b.axis == c.axis);
        if valid && (p.down || p.released) {
            if let Some(g) = scrollbar_geometry(world, c.entity) {
                let travel = length(g.track, c.axis) - length(g.thumb, c.axis);
                if travel > 0. {
                    set_scrollbar_value(
                        world,
                        c.entity,
                        (p.position[index(c.axis)] - c.grab - start(g.track, c.axis)) / travel
                            * g.max,
                    );
                }
            }
        }
        if !valid || !p.down || p.released || scrollbar_geometry(world, c.entity).is_none() {
            world.insert_resource(CaptureState::default());
            capture = None;
        }
    }
    if capture.is_none() && p.pressed && p.button == PointerButton::Left {
        if let Some(entity) = press_hit {
            if let Some(g) = scrollbar_geometry(world, entity) {
                let bar = *world.get::<Scrollbar>(entity).unwrap();
                // Pointer scrolling preserves the current text/keyboard owner.
                // Tab and accessibility can still focus the track explicitly.
                let focus = world.resource::<Focus>().and_then(|f| f.entity);
                let coord = p.press_at.unwrap_or(p.position)[index(bar.axis)];
                if coord >= start(g.thumb, bar.axis)
                    && coord < start(g.thumb, bar.axis) + length(g.thumb, bar.axis)
                {
                    let grab = coord - start(g.thumb, bar.axis);
                    let travel = length(g.track, bar.axis) - length(g.thumb, bar.axis);
                    if travel > 0.0 && (p.down || p.released) {
                        set_scrollbar_value(
                            world,
                            entity,
                            (p.position[index(bar.axis)] - grab - start(g.track, bar.axis))
                                / travel
                                * g.max,
                        );
                    }
                    if p.down && !p.released {
                        world.insert_resource(CaptureState(Some(Capture {
                            entity,
                            target: bar.target,
                            axis: bar.axis,
                            id: p.id,
                            grab,
                            focus,
                        })));
                    }
                } else {
                    set_scrollbar_value(
                        world,
                        entity,
                        g.value
                            + if coord < start(g.thumb, bar.axis) {
                                -g.page
                            } else {
                                g.page
                            },
                    );
                }
            }
        }
    }
    if let Some(entity) = world
        .resource::<Focus>()
        .and_then(|f| f.entity)
        .filter(|e| world.has::<Scrollbar>(*e))
    {
        if let Some(g) = scrollbar_geometry(world, entity) {
            let axis = world.get::<Scrollbar>(entity).unwrap().axis;
            let mut value = g.value;
            let mut consumed = Vec::new();
            for (event_index, event) in input.keys.iter().enumerate() {
                if event.synthetic
                    || event.released
                    || event.modifiers.ctrl
                    || event.modifiers.alt
                    || event.modifiers.super_key
                {
                    continue;
                }
                let delta = match event.logical {
                    crate::LogicalKey::Named(NamedKey::Home) => Some(-g.max),
                    crate::LogicalKey::Named(NamedKey::End) => Some(g.max),
                    crate::LogicalKey::Named(NamedKey::PageUp) => Some(-g.page),
                    crate::LogicalKey::Named(NamedKey::PageDown) => Some(g.page),
                    crate::LogicalKey::Named(NamedKey::ArrowUp) if index(axis) == 1 => Some(-40.),
                    crate::LogicalKey::Named(NamedKey::ArrowDown) if index(axis) == 1 => Some(40.),
                    crate::LogicalKey::Named(NamedKey::ArrowLeft) if index(axis) == 0 => Some(-40.),
                    crate::LogicalKey::Named(NamedKey::ArrowRight) if index(axis) == 0 => Some(40.),
                    _ => None,
                };
                if let Some(delta) = delta {
                    value = (value + delta).clamp(0., g.max);
                    consumed.push(event_index);
                }
            }
            if !consumed.is_empty() {
                set_scrollbar_value(world, entity, value);
                let platform = crate::platform_of(world);
                if let Some(i) = world.resource_mut::<Input>() {
                    let mut n = 0;
                    i.keys.retain(|_| {
                        let keep = !consumed.contains(&n);
                        n += 1;
                        keep
                    });
                    i.key = i
                        .keys
                        .iter()
                        .find(|e| !e.released)
                        .and_then(|e| crate::legacy_key(e, platform));
                }
            }
        }
    }
    if let Some(entity) = hit {
        if input.scroll != [0., 0.] {
            if let Some(g) = scrollbar_geometry(world, entity) {
                let axis = world.get::<Scrollbar>(entity).unwrap().axis;
                let target = world.get::<Scrollbar>(entity).unwrap().target;
                if world.has::<Editor>(target) {
                    crate::editor::scroll_editor_wheel(world, target, input.scroll[index(axis)]);
                } else {
                    set_scrollbar_value(world, entity, g.value - input.scroll[index(axis)]);
                }
            }
        }
    }
    owned |= world
        .resource::<CaptureState>()
        .is_some_and(|s| s.0.is_some());
    if owned {
        if let Some(i) = world.resource_mut::<Input>() {
            if pointer_owned {
                i.pointer.pressed = false;
            }
            i.scroll = [0., 0.];
        }
        if pointer_owned {
            world.insert_resource(crate::ScrollDrag::default());
        }
    }
}
