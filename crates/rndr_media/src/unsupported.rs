use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::{Command, Shared};

pub(crate) fn spawn(shared: Arc<Shared>, _commands: Receiver<Command>) -> Option<JoinHandle<()>> {
    shared.fail("media playback is not supported on this platform");
    None
}
