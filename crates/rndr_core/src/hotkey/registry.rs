//! Registration refcount, backend calls, and press delivery.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::event::{self, Event};
use crate::input::Input;
use crate::keys::{legacy_key, platform_of, KeyCode, KeyEvent, Keystroke, LogicalKey, Trigger};
use crate::window;
use crate::world::World;

use super::accel::{platform_accel, PlatformAccel};
use super::{HotkeyBinding, HotkeyError, IntoHotkey};

/// Failure reported by a [`HotkeyBackend`]. The registry maps it onto [`HotkeyError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyFailure {
    /// The OS already gave this chord to another client.
    Taken,
    /// This session cannot bind system-wide hotkeys.
    Unsupported,
    /// The platform representation does not exist on this machine.
    Invalid,
}

/// OS or test double that owns the grabs.
///
/// `poll` returns presses queued since the last call. A real backend pushes
/// those from the thread the OS woke and does not touch the world.
pub trait HotkeyBackend: Send {
    fn register(&mut self, id: HotkeyId, accel: &PlatformAccel) -> Result<(), HotkeyFailure>;
    fn unregister(&mut self, id: HotkeyId);
    fn poll(&mut self) -> Vec<HotkeyId>;
}

impl HotkeyBackend for Box<dyn HotkeyBackend> {
    fn register(&mut self, id: HotkeyId, accel: &PlatformAccel) -> Result<(), HotkeyFailure> {
        (**self).register(id, accel)
    }

    fn unregister(&mut self, id: HotkeyId) {
        (**self).unregister(id)
    }

    fn poll(&mut self) -> Vec<HotkeyId> {
        (**self).poll()
    }
}

/// Id assigned to one OS grab. Re-registering the same resolved chord returns it again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HotkeyId(u32);

impl HotkeyId {
    pub fn get(self) -> u32 {
        self.0
    }

    /// Rebuild the id a platform callback echoed back.
    pub fn from_raw(id: u32) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for HotkeyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A system-wide chord was pressed. Readable on the frame that wakes for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalHotkey {
    pub id: HotkeyId,
    pub binding: HotkeyBinding,
}

impl Event for GlobalHotkey {}

/// Drops or [`HotkeyHandle::unregister`] release one owner of the grab.
///
/// The OS registration stays until the last handle is gone. Registering the
/// same resolved chord again returns another handle for that same id and does
/// not call the backend a second time.
#[must_use = "dropping the handle unregisters the hotkey"]
pub struct HotkeyHandle {
    id: HotkeyId,
    hub: Arc<Hub>,
    released: bool,
}

impl std::fmt::Debug for HotkeyHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HotkeyHandle")
            .field("id", &self.id)
            .finish()
    }
}

impl HotkeyHandle {
    pub fn id(&self) -> HotkeyId {
        self.id
    }

    /// Release this owner. The last owner tells the backend to drop the grab.
    pub fn unregister(mut self) {
        self.release();
    }

    fn release(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        let id = self.id;
        let remove = {
            let mut state = lock(&self.hub.state);
            let Some(refs) = state.refs.get_mut(&id) else {
                return;
            };
            *refs = refs.saturating_sub(1);
            if *refs > 0 {
                return;
            }
            state.refs.remove(&id);
            state.toggles.remove(&id);
            state.bindings.remove(&id);
            if let Some(accel) = state.by_id.remove(&id) {
                state.by_accel.remove(&accel);
            }
            true
        };
        if remove {
            lock(&self.hub.backend).unregister(id);
        }
    }
}

impl Drop for HotkeyHandle {
    fn drop(&mut self) {
        self.release();
    }
}

struct Hub {
    state: Mutex<State>,
    cv: Condvar,
    backend: Mutex<Box<dyn HotkeyBackend>>,
}

struct State {
    next_id: u32,
    by_accel: HashMap<PlatformAccel, Entry>,
    by_id: HashMap<HotkeyId, PlatformAccel>,
    refs: HashMap<HotkeyId, usize>,
    toggles: HashSet<HotkeyId>,
    bindings: HashMap<HotkeyId, HotkeyBinding>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            next_id: 1,
            by_accel: HashMap::new(),
            by_id: HashMap::new(),
            refs: HashMap::new(),
            toggles: HashSet::new(),
            bindings: HashMap::new(),
        }
    }
}

