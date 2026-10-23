//! Saved Create scene, mirrored from `medousa-types` schema `medousa-create`
//! version 3.
//!
//! The published rndr crates do not depend on Medousa. This module is the
//! subset a viewer needs to draw a document: nodes, transforms, opacity,
//! appearance, and the content kinds Home saves. Camera, selection, and
//! history stay out. Unknown fields are rejected so a newer document fails
//! loudly instead of dropping a property the viewer never read.

use serde::Deserialize;

/// Schema name written on every saved scene.
pub const SCHEMA: &str = "medousa-create";
/// Document version this viewer reads.
pub const VERSION: u32 = 3;

/// One saved canvas.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativeScene {
    /// Always [`SCHEMA`].
    pub schema: String,
    /// Always [`VERSION`] for documents this viewer accepts.
    pub version: u32,
    /// CSS color, or `"transparent"`.
    pub background: String,
    /// Back-to-front. [`CreativeNode::parent_id`] is containment.
    pub nodes: Vec<CreativeNode>,
}

/// One object on the canvas.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativeNode {
    /// Stable id inside the scene.
    pub id: String,
    /// Layer label. Omitted on older documents.
    #[serde(default)]
    pub name: Option<String>,
    /// Corner radius and drop shadow. Omitted on older documents.
    #[serde(default)]
    pub appearance: Option<CreativeAppearance>,
    /// Present on text nodes that wrap, align, or carry spans.
    #[serde(default)]
    pub text_layout: Option<CreativeTextLayout>,
    /// `None` is a root. Otherwise the parent group or frame.
    pub parent_id: Option<String>,
    /// Local box. Rotation is degrees, clockwise in y-down space, about the box center.
    pub transform: CreativeTransform,
    /// Multiplier on this node and the children drawn inside it.
    pub opacity: f64,
    /// `false` skips the node and its descendants.
    pub visible: bool,
    /// Stored for the editor. The viewer still draws a locked node.
    pub locked: bool,
    /// What the node paints.
    pub content: CreativeContent,
}

/// Translation, size, and rotation of a node.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativeTransform {
    /// Local origin x.
    pub x: f64,
    /// Local origin y.
    pub y: f64,
    /// Box width. Also the rotation center's horizontal half-extent.
    pub width: f64,
    /// Box height. Also the rotation center's vertical half-extent.
    pub height: f64,
    /// Degrees clockwise, y down.
    pub rotation: f64,
}

/// Corner radius and an optional drop shadow.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativeAppearance {
    /// Local units. Clamped to half the shorter side of a surface.
    #[serde(default)]
    pub corner_radius: f64,
    /// `None` draws no shadow.
    #[serde(default)]
    pub shadow: Option<CreativeShadow>,
}

/// Gaussian shadow in local units.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativeShadow {
    /// Horizontal offset, y down.
    pub x: f64,
    /// Vertical offset, y down.
    pub y: f64,
    /// Standard deviation.
    pub blur: f64,
    /// CSS color. Alpha is multiplied by [`Self::opacity`].
    pub color: String,
    /// Extra coverage multiplier, `0.0..=1.0`.
    pub opacity: f64,
}

/// Wrap width, alignment, and inline spans.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativeTextLayout {
    /// `None` keeps one line per hard break.
    pub width: Option<f64>,
    /// Horizontal alignment inside [`Self::width`].
    pub align: CreativeTextAlign,
    /// Line box as a multiple of the font size. The viewer records it; the
    /// text engine still uses the face's own line height.
    pub line_height: f64,
    /// Extra advance between clusters, in local units.
    #[serde(default)]
    pub letter_spacing: Option<f64>,
    /// Extra gap after a hard break, in local units.
    #[serde(default)]
    pub paragraph_spacing: Option<f64>,
    /// Underline every run that does not set its own flag.
    #[serde(default)]
    pub underline: Option<bool>,
    /// Non-overlapping UTF-16 ranges.
    #[serde(default)]
    pub spans: Vec<CreativeTextSpan>,
}

/// Horizontal alignment of a text node.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreativeTextAlign {
    /// Flush to the start of the line.
    Left,
    /// Centered in the wrap width.
    Center,
    /// Flush to the end of the line.
    Right,
}

/// One inline style range. Offsets are UTF-16 code units, inclusive of `start`
/// and exclusive of `end`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativeTextSpan {
    /// First UTF-16 code unit.
    pub start: u32,
    /// First UTF-16 code unit after the range.
    pub end: u32,
    /// `None` keeps the node's weight.
    pub font_weight: Option<u16>,
    /// `None` keeps the node's style.
    pub font_style: Option<CreativeFontStyle>,
    /// `None` keeps the node's color.
    pub color: Option<String>,
    /// `None` keeps [`CreativeTextLayout::underline`].
    pub underline: Option<bool>,
}

/// Upright or italic.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreativeFontStyle {
    /// Upright.
    Normal,
    /// Italic.
    Italic,
}

