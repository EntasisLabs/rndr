//! Opt-in, bounded CPU timing. Insert [`FrameTimings`] as a world resource.
//!
//! These clocks measure CPU wall time, not GPU execution or physical display
//! latency. No content or automatic logging is captured. Without the resource,
//! instrumentation performs resource lookups only: no clocks or allocations.
use crate::World;
use std::{collections::VecDeque, sync::Mutex, time::Duration};
use web_time::Instant;

/// CPU stage; stages can nest and must not be summed as independent costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum FrameStage {
    /// Entire normal App::update, including layout and text.
    Update,
    /// User layout systems and hierarchy/list/markdown layout passes.
    Layout,
    /// Text layout and mono-grid preparation after hierarchy layout.
    Text,
    /// Live-only update (no normal update/layout).
    LiveUpdate,
    /// Paint projection; excludes update and render-schedule systems.
    Paint,
    /// Glyph atlas upload and CPU image ingestion.
    Ingest,
    /// Surface acquisition, including failed attempts.
    Acquire,
    /// Quad instance encoding and buffer uploads on the plain-quad path.
    QuadUpload,
    /// GPU command encoding and scene image upload preparation (CPU only).
    Encode,
    /// Queue submit through CPU present return; may include observe readback.
    SubmitPresent,
    /// Native redraw callback through completion, including other CPU stages.
    Redraw,
    /// Oldest platform input receipt to redraw callback entry.
    InputToRedraw,
    /// Oldest platform input receipt to successful CPU present return.
    InputToPresent,
}
const STAGES: usize = FrameStage::InputToPresent as usize + 1;

/// Nearest-rank percentiles over a stage's current rolling window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimingSummary {
    /// Number of retained samples, not total frame count.
    pub count: usize,
    /// Median wall duration.
    pub p50: Duration,
    /// 95th percentile wall duration.
    pub p95: Duration,
    /// 99th percentile wall duration.
    pub p99: Duration,
}

/// One native redraw attempt. Missing input latency means no pending input.
#[derive(Clone, Copy, Debug)]
pub struct NativeFrameTiming {
    /// Whether queue presentation succeeded (not physical display completion).
    pub presented: bool,
    /// True only when the live-only path actually ran.
    pub live_only: bool,
    /// CPU time from redraw callback entry until redraw returns.
    pub redraw: Duration,
    /// Oldest pending platform input callback receipt to redraw callback entry.
    pub input_to_redraw: Option<Duration>,
    /// Oldest pending input receipt to successful CPU present return.
    pub input_to_present: Option<Duration>,
}

