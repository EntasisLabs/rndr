//! Command palette.
//!
//! A focus-scoped combobox: a text field over a uniform virtual list, opened
//! as an [`AnchoredPopup`] on the next [`rndr_core::Layer`]. The popup is the focus
//! scope. Escape dismisses it and returns focus to the anchor, unless the
//! host defers that key. Arrow, Page, Home, and End move the active command.
//! Enter activates it. Focus stays on the field.
//!
//! The host supplies commands. The default matcher is [`fuzzy_match`]. A
//! synchronous catalog filters on each edit. Async results keep the current
//! rows until [`PaletteHandle::resolve`] accepts the epoch that edit started.
//! Prefix modes, sections, Lucide icons, and shortcut hints are optional.
//!
//! Headless tests cover this contract. They do not certify VoiceOver,
//! Narrator, or Orca.

use std::collections::HashMap;

use crate::{
    collections::{
        CollectionEvent, CollectionEventKind, Item, ItemId, ListView, ViewProps, VirtualListPolicy,
        VirtualViewHandle,
    },
    Corner, Token, Typography, UiError, UiId, UiStyle, UiTheme,
};
use rndr_core::{
    replace_field_text, AccessMark, AccessRole, Align, AnchoredPopup, CapturesKeys, Entity,
    Focusable, Hidden, Icon, Input, Items, Justify, KeyEvent, KeyNavigation, Label, Layout,
    LivePoliteness, LiveRegion, LogicalKey, NamedKey, Placeholder, PopupEscape, PopupPlacement,
    Rect, SelectionAccess, Semantics, Size, Spans, TextChanged, TextField, TextOverflow, TextSpan,
    World, TEXT_ORIGIN_X,
};
use unicode_normalization::UnicodeNormalization;

/// High bit reserved for section rows. Command ids must leave it clear.
const SECTION_BIT: u64 = 1 << 63;
const LIST_KEY: UiId = UiId(1);
const ICON_PX: f32 = 16.0;

/// `id` is a generated section row, not a command.
pub fn is_section(id: ItemId) -> bool {
    id.0 & SECTION_BIT != 0
}

/// One prefix the host chose, such as `>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteMode {
    /// Leading character. Whitespace is rejected.
    pub prefix: char,
    /// Accessible name of the mode.
    pub name: String,
    /// Field hint while this mode is active. Empty keeps the palette placeholder.
    pub placeholder: String,
}

impl PaletteMode {
    /// Mode selected by `prefix`.
    pub fn new(prefix: char, name: impl Into<String>, placeholder: impl Into<String>) -> Self {
        Self {
            prefix,
            name: name.into(),
            placeholder: placeholder.into(),
        }
    }
}

/// One command the palette can show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteCommand {
    /// Stable id. The high bit is reserved for section rows.
    pub id: ItemId,
    /// Visible title and accessible name. Match ranges refer to these bytes.
    pub title: String,
    /// Section label. A new label inserts a header the arrows skip.
    pub group: Option<String>,
    /// Lucide glyph drawn at the start of the row.
    pub icon: Option<Icon>,
    /// Hint only. The palette does not bind this chord.
    pub shortcut: String,
    /// Disabled commands stay visible and are not activated.
    pub enabled: bool,
    /// Empty means the unprefixed mode only. Otherwise only these prefixes.
    pub modes: Vec<char>,
}

impl PaletteCommand {
    /// Enabled command with no group, icon, shortcut, or mode.
    pub fn new(id: u64, title: impl Into<String>) -> Self {
        Self {
            id: ItemId(id),
            title: title.into(),
            group: None,
            icon: None,
            shortcut: String::new(),
            enabled: true,
            modes: Vec::new(),
        }
    }
}

/// Scored hit from a [`PaletteMatcher`]. Lower [`Self::score`] sorts first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteMatch {
    /// Sort key. The default matcher prefers a tight, early match.
    pub score: i32,
    /// Byte ranges in the title. Adjacent hits are already merged.
    pub ranges: Vec<(usize, usize)>,
}

/// Host filter. `None` hides the command. The palette does not call it for
/// async rows the host already chose; it still uses it to highlight those titles.
pub type PaletteMatcher = fn(&str, &str) -> Option<PaletteMatch>;

/// Field text after the optional prefix has been split off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteQuery {
    /// Exact field text.
    pub text: String,
    /// Text after the mode prefix, with one leading space removed.
    pub filter: String,
    /// Active prefix, if the first character selected a host mode.
    pub mode: Option<char>,
    /// Generation of this text. [`PaletteHandle::resolve`] must echo it.
    pub epoch: u64,
}

/// The field text changed. Readable in Update on the following frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteQueryChanged {
    /// Palette popup.
    pub palette: Entity,
    /// Exact field text.
    pub text: String,
    /// Text the matcher should see.
    pub filter: String,
    /// Active prefix.
    pub mode: Option<char>,
    /// Epoch [`PaletteHandle::resolve`] must match.
    pub epoch: u64,
}

impl rndr_core::Event for PaletteQueryChanged {}

/// Enter or a later pointer activation.
///
/// Keyboard Enter is visible to Update on that frame. A pointer double-click
/// is observed in Update, so this event is visible on the following frame.
/// `CollectionEvent::Activated` on the list is visible in Update for both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteActivated {
    /// Palette popup.
    pub palette: Entity,
    /// Command id. Section rows are not activated.
    pub id: ItemId,
}

impl rndr_core::Event for PaletteActivated {}

/// Escape closed the popup, or the host called [`PaletteHandle::close`].
/// Visible to Update on that frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteDismissed {
    /// Palette popup.
    pub palette: Entity,
    /// Focus after the popup returned it to the anchor, when that succeeded.
    pub focus: Option<Entity>,
}

impl rndr_core::Event for PaletteDismissed {}

