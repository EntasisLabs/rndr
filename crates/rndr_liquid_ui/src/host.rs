//! What a Liquid view asks of the application.
use rndr_core::World;

/// A request a rendered block makes when the reader activates it.
///
/// Nothing here has been acted on. Intents and URLs are untrusted document
/// text; the host decides what, if anything, each one means.
#[derive(Clone, Debug, PartialEq)]
pub enum LiquidEvent {
    /// An `actions` button, a recipe action, or any other labelled action.
    Action {
        /// Button label.
        label: String,
        /// The `intent:` the document attached, if any.
        intent: Option<String>,
    },
    /// A citation or source link.
    OpenLink {
        /// Destination as written.
        href: String,
    },
    /// The copy button of a `code` block.
    Copy {
        /// Source text to place on the clipboard.
        text: String,
    },
    /// A recipe step's timer button.
    StartTimer {
        /// Step label.
        label: String,
        /// Duration in milliseconds.
        duration_ms: u64,
    },
    /// The refresh button of a `feed` block.
    RefreshFeed {
        /// Feed id as written.
        feed_id: String,
    },
}

/// An image the host has loaded for a `media`, `card` or `plan` source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidImage {
    /// Texture id for [`rndr_core::ImageView`].
    pub texture: u64,
    /// Width over height.
    pub aspect: f32,
}

/// One row of a `feed` block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FeedEntry {
    /// Primary line.
    pub title: String,
    /// Optional wrapped detail.
    pub detail: Option<String>,
    /// Optional trailing metadata, such as a time.
    pub meta: Option<String>,
}

/// Application hooks for actions, links, media, feeds, timers and clipboard.
///
/// Every method has a default that does nothing, so a host implements only
/// what it supports. Install one with [`crate::LiquidUiPlugin::new`].
pub trait LiquidHost: Send + Sync {
    /// The reader activated something that leaves the view.
    fn event(&mut self, world: &mut World, event: LiquidEvent) {
        let _ = (world, event);
    }

    /// Resolve an image source. `None` shows the alt text or caption.
    fn image(&mut self, world: &mut World, src: &str) -> Option<LiquidImage> {
        let _ = (world, src);
        None
    }

    /// Current rows of a feed. Called when the block is mounted and after
    /// [`crate::refresh`].
    fn feed(&mut self, world: &mut World, feed_id: &str, datatype: &str) -> Vec<FeedEntry> {
        let _ = (world, feed_id, datatype);
        Vec::new()
    }

    /// Current value of a feed-bound dashboard tile, or `None` to keep the
    /// value the document wrote.
    fn tile(&mut self, world: &mut World, feed_id: &str, field: Option<&str>) -> Option<String> {
        let _ = (world, feed_id, field);
        None
    }
}

/// A host that ignores every request.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHost;

impl LiquidHost for NoHost {}
