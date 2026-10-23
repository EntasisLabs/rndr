//! Image ids for the display list.
//!
//! [`load_image`] decodes on the background executor and delivers the pixels
//! through a [`rndr_core::feed`]. [`load_rgba`] stores pixels that are already
//! decoded. The renderer uploads them into a GPU cache and then drops the CPU
//! copy. A texture the cache evicts is not decoded again.

use std::collections::HashMap;
use std::sync::Arc;

use rndr_core::feed::Sender;
use rndr_core::{events, feed, spawn_detached, Event, Feed, FeedConfig, World};

use crate::decode::decode_image;

pub use crate::decode::{DecodeError, Raster};

/// Default GPU cache budget, in bytes of texture memory. 256 MiB.
pub const IMAGE_BUDGET_DEFAULT: u64 = 256 * 1024 * 1024;

/// One decoded image, delivered on the UI thread.
#[derive(Clone, Debug)]
pub struct DecodedImage {
    /// Id passed to [`load_image`] or [`load_rgba`].
    pub id: u64,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Straight RGBA, top row first.
    pub rgba: Arc<[u8]>,
}

impl Event for DecodedImage {}

/// Why [`load_rgba`] rejected the buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// `width` or `height` is zero.
    Empty,
    /// `rgba` is shorter than `width * height * 4`.
    Short,
}

struct ImageHub {
    /// Kept so the sender stays open.
    _feed: Feed<DecodedImage>,
    sender: Sender<DecodedImage>,
}

struct Stored {
    width: u32,
    height: u32,
    rgba: Option<Arc<[u8]>>,
}

/// Decoded images waiting for, or retained after, a GPU upload.
#[derive(Default)]
pub(crate) struct ImageStore {
    images: HashMap<u64, Stored>,
    budget: u64,
}

impl ImageStore {
    fn ensure(world: &mut World) -> &mut Self {
        if !world.has_resource::<ImageStore>() {
            world.insert_resource(ImageStore {
                images: HashMap::new(),
                budget: IMAGE_BUDGET_DEFAULT,
            });
        }
        world.resource_mut::<ImageStore>().expect("image store")
    }
}

/// Store `rgba` under `id`. A later frame uploads it.
///
/// Replaces an image already stored at `id`.
pub fn load_rgba(
    world: &mut World,
    id: u64,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Result<(), ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::Empty);
    }
    let need = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if rgba.len() < need {
        return Err(ImageError::Short);
    }
    let rgba: Arc<[u8]> = Arc::from(rgba[..need].to_vec());
    ImageStore::ensure(world).images.insert(
        id,
        Stored {
            width,
            height,
            rgba: Some(rgba),
        },
    );
    Ok(())
}

/// Decode `bytes` off the UI thread and store the image under `id`.
///
/// PNG, JPEG, and WebP are accepted. A bad payload is dropped. The pixels
/// arrive as a [`DecodedImage`] event on a later pump, which the renderer
/// folds into the store before it draws.
pub fn load_image(world: &mut World, id: u64, bytes: Vec<u8>) {
    let sender = hub_sender(world);
    spawn_detached(world, async move {
        if let Ok(raster) = decode_image(&bytes) {
            let _ = sender.try_send(DecodedImage {
                id,
                width: raster.width,
                height: raster.height,
                rgba: Arc::from(raster.rgba),
            });
        }
    })
    .detach();
}

/// GPU texture budget in bytes. The default is [`IMAGE_BUDGET_DEFAULT`].
///
/// The cache evicts least-recently-used textures until an upload fits. One
/// image larger than the budget is box-downsampled until it fits.
pub fn set_image_budget(world: &mut World, bytes: u64) {
    ImageStore::ensure(world).budget = bytes.max(64 * 1024);
}

fn hub_sender(world: &mut World) -> Sender<DecodedImage> {
    if world.resource::<ImageHub>().is_none() {
        let (feed, sender) = feed::open(world, FeedConfig::bounded(64, rndr_core::OnFull::Reject));
        world.insert_resource(ImageHub {
            _feed: feed,
            sender: sender.clone(),
        });
    }
    world
        .resource::<ImageHub>()
        .expect("image feed")
        .sender
        .clone()
}

/// Copy [`DecodedImage`] events into the store. Called once per present.
pub(crate) fn ingest(world: &mut World) {
    let incoming: Vec<DecodedImage> = events::<DecodedImage>(world).to_vec();
    if incoming.is_empty() {
        return;
    }
    let store = ImageStore::ensure(world);
    for image in incoming {
        store.images.insert(
            image.id,
            Stored {
                width: image.width,
                height: image.height,
                rgba: Some(image.rgba),
            },
        );
    }
}

pub(crate) struct CpuImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

pub(crate) fn pending(world: &World) -> Vec<(u64, CpuImage)> {
    let Some(store) = world.resource::<ImageStore>() else {
        return Vec::new();
    };
    store
        .images
        .iter()
        .filter_map(|(id, image)| {
            image.rgba.as_ref().map(|rgba| {
                (
                    *id,
                    CpuImage {
                        width: image.width,
                        height: image.height,
                        rgba: Arc::clone(rgba),
                    },
                )
            })
        })
        .collect()
}

pub(crate) fn budget_of(world: &World) -> u64 {
    world
        .resource::<ImageStore>()
        .map(|store| store.budget)
        .unwrap_or(IMAGE_BUDGET_DEFAULT)
}

/// Drop CPU pixels for `ids` after the GPU cache owns the texture.
pub(crate) fn release_cpu(world: &mut World, ids: &[u64]) {
    let Some(store) = world.resource_mut::<ImageStore>() else {
        return;
    };
    for id in ids {
        if let Some(image) = store.images.get_mut(id) {
            image.rgba = None;
        }
    }
}

/// [`DecodeError`] re-export for callers that decode ahead of [`load_rgba`].
pub fn decode(bytes: &[u8]) -> Result<Raster, DecodeError> {
    decode_image(bytes)
}
