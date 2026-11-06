//! Starter helpers for a desktop application.
//!
//! This is not the component library. Semantic controls live in `rndr_ui`,
//! which this crate does not depend on. The helpers here are a small starting
//! point: form navigation, a few layout constants, and draft preferences.
//! Install [`FormNavigation`] before your own Update systems.
//! See `examples/desktop` for navigation, draft settings and task ownership.
#![deny(missing_docs)]

use rndr_core::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// Enable core's canonical chronological form interaction policy.
///
/// Cyclic Tab traversal respects visibility, disabled controls and TabStop.
/// Fields created by [`field`] opt into FieldTransactions; custom fields can
/// supply their own transaction callback. No input queue is rewritten here.
///
/// Scrolling is installed the same way `UiPlugin` in `rndr_ui` installs it:
/// [`rndr_core::apply_scroll`] and [`rndr_core::apply_scrollbars`] at the front
/// of Input, so a wheel or a scrollbar track is claimed before focus reveal.
/// A second registration is safe. Wheel deltas are consumed, and a drag is
/// measured from the last applied position.
pub struct FormNavigation;
impl Plugin for FormNavigation {
    fn build(self, app: &mut App) {
        app.add_plugin(GenericInteraction);
        app.add_system_front(Schedule::Input, apply_scroll);
        // After the front scroll registration, so a track claims its input
        // before the parent viewport scrolls.
        app.add_system_front(Schedule::Input, apply_scrollbars);
    }
}

fn field_transaction(_: &mut World, _: Entity, _: FieldTransaction) {}

/// Space between related controls, in logical points.
pub const GAP: f32 = 12.0;
/// Page inset, in logical points.
pub const INSET: f32 = 24.0;
/// Minimum single-line control height, in logical points.
pub const CONTROL: f32 = 44.0;
/// Body typography size; headings use 24 points.
pub const BODY: f32 = 16.0;

/// A fill-width column using the shared spacing scale.
pub fn column(height: Size) -> Layout {
    Layout {
        width: Size::Fill,
        height,
        gap: GAP,
        ..Layout::column()
    }
}

/// Attach an ordinary ECS entity to a parent and return it.
pub fn child(world: &mut World, parent: Entity, entity: Entity) -> Entity {
    set_parent(world, entity, parent);
    entity
}

/// Wrapped text without a background fill. The default Theme supplies ink.
pub fn label(world: &mut World, parent: Entity, text: &str, heading: bool) -> Entity {
    let entity = world.spawn((
        column(Size::Hug),
        Label::new(text).overflow(TextOverflow::Wrap),
        TextStyle {
            family: "sans-serif".into(),
            size: if heading { 24.0 } else { BODY },
            line_height: Some(1.35),
            ..TextStyle::default()
        },
    ));
    child(world, parent, entity)
}

/// Focusable action using canonical release activation and ActionIntents.
/// Hosts drain their accepted intents during Update or supply ActivationHandler.
pub fn button(world: &mut World, parent: Entity, action: &str, text: &str) -> Entity {
    let theme = theme_of(world);
    let entity = world.spawn((
        column(Size::Hug),
        Label::new(text).overflow(TextOverflow::Wrap),
        Button::new(action),
        Focusable,
        theme.title,
    ));
    world.insert(entity, ReleaseActivation);
    world.insert(entity, Radius(theme.radius));
    world.insert(
        entity,
        TextStyle {
            family: "sans-serif".into(),
            size: BODY,
            line_height: Some(1.35),
            ..TextStyle::default()
        },
    );
    world.get_mut::<Layout>(entity).unwrap().min_height = CONTROL;
    child(world, parent, entity)
}

/// Labeled nonsecret text field. Never use this for passwords or API keys:
/// TextField publishes ordinary plaintext edit events and supports clipboard.
pub fn field(world: &mut World, parent: Entity, caption: &str, value: &str) -> Entity {
    label(world, parent, caption, false);
    let theme = theme_of(world);
    let entity = world.spawn((
        column(Size::Px(CONTROL)),
        Label::new(value).overflow(TextOverflow::Ellipsis),
        TextField,
        Focusable,
        CapturesKeys,
        theme.pane,
    ));
    world.insert(entity, Radius(theme.radius));
    world.insert(
        entity,
        TextStyle {
            family: "sans-serif".into(),
            size: BODY,
            ..TextStyle::default()
        },
    );
    world.insert(entity, FieldTransactions(field_transaction));
    world.insert(entity, Placeholder::new(caption));
    world.insert(
        entity,
        Semantics {
            name: Some(caption.into()),
            ..Semantics::default()
        },
    );
    child(world, parent, entity)
}

