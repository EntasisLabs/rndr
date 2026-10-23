//! Draw a [`CreativeScene`](crate::model::CreativeScene) into a display list
//! and into the SVG the Create canvas exports.
//!
//! Both paths share the geometry: the local matrix rotates about the node
//! center, a line's shaft stops short of its arrowheads, and an ink node is
//! the filled quadratic outline of its samples. The SVG uses the same element
//! kinds the canvas writes (`rect`, `ellipse`, `line`, `polygon`, `path`,
//! `text`, `image`, `clipPath`, `feDropShadow`). The display list uses the
//! matching path, layer, and image commands.

use std::collections::BTreeMap;

use base64::Engine;
use rndr_core::{
    Color, FontStyle, Rect, TextAlign, TextSpan, TextStyle, World, FONT_WEIGHT_REGULAR,
};
use rndr_paint::{
    Affine, Canvas, Cap, ClipShape, Effect, FillRule, Path, Scene, SceneLayer, Stroke, TextSource,
};

use crate::model::{
    utf16_to_byte, CreativeAppearance, CreativeBrush, CreativeContent, CreativeFontStyle,
    CreativeLine, CreativeNode, CreativePoint, CreativeScene, CreativeShapeKind, CreativeTextAlign,
    CreativeTextLayout, CreativeTextSpan,
};

/// Loaded pixels for one image asset.
#[derive(Clone, Debug)]
pub struct Asset {
    /// PNG bytes, embedded in the SVG.
    pub png: Vec<u8>,
    /// Decoded width.
    pub width: u32,
    /// Decoded height.
    pub height: u32,
    /// Straight RGBA.
    pub rgba: Vec<u8>,
}

/// Integer pixel rectangle that contains `bounds`.
///
/// Export uses this as both the SVG view box and the offscreen target, so the
/// two rasters share an origin and a size.
pub fn pixel_rect(bounds: Rect) -> Rect {
    if !bounds.w.is_finite() || !bounds.h.is_finite() || bounds.w <= 0.0 || bounds.h <= 0.0 {
        return Rect {
            x: 0.0,
            y: 0.0,
            w: 640.0,
            h: 400.0,
        };
    }
    let x0 = bounds.x.floor();
    let y0 = bounds.y.floor();
    let x1 = (bounds.x + bounds.w).ceil();
    let y1 = (bounds.y + bounds.h).ceil();
    Rect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(1.0),
        h: (y1 - y0).max(1.0),
    }
}

/// Axis-aligned bounds of the visible roots, including stroke and shadow padding.
pub fn scene_bounds(scene: &CreativeScene) -> Rect {
    let rects: Vec<Rect> = scene
        .nodes
        .iter()
        .filter(|node| node.parent_id.is_none())
        .filter_map(|node| visual_bounds(node, &scene.nodes))
        .collect();
    union_rects(&rects).unwrap_or(Rect {
        x: 0.0,
        y: 0.0,
        w: 640.0,
        h: 400.0,
    })
}

/// Record `scene` into `canvas`. The caller owns the camera.
pub fn paint_scene(
    canvas: &mut Canvas,
    world: &World,
    scene: &CreativeScene,
    images: &BTreeMap<String, u64>,
) {
    let background = parse_color(&scene.background);
    if background.a > 0.0 {
        canvas.fill(
            Path::polygon(&[
                [-100_000.0, -100_000.0],
                [100_000.0, -100_000.0],
                [100_000.0, 100_000.0],
                [-100_000.0, 100_000.0],
            ]),
            background,
            FillRule::NonZero,
        );
    }
    for node in scene.nodes.iter().filter(|node| node.parent_id.is_none()) {
        paint_node(canvas, world, node, &scene.nodes, images);
    }
}

/// SVG document for `rect`, with image assets inlined as data URLs.
pub fn scene_svg(scene: &CreativeScene, images: &BTreeMap<String, Asset>, rect: Rect) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\">",
        num(rect.w),
        num(rect.h),
        num(rect.x),
        num(rect.y),
        num(rect.w),
        num(rect.h),
    ));
    let background = parse_color(&scene.background);
    if background.a > 0.0 {
        out.push_str(&format!(
            "<rect x=\"-100000\" y=\"-100000\" width=\"200000\" height=\"200000\" fill=\"{}\"/>",
            xml_attr(&scene.background)
        ));
    }
    for node in scene.nodes.iter().filter(|node| node.parent_id.is_none()) {
        svg_node(&mut out, node, &scene.nodes, images);
    }
    out.push_str("</svg>");
    out
}

/// Build a scene with an identity camera, for offscreen export.
pub fn export_scene(
    world: &World,
    scene: &CreativeScene,
    images: &BTreeMap<String, u64>,
) -> (Scene, Rect) {
    let mut canvas = Canvas::new();
    paint_scene(&mut canvas, world, scene, images);
    (canvas.into_scene(), pixel_rect(scene_bounds(scene)))
}