struct Samples {
    stages: [VecDeque<Duration>; STAGES],
    frames: VecDeque<NativeFrameTiming>,
    input: Option<Instant>,
}
/// Opt-in world resource. Each stage and native frame history retains at most
/// `capacity` samples. Summary queries allocate a temporary sorted copy.
///
/// Input starts at platform callback receipt, not device/OS event creation.
/// Coalesced input uses the oldest receipt; failed frames retain that receipt.
/// Removing the resource discards all timing history and pending input.
pub struct FrameTimings {
    capacity: usize,
    samples: Mutex<Samples>,
}
impl FrameTimings {
    /// Create a rolling window. Zero is clamped to one sample.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            samples: Mutex::new(Samples {
                stages: std::array::from_fn(|_| VecDeque::new()),
                frames: VecDeque::new(),
                input: None,
            }),
        }
    }
    /// Maximum retained samples per stage and native history.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    /// Record an externally measured CPU duration.
    pub fn record(&self, stage: FrameStage, duration: Duration) {
        push(
            &mut self.samples.lock().unwrap().stages[stage as usize],
            self.capacity,
            duration,
        );
    }
    /// Percentiles for a stage, or None before it has any samples.
    pub fn summary(&self, stage: FrameStage) -> Option<TimingSummary> {
        let mut values: Vec<_> = self.samples.lock().unwrap().stages[stage as usize]
            .iter()
            .copied()
            .collect();
        if values.is_empty() {
            return None;
        }
        values.sort_unstable();
        let rank = |p: usize| values[(values.len() * p).div_ceil(100) - 1];
        Some(TimingSummary {
            count: values.len(),
            p50: rank(50),
            p95: rank(95),
            p99: rank(99),
        })
    }
    /// Snapshot of native redraw attempts, oldest first.
    pub fn native_frames(&self) -> Vec<NativeFrameTiming> {
        self.samples
            .lock()
            .unwrap()
            .frames
            .iter()
            .copied()
            .collect()
    }
}
impl Default for FrameTimings {
    fn default() -> Self {
        Self::new(240)
    }
}
fn push<T>(queue: &mut VecDeque<T>, capacity: usize, value: T) {
    if queue.len() == capacity {
        queue.pop_front();
    }
    queue.push_back(value);
}
/// Start a stage only when timing is enabled.
pub fn start(world: &World) -> Option<Instant> {
    world.resource::<FrameTimings>().map(|_| Instant::now())
}
/// Complete a stage; disabled starts never read a clock.
pub fn finish(world: &World, stage: FrameStage, start: Option<Instant>) {
    if let (Some(start), Some(timings)) = (start, world.resource::<FrameTimings>()) {
        timings.record(stage, start.elapsed());
    }
}
/// Note a platform input receipt. Coalescing preserves the oldest receipt.
pub fn input_received(world: &World) {
    if let Some(timings) = world.resource::<FrameTimings>() {
        timings
            .samples
            .lock()
            .unwrap()
            .input
            .get_or_insert_with(Instant::now);
    }
}
/// Complete a redraw attempt. `presented_at` is the CPU present-return clock,
/// not the later redraw return; a failed attempt must pass None.
pub fn finish_native(
    world: &World,
    start: Option<Instant>,
    live_only: bool,
    presented_at: Option<Instant>,
) {
    if let (Some(start), Some(timings)) = (start, world.resource::<FrameTimings>()) {
        let mut samples = timings.samples.lock().unwrap();
        let input = samples.input;
        let frame = NativeFrameTiming {
            presented: presented_at.is_some(),
            live_only,
            redraw: start.elapsed(),
            input_to_redraw: input.map(|t| start.saturating_duration_since(t)),
            input_to_present: presented_at
                .zip(input)
                .map(|(end, t)| end.saturating_duration_since(t)),
        };
        push(
            &mut samples.stages[FrameStage::Redraw as usize],
            timings.capacity,
            frame.redraw,
        );
        if let Some(duration) = frame.input_to_redraw {
            push(
                &mut samples.stages[FrameStage::InputToRedraw as usize],
                timings.capacity,
                duration,
            );
        }
        if let Some(duration) = frame.input_to_present {
            push(
                &mut samples.stages[FrameStage::InputToPresent as usize],
                timings.capacity,
                duration,
            );
        }
        push(&mut samples.frames, timings.capacity, frame);
        if presented_at.is_some() {
            samples.input = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_nearest_rank() {
        let timings = FrameTimings::new(100);
        for n in 0..=100 {
            timings.record(FrameStage::Paint, Duration::from_millis(n));
        }
        let s = timings.summary(FrameStage::Paint).unwrap();
        assert_eq!(
            (
                s.count,
                s.p50.as_millis(),
                s.p95.as_millis(),
                s.p99.as_millis()
            ),
            (100, 50, 95, 99)
        );
        assert!(timings.summary(FrameStage::Text).is_none());
        assert_eq!(FrameTimings::new(0).capacity(), 1);
    }
    #[test]
    fn app_stages_and_live_separation() {
        let mut app = crate::App::new();
        app.update();
        assert!(app.world().resource::<FrameTimings>().is_none());
        app.world_mut().insert_resource(FrameTimings::default());
        app.update_live();
        let timings = app.world().resource::<FrameTimings>().unwrap();
        assert_eq!(timings.summary(FrameStage::LiveUpdate).unwrap().count, 1);
        assert!(timings.summary(FrameStage::Update).is_none());
        assert!(timings.summary(FrameStage::Layout).is_none());
        assert!(timings.summary(FrameStage::Text).is_none());
        app.update();
        let timings = app.world().resource::<FrameTimings>().unwrap();
        for stage in [FrameStage::Update, FrameStage::Layout, FrameStage::Text] {
            assert_eq!(timings.summary(stage).unwrap().count, 1);
        }
    }

    #[test]
    fn disabled_and_native_lifecycle() {
        let mut world = World::new();
        assert!(start(&world).is_none());
        input_received(&world);
        finish(&world, FrameStage::Paint, None);
        finish_native(&world, None, false, None);
        world.insert_resource(FrameTimings::new(2));
        input_received(&world);
        let input = world
            .resource::<FrameTimings>()
            .unwrap()
            .samples
            .lock()
            .unwrap()
            .input;
        input_received(&world);
        assert_eq!(
            world
                .resource::<FrameTimings>()
                .unwrap()
                .samples
                .lock()
                .unwrap()
                .input,
            input
        );
        finish_native(&world, start(&world), false, None);
        let failed = world.resource::<FrameTimings>().unwrap().native_frames()[0];
        assert!(!failed.presented);
        assert!(failed.input_to_redraw.is_some());
        assert!(failed.input_to_present.is_none());
        finish_native(&world, start(&world), true, start(&world));
        finish_native(&world, start(&world), false, start(&world));
        let frames = world.resource::<FrameTimings>().unwrap().native_frames();
        assert_eq!(frames.len(), 2);
        assert!(frames[0].presented && frames[0].live_only);
        assert!(frames[0].input_to_present.is_some());
        assert!(frames[1].input_to_redraw.is_none());
        assert!(frames[1].input_to_present.is_none());
    }
}