/// Width, list height, and the strings the palette shows for itself.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteProps {
    /// Accessible name of the combobox.
    pub name: String,
    /// Field hint in the unprefixed mode.
    pub placeholder: String,
    /// Shown when a finished query has no rows.
    pub empty: String,
    /// Shown while an async query is unresolved.
    pub loading: String,
    /// Popup width in points.
    pub width: f32,
    /// Virtual list viewport height in points.
    pub list_height: f32,
    /// Uniform row height in points, including section headers.
    pub row_height: f32,
}

impl PaletteProps {
    /// 560 by 320 points, 32-point rows, and short English strings.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            placeholder: "Type to filter".into(),
            empty: "No matches".into(),
            loading: "Loading…".into(),
            width: 560.0,
            list_height: 320.0,
            row_height: 32.0,
        }
    }
}

#[derive(Clone)]
struct Shown {
    title: String,
    shortcut: String,
    icon: Option<Icon>,
    section: bool,
    ranges: Vec<(usize, usize)>,
}

#[derive(Clone, Copy)]
struct Parts {
    lead: Entity,
    icon: Entity,
    title: Entity,
    shortcut: Entity,
    header: Entity,
}

impl rndr_core::Component for Parts {}

struct PaletteState {
    id: UiId,
    anchor: Entity,
    field: Entity,
    list: Entity,
    status: Entity,
    props: PaletteProps,
    modes: Vec<PaletteMode>,
    catalog: Vec<PaletteCommand>,
    resolved: Vec<PaletteCommand>,
    matcher: PaletteMatcher,
    shown: HashMap<ItemId, Shown>,
    async_results: bool,
    loading: bool,
    text: String,
    filter: String,
    mode: Option<char>,
    epoch: u64,
    defer_escape: bool,
    suspended: bool,
}

impl rndr_core::Component for PaletteState {}

#[derive(Clone, Copy)]
struct PaletteLink(Entity);

impl rndr_core::Component for PaletteLink {}

/// Open or closed palette. The root is the popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteHandle {
    /// Anchored popup. Hidden while closed.
    pub root: Entity,
    /// Query field. Focus stays here while the palette is open.
    pub field: Entity,
    /// Virtual list of matches and section headers.
    pub list: Entity,
    /// Control the popup is anchored to, and where Escape returns focus.
    pub anchor: Entity,
    /// Live loading or empty line. Hidden while rows are showing and idle.
    pub status: Entity,
}

/// Mount a closed palette. Call [`PaletteHandle::open`] to show it.
pub struct CommandPalette;

impl CommandPalette {
    /// Mount under `id` for `anchor`. A second mount with the same pair
    /// updates strings and sizes and keeps the query.
    pub fn mount(
        world: &mut World,
        anchor: Entity,
        id: UiId,
        props: PaletteProps,
    ) -> Result<PaletteHandle, UiError> {
        if !world.is_alive(anchor) || !world.has::<Layout>(anchor) {
            return Err(UiError::Stale);
        }
        validate_props(&props)?;
        if let Some(root) = find_existing(world, anchor, id) {
            let handle = handle_of(world, root)?;
            {
                let state = world.get_mut::<PaletteState>(root).ok_or(UiError::Stale)?;
                state.props = props;
            }
            rebuild(world, root)?;
            return Ok(handle);
        }
        let root = world.spawn((
            Layout {
                width: Size::Px(props.width),
                padding: 8.0,
                gap: 6.0,
                ..Layout::column()
            },
            Hidden,
            AnchoredPopup {
                anchor,
                max_height: props.list_height + 180.0,
                closed: on_closed,
            },
            PopupEscape(escape_allows_close),
            PopupPlacement::At([0.0, 0.0]),
            UiStyle {
                background: Some(Token::Overlay.into()),
                corner: Some(Corner::Group),
                ..UiStyle::default()
            },
        ));
        let field = world.spawn((
            Layout {
                width: Size::Fill,
                min_height: 36.0,
                padding: 8.0,
                ..Layout::column()
            },
            Label::new(""),
            TextField,
            CapturesKeys,
            Focusable,
            Placeholder::new(&props.placeholder),
        ));
        world.insert(field, KeyNavigation(on_key));
        world.insert(
            field,
            UiStyle {
                background: Some(Token::Content.into()),
                corner: Some(Corner::Control),
                ..UiStyle::default()
            },
        );
        rndr_core::set_parent(world, field, root);
        let list = ListView::mount_virtual(
            world,
            root,
            LIST_KEY,
            ViewProps::new(&props.name),
            &[],
            props.row_height,
            compose_row,
        )
        .map_err(|_| UiError::InvalidProps)?
        .root;
        if let Some(layout) = world.get_mut::<Layout>(list) {
            layout.height = Size::Px(props.list_height);
        }
        world.insert(list, PaletteLink(root));
        world.insert(
            list,
            VirtualListPolicy {
                hover_navigation: true,
                selection_follows_navigation: true,
                double_click_activate: true,
                focus_owner: Some(field),
            },
        );
        let status = world.spawn((
            Layout {
                width: Size::Fill,
                min_height: 20.0,
                ..Layout::column()
            },
            Label::new(&props.empty).overflow(TextOverflow::Ellipsis),
            AccessMark(AccessRole::Status),
            LiveRegion(LivePoliteness::Polite),
            Hidden,
        ));
        rndr_core::set_parent(world, status, root);
        world.insert(
            root,
            PaletteState {
                id,
                anchor,
                field,
                list,
                status,
                props,
                modes: Vec::new(),
                catalog: Vec::new(),
                resolved: Vec::new(),
                matcher: fuzzy_match,
                shown: HashMap::new(),
                async_results: false,
                loading: false,
                text: String::new(),
                filter: String::new(),
                mode: None,
                epoch: 0,
                defer_escape: false,
                suspended: false,
            },
        );
        project_access(world, root);
        Ok(PaletteHandle {
            root,
            field,
            list,
            anchor,
            status,
        })
    }
}

