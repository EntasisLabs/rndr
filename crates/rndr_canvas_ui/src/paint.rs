//! Draws a [`Scene`] into an `rndr_paint` display list.
//!
//! Geometry comes from `rndr_canvas`, so what is painted is what picking and
//! selection measure: boxes rotate about their centers, a line's shaft stops
//! short of its arrowheads, ink is the filled outline of its samples, and
//! text is laid out by the same [`TextMeasure`] that sizes it.

use rndr_canvas::geometry::local_matrix;
use rndr_canvas::text::{Font, TextMeasure, DEFAULT_LINE_HEIGHT};
use rndr_canvas::{
    Brush, Content, FontStyle as CanvasFontStyle, InkPoint, LineGeometry, Node, Rect as CanvasRect,
    Scene, SceneIndex, ShapeKind, TextAlign as CanvasAlign,
};
use rndr_core::{
    Color, FontStyle, Rect, TextAlign, TextEngine, TextSpan, TextStyle, World, FONT_WEIGHT_REGULAR,
};
use rndr_paint::{
    Affine, Canvas, Cap, ClipShape, Effect, FillRule, Path, SceneLayer, Stroke, TextSource,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Texture ids for image assets, keyed by asset id. Load the pixels with
/// `rndr_render::load_rgba` (or any loader) under the same ids.
pub type ImageIds = BTreeMap<String, u64>;

/// Measures canvas text with the world's [`TextEngine`].
#[derive(Clone)]
pub struct EngineMeasure(pub Arc<dyn TextEngine>);

impl EngineMeasure {
    /// The engine installed in `world`, if any.
    pub fn from_world(world: &World) -> Option<Self> {
        world
            .resource::<Arc<dyn TextEngine>>()
            .map(|engine| Self(engine.clone()))
    }
}

impl TextMeasure for EngineMeasure {
    fn width(&self, line: &str, font: &Font<'_>) -> f64 {
        let style = TextStyle {
            family: font.family.to_string(),
            size: font.size as f32,
            weight: font.weight,
            style: match font.style {
                CanvasFontStyle::Italic => FontStyle::Italic,
                CanvasFontStyle::Normal => FontStyle::Normal,
            },
            ..TextStyle::default()
        };
        f64::from(self.0.measure(line, &style, &[], f32::INFINITY).width)
    }
}

/// Parses `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` and `transparent`.
/// Anything else is opaque black.
pub fn parse_color(text: &str) -> Color {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("transparent") {
        return Color::rgba(0.0, 0.0, 0.0, 0.0);
    }
    let hex = text.strip_prefix('#').unwrap_or(text);
    let digits: String = match hex.len() {
        3 | 4 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => hex.to_string(),
        _ => return Color::rgba(0.0, 0.0, 0.0, 1.0),
    };
    let channel = |i: usize| {
        digits
            .get(i..i + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .map_or(0.0, |b| f32::from(b) / 255.0)
    };
    let a = if digits.len() == 8 { channel(6) } else { 1.0 };
    Color::rgba(channel(0), channel(2), channel(4), a)
}

/// Converts a canvas matrix for `rndr_paint`.
pub fn affine(m: rndr_canvas::Matrix) -> Affine {
    let [a, b, c, d, e, f] = m.0;
    Affine {
        m: [a as f32, b as f32, c as f32, d as f32, e as f32, f as f32],
    }
}

/// Paints `scene` with the canvas's current camera. Hidden nodes and their
/// children are skipped; frames clip their children.
pub fn paint_scene(
    canvas: &mut Canvas,
    world: &World,
    scene: &Scene,
    images: &ImageIds,
    measure: &dyn TextMeasure,
) {
    let background = parse_color(&scene.background);
    if background.a > 0.0 {
        let far = 1.0e6;
        canvas.fill(
            Path::polygon(&[[-far, -far], [far, -far], [far, far], [-far, far]]),
            background,
            FillRule::NonZero,
        );
    }
    let index = SceneIndex::with_measure(&scene.nodes, measure);
    let painter = Painter {
        world,
        index: &index,
        images,
    };
    for node in index.children(None) {
        painter.node(canvas, node);
    }
}

/// Bounds of the visible root nodes including stroke and shadow, or `None`
/// for an empty scene.
pub fn visual_bounds(scene: &Scene, measure: &dyn TextMeasure) -> Option<CanvasRect> {
    let index = SceneIndex::with_measure(&scene.nodes, measure);
    CanvasRect::union_all(
        index
            .children(None)
            .into_iter()
            .filter(|n| n.visible)
            .map(|n| node_visual_bounds(&index, n)),
    )
}

fn node_visual_bounds(index: &SceneIndex<'_>, node: &Node) -> CanvasRect {
    let mut bounds = index.object_bounds(node);
    let stroke = match &node.content {
        Content::Shape { stroke_width, .. } => *stroke_width / 2.0,
        Content::Ink { brush, .. } => brush.size / 2.0,
        _ => 0.0,
    };
    bounds = inflate(bounds, stroke);
    if let Some(shadow) = node.appearance.as_ref().and_then(|a| a.shadow.as_ref()) {
        let blur = shadow.blur * 3.0;
        let moved = CanvasRect::new(
            bounds.x + shadow.x - blur,
            bounds.y + shadow.y - blur,
            bounds.width + blur * 2.0,
            bounds.height + blur * 2.0,
        );
        bounds = bounds.union(moved);
    }
    if matches!(node.content, Content::Group) {
        for child in index
            .children(Some(&node.id))
            .into_iter()
            .filter(|c| c.visible)
        {
            bounds = bounds.union(node_visual_bounds(index, child));
        }
    }
    bounds
}

fn inflate(r: CanvasRect, pad: f64) -> CanvasRect {
    CanvasRect::new(
        r.x - pad,
        r.y - pad,
        r.width + pad * 2.0,
        r.height + pad * 2.0,
    )
}

struct Painter<'a> {
    world: &'a World,
    index: &'a SceneIndex<'a>,
    images: &'a ImageIds,
}

fn rect(w: f32, h: f32) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        w,
        h,
    }
}

