//! Optional source tools built on canonical core fields and command controls.
use crate::controls::{host_intent, Button, ButtonProps, HostIntent};
use crate::{UiError, UiId};
use rndr_core::editing::{
    attached_source, source_attachment_epoch, DocumentId, EditError, SourceDocuments,
};
use rndr_core::source_search::SourceSearch;
use rndr_core::*;

/// Optional literal find/replace bar. Install [`apply_find_bars`] in Update.
/// It owns no document, input dispatcher or focus loop. Destroy its root to tear down.
#[derive(Clone, Copy, Debug)]
pub struct FindBar {
    /// Owned toolbar subtree.
    pub root: Entity,
    /// Canonical search TextField.
    pub query: Entity,
    /// Canonical replacement TextField.
    pub replacement: Entity,
    /// Accessible result status.
    pub status: Entity,
    editor: Entity,
    document: DocumentId,
    attachment: u64,
    next: Entity,
    previous: Entity,
    replace: Entity,
    all: Entity,
}
impl Component for FindBar {}
#[derive(Clone)]
struct FindState(Option<SourceSearch>);
impl Component for FindState {}
impl FindBar {
    /// Mount a separate optional toolbar for an attached source editor.
    /// The target identity is captured; detach/rebind disables commands, never retargets them.
    pub fn mount(world: &mut World, parent: Entity, editor: Entity) -> Result<Self, UiError> {
        if !world.is_alive(parent) || !world.has::<Layout>(parent) {
            return Err(UiError::Stale);
        }
        let document = attached_source(world, editor).ok_or(UiError::InvalidProps)?;
        let root = world.spawn((Layout {
            width: Size::Fill,
            gap: 4.0,
            ..Layout::column()
        },));
        set_parent(world, root, parent);
        let fields = world.spawn((Layout {
            width: Size::Fill,
            gap: 6.0,
            ..Layout::row()
        },));
        set_parent(world, fields, root);
        let commands = world.spawn((Layout {
            width: Size::Fill,
            gap: 4.0,
            ..Layout::row()
        },));
        set_parent(world, commands, root);
        fn field(world: &mut World, root: Entity, name: &str) -> Entity {
            let group = world.spawn((Layout {
                width: Size::Fill,
                gap: 2.0,
                ..Layout::column()
            },));
            set_parent(world, group, root);
            let caption = world.spawn((Layout::column(), Label::new(name)));
            world.insert(
                caption,
                crate::UiStyle {
                    typography: Some(crate::Typography::Secondary),
                    ..Default::default()
                },
            );
            set_parent(world, caption, group);
            let e = world.spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Px(30.0),
                    ..Layout::column()
                },
                Label::new(""),
                TextField,
                Focusable,
                CapturesKeys,
                Semantics {
                    name: Some(name.into()),
                    ..Default::default()
                },
            ));
            world.insert(e, crate::UiStyle::editor());
            set_parent(world, e, group);
            e
        }
        let query = field(world, fields, "Find literal text");
        let replacement = field(world, fields, "Replacement text");
        let previous = Button::mount(
            world,
            commands,
            UiId(1),
            ButtonProps::new("Previous", HostIntent::Command(1)),
        )?
        .root;
        let next = Button::mount(
            world,
            commands,
            UiId(2),
            ButtonProps::new("Next", HostIntent::Command(2)),
        )?
        .root;
        let replace = Button::mount(
            world,
            commands,
            UiId(3),
            ButtonProps::new("Replace", HostIntent::Command(3)),
        )?
        .root;
        let all = Button::mount(
            world,
            commands,
            UiId(4),
            ButtonProps::new("Replace all", HostIntent::Command(4)),
        )?
        .root;
        let status = world.spawn((
            Layout::column(),
            Label::new("Literal search"),
            Semantics {
                name: Some("Search results".into()),
                ..Default::default()
            },
        ));
        set_parent(world, status, root);
        let bar = Self {
            root,
            query,
            replacement,
            status,
            editor,
            document,
            attachment: source_attachment_epoch(world, editor).ok_or(UiError::Stale)?,
            next,
            previous,
            replace,
            all,
        };
        world.insert(root, bar);
        world.insert(root, FindState(None));
        Ok(bar)
    }
}
/// Process only this toolbar's FIFO commands; unrelated host commands are untouched.
/// Query/revision changes refresh literal results. A click never changes source identity.
pub fn apply_find_bars(world: &mut World) {
    let bars: Vec<FindBar> = world
        .entities_with::<FindBar>()
        .into_iter()
        .filter_map(|e| world.get::<FindBar>(e).copied())
        .collect();
    for bar in bars {
        // Drain this toolbar's commands even after detach. Never retain a stale click
        // for a later reattachment of the same document.
        let intents = take_action_intents_matching(world, |_, i| {
            [bar.next, bar.previous, bar.replace, bar.all].contains(&i.entity)
        });
        if attached_source(world, bar.editor) != Some(bar.document)
            || source_attachment_epoch(world, bar.editor) != Some(bar.attachment)
        {
            if let Some(label) = world.get_mut::<Label>(bar.status) {
                label.text = "Source detached".into();
            }
            continue;
        }
        let query = world
            .get::<Label>(bar.query)
            .map(|l| l.text.clone())
            .unwrap_or_default();
        let replacement = world
            .get::<Label>(bar.replacement)
            .map(|l| l.text.clone())
            .unwrap_or_default();
        let revision = world
            .resource::<SourceDocuments>()
            .and_then(|d| d.get(bar.document))
            .map(|s| s.revision());
        let mut search = world.get::<FindState>(bar.root).and_then(|s| s.0.clone());
        if search
            .as_ref()
            .is_none_or(|s| s.query() != query || Some(s.revision()) != revision)
        {
            search = world
                .resource::<SourceDocuments>()
                .and_then(|d| SourceSearch::new(d, bar.document, query).ok());
        }
        let mut error: Option<EditError> = None;
        if let Some(search) = search.as_mut() {
            for intent in intents {
                let Some(HostIntent::Command(command)) = host_intent(world, &intent) else {
                    continue;
                };
                let policy = access_details(world, bar.editor);
                if !policy.focus || (matches!(command, 3 | 4) && policy.readonly) {
                    error = Some(EditError::Readonly);
                    continue;
                }
                if let Some(docs) = world.resource_mut::<SourceDocuments>() {
                    let result = match command {
                        1 | 2 => search.navigate(docs, bar.editor, command == 1).map(|_| ()),
                        3 => search.replace_current(docs, &replacement).map(|_| ()),
                        4 => search.replace_all(docs, &replacement).map(|_| ()),
                        _ => Ok(()),
                    };
                    if let Err(e) = result {
                        error = Some(e);
                    }
                }
            }
        }
        let message = if let Some(e) = error {
            format!("Search: {e}")
        } else {
            format!(
                "{} matches",
                search.as_ref().map_or(0, |s| s.matches().len())
            )
        };
        if let Some(label) = world.get_mut::<Label>(bar.status) {
            label.text = message;
        }
        world.insert(bar.root, FindState(search));
    }
}
