//! Native rendering of [Liquid Markdown](rndr_liquid) in an rndr markdown view.
//!
//! [`liquid_view`] parses Liquid source, shows the ordinary markdown with
//! [`rndr_core::markdown_view`], and mounts every Liquid block in a
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
mod host;
mod render;
mod view;

pub use host::{FeedEntry, LiquidEvent, LiquidHost, LiquidImage, NoHost};
pub use render::plain;
pub use view::{dispatch, liquid_view, refresh, set_liquid_source, LiquidView};

use rndr_core::{App, Plugin, Schedule};

/// Mounts Liquid blocks after markdown sync and tracks chart hover.
///
/// Add it after [`rndr_core::MarkdownPlugin`] so blocks mount in the same
/// frame the markdown parses. It installs [`rndr_ui::UiPlugin`] if needed.
pub struct LiquidUiPlugin {
    host: Box<dyn LiquidHost>,
}

impl LiquidUiPlugin {
    /// Plugin that sends block requests to `host`.
    pub fn new(host: impl LiquidHost + 'static) -> Self {
        Self {
            host: Box::new(host),
        }
    }
}

impl Plugin for LiquidUiPlugin {
    fn build(self, app: &mut App) {
        app.add_plugin(rndr_ui::UiPlugin);
        app.world_mut()
            .insert_resource(view::HostSlot(Some(self.host)));
        app.add_system(Schedule::Update, chart::hover);
        app.add_system(Schedule::PostUpdate, view::reconcile);
    }
}