impl PaletteHandle {
    /// Show the popup and focus the field. A higher modal refuses the open.
    pub fn open(self, world: &mut World) -> bool {
        if !rndr_core::open_popup(world, self.root) {
            return false;
        }
        let _ = rndr_core::request_focus(world, self.field);
        project_access(world, self.root);
        true
    }

    /// Hide the popup and return focus to the anchor.
    pub fn close(self, world: &mut World) {
        rndr_core::close_popup(world, self.root, true);
    }

    /// The popup is open.
    pub fn is_open(self, world: &World) -> bool {
        world.is_alive(self.root) && !world.has::<Hidden>(self.root)
    }

    /// Current field text and epoch.
    pub fn query(self, world: &World) -> Option<PaletteQuery> {
        world.get::<PaletteState>(self.root).map(query_of)
    }

    /// Epoch a new resolve must match. Zero until the first edit.
    pub fn epoch(self, world: &World) -> Option<u64> {
        world
            .get::<PaletteState>(self.root)
            .map(|state| state.epoch)
    }

    /// Active command. Section rows are omitted.
    pub fn active(self, world: &World) -> Option<ItemId> {
        VirtualViewHandle { root: self.list }
            .active(world)
            .filter(|id| !is_section(*id))
    }

    /// `true` while the host has not resolved the current async epoch.
    pub fn is_loading(self, world: &World) -> bool {
        world
            .get::<PaletteState>(self.root)
            .is_some_and(|state| state.loading)
    }

    /// Replace the synchronous catalog. Async mode keeps the visible rows
    /// until [`Self::resolve`].
    pub fn set_commands(
        self,
        world: &mut World,
        commands: &[PaletteCommand],
    ) -> Result<(), UiError> {
        validate_commands(commands)?;
        {
            let state = world
                .get_mut::<PaletteState>(self.root)
                .ok_or(UiError::Stale)?;
            state.catalog = commands.to_vec();
        }
        if !world
            .get::<PaletteState>(self.root)
            .is_some_and(|state| state.async_results)
        {
            rebuild(world, self.root)?;
        }
        Ok(())
    }

    /// Replace the host's prefix modes and re-split the current text.
    pub fn set_modes(self, world: &mut World, modes: &[PaletteMode]) -> Result<(), UiError> {
        validate_modes(modes)?;
        let text = world
            .get::<PaletteState>(self.root)
            .ok_or(UiError::Stale)?
            .text
            .clone();
        {
            let state = world
                .get_mut::<PaletteState>(self.root)
                .ok_or(UiError::Stale)?;
            state.modes = modes.to_vec();
        }
        apply_text(world, self.root, &text, false)?;
        Ok(())
    }

    /// Replace the matcher. Sync mode rebuilds immediately.
    pub fn set_matcher(self, world: &mut World, matcher: PaletteMatcher) -> Result<(), UiError> {
        {
            let state = world
                .get_mut::<PaletteState>(self.root)
                .ok_or(UiError::Stale)?;
            state.matcher = matcher;
        }
        if !world
            .get::<PaletteState>(self.root)
            .is_some_and(|state| state.async_results)
        {
            rebuild(world, self.root)?;
        }
        Ok(())
    }

    /// Async mode emits a new epoch and waits for [`Self::resolve`].
    /// Turning it off filters the catalog locally again.
    pub fn set_async(self, world: &mut World, enabled: bool) -> Result<(), UiError> {
        {
            let state = world
                .get_mut::<PaletteState>(self.root)
                .ok_or(UiError::Stale)?;
            state.async_results = enabled;
            state.loading = enabled;
        }
        if enabled {
            paint_status(world, self.root);
            if world
                .get::<PaletteState>(self.root)
                .is_some_and(|state| state.shown.is_empty())
            {
                rebuild(world, self.root)?;
            }
        } else {
            rebuild(world, self.root)?;
        }
        Ok(())
    }

    /// Force the loading line. An accepted [`Self::resolve`] clears it.
    pub fn set_loading(self, world: &mut World, loading: bool) -> Result<(), UiError> {
        {
            let state = world
                .get_mut::<PaletteState>(self.root)
                .ok_or(UiError::Stale)?;
            state.loading = loading;
        }
        paint_status(world, self.root);
        if world
            .get::<PaletteState>(self.root)
            .is_some_and(|state| state.shown.is_empty())
        {
            rebuild(world, self.root)?;
        }
        Ok(())
    }

    /// Show `commands` for `epoch`. `Ok(false)` means a newer edit won and
    /// nothing changed. Invalid commands are refused before mutation.
    pub fn resolve(
        self,
        world: &mut World,
        epoch: u64,
        commands: &[PaletteCommand],
    ) -> Result<bool, UiError> {
        validate_commands(commands)?;
        let current = world
            .get::<PaletteState>(self.root)
            .ok_or(UiError::Stale)?
            .epoch;
        if epoch != current {
            return Ok(false);
        }
        {
            let state = world
                .get_mut::<PaletteState>(self.root)
                .ok_or(UiError::Stale)?;
            state.resolved = commands.to_vec();
            state.loading = false;
        }
        rebuild(world, self.root)?;
        Ok(true)
    }

    /// Replace the field text and start a new epoch, as if the user had typed it.
    pub fn set_query(self, world: &mut World, text: &str) -> Result<PaletteQuery, UiError> {
        if !world.is_alive(self.field) {
            return Err(UiError::Stale);
        }
        replace_field_text(world, self.field, text);
        apply_text(world, self.root, text, true)?;
        world
            .get::<PaletteState>(self.root)
            .map(query_of)
            .ok_or(UiError::Stale)
    }

