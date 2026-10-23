//! Glyph outlines for em sizes at or above [`rndr_paint::OUTLINE_PX`], and for
//! any run whose transform is not axis-aligned.
//!
//! An axis-aligned run below that size keeps its atlas sprites. A larger run,
//! or a rotated one, is filled from the face outlines so the edges stay smooth.
//! The builtin bitmap face has no outlines and keeps its sprites. Rotated
//! bitmap sprites are sampled linearly.

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
