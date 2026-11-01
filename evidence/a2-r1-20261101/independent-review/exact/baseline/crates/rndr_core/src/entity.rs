/// Index plus generation. Despawn recycles the index and bumps the generation.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    pub(crate) fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Recycled slot number. Pair it with [`Entity::generation`].
    pub fn index(self) -> u32 {
        self.index
    }

    /// Generation stored in this id. [`crate::World::is_alive`] accepts it only
    /// while the slot still holds this generation.
    pub fn generation(self) -> u32 {
        self.generation
    }
}
