use crate::world::Component;

impl Component for SurfaceSlot {}

/// Rectangle reserved for a foreign surface. Core does not draw it and does not
/// link a webview. The host places that surface on the rect `frame` reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSlot {
    pub id: u64,
}