fn paint_node(
    canvas: &mut Canvas,
    world: &World,
    node: &CreativeNode,
    nodes: &[CreativeNode],
    images: &BTreeMap<String, u64>,
) {
    if !node.visible {
        return;
    }
    let opacity = (node.opacity as f32).clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    canvas.push_transform(local_affine(node));
    let fade = opacity < 0.999;
    if fade {
        canvas.push_layer(SceneLayer::opacity(opacity));
    }
    match &node.content {
        CreativeContent::Frame { background, .. } => {
            let (w, h) = size_of(node);
            let radius = corner_radius(node);
            let path = Path::round_rect(box_rect(w, h), radius);
            let fill = parse_color(background);
            let shadow = shadow_of(node);
            if shadow.is_some() || fill.a > 0.0 {
                push_effect(canvas, shadow, None);
                if fill.a > 0.0 {
                    canvas.fill(path, fill, FillRule::NonZero);
                }
                pop_effect(canvas, shadow, None);
            }
            let clip = ClipShape::RoundRect {
                rect: box_rect(w, h),
                radius,
            };
            canvas.push_layer(SceneLayer {
                opacity: 1.0,
                clip: Some(clip),
                effect: None,
            });
            paint_children(canvas, world, node, nodes, images);
            canvas.pop_layer();
        }
        CreativeContent::Group => paint_children(canvas, world, node, nodes, images),
        CreativeContent::Text {
            text,
            color,
            font_family,
            font_size,
            font_weight,
            font_style,
        } => {
            canvas.text(
                world,
                TextSource {
                    text: text.clone(),
                    style: text_style(node, font_family, *font_size, *font_weight, *font_style),
                    spans: text_spans(node, text, color),
                    max_width: node
                        .text_layout
                        .as_ref()
                        .and_then(|layout| layout.width)
                        .map(|width| width as f32)
                        .unwrap_or(f32::INFINITY),
                    origin: [0.0, 0.0],
                    color: parse_color(color),
                },
            );
        }
        CreativeContent::Shape {
            shape,
            fill,
            stroke,
            stroke_width,
            line,
        } => {
            if let Some(geom) = line_geometry(node) {
                paint_line(canvas, &geom);
            } else if !matches!(shape, CreativeShapeKind::Line) {
                let (w, h) = size_of(node);
                let path = match shape {
                    CreativeShapeKind::Ellipse => {
                        Path::ellipse([w * 0.5, h * 0.5], [w * 0.5, h * 0.5])
                    }
                    _ => Path::round_rect(box_rect(w, h), corner_radius(node)),
                };
                let fill = parse_color(fill);
                let stroke_color = parse_color(stroke);
                let width = (*stroke_width as f32).max(0.0);
                let shadow = shadow_of(node);
                push_effect(canvas, shadow, None);
                stroke_fill(canvas, path, fill, stroke_color, width, Cap::Butt, &[]);
                pop_effect(canvas, shadow, None);
            }
            let _ = line;
        }
        CreativeContent::Ink {
            color,
            brush,
            points,
            ..
        } => {
            if let Some(geom) = line_geometry(node) {
                paint_line(canvas, &geom);
            } else {
                let path = ink_outline(brush, points);
                let mut ink = parse_color(color);
                ink.a *= (brush.opacity as f32).clamp(0.0, 1.0);
                if ink.a > 0.0 && !path.is_empty() {
                    canvas.fill(path, ink, FillRule::NonZero);
                }
            }
        }
        CreativeContent::Image { asset_id, crop } => {
            if let Some(id) = images.get(asset_id).copied() {
                let (w, h) = size_of(node);
                let radius = corner_radius(node);
                let uv = crop_uv(crop);
                let shadow = shadow_of(node);
                let clip = (radius > 0.0).then(|| ClipShape::RoundRect {
                    rect: box_rect(w, h),
                    radius,
                });
                push_effect(canvas, shadow, clip.clone());
                canvas.image(
                    id,
                    box_rect(w, h),
                    uv,
                    if clip.is_some() { 0.0 } else { radius },
                );
                pop_effect(canvas, shadow, clip);
            } else {
                let (w, h) = size_of(node);
                canvas.fill(
                    Path::round_rect(box_rect(w, h), corner_radius(node)),
                    Color::rgba(0.53, 0.53, 0.53, 0.1),
                    FillRule::NonZero,
                );
            }
        }
    }
    if fade {
        canvas.pop_layer();
    }
    canvas.pop_transform();
}

fn paint_children(
    canvas: &mut Canvas,
    world: &World,
    node: &CreativeNode,
    nodes: &[CreativeNode],
    images: &BTreeMap<String, u64>,
) {
    for child in nodes
        .iter()
        .filter(|child| child.parent_id.as_deref() == Some(node.id.as_str()))
    {
        paint_node(canvas, world, child, nodes, images);
    }
}

fn paint_line(canvas: &mut Canvas, line: &LineGeom) {
    let paint = line_paint(line);
    let color = parse_color(&line.color);
    if color.a <= 0.0 || line.stroke_width <= 0.0 {
        return;
    }
    let fade = line.opacity < 0.999;
    if fade {
        canvas.push_layer(SceneLayer::opacity(line.opacity));
    }
    let dash = if line.dashed {
        vec![line.stroke_width * 3.0, line.stroke_width * 2.0]
    } else {
        Vec::new()
    };
    canvas.stroke(
        Path::line(paint.start, paint.end),
        color,
        Stroke::solid(line.stroke_width)
            .with_cap(Cap::Round)
            .dashed(dash),
    );
    for triangle in paint.heads {
        canvas.fill(Path::polygon(&triangle), color, FillRule::NonZero);
    }
    if fade {
        canvas.pop_layer();
    }
}

fn stroke_fill(
    canvas: &mut Canvas,
    path: Path,
    fill: Color,
    stroke: Color,
    width: f32,
    cap: Cap,
    dash: &[f32],
) {
    let draw_fill = fill.a > 0.0;
    let draw_stroke = stroke.a > 0.0 && width > 0.0;
    if draw_fill && draw_stroke {
        canvas.fill_stroke(
            path,
            fill,
            FillRule::NonZero,
            stroke,
            Stroke::solid(width).with_cap(cap).dashed(dash.to_vec()),
        );
    } else if draw_fill {
        canvas.fill(path, fill, FillRule::NonZero);
    } else if draw_stroke {
        canvas.stroke(
            path,
            stroke,
            Stroke::solid(width).with_cap(cap).dashed(dash.to_vec()),
        );
    }
}

fn push_effect(canvas: &mut Canvas, shadow: Option<Effect>, clip: Option<ClipShape>) {
    if shadow.is_none() && clip.is_none() {
        return;
    }
    canvas.push_layer(SceneLayer {
        opacity: 1.0,
        clip,
        effect: shadow,
    });
}

fn pop_effect(canvas: &mut Canvas, shadow: Option<Effect>, clip: Option<ClipShape>) {
    if shadow.is_some() || clip.is_some() {
        canvas.pop_layer();
    }
}

