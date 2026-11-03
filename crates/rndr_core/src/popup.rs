//! Small retained popup scope integrated with canonical input and layout.
use crate::*;

/// A detached Layout/Scroll root anchored to a live control. Hidden when closed.
/// Call `open_popup` / `close_popup`; do not install a parallel input system.
#[derive(Clone, Copy)]
pub struct AnchoredPopup {
    /// Logical owner; also joins focus boundaries and inherited access policy.
    pub anchor: Entity,
    /// Preferred maximum height in logical points (viewport remains authoritative).
    pub max_height: f32,
    /// Notification after closing; use to reconcile trigger semantics.
    pub closed: fn(&mut World, Entity),
}
impl Component for AnchoredPopup {}
/// Logical focus ownership only, not layout/paint parenting.
#[derive(Clone, Copy)]
pub struct FocusOwner(pub Entity);
impl Component for FocusOwner {}
#[derive(Default)]
struct SwallowRelease(bool);

pub(crate) fn active(world: &World) -> Option<Entity> {
    world
        .entities_with::<AnchoredPopup>()
        .into_iter()
        .find(|e| !world.has::<Hidden>(*e))
}
/// Open one popup, replacing any previous scope. Caller then focuses its row/surface.
/// Rejects unavailable anchors and popups below a higher modal layer.
pub fn open_popup(world: &mut World, root: Entity) -> bool {
    let Some(p) = world.get::<AnchoredPopup>(root).copied() else {
        return false;
    };
    if !crate::interaction::policy(world, p.anchor).0
        || crate::interaction::policy(world, p.anchor).1
    {
        return false;
    }
    let layer = crate::overlay::layer_of(world, p.anchor);
    if crate::overlay::modal_floor(world).is_some_and(|floor| floor > layer) {
        return false;
    }
    if let Some(old) = active(world) {
        if old == root {
            return true;
        }
        close_popup(world, old, true);
    }
    reveal_entity(world, p.anchor);
    world.insert(root, FocusOwner(p.anchor));
    world.insert(root, Layer(layer.saturating_add(1)));
    world.insert(root, Modal);
    world.remove::<Hidden>(root);
    cancel_activation(world);
    true
}
/// Close without rollback. Return focus only if requested and the owner remains eligible.
pub fn close_popup(world: &mut World, root: Entity, return_focus: bool) {
    let Some(p) = world.get::<AnchoredPopup>(root).copied() else {
        return;
    };
    if world.has::<Hidden>(root) {
        return;
    }
    world.remove::<Modal>(root);
    world.remove::<Layer>(root);
    cancel_activation(world);
    if return_focus {
        request_focus(world, p.anchor);
    }
    world.insert(root, Hidden);
    (p.closed)(world, root);
}
pub(crate) fn prepare(world: &mut World) {
    if !world.has_resource::<SwallowRelease>() {
        world.insert_resource(SwallowRelease::default());
    }
    if world.resource::<SwallowRelease>().unwrap().0 {
        if let Some(input) = world.resource_mut::<Input>() {
            let ended = input.pointer.released || !input.pointer.down;
            input.pointer.pressed = false;
            input.pointer.released = false;
            input.scroll = [0.0, 0.0];
            if ended {
                world.resource_mut::<SwallowRelease>().unwrap().0 = false;
            }
        }
    }
    let Some(root) = active(world) else {
        // Detached roots must not leak when their anchor was destroyed.
        for e in world.entities_with::<AnchoredPopup>() {
            if world
                .get::<AnchoredPopup>(e)
                .is_some_and(|p| !world.is_alive(p.anchor))
            {
                despawn(world, e);
            }
        }
        return;
    };
    let p = *world.get::<AnchoredPopup>(root).unwrap();
    let (enabled, readonly) = crate::interaction::policy(world, p.anchor);
    let blurred = events::<WindowFocus>(world).contains(&WindowFocus::Blurred);
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    let inside_focus =
        focus.is_some_and(|e| e == root || ancestor_with::<AnchoredPopup>(world, e) == Some(root));
    let covered = crate::overlay::modal_floor(world)
        .is_some_and(|floor| floor > crate::overlay::layer_of(world, root));
    let clipped = world
        .get::<Clip>(p.anchor)
        .is_some_and(|r| r.w <= 0.0 || r.h <= 0.0);
    if !enabled || readonly || blurred || !inside_focus || covered || clipped {
        close_popup(world, root, !blurred && inside_focus);
        if let Some(i) = world.resource_mut::<Input>() {
            let contact = i.pointer.pressed || i.pointer.released || i.pointer.down;
            let ended = i.pointer.released || !i.pointer.down;
            i.pointer.pressed = false;
            i.pointer.released = false;
            i.scroll = [0.0, 0.0];
            if contact {
                world.resource_mut::<SwallowRelease>().unwrap().0 = !ended;
            }
        }
        return;
    }
    let outside = world.resource::<Input>().is_some_and(|i| {
        let at = i.pointer.press_at.unwrap_or(i.pointer.position);
        i.pointer.pressed
            && !world
                .get::<Rect>(root)
                .is_some_and(|r| r.contains(at[0], at[1]))
    });
    if outside {
        close_popup(world, root, true);
        cancel_activation(world);
        let i = world.resource_mut::<Input>().unwrap();
        let ended = i.pointer.released || !i.pointer.down;
        i.pointer.pressed = false;
        i.pointer.released = false;
        i.scroll = [0.0, 0.0];
        world.resource_mut::<SwallowRelease>().unwrap().0 = !ended;
    }
}
/// Handle scope keys in the existing chronological key loop, before focused widgets.
pub(crate) fn key(world: &mut World, event: &KeyEvent) -> bool {
    let Some(root) = active(world) else {
        return false;
    };
    let dismissal = matches!(
        event.logical,
        LogicalKey::Named(NamedKey::Escape | NamedKey::Tab)
    );
    if dismissal
        && (event.synthetic
            || event.repeat
            || event.released
            || event.modifiers.command()
            || event.modifiers.ctrl
            || event.modifiers.alt)
    {
        return true;
    }
    match event.logical {
        LogicalKey::Named(NamedKey::Escape) => {
            close_popup(world, root, true);
            true
        }
        LogicalKey::Named(NamedKey::Tab) => {
            close_popup(world, root, true);
            false
        }
        _ => false,
    }
}

/// Viewport-aware anchored rectangle. Insets are respected; prefers below then above.
/// Height is constrained to the larger available side; width never exceeds viewport.
pub fn popup_rect(anchor: Rect, viewport: Viewport, width: f32, height: f32) -> Rect {
    let left = viewport.insets.left;
    let top = viewport.insets.top;
    let right = left + viewport.content_width().max(0.0);
    let bottom = top + viewport.content_height().max(0.0);
    // Owners may move offscreen between scroll and layout; never place a
    // transient popup beyond the viewport while the next input closes it.
    let anchor_top = anchor.y.clamp(top, bottom);
    let anchor_bottom = (anchor.y + anchor.h).clamp(top, bottom);
    let below = (bottom - anchor_bottom - 4.0).max(0.0);
    let above = (anchor_top - top - 4.0).max(0.0);
    let down = height <= below || below >= above;
    let h = height.max(0.0).min(if down { below } else { above });
    let w = width.max(0.0).min((right - left).max(0.0));
    Rect {
        x: anchor.x.clamp(left, (right - w).max(left)),
        y: if down {
            (anchor_bottom + 4.0).clamp(top, (bottom - h).max(top))
        } else {
            (anchor_top - 4.0 - h).clamp(top, (bottom - h).max(top))
        },
        w,
        h,
    }
}
