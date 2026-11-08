//! Invariants every stored document satisfies. They match Medousa Create's
//! so documents move between the two without loss or refusal.

use crate::error::{invalid, CanvasError, Result};
use crate::schema::{Asset, Content, Document, Scene, ShapeKind};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_SCENE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_NODES: usize = 10_000;
pub const MAX_INK_POINTS: usize = 250_000;
pub const MAX_ASSET_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_ASSETS: usize = 256;
pub const MAX_TOTAL_ASSET_BYTES: u128 = 512 * 1024 * 1024;
pub const MAX_ORIGIN_BYTES: usize = 16 * 1024;
/// Bitmap types accepted as assets.
pub const ASSET_MIMES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];

/// Ids are 1 to 128 ASCII letters, digits, `_` or `-`.
pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(invalid("invalid identifier"));
    }
    Ok(())
}

/// Accepts `transparent` and `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`.
pub fn validate_paint(value: &str) -> Result<()> {
    if value == "transparent"
        || (matches!(value.len(), 4 | 5 | 7 | 9)
            && value.starts_with('#')
            && value[1..].bytes().all(|c| c.is_ascii_hexdigit()))
    {
        Ok(())
    } else {
        Err(invalid("paint must be a hex color or transparent"))
    }
}

fn bounded_text(value: &str, max: usize, field: &str) -> Result<()> {
    if value.len() > max || value.contains('\0') {
        return Err(invalid(format!("invalid {field}")));
    }
    Ok(())
}

fn number(value: f64, min: f64, max: f64) -> Result<()> {
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(invalid("numeric value out of bounds"));
    }
    Ok(())
}

