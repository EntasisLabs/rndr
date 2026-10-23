//! Context keymap. Bindings map keystroke sequences to named actions.
//!
//! The focused element's context stack is the [`Context`] names from the root
//! down to that element. A binding's path matches when it appears as a
//! contiguous slice of that stack. The longest match wins. At the same length,
//! the later binding wins.

use crate::hierarchy::Parent;
use crate::input::{Focus, Input};
use crate::keys::{legacy_key, KeyEvent, Keystroke, Platform};
use crate::world::{Component, World};

impl Component for Context {}

/// One layer of the keymap context stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    /// Name matched against a binding path. Compared as a whole string.
    pub name: String,
}

impl Context {
    /// One stack layer named `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// Milliseconds since an arbitrary origin. Tests advance this to expire chords.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    /// Milliseconds since the origin the app chose. [`Keymap::dispatch`]
    /// expires a partial chord against this.
    pub millis: u64,
}

/// Actions fired by [`apply_keymap`] this frame, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeymapFired {
    /// Action names in the order [`Keymap::dispatch`] produced them.
    pub actions: Vec<String>,
}

/// Split `"Palette > List"` into `["Palette", "List"]`.
pub fn context_path(path: &str) -> Vec<String> {
    path.split('>')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

/// Root-first context names for `entity`, walking [`Parent`] links.
pub fn context_stack(world: &World, entity: crate::entity::Entity) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = Some(entity);
    let mut seen = Vec::new();
    while let Some(entity) = current {
        if seen.contains(&entity) || seen.len() > 64 {
            break;
        }
        seen.push(entity);
        if let Some(context) = world.get::<Context>(entity) {
            names.push(context.name.clone());
        }
        current = world.get::<Parent>(entity).map(|parent| parent.entity);
    }
    names.reverse();
    names
}

/// One binding. An empty context matches every stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBinding {
    /// Name recorded on [`KeymapFired`] when the strokes match.
    pub action: String,
    /// Chord in order. Empty strokes never match.
    pub strokes: Vec<Keystroke>,
    /// Contiguous context slice. Empty matches every stack.
    pub context: Vec<String>,
}

impl KeyBinding {
    /// Binding with an empty context, so it matches every stack.
    pub fn new(action: impl Into<String>, strokes: impl IntoIterator<Item = Keystroke>) -> Self {
        Self {
            action: action.into(),
            strokes: strokes.into_iter().collect(),
            context: Vec::new(),
        }
    }

    /// Replace the context with [`context_path`] of `path`.
    pub fn context(mut self, path: &str) -> Self {
        self.context = context_path(path);
        self
    }

    /// Display text for these strokes. See [`Keymap::shortcut`].
    pub fn shortcut(&self, platform: Platform) -> String {
        display_strokes(&self.strokes, platform)
    }
}

fn display_strokes(strokes: &[Keystroke], platform: Platform) -> String {
    strokes
        .iter()
        .map(|stroke| stroke.display(platform))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug)]
struct Win {
    action: String,
    spec: usize,
}

/// What [`Keymap::dispatch`] did with one batch of events.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dispatch {
    /// Actions that completed this call, including a chord that timed out.
    pub actions: Vec<String>,
    /// Indexes into the event slice that were eaten as a chord or a prefix.
    pub consumed: Vec<usize>,
}

/// Bindings plus the chord waiting for its next key.
///
/// A binding matches when its context is a contiguous slice of the focus
/// stack. The longest slice wins. At the same length, the later binding wins.
#[derive(Clone, Debug)]
pub struct Keymap {
    /// Candidates in registration order. A later binding wins a tie at the
    /// same context depth.
    pub bindings: Vec<KeyBinding>,
    /// How long a partial chord stays armed, in milliseconds.
    pub timeout_ms: u64,
    pending: Vec<KeyEvent>,
    pending_at: u64,
    /// Exact action held back because a longer chord is still possible.
    held: Option<String>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
            timeout_ms: 1000,
            pending: Vec::new(),
            pending_at: 0,
            held: None,
        }
    }
}