enum Entry {
    /// A thread is inside the backend call for this chord.
    Pending,
    Live(HotkeyId),
}

#[derive(Clone)]
pub(crate) struct HotkeyHub {
    hub: Arc<Hub>,
}

pub(crate) fn installed(world: &World) -> bool {
    world.has_resource::<HotkeyHub>()
}

pub(crate) fn install(world: &mut World, backend: impl HotkeyBackend + 'static) {
    if installed(world) {
        return;
    }
    world.insert_resource(HotkeyHub {
        hub: Arc::new(Hub {
            state: Mutex::new(State::default()),
            cv: Condvar::new(),
            backend: Mutex::new(Box::new(backend)),
        }),
    });
}

pub(crate) fn register(
    world: &World,
    binding: impl IntoHotkey,
    toggle: bool,
) -> Result<HotkeyHandle, HotkeyError> {
    let binding = binding.into_hotkey()?;
    let accel = platform_accel(&binding, platform_of(world))?;
    let hub = world
        .resource::<HotkeyHub>()
        .map(|installed| Arc::clone(&installed.hub))
        .ok_or(HotkeyError::Unsupported)?;
    register_in(hub, binding, accel, toggle)
}

fn register_in(
    hub: Arc<Hub>,
    binding: HotkeyBinding,
    accel: PlatformAccel,
    toggle: bool,
) -> Result<HotkeyHandle, HotkeyError> {
    let id = match claim(&hub, &accel)? {
        Claim::Live(id) => return Ok(handle(&hub, id)),
        Claim::Mine(id) => id,
    };

    let registered = lock(&hub.backend).register(id, &accel);
    let mut state = lock(&hub.state);
    match registered {
        Ok(()) => {
            state.by_accel.insert(accel.clone(), Entry::Live(id));
            state.by_id.insert(id, accel);
            state.refs.insert(id, 1);
            state.bindings.insert(id, binding);
            if toggle {
                state.toggles.insert(id);
            }
            hub.cv.notify_all();
            drop(state);
            Ok(handle(&hub, id))
        }
        Err(failure) => {
            state.by_accel.remove(&accel);
            hub.cv.notify_all();
            drop(state);
            Err(map_failure(failure))
        }
    }
}

enum Claim {
    /// The chord is already grabbed. The refcount was incremented.
    Live(HotkeyId),
    /// This caller owns the pending slot and must call the backend.
    Mine(HotkeyId),
}

fn claim(hub: &Hub, accel: &PlatformAccel) -> Result<Claim, HotkeyError> {
    let mut state = lock(&hub.state);
    loop {
        match state.by_accel.get(accel) {
            Some(Entry::Live(id)) => {
                let id = *id;
                let refs = state.refs.get_mut(&id).expect("live hotkey has a refcount");
                *refs += 1;
                return Ok(Claim::Live(id));
            }
            Some(Entry::Pending) => {
                state = hub.cv.wait(state).unwrap_or_else(|err| err.into_inner());
            }
            None => {
                let id = state.alloc()?;
                state.by_accel.insert(accel.clone(), Entry::Pending);
                return Ok(Claim::Mine(id));
            }
        }
    }
}

impl State {
    fn alloc(&mut self) -> Result<HotkeyId, HotkeyError> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(HotkeyError::Unsupported)?;
        if id == 0 {
            return Err(HotkeyError::Unsupported);
        }
        Ok(HotkeyId(id))
    }
}

fn handle(hub: &Arc<Hub>, id: HotkeyId) -> HotkeyHandle {
    HotkeyHandle {
        id,
        hub: Arc::clone(hub),
        released: false,
    }
}

fn map_failure(failure: HotkeyFailure) -> HotkeyError {
    match failure {
        HotkeyFailure::Taken => HotkeyError::AlreadyTaken,
        HotkeyFailure::Unsupported => HotkeyError::Unsupported,
        HotkeyFailure::Invalid => HotkeyError::InvalidBinding,
    }
}

