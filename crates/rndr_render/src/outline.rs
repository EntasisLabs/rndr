//! Glyph outlines for em sizes at or above [`rndr_paint::OUTLINE_PX`].
//!
//! Below that size the atlas sprites are the drawn glyphs. Above it, a canvas
//! run is filled from the face outlines so a zoomed or rotated run stays sharp.
//! The builtin bitmap face has no outlines and keeps its sprites.

use rndr_core::{Color, World};
use rndr_paint::{Path, TextSource};

/// Filled outlines for `source`, in the text's local space. Empty when the
/// active engine cannot supply them.
pub(crate) fn outlines(world: &World, source: &TextSource) -> Vec<(Path, Color)> {
    #[cfg(feature = "parley")]
    {
        use rndr_core::TextEngine;
        use std::sync::Arc;
        world
            .resource::<Arc<dyn TextEngine>>()
            .and_then(|engine| engine.as_any().downcast_ref::<crate::text::ParleyText>())
            .map(|parley| parley.glyph_outlines(source))
            .unwrap_or_default()
    }
    #[cfg(not(feature = "parley"))]
    {
        let _ = (world, source);
        Vec::new()
    }
}
