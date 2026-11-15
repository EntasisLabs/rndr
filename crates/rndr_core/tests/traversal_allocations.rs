//! Allocation counter for the immutable hit path. Run alone and serially.
use rndr_core::*;
use std::alloc::{GlobalAlloc, Layout as AllocationLayout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Counting;
static COUNT: AtomicUsize = AtomicUsize::new(0);
static ENABLED: AtomicBool = AtomicBool::new(false);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: AllocationLayout) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: AllocationLayout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: AllocationLayout, size: usize) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn alloc_zeroed(&self, layout: AllocationLayout) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc_zeroed(layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
#[ignore = "opt-in allocation characterization; run serially"]
fn retained_nested_hit_allocations() {
    let mut app = App::new();
    let root = app.world_mut().spawn(Layout::column());
    for _ in 0..1000 {
        let row = app.world_mut().spawn(Layout {
            height: Size::Px(24.0),
            ..Layout::column()
        });
        let child = app.world_mut().spawn(Layout {
            height: Size::Px(24.0),
            ..Layout::column()
        });
        set_parent(app.world_mut(), row, root);
        set_parent(app.world_mut(), child, row);
    }
    app.update();
    let expected = hit_test(app.world(), 1.0, 1.0);
    COUNT.store(0, Ordering::Relaxed);
    ENABLED.store(true, Ordering::Relaxed);
    let actual = hit_test(app.world(), 1.0, 1.0);
    ENABLED.store(false, Ordering::Relaxed);
    let count = COUNT.load(Ordering::Relaxed);
    assert_eq!(actual, expected);
    println!("nested 1000-row hit allocations={count}");
}