impl Keymap {
    /// Append `binding`. A later binding wins a tie at the same context depth.
    pub fn bind(&mut self, binding: KeyBinding) -> &mut Self {
        self.bindings.push(binding);
        self
    }

    /// Strokes held for a partial chord. Zero when nothing is armed.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Feed `events` in order. `now_ms` expires a partial chord when it is past
    /// [`Keymap::timeout_ms`]. A timed-out chord fires when it was itself a binding.
    pub fn dispatch(
        &mut self,
        stack: &[String],
        events: &[KeyEvent],
        platform: Platform,
        now_ms: u64,
    ) -> Dispatch {
        let mut out = Dispatch::default();
        if let Some(action) = self.expire(now_ms) {
            out.actions.push(action);
        }
        let mut index = 0;
        while index < events.len() {
            let event = &events[index];
            let mut candidate = self.pending.clone();
            candidate.push(event.clone());
            let exact = self.winner(stack, &candidate, platform);
            let min_spec = exact.as_ref().map(|win| win.spec).unwrap_or(0);
            let longer = self.continues(stack, &candidate, platform, min_spec, true);
            let matched = exact.is_some() || self.continues(stack, &candidate, platform, 0, false);
            if !matched {
                if !self.pending.is_empty() {
                    if let Some(action) = self.held.take() {
                        out.actions.push(action);
                    }
                    self.pending.clear();
                    continue;
                }
                index += 1;
                continue;
            }
            out.consumed.push(index);
            if let Some(win) = exact {
                if longer {
                    self.held = Some(win.action);
                    self.pending = candidate;
                    self.pending_at = now_ms;
                } else {
                    out.actions.push(win.action);
                    self.held = None;
                    self.pending.clear();
                }
            } else {
                self.held = None;
                self.pending = candidate;
                self.pending_at = now_ms;
            }
            index += 1;
        }
        out
    }

    /// Keystroke sequences that would fire `action` on `stack`.
    ///
    /// A sequence is omitted when a deeper context, or a later binding of the
    /// same depth, takes those same strokes.
    pub fn lookup(&self, action: &str, stack: &[String]) -> Vec<Vec<Keystroke>> {
        let mut found = Vec::new();
        for (index, binding) in self.bindings.iter().enumerate() {
            if binding.action != action || binding.strokes.is_empty() {
                continue;
            }
            if !context_matches(&binding.context, stack) {
                continue;
            }
            if self.owns_sequence(index, stack) {
                found.push(binding.strokes.clone());
            }
        }
        found
    }

    /// Display text for the binding that fires `action` on `stack`.
    ///
    /// macOS uses symbols (`⌘K`). Other platforms use words (`Ctrl+K`).
    /// A two-stroke chord joins with a space (`Ctrl+G M`). Empty when `action`
    /// is not bound on `stack`. When several sequences win, this is the first.
    pub fn shortcut(&self, action: &str, stack: &[String], platform: Platform) -> String {
        self.lookup(action, stack)
            .first()
            .map(|strokes| display_strokes(strokes, platform))
            .unwrap_or_default()
    }

    fn expire(&mut self, now_ms: u64) -> Option<String> {
        if self.pending.is_empty() {
            return None;
        }
        if now_ms.saturating_sub(self.pending_at) <= self.timeout_ms {
            return None;
        }
        self.pending.clear();
        self.held.take()
    }

    fn winner(&self, stack: &[String], events: &[KeyEvent], platform: Platform) -> Option<Win> {
        let mut best: Option<(usize, usize, String)> = None;
        for (index, binding) in self.bindings.iter().enumerate() {
            if binding.strokes.len() != events.len() || binding.strokes.is_empty() {
                continue;
            }
            if !context_matches(&binding.context, stack) {
                continue;
            }
            if !strokes_match(&binding.strokes, events, platform) {
                continue;
            }
            let spec = binding.context.len();
            let replace = match &best {
                None => true,
                Some((best_spec, best_index, _)) => {
                    spec > *best_spec || (spec == *best_spec && index > *best_index)
                }
            };
            if replace {
                best = Some((spec, index, binding.action.clone()));
            }
        }
        best.map(|(spec, _, action)| Win { action, spec })
    }