impl Painter<'_> {
    fn node(&self, canvas: &mut Canvas, node: &Node) {
        let opacity = (node.opacity as f32).clamp(0.0, 1.0);
        if !node.visible || opacity <= 0.0 {
            return;
        }
        canvas.push_transform(affine(local_matrix(&node.transform)));
        let fade = opacity < 0.999;
        if fade {
            canvas.push_layer(SceneLayer::opacity(opacity));
        }
        let (w, h) = (node.transform.width as f32, node.transform.height as f32);
        let radius = corner_radius(node);
        let shadow = shadow_of(node);
        match &node.content {
            Content::Frame { background, .. } => {
                let fill = parse_color(background);
                if shadow.is_some() || fill.a > 0.0 {
                    push_effect(canvas, shadow, None);
                    if fill.a > 0.0 {
                        canvas.fill(
                            Path::round_rect(rect(w, h), radius),
                            fill,
                            FillRule::NonZero,
                        );
                    }
                    pop_effect(canvas, shadow, None);
                }
                canvas.push_layer(SceneLayer {
                    opacity: 1.0,
                    clip: Some(ClipShape::RoundRect {
                        rect: rect(w, h),
                        radius,
                    }),
                    effect: None,
                    ..SceneLayer::default()
                });
                self.children(canvas, node);
                canvas.pop_layer();
            }
            Content::Group => self.children(canvas, node),
            Content::Text { .. } => self.text(canvas, node),
            Content::Shape {
                shape,
                fill,
                stroke,
                stroke_width,
                ..
            } => {
                if let Some(line) = LineGeometry::of(node) {
                    paint_line(canvas, &line, parse_color(stroke));
                } else {
                    let path = match shape {
                        ShapeKind::Ellipse => Path::ellipse([w * 0.5, h * 0.5], [w * 0.5, h * 0.5]),
                        _ => Path::round_rect(rect(w, h), radius),
                    };
                    push_effect(canvas, shadow, None);
                    stroke_fill(
                        canvas,
                        path,
                        parse_color(fill),
                        parse_color(stroke),
                        (*stroke_width as f32).max(0.0),
                    );
                    pop_effect(canvas, shadow, None);
                }
            }
            Content::Ink {
                color,
                brush,
                points,
                ..
            } => {
                let path = ink_outline(brush, points);
                let mut ink = parse_color(color);
                ink.a *= (brush.opacity as f32).clamp(0.0, 1.0);
                if ink.a > 0.0 && !path.is_empty() {
                    canvas.fill(path, ink, FillRule::NonZero);
                }
            }
            Content::Image { asset_id, crop } => match self.images.get(asset_id).copied() {
                Some(id) => {
                    let uv = crop.map_or([0.0, 0.0, 1.0, 1.0], |c| {
                        [
                            c.x as f32,
                            c.y as f32,
                            (c.x + c.width) as f32,
                            (c.y + c.height) as f32,
                        ]
                    });
                    let clip = (radius > 0.0).then(|| ClipShape::RoundRect {
                        rect: rect(w, h),
                        radius,
                    });
                    push_effect(canvas, shadow, clip.clone());
                    canvas.image(
                        id,
                        rect(w, h),
                        uv,
                        if clip.is_some() { 0.0 } else { radius },
                    );
                    pop_effect(canvas, shadow, clip);
                }
                None => canvas.fill(
                    Path::round_rect(rect(w, h), radius),
                    Color::rgba(0.53, 0.53, 0.53, 0.1),
                    FillRule::NonZero,
                ),
            },
        }
        if fade {
            canvas.pop_layer();
        }
        canvas.pop_transform();
    }

    fn children(&self, canvas: &mut Canvas, node: &Node) {
        for child in self.index.children(Some(&node.id)) {
            self.node(canvas, child);
        }
    }

    fn text(&self, canvas: &mut Canvas, node: &Node) {
        let Content::Text {
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
        let layout = node.text_layout.as_ref();
        let size = *font_size as f32;
        let padding = size * 0.06;
        let style = TextStyle {
            family: font_family.clone(),
            size,
            weight: font_weight.unwrap_or(FONT_WEIGHT_REGULAR),
            style: match font_style.unwrap_or(CanvasFontStyle::Normal) {
                CanvasFontStyle::Italic => FontStyle::Italic,
                CanvasFontStyle::Normal => FontStyle::Normal,
            },
            letter_spacing: layout.and_then(|l| l.letter_spacing).unwrap_or(0.0) as f32,
            paragraph_spacing: layout.and_then(|l| l.paragraph_spacing).unwrap_or(0.0) as f32,
            align: match layout.map_or(CanvasAlign::Left, |l| l.align) {
                CanvasAlign::Left => TextAlign::Start,
                CanvasAlign::Center => TextAlign::Center,
                CanvasAlign::Right => TextAlign::End,
            },
            line_height: Some(layout.map_or(DEFAULT_LINE_HEIGHT, |l| l.line_height) as f32),
            ..TextStyle::default()
        };
        let (width, _) = self.index.dimensions(node);
        let max_width = match layout.and_then(|l| l.width) {
            Some(_) => (width as f32 - padding * 2.0).max(1.0),
            None if !matches!(style.align, TextAlign::Start) => {
                (width as f32 - padding * 2.0).max(1.0)
            }
            None => f32::INFINITY,
        };
        canvas.text(
            self.world,
            TextSource {
                text: text.clone(),
                style,
                spans: text_spans(node, text, color),
                max_width,
                origin: [padding, padding],
                color: parse_color(color),
            },
        );
    }
}