/// Receive-order of keyboard events, published by the desktop runner.
///
/// The runner bumps this when a key arrives, which can be earlier than the
/// frame that copies the event into [`Input`]. A hotkey arms against this
/// sequence so a key-up queued before that press cannot clear the guard.
#[derive(Clone, Copy, Debug, Default)]
struct KeyOrder {
    seq: u64,
}

/// Record that the platform delivered a key event.
///
/// `seq` is monotonic. A later call with a smaller value is ignored. Headless
/// tests leave this unset; key events then use [`KeyEvent::UNORDERED`] and
/// count as arriving after any hotkey.
pub fn note_key_order(world: &mut World, seq: u64) {
    if let Some(order) = world.resource_mut::<KeyOrder>() {
        if seq > order.seq {
            order.seq = seq;
        }
        return;
    }
    world.insert_resource(KeyOrder { seq });
}

/// Physical keys held because a global hotkey just fired.
///
/// The value is the receive-order just before the key-up that is allowed to
/// release the guard. Tracked by [`KeyCode`] rather than the modifiers on the
/// event that arrives later.
#[derive(Default)]
struct ArmedKeys {
    codes: HashMap<KeyCode, u64>,
}

pub(crate) fn drive(world: &mut World) {
    let fired = deliver_presses(world);
    suppress_hotkey_keys(world, &fired);
}

fn deliver_presses(world: &mut World) -> Vec<HotkeyBinding> {
    let Some(installed) = world.resource::<HotkeyHub>() else {
        return Vec::new();
    };
    let hub = Arc::clone(&installed.hub);
    let presses = lock(&hub.backend).poll();
    if presses.is_empty() {
        return Vec::new();
    }
    let planned: Vec<(HotkeyId, HotkeyBinding, bool)> = {
        let state = lock(&hub.state);
        presses
            .into_iter()
            .filter_map(|id| {
                if !state.refs.contains_key(&id) {
                    return None;
                }
                let binding = *state.bindings.get(&id)?;
                let toggle = state.toggles.contains(&id);
                Some((id, binding, toggle))
            })
            .collect()
    };
    let mut fired = Vec::with_capacity(planned.len());
    for (id, binding, toggle) in planned {
        event::send(world, GlobalHotkey { id, binding });
        if toggle {
            window::toggle(world);
        }
        fired.push(binding);
    }
    fired
}

/// Drop chord matches, synthetic focus events, the armed base key, and releases.
///
/// A release is not text. Synthetic key-ups and key-downs (winit focus changes
/// on X11 and Windows) are not text either, and a synthetic key-up does not
/// release the guard. macOS and Wayland leave `synthetic` false.
///
/// A key-up releases the guard only when it was received after the hotkey.
/// Events already in this frame's queue count: the runner may have stamped
/// them before this call. A key-up stamped earlier, and delivered only once
/// the hidden window catches up, does not.
fn suppress_hotkey_keys(world: &mut World, fired: &[HotkeyBinding]) {
    let strokes = registered_strokes(world);
    let platform = platform_of(world);
    let clock = key_order(world);
    let queued = world
        .resource::<Input>()
        .and_then(|input| queued_span(&input.keys));
    let armed_at = arm_at(clock, queued);
    let mut armed = world
        .resource_mut::<ArmedKeys>()
        .map(|armed| std::mem::take(&mut armed.codes))
        .unwrap_or_default();
    for binding in fired {
        if let Some(code) = trigger_code(binding.keystroke()) {
            armed.insert(code, armed_at);
        }
    }
    let live: HashSet<KeyCode> = strokes
        .iter()
        .filter_map(|stroke| trigger_code(*stroke))
        .collect();
    armed.retain(|code, _| live.contains(code));

    let Some(input) = world.resource_mut::<Input>() else {
        store_armed(world, armed);
        return;
    };
    let before = input.keys.len();
    let mut kept = Vec::with_capacity(before);
    for event in input.keys.drain(..) {
        if event.synthetic {
            continue;
        }
        if event.released {
            if let Some(code) = source_code(&event) {
                if release_clears(event.order, armed.get(&code).copied()) {
                    armed.remove(&code);
                }
            }
            continue;
        }
        if strokes
            .iter()
            .any(|stroke| stroke.matches(&event, platform))
        {
            continue;
        }
        if held_hotkey(&event, &armed) {
            continue;
        }
        kept.push(event);
    }
    let changed = kept.len() != before;
    input.keys = kept;
    if changed {
        input.chars.clear();
        for event in &input.keys {
            if let Some(text) = event.insertable_text() {
                input.chars.push_str(&text);
            }
        }
        input.key = input
            .keys
            .first()
            .and_then(|event| legacy_key(event, platform));
    }
    store_armed(world, armed);
}