/// What a node draws.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreativeContent {
    /// Pressure-sensitive stroke, stored as samples and filled from its outline.
    Ink {
        /// CSS color.
        color: String,
        /// Device that produced the samples.
        input: CreativeInputKind,
        /// Width, thinning, and smoothing.
        brush: CreativeBrush,
        /// Samples in the node's local space.
        points: Vec<CreativePoint>,
    },
    /// A text block.
    Text {
        /// Hard breaks are `\n`.
        text: String,
        /// CSS color of runs no span tints.
        color: String,
        /// Family name. `"Inter"` matches the face bundled with the desktop crate.
        font_family: String,
        /// Em size in local units.
        font_size: f64,
        /// `None` is 400.
        #[serde(default)]
        font_weight: Option<u16>,
        /// `None` is upright.
        #[serde(default)]
        font_style: Option<CreativeFontStyle>,
    },
    /// Rectangle, ellipse, or line.
    Shape {
        /// Which primitive.
        shape: CreativeShapeKind,
        /// CSS fill. `"transparent"` skips the fill.
        fill: String,
        /// CSS stroke. `"transparent"` skips the stroke.
        stroke: String,
        /// Stroke width in local units.
        stroke_width: f64,
        /// Endpoints and arrow flags. Read when `shape` is [`CreativeShapeKind::Line`].
        #[serde(default)]
        line: Option<CreativeLine>,
    },
    /// A raster, cropped and clipped to the node box.
    Image {
        /// Key into the scene's asset map. The viewer loads `assets/{id}.png`.
        asset_id: String,
        /// Normalized crop of the unit square. `None` draws the whole image.
        crop: Option<CreativeCrop>,
    },
    /// Children in the parent's space. A group does not clip.
    Group,
    /// A labeled board. Children are clipped to the rounded box.
    Frame {
        /// Board title. The viewer does not paint it; export omits editor chrome.
        name: String,
        /// CSS fill of the board.
        background: String,
    },
}

/// Device that recorded an ink sample.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreativeInputKind {
    /// Stylus.
    Pen,
    /// Finger.
    Touch,
    /// Mouse.
    Mouse,
    /// Not recorded.
    Unknown,
}

/// Brush parameters stored with an ink node.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativeBrush {
    /// Pen, pencil, marker, or highlighter. The outline uses the same radius
    /// formula for each; kind is recorded so a document round-trips.
    pub kind: CreativeBrushKind,
    /// Diameter in local units.
    pub size: f64,
    /// How far pressure moves the radius off the base size. `0` is constant width.
    pub thinning: f64,
    /// Pulls interior samples toward their neighbors.
    pub smoothing: f64,
    /// Mixed into the smoothing amount.
    pub streamline: f64,
    /// Coverage of the filled outline, `0.0..=1.0`.
    pub opacity: f64,
}

/// Named brush. The outline math does not branch on this.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreativeBrushKind {
    /// Fine pressure-sensitive stroke.
    Pen,
    /// Same outline, typically a smaller size.
    Pencil,
    /// Same outline, typically a wider size.
    Marker,
    /// Same outline, typically wide and translucent.
    Highlighter,
}

/// One ink sample.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativePoint {
    /// Local x.
    pub x: f64,
    /// Local y.
    pub y: f64,
    /// `0.0..=1.0`. `None` derives a stand-in from speed.
    #[serde(default)]
    pub pressure: Option<f64>,
    /// Milliseconds since the previous sample.
    #[serde(default)]
    pub elapsed_ms: Option<f64>,
    /// Stylus tilt.
    #[serde(default)]
    pub tilt_x: Option<f64>,
    /// Stylus tilt.
    #[serde(default)]
    pub tilt_y: Option<f64>,
}

/// Rectangle, ellipse, or straight line.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreativeShapeKind {
    /// Rounded when the node has a corner radius.
    Rectangle,
    /// Axis-aligned ellipse filling the node box.
    Ellipse,
    /// Segment from [`CreativeLine::start`] to [`CreativeLine::end`].
    Line,
}

/// Endpoints of a line, in the node's local space.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativeLine {
    /// Start, local units.
    pub start: CreativeLinePoint,
    /// End, local units.
    pub end: CreativeLinePoint,
    /// Filled triangle at the start. Not dashed.
    #[serde(default)]
    pub start_arrow: bool,
    /// Filled triangle at the end. Not dashed.
    #[serde(default)]
    pub end_arrow: bool,
    /// Dash the shaft. Arrowheads stay filled.
    #[serde(default)]
    pub dashed: bool,
}

/// A point on a line.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativeLinePoint {
    /// Local x.
    pub x: f64,
    /// Local y.
    pub y: f64,
}

/// Crop of an image, in the image's unit square (`0.0..=1.0`).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreativeCrop {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// Short name of [`CreativeContent`], for tests that check coverage.
pub fn content_kind(content: &CreativeContent) -> &'static str {
    match content {
        CreativeContent::Ink { .. } => "ink",
        CreativeContent::Text { .. } => "text",
        CreativeContent::Shape {
            shape: CreativeShapeKind::Rectangle,
            ..
        } => "rectangle",
        CreativeContent::Shape {
            shape: CreativeShapeKind::Ellipse,
            ..
        } => "ellipse",
        CreativeContent::Shape {
            shape: CreativeShapeKind::Line,
            ..
        } => "line",
        CreativeContent::Image { .. } => "image",
        CreativeContent::Group => "group",
        CreativeContent::Frame { .. } => "frame",
    }
}

/// Map a UTF-16 index onto a byte index. A past-the-end index is the byte length.
pub fn utf16_to_byte(text: &str, units: u32) -> usize {
    let mut seen = 0u32;
    for (byte, ch) in text.char_indices() {
        if seen >= units {
            return byte;
        }
        seen += ch.len_utf16() as u32;
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_skip_a_surrogate_pair() {
        let text = "A😀B";
        assert_eq!(utf16_to_byte(text, 0), 0);
        assert_eq!(utf16_to_byte(text, 1), 1);
        assert_eq!(utf16_to_byte(text, 3), 1 + "😀".len());
        assert_eq!(utf16_to_byte(text, 4), text.len());
    }
}
