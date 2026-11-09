//! The OS reopening the app or handing it a link.
//!
//! The desktop runner sends [`UrlOpened`] for each `myapp://…` link the OS
//! opens with the app: the link that launched it, a later one while it runs,
//! and one a second launch forwarded through single-instance support. It
//! sends [`Reopened`] when the user opens the app again while it runs (a Dock
//! click on macOS, or a second launch without links). Both reach the primary
//! window's world, in arrival order, and are readable in `Update`.
//!
//! A link comes from outside the app and is passed on as the OS gave it, so
//! the app must parse and check it before acting on it.
use crate::event::{send, Event};
use crate::world::World;

/// The OS opened `myapp://…` with this app. Carries the link unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UrlOpened(pub String);

impl Event for UrlOpened {}

/// The user opened the app again while it was running. The runner has
/// already shown and focused the primary window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reopened;

impl Event for Reopened {}

/// Send [`UrlOpened`] for `url`.
pub fn push_opened_url(world: &mut World, url: impl Into<String>) {
    send(world, UrlOpened(url.into()));
}

/// Send [`Reopened`].
pub fn push_reopened(world: &mut World) {
    send(world, Reopened);
}

/// Whether a launch argument is a link: `scheme://…` with a scheme of at
/// least two characters, starting with a letter and made of letters, digits,
/// `+`, `-` or `.`. A Windows path (`C:\…`), a file path and a flag are not.
pub fn is_link(arg: &str) -> bool {
    let Some((scheme, _)) = arg.split_once("://") else {
        return false;
    };
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// The links among `args`, in order. Windows and Linux desktop entries pass
/// the link a launch is for as an argument.
pub fn launch_links<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .filter(|arg| is_link(arg.as_ref()))
        .map(|arg| arg.as_ref().to_owned())
        .collect()
}