fn svg_node(
    out: &mut String,
    node: &CreativeNode,
    nodes: &[CreativeNode],
    images: &BTreeMap<String, Asset>,
) {
    if !node.visible {
        return;
    }
    let opacity = node.opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let matrix = local_affine(node);
    out.push_str(&format!(
        "<g data-node-id=\"{}\" transform=\"matrix({})\" opacity=\"{}\">",
        xml_attr(&node.id),
        matrix
            .m
            .iter()
            .map(|value| num(*value))
            .collect::<Vec<_>>()
            .join(" "),
        num(opacity as f32),
    ));
    match &node.content {
        CreativeContent::Frame { background, .. } => {
            let (w, h) = size_of(node);
            let radius = corner_radius(node);
            let clip = format!("clip-{}", node.id);
            svg_defs_shadow(out, node, w, h);
            out.push_str(&format!(
                "<defs><clipPath id=\"{}\"><rect width=\"{}\" height=\"{}\" rx=\"{}\"/></clipPath></defs>",
                xml_attr(&clip),
                num(w),
                num(h),
                num(radius),
            ));
            out.push_str(&format!(
                "<rect width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\"{}/>",
                num(w),
                num(h),
                num(radius),
                xml_attr(background),
                filter_attr(node),
            ));
            out.push_str(&format!("<g clip-path=\"url(#{clip})\">"));
            svg_children(out, node, nodes, images);
            out.push_str("</g>");
        }
        CreativeContent::Group => svg_children(out, node, nodes, images),
        CreativeContent::Text { .. } => svg_text(out, node),
        CreativeContent::Shape {
            shape,
            fill,
            stroke,
            stroke_width,
            ..
        } => {
            if let Some(geom) = line_geometry(node) {
                svg_line(out, &geom);
            } else if !matches!(shape, CreativeShapeKind::Line) {
                let (w, h) = size_of(node);
                svg_defs_shadow(out, node, w, h);
                let filter = filter_attr(node);
                match shape {
                    CreativeShapeKind::Ellipse => out.push_str(&format!(
                        "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{}/>",
                        num(w * 0.5),
                        num(h * 0.5),
                        num(w * 0.5),
                        num(h * 0.5),
                        xml_attr(fill),
                        xml_attr(stroke),
                        num(*stroke_width as f32),
                        filter,
                    )),
                    _ => out.push_str(&format!(
                        "<rect width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{}/>",
                        num(w),
                        num(h),
                        num(corner_radius(node)),
                        xml_attr(fill),
                        xml_attr(stroke),
                        num(*stroke_width as f32),
                        filter,
                    )),
                }
            }
        }
        CreativeContent::Ink {
            color,
            brush,
            points,
            ..
        } => {
            if let Some(geom) = line_geometry(node) {
                svg_line(out, &geom);
            } else {
                out.push_str(&format!(
                    "<path d=\"{}\" fill=\"{}\" opacity=\"{}\"/>",
                    xml_attr(&outline_data(brush, points)),
                    xml_attr(color),
                    num((brush.opacity as f32).clamp(0.0, 1.0)),
                ));
            }
        }
        CreativeContent::Image { asset_id, crop } => {
            let (w, h) = size_of(node);
            let radius = corner_radius(node);
            if let Some(asset) = images.get(asset_id) {
                let clip = format!("clip-{asset_id}-{}", node.id);
                svg_defs_shadow(out, node, w, h);
                out.push_str(&format!(
                    "<defs><clipPath id=\"{}\"><rect width=\"{}\" height=\"{}\" rx=\"{}\"/></clipPath></defs>",
                    xml_attr(&clip),
                    num(w),
                    num(h),
                    num(radius),
                ));
                let (vx, vy, vw, vh) = crop
                    .map(|crop| (crop.x, crop.y, crop.width, crop.height))
                    .unwrap_or((0.0, 0.0, 1.0, 1.0));
                let href = format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(&asset.png)
                );
                out.push_str(&format!(
                    "<g{}><g clip-path=\"url(#{})\"><svg width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" preserveAspectRatio=\"none\" overflow=\"hidden\"><image href=\"{}\" width=\"1\" height=\"1\" preserveAspectRatio=\"none\"/></svg></g></g>",
                    filter_attr(node),
                    xml_attr(&clip),
                    num(w),
                    num(h),
                    num(vx as f32),
                    num(vy as f32),
                    num(vw as f32),
                    num(vh as f32),
                    xml_attr(&href),
                ));
            } else {
                out.push_str(&format!(
                    "<rect width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"#88888818\"/>",
                    num(w),
                    num(h),
                    num(radius),
                ));
            }
        }
    }
    out.push_str("</g>");
}

fn svg_children(
    out: &mut String,
    node: &CreativeNode,
    nodes: &[CreativeNode],
    images: &BTreeMap<String, Asset>,
) {
    for child in nodes
        .iter()
        .filter(|child| child.parent_id.as_deref() == Some(node.id.as_str()))
    {
        svg_node(out, child, nodes, images);
    }
}

fn svg_line(out: &mut String, line: &LineGeom) {
    let paint = line_paint(line);
    let dash = if line.dashed {
        format!(
            " stroke-dasharray=\"{} {}\"",
            num(line.stroke_width * 3.0),
            num(line.stroke_width * 2.0)
        )
    } else {
        String::new()
    };
    out.push_str(&format!(
        "<g data-line=\"true\" opacity=\"{}\"><line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"round\"{dash}/>",
        num(line.opacity),
        num(paint.start[0]),
        num(paint.start[1]),
        num(paint.end[0]),
        num(paint.end[1]),
        xml_attr(&line.color),
        num(line.stroke_width),
    ));
    for triangle in paint.heads {
        let points = triangle
            .iter()
            .map(|point| format!("{},{}", num(point[0]), num(point[1])))
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!(
            "<polygon data-arrowhead=\"true\" points=\"{points}\" fill=\"{}\"/>",
            xml_attr(&line.color)
        ));
    }
    out.push_str("</g>");
}

