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

/// One edge from a global hotkey grab. Repeats are not edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyEdge {
    /// The chord went down.
    Press(HotkeyId),
    /// The chord went up. Clears the text guard for that chord's base key.
    Release(HotkeyId),
}

/// OS or test double that owns the grabs.
///
/// [`HotkeyBackend::poll_edges`] returns edges queued since the last call. A
/// real backend pushes those from the thread the OS woke and does not touch
/// the world.
pub trait HotkeyBackend: Send {
    /// Grab `accel` under `id`.
    ///
    /// The registry calls this once per operating-system registration. A
    /// second handle for the same resolved chord does not call it again.
    /// Return [`HotkeyFailure::Taken`] when another client owns the chord,
    /// [`HotkeyFailure::Unsupported`] when this session cannot bind, and
    /// [`HotkeyFailure::Invalid`] when `accel` has no encoding here.
    fn register(&mut self, id: HotkeyId, accel: &PlatformAccel) -> Result<(), HotkeyFailure>;
    /// Release the operating-system grab for `id`.
    ///
    /// Called when the last [`HotkeyHandle`] for `id` is dropped or passed to
    /// [`HotkeyHandle::unregister`]. Earlier drops only decrement the owner
    /// count.
    fn unregister(&mut self, id: HotkeyId);
    /// Presses queued since the last call.
    ///
    /// Backends that also see a key-up should drain in [`Self::poll_edges`] instead,
    /// and implement this as the press half of that drain only when nothing
    /// else calls both.
    fn poll(&mut self) -> Vec<HotkeyId>;
    /// Presses and releases in the order they happened.
    ///
    /// The default treats every [`poll`](Self::poll) id as a press. A backend
    /// with a release path overrides this and does its draining here.
    fn poll_edges(&mut self) -> Vec<HotkeyEdge> {
        self.poll().into_iter().map(HotkeyEdge::Press).collect()
    }
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

    fn poll_edges(&mut self) -> Vec<HotkeyEdge> {
        (**self).poll_edges()
    }
}

/// Id assigned to one OS grab. Re-registering the same resolved chord returns it again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HotkeyId(u32);