    /// While suspended, typing and arrows stay with the field. Ask-style
    /// prompts use this so Enter submits instead of activating a command.
    pub fn set_suspended(self, world: &mut World, suspended: bool) -> Result<(), UiError> {
        world
            .get_mut::<PaletteState>(self.root)
            .ok_or(UiError::Stale)?
            .suspended = suspended;
        Ok(())
    }

    /// While deferred, Escape leaves the popup open and the key unconsumed.
    pub fn set_defer_escape(self, world: &mut World, defer: bool) -> Result<(), UiError> {
        world
            .get_mut::<PaletteState>(self.root)
            .ok_or(UiError::Stale)?
            .defer_escape = defer;
        Ok(())
    }
}

/// Caller-stable id of a palette popup. Row contents have none.
pub(crate) fn key_at(world: &World, entity: Entity) -> Option<UiId> {
    world.get::<PaletteState>(entity).map(|state| state.id)
}

/// Apply field edits and republish pointer activations.
pub(crate) fn drive(world: &mut World) {
    let edits: Vec<(Entity, String)> = rndr_core::events::<TextChanged>(world)
        .iter()
        .map(|event| (event.entity, event.text.clone()))
        .collect();
    for (entity, text) in edits {
        if let Some(root) = palette_for_field(world, entity) {
            let suspended = world
                .get::<PaletteState>(root)
                .is_some_and(|state| state.suspended);
            if !suspended {
                let _ = apply_text(world, root, &text, true);
            }
        }
    }
    let activations: Vec<(Entity, ItemId)> = rndr_core::events::<CollectionEvent>(world)
        .iter()
        .filter_map(|event| match event.kind {
            CollectionEventKind::Activated(id) => Some((event.view, id)),
            _ => None,
        })
        .collect();
    for (list, id) in activations {
        if is_section(id) {
            continue;
        }
        let Some(root) = palette_for_list(world, list) else {
            continue;
        };
        let published = rndr_core::events::<PaletteActivated>(world)
            .iter()
            .any(|event| event.palette == root && event.id == id);
        if !published {
            rndr_core::send(world, PaletteActivated { palette: root, id });
        }
    }
}

/// Keep an open palette under its anchor at the host's width.
pub(crate) fn place(world: &mut World) {
    let roots: Vec<Entity> = world.entities_with::<PaletteState>();
    for root in roots {
        if world.has::<Hidden>(root) {
            continue;
        }
        let Some(state) = world.get::<PaletteState>(root) else {
            continue;
        };
        let anchor = state.anchor;
        let width = state.props.width;
        let rect = world.get::<Rect>(anchor).copied().unwrap_or_default();
        world.insert(root, PopupPlacement::At([rect.x, rect.y + rect.h]));
        if let Some(layout) = world.get_mut::<Layout>(root) {
            layout.width = Size::Px(width);
        }
    }
}

fn find_existing(world: &World, anchor: Entity, id: UiId) -> Option<Entity> {
    world
        .entities_with::<PaletteState>()
        .into_iter()
        .find(|root| {
            world
                .get::<PaletteState>(*root)
                .is_some_and(|state| state.anchor == anchor && state.id == id)
        })
}

fn handle_of(world: &World, root: Entity) -> Result<PaletteHandle, UiError> {
    let state = world.get::<PaletteState>(root).ok_or(UiError::Stale)?;
    Ok(PaletteHandle {
        root,
        field: state.field,
        list: state.list,
        anchor: state.anchor,
        status: state.status,
    })
}

fn query_of(state: &PaletteState) -> PaletteQuery {
    PaletteQuery {
        text: state.text.clone(),
        filter: state.filter.clone(),
        mode: state.mode,
        epoch: state.epoch,
    }
}

fn validate_props(props: &PaletteProps) -> Result<(), UiError> {
    if props.name.trim().is_empty()
        || props.placeholder.trim().is_empty()
        || props.empty.trim().is_empty()
        || props.loading.trim().is_empty()
        || !props.width.is_finite()
        || props.width <= 0.0
        || !props.list_height.is_finite()
        || props.list_height <= 0.0
        || !props.row_height.is_finite()
        || props.row_height <= 0.0
    {
        return Err(UiError::InvalidProps);
    }
    Ok(())
}

fn validate_commands(commands: &[PaletteCommand]) -> Result<(), UiError> {
    let mut seen = HashMap::new();
    for command in commands {
        if command.title.trim().is_empty()
            || is_section(command.id)
            || seen.insert(command.id, ()).is_some()
        {
            return Err(UiError::InvalidProps);
        }
    }
    Ok(())
}

fn validate_modes(modes: &[PaletteMode]) -> Result<(), UiError> {
    let mut seen = Vec::new();
    for mode in modes {
        if mode.prefix.is_whitespace() || mode.name.trim().is_empty() || seen.contains(&mode.prefix)
        {
            return Err(UiError::InvalidProps);
        }
        seen.push(mode.prefix);
    }
    Ok(())
}

fn palette_for_field(world: &World, field: Entity) -> Option<Entity> {
    world
        .entities_with::<PaletteState>()
        .into_iter()
        .find(|root| {
            world
                .get::<PaletteState>(*root)
                .is_some_and(|state| state.field == field)
        })
}

fn palette_for_list(world: &World, list: Entity) -> Option<Entity> {
    world
        .get::<PaletteLink>(list)
        .map(|link| link.0)
        .filter(|root| world.is_alive(*root))
}

