//! Small thread pool behind the `pool` feature. Core does not depend on tokio.
//!
//! Workers poll [`Job`]s. Completions land in the runtime inbox; the desktop
//! runner wakes winit from that path.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use crate::runtime::{lock, waker_for, Executor, Job};

struct PoolSlot {
    job: Option<Job>,
    schedule: Arc<dyn Fn() + Send + Sync>,
}

/// Background executor used by the desktop runner.
pub struct ThreadPool {
    tx: Mutex<Option<Sender<u64>>>,
    slots: Mutex<HashMap<u64, PoolSlot>>,
    next: AtomicU64,
    shutdown: AtomicBool,
}

/// Start a pool sized to the machine, capped at four workers.
pub fn thread_pool() -> Arc<dyn Executor> {
    ThreadPool::start()
}

impl ThreadPool {
    fn start() -> Arc<Self> {
        let pool = Arc::new(Self {
            tx: Mutex::new(None),
            slots: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            shutdown: AtomicBool::new(false),
        });
        let (tx, rx) = mpsc::channel();
        *lock(&pool.tx) = Some(tx);
        let rx = Arc::new(Mutex::new(rx));
        for _ in 0..worker_count() {
            let rx = Arc::clone(&rx);
            let pool = Arc::clone(&pool);
            std::thread::Builder::new()
                .name("rndr-pool".into())
                .spawn(move || worker(pool, rx))
                .expect("pool thread");
        }
        pool
    }

    fn poll_id(&self, id: u64) {
        let (job, schedule) = {
            let mut slots = lock(&self.slots);
            let Some(slot) = slots.get_mut(&id) else {
                return;
            };
            let schedule = Arc::clone(&slot.schedule);
            (slot.job.take(), schedule)
        };
        let Some(mut job) = job else {
            return;
        };
        if job.cancelled() || self.shutdown.load(Ordering::Acquire) {
            lock(&self.slots).remove(&id);
            return;
        }
        let waker = waker_for(schedule);
        let mut cx = Context::from_waker(&waker);
        match job.poll(&mut cx) {
            Poll::Ready(()) => {
                lock(&self.slots).remove(&id);
            }
            Poll::Pending => {
                let mut slots = lock(&self.slots);
                if self.shutdown.load(Ordering::Acquire) || job.cancelled() {
                    slots.remove(&id);
                    return;
                }
                if let Some(slot) = slots.get_mut(&id) {
                    slot.job = Some(job);
                }
            }
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let slots = std::mem::take(&mut *lock(&self.slots));
        drop(slots);
        *lock(&self.tx) = None;
    }
}

impl Executor for ThreadPool {
    fn spawn(&self, job: Job) {
        if self.shutdown.load(Ordering::Acquire) {
            return;
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let tx = lock(&self.tx).as_ref().cloned();
        let Some(tx) = tx else {
            return;
        };
        let schedule: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let _ = tx.send(id);
        });
        job.set_nudge(Arc::clone(&schedule));
        lock(&self.slots).insert(
            id,
            PoolSlot {
                job: Some(job),
                schedule: Arc::clone(&schedule),
            },
        );
        schedule();
    }
}

fn worker(pool: Arc<ThreadPool>, rx: Arc<Mutex<Receiver<u64>>>) {
    loop {
        if pool.shutdown.load(Ordering::Acquire) {
            break;
        }
        let message = {
            let rx = lock(&rx);
            rx.recv()
        };
        let Ok(id) = message else {
            break;
        };
        pool.poll_id(id);
    }
}

fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .clamp(2, 4)
}
