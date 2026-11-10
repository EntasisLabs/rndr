//! Recipe step timers.
//!
//! A timer is stored by view, source byte and step, so recycling the recipe
//! block does not reset it. Deadlines are absolute instants on [`rndr_core::Time`].
//! Pausing freezes the remaining time. Reaching the deadline publishes
//! [`LiquidInteraction::TimerComplete`] once.
use crate::render;
use crate::state::{self, LiquidDisposition, LiquidInteraction, LiquidInteractionEnvelope};
use crate::view::{restore_tab, BlockRoot};
use rndr_core::{Entity, Parent, Time, World};
use std::collections::HashMap;

/// What a timer is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidTimerStatus {
    /// Not started.
    Idle,
    /// Counting toward [`LiquidTimerRecord::deadline_ms`].
    Running,
    /// Remaining time is frozen.
    Paused,
    /// The deadline passed, or the reader finished it.
    Complete,
}

/// A change the reader, or the deadline, applies to a timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidTimerAction {
    /// Start from idle, or resume from paused.
    Start,
    /// Freeze a running timer.
    Pause,
    /// Return to idle at the full duration.
    Reset,
    /// Mark the timer finished.
    Complete,
}

/// Persisted timer for one recipe step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidTimerRecord {
    /// Schema of this record. This crate writes `1`.
    pub schema_version: u32,
    /// Source byte of the recipe block.
    pub source: usize,
    /// Step index inside that recipe.
    pub step: usize,
    /// Monotonic revision of this timer.
    pub revision: u64,
    /// Current phase.
    pub status: LiquidTimerStatus,
    /// Step duration in milliseconds, clamped to one second through one day.
    pub duration_ms: u64,
    /// Milliseconds left while paused or idle. While running, this is the time until [`Self::deadline_ms`].
    pub remaining_ms: u64,
    /// Absolute [`rndr_core::Time`] deadline, in milliseconds, while running.
    pub deadline_ms: Option<u64>,
}

#[derive(Default)]
struct TimerStore {
    records: HashMap<(Entity, usize, usize), LiquidTimerRecord>,
}

const MIN_MS: u64 = 1_000;
const MAX_MS: u64 = 86_400_000;

pub(crate) fn clamp_duration(ms: u64) -> u64 {
    ms.clamp(MIN_MS, MAX_MS)
}

/// Timers stored for `view`, in source then step order.
pub fn liquid_timers(world: &World, view: Entity) -> Vec<LiquidTimerRecord> {
    let mut records: Vec<_> = world
        .resource::<TimerStore>()
        .map(|store| {
            store
                .records
                .iter()
                .filter(|((owner, _, _), _)| *owner == view)
                .map(|(_, record)| record.clone())
                .collect()
        })
        .unwrap_or_default();
    records.sort_by_key(|record| (record.source, record.step));
    records
}

/// The timer for one step, if it has been started, paused, reset or completed.
pub fn liquid_timer(
    world: &World,
    view: Entity,
    source: usize,
    step: usize,
) -> Option<LiquidTimerRecord> {
    world
        .resource::<TimerStore>()
        .and_then(|store| store.records.get(&(view, source, step)).cloned())
        .map(|record| reconcile_record(record, now_ms(world)))
}

pub(crate) fn fresh(source: usize, step: usize, duration_ms: u64) -> LiquidTimerRecord {
    let duration_ms = clamp_duration(duration_ms);
    LiquidTimerRecord {
        schema_version: 1,
        source,
        step,
        revision: 0,
        status: LiquidTimerStatus::Idle,
        duration_ms,
        remaining_ms: duration_ms,
        deadline_ms: None,
    }
}

pub(crate) fn apply(
    world: &mut World,
    view: Entity,
    source: usize,
    step: usize,
    duration_ms: u64,
    action: LiquidTimerAction,
) -> LiquidTimerRecord {
    let now = now_ms(world);
    let prior =
        liquid_timer(world, view, source, step).unwrap_or_else(|| fresh(source, step, duration_ms));
    let mut next = transition(&prior, action, now);
    next.revision = prior.revision + 1;
    write(world, view, next.clone());
    publish(world, view, &next, action);
    rebuild_recipe(world, view, source);
    next
}

/// Finish timers whose deadline has passed and publish one completion each.
pub(crate) fn reconcile(world: &mut World) {
    let now = now_ms(world);
    let due: Vec<(Entity, LiquidTimerRecord)> = world
        .resource::<TimerStore>()
        .map(|store| {
            store
                .records
                .iter()
                .filter_map(|((view, _, _), record)| {
                    let next = reconcile_record(record.clone(), now);
                    (next.status == LiquidTimerStatus::Complete
                        && record.status == LiquidTimerStatus::Running)
                        .then_some((*view, next))
                })
                .collect()
        })
        .unwrap_or_default();
    for (view, mut record) in due {
        record.revision += 1;
        let source = record.source;
        write(world, view, record.clone());
        publish(world, view, &record, LiquidTimerAction::Complete);
        rebuild_recipe(world, view, source);
    }
}