fn apply_text(world: &mut World, root: Entity, text: &str, bump: bool) -> Result<(), UiError> {
    let (modes, previous) = {
        let state = world.get::<PaletteState>(root).ok_or(UiError::Stale)?;
        (state.modes.clone(), state.text.clone())
    };
    if bump && text == previous {
        return Ok(());
    }
    let (mode, filter) = split_mode(text, &modes);
    let placeholder = modes
        .iter()
        .find(|candidate| Some(candidate.prefix) == mode)
        .map(|candidate| candidate.placeholder.clone())
        .filter(|text| !text.is_empty());
    let async_results = {
        let state = world.get_mut::<PaletteState>(root).ok_or(UiError::Stale)?;
        if bump {
            state.epoch = state.epoch.saturating_add(1);
        }
        state.text = text.to_string();
        state.filter = filter;
        state.mode = mode;
        if state.async_results && bump {
            state.loading = true;
        }
        let fallback = state.props.placeholder.clone();
        let field = state.field;
        let async_results = state.async_results;
        let hint = placeholder.unwrap_or(fallback);
        if let Some(slot) = world.get_mut::<Placeholder>(field) {
            slot.text = hint;
        }
        async_results
    };
    if async_results && bump {
        paint_status(world, root);
        if world
            .get::<PaletteState>(root)
            .is_some_and(|state| state.shown.is_empty())
        {
            rebuild(world, root)?;
        }
    } else if !async_results {
        rebuild(world, root)?;
    }
    let query = world
        .get::<PaletteState>(root)
        .map(query_of)
        .ok_or(UiError::Stale)?;
    rndr_core::send(
        world,
        PaletteQueryChanged {
            palette: root,
            text: query.text,
            filter: query.filter,
            mode: query.mode,
            epoch: query.epoch,
        },
    );
    Ok(())
}

fn split_mode(text: &str, modes: &[PaletteMode]) -> (Option<char>, String) {
    let mut chars = text.chars();
    if let Some(first) = chars.next() {
        if !first.is_whitespace() && modes.iter().any(|mode| mode.prefix == first) {
            return (Some(first), chars.as_str().trim_start().to_string());
        }
    }
    (None, text.to_string())
}

fn accepts(command: &PaletteCommand, mode: Option<char>) -> bool {
    match mode {
        None => command.modes.is_empty(),
        Some(prefix) => command.modes.contains(&prefix),
    }
}

fn rebuild(world: &mut World, root: Entity) -> Result<(), UiError> {
    let state = world
        .get::<PaletteState>(root)
        .and_then(PaletteView::cloned_parts)
        .ok_or(UiError::Stale)?;
    let (items, shown) = layout_rows(&state);
    let empty = if state.loading && items.is_empty() {
        state.props.loading.clone()
    } else {
        state.props.empty.clone()
    };
    {
        let state = world.get_mut::<PaletteState>(root).ok_or(UiError::Stale)?;
        state.shown = shown;
    }
    let mut props = ViewProps::new(&state.props.name);
    props.empty = empty;
    let list = state.list;
    let parent = world
        .get::<rndr_core::Parent>(list)
        .map(|parent| parent.entity)
        .unwrap_or(root);
    ListView::mount_virtual(
        world,
        parent,
        LIST_KEY,
        props,
        &items,
        state.props.row_height,
        compose_row,
    )
    .map_err(|_| UiError::InvalidProps)?;
    if let Some(layout) = world.get_mut::<Layout>(list) {
        layout.height = Size::Px(state.props.list_height);
    }
    world.insert(list, PaletteLink(root));
    world.insert(
        list,
        VirtualListPolicy {
            hover_navigation: true,
            selection_follows_navigation: true,
            double_click_activate: true,
            focus_owner: Some(state.field),
        },
    );
    let active = VirtualViewHandle { root: list }
        .active(world)
        .filter(|id| !is_section(*id));
    if let Some(id) = active {
        let _ = VirtualViewHandle { root: list }.select(world, Some(id));
    }
    paint_status(world, root);
    project_access(world, root);
    Ok(())
}

struct Ranked {
    command: PaletteCommand,
    ranges: Vec<(usize, usize)>,
}

fn layout_rows(state: &PaletteParts) -> (Vec<Item>, HashMap<ItemId, Shown>) {
    let ranked: Vec<Ranked> = if state.async_results {
        state
            .resolved
            .iter()
            .cloned()
            .map(|command| Ranked {
                ranges: highlight(state.matcher, &state.filter, &command.title),
                command,
            })
            .collect()
    } else {
        let mut ranked = Vec::new();
        for (index, command) in state.catalog.iter().enumerate() {
            if !accepts(command, state.mode) {
                continue;
            }
            let Some(hit) = (state.matcher)(&state.filter, &command.title) else {
                continue;
            };
            ranked.push((hit.score, index, command.clone(), hit.ranges));
        }
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        // Keep a section together. Group order follows the best score in it,
        // and rows inside a section stay in score order.
        let mut buckets: Vec<(Option<String>, Vec<Ranked>)> = Vec::new();
        for (_, _, command, ranges) in ranked {
            let key = command
                .group
                .as_ref()
                .map(|group| group.trim().to_string())
                .filter(|group| !group.is_empty());
            if let Some((_, rows)) = buckets.iter_mut().find(|(name, _)| name == &key) {
                rows.push(Ranked { command, ranges });
            } else {
                buckets.push((key, vec![Ranked { command, ranges }]));
            }
        }
        buckets.into_iter().flat_map(|(_, rows)| rows).collect()
    };
    let mut items = Vec::new();
    let mut shown = HashMap::new();
    let mut last_group: Option<String> = None;
    let mut section_index = 0u64;
    for row in ranked {
        if let Some(group) = row
            .command
            .group
            .as_ref()
            .map(|group| group.trim())
            .filter(|group| !group.is_empty())
        {
            if last_group.as_deref() != Some(group) {
                let id = ItemId(SECTION_BIT | section_index);
                section_index += 1;
                items.push(Item {
                    id,
                    name: group.to_string(),
                    enabled: false,
                });
                shown.insert(
                    id,
                    Shown {
                        title: group.to_string(),
                        shortcut: String::new(),
                        icon: None,
                        section: true,
                        ranges: Vec::new(),
                    },
                );
                last_group = Some(group.to_string());
            }
        } else {
            last_group = None;
        }
        let mut item = Item::new(row.command.id.0, row.command.title.clone());
        item.enabled = row.command.enabled;
        let title = row.command.title.clone();
        items.push(item);
        shown.insert(
            row.command.id,
            Shown {
                ranges: safe_ranges(&title, &row.ranges),
                title,
                shortcut: row.command.shortcut,
                icon: row.command.icon,
                section: false,
            },
        );
    }
    (items, shown)
}

