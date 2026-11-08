//! The saved document. Coordinates are world coordinates; camera, selection
//! and inspector state never belong in a document.
//!
//! Field names and tags match Medousa Create's version-3 scenes, so a
//! document saved by either side deserializes in the other.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Schema name written by [`Scene::default`].
pub const SCHEMA: &str = "rndr-canvas";
/// Schema version written by [`Scene::default`].
pub const VERSION: u32 = 1;
/// Medousa Create's schema name. Scenes with this name at
/// [`MEDOUSA_VERSION`] are accepted and keep their header when saved.
pub const MEDOUSA_SCHEMA: &str = "medousa-create";
/// The Medousa Create version this crate reads and writes.
pub const MEDOUSA_VERSION: u32 = 3;

/// Position, size and rotation in the parent's coordinates. Rotation is in
/// degrees, clockwise, around the center of the `width` x `height` box.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
}

/// A sampled ink point in the node's local coordinates.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InkPoint {
    pub x: f64,
    pub y: f64,
    pub pressure: Option<f64>,
    pub elapsed_ms: Option<f64>,
    pub tilt_x: Option<f64>,
    pub tilt_y: Option<f64>,
}

impl InkPoint {
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Brush {
    pub kind: BrushKind,
    pub size: f64,
    pub thinning: f64,
    pub smoothing: f64,
    pub streamline: f64,
    pub opacity: f64,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            kind: BrushKind::Pen,
            size: 4.0,
            thinning: 0.5,
            smoothing: 0.5,
            streamline: 0.5,
            opacity: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrushKind {
    Pen,
    Pencil,
    Marker,
    Highlighter,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Pen,
    Touch,
    Mouse,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Line,
}

/// Visible part of an image, as fractions of its size.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FontStyle {
    Normal,
    Italic,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LinePoint {
    pub x: f64,
    pub y: f64,
}

/// Endpoints of a [`ShapeKind::Line`] in the node's local coordinates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Line {
    pub start: LinePoint,
    pub end: LinePoint,
    #[serde(default)]
    pub start_arrow: bool,
    #[serde(default)]
    pub end_arrow: bool,
    #[serde(default)]
    pub dashed: bool,
}

/// What a node draws. Paint values are `#rgb`, `#rgba`, `#rrggbb`,
/// `#rrggbbaa` or `transparent`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Content {
    Ink {
        color: String,
        input: InputKind,
        brush: Brush,
        points: Vec<InkPoint>,
    },
    Text {
        text: String,
        color: String,
        font_family: String,
        font_size: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font_weight: Option<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font_style: Option<FontStyle>,
    },
    Shape {
        shape: ShapeKind,
        fill: String,
        stroke: String,
        stroke_width: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<Line>,
    },
    Image {
        asset_id: String,
        crop: Option<Crop>,
    },
    /// Contains children; draws nothing itself.
    Group,
    /// Contains children over a background.
    Frame { name: String, background: String },
}

impl Content {
    /// Whether nodes may name this node as their parent.
    pub fn is_container(&self) -> bool {
        matches!(self, Content::Group | Content::Frame { .. })
    }
}

/// Surface styling for rectangles, ellipses, frames and images.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Appearance {
    pub corner_radius: f64,
    pub shadow: Option<Shadow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Shadow {
    pub x: f64,
    pub y: f64,
    /// Gaussian standard deviation in canvas units.
    pub blur: f64,
    pub color: String,
    pub opacity: f64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Paragraph settings for a text node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TextLayout {
    /// `None` sizes the text to its longest line; a width wraps it.
    pub width: Option<f64>,
    pub align: TextAlign,
    /// Multiple of the font size.
    pub line_height: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraph_spacing: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<TextSpan>,
}

impl Default for TextLayout {
    fn default() -> Self {
        Self {
            width: None,
            align: TextAlign::Left,
            line_height: 1.25,
            letter_spacing: None,
            paragraph_spacing: None,
            underline: None,
            spans: Vec::new(),
        }
    }
}

/// Inline style over a UTF-16 range of the text. Spans are sorted and do
/// not overlap.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TextSpan {
    pub start: u32,
    pub end: u32,
    pub font_weight: Option<u16>,
    pub font_style: Option<FontStyle>,
    pub color: Option<String>,
    pub underline: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    /// Layer label shown instead of the derived [`Node::label`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<Appearance>,
    /// Opaque provenance attached to a frame by the application that made
    /// it. Kept and saved unchanged; this crate never interprets it.
    #[serde(
        default,
        rename = "design_source",
        skip_serializing_if = "Option::is_none"
    )]
    pub origin: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_layout: Option<TextLayout>,
    pub parent_id: Option<String>,
    pub transform: Transform,
    pub opacity: f64,
    pub visible: bool,
    pub locked: bool,
    pub content: Content,
}

impl Node {
    /// A visible, unlocked, opaque node with no parent.
    pub fn new(id: impl Into<String>, content: Content, transform: Transform) -> Self {
        Self {
            id: id.into(),
            name: None,
            appearance: None,
            origin: None,
            text_layout: None,
            parent_id: None,
            transform,
            opacity: 1.0,
            visible: true,
            locked: false,
            content,
        }
    }

    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent_id = Some(parent.into());
        self
    }

    /// Layer label: the explicit name, else one derived from the content.
    pub fn label(&self) -> String {
        if let Some(name) = self.name.as_deref().filter(|n| !n.trim().is_empty()) {
            return name.to_string();
        }
        match &self.content {
            Content::Shape {
                line: Some(line), ..
            } => if line.start_arrow || line.end_arrow {
                "Arrow"
            } else {
                "Line"
            }
            .into(),
            Content::Frame { name, .. } => name.clone(),
            Content::Text { text, .. } => {
                let first: String = text.lines().next().unwrap_or("").chars().take(48).collect();
                if first.is_empty() {
                    "Text".into()
                } else {
                    first
                }
            }
            Content::Shape { shape, .. } => match shape {
                ShapeKind::Rectangle => "rectangle",
                ShapeKind::Ellipse => "ellipse",
                ShapeKind::Line => "line",
            }
            .into(),
            Content::Ink { .. } => "Ink".into(),
            Content::Image { .. } => "Image".into(),
            Content::Group => "Group".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub schema: String,
    pub version: u32,
    pub background: String,
    /// Back-to-front paint order. `parent_id` defines containment.
    pub nodes: Vec<Node>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            schema: SCHEMA.into(),
            version: VERSION,
            background: "transparent".into(),
            nodes: Vec::new(),
        }
    }
}

impl Scene {
    /// An empty scene with Medousa Create's version-3 header.
    pub fn medousa() -> Self {
        Self {
            schema: MEDOUSA_SCHEMA.into(),
            version: MEDOUSA_VERSION,
            ..Self::default()
        }
    }

    /// Whether this crate understands the scene's schema and version.
    pub fn is_supported(&self) -> bool {
        supported(&self.schema, u64::from(self.version))
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

pub(crate) fn supported(schema: &str, version: u64) -> bool {
    (schema == SCHEMA && version == u64::from(VERSION))
        || (schema == MEDOUSA_SCHEMA && version == u64::from(MEDOUSA_VERSION))
}

/// An imported bitmap. The id is `a_` followed by the SHA-256 of its bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub id: String,
    pub mime: String,
    pub byte_size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: String,
    /// Increases by one with every accepted change.
    pub revision: u64,
    pub title: String,
    pub pinned: bool,
    /// RFC 3339 timestamps.
    pub created_at: String,
    pub updated_at: String,
    pub scene: Scene,
    pub assets: BTreeMap<String, Asset>,
}
