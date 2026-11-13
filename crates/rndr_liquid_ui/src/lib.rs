//! Native rendering of [Liquid Markdown](rndr_liquid) in an rndr markdown view.
//!
//! [`liquid_view`] parses Liquid source, shows the ordinary markdown with
//! [`rndr_core::markdown_view`], and mounts each Liquid block in the visible
//! window in a
//! [markdown host slot](rndr_core::mount_markdown_slot) as `rndr_ui`
//! surfaces and controls. Charts are painted with [`rndr_chart`].
//!
//! Anything that leaves the view is the application's: actions, links,
//! clipboard, timers, images and feeds go through a [`LiquidHost`].
//!
//! ```
//! use rndr_core::*;
//! use rndr_liquid_ui::{liquid_view, LiquidUiPlugin, LiquidView, NoHost};
//!
//! let mut app = App::new();
//! app.add_plugin(rndr_ui::UiPlugin);
//! app.add_plugin(MarkdownPlugin);
//! app.add_plugin(LiquidUiPlugin::new(NoHost));
//! app.world_mut().insert_resource(Viewport::new(640.0, 480.0));
//! let view = liquid_view(
//!     app.world_mut(),
//!     "Intro\n\n```callout\ntone: tip\ntitle: Hello\nbody: Body\n```\n",
//! );
//! app.update();
//! assert_eq!(app.world().get::<LiquidView>(view).unwrap().mounted(), vec![0]);
//! ```
//!
//! Each frame, drain [`take_action_intents`](rndr_core::take_action_intents)
//! and hand every request to [`dispatch`]; it returns `false` for requests
//! that are not from a Liquid block.
#![warn(missing_docs)]

mod chart;
mod feed;
mod host;
mod identity;
mod lifecycle;
pub use identity::{
    inspect_liquid_occurrences, liquid_component_handle, liquid_component_state_for,
    liquid_source_revision, put_liquid_component_state_for, reset_liquid_block,
    reset_liquid_surface, set_liquid_source_keyed, subscribe_liquid_interactions_owned_for,
    LiquidBlockHandle, LiquidF2Error, LiquidInteractionEnvelopeFor, LiquidInteractionHandlerFor,
    LiquidKeyMap, LiquidOccurrenceDescription, LiquidSourceRevision, LiquidSubscribeErrorFor,
};
mod render;
mod state;
mod timer;
mod view;

pub use feed::{put_liquid_feed_result, put_liquid_feed_result_for, LiquidFeedContext};
pub use host::{
    FeedBody, FeedDatatype, FeedEntry, FeedOutcome, FeedRequest, LiquidEvent, LiquidHost,
    LiquidImage, NoHost,
};
pub use lifecycle::{liquid_surface_handle, LiquidLimits, LiquidSurfaceHandle, LiquidUpdateError};
pub use render::plain;
pub use state::{
    liquid_component_state, liquid_component_states, put_liquid_component_state,
    subscribe_liquid_interactions, FeedPhase, FeedSlot, LiquidComponentRecord,
    LiquidComponentState, LiquidDisposition, LiquidInteraction, LiquidInteractionEnvelope,
    LiquidInteractionHandler, LiquidRevisionError,
};
pub use state::{
    subscribe_liquid_interactions_owned, unsubscribe_liquid_interactions,
    LiquidInteractionSubscription, LiquidListenerOwner, LiquidSubscribeError,
};
pub use timer::{
    liquid_timer, liquid_timers, LiquidTimerAction, LiquidTimerRecord, LiquidTimerStatus,
};
pub use view::{dispatch, liquid_view, push_liquid_str, refresh, set_liquid_source, LiquidView};

use rndr_core::{App, Plugin, Schedule};

/// Mounts Liquid blocks after markdown sync and tracks chart hover.
///
/// Add it after [`rndr_core::MarkdownPlugin`] so blocks mount in the same
/// frame the markdown parses. It installs [`rndr_ui::UiPlugin`] if needed.
pub struct LiquidUiPlugin {
    limits: LiquidLimits,
    host: Box<dyn LiquidHost>,
}

impl LiquidUiPlugin {
    /// Plugin that sends block requests to `host`.
    pub fn new(host: impl LiquidHost + 'static) -> Self {
        Self {
            limits: LiquidLimits::default(),
            host: Box::new(host),
        }
    }
}

impl LiquidUiPlugin {
    /// Configure admission before plugin installation. Zero budgets are refused.
    pub fn with_limits(mut self, limits: LiquidLimits) -> Result<Self, LiquidUpdateError> {
        if !limits.valid() {
            return Err(LiquidUpdateError::Capacity);
        }
        self.limits = limits;
        Ok(self)
    }
}
impl Plugin for LiquidUiPlugin {
    fn build(self, app: &mut App) {
        app.add_plugin(rndr_ui::UiPlugin);
        app.world_mut().insert_resource(self.limits);
        app.world_mut()
            .insert_resource(view::HostSlot(Some(self.host)));
        app.add_system(Schedule::Update, state::begin_pump);
        app.add_system(Schedule::Update, lifecycle::drain);
        app.add_system(Schedule::Update, chart::hover);
        app.add_system(Schedule::Update, timer::reconcile);
        app.add_system(Schedule::Update, feed::reconcile);
        app.add_system(Schedule::PostUpdate, view::reconcile);
        app.add_system(Schedule::PostUpdate, lifecycle::drain);
    }
}
