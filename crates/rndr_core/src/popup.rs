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
/// Nonmodal informational popup whose anchor retains keyboard/IME ownership.
/// The source controller owns its visibility and key admission. It never participates
/// in generic popup focus, dismissal, or modal capture.
#[derive(Clone, Copy, Debug)]
pub struct PassivePopup;
impl Component for PassivePopup {}
/// Logical focus ownership only, not layout/paint parenting.
#[derive(Clone, Copy)]
pub struct FocusOwner(pub Entity);
impl Component for FocusOwner {}
/// Where an [`AnchoredPopup`] opens relative to its anchor. Without this
/// component it opens below (or above) the anchor at least 240 points wide.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PopupPlacement {
    /// Below the anchor, or above when there is more room there.
    #[default]
    Below,
    /// At a window point, such as a right-click, at the popup's own width.
    At([f32; 2]),
    /// Beside the anchor: right of it, or left when the right side lacks room.
    /// A popup anchored inside another open popup is its submenu.
    Beside,
}
impl Component for PopupPlacement {}
#[derive(Default)]
struct SwallowRelease(bool);

fn open(world: &World, e: Entity) -> bool {
    world.has::<AnchoredPopup>(e) && !world.has::<Hidden>(e) && !world.has::<PassivePopup>(e)
}
/// The open popup containing `root`'s anchor: `root` is its submenu.
pub(crate) fn parent_popup(world: &World, root: Entity) -> Option<Entity> {
    let anchor = world.get::<AnchoredPopup>(root)?.anchor;
    ancestor_with::<AnchoredPopup>(world, anchor).filter(|p| *p != root)
}
fn outermost(world: &World, mut root: Entity) -> Entity {
    for _ in 0..64 {
        match parent_popup(world, root).filter(|p| open(world, *p)) {
            Some(parent) => root = parent,
            None => break,
        }
    }
    root
}
/// `root` and the open popups it is nested in.
fn chain(world: &World, root: Entity) -> Vec<Entity> {
    let mut out = vec![root];
    while let Some(parent) = parent_popup(world, *out.last().unwrap()).filter(|p| open(world, *p)) {
        if out.contains(&parent) || out.len() >= 64 {
            break;
        }
        out.push(parent);
    }
    out
}
/// The innermost open popup scope. Submenus nest inside their parent's scope.
pub(crate) fn active(world: &World) -> Option<Entity> {
    let all: Vec<_> = world
        .entities_with::<AnchoredPopup>()
        .into_iter()
        .filter(|e| open(world, *e))
        .collect();
    all.iter()
        .copied()
        .find(|e| !all.iter().any(|c| parent_popup(world, *c) == Some(*e)))
}
/// Open one popup, replacing any previous scope. Caller then focuses its row/surface.
/// Rejects unavailable anchors and popups below a higher modal layer.
pub fn open_popup(world: &mut World, root: Entity) -> bool {
    let Some(p) = world.get::<AnchoredPopup>(root).copied() else {
        return false;
    };
    if world.has::<PassivePopup>(root) {
        return false;
    }
    if !crate::interaction::policy(world, p.anchor).0
        || crate::interaction::policy(world, p.anchor).1
    {
        return false;
    }
    let layer = crate::overlay::layer_of(world, p.anchor);
    // Open popup scopes are replaced or nest this one, so only other modals block it.
    let blocked = world
        .entities_with::<Modal>()
        .into_iter()
        .filter(|e| !open(world, *e))
        .any(|e| crate::overlay::layer_of(world, e) > layer);
    if blocked {
        return false;
    }
    if let Some(old) = active(world) {
        if old == root {
            return true;
        }
        // Keep the popups this one is nested in; close every other open scope.
        let keep = chain(world, root);
        let nested = keep.len() > 1;
        for e in chain(world, old).into_iter() {
            if !keep.contains(&e) {
                close_popup(world, e, !nested);
            }
        }
    }
    reveal_entity(world, p.anchor);
    world.insert(root, FocusOwner(p.anchor));
    world.insert(root, Layer(layer.saturating_add(1)));
    // The outermost popup's Modal already blocks the app; a nested one leaves
    // its parent popups interactive.
    if parent_popup(world, root).is_some_and(|p| open(world, p)) {
        world.remove::<Modal>(root);
    } else {
        world.insert(root, Modal);
    }
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
    for child in world.entities_with::<AnchoredPopup>() {
        if open(world, child) && parent_popup(world, child) == Some(root) {
            close_popup(world, child, false);
        }
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
    let scope = chain(world, root);
    let outer = *scope.last().unwrap();
    let (enabled, readonly) = crate::interaction::policy(world, p.anchor);
    let blurred = events::<WindowFocus>(world).contains(&WindowFocus::Blurred);
    let focus = world.resource::<Focus>().and_then(|f| f.entity);
    let inside_focus = focus.is_some_and(|e| {
        scope.contains(&e)
            || ancestor_with::<AnchoredPopup>(world, e).is_some_and(|p| scope.contains(&p))
    });
    let covered = crate::overlay::modal_floor(world)
        .is_some_and(|floor| floor > crate::overlay::layer_of(world, root));
    let clipped = world
        .get::<Clip>(p.anchor)
        .is_some_and(|r| r.w <= 0.0 || r.h <= 0.0);
    if !enabled || readonly || blurred || !inside_focus || covered || clipped {
        // A blurred window or a lost scope closes every nested level.
        let close = if blurred || !inside_focus {
            outer
        } else {
            root
        };
        close_popup(world, close, !blurred && inside_focus);
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
            && !scope.iter().any(|e| {
                world
                    .get::<Rect>(*e)
                    .is_some_and(|r| r.contains(at[0], at[1]))
            })
    });
    if outside {
        close_popup(world, outer, true);
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
            close_popup(world, outermost(world, root), true);
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

/// Beside `anchor`: right of it, or left when the right lacks room. The top
/// aligns with the anchor and moves up to stay inside the viewport.
pub(crate) fn beside_rect(anchor: Rect, viewport: Viewport, width: f32, height: f32) -> Rect {
    let left = viewport.insets.left;
    let top = viewport.insets.top;
    let right = left + viewport.content_width().max(0.0);
    let bottom = top + viewport.content_height().max(0.0);
    let w = width.max(0.0).min((right - left).max(0.0));
    let h = height.max(0.0).min((bottom - top).max(0.0));
    let x = if anchor.x + anchor.w + w <= right || anchor.x - w < left {
        anchor.x + anchor.w
    } else {
        anchor.x - w
    };
    Rect {
        x: x.clamp(left, (right - w).max(left)),
        y: (anchor.y - 4.0).clamp(top, (bottom - h).max(top)),
        w,
        h,
    }
}
