//! First-press filter for a grabbed key.
//!
//! Detectable auto-repeat delivers further presses while the key is held and
//! no release between them. The older sequence is a release and a press that
//! share a timestamp. Both are repeats. The next press after a real release
//! is a new edge.

use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct RepeatFilter {
    held: HashSet<u32>,
    release_at: HashMap<u32, u64>,
}

impl RepeatFilter {
    /// `true` when this press is the first edge and should be delivered.
    pub(super) fn press(&mut self, id: u32, time: u64) -> bool {
        let paired = self.release_at.get(&id).copied() == Some(time);
        let already = self.held.contains(&id);
        self.release_at.remove(&id);
        self.held.insert(id);
        !(already || paired)
    }

    pub(super) fn release(&mut self, id: u32, time: u64) {
        self.held.remove(&id);
        self.release_at.insert(id, time);
    }
}

#[cfg(test)]
mod tests {
    use super::RepeatFilter;

    #[test]
    fn only_the_first_press_while_held_is_delivered() {
        let mut filter = RepeatFilter::default();
        assert!(filter.press(65, 10));
        assert!(!filter.press(65, 670));
        assert!(!filter.press(65, 700));
        filter.release(65, 900);
        assert!(filter.press(65, 1200));
    }

    #[test]
    fn a_release_and_press_with_the_same_timestamp_is_a_repeat() {
        let mut filter = RepeatFilter::default();
        assert!(filter.press(32, 10));
        filter.release(32, 670);
        assert!(!filter.press(32, 670));
        assert!(!filter.press(32, 800));
        filter.release(32, 900);
        assert!(filter.press(32, 1400));
    }

    #[test]
    fn a_different_key_still_fires_while_another_is_held() {
        let mut filter = RepeatFilter::default();
        assert!(filter.press(32, 10));
        assert!(filter.press(65, 10));
        assert!(!filter.press(32, 700));
        filter.release(32, 800);
        assert!(filter.press(32, 900));
        assert!(!filter.press(65, 900));
    }
}
