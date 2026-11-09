use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::JoinHandle;

use super::{Command, MicPermission, Shared};

pub(crate) fn permission() -> MicPermission {
    MicPermission::Restricted
}

pub(crate) fn spawn(shared: Arc<Shared>, _commands: Receiver<Command>) -> Option<JoinHandle<()>> {
    shared.fail("recording is not supported on this platform");
    None
}