fn key_order(world: &World) -> u64 {
    world
        .resource::<KeyOrder>()
        .map(|order| order.seq)
        .unwrap_or(0)
}

/// Oldest and newest receive-order already sitting in this frame.
fn queued_span(keys: &[KeyEvent]) -> Option<(u64, u64)> {
    let mut orders = keys
        .iter()
        .map(|event| event.order)
        .filter(|order| *order != KeyEvent::UNORDERED);
    let first = orders.next()?;
    Some(orders.fold((first, first), |(min, max), order| {
        (min.min(order), max.max(order))
    }))
}

/// Sequence a key-up must pass to release a guard armed on this frame.
///
/// The clock is the cutoff when this frame's queue is empty or older than the
/// clock: the runner has stamped key-ups that are not in the queue yet, which
/// is what happens while the window is hidden. When the queue includes the
/// newest stamp, those events were delivered with this hotkey, and the cutoff
/// sits just before the oldest of them so a real key-up in the queue still
/// releases.
fn arm_at(clock: u64, queued: Option<(u64, u64)>) -> u64 {
    match queued {
        Some((min, max)) if max == clock => min.saturating_sub(1),
        _ => clock,
    }
}

fn release_clears(order: u64, armed_at: Option<u64>) -> bool {
    let Some(armed_at) = armed_at else {
        return false;
    };
    order == KeyEvent::UNORDERED || order > armed_at
}

fn store_armed(world: &mut World, codes: HashMap<KeyCode, u64>) {
    if codes.is_empty() {
        if let Some(armed) = world.resource_mut::<ArmedKeys>() {
            armed.codes.clear();
        }
        return;
    }
    world.insert_resource(ArmedKeys { codes });
}

fn registered_strokes(world: &World) -> Vec<Keystroke> {
    let Some(installed) = world.resource::<HotkeyHub>() else {
        return Vec::new();
    };
    let state = lock(&installed.hub.state);
    state
        .bindings
        .values()
        .map(|binding| binding.keystroke())
        .collect()
}

fn trigger_code(stroke: Keystroke) -> Option<KeyCode> {
    match stroke.trigger {
        Trigger::Code(code) => Some(normalize_code(code)),
        Trigger::Logical(LogicalKey::Named(named)) => named.code(),
        Trigger::Logical(LogicalKey::Character(ch)) => KeyCode::letter(ch),
        Trigger::Logical(LogicalKey::Unidentified) => None,
    }
}

fn source_code(event: &KeyEvent) -> Option<KeyCode> {
    if let Some(code) = event.code {
        return Some(normalize_code(code));
    }
    match event.logical {
        LogicalKey::Named(named) => named.code(),
        LogicalKey::Character(ch) => KeyCode::letter(ch),
        LogicalKey::Unidentified => None,
    }
}

fn normalize_code(code: KeyCode) -> KeyCode {
    match code {
        KeyCode::Letter(ch) => KeyCode::Letter(ch.to_ascii_lowercase()),
        other => other,
    }
}

fn held_hotkey(event: &KeyEvent, armed: &HashMap<KeyCode, u64>) -> bool {
    if armed.is_empty() {
        return false;
    }
    if let Some(code) = source_code(event) {
        if armed.contains_key(&code) {
            return true;
        }
    }
    if event.code.is_none() {
        if let Some(code) = event.text.as_deref().and_then(text_source) {
            return armed.contains_key(&code);
        }
    }
    false
}

fn text_source(text: &str) -> Option<KeyCode> {
    let mut chars = text.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    if ch == ' ' {
        Some(KeyCode::Space)
    } else {
        KeyCode::letter(ch)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}
