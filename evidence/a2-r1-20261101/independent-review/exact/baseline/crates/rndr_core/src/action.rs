use crate::entity::Entity;
use crate::input::{Focus, Input, Key};
use crate::world::World;

/// A key edge plus the modifiers that have to match.
///
/// [`apply_actions`] compares this with the legacy [`Input::key`],
/// [`Input::ctrl`], and [`Input::shift`]. The [`Input::keys`] queue is not
/// consulted. A held Alt or Super still matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    /// Legacy key edge this chord listens for. Alt and Super are not part of a chord.
    pub key: Key,
    /// When `true`, Ctrl must be held. When `false`, Ctrl must be released.
    pub ctrl: bool,
    /// When `true`, Shift must be held. When `false`, Shift must be released.
    pub shift: bool,
}

impl Chord {
    /// Chord on `key` with Ctrl and Shift released.
    pub const fn new(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            shift: false,
        }
    }

    /// Require Ctrl to be held. Shift is left as it was.
    pub const fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    /// Require Shift to be held. Ctrl is left as it was.
    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }
}

/// One named chord. `scope` limits it to that focused entity. `consume` clears
/// the key so later systems (caret, focus, save) do not also see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    /// Name copied into [`Fired`] when this binding wins.
    pub name: String,
    /// Key edge and Ctrl/Shift state that have to match.
    pub chord: Chord,
    /// Focus required for a match. `None` is global and loses to any scoped hit.
    pub scope: Option<Entity>,
    /// When `true`, a win clears [`Input::key`] so later systems do not see it.
    pub consume: bool,
}

impl Binding {
    /// Binding named `name` on `chord`, with no scope and the key left in place.
    pub fn new(name: impl Into<String>, chord: Chord) -> Self {
        Self {
            name: name.into(),
            chord,
            scope: None,
            consume: false,
        }
    }

    /// Match only while [`Focus`] is `entity`.
    pub fn scope(mut self, entity: Entity) -> Self {
        self.scope = Some(entity);
        self
    }

    /// Clear [`Input::key`] when this binding is the one that fires.
    pub fn consume(mut self) -> Self {
        self.consume = true;
        self
    }
}

/// Bindings consulted once per update. The first matching scoped binding wins,
/// then the first global binding.
#[derive(Clone, Debug, Default)]
pub struct Actions {
    /// Match list. Earlier entries win inside the same scope class.
    pub bindings: Vec<Binding>,
}

/// The one action fired this frame, if any.
///
/// [`apply_actions`] and [`crate::Button`] both write [`Fired::name`]. Read it
/// after those systems have run. [`crate::Chrome`] runs actions, then buttons,
/// on Update, and reads this name for copy, cut, and paste on PostUpdate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fired {
    /// [`Binding::name`] or [`crate::Button::action`] for this frame.
    /// `None` when neither has fired.
    pub name: Option<String>,
}

/// [`Fired`] name for copy (`"copy"`).
///
/// [`crate::Chrome`] binds [`Key::Copy`] to this name and consumes the key.
pub const COPY: &str = "copy";
/// [`Fired`] name for cut (`"cut"`).
///
/// [`crate::Chrome`] binds [`Key::Cut`] to this name and consumes the key.
pub const CUT: &str = "cut";
/// [`Fired`] name for paste (`"paste"`).
///
/// [`crate::Chrome`] binds [`Key::Paste`] to this name and consumes the key.
pub const PASTE: &str = "paste";

/// Resolve [`Actions`] against [`Input`] and [`Focus`]. A consumed chord clears
/// the key. An unbound key is left for the caret and the file commands.
///
/// Stores the winning [`Binding::name`] on [`Fired`] when that resource
/// exists, or `None` when nothing matches. A missing [`Input`], legacy key,
/// or [`Actions`] resource is no match. The first binding whose
/// [`Binding::scope`] equals the focused entity wins. With no scoped hit, the
/// first binding that has no scope wins. A missing [`Focus`] resource means
/// only globals can match.
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
