//! Registration refcount, backend calls, and press delivery.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use crate::event::{self, Event};
use crate::keys::platform_of;
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

pub(crate) fn drive(world: &mut World) {
    let hub = {
        let Some(installed) = world.resource::<HotkeyHub>() else {
            return;
        };
        Arc::clone(&installed.hub)
    };
    let presses = lock(&hub.backend).poll();
    if presses.is_empty() {
        return;
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
    for (id, binding, toggle) in planned {
        event::send(world, GlobalHotkey { id, binding });
        if toggle {
            window::toggle(world);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}
