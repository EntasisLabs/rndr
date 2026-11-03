//! Component-targeted, genuinely offscreen capture. See `docs/component-capture.md`.
//!
//! Pixels are the viewport composite in the target's visible layout box (not an
//! isolated repaint of the subtree). Siblings/overlays may occlude it. No window,
//! event loop, native surface, socket, or screenshot service is created.
use crate::{observe, ExportError, Exporter, RgbaImage};
use rndr_core::{
    children_of, layered_roots, App, Clip, Entity, Hidden, Parent, Rect, SurfaceSlot, Viewport,
    World,
};
use serde_json::{json, Value};

/// Maximum visited hierarchy nodes, including identity lookup. Fail closed.
pub const MAX_NODES: usize = 4096;

/// Generation-checked target or a caller key resolved at the paint boundary.
#[derive(Clone, Copy, Debug)]
pub enum Target {
    /// An exact live ECS handle. Never accepts a recycled slot's new generation.
    Entity(Entity),
    /// A UI key, supplied by a read-only identity adapter (for example
    /// `|w, e| rndr_ui::identity(w, e).map(|id| id.0)`).
    UiId {
        /// Numeric caller key, not an entity index or accessibility id.
        id: u64,
        /// Direct parent scope. None searches globally and rejects duplicates.
        parent: Option<Entity>,
        /// Optional previous resolution. Rejects remounts instead of retargeting.
        expected: Option<Entity>,
    },
}

/// A public geometry/identity record; deliberately excludes all text and values.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Full generation-bearing ECS handle.
    pub entity: Entity,
    /// Direct parent at paint time, including a parent outside the subtree.
    pub parent: Option<Entity>,
    /// Caller key, if the identity adapter recognizes a component root.
    pub ui_id: Option<u64>,
    /// Layout box in global logical points; absent on non-layout nodes.
    pub bounds: Option<Rect>,
    /// Effective clip after layout, in global logical points.
    pub clip: Option<Clip>,
    /// This node or an ancestor is Hidden.
    pub hidden: bool,
}

/// Same-paint geometry and the outward-rounded physical extraction rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct Metadata {
    /// Selected entity (includes its generation).
    pub target: Entity,
    /// Viewport dimensions in logical points.
    pub viewport: [f32; 2],
    /// Physical pixels per logical point; invalid raw scales are rejected.
    pub scale: f32,
    /// [x, y, width, height] in the offscreen viewport's physical pixels.
    pub pixel_rect: [u32; 4],
    /// Target first, then descendants in hierarchy order. Includes hidden nodes.
    pub nodes: Vec<Node>,
}
impl Metadata {
    /// Versioned, text-free JSON. Numeric keys are serialized as decimal strings
    /// so JavaScript consumers do not truncate u64 identities.
    pub fn to_json(&self) -> Value {
        fn entity(e: Entity) -> Value {
            json!({"index":e.index(), "generation":e.generation()})
        }
        json!({"protocol":"rndr-component-capture-1", "renderer":"headless_offscreen_viewport_composite",
            "target":entity(self.target), "viewport_points":self.viewport, "scale":self.scale,
            "pixel_rect":self.pixel_rect, "nodes":self.nodes.iter().map(|n| json!({
                "entity":entity(n.entity), "parent":n.parent.map(entity),
                "ui_id":n.ui_id.map(|id| id.to_string()),
                "bounds_points":n.bounds.map(|r| [r.x,r.y,r.w,r.h]),
                "clip_points":n.clip.map(|r| [r.x,r.y,r.w,r.h]), "hidden":n.hidden
            })).collect::<Vec<_>>()})
    }
}

/// A successful capture. Errors never return partial pixels or metadata.
#[derive(Clone, Debug)]
pub struct Capture {
    /// Straight RGBA, top row first, in the target's visible box.
    pub image: RgbaImage,
    /// Layout and identity read at the same paint boundary as the image.
    pub metadata: Metadata,
}