/// Byte offset of a UTF-16 offset, clamped to the text.
pub fn utf16_to_byte(text: &str, offset: u32) -> usize {
    let mut units = 0u32;
    for (byte, ch) in text.char_indices() {
        if units >= offset {
            return byte;
        }
        units += ch.len_utf16() as u32;
    }
    text.len()
}

fn text_spans(node: &Node, text: &str, color: &str) -> Vec<TextSpan> {
    let layout = node.text_layout.as_ref();
    let underline_all = layout.and_then(|l| l.underline).unwrap_or(false);
    let spans = layout.map_or(&[][..], |l| l.spans.as_slice());
    if spans.is_empty() {
        return if underline_all {
            vec![TextSpan::new(0, text.len(), parse_color(color)).underline()]
        } else {
            Vec::new()
        };
    }
    spans
        .iter()
        .map(|span| {
            let mut styled = TextSpan::new(
                utf16_to_byte(text, span.start),
                utf16_to_byte(text, span.end),
                parse_color(span.color.as_deref().unwrap_or(color)),
            );
            styled.weight = span.font_weight;
            styled.italic = matches!(span.font_style, Some(CanvasFontStyle::Italic));
            styled.underline = span.underline.unwrap_or(underline_all);
            styled
        })
        .filter(|span| span.start < span.end)
        .collect()
}

