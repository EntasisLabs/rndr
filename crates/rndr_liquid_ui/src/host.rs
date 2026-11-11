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

/// The five feed bodies Medousa's Liquid fence accepts.
///
/// `md`, `text`, `json`, `csv`, and `image`, matching the parser and
/// Medousa's `FeedDatatype`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedDatatype {
    /// Markdown body.
    Md,
    /// Plain text.
    Text,
    /// JSON text.
    Json,
    /// Comma-separated rows.
    Csv,
    /// Image source, passed to [`LiquidHost::image`].
    Image,
}

impl FeedDatatype {
    /// Parse a fence `datatype` value.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "md" => Some(Self::Md),
            "text" => Some(Self::Text),
            "json" => Some(Self::Json),
            "csv" => Some(Self::Csv),
            "image" => Some(Self::Image),
            _ => None,
        }
    }

    /// Fence spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Md => "md",
            Self::Text => "text",
            Self::Json => "json",
            Self::Csv => "csv",
            Self::Image => "image",
        }
    }
}

/// What the renderer asks the host to load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedRequest {
    /// Feed id from the fence.
    pub feed_id: String,
    /// Expected body kind.
    pub datatype: FeedDatatype,
    /// Dashboard field path, when this request is for one tile.
    pub field: Option<String>,
    /// Stable child id: the feed id, or the dashboard tile id.
    pub instance: String,
}

/// A loaded feed body. The host owns how it was fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedBody {
    /// Kind of `body`.
    pub datatype: FeedDatatype,
    /// Raw payload. Empty is a successful empty feed.
    pub body: String,
}

/// Result of one [`LiquidHost::load_feed`] call.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FeedOutcome {
    /// This host has no async loader. The renderer uses [`LiquidHost::feed`]
    /// or [`LiquidHost::tile`].
    #[default]
    Defer,
    /// The load is still running. Call again on a later frame.
    Pending,
    /// A finished payload. Replaces the last good body.
    Ready(FeedBody),
    /// The load failed. A previous body is kept and shown as stale.
    Failed {
        /// Host message. Shown next to the last good body when one exists.
        message: String,
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
    /// Load a local observation with an exact renderer lifetime. The default
    /// delegates to the legacy loader; it cannot prove external quiescence.
    fn load_feed_for(
        &mut self,
        world: &mut World,
        context: &crate::LiquidFeedContext,
        request: &FeedRequest,
    ) -> FeedOutcome {
        let _ = context;
        self.load_feed(world, request)
    }
    /// Release local followers/tasks only. Never cancel a durable backend job,
    /// resolve approvals, or infer backend completion from UI disposal.
    /// Cooperative hosts call `context.acknowledge_retirement()` after release.
    fn retire_feed_observation(&mut self, world: &mut World, context: &crate::LiquidFeedContext) {
        let _ = (world, context);
    }

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

    /// Async feed load. The default defers to [`Self::feed`] and [`Self::tile`].
    ///
    /// Return [`FeedOutcome::Pending`] until the bytes are ready, then
    /// [`FeedOutcome::Ready`] or [`FeedOutcome::Failed`]. The renderer does
    /// not open a network connection. A later failure keeps the last ready
    /// body.
    fn load_feed(&mut self, world: &mut World, request: &FeedRequest) -> FeedOutcome {
        let _ = (world, request);
        FeedOutcome::Defer
    }
}

/// A host that ignores every request.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoHost;

impl LiquidHost for NoHost {}
