//! Content-first layout recipes. These configure core Layout, never a second tree.
use crate::{Space, Typography, UiError, UiTheme};
use rndr_core::*;

/// Resolved desktop workspace metrics, in logical points. The host supplies the
/// *content* width (after its own padding), not the window width. Text scaling
/// changes the sidebar breakpoint; density changes spacing, never type size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorkspaceLayout {
    /// Navigation beside the document, or stacked before it in document order.
    pub side_by_side: bool,
    /// Resolved inter-plane gap.
    pub gap: f32,
    /// Navigation width in a row; full available width when stacked.
    pub navigation_width: f32,
    /// Document plane width, before that plane's own padding.
    pub document_width: f32,
    /// Actual bounded content width used to resolve this layout.
    pub available: f32,
}
impl WorkspaceLayout {
    /// Quiet sidebar + dominant document. Nonfinite widths resolve to zero;
    /// extremely narrow hosts may scroll vertically, never force a wide row.
    pub fn resolve(available: f32, theme: &UiTheme) -> Self {
        let available = if available.is_finite() {
            available.max(0.0)
        } else {
            0.0
        };
        let scale = theme.typography(Typography::Label).size / 14.0;
        let gap = theme.space(Space::X16);
        let navigation = 240.0 * scale;
        // Leave at least 440 scaled points for the primary content plane.
        let side_by_side = available >= navigation + gap + 440.0 * scale;
        Self {
            side_by_side,
            gap,
            navigation_width: if side_by_side { navigation } else { available },
            document_width: if side_by_side {
                available - navigation - gap
            } else {
                available
            },
            available,
        }
    }
    /// Apply to an existing row/column and its direct navigation child. This
    /// explicit operation owns root axis/gap and navigation width, preserving
    /// all identities, content, focus, editor/history and authored other fields.
    /// Invalid relations are rejected before either node is changed.
    pub fn apply(self, world: &mut World, root: Entity, navigation: Entity) -> Result<(), UiError> {
        if !world.has::<Layout>(root)
            || !world.has::<Layout>(navigation)
            || world.get::<Parent>(navigation).map(|p| p.entity) != Some(root)
        {
            return Err(UiError::Stale);
        }
        if [
            self.gap,
            self.navigation_width,
            self.document_width,
            self.available,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err(UiError::InvalidProps);
        }
        let layout = world.get_mut::<Layout>(root).unwrap();
        layout.axis = if self.side_by_side {
            Axis::Row
        } else {
            Axis::Column
        };
        layout.gap = self.gap;
        world.get_mut::<Layout>(navigation).unwrap().width = if self.side_by_side {
            Size::Px(self.navigation_width)
        } else {
            Size::Fill
        };
        Ok(())
    }
}

/// Restrained section layout with density-aware rhythm, no automatic surface.
/// Use on caller-owned content slots when a second filled card is unnecessary.
pub fn section_layout(theme: &UiTheme) -> Layout {
    Layout {
        width: Size::Fill,
        gap: theme.space(Space::X12),
        ..Layout::column()
    }
}
