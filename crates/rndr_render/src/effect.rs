//! App GPU passes drawn at [`rndr_core::EffectSlot`]s, in paint order.

use std::collections::HashMap;
use std::sync::Mutex;

/// GPU drawing for one effect id.
///
/// The renderer calls [`Self::draw`] once per shown slot with that id, at the
/// slot's place in paint order. Everything painted before the slot is already
/// in the target; everything after draws over it. Inside a faded or transformed
/// group the target is the group's offscreen layer, and the group's opacity
/// and transform then apply to what the effect drew.
pub trait GpuEffect: Send {
    /// Record GPU work for one slot into [`EffectContext::encoder`].
    fn draw(&mut self, cx: &mut EffectContext<'_>);
}

/// Effects keyed by [`rndr_core::EffectSlot::id`].
///
/// Insert this resource into the app's world. A slot whose id has no effect
/// draws nothing.
#[derive(Default)]
pub struct GpuEffects {
    effects: HashMap<u64, Mutex<Box<dyn GpuEffect>>>,
}

impl GpuEffects {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `effect` for `id`, replacing any earlier one.
    pub fn insert(&mut self, id: u64, effect: impl GpuEffect + 'static) {
        self.effects.insert(id, Mutex::new(Box::new(effect)));
    }

    /// Drop the effect for `id`.
    pub fn remove(&mut self, id: u64) -> bool {
        self.effects.remove(&id).is_some()
    }

    /// `true` when `id` has an effect.
    pub fn contains(&self, id: u64) -> bool {
        self.effects.contains_key(&id)
    }

    pub(crate) fn run(&self, id: u64, cx: &mut EffectContext<'_>) {
        if let Some(effect) = self.effects.get(&id) {
            let mut effect = effect.lock().unwrap_or_else(|poison| poison.into_inner());
            effect.draw(cx);
        }
    }
}

/// Where one slot draws this frame. Pixel values are in the target's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectSite {
    /// [`rndr_core::EffectSlot::id`].
    pub id: u64,
    /// [`rndr_core::EffectSlot::data`].
    pub data: [f32; 4],
    /// Slot rect `[x, y, w, h]` in target pixels.
    pub rect: [f32; 4],
    /// Scissor `[x, y, w, h]` from the slot's parent clip, inside the target.
    pub clip: [u32; 4],
    /// Target pixels per logical point.
    pub scale: f32,
    /// Target size in pixels.
    pub size: [u32; 2],
    /// Colour format of the target. Build pipelines for this format.
    pub format: wgpu::TextureFormat,
    /// Sample count of the target. Build pipelines with this count.
    pub samples: u32,
    /// `true` inside a faded or transformed group's offscreen layer.
    pub in_layer: bool,
    /// [`rndr_core::Time::now`] in seconds, or zero without that resource.
    pub time: f32,
}

/// GPU access for one slot.
pub struct EffectContext<'a> {
    /// The renderer's device.
    pub device: &'a wgpu::Device,
    /// The renderer's queue. A `write_buffer` here lands before the whole
    /// frame runs; use [`Self::write_buffer`] for data that must be ordered
    /// with other slots of the same effect.
    pub queue: &'a wgpu::Queue,
    /// The frame's encoder, after everything painted before the slot.
    pub encoder: &'a mut wgpu::CommandEncoder,
    /// This slot.
    pub site: EffectSite,
    pub(crate) target: &'a wgpu::TextureView,
    pub(crate) resolve: Option<&'a wgpu::TextureView>,
    pub(crate) backdrop: Option<&'a wgpu::TextureView>,
}

impl<'a> EffectContext<'a> {
    /// Copy of the target as painted before this slot, in [`EffectSite::format`]
    /// and [`EffectSite::size`]. `None` unless the slot asked for a backdrop
    /// and the target can be copied.
    pub fn backdrop(&self) -> Option<&wgpu::TextureView> {
        self.backdrop
    }

    /// Open a render pass on the target that keeps its contents, with the
    /// scissor on [`EffectSite::clip`].
    pub fn begin_pass(&mut self) -> wgpu::RenderPass<'_> {
        let mut pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("effect"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.target,
                depth_slice: None,
                resolve_target: self.resolve,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let [x, y, w, h] = self.site.clip;
        pass.set_scissor_rect(x, y, w, h);
        pass
    }

    /// Write `bytes` into `buffer` at `offset` in encoder order, so two slots
    /// of one effect can share a buffer region. `buffer` needs `COPY_DST`;
    /// `offset` and `bytes.len()` must be multiples of 4.
    pub fn write_buffer(&mut self, buffer: &wgpu::Buffer, offset: u64, bytes: &[u8]) {
        assert!(
            offset.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
                && (bytes.len() as u64).is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
            "effect buffer writes must be 4-byte aligned"
        );
        if bytes.is_empty() {
            return;
        }
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("effect-staging"),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&staging, 0, bytes);
        self.encoder
            .copy_buffer_to_buffer(&staging, 0, buffer, offset, bytes.len() as u64);
    }
}
