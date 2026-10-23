//! Feed delivery: one wake per burst, FIFO order, cancel, and stale epochs.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rndr_core::{
    events, feed, spawn_detached, Event, Feed, FeedConfig, FeedError, OnFull, Sim, Wake,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Item(u32);

impl Event for Item {}

struct CountWake {
    n: AtomicUsize,
}

impl Wake for CountWake {
    fn wake(&self) {
        self.n.fetch_add(1, Ordering::SeqCst);
    }
}

struct Stop<T> {
    feed: Option<Feed<T>>,
    worker: Option<JoinHandle<()>>,
}

impl<T> Drop for Stop<T> {
    fn drop(&mut self) {
        self.feed.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn woken() -> (Sim, Arc<CountWake>) {
    let mut sim = Sim::new();
    let wake = Arc::new(CountWake {
        n: AtomicUsize::new(0),
    });
    sim.app.set_wake(wake.clone());
    (sim, wake)
}

fn wakes(wake: &CountWake) -> usize {
    wake.n.load(Ordering::SeqCst)
}

#[test]
fn a_burst_wakes_once_and_keeps_order() {
    let (mut sim, wake) = woken();
    let (handle, tx) = feed::open(sim.world(), FeedConfig::queue(16));
    for i in 0..10 {
        tx.send(Item(i)).unwrap();
    }
    assert_eq!(wakes(&wake), 1);
    assert!(sim.app.wants_frame());
    sim.update();
    assert_eq!(
        events::<Item>(sim.world()),
        (0..10).map(Item).collect::<Vec<_>>().as_slice()
    );
    assert_eq!(wakes(&wake), 1);
    assert!(!sim.app.wants_frame());
    tx.send(Item(10)).unwrap();
    assert_eq!(wakes(&wake), 2);
    drop(handle);
}

#[test]
fn a_worker_burst_stays_in_order() {
    let (mut sim, wake) = woken();
    let (handle, tx) = feed::open(sim.world(), FeedConfig::queue(64));
    let worker = thread::spawn(move || {
        for i in 0..32 {
            tx.send(Item(i)).unwrap();
        }
    });
    worker.join().unwrap();
    assert_eq!(wakes(&wake), 1);
    sim.update();
    assert_eq!(
        events::<Item>(sim.world()),
        (0..32).map(Item).collect::<Vec<_>>().as_slice()
    );
    drop(handle);
}

#[test]
fn latest_keeps_the_newest_value() {
    let (mut sim, wake) = woken();
    let (handle, tx) = feed::open(sim.world(), FeedConfig::latest());
    tx.send(Item(1)).unwrap();
    tx.send(Item(2)).unwrap();
    tx.send(Item(3)).unwrap();
    assert_eq!(wakes(&wake), 1);
    sim.update();
    assert_eq!(events::<Item>(sim.world()), &[Item(3)]);
    drop(handle);
}

#[test]
fn drop_oldest_discards_the_front() {
    let (mut sim, wake) = woken();
    let (handle, tx) = feed::open(sim.world(), FeedConfig::bounded(2, OnFull::DropOldest));
    tx.send(Item(1)).unwrap();
    tx.send(Item(2)).unwrap();
    tx.send(Item(3)).unwrap();
    assert_eq!(wakes(&wake), 1);
    sim.update();
    assert_eq!(events::<Item>(sim.world()), &[Item(2), Item(3)]);
    drop(handle);
}

#[test]
fn reject_does_not_grow_or_wake() {
    let (mut sim, wake) = woken();
    let (handle, tx) = feed::open(sim.world(), FeedConfig::bounded(1, OnFull::Reject));
    tx.send(Item(1)).unwrap();
    assert_eq!(wakes(&wake), 1);
    assert_eq!(tx.try_send(Item(2)), Err(FeedError::Full));
    assert_eq!(tx.send(Item(3)), Err(FeedError::Full));
    assert_eq!(wakes(&wake), 1);
    sim.update();
    assert_eq!(events::<Item>(sim.world()), &[Item(1)]);
    drop(handle);
}

#[test]
fn wait_blocks_until_the_frame_drains() {
    let (mut sim, _wake) = woken();
    let (feed, tx) = feed::open(sim.world(), FeedConfig::queue(1));
    tx.send(Item(1)).unwrap();
    let tx2 = tx.clone();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = tx2.send(Item(2));
        let _ = done_tx.send(result);
    });
    let mut stop = Stop {
        feed: Some(feed),
        worker: Some(worker),
    };
    assert!(
        done_rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "a full Wait queue blocks the sender"
    );
    sim.update();
    assert_eq!(events::<Item>(sim.world()), &[Item(1)]);
    assert_eq!(
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        Ok(())
    );
    sim.update();
    assert_eq!(events::<Item>(sim.world()), &[Item(2)]);
    stop.feed.take();
}

