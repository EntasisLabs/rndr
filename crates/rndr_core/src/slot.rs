use crate::world::Component;

impl Component for SurfaceSlot {}

/// Rectangle reserved for a foreign surface, or for a canvas painter.
///
/// Core does not draw it and does not link a webview. The host places a foreign
/// surface on the rect `frame` reports. A canvas painter registered for
/// [`Self::id`] draws the interior in the same frame, under the UI.
///
/// Spawn this on the entity whose [`crate::Rect`] is the surface. The render
/// plugin skips that entity's quad and still walks its children. Core does
/// not allocate the id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSlot {
    /// Opaque id the host assigned. Reported with the laid-out rect.
    pub id: u64,
}
