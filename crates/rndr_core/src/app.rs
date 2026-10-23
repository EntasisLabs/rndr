use std::sync::Arc;
use std::time::Instant;

use crate::clock::Clock;
use crate::event;
use crate::font::apply_text_layout;
use crate::input::{Input, Viewport};
use crate::keys::Platform;
use crate::layout::layout_propagate;
use crate::mono::apply_mono_grid;
use crate::runtime::{self, Executor, Wake};
use crate::text::ensure_text_engine;
use crate::tray;
use crate::window;
use crate::world::World;

/// Named stages of one frame, in the order [`App::update`] runs them.
///
/// Paint is not a schedule. The paint step calls [`App::update`], draws the quads,
/// then calls [`App::render`].
///
/// 1. [`Startup`](Schedule::Startup) — once, before the first Input.
/// 2. [`Input`](Schedule::Input) — window dismiss, then keymap, list navigation,
///    and text fields. Focus and blur from the platform are already readable.
///    Events sent here, including `TextChanged`, `Submitted`, and a dismiss
///    blur, are merged before Update so an application system on this frame
///    can read them.
/// 3. [`Update`](Schedule::Update) — application systems.
/// 4. [`PostUpdate`](Schedule::PostUpdate) — widgets that follow application
///    writes. The virtual list rebuilds here, after a system has changed `List::count`,
///    and once more after layout when that height was not known yet.
/// 5. [`Layout`](Schedule::Layout) — user layout systems, then the hierarchy walk
///    and text measurement.
/// 6. Paint — the quad walk. Not a system list.
/// 7. [`Render`](Schedule::Render) — after paint, before present.
///
/// An Input system does not see events sent earlier in that same Input pass.
/// Read them from Update. A system that writes `List::count` belongs on Update,
/// not after the list has already rebuilt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Schedule {
    /// Once, before the first [`Schedule::Input`]. Later frames skip it.
    Startup,
    /// Keymap, list keys, and text fields. Runs before [`Schedule::Update`].
    Input,
    /// Application systems. Events sent during [`Schedule::Input`] are readable here.
    Update,
    /// After [`Schedule::Update`] and before layout.
    PostUpdate,
    /// User layout systems, then the hierarchy walk and text measurement.
    Layout,
    /// After paint, before present.
    Render,
}

/// Function [`App`] calls with the world for one schedule pass.
pub type System = fn(&mut World);

/// Registers systems and resources on a blank [`App`].
///
/// [`App::add_plugin`] calls [`Plugin::build`] immediately and returns the app
/// so plugins can be chained. `build` may insert resources and register systems.
pub trait Plugin {
    /// Install this plugin's resources and systems on `app`.
    fn build(self, app: &mut App);
}

/// Headless runner. One frame is Startup (once), Input, Update, PostUpdate, then Layout.
/// Render is the platform's job, after paint. A new app owns [`Input`] and [`Viewport`]
/// only. [`crate::Shell`] installs the desktop systems.
pub struct App {
    world: World,
    startup: Vec<System>,
    input: Vec<System>,
    update: Vec<System>,
    post_update: Vec<System>,
    layout: Vec<System>,
    render: Vec<System>,
    started: bool,
}