/// Checks a scene against the document's assets.
pub fn validate_scene(scene: &Scene, assets: &BTreeMap<String, Asset>) -> Result<()> {
    if !scene.is_supported() {
        return Err(CanvasError::Unsupported);
    }
    if scene.nodes.len() > MAX_NODES
        || serde_json::to_vec(scene)
            .map_err(|e| invalid(e.to_string()))?
            .len()
            > MAX_SCENE_BYTES
    {
        return Err(invalid("scene exceeds capacity"));
    }
    validate_paint(&scene.background)?;
    let mut nodes = BTreeMap::new();
    let mut total_points = 0usize;
    for node in &scene.nodes {
        validate_id(&node.id)?;
        if let Some(name) = &node.name {
            bounded_text(name, 256, "layer name")?;
            if name.trim().is_empty() {
                return Err(invalid("layer name is empty"));
            }
        }
        if nodes.insert(node.id.as_str(), node).is_some() {
            return Err(invalid("duplicate node id"));
        }
        let t = &node.transform;
        number(t.x, -1e9, 1e9)?;
        number(t.y, -1e9, 1e9)?;
        number(t.width, 0., 1e7)?;
        number(t.height, 0., 1e7)?;
        number(t.rotation, -360_000., 360_000.)?;
        number(node.opacity, 0., 1.)?;
        if let Some(origin) = &node.origin {
            if !matches!(node.content, Content::Frame { .. }) {
                return Err(invalid("design source requires a frame"));
            }
            if serde_json::to_vec(origin)
                .map_err(|e| invalid(e.to_string()))?
                .len()
                > MAX_ORIGIN_BYTES
            {
                return Err(invalid("design source exceeds capacity"));
            }
        }
        if let Some(appearance) = &node.appearance {
            if !matches!(
                node.content,
                Content::Shape {
                    shape: ShapeKind::Rectangle | ShapeKind::Ellipse,
                    ..
                } | Content::Frame { .. }
                    | Content::Image { .. }
            ) {
                return Err(invalid(
                    "surface styling requires a rectangle, ellipse, frame or image",
                ));
            }
            number(appearance.corner_radius, 0., 1e7)?;
            if let Some(shadow) = &appearance.shadow {
                number(shadow.x, -1000., 1000.)?;
                number(shadow.y, -1000., 1000.)?;
                number(shadow.blur, 0., 128.)?;
                number(shadow.opacity, 0., 1.)?;
                validate_paint(&shadow.color)?;
            }
        }
        if let Some(layout) = &node.text_layout {
            let Content::Text { text, .. } = &node.content else {
                return Err(invalid("text layout requires text"));
            };
            if let Some(width) = layout.width {
                number(width, 4., 1e7)?;
            }
            number(layout.line_height, 0.8, 4.)?;
            if let Some(v) = layout.letter_spacing {
                number(v, -100., 100.)?;
            }
            if let Some(v) = layout.paragraph_spacing {
                number(v, 0., 512.)?;
            }
            if layout.spans.len() > 1024 {
                return Err(invalid("too many text spans"));
            }
            let length = text.encode_utf16().count();
            let mut boundaries = BTreeSet::from([0_u32]);
            let mut position = 0;
            for ch in text.chars() {
                position += ch.len_utf16() as u32;
                boundaries.insert(position);
            }
            let mut end = 0;
            for span in &layout.spans {
                if span.start < end
                    || span.end <= span.start
                    || span.end as usize > length
                    || !boundaries.contains(&span.start)
                    || !boundaries.contains(&span.end)
                {
                    return Err(invalid("invalid text span range"));
                }
                if span.font_weight.is_some_and(|w| !(100..=900).contains(&w)) {
                    return Err(invalid("invalid text span weight"));
                }
                if let Some(color) = &span.color {
                    validate_paint(color)?;
                }
                end = span.end;
            }
        }
        match &node.content {
            Content::Ink {
                color,
                brush,
                points,
                ..
            } => {
                validate_paint(color)?;
                number(brush.size, 0.25, 160.)?;
                for v in [
                    brush.thinning,
                    brush.smoothing,
                    brush.streamline,
                    brush.opacity,
                ] {
                    number(v, 0., 1.)?;
                }
                total_points = total_points.saturating_add(points.len());
                if points.is_empty() || total_points > MAX_INK_POINTS {
                    return Err(invalid("ink point capacity exceeded"));
                }
                for p in points {
                    number(p.x, -1e9, 1e9)?;
                    number(p.y, -1e9, 1e9)?;
                    if let Some(v) = p.pressure {
                        number(v, 0., 1.)?;
                    }
                    if let Some(v) = p.elapsed_ms {
                        number(v, 0., 86_400_000.)?;
                    }
                    for v in [p.tilt_x, p.tilt_y].into_iter().flatten() {
                        number(v, -90., 90.)?;
                    }
                }
            }
            Content::Text {
                text,
                color,
                font_family,
                font_size,
                font_weight,
                ..
            } => {
                bounded_text(text, 100_000, "text")?;
                validate_paint(color)?;
                bounded_text(font_family, 128, "font family")?;
                if font_family.is_empty()
                    || !font_family
                        .chars()
                        .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
                {
                    return Err(invalid("invalid font family"));
                }
                number(*font_size, 1., 4096.)?;
                if let Some(weight) = font_weight {
                    number(f64::from(*weight), 100., 900.)?;
                }
            }
            Content::Shape {
                shape,
                fill,
                stroke,
                stroke_width,
                line,
            } => {
                validate_paint(fill)?;
                validate_paint(stroke)?;
                number(*stroke_width, 0., 1000.)?;
                if let Some(line) = line {
                    if *shape != ShapeKind::Line {
                        return Err(invalid("line geometry requires a line shape"));
                    }
                    for point in [&line.start, &line.end] {
                        number(point.x, -1e9, 1e9)?;
                        number(point.y, -1e9, 1e9)?;
                    }
                }
            }
            Content::Image { asset_id, crop } => {
                validate_id(asset_id)?;
                if !assets.contains_key(asset_id) {
                    return Err(invalid("image references an unknown asset"));
                }
                if let Some(c) = crop {
                    number(c.x, 0., 1.)?;
                    number(c.y, 0., 1.)?;
                    number(c.width, f64::EPSILON, 1.)?;
                    number(c.height, f64::EPSILON, 1.)?;
                    if c.x + c.width > 1. + 1e-9 || c.y + c.height > 1. + 1e-9 {
                        return Err(invalid("crop extends outside image"));
                    }
                }
            }
            Content::Frame { name, background } => {
                bounded_text(name, 256, "frame name")?;
                validate_paint(background)?;
            }
            Content::Group => {}
        }
    }
    // Each parent chain is walked once, so deep but valid nesting stays linear.
    let mut done = BTreeSet::new();
    for id in nodes.keys() {
        let mut path = BTreeSet::new();
        let mut cursor = Some(*id);
        while let Some(id) = cursor {
            if done.contains(id) {
                break;
            }
            if !path.insert(id) {
                return Err(invalid("cyclic node hierarchy"));
            }
            let node = nodes.get(id).ok_or_else(|| invalid("missing parent"))?;
            cursor = node.parent_id.as_deref();
            if let Some(parent) = cursor {
                let parent = nodes.get(parent).ok_or_else(|| invalid("missing parent"))?;
                if !parent.content.is_container() {
                    return Err(invalid("parent must be a group or frame"));
                }
            }
        }
        done.extend(path);
    }
    Ok(())
}

/// Checks an asset's id, digest, type and size.
pub fn validate_asset(id: &str, asset: &Asset) -> Result<()> {
    if id != asset.id
        || asset.sha256.len() != 64
        || !asset.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        || id != format!("a_{}", asset.sha256)
        || asset.byte_size == 0
        || asset.byte_size > MAX_ASSET_BYTES as u64
        || !ASSET_MIMES.contains(&asset.mime.as_str())
    {
        return Err(invalid("invalid asset"));
    }
    Ok(())
}

pub fn validate_document(doc: &Document) -> Result<()> {
    validate_id(&doc.id)?;
    bounded_text(&doc.title, 256, "title")?;
    if doc.title.trim().is_empty() {
        return Err(invalid("title is empty"));
    }
    if doc.assets.len() > MAX_ASSETS
        || doc
            .assets
            .values()
            .map(|a| u128::from(a.byte_size))
            .sum::<u128>()
            > MAX_TOTAL_ASSET_BYTES
    {
        return Err(invalid("asset capacity exceeded"));
    }
    for (id, asset) in &doc.assets {
        validate_asset(id, asset)?;
    }
    validate_scene(&doc.scene, &doc.assets)
}