fn corner_radius(node: &Node) -> f32 {
    let cornered = matches!(
        node.content,
        Content::Frame { .. }
            | Content::Image { .. }
            | Content::Shape {
                shape: ShapeKind::Rectangle,
                ..
            }
    );
    if !cornered {
        return 0.0;
    }
    let (w, h) = (node.transform.width as f32, node.transform.height as f32);
    node.appearance
        .as_ref()
        .map_or(0.0, |a| a.corner_radius as f32)
        .max(0.0)
        .min(w * 0.5)
        .min(h * 0.5)
}

fn shadow_of(node: &Node) -> Option<Effect> {
    let surface = matches!(
        node.content,
        Content::Frame { .. }
            | Content::Image { .. }
            | Content::Shape {
                shape: ShapeKind::Rectangle | ShapeKind::Ellipse,
                ..
            }
    );
    if !surface {
        return None;
    }
    let shadow = node.appearance.as_ref()?.shadow.as_ref()?;
    let mut color = parse_color(&shadow.color);
    color.a *= (shadow.opacity as f32).clamp(0.0, 1.0);
    (color.a > 0.0).then(|| Effect::DropShadow {
        offset: [shadow.x as f32, shadow.y as f32],
        blur: (shadow.blur as f32).max(0.0),
        color,
    })
}

fn push_effect(canvas: &mut Canvas, shadow: Option<Effect>, clip: Option<ClipShape>) {
    if shadow.is_some() || clip.is_some() {
        canvas.push_layer(SceneLayer {
            opacity: 1.0,
            clip,
            effect: shadow,
            ..SceneLayer::default()
        });
    }
}

fn pop_effect(canvas: &mut Canvas, shadow: Option<Effect>, clip: Option<ClipShape>) {
    if shadow.is_some() || clip.is_some() {
        canvas.pop_layer();
    }
}

fn stroke_fill(canvas: &mut Canvas, path: Path, fill: Color, stroke: Color, width: f32) {
    let stroke_style = || Stroke::solid(width).with_cap(Cap::Butt);
    match (fill.a > 0.0, stroke.a > 0.0 && width > 0.0) {
        (true, true) => canvas.fill_stroke(path, fill, FillRule::NonZero, stroke, stroke_style()),
        (true, false) => canvas.fill(path, fill, FillRule::NonZero),
        (false, true) => canvas.stroke(path, stroke, stroke_style()),
        (false, false) => {}
    }
}

fn point(v: rndr_canvas::Vec2) -> [f32; 2] {
    [v.x as f32, v.y as f32]
}

fn paint_line(canvas: &mut Canvas, line: &LineGeometry, color: Color) {
    let width = line.stroke_width as f32;
    if color.a <= 0.0 || width <= 0.0 {
        return;
    }
    let dash = if line.dashed {
        vec![width * 3.0, width * 2.0]
    } else {
        Vec::new()
    };
    let (start, end) = line.shaft();
    canvas.stroke(
        Path::line(point(start), point(end)),
        color,
        Stroke::solid(width).with_cap(Cap::Round).dashed(dash),
    );
    for head in line.arrowheads() {
        canvas.fill(Path::polygon(&head.map(point)), color, FillRule::NonZero);
    }
}

struct Sample {
    at: [f32; 2],
    pressure: Option<f32>,
    elapsed_ms: Option<f32>,
}