impl App {
    /// Blank app. Inserts default [`Input`], [`Viewport`], and [`Platform::host`],
    /// plus [`crate::Time`] and the window and tray slots. Startup has not run.
    pub fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(Input::default());
        world.insert_resource(Viewport::default());
        world.insert_resource(Platform::host());
        runtime::install(&mut world);
        window::install(&mut world);
        tray::install(&mut world);
        Self {
            world,
            startup: Vec::new(),
            input: Vec::new(),
            update: Vec::new(),
            post_update: Vec::new(),
            layout: Vec::new(),
            render: Vec::new(),
            started: false,
        }
    }

    /// Run [`Plugin::build`] now. Returns `self` so calls can chain.
    pub fn add_plugin(&mut self, plugin: impl Plugin) -> &mut Self {
        plugin.build(self);
        self
    }

    /// Append `system` so it runs after systems already on `schedule`.
    ///
    /// A [`Schedule::Startup`] system added after the first [`App::update`]
    /// never runs. Returns `self` so calls can chain.
    pub fn add_system(&mut self, schedule: Schedule, system: System) -> &mut Self {
        self.insert_system(schedule, system, false)
    }

    /// Insert at the front of a schedule so the system runs before ones already registered.
    ///
    /// A [`Schedule::Startup`] system added after the first [`App::update`]
    /// never runs.
    pub fn add_system_front(&mut self, schedule: Schedule, system: System) -> &mut Self {
        self.insert_system(schedule, system, true)
    }

    fn insert_system(&mut self, schedule: Schedule, system: System, front: bool) -> &mut Self {
        let list = match schedule {
            Schedule::Startup => &mut self.startup,
            Schedule::Input => &mut self.input,
            Schedule::Update => &mut self.update,
            Schedule::PostUpdate => &mut self.post_update,
            Schedule::Layout => &mut self.layout,
            Schedule::Render => &mut self.render,
        };
        if front {
            list.insert(0, system);
        } else {
            list.push(system);
        }
        self
    }

    /// Shared borrow of the world, including resources plugins have inserted.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Exclusive borrow of the world. Plugins and tests insert resources through it.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Replace the [`Clock`]. [`crate::Sim`] installs a [`crate::ManualClock`].
    pub fn set_clock(&mut self, clock: Arc<dyn Clock>) -> &mut Self {
        runtime::set_clock(self.world(), clock);
        self
    }

    /// Wake hook for the window loop. Called when a timer is armed or a task finishes.
    pub fn set_wake(&mut self, wake: Arc<dyn Wake>) -> &mut Self {
        runtime::set_wake(self.world(), wake);
        self
    }

    /// Background executor. The desktop runner installs the `pool` feature's thread pool.
    pub fn set_executor(&mut self, executor: Arc<dyn Executor>) -> &mut Self {
        runtime::set_executor(self.world_mut(), executor);
        self
    }

    /// Wall instant of the soonest timer, if any are armed.
    pub fn next_deadline(&self) -> Option<Instant> {
        runtime::next_deadline(self.world())
    }

    /// Inbox, ready task, due timer, or a [`crate::feed`] value waiting to be
    /// delivered. The runner requests a redraw when this is set.
    pub fn wants_frame(&self) -> bool {
        runtime::wants_frame(self.world())
    }

    /// One frame: Startup once, then Input, Update, PostUpdate, Layout.
    ///
    /// The pump ticks [`crate::Time`], drops the previous frame's events, and commits
    /// task and timer output. Global hotkey presses are delivered next, and a
    /// toggle chord queues show or hide before window commands run. Those
    /// commands, including ones that arrived from tasks or the platform, are
    /// applied next. Tray clicks and menu activations are mapped into their
    /// event queues, and those events are merged so Input can read them.
    /// [`ScaleChanged`](crate::ScaleChanged) and
    /// [`Resized`](crate::Resized) were published before this frame and are
    /// already in the readable queues.
    ///
    /// Input then runs. Dismiss claims a bare Escape before the keymap and
    /// text fields. Events this stage sends, including `TextChanged` and a
    /// dismiss blur, are merged before Update. Layout systems run, then the
    /// hierarchy walk. Window commands queued by systems are applied after
    /// that, so the runner can show or hide on this frame; those focus events
    /// stay queued until the next frame.
    pub fn update(&mut self) {
        ensure_text_engine(&mut self.world);
        let scale = self
            .world
            .resource::<crate::Viewport>()
            .map(|viewport| viewport.pixels_per_point())
            .unwrap_or(1.0);
        if let Some(engine) = self.world.resource::<Arc<dyn crate::TextEngine>>() {
            engine.begin_frame();
            engine.set_pixels_per_point(scale);
        }
        runtime::tick_time(&mut self.world);
        if !self.started {
            for system in self.startup.clone() {
                system(&mut self.world);
            }
            tray::startup(&mut self.world);
            self.started = true;
        }
        event::flip_events(&mut self.world);
        runtime::run_until_parked(&mut self.world);
        // Hotkey presses wake this frame. Toggle runs before window commands
        // so a show/hide from the chord is applied below and the event is
        // readable in Input.
        crate::hotkey::drive(&mut self.world);
        window::drive(&mut self.world);
        tray::drive(&mut self.world);
        event::merge_events(&mut self.world);
        window::apply_dismiss(&mut self.world);
        for system in self.input.clone() {
            system(&mut self.world);
        }
        // TextChanged, ListActivated, and a dismiss blur are readable below.
        event::merge_events(&mut self.world);
        for system in self.update.clone() {
            system(&mut self.world);
        }
        for system in self.post_update.clone() {
            system(&mut self.world);
        }
        for system in self.layout.clone() {
            system(&mut self.world);
        }
        layout_propagate(&mut self.world);
        // PostUpdate measured the list before layout assigned a height, so the
        // first pass builds no rows. Rebuild once the rect is known, then lay
        // the new rows out. Two passes cover a fill child of a fixed parent.
        for _ in 0..2 {
            if !crate::list::sync_viewport(&mut self.world) {
                break;
            }
            layout_propagate(&mut self.world);
        }
        apply_text_layout(&mut self.world);
        apply_mono_grid(&mut self.world);
        crate::editor::apply_decorations(&mut self.world);
        window::drive(&mut self.world);
    }

    /// Extra render-schedule systems. The surface runs these after paint and before present.
    pub fn render(&mut self) {
        for system in self.render.clone() {
            system(&mut self.world);
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