    fn continues(
        &self,
        stack: &[String],
        events: &[KeyEvent],
        platform: Platform,
        min_spec: usize,
        longer_only: bool,
    ) -> bool {
        let n = events.len();
        self.bindings.iter().any(|binding| {
            if binding.strokes.len() < n || binding.strokes.is_empty() {
                return false;
            }
            if longer_only && binding.strokes.len() == n {
                return false;
            }
            if binding.context.len() < min_spec {
                return false;
            }
            if !context_matches(&binding.context, stack) {
                return false;
            }
            strokes_match(&binding.strokes[..n], events, platform)
        })
    }

    fn owns_sequence(&self, index: usize, stack: &[String]) -> bool {
        let binding = &self.bindings[index];
        let spec = binding.context.len();
        for (other_index, other) in self.bindings.iter().enumerate() {
            if other.strokes != binding.strokes || other.strokes.is_empty() {
                continue;
            }
            if !context_matches(&other.context, stack) {
                continue;
            }
            let other_spec = other.context.len();
            if other_spec > spec || (other_spec == spec && other_index > index) {
                return false;
            }
        }
        true
    }
}

pub(crate) fn context_matches(pattern: &[String], stack: &[String]) -> bool {
    if pattern.is_empty() {
        return true;
    }
    stack.windows(pattern.len()).any(|window| window == pattern)
}

fn strokes_match(strokes: &[Keystroke], events: &[KeyEvent], platform: Platform) -> bool {
    strokes.len() == events.len()
        && strokes
            .iter()
            .zip(events)
            .all(|(stroke, event)| stroke.matches(event, platform))
}

/// Match [`Input::keys`] against the [`Keymap`] and drop chords it consumed.
///
/// Partial chords are removed from the queue so a text field does not insert
/// them. When the queue was non-empty, the deprecated [`Input::key`] shim is
/// recomputed from whatever remains.
pub fn apply_keymap(world: &mut World) {
    if world.resource::<Keymap>().is_none() {
        return;
    }
    let platform = crate::keys::platform_of(world);
    let now = world
        .resource::<Clock>()
        .map(|clock| clock.millis)
        .unwrap_or(0);
    let stack = focused_stack(world);
    let events = world
        .resource::<Input>()
        .map(|input| input.keys.clone())
        .unwrap_or_default();
    let had_keys = !events.is_empty();
    let dispatch = {
        let keymap = world.resource_mut::<Keymap>().unwrap();
        keymap.dispatch(&stack, &events, platform, now)
    };
    if world.resource::<KeymapFired>().is_some() {
        world.resource_mut::<KeymapFired>().unwrap().actions = dispatch.actions;
    } else {
        world.insert_resource(KeymapFired {
            actions: dispatch.actions,
        });
    }
    if dispatch.consumed.is_empty() {
        return;
    }
    let Some(input) = world.resource_mut::<Input>() else {
        return;
    };
    let mut keep = Vec::with_capacity(input.keys.len());
    for (index, event) in input.keys.iter().cloned().enumerate() {
        if !dispatch.consumed.contains(&index) {
            keep.push(event);
        }
    }
    input.keys = keep;
    if had_keys {
        input.key = input
            .keys
            .first()
            .and_then(|event| legacy_key(event, platform));
        input.chars.clear();
        for event in &input.keys {
            if let Some(text) = event.insertable_text() {
                input.chars.push_str(&text);
            }
        }
    }
}

fn focused_stack(world: &World) -> Vec<String> {
    let Some(entity) = world.resource::<Focus>().and_then(|focus| focus.entity) else {
        return Vec::new();
    };
    if !world.is_alive(entity) {
        return Vec::new();
    }
    context_stack(world, entity)
}