/// Stable failure categories. Display never includes application content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Missing public attestation or any sensitive marker anywhere in the world.
    Sensitive,
    /// Foreign surfaces cannot be reproduced offscreen.
    ForeignSurface,
    /// No matching key.
    NotFound,
    /// More than one matching key (supply a direct parent scope).
    Ambiguous,
    /// Dead Entity/parent, or a pinned key resolved to a different entity.
    Stale,
    /// Missing/nonfinite/negative geometry or invalid viewport/scale.
    Geometry,
    /// Hidden, clipped out, zero size, or entirely outside the viewport.
    Empty,
    /// Hierarchy cycle, duplicate traversal, or node/metadata size cap exceeded.
    Limit,
    /// Identity, hierarchy, geometry or accessibility changed during Render.
    Changed,
    /// A painted image has no retained CPU pixels. Reload with `load_rgba`;
    /// native-only GPU textures and unfinished decodes cannot be reproduced.
    /// Also returned if the GPU image budget cannot retain every painted image.
    ImageUnavailable,
    /// Offscreen adapter, GPU, size cap or readback failure.
    Export(ExportError),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Sensitive => "sensitive_frame",
                Self::ForeignSurface => "foreign_surface_unsupported",
                Self::NotFound => "target_not_found",
                Self::Ambiguous => "target_ambiguous",
                Self::Stale => "stale_identity",
                Self::Geometry => "invalid_geometry",
                Self::Empty => "empty_target",
                Self::Limit => "capture_limit",
                Self::Changed => "post_paint_mutation",
                Self::ImageUnavailable => "image_unavailable",
                Self::Export(_) => "headless_render_failed",
            }
        )
    }
}
impl std::error::Error for Error {}
impl From<ExportError> for Error {
    fn from(e: ExportError) -> Self {
        Self::Export(e)
    }
}