fn svg_text(out: &mut String, node: &CreativeNode) {
    let CreativeContent::Text {
        text,
        color,
        font_family,
        font_size,
        font_weight,
        font_style,
    } = &node.content
    else {
        return;
    };
    let style = *font_style;
    let weight = *font_weight;
    let layout = node.text_layout.as_ref();
    let size = *font_size as f32;
    // Same line box the scene renderer uses when it cannot measure a face:
    // line height is the multiplier times the em size, the em is 0.8 ascent
    // and 0.2 descent, and that box is centered in the line. A live canvas
    // replaces the em box with the browser's CSS baseline. Side bearings are
    // omitted, so a left-aligned run starts at the 6% em padding.
    let line_height = size * layout.map(|item| item.line_height as f32).unwrap_or(1.3);
    let paragraph = layout
        .and_then(|item| item.paragraph_spacing)
        .unwrap_or(0.0) as f32;
    let padding = size * 0.06;
    let ascent = size * 0.8;
    let descent = size * 0.2;
    let baseline = padding + (line_height - ascent - descent) / 2.0 + ascent;
    let letter = layout.and_then(|item| item.letter_spacing).unwrap_or(0.0) as f32;
    let weight = weight.unwrap_or(FONT_WEIGHT_REGULAR);
    let font_style = match style.unwrap_or(CreativeFontStyle::Normal) {
        CreativeFontStyle::Italic => "italic",
        CreativeFontStyle::Normal => "normal",
    };
    let (w, _) = size_of(node);
    out.push_str(&format!(
        "<text fill=\"{}\" style=\"font-family:{};font-size:{}px;font-weight:{weight};font-style:{font_style};white-space:pre;letter-spacing:{}px;dominant-baseline:alphabetic\">",
        xml_attr(color),
        xml_attr(font_family),
        num(size),
        num(letter),
    ));
    let mut byte = 0usize;
    let mut line_y = 0.0f32;
    for line in text.split('\n') {
        let y = baseline + line_y;
        let anchor = layout
            .map(|item| item.align)
            .unwrap_or(CreativeTextAlign::Left);
        let (x, text_anchor) = match anchor {
            CreativeTextAlign::Center => (w * 0.5, "middle"),
            CreativeTextAlign::Right => (w - padding, "end"),
            CreativeTextAlign::Left => (padding, "start"),
        };
        line_y += line_height + paragraph;
        out.push_str(&format!(
            "<tspan x=\"{}\" y=\"{}\" text-anchor=\"{text_anchor}\">",
            num(x),
            num(y),
        ));
        let end = byte + line.len();
        for segment in text_segments(text, layout, byte, end) {
            let seg_weight = segment.style.font_weight.unwrap_or(weight);
            let seg_style = match segment
                .style
                .font_style
                .unwrap_or(style.unwrap_or(CreativeFontStyle::Normal))
            {
                CreativeFontStyle::Italic => "italic",
                CreativeFontStyle::Normal => "normal",
            };
            let seg_color = segment.style.color.unwrap_or(color);
            let underline = segment
                .style
                .underline
                .or(layout.and_then(|item| item.underline))
                .unwrap_or(false);
            out.push_str(&format!(
                "<tspan fill=\"{}\" font-weight=\"{seg_weight}\" font-style=\"{seg_style}\" text-decoration=\"{}\">{}</tspan>",
                xml_attr(seg_color),
                if underline { "underline" } else { "none" },
                xml_text(segment.text),
            ));
        }
        if line.is_empty() {
            out.push(' ');
        }
        out.push_str("</tspan>");
        byte = end + 1;
    }
    out.push_str("</text>");
}

fn svg_defs_shadow(out: &mut String, node: &CreativeNode, w: f32, h: f32) {
    let Some(shadow) = node
        .appearance
        .as_ref()
        .and_then(|item| item.shadow.as_ref())
    else {
        return;
    };
    if !uses_shadow(&node.content) {
        return;
    }
    let pad = shadow.blur as f32 * 4.0
        + (shadow.x as f32).abs().max((shadow.y as f32).abs())
        + shape_stroke(node) * 0.5
        + 2.0;
    out.push_str(&format!(
        "<defs><filter id=\"shadow-{}\" filterUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" color-interpolation-filters=\"sRGB\"><feDropShadow dx=\"{}\" dy=\"{}\" stdDeviation=\"{}\" flood-color=\"{}\" flood-opacity=\"{}\"/></filter></defs>",
        xml_attr(&node.id),
        num(-pad),
        num(-pad),
        num(w + pad * 2.0),
        num(h + pad * 2.0),
        num(shadow.x as f32),
        num(shadow.y as f32),
        num(shadow.blur as f32),
        xml_attr(&shadow.color),
        num((shadow.opacity as f32).clamp(0.0, 1.0)),
    ));
}

fn filter_attr(node: &CreativeNode) -> String {
    if node
        .appearance
        .as_ref()
        .and_then(|item| item.shadow.as_ref())
        .is_some()
        && uses_shadow(&node.content)
    {
        format!(" filter=\"url(#shadow-{})\"", xml_attr(&node.id))
    } else {
        String::new()
    }
}

fn uses_shadow(content: &CreativeContent) -> bool {
    match content {
        CreativeContent::Frame { .. } | CreativeContent::Image { .. } => true,
        CreativeContent::Shape { shape, .. } => !matches!(shape, CreativeShapeKind::Line),
        _ => false,
    }
}

fn shape_stroke(node: &CreativeNode) -> f32 {
    match &node.content {
        CreativeContent::Shape { stroke_width, .. } => *stroke_width as f32,
        _ => 0.0,
    }
}

struct Segment<'a> {
    text: &'a str,
    style: SpanStyle<'a>,
}

struct SpanStyle<'a> {
    font_weight: Option<u16>,
    font_style: Option<CreativeFontStyle>,
    color: Option<&'a str>,
    underline: Option<bool>,
}

