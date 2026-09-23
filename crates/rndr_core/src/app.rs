use crate::input::{Input, Viewport};
use crate::layout::layout_propagate;
use crate::world::World;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Schedule {
    Startup,
    Update,
    Layout,
    Render,
}

pub type System = fn(&mut World);

/// Registers systems and resources on a blank [`App`].
pub trait Plugin {
    fn build(self, app: &mut App);
}

/// Headless runner: Startup once, then Update, then Layout. Render is the platform's job.
/// A new app owns [`Input`] and [`Viewport`] only. [`crate::Shell`] installs the desktop systems.
pub struct App {
    world: World,
    startup: Vec<System>,
    update: Vec<System>,
    layout: Vec<System>,
    render: Vec<System>,
    started: bool,
}

impl App {
    pub fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(Input::default());
        world.insert_resource(Viewport::default());
        let app = Self {
            world,
            startup: Vec::new(),
            update: Vec::new(),
            layout: Vec::new(),
            render: Vec::new(),
            started: false,
        };
        app
    }

    pub fn add_plugin(&mut self, plugin: impl Plugin) -> &mut Self {
        plugin.build(self);
        self
    }

    pub fn add_system(&mut self, schedule: Schedule, system: System) -> &mut Self {
        match schedule {
            Schedule::Startup => self.startup.push(system),
            Schedule::Update => self.update.push(system),
            Schedule::Layout => self.layout.push(system),
            Schedule::Render => self.render.push(system),
        }
        self
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Runs Startup (once), Update, user Layout systems, then the hierarchy walk.
    pub fn update(&mut self) {
        if !self.started {
            for system in self.startup.clone() {
                system(&mut self.world);
            }
            self.started = true;
        }
        for system in self.update.clone() {
            system(&mut self.world);
        }
        for system in self.layout.clone() {
            system(&mut self.world);
        }
        layout_propagate(&mut self.world);
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