impl HotkeyId {
    /// The raw id passed to [`HotkeyBackend::register`].
    ///
    /// The registry starts at 1 and does not issue 0. [`Self::from_raw`]
    /// will still rebuild a 0 when a caller passes one.
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
    /// Grab id. Every handle for this resolved chord shares it.
    pub id: HotkeyId,
    /// Chord that was registered. `primary` is still set when the binding
    /// used it. The platform fold lives on [`PlatformAccel`].
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
    /// Id shared by every handle for this resolved chord.
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

/// One armed base key.
///
/// `at` is the receive-order a key-up must pass. `released` is set when the
/// grab reports the chord up, which a window key-up on X11 never does.
#[derive(Clone, Copy)]
struct Guard {
    at: u64,
    released: bool,
}

/// Physical keys held because a global hotkey just fired.
///
/// Tracked by [`KeyCode`] rather than the modifiers on the event that arrives
/// later. Hiding the window drops the map.
#[derive(Default)]
struct ArmedKeys {
    codes: HashMap<KeyCode, Guard>,
}

/// The window hid. A guard must not still be armed on the next show.
pub(crate) fn disarm_guard(world: &mut World) {
    if let Some(armed) = world.resource_mut::<ArmedKeys>() {
        armed.codes.clear();
    }
}

pub(crate) fn drive(world: &mut World) {
    let updates = deliver_edges(world);
    suppress_hotkey_keys(world, &updates);
}

enum ChordUpdate {
    Fire(HotkeyBinding),
    Release(HotkeyBinding),
}

fn deliver_edges(world: &mut World) -> Vec<ChordUpdate> {
    let Some(installed) = world.resource::<HotkeyHub>() else {
        return Vec::new();
    };
    let hub = Arc::clone(&installed.hub);
    let edges = lock(&hub.backend).poll_edges();
    if edges.is_empty() {
        return Vec::new();
    }
    let planned: Vec<(HotkeyEdge, HotkeyBinding, bool)> = {
        let state = lock(&hub.state);
        edges
            .into_iter()
            .filter_map(|edge| {
                let id = match edge {
                    HotkeyEdge::Press(id) | HotkeyEdge::Release(id) => id,
                };
                if !state.refs.contains_key(&id) {
                    return None;
                }
                let binding = *state.bindings.get(&id)?;
                let toggle = state.toggles.contains(&id);
                Some((edge, binding, toggle))
            })
            .collect()
    };
    let mut updates = Vec::with_capacity(planned.len());
    for (edge, binding, toggle) in planned {
        match edge {
            HotkeyEdge::Press(id) => {
                event::send(world, GlobalHotkey { id, binding });
                if toggle {
                    window::toggle(world);
                }
                updates.push(ChordUpdate::Fire(binding));
            }
            HotkeyEdge::Release(_) => updates.push(ChordUpdate::Release(binding)),
        }
    }
    updates
}

/// Drop chord matches, synthetic focus events, held repeats, and releases.
///
/// A release is not text. Synthetic key-ups and key-downs (winit focus changes
/// on X11 and Windows) are not text either, and a synthetic key-up does not
/// release the guard. macOS and Wayland leave `synthetic` false.
///
/// A key-up releases the guard only when it was received after the hotkey.
/// Events already in this frame's queue count: the runner may have stamped
/// them before this call. A key-up stamped earlier, and delivered only once
/// the hidden window catches up, does not. On X11 that physical key-up is
/// delivered to the grab instead of the window, so a backend [`HotkeyEdge::Release`]
/// marks the guard released. A fresh non-repeat of the base key is the press
/// after that, and is inserted. Auto-repeat of the held chord stays out, as
/// does the base key on the frame the chord fired and any key-down queued
/// before the guard.
fn suppress_hotkey_keys(world: &mut World, updates: &[ChordUpdate]) {
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
    let mut fired_now = HashSet::new();
    for update in updates {
        match update {
            ChordUpdate::Fire(binding) => {
                if let Some(code) = trigger_code(binding.keystroke()) {
                    armed.insert(
                        code,
                        Guard {
                            at: armed_at,
                            released: false,
                        },
                    );
                    fired_now.insert(code);
                }
            }
            ChordUpdate::Release(binding) => {
                if let Some(code) = trigger_code(binding.keystroke()) {
                    if let Some(guard) = armed.get_mut(&code) {
                        guard.released = true;
                    }
                }
            }
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
    // Key-ups in this frame that did not clear the guard. A key-down beside
    // one of those is the focus-in pair, not a new press.
    let mut blocked = HashSet::new();
    for event in input.keys.drain(..) {
        if event.synthetic {
            if event.released {
                note_failed_release(&event, &armed, &mut blocked);
            }
            continue;
        }
        if event.released {
            if let Some(code) = source_code(&event) {
                if release_clears(event.order, armed.get(&code).map(|guard| guard.at)) {
                    armed.remove(&code);
                    blocked.remove(&code);
                } else {
                    note_failed_release(&event, &armed, &mut blocked);
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
        if let Some(code) = armed_code(&event, &armed) {
            let guard = armed[&code];
            if suppress_armed_down(
                &event,
                guard,
                fired_now.contains(&code),
                blocked.contains(&code),
            ) {
                continue;
            }
            armed.remove(&code);
            blocked.remove(&code);
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

fn note_failed_release(
    event: &KeyEvent,
    armed: &HashMap<KeyCode, Guard>,
    blocked: &mut HashSet<KeyCode>,
) {
    if let Some(code) = source_code(event) {
        if armed.contains_key(&code) {
            blocked.insert(code);
        }
    }
}

/// `true` when this key-down is still the held hotkey.
///
/// Auto-repeat, the key on the frame the chord fired, and a key-down queued
/// before the guard are not text. A key-down sitting next to a key-up that
/// failed to clear the guard is the focus-in pair. Anything else is a new
/// press. A backend release means a failed key-up in this frame is not that
/// pair: the grab already saw the chord go up.
fn suppress_armed_down(event: &KeyEvent, guard: Guard, fired_now: bool, blocked: bool) -> bool {
    if event.repeat || fired_now {
        return true;
    }
    if event.order != KeyEvent::UNORDERED && event.order <= guard.at {
        return true;
    }
    blocked && !guard.released
}

fn store_armed(world: &mut World, codes: HashMap<KeyCode, Guard>) {
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

fn armed_code(event: &KeyEvent, armed: &HashMap<KeyCode, Guard>) -> Option<KeyCode> {
    if armed.is_empty() {
        return None;
    }
    if let Some(code) = source_code(event) {
        if armed.contains_key(&code) {
            return Some(code);
        }
    }
    if event.code.is_none() {
        if let Some(code) = event.text.as_deref().and_then(text_source) {
            if armed.contains_key(&code) {
                return Some(code);
            }
        }
    }
    None
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