fn text_segments<'a>(
    text: &'a str,
    layout: Option<&'a CreativeTextLayout>,
    start: usize,
    end: usize,
) -> Vec<Segment<'a>> {
    let spans = layout.map(|item| item.spans.as_slice()).unwrap_or(&[]);
    let mut points = vec![start, end];
    for span in spans {
        let from = utf16_to_byte(text, span.start);
        let to = utf16_to_byte(text, span.end);
        if from > start && from < end {
            points.push(from);
        }
        if to > start && to < end {
            points.push(to);
        }
    }
    points.sort_unstable();
    points.dedup();
    points
        .windows(2)
        .map(|pair| {
            let style = spans
                .iter()
                .find(|span| {
                    utf16_to_byte(text, span.start) <= pair[0]
                        && utf16_to_byte(text, span.end) >= pair[1]
                })
                .map(span_style)
                .unwrap_or(SpanStyle {
                    font_weight: None,
                    font_style: None,
                    color: None,
                    underline: None,
                });
            Segment {
                text: &text[pair[0]..pair[1]],
                style,
            }
        })
        .collect()
}

fn span_style(span: &CreativeTextSpan) -> SpanStyle<'_> {
    SpanStyle {
        font_weight: span.font_weight,
        font_style: span.font_style,
        color: span.color.as_deref(),
        underline: span.underline,
    }
}

fn text_style(
    node: &CreativeNode,
    family: &str,
    size: f64,
    weight: Option<u16>,
    style: Option<CreativeFontStyle>,
) -> TextStyle {
    let layout = node.text_layout.as_ref();
    TextStyle {
        family: family.to_string(),
        size: size as f32,
        weight: weight.unwrap_or(FONT_WEIGHT_REGULAR),
        style: match style.unwrap_or(CreativeFontStyle::Normal) {
            CreativeFontStyle::Italic => FontStyle::Italic,
            CreativeFontStyle::Normal => FontStyle::Normal,
        },
        letter_spacing: layout.and_then(|item| item.letter_spacing).unwrap_or(0.0) as f32,
        paragraph_spacing: layout
            .and_then(|item| item.paragraph_spacing)
            .unwrap_or(0.0) as f32,
        align: match layout
            .map(|item| item.align)
            .unwrap_or(CreativeTextAlign::Left)
        {
            CreativeTextAlign::Left => TextAlign::Start,
            CreativeTextAlign::Center => TextAlign::Center,
            CreativeTextAlign::Right => TextAlign::End,
        },
    }
}

fn text_spans(node: &CreativeNode, text: &str, color: &str) -> Vec<TextSpan> {
    let layout = node.text_layout.as_ref();
    let underline_all = layout.and_then(|item| item.underline).unwrap_or(false);
    let spans = layout.map(|item| item.spans.as_slice()).unwrap_or(&[]);
    if spans.is_empty() {
        if !underline_all {
            return Vec::new();
        }
        return vec![TextSpan::new(0, text.len(), parse_color(color)).underline()];
    }
    spans
        .iter()
        .map(|span| {
            let start = utf16_to_byte(text, span.start);
            let end = utf16_to_byte(text, span.end);
            let mut styled = TextSpan::new(
                start,
                end,
                parse_color(span.color.as_deref().unwrap_or(color)),
            );
            styled.weight = span.font_weight;
            styled.italic = matches!(span.font_style, Some(CreativeFontStyle::Italic));
            styled.underline = span.underline.unwrap_or(underline_all);
            styled
        })
        .filter(|span| span.start < span.end)
        .collect()
}

struct LineGeom {
    start: [f32; 2],
    end: [f32; 2],
    stroke_width: f32,
    color: String,
    opacity: f32,
    start_arrow: bool,
    end_arrow: bool,
    dashed: bool,
}

struct LinePaint {
    start: [f32; 2],
    end: [f32; 2],
    heads: Vec<[[f32; 2]; 3]>,
}

fn line_geometry(node: &CreativeNode) -> Option<LineGeom> {
    match &node.content {
        CreativeContent::Shape {
            shape: CreativeShapeKind::Line,
            stroke,
            stroke_width,
            line,
            ..
        } => {
            let (w, h) = size_of(node);
            let line = line.clone().unwrap_or(CreativeLine {
                start: crate::model::CreativeLinePoint { x: 0.0, y: 0.0 },
                end: crate::model::CreativeLinePoint {
                    x: f64::from(w),
                    y: f64::from(h),
                },
                start_arrow: false,
                end_arrow: false,
                dashed: false,
            });
            Some(LineGeom {
                start: [line.start.x as f32, line.start.y as f32],
                end: [line.end.x as f32, line.end.y as f32],
                stroke_width: *stroke_width as f32,
                color: stroke.clone(),
                opacity: 1.0,
                start_arrow: line.start_arrow,
                end_arrow: line.end_arrow,
                dashed: line.dashed,
            })
        }
        CreativeContent::Ink {
            color,
            input,
            brush,
            points,
        } => legacy_arrow(color, input, brush, points),
        _ => None,
    }
}

fn legacy_arrow(
    color: &str,
    input: &crate::model::CreativeInputKind,
    brush: &CreativeBrush,
    points: &[CreativePoint],
) -> Option<LineGeom> {
    if !matches!(input, crate::model::CreativeInputKind::Mouse)
        || !matches!(brush.kind, crate::model::CreativeBrushKind::Pen)
        || points.len() != 5
    {
        return None;
    }
    let samples: Vec<[f32; 2]> = points
        .iter()
        .map(|point| [point.x as f32, point.y as f32])
        .collect();
    if points.iter().any(|point| {
        point.pressure != Some(0.5)
            || point.elapsed_ms.is_some()
            || point.tilt_x.is_some()
            || point.tilt_y.is_some()
    }) {
        return None;
    }
    let [a, b, left, repeat, right] = [samples[0], samples[1], samples[2], samples[3], samples[4]];
    if b != repeat {
        return None;
    }
    let distance = hypot(a, b);
    let left_len = hypot(left, b);
    let right_len = hypot(right, b);
    if distance < 1.0
        || left_len < 1.0
        || (left_len - right_len).abs() > 0.01
        || left_len > distance * 0.36
    {
        return None;
    }
    let direction = (b[1] - a[1]).atan2(b[0] - a[0]);
    let close = |point: [f32; 2], len: f32, angle: f32| {
        (((b[0] - point[0]) / len) - angle.cos()).abs() > 0.001
            || (((b[1] - point[1]) / len) - angle.sin()).abs() > 0.001
    };
    if close(left, left_len, direction - 0.5) || close(right, right_len, direction + 0.5) {
        return None;
    }
    Some(LineGeom {
        start: a,
        end: b,
        stroke_width: brush.size as f32,
        color: color.to_string(),
        opacity: brush.opacity as f32,
        start_arrow: false,
        end_arrow: true,
        dashed: false,
    })
}