fn permitted(world: &World) -> Result<(), Error> {
    if !observe::public(world) {
        return Err(Error::Sensitive);
    }
    if !world.entities_with::<SurfaceSlot>().is_empty() {
        return Err(Error::ForeignSurface);
    }
    Ok(())
}
fn valid(r: Rect) -> bool {
    [r.x, r.y, r.w, r.h, r.x + r.w, r.y + r.h]
        .iter()
        .all(|x| x.is_finite())
        && r.w >= 0.0
        && r.h >= 0.0
}
fn hierarchy(world: &World, roots: Vec<Entity>) -> Result<Vec<Entity>, Error> {
    let mut stack: Vec<_> = roots.into_iter().rev().collect();
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    while let Some(e) = stack.pop() {
        if out.len() >= MAX_NODES || !seen.insert(e) {
            return Err(Error::Limit);
        }
        out.push(e);
        stack.extend(children_of(world, e).into_iter().rev());
    }
    Ok(out)
}
fn hidden(world: &World, mut e: Entity) -> Result<bool, Error> {
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(e) || seen.len() > MAX_NODES {
            return Err(Error::Limit);
        }
        if world.has::<Hidden>(e) {
            return Ok(true);
        }
        match world.get::<Parent>(e) {
            Some(p) => e = p.entity,
            None => return Ok(false),
        }
    }
}
fn metadata(
    world: &World,
    target: Target,
    ids: &impl Fn(&World, Entity) -> Option<u64>,
) -> Result<Metadata, Error> {
    permitted(world)?;
    let v = world.resource::<Viewport>().ok_or(Error::Geometry)?;
    if !v.scale.is_finite()
        || v.scale <= 0.0
        || !v.width.is_finite()
        || !v.height.is_finite()
        || v.width <= 0.0
        || v.height <= 0.0
    {
        return Err(Error::Geometry);
    }
    let entity = match target {
        Target::Entity(e) => {
            if !world.is_alive(e) {
                return Err(Error::Stale);
            }
            e
        }
        Target::UiId {
            id,
            parent,
            expected,
        } => {
            if parent.is_some_and(|p| !world.is_alive(p))
                || expected.is_some_and(|e| !world.is_alive(e))
            {
                return Err(Error::Stale);
            }
            let candidates = if let Some(p) = parent {
                children_of(world, p)
            } else {
                hierarchy(world, layered_roots(world))?
            };
            if candidates.len() > MAX_NODES {
                return Err(Error::Limit);
            }
            let mut matches = candidates
                .into_iter()
                .filter(|e| ids(world, *e) == Some(id));
            let e = matches.next().ok_or(Error::NotFound)?;
            if matches.next().is_some() {
                return Err(Error::Ambiguous);
            }
            if expected.is_some_and(|old| old != e) {
                return Err(Error::Stale);
            }
            e
        }
    };
    let nodes = hierarchy(world, vec![entity])?
        .into_iter()
        .map(|e| {
            let bounds = world.get::<Rect>(e).copied();
            let clip = world.get::<Clip>(e).copied();
            if bounds.is_some_and(|r| !valid(r))
                || clip.is_some_and(|r| {
                    !valid(Rect {
                        x: r.x,
                        y: r.y,
                        w: r.w,
                        h: r.h,
                    })
                })
            {
                return Err(Error::Geometry);
            }
            Ok(Node {
                entity: e,
                parent: world.get::<Parent>(e).map(|p| p.entity),
                ui_id: ids(world, e),
                bounds,
                clip,
                hidden: hidden(world, e)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let root = &nodes[0];
    let bounds = root.bounds.ok_or(Error::Geometry)?;
    if root.hidden {
        return Err(Error::Empty);
    }
    let visible = root
        .clip
        .unwrap_or_else(|| Clip::from_rect(bounds))
        .intersect(Clip::from_rect(bounds))
        .intersect(Clip {
            x: 0.0,
            y: 0.0,
            w: v.width,
            h: v.height,
        });
    if visible.w <= 0.0 || visible.h <= 0.0 {
        return Err(Error::Empty);
    }
    let [vw, vh] = viewport_pixels(v)?;
    let x = (visible.x * v.scale).floor().max(0.0) as u32;
    let y = (visible.y * v.scale).floor().max(0.0) as u32;
    let right = ((visible.x + visible.w) * v.scale).ceil().min(vw as f32) as u32;
    let bottom = ((visible.y + visible.h) * v.scale).ceil().min(vh as f32) as u32;
    if right <= x || bottom <= y {
        return Err(Error::Empty);
    }
    let m = Metadata {
        target: entity,
        viewport: [v.width, v.height],
        scale: v.scale,
        pixel_rect: [x, y, right - x, bottom - y],
        nodes,
    };
    if m.to_json().to_string().len() > observe::MAX_JSON {
        return Err(Error::Limit);
    }
    Ok(m)
}
fn viewport_pixels(v: &Viewport) -> Result<[u32; 2], Error> {
    let w = (v.width * v.scale).ceil();
    let h = (v.height * v.scale).ceil();
    if w > observe::MAX_SIDE as f32
        || h > observe::MAX_SIDE as f32
        || w * h > observe::MAX_PIXELS as f32
    {
        return Err(Error::Limit);
    }
    Ok([w as u32, h as u32])
}

/// Advance exactly one app frame, render offscreen at its paint boundary, then
/// return target pixels and subtree metadata atomically. Reuse `exporter` for
/// repeated captures. An Entity-only caller may pass `|_, _| None` for `ids`.
///
/// `ids` must be a pure read-only mapping, with keys unique under direct parents.
/// Render systems still run, even if paint-time capture fails. Sensitive frames
/// are denied before update, at paint, and after Render; no partial result escapes.
/// Resource-backed GPU submission/readback finishes *before* Render can mutate
/// atlases or images. After Render, changed target metadata/accessibility denies
/// the result rather than returning stale identity. No second paint is performed.
pub fn capture(
    app: &mut App,
    exporter: &mut Exporter,
    target: Target,
    ids: impl Fn(&World, Entity) -> Option<u64>,
) -> Result<Capture, Error> {
    permitted(app.world())?;
    let (_, result) = rndr_paint::frame_with_snapshot_frame(app, |world, frame| {
        let m = metadata(world, target, &ids)?;
        let access = rndr_core::access_tree(world);
        if access.nodes.len() > MAX_NODES {
            return Err(Error::Limit);
        }
        let semantics = (
            observe::semantic_details(world, &access),
            observe::scroll_state(world),
            observe::theme(world).to_owned(),
        );
        let engine = world
            .resource::<std::sync::Arc<dyn rndr_core::TextEngine>>()
            .cloned();
        let epoch = observe::engine_epoch(rndr_core::text_engine(world).as_ref());
        let v = world.resource::<Viewport>().ok_or(Error::Geometry)?;
        let [w, h] = viewport_pixels(v)?;
        let full = exporter
            .render_capture_frame(world, frame, w, h, m.scale)?
            .ok_or(Error::ImageUnavailable)?;
        let [x, y, w, h] = m.pixel_rect;
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for row in y..y + h {
            let start = ((row * full.width + x) * 4) as usize;
            rgba.extend_from_slice(&full.rgba[start..start + (w * 4) as usize]);
        }
        Ok::<_, Error>((
            Capture {
                image: RgbaImage {
                    width: w,
                    height: h,
                    rgba,
                },
                metadata: m,
            },
            access,
            semantics,
            engine,
            epoch,
        ))
    });
    permitted(app.world())?;
    let (captured, access, semantics, engine, epoch) = result?;
    let current = app
        .world()
        .resource::<std::sync::Arc<dyn rndr_core::TextEngine>>();
    let same_engine = match (&engine, current) {
        (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    };
    if !same_engine || observe::engine_epoch(rndr_core::text_engine(app.world()).as_ref()) != epoch
    {
        return Err(Error::Changed);
    }
    if metadata(app.world(), target, &ids).map_err(|_| Error::Changed)? != captured.metadata
        || rndr_core::access_tree(app.world()) != access
        || semantics
            != (
                observe::semantic_details(app.world(), &access),
                observe::scroll_state(app.world()),
                observe::theme(app.world()).to_owned(),
            )
    {
        return Err(Error::Changed);
    }
    Ok(captured)
}

#[cfg(test)]
mod tests;
