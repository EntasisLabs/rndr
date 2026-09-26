use crate::entity::Entity;
use crate::input::{Focus, Input, Key};
use crate::world::World;

/// A key edge plus the modifiers that have to match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
}

impl Chord {
    pub const fn new(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
        }
    }

    pub const fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }
}

/// One named chord. `scope` limits it to that focused entity. `consume` clears
/// the key so later systems (caret, focus, save) do not also see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    pub chord: Chord,
    pub scope: Option<Entity>,
    pub consume: bool,
}

impl Binding {
    pub fn new(name: impl Into<String>, chord: Chord) -> Self {
        Self {
            name: name.into(),
            chord,
            scope: None,
            consume: false,
        }
    }

    pub fn scope(mut self, entity: Entity) -> Self {
        self.scope = Some(entity);
        self
    }

    pub fn consume(mut self) -> Self {
        self.consume = true;
        self
    }
}

/// Bindings consulted once per update. The first matching scoped binding wins,
/// then the first global binding.
#[derive(Clone, Debug, Default)]
pub struct Actions {
    pub bindings: Vec<Binding>,
}

/// The one action fired this frame, if any.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fired {
    pub name: Option<String>,
}

/// Resolve [`Actions`] against [`Input`] and [`Focus`]. A consumed chord clears
/// the key. An unbound key is left for the caret and the file commands.
pub fn apply_actions(world: &mut World) {
    let matched = resolve(world);
    let name = matched.as_ref().map(|binding| binding.name.clone());
    let consume = matched.as_ref().is_some_and(|binding| binding.consume);
    if let Some(fired) = world.resource_mut::<Fired>() {
        fired.name = name;
    }
    if consume {
        if let Some(input) = world.resource_mut::<Input>() {
            input.key = None;
        }
    }
}

fn resolve(world: &World) -> Option<Binding> {
    let input = world.resource::<Input>()?;
    let key = input.key?;
    let actions = world.resource::<Actions>()?;
    let focus = world.resource::<Focus>().and_then(|focus| focus.entity);
    let mut global = None;
    for binding in &actions.bindings {
        if binding.chord.key != key
            || binding.chord.ctrl != input.ctrl
            || binding.chord.shift != input.shift
        {
            continue;
        }
        if binding.scope.is_some() && binding.scope == focus {
            return Some(binding.clone());
        }
        if binding.scope.is_none() && global.is_none() {
            global = Some(binding.clone());
        }
    }
    global
}