/// Switch existing page subtrees without rebuilding them or losing drafts.
/// The destination gets focus, so a hidden page never retains keyboard input.
/// Returns false without changes if the destination is not one of `pages`.
pub fn navigate(world: &mut World, pages: &[Entity], destination: Entity, focus: Entity) -> bool {
    if !pages.contains(&destination) {
        return false;
    }
    for page in pages {
        if *page == destination {
            world.remove::<Hidden>(*page);
        } else {
            world.insert(*page, Hidden);
        }
    }
    request_focus(world, focus);
    true
}

/// Explicit copy for common async states. The app owns transitions, task
/// handles, retry policy and actual error details; do not put secrets here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewState {
    /// No work or data yet.
    Empty,
    /// A cancellable operation is in flight.
    Loading,
    /// Success with application-owned safe copy.
    Ready(String),
    /// Failure with application-owned safe copy.
    Error(String),
    /// User cancelled the operation.
    Cancelled,
}
impl ViewState {
    /// Short display copy. This does not log or classify provider errors.
    pub fn message(&self) -> &str {
        match self {
            Self::Empty => "Nothing here yet. Start the offline sample.",
            Self::Loading => "Working locally… You can cancel safely.",
            Self::Ready(text) | Self::Error(text) => text,
            Self::Cancelled => "Cancelled. No result was applied. Try again when ready.",
        }
    }
}

/// Bounded JSON preferences for **nonsecret** application-owned schemas.
/// Missing files return None; malformed/unreadable files return an error,
/// never silently overwrite them. Saves use a unique sibling temp and rename.
/// This is single-writer, not a multi-process database or secure storage.
#[derive(Clone, Debug)]
pub struct Preferences {
    path: PathBuf,
}
impl Preferences {
    /// Explicit path; useful for isolated tests and portable app configuration.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    /// Platform config directory plus a single safe app identifier.
    /// macOS: Application Support; Windows: APPDATA; Unix: XDG_CONFIG_HOME
    /// (absolute only), then HOME/.config. No current-directory fallback.
    pub fn for_app(id: &str) -> std::io::Result<Self> {
        if id.is_empty()
            || id == "."
            || id == ".."
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.')
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid app identifier",
            ));
        }
        let home = || std::env::var_os("HOME").map(PathBuf::from);
        let base = if cfg!(target_os = "macos") {
            home().map(|p| p.join("Library/Application Support"))
        } else if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| home().map(|p| p.join(".config")))
        }
        .filter(|p| p.is_absolute())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no absolute config directory")
        })?;
        Ok(Self::at(base.join(id).join("preferences.json")))
    }
    /// Config path. The app may display it but should not put secrets in it.
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Read at most 16 KiB. Apps should validate schema version and semantics
    /// after decoding, and use serde's deny_unknown_fields for strict schemas.
    pub fn load<T: DeserializeOwned>(&self) -> std::io::Result<Option<T>> {
        let file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let mut bytes = Vec::new();
        file.take(16385).read_to_end(&mut bytes)?;
        if bytes.len() > 16384 {
            return Err(invalid("preferences exceed 16 KiB"));
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| invalid("invalid preferences JSON"))
    }
    /// Encode at most 16 KiB, sync the temp file, then atomically replace on
    /// macOS/Unix. Rename/replace behavior on Windows needs native validation.
    /// A failure preserves the previous destination. No crash-durability claim
    /// is made for the parent directory. Only our own temp file is removed.
    pub fn save<T: Serialize>(&self, value: &T) -> std::io::Result<()> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let bytes =
            serde_json::to_vec_pretty(value).map_err(|_| invalid("cannot encode preferences"))?;
        if bytes.len() > 16384 {
            return Err(invalid("preferences exceed 16 KiB"));
        }
        let parent = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)?;
        let temp = parent.join(format!(
            ".rndr-prefs-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        let result = (|| {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temp, &self.path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result
    }
}
fn invalid(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}
