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

impl Component for EffectSlot {}

/// Box whose pixels an app-registered GPU effect draws, in paint order.
///
/// Core and paint draw nothing for this entity, and its children paint after
/// the effect. The render crate calls the effect registered for [`Self::id`]
/// at this point in the frame, inside any faded or transformed group around
/// it, so the group's opacity and transform apply to what the effect draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectSlot {
    /// Effect id the app registered with the renderer.
    pub id: u64,
    /// Four values handed to the effect each frame, such as a colour or a phase.
    pub data: [f32; 4],
    /// Give the effect a copy of everything drawn beneath it this frame.
    pub backdrop: bool,
    /// Keep requesting frames while this slot is shown, for effects that
    /// change on their own.
    pub animated: bool,
}

impl EffectSlot {
    /// Slot for effect `id`, with zero data, no backdrop and no animation.
    pub fn new(id: u64) -> Self {
        Self {
            id,
            data: [0.0; 4],
            backdrop: false,
            animated: false,
        }
    }

    /// Replace [`Self::data`].
    pub fn with_data(mut self, data: [f32; 4]) -> Self {
        self.data = data;
        self
    }

    /// Request a copy of the pixels beneath the slot.
    pub fn with_backdrop(mut self) -> Self {
        self.backdrop = true;
        self
    }

    /// Keep frames coming while the slot is shown.
    pub fn animated(mut self) -> Self {
        self.animated = true;
        self
    }
}

/// Some shown [`EffectSlot`] is animated and the window is visible.
pub fn effects_animating(world: &crate::world::World) -> bool {
    crate::window::is_visible(world)
        && world
            .entities_with::<EffectSlot>()
            .into_iter()
            .any(|entity| {
                world
                    .get::<EffectSlot>(entity)
                    .is_some_and(|slot| slot.animated)
                    && world.get::<crate::Rect>(entity).is_some()
                    && !hidden_in_tree(world, entity)
            })
}

fn hidden_in_tree(world: &crate::world::World, mut entity: crate::Entity) -> bool {
    loop {
        if world.has::<crate::Hidden>(entity) {
            return true;
        }
        match world.get::<crate::Parent>(entity) {
            Some(parent) => entity = parent.entity,
            None => return false,
        }
    }
}