fn line_paint(line: &LineGeom) -> LinePaint {
    let a = line.start;
    let b = line.end;
    let length = hypot(a, b);
    let (ux, uy) = if length > 1.0e-6 {
        ((b[0] - a[0]) / length, (b[1] - a[1]) / length)
    } else {
        (1.0, 0.0)
    };
    let head = (length
        * if line.start_arrow && line.end_arrow {
            0.4
        } else {
            0.6
        })
    .min(line.stroke_width * 4.0);
    let triangle = |tip: [f32; 2], direction: f32| {
        [
            tip,
            [
                tip[0] + direction * ux * head - uy * head * 0.45,
                tip[1] + direction * uy * head + ux * head * 0.45,
            ],
            [
                tip[0] + direction * ux * head + uy * head * 0.45,
                tip[1] + direction * uy * head - ux * head * 0.45,
            ],
        ]
    };
    let mut heads = Vec::new();
    if line.start_arrow && head > 0.001 {
        heads.push(triangle(a, 1.0));
    }
    if line.end_arrow && head > 0.001 {
        heads.push(triangle(b, -1.0));
    }
    LinePaint {
        start: [
            a[0] + ux * if line.start_arrow { head * 0.8 } else { 0.0 },
            a[1] + uy * if line.start_arrow { head * 0.8 } else { 0.0 },
        ],
        end: [
            b[0] - ux * if line.end_arrow { head * 0.8 } else { 0.0 },
            b[1] - uy * if line.end_arrow { head * 0.8 } else { 0.0 },
        ],
        heads,
    }
}

fn line_outline(line: &LineGeom) -> Vec<[f32; 2]> {
    let paint = line_paint(line);
    let pad = line.stroke_width * 0.5;
    let mut points = vec![line.start, line.end];
    for head in paint.heads {
        points.extend(head);
    }
    points
        .into_iter()
        .flat_map(|point| {
            [
                [point[0] - pad, point[1] - pad],
                [point[0] + pad, point[1] + pad],
            ]
        })
        .collect()
}

#[derive(Clone)]
struct Sample {
    at: [f32; 2],
    pressure: Option<f32>,
    elapsed_ms: Option<f32>,
}

fn ink_outline(brush: &CreativeBrush, points: &[CreativePoint]) -> Path {
    quadratic_outline(&ribbon(brush, points))
}

fn ribbon(brush: &CreativeBrush, points: &[CreativePoint]) -> Vec<[f32; 2]> {
    let smoothed = smooth_points(points, brush.smoothing * 0.7 + brush.streamline * 0.3);
    if smoothed.is_empty() {
        return Vec::new();
    }
    if smoothed.len() == 1 {
        let radius = stroke_radius(brush, &smoothed, 0);
        return (0..16)
            .map(|index| {
                let angle = index as f32 / 16.0 * std::f32::consts::TAU;
                [
                    smoothed[0].at[0] + angle.cos() * radius,
                    smoothed[0].at[1] + angle.sin() * radius,
                ]
            })
            .collect();
    }
    let mut left = Vec::new();
    let mut right = Vec::new();
    for index in 0..smoothed.len() {
        let previous = smoothed[index.saturating_sub(1)].at;
        let next = smoothed[(index + 1).min(smoothed.len() - 1)].at;
        let dx = next[0] - previous[0];
        let dy = next[1] - previous[1];
        let magnitude = hypot(previous, next);
        let magnitude = if magnitude == 0.0 { 1.0 } else { magnitude };
        let normal = [-dy / magnitude, dx / magnitude];
        let radius = stroke_radius(brush, &smoothed, index);
        let at = smoothed[index].at;
        left.push([at[0] + normal[0] * radius, at[1] + normal[1] * radius]);
        right.push([at[0] - normal[0] * radius, at[1] - normal[1] * radius]);
    }
    right.reverse();
    left.extend(right);
    left
}

fn smooth_points(points: &[CreativePoint], smoothing: f64) -> Vec<Sample> {
    let raw: Vec<Sample> = points
        .iter()
        .map(|point| Sample {
            at: [point.x as f32, point.y as f32],
            pressure: point.pressure.map(|value| value as f32),
            elapsed_ms: point.elapsed_ms.map(|value| value as f32),
        })
        .collect();
    if raw.len() < 3 || smoothing <= 0.0 {
        return raw;
    }
    let amount = (smoothing as f32).clamp(0.0, 1.0) * 0.35;
    raw.iter()
        .enumerate()
        .map(|(index, point)| {
            if index == 0 || index + 1 == raw.len() {
                return point.clone();
            }
            let previous = raw[index - 1].at;
            let next = raw[index + 1].at;
            Sample {
                at: [
                    point.at[0] * (1.0 - amount) + (previous[0] + next[0]) * 0.5 * amount,
                    point.at[1] * (1.0 - amount) + (previous[1] + next[1]) * 0.5 * amount,
                ],
                pressure: point.pressure,
                elapsed_ms: point.elapsed_ms,
            }
        })
        .collect()
}

fn stroke_radius(brush: &CreativeBrush, points: &[Sample], index: usize) -> f32 {
    let pressure = pressure_at(brush, points, index);
    let factor = 1.0 + brush.thinning as f32 * (pressure * 2.0 - 1.0);
    ((brush.size as f32) * 0.12).max((brush.size as f32) * 0.5 * factor)
}