fn highlight(matcher: PaletteMatcher, filter: &str, title: &str) -> Vec<(usize, usize)> {
    if filter.is_empty() {
        Vec::new()
    } else {
        matcher(filter, title)
            .map(|hit| hit.ranges)
            .unwrap_or_default()
    }
}

fn safe_ranges(title: &str, ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    ranges
        .iter()
        .copied()
        .filter(|(start, end)| {
            *start <= *end
                && *end <= title.len()
                && title.is_char_boundary(*start)
                && title.is_char_boundary(*end)
        })
        .collect()
}

/// Snapshot of the fields [`layout_rows`] needs. Avoids cloning the whole map.
struct PaletteParts {
    props: PaletteProps,
    catalog: Vec<PaletteCommand>,
    resolved: Vec<PaletteCommand>,
    matcher: PaletteMatcher,
    async_results: bool,
    loading: bool,
    filter: String,
    mode: Option<char>,
    list: Entity,
    field: Entity,
}

trait PaletteView {
    fn cloned_parts(&self) -> Option<PaletteParts>;
}

impl PaletteView for PaletteState {
    fn cloned_parts(&self) -> Option<PaletteParts> {
        Some(PaletteParts {
            props: self.props.clone(),
            catalog: self.catalog.clone(),
            resolved: self.resolved.clone(),
            matcher: self.matcher,
            async_results: self.async_results,
            loading: self.loading,
            filter: self.filter.clone(),
            mode: self.mode,
            list: self.list,
            field: self.field,
        })
    }
}

fn paint_status(world: &mut World, root: Entity) {
    let Some(state) = world.get::<PaletteState>(root) else {
        return;
    };
    let show_rows = !state.shown.is_empty();
    let (text, show) = if state.loading {
        (state.props.loading.clone(), true)
    } else if !show_rows {
        (state.props.empty.clone(), true)
    } else {
        (String::new(), false)
    };
    let status = state.status;
    if let Some(label) = world.get_mut::<Label>(status) {
        label.text = text;
    }
    if show {
        world.remove::<Hidden>(status);
    } else {
        world.insert(status, Hidden);
    }
}

fn project_access(world: &mut World, root: Entity) {
    let Some(state) = world.get::<PaletteState>(root) else {
        return;
    };
    let open = !world.has::<Hidden>(root);
    let field = state.field;
    let list = state.list;
    let name = state.props.name.clone();
    let active_row = VirtualViewHandle { root: list }
        .active(world)
        .filter(|id| !is_section(*id))
        .and_then(|id| VirtualViewHandle { root: list }.row(world, id))
        .map(|slot| slot.root);
    world.insert(
        field,
        SelectionAccess {
            role: AccessRole::ComboBox,
            checked: None,
            expanded: Some(open),
            multiselect: false,
            controls: open.then_some(list),
            labelled_by: None,
            active_descendant: active_row,
        },
    );
    world.insert(
        field,
        Semantics {
            name: Some(name),
            description: "Type to filter. Arrows move. Enter activates. Escape dismisses.".into(),
            readonly: false,
        },
    );
    world.insert(
        list,
        SelectionAccess {
            role: AccessRole::ListBox,
            checked: None,
            expanded: None,
            multiselect: false,
            controls: None,
            labelled_by: Some(field),
            active_descendant: None,
        },
    );
}

fn compose_row(world: &mut World, slot: crate::collections::RowSlot, item: &Item) {
    let Some(list) = world
        .get::<rndr_core::Parent>(slot.root)
        .map(|parent| parent.entity)
    else {
        return;
    };
    let Some(root) = world.get::<PaletteLink>(list).map(|link| link.0) else {
        return;
    };
    let Some(shown) = world
        .get::<PaletteState>(root)
        .and_then(|state| state.shown.get(&item.id).cloned())
    else {
        return;
    };
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    if let Some(layout) = world.get_mut::<Layout>(slot.content) {
        layout.axis = rndr_core::Axis::Row;
        layout.gap = 8.0;
        layout.padding = 0.0;
        layout.width = Size::Fill;
        layout.height = Size::Fill;
    }
    world.insert(
        slot.content,
        Align {
            justify: Justify::Start,
            items: Items::Center,
        },
    );
    let parts = ensure_parts(world, slot.content);
    if shown.section {
        world.insert(parts.lead, Hidden);
        world.insert(parts.title, Hidden);
        world.insert(parts.shortcut, Hidden);
        world.remove::<Hidden>(parts.header);
        if let Some(label) = world.get_mut::<Label>(parts.header) {
            label.text = shown.title;
        }
        world.insert(
            slot.root,
            UiStyle {
                background: Some(Token::Overlay.into()),
                foreground: Some(Token::Secondary.into()),
                ..UiStyle::default()
            },
        );
        world.insert(slot.root, section_access());
        project_access(world, root);
        return;
    }
    world.insert(parts.header, Hidden);
    world.remove::<UiStyle>(slot.root);
    if let Some(icon) = shown.icon {
        world.remove::<Hidden>(parts.lead);
        world.remove::<Hidden>(parts.icon);
        if let Some(label) = world.get_mut::<Label>(parts.icon) {
            label.text = icon.scalar().to_string();
        }
    } else {
        world.insert(parts.lead, Hidden);
        world.insert(parts.icon, Hidden);
    }
    world.remove::<Hidden>(parts.title);
    if let Some(label) = world.get_mut::<Label>(parts.title) {
        label.text = shown.title.clone();
    }
    if shown.shortcut.is_empty() {
        world.insert(parts.shortcut, Hidden);
    } else {
        world.remove::<Hidden>(parts.shortcut);
        if let Some(label) = world.get_mut::<Label>(parts.shortcut) {
            label.text = shown.shortcut.clone();
        }
    }
    world.insert(
        parts.shortcut,
        UiStyle {
            foreground: Some(Token::Secondary.into()),
            typography: Some(Typography::Secondary),
            ..UiStyle::default()
        },
    );
    if shown.ranges.is_empty() {
        world.remove::<Spans>(parts.title);
    } else {
        let ink = theme.color(Token::Text);
        let mut mark = theme.color(Token::InfoFill);
        mark.a = 0.9;
        world.insert(
            parts.title,
            Spans {
                runs: shown
                    .ranges
                    .iter()
                    .map(|(start, end)| TextSpan::new(*start, *end, ink).highlight(mark))
                    .collect(),
            },
        );
    }
    world.insert(
        slot.root,
        Semantics {
            name: Some(shown.title),
            description: shown.shortcut,
            readonly: false,
        },
    );
    world.insert(
        slot.root,
        SelectionAccess {
            role: AccessRole::Option,
            checked: None,
            expanded: None,
            multiselect: false,
            controls: None,
            labelled_by: None,
            active_descendant: None,
        },
    );
    project_access(world, root);
}

