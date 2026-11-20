//! Caret blink for collapsed text carets.
//!
//! One phase is shared by every painted [`Caret`]: only one text target has
//! focus at a time. Moving the caret or changing the focused text shows it
//! again and restarts the interval, so it never vanishes mid-typing. A
//! selection does not blink. With nothing focused, or when
//! [`SystemAppearance::reduced_motion`](crate::SystemAppearance) is set, the
//! caret stays on and no timer is armed.
use std::time::Duration;

use crate::caret::Caret;
use crate::editor::Buffer;
use crate::entity::Entity;
use crate::event::{events, Event};
use crate::input::Focus;
use crate::runtime::{after, TimerHandle};
use crate::style::Label;
use crate::world::World;

/// Half period of the blink: the caret is on for this long, then off.
pub const CARET_BLINK: Duration = Duration::from_millis(530);

#[derive(Clone)]
struct Tick;
impl Event for Tick {}

type Key = (Entity, usize, usize, usize);

#[derive(Default)]
struct Blink {
    hidden: bool,
    key: Option<Key>,
    timer: Option<TimerHandle>,
}

/// Whether a collapsed caret paints this frame.
pub fn caret_visible(world: &World) -> bool {
    world.local::<Blink>().is_none_or(|b| !b.hidden)
}

pub(crate) fn apply_caret_blink(world: &mut World) {
    if !world.has_local::<Blink>() {
        world.insert_local(Blink::default());
    }
    let ticked = !events::<Tick>(world).is_empty();
    let key = world
        .resource::<Focus>()
        .and_then(|f| f.entity)
        .and_then(|e| {
            let caret = world.get::<Caret>(e)?;
            // Editors paint a window-relative caret; the buffer holds the
            // document selection, which an offscreen range can clamp away.
            let (index, anchor) = world
                .get::<Buffer>(e)
                .map_or((caret.index, caret.anchor), |b| (b.caret, b.anchor));
            let len = world.get::<Label>(e).map_or(0, |l| l.text.len());
            Some((e, index, anchor, len))
        });
    let blinks = key.is_some_and(|(_, index, anchor, _)| index == anchor)
        && !crate::system_appearance(world).reduced_motion;
    let state = world.local_mut::<Blink>().unwrap();
    if !blinks {
        *state = Blink {
            key,
            ..Blink::default()
        };
        return;
    }
    let rearm = if state.key != key {
        state.key = key;
        state.hidden = false;
        true
    } else if ticked {
        state.hidden = !state.hidden;
        true
    } else {
        state.timer.is_none()
    };
    if rearm {
        let timer = after(world, CARET_BLINK, Tick);
        world.local_mut::<Blink>().unwrap().timer = Some(timer);
    }
}