fn pressure_at(brush: &CreativeBrush, points: &[Sample], index: usize) -> f32 {
    if let Some(direct) = points[index].pressure {
        return direct.clamp(0.0, 1.0);
    }
    if index == 0 {
        return 0.5;
    }
    let previous = &points[index - 1];
    let point = &points[index];
    let elapsed = (point.elapsed_ms.unwrap_or(index as f32 * 8.0)
        - previous.elapsed_ms.unwrap_or((index as f32 - 1.0) * 8.0))
    .max(1.0);
    let velocity = hypot(previous.at, point.at) / elapsed;
    let velocity_pressure =
        0.82 - (velocity / (brush.size as f32 * 0.08).max(0.35)).clamp(0.0, 1.0) * 0.52;
    velocity_pressure.clamp(0.25, 0.82)
}

fn quadratic_outline(outline: &[[f32; 2]]) -> Path {
    if outline.is_empty() {
        return Path::new();
    }
    let mut path = Path::builder();
    path.move_to(round2(outline[0]));
    let n = outline.len();
    for index in 1..n {
        let point = outline[index];
        let next = outline[(index + 1) % n];
        path.quad_to(
            round2(point),
            round2([(point[0] + next[0]) * 0.5, (point[1] + next[1]) * 0.5]),
        );
    }
    path.close();
    path.build()
}

fn visual_bounds(node: &CreativeNode, nodes: &[CreativeNode]) -> Option<Rect> {
    if !node.visible {
        return None;
    }
    if matches!(node.content, CreativeContent::Group) {
        let children: Vec<Rect> = nodes
            .iter()
            .filter(|child| child.parent_id.as_deref() == Some(node.id.as_str()))
            .filter_map(|child| visual_bounds(child, nodes))
            .collect();
        return union_rects(&children).or_else(|| Some(node_bounds(node, nodes)));
    }
    let bounds = node_bounds(node, nodes);
    let mut pad = shape_stroke(node) * 0.5;
    if let Some(shadow) = node
        .appearance
        .as_ref()
        .and_then(|item| item.shadow.as_ref())
    {
        if uses_shadow(&node.content) {
            pad += shadow.blur as f32 * 4.0 + (shadow.x as f32).hypot(shadow.y as f32);
        }
    }
    Some(inflate(bounds, pad))
}

fn node_bounds(node: &CreativeNode, nodes: &[CreativeNode]) -> Rect {
    let matrix = world_affine(node, nodes);
    let points = if let Some(line) = line_geometry(node) {
        line_outline(&line)
    } else {
        let (w, h) = size_of(node);
        vec![[0.0, 0.0], [w, 0.0], [0.0, h], [w, h]]
    };
    aabb(
        points
            .iter()
            .copied()
            .map(|point| matrix.transform_point(point)),
    )
}

fn world_affine(node: &CreativeNode, nodes: &[CreativeNode]) -> Affine {
    let local = local_affine(node);
    let Some(parent_id) = node.parent_id.as_deref() else {
        return local;
    };
    let Some(parent) = nodes.iter().find(|candidate| candidate.id == parent_id) else {
        return local;
    };
    local.then(world_affine(parent, nodes))
}

fn local_affine(node: &CreativeNode) -> Affine {
    let transform = &node.transform;
    let (x, y, w, h) = (
        transform.x as f32,
        transform.y as f32,
        transform.width as f32,
        transform.height as f32,
    );
    let (sin, cos) = (transform.rotation as f32).to_radians().sin_cos();
    Affine {
        m: [
            cos,
            sin,
            -sin,
            cos,
            x + w * 0.5 - cos * w * 0.5 + sin * h * 0.5,
            y + h * 0.5 - sin * w * 0.5 - cos * h * 0.5,
        ],
    }
}

fn corner_radius(node: &CreativeNode) -> f32 {
    let surface = match &node.content {
        CreativeContent::Frame { .. } | CreativeContent::Image { .. } => true,
        CreativeContent::Shape { shape, .. } => matches!(shape, CreativeShapeKind::Rectangle),
        _ => false,
    };
    if !surface {
        return 0.0;
    }
    let radius = node
        .appearance
        .as_ref()
        .map(|CreativeAppearance { corner_radius, .. }| *corner_radius as f32)
        .unwrap_or(0.0)
        .max(0.0);
    let (w, h) = size_of(node);
    radius.min(w * 0.5).min(h * 0.5)
}

fn shadow_of(node: &CreativeNode) -> Option<Effect> {
    if !uses_shadow(&node.content) {
        return None;
    }
    let shadow = node.appearance.as_ref()?.shadow.as_ref()?;
    let mut color = parse_color(&shadow.color);
    color.a *= (shadow.opacity as f32).clamp(0.0, 1.0);
    if color.a <= 0.0 {
        return None;
    }
    Some(Effect::DropShadow {
        offset: [shadow.x as f32, shadow.y as f32],
        blur: (shadow.blur as f32).max(0.0),
        color,
    })
}

fn size_of(node: &CreativeNode) -> (f32, f32) {
    (node.transform.width as f32, node.transform.height as f32)
}

fn box_rect(w: f32, h: f32) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        w,
        h,
    }
}

fn crop_uv(crop: &Option<crate::model::CreativeCrop>) -> [f32; 4] {
    match crop {
        Some(crop) => [
            crop.x as f32,
            crop.y as f32,
            (crop.x + crop.width) as f32,
            (crop.y + crop.height) as f32,
        ],
        None => [0.0, 0.0, 1.0, 1.0],
    }
}

fn inflate(rect: Rect, pad: f32) -> Rect {
    Rect {
        x: rect.x - pad,
        y: rect.y - pad,
        w: rect.w + pad * 2.0,
        h: rect.h + pad * 2.0,
    }
}