/// The filled outline of an ink stroke in its local coordinates: a ribbon
/// whose width follows pressure (or, without pressure, drawing speed).
pub fn ink_outline(brush: &Brush, points: &[InkPoint]) -> Path {
    quadratic_outline(&ribbon(brush, points))
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn ribbon(brush: &Brush, points: &[InkPoint]) -> Vec<[f32; 2]> {
    let smoothed = smooth_points(points, brush.smoothing * 0.7 + brush.streamline * 0.3);
    match smoothed.len() {
        0 => return Vec::new(),
        1 => {
            let radius = stroke_radius(brush, &smoothed, 0);
            let at = smoothed[0].at;
            return (0..16)
                .map(|i| {
                    let angle = i as f32 / 16.0 * std::f32::consts::TAU;
                    [at[0] + angle.cos() * radius, at[1] + angle.sin() * radius]
                })
                .collect();
        }
        _ => {}
    }
    let mut left = Vec::new();
    let mut right = Vec::new();
    for i in 0..smoothed.len() {
        let previous = smoothed[i.saturating_sub(1)].at;
        let next = smoothed[(i + 1).min(smoothed.len() - 1)].at;
        let magnitude = Some(distance(previous, next))
            .filter(|m| *m != 0.0)
            .unwrap_or(1.0);
        let normal = [
            -(next[1] - previous[1]) / magnitude,
            (next[0] - previous[0]) / magnitude,
        ];
        let radius = stroke_radius(brush, &smoothed, i);
        let at = smoothed[i].at;
        left.push([at[0] + normal[0] * radius, at[1] + normal[1] * radius]);
        right.push([at[0] - normal[0] * radius, at[1] - normal[1] * radius]);
    }
    right.reverse();
    left.extend(right);
    left
}

fn smooth_points(points: &[InkPoint], smoothing: f64) -> Vec<Sample> {
    let raw: Vec<Sample> = points
        .iter()
        .map(|p| Sample {
            at: [p.x as f32, p.y as f32],
            pressure: p.pressure.map(|v| v as f32),
            elapsed_ms: p.elapsed_ms.map(|v| v as f32),
        })
        .collect();
    if raw.len() < 3 || smoothing <= 0.0 {
        return raw;
    }
    let amount = (smoothing as f32).clamp(0.0, 1.0) * 0.35;
    (0..raw.len())
        .map(|i| {
            let p = &raw[i];
            if i == 0 || i + 1 == raw.len() {
                return Sample { at: p.at, ..*p };
            }
            let (a, b) = (raw[i - 1].at, raw[i + 1].at);
            Sample {
                at: [
                    p.at[0] * (1.0 - amount) + (a[0] + b[0]) * 0.5 * amount,
                    p.at[1] * (1.0 - amount) + (a[1] + b[1]) * 0.5 * amount,
                ],
                ..*p
            }
        })
        .collect()
}

fn stroke_radius(brush: &Brush, points: &[Sample], i: usize) -> f32 {
    let pressure = pressure_at(brush, points, i);
    let size = brush.size as f32;
    let factor = 1.0 + brush.thinning as f32 * (pressure * 2.0 - 1.0);
    (size * 0.12).max(size * 0.5 * factor)
}

fn pressure_at(brush: &Brush, points: &[Sample], i: usize) -> f32 {
    if let Some(direct) = points[i].pressure {
        return direct.clamp(0.0, 1.0);
    }
    if i == 0 {
        return 0.5;
    }
    let (previous, p) = (&points[i - 1], &points[i]);
    let elapsed = (p.elapsed_ms.unwrap_or(i as f32 * 8.0)
        - previous.elapsed_ms.unwrap_or((i as f32 - 1.0) * 8.0))
    .max(1.0);
    let velocity = distance(previous.at, p.at) / elapsed;
    let scale = (brush.size as f32 * 0.08).max(0.35);
    (0.82 - (velocity / scale).clamp(0.0, 1.0) * 0.52).clamp(0.25, 0.82)
}

fn round2(p: [f32; 2]) -> [f32; 2] {
    [
        (p[0] * 100.0).round() / 100.0,
        (p[1] * 100.0).round() / 100.0,
    ]
}

fn mid(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    round2([(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5])
}

fn quadratic_outline(outline: &[[f32; 2]]) -> Path {
    let n = outline.len();
    if n == 0 {
        return Path::new();
    }
    let mut path = Path::builder();
    if n < 2 {
        path.move_to(round2(outline[0]));
        path.close();
        return path.build();
    }
    // Starting on a midpoint makes every vertex a quadratic control, so both
    // ends of the ribbon get the same cap.
    path.move_to(mid(outline[n - 1], outline[0]));
    for i in 0..n {
        let next = outline[(i + 1) % n];
        path.quad_to(round2(outline[i]), mid(outline[i], next));
    }
    path.close();
    path.build()
}