fn section_access() -> SelectionAccess {
    SelectionAccess {
        role: AccessRole::Group,
        checked: None,
        expanded: None,
        multiselect: false,
        controls: None,
        labelled_by: None,
        active_descendant: None,
    }
}

fn ensure_parts(world: &mut World, content: Entity) -> Parts {
    if let Some(parts) = world.get::<Parts>(content).copied() {
        return parts;
    }
    let lead = world.spawn((Layout {
        axis: rndr_core::Axis::Row,
        width: Size::Px(TEXT_ORIGIN_X + ICON_PX),
        height: Size::Px(ICON_PX),
        ..Layout::row()
    },));
    let spacer = world.spawn((Layout {
        width: Size::Px(TEXT_ORIGIN_X),
        height: Size::Px(1.0),
        ..Layout::row()
    },));
    let icon = world.spawn((
        Layout {
            width: Size::Px(ICON_PX),
            height: Size::Px(ICON_PX),
            ..Layout::row()
        },
        Label::new(""),
    ));
    rndr_core::set_parent(world, spacer, lead);
    rndr_core::set_parent(world, icon, lead);
    rndr_core::set_parent(world, lead, content);
    let title = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::row()
        },
        Label::new("").overflow(TextOverflow::Ellipsis),
    ));
    rndr_core::set_parent(world, title, content);
    let shortcut = world.spawn((
        Layout {
            width: Size::Hug,
            height: Size::Fill,
            ..Layout::row()
        },
        Label::new("").overflow(TextOverflow::Ellipsis),
    ));
    rndr_core::set_parent(world, shortcut, content);
    let header = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::row()
        },
        Label::new("").overflow(TextOverflow::Ellipsis),
        UiStyle {
            foreground: Some(Token::Secondary.into()),
            typography: Some(Typography::Secondary),
            ..UiStyle::default()
        },
        Hidden,
    ));
    rndr_core::set_parent(world, header, content);
    let parts = Parts {
        lead,
        icon,
        title,
        shortcut,
        header,
    };
    world.insert(content, parts);
    parts
}

fn on_key(world: &mut World, field: Entity, key: &KeyEvent) -> bool {
    let Some(root) = palette_for_field(world, field) else {
        return false;
    };
    if world
        .get::<PaletteState>(root)
        .is_some_and(|state| state.suspended)
    {
        return false;
    }
    let Some(list) = world.get::<PaletteState>(root).map(|state| state.list) else {
        return false;
    };
    let handled = VirtualViewHandle { root: list }.forward_key(world, key);
    if handled && plain_enter(world, key) {
        if let Some(id) = (VirtualViewHandle { root: list })
            .active(world)
            .filter(|id| !is_section(*id))
        {
            rndr_core::send(world, PaletteActivated { palette: root, id });
        }
    }
    handled
}

fn plain_enter(world: &World, key: &KeyEvent) -> bool {
    let LogicalKey::Named(NamedKey::Enter) = key.logical else {
        return false;
    };
    if key.repeat
        || key.released
        || key.synthetic
        || key.modifiers.command()
        || key.modifiers.alt
        || key.modifiers.shift
    {
        return false;
    }
    world.resource::<Input>().is_none_or(|input| {
        !input.ime_enabled && input.ime_preedit.is_empty() && input.ime_commit.is_empty()
    })
}

fn escape_allows_close(world: &mut World, root: Entity) -> bool {
    !world
        .get::<PaletteState>(root)
        .is_some_and(|state| state.defer_escape)
}

fn on_closed(world: &mut World, root: Entity) {
    let focus = world
        .resource::<rndr_core::Focus>()
        .and_then(|focus| focus.entity);
    rndr_core::send(
        world,
        PaletteDismissed {
            palette: root,
            focus,
        },
    );
    project_access(world, root);
}