#[test]
fn drop_cancels_and_unblocks_a_sender() {
    let (mut sim, wake) = woken();
    let (feed, tx) = feed::open(sim.world(), FeedConfig::queue(1));
    tx.send(Item(1)).unwrap();
    let before = wakes(&wake);
    let tx2 = tx.clone();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = thread::spawn(move || {
        let result = tx2.send(Item(2));
        let _ = done_tx.send(result);
    });
    let mut stop = Stop {
        feed: Some(feed),
        worker: Some(worker),
    };
    assert!(done_rx.recv_timeout(Duration::from_millis(200)).is_err());
    stop.feed.take();
    assert!(tx.is_cancelled());
    assert_eq!(tx.try_send(Item(3)), Err(FeedError::Closed));
    assert_eq!(
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
        Err(FeedError::Closed)
    );
    assert_eq!(wakes(&wake), before, "closing does not request a redraw");
    sim.update();
    assert!(events::<Item>(sim.world()).is_empty());
}

#[test]
fn stale_epoch_is_dropped_and_does_not_wake() {
    let (mut sim, wake) = woken();
    let (handle, tx) = feed::open(sim.world(), FeedConfig::queue(8));
    let stale = tx.epoch();
    tx.send_at(stale, Item(1)).unwrap();
    assert_eq!(wakes(&wake), 1);
    assert!(sim.app.wants_frame());
    let epoch = handle.advance();
    assert!(!sim.app.wants_frame());
    assert_eq!(wakes(&wake), 1);
    assert_eq!(tx.send_at(stale, Item(9)), Err(FeedError::Stale));
    assert_eq!(tx.try_send_at(stale, Item(8)), Err(FeedError::Stale));
    assert_eq!(wakes(&wake), 1);
    tx.send_at(epoch, Item(2)).unwrap();
    assert_eq!(wakes(&wake), 2);
    sim.update();
    assert_eq!(events::<Item>(sim.world()), &[Item(2)]);
    drop(handle);
}

#[test]
fn cancelled_future_completes_after_drop() {
    let mut sim = Sim::new();
    let (handle, tx) = feed::open::<Item>(sim.world(), FeedConfig::queue(4));
    let flag = Arc::new(AtomicBool::new(false));
    let flag2 = Arc::clone(&flag);
    let task = spawn_detached(sim.world(), async move {
        tx.cancelled().await;
        flag2.store(true, Ordering::SeqCst);
    });
    sim.update();
    assert!(!flag.load(Ordering::SeqCst));
    drop(handle);
    sim.update();
    assert!(flag.load(Ordering::SeqCst));
    drop(task);
}

#[test]
fn spawn_thread_stops_when_the_feed_is_dropped() {
    let (mut sim, _wake) = woken();
    let (done_tx, done_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let handle = feed::spawn_thread(sim.world(), FeedConfig::queue(4), move |tx| {
        tx.send(Item(4)).unwrap();
        let _ = ready_tx.send(());
        tx.wait_until_cancelled();
        let _ = done_tx.send(());
    });
    ready_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    sim.update();
    assert_eq!(events::<Item>(sim.world()), &[Item(4)]);
    drop(handle);
    done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
}

#[test]
fn spawn_task_delivers_on_the_ui_thread() {
    let mut sim = Sim::new();
    let handle = feed::spawn_task(sim.world(), FeedConfig::queue(4), |tx| async move {
        tx.send(Item(1)).unwrap();
        tx.send(Item(2)).unwrap();
    });
    sim.update();
    assert_eq!(events::<Item>(sim.world()), &[Item(1), Item(2)]);
    drop(handle);
}

#[cfg(feature = "tokio")]
#[test]
fn tokio_spawn_delivers_on_the_app_pump() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut sim = Sim::new();
    let handle = {
        let _enter = runtime.enter();
        feed::spawn_tokio(sim.world(), FeedConfig::queue(4), |tx| async move {
            tx.send(Item(7)).unwrap();
        })
        .unwrap()
    };
    runtime.block_on(async {
        tokio::task::yield_now().await;
    });
    sim.update();
    assert_eq!(events::<Item>(sim.world()), &[Item(7)]);
    drop(handle);
}

#[cfg(feature = "tokio")]
#[test]
fn tokio_spawn_without_a_runtime_is_an_error() {
    let mut sim = Sim::new();
    let error = feed::spawn_tokio(
        sim.world(),
        FeedConfig::queue(1),
        |tx: feed::Sender<Item>| async move {
            let _ = tx;
        },
    );
    assert_eq!(error.err(), Some(FeedError::NoRuntime));
}
