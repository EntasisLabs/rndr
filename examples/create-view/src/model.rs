//! Saved Create scene types, from [`rndr_canvas`].
//!
//! `rndr_canvas` reads Medousa Create's `medousa-create` version 3 documents
//! as well as its own `rndr-canvas` header. Unknown fields are rejected, so a
//! newer document fails loudly instead of dropping a property the viewer never
//! read.

pub use rndr_canvas as canvas;
pub use rndr_canvas_ui::paint::utf16_to_byte;

use canvas::{Content, ShapeKind};

/// Short name of [`Content`], for tests that check coverage.
pub fn content_kind(content: &Content) -> &'static str {
    match content {
        Content::Ink { .. } => "ink",
        Content::Text { .. } => "text",
        Content::Shape {
            shape: ShapeKind::Rectangle,
            ..
        } => "rectangle",
        Content::Shape {
            shape: ShapeKind::Ellipse,
            ..
        } => "ellipse",
        Content::Shape {
            shape: ShapeKind::Line,
            ..
        } => "line",
        Content::Image { .. } => "image",
        Content::Group => "group",
        Content::Frame { .. } => "frame",
    }
}