/// Subsequence match used when the host does not install its own matcher.
///
/// Lower score is better. Case and diacritics fold. An exact accent match
/// sorts ahead of the same letters with the accent folded off. Ranges are
/// byte spans in `title`, merged when they sit next to each other. An empty
/// or all-whitespace query matches everything and highlights nothing.
pub fn fuzzy_match(query: &str, title: &str) -> Option<PaletteMatch> {
    let query = query.trim();
    if query.is_empty() {
        return Some(PaletteMatch {
            score: 0,
            ranges: Vec::new(),
        });
    }
    let (pieces, needles) = query_needles(query);
    if needles.is_empty() {
        return Some(PaletteMatch {
            score: 0,
            ranges: Vec::new(),
        });
    }
    let mut score = 0i32;
    let mut last: Option<usize> = None;
    let mut ranges = Vec::new();
    let mut needle = 0usize;
    for atom in atoms(title) {
        if needle >= needles.len() {
            break;
        }
        let grapheme = &title[atom.start..atom.end];
        let mut used = false;
        let mut inexact = false;
        for base in &atom.bases {
            if needle >= needles.len() {
                break;
            }
            if *base == needles[needle].base {
                let at = atom.start;
                score += match last {
                    Some(prev) => (at as i32 - prev as i32 - 1).max(0),
                    None => at as i32,
                };
                last = Some(at);
                if !same_grapheme(&pieces[needles[needle].piece], grapheme) {
                    inexact = true;
                }
                needle += 1;
                used = true;
            }
        }
        if !used {
            continue;
        }
        if inexact {
            score += ACCENT_PENALTY;
        }
        if let Some((_, end)) = ranges.last_mut() {
            if *end == atom.start {
                *end = atom.end;
                continue;
            }
        }
        ranges.push((atom.start, atom.end));
    }
    (needle == needles.len()).then_some(PaletteMatch { score, ranges })
}

/// Cost of matching a base only after folding an accent away.
const ACCENT_PENALTY: i32 = 1_000;

struct Atom {
    start: usize,
    end: usize,
    bases: Vec<char>,
}

struct Needle {
    base: char,
    piece: usize,
}

fn query_needles(query: &str) -> (Vec<String>, Vec<Needle>) {
    let mut pieces = Vec::new();
    let mut needles = Vec::new();
    for atom in atoms(query) {
        let grapheme = &query[atom.start..atom.end];
        if grapheme.chars().all(char::is_whitespace) {
            continue;
        }
        let piece = pieces.len();
        pieces.push(grapheme.to_string());
        for base in atom.bases {
            needles.push(Needle { base, piece });
        }
    }
    (pieces, needles)
}

fn same_grapheme(query: &str, title: &str) -> bool {
    fold_grapheme(query) == fold_grapheme(title)
}

fn fold_grapheme(text: &str) -> String {
    text.nfd().flat_map(char::to_lowercase).collect()
}

fn atoms(text: &str) -> Vec<Atom> {
    let mut atoms: Vec<Atom> = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if is_mark(ch) {
            if let Some(prev) = atoms.last_mut() {
                prev.end = start + ch.len_utf8();
            }
            continue;
        }
        let mut end = start + ch.len_utf8();
        while let Some(&(at, next)) = chars.peek() {
            if !is_mark(next) {
                break;
            }
            chars.next();
            end = at + next.len_utf8();
        }
        atoms.push(Atom {
            start,
            end,
            bases: fold_bases(ch),
        });
    }
    atoms
}

fn fold_bases(ch: char) -> Vec<char> {
    let lower: String = ch.to_lowercase().collect();
    lower.nfd().filter(|ch| !is_mark(*ch)).collect()
}

fn is_mark(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
            | '\u{3099}'..='\u{309A}'
    )
}

#[cfg(test)]
mod fuzzy_tests {
    use super::fuzzy_match;

    #[test]
    fn fuzzy_records_the_matched_letters() {
        let hit = fuzzy_match("sm", "Search memory").unwrap();
        assert_eq!(hit.ranges, vec![(0, 1), (7, 8)]);
        assert_eq!(hit.score, 6);
        assert_eq!(
            fuzzy_match("se", "Search memory").unwrap().ranges,
            vec![(0, 2)]
        );
        assert!(fuzzy_match("zzz", "Search memory").is_none());
        assert!(fuzzy_match("  ", "Search memory")
            .unwrap()
            .ranges
            .is_empty());
        let accent = fuzzy_match("é", "Résumé builder").unwrap();
        assert_eq!(accent.ranges, vec![(1, 3)]);
        assert_eq!(accent.score, 1);
        let upper = fuzzy_match("É", "Résumé builder").unwrap();
        assert_eq!(upper.ranges, vec![(1, 3)]);
        assert_eq!(upper.score, 1);
        let exact = fuzzy_match("rés", "Résumé builder").unwrap();
        assert_eq!(exact.ranges, vec![(0, 4)]);
        let folded = fuzzy_match("rés", "Restart daemon").unwrap();
        assert!(exact.score < folded.score);
        assert_eq!(
            fuzzy_match("cafe", "Café").unwrap().ranges,
            vec![(0, "Café".len())]
        );
        assert_eq!(
            fuzzy_match("resume", "résumé").unwrap().ranges,
            vec![(0, "résumé".len())]
        );
        assert_eq!(
            fuzzy_match("CAFE", "café menu").unwrap().ranges,
            vec![(0, "café".len())]
        );
        let nfd = "Cafe\u{0301}";
        assert_eq!(
            fuzzy_match("cafe", nfd).unwrap().ranges,
            vec![(0, nfd.len())]
        );
        assert_eq!(
            fuzzy_match("e\u{0301}", "Résumé builder").unwrap().ranges,
            vec![(1, 3)]
        );
        let decomposed = "re\u{0301}sume\u{0301}";
        assert_eq!(
            fuzzy_match("resume", decomposed).unwrap().ranges,
            vec![(0, decomposed.len())]
        );
        assert!(fuzzy_match("cafe", "cat").is_none());
        assert!(fuzzy_match("se", "Quit").is_none());
    }
}