fn transition(prior: &LiquidTimerRecord, action: LiquidTimerAction, now: u64) -> LiquidTimerRecord {
    let prior = reconcile_record(prior.clone(), now);
    match action {
        LiquidTimerAction::Reset => fresh(prior.source, prior.step, prior.duration_ms),
        LiquidTimerAction::Complete => LiquidTimerRecord {
            status: LiquidTimerStatus::Complete,
            remaining_ms: 0,
            deadline_ms: None,
            ..prior
        },
        LiquidTimerAction::Pause if prior.status == LiquidTimerStatus::Running => {
            LiquidTimerRecord {
                status: LiquidTimerStatus::Paused,
                remaining_ms: remaining_ms(&prior, now),
                deadline_ms: None,
                ..prior
            }
        }
        LiquidTimerAction::Start
            if matches!(
                prior.status,
                LiquidTimerStatus::Idle | LiquidTimerStatus::Paused
            ) =>
        {
            let remaining_ms = if prior.status == LiquidTimerStatus::Idle {
                prior.duration_ms
            } else {
                prior.remaining_ms
            };
            LiquidTimerRecord {
                status: LiquidTimerStatus::Running,
                remaining_ms,
                deadline_ms: Some(now.saturating_add(remaining_ms)),
                ..prior
            }
        }
        _ => prior,
    }
}

fn reconcile_record(mut record: LiquidTimerRecord, now: u64) -> LiquidTimerRecord {
    if record.status != LiquidTimerStatus::Running {
        return record;
    }
    let remaining = remaining_ms(&record, now);
    if remaining > 0 {
        record.remaining_ms = remaining;
        return record;
    }
    record.status = LiquidTimerStatus::Complete;
    record.remaining_ms = 0;
    record.deadline_ms = None;
    record
}

pub(crate) fn format_remaining(ms: u64) -> String {
    let seconds = ms.div_ceil(1_000);
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let rest = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{rest:02}")
    } else {
        format!("{minutes}:{rest:02}")
    }
}

pub(crate) fn remaining_ms(record: &LiquidTimerRecord, now: u64) -> u64 {
    match record.status {
        LiquidTimerStatus::Running => record
            .deadline_ms
            .map(|deadline| deadline.saturating_sub(now))
            .unwrap_or(record.remaining_ms),
        LiquidTimerStatus::Complete => 0,
        _ => record.remaining_ms,
    }
}

fn publish(world: &mut World, view: Entity, record: &LiquidTimerRecord, action: LiquidTimerAction) {
    let event = match action {
        LiquidTimerAction::Start => LiquidInteraction::TimerStart,
        LiquidTimerAction::Pause => LiquidInteraction::TimerPause,
        LiquidTimerAction::Reset => LiquidInteraction::TimerReset,
        LiquidTimerAction::Complete => LiquidInteraction::TimerComplete,
    };
    state::emit(
        world,
        LiquidInteractionEnvelope {
            version: 1,
            view,
            source: record.source,
            node_id: "timer".into(),
            instance_id: record.step.to_string(),
            event,
            disposition: LiquidDisposition::LocalState,
            revision: record.revision,
        },
    );
}

fn write(world: &mut World, view: Entity, record: LiquidTimerRecord) {
    if world.resource_mut::<TimerStore>().is_none() {
        world.insert_resource(TimerStore::default());
    }
    world
        .resource_mut::<TimerStore>()
        .unwrap()
        .records
        .insert((view, record.source, record.step), record);
}

fn rebuild_recipe(world: &mut World, view: Entity, source: usize) {
    let roots: Vec<Entity> = world
        .entities_with::<BlockRoot>()
        .into_iter()
        .filter(|root| {
            world
                .get::<Parent>(*root)
                .is_some_and(|parent| parent.entity == view)
                && world
                    .get::<BlockRoot>(*root)
                    .is_some_and(|info| info.source == source)
        })
        .collect();
    for root in roots {
        let selected = world
            .get::<crate::render::BlockState>(root)
            .map(|state| state.selected)
            .unwrap_or(0);
        render::rebuild(world, root);
        restore_tab(world, root, selected);
    }
}

fn now_ms(world: &World) -> u64 {
    world
        .resource::<Time>()
        .map(|time| u64::try_from(time.now.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> LiquidTimerRecord {
        fresh(0, 1, 10_000)
    }

    #[test]
    fn start_pause_resume_and_deadline() {
        let running = transition(&idle(), LiquidTimerAction::Start, 1_000);
        assert_eq!(running.status, LiquidTimerStatus::Running);
        assert_eq!(running.deadline_ms, Some(11_000));
        assert_eq!(remaining_ms(&running, 4_000), 7_000);
        let paused = transition(&running, LiquidTimerAction::Pause, 4_000);
        assert_eq!(paused.status, LiquidTimerStatus::Paused);
        assert_eq!(paused.remaining_ms, 7_000);
        assert!(paused.deadline_ms.is_none());
        let resumed = transition(&paused, LiquidTimerAction::Start, 5_000);
        assert_eq!(resumed.deadline_ms, Some(12_000));
        let done = reconcile_record(resumed, 12_000);
        assert_eq!(done.status, LiquidTimerStatus::Complete);
        assert_eq!(done.remaining_ms, 0);
        let reset = transition(&done, LiquidTimerAction::Reset, 13_000);
        assert_eq!(reset.status, LiquidTimerStatus::Idle);
        assert_eq!(reset.remaining_ms, 10_000);
    }
}