fn union_rects(rects: &[Rect]) -> Option<Rect> {
    let mut iter = rects.iter().copied();
    let first = iter.next()?;
    let mut x0 = first.x;
    let mut y0 = first.y;
    let mut x1 = first.x + first.w;
    let mut y1 = first.y + first.h;
    for rect in iter {
        x0 = x0.min(rect.x);
        y0 = y0.min(rect.y);
        x1 = x1.max(rect.x + rect.w);
        y1 = y1.max(rect.y + rect.h);
    }
    Some(Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

fn aabb(points: impl Iterator<Item = [f32; 2]>) -> Rect {
    let mut points = points.filter(|point| point[0].is_finite() && point[1].is_finite());
    let Some(first) = points.next() else {
        return Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        };
    };
    let mut x0 = first[0];
    let mut y0 = first[1];
    let mut x1 = first[0];
    let mut y1 = first[1];
    for point in points {
        x0 = x0.min(point[0]);
        y0 = y0.min(point[1]);
        x1 = x1.max(point[0]);
        y1 = y1.max(point[1]);
    }
    Rect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(1.0),
        h: (y1 - y0).max(1.0),
    }
}

fn hypot(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (dx * dx + dy * dy).sqrt()
}

fn round2(point: [f32; 2]) -> [f32; 2] {
    [
        (point[0] * 100.0).round() / 100.0,
        (point[1] * 100.0).round() / 100.0,
    ]
}

/// CSS hex color, `"transparent"`, or `"none"`.
pub fn parse_color(text: &str) -> Color {
    let text = text.trim();
    if text.eq_ignore_ascii_case("transparent")
        || text.eq_ignore_ascii_case("none")
        || text.is_empty()
    {
        return Color::rgba(0.0, 0.0, 0.0, 0.0);
    }
    let hex = text.strip_prefix('#').unwrap_or(text);
    let expand = |byte: u8| byte as f32 / 255.0;
    let channel = |pair: &str| u8::from_str_radix(pair, 16).ok().map(expand);
    match hex.len() {
        3 | 4 => {
            let chars: Vec<char> = hex.chars().collect();
            let mut wide = String::new();
            for ch in chars {
                wide.push(ch);
                wide.push(ch);
            }
            parse_color(&format!("#{wide}"))
        }
        6 | 8 => {
            let r = channel(&hex[0..2]).unwrap_or(0.0);
            let g = channel(&hex[2..4]).unwrap_or(0.0);
            let b = channel(&hex[4..6]).unwrap_or(0.0);
            let a = if hex.len() == 8 {
                channel(&hex[6..8]).unwrap_or(1.0)
            } else {
                1.0
            };
            Color::rgba(r, g, b, a)
        }
        _ => Color::rgba(0.0, 0.0, 0.0, 1.0),
    }
}

fn num(value: f32) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    let mut text = format!("{value:.4}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" {
        text = "0".into();
    }
    text
}

fn xml_attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn outline_data(brush: &CreativeBrush, points: &[CreativePoint]) -> String {
    let outline = ribbon(brush, points);
    if outline.is_empty() {
        return String::new();
    }
    let mut data = format!(
        "M {} {}",
        num(round2(outline[0])[0]),
        num(round2(outline[0])[1])
    );
    let n = outline.len();
    for index in 1..n {
        let point = round2(outline[index]);
        let next = outline[(index + 1) % n];
        let mid = round2([
            (outline[index][0] + next[0]) * 0.5,
            (outline[index][1] + next[1]) * 0.5,
        ]);
        data.push_str(&format!(
            " Q {} {} {} {}",
            num(point[0]),
            num(point[1]),
            num(mid[0]),
            num(mid[1])
        ));
    }
    data.push_str(" Z");
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_and_hex_colors() {
        let clear = parse_color("transparent");
        assert_eq!(clear.a, 0.0);
        let red = parse_color("#ff0000");
        assert!((red.r - 1.0).abs() < 1.0e-6 && red.g == 0.0);
        let faded = parse_color("#00000080");
        assert!((faded.a - 128.0 / 255.0).abs() < 1.0e-6);
    }

    #[test]
    fn a_five_point_ink_sample_draws_a_solid_arrow() {
        let tip = [100.0f32, 0.0];
        let direction = tip[1].atan2(tip[0]);
        let len = 20.0f32;
        let arm = |angle: f32| {
            let point = [
                tip[0] - len * (direction + angle).cos(),
                tip[1] - len * (direction + angle).sin(),
            ];
            [f64::from(point[0]), f64::from(point[1])]
        };
        let sample = |point: [f64; 2]| CreativePoint {
            x: point[0],
            y: point[1],
            pressure: Some(0.5),
            elapsed_ms: None,
            tilt_x: None,
            tilt_y: None,
        };
        let scene = CreativeScene {
            schema: "medousa-create".into(),
            version: 3,
            background: "transparent".into(),
            nodes: vec![CreativeNode {
                id: "old-arrow".into(),
                name: None,
                appearance: None,
                text_layout: None,
                parent_id: None,
                transform: crate::model::CreativeTransform {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 24.0,
                    rotation: 0.0,
                },
                opacity: 1.0,
                visible: true,
                locked: false,
                content: CreativeContent::Ink {
                    color: "#111111".into(),
                    input: crate::model::CreativeInputKind::Mouse,
                    brush: CreativeBrush {
                        kind: crate::model::CreativeBrushKind::Pen,
                        size: 4.0,
                        thinning: 0.0,
                        smoothing: 0.0,
                        streamline: 0.0,
                        opacity: 1.0,
                    },
                    points: vec![
                        sample([0.0, 0.0]),
                        sample([f64::from(tip[0]), f64::from(tip[1])]),
                        sample(arm(-0.5)),
                        sample([f64::from(tip[0]), f64::from(tip[1])]),
                        sample(arm(0.5)),
                    ],
                },
            }],
        };
        let svg = scene_svg(
            &scene,
            &BTreeMap::new(),
            Rect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 24.0,
            },
        );
        assert!(svg.contains("data-arrowhead"), "{svg}");
        assert!(svg.contains("<line "), "{svg}");
        assert!(!svg.contains("stroke-dasharray"), "{svg}");
    }
}
