use crate::{Error, State};

pub(crate) fn visibility(current: State, next: State) -> Result<State, Error> {
    match (current, next) {
        (State::Visible | State::Hidden, State::Visible | State::Hidden) => Ok(next),
        (State::Disposed, _) => Err(Error::Stale),
        _ => Err(Error::Suspended),
    }
}

pub(crate) fn suspend(current: State, generation: u64) -> Result<u64, Error> {
    match current {
        State::Visible | State::Hidden => generation.checked_add(1).ok_or(Error::Exhausted),
        State::Suspended => Err(Error::Suspended),
        State::Disposed => Err(Error::Stale),
    }
}
