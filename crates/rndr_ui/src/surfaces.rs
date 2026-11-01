//! Passive compositions with stable caller identity and explicit subtree ownership.
use crate::{Corner, Space, Token, Typography, UiRecipe, UiStyle};
use rndr_core::*;

/// Caller-stable key, unique under a parent. No global registry or second tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiId(pub u64);
/// Semantic passive surface plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// Canvas.
    Canvas,
    /// Navigation.
    Navigation,
    /// Reading.
    Content,
    /// Grouped content.
    Raised,
    /// Opaque overlay plane (not a modal/dialog widget).
    Overlay,
}
impl Surface {
    fn token(self) -> Token {
        match self {
            Self::Canvas => Token::Canvas,
            Self::Navigation => Token::Navigation,
            Self::Content => Token::Content,
            Self::Raised => Token::Raised,
            Self::Overlay => Token::Overlay,
        }
    }
}
/// Callout meaning, always exposed in text in addition to color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feedback {
    /// Informational.
    Info,
    /// Successful outcome.
    Success,
    /// Warning.
    Warning,
    /// Danger.
    Danger,
}
impl Feedback {
    fn palette(self) -> (Token, Token) {
        match self {
            Self::Info => (Token::InfoFill, Token::InfoText),
            Self::Success => (Token::SuccessFill, Token::SuccessText),
            Self::Warning => (Token::WarningFill, Token::WarningText),
            Self::Danger => (Token::DangerFill, Token::DangerText),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Success => "Success",
            Self::Warning => "Warning",
            Self::Danger => "Danger",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Surface(Surface),
    Card,
    Callout(Feedback),
}
#[derive(Clone, Copy)]
struct Node {
    id: UiId,
    kind: Kind,
    content: Entity,
    title: Entity,
}
impl Component for Node {}
#[derive(Clone, Copy)]
struct Owned {
    root: Entity,
}
impl Component for Owned {}

/// Generation-checked roots/content slots on the existing World.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiHandle {
    /// Stable library root.
    pub root: Entity,
    /// Stable content slot. Directly attached caller children remain borrowed.
    pub content: Entity,
}
/// Lifecycle operation error; no partial update is performed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiError {
    /// Destroyed or malformed handle/parent.
    Stale,
    /// Key already used for a different component kind.
    IdentityConflict,
    /// Invalid adoption (cycles, existing UI component or stale entity).
    InvalidAdoption,
}

/// Passive content Card, not a clickable control.
pub struct Card;
/// Passive semantic message; no implicit actions or network behavior.
pub struct Callout;
impl Surface {
    /// Mount or update in place under a live Layout parent and stable key.
    pub fn mount(self, world: &mut World, parent: Entity, id: UiId) -> Result<UiHandle, UiError> {
        mount(world, parent, id, Kind::Surface(self), "")
    }
}
impl Card {
    /// Mount/update title in place. Content and caller state survive updates.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        title: &str,
    ) -> Result<UiHandle, UiError> {
        mount(world, parent, id, Kind::Card, title)
    }
}
impl Callout {
    /// Mount/update message title with a persistent textual meaning prefix.
    pub fn mount(
        world: &mut World,
        parent: Entity,
        id: UiId,
        meaning: Feedback,
        title: &str,
    ) -> Result<UiHandle, UiError> {
        mount(world, parent, id, Kind::Callout(meaning), title)
    }
}
fn same_family(a: Kind, b: Kind) -> bool {
    matches!(
        (a, b),
        (Kind::Card, Kind::Card)
            | (Kind::Surface(_), Kind::Surface(_))
            | (Kind::Callout(_), Kind::Callout(_))
    )
}
fn mount(
    world: &mut World,
    parent: Entity,
    id: UiId,
    kind: Kind,
    title: &str,
) -> Result<UiHandle, UiError> {
    if !world.is_alive(parent) || !world.has::<Layout>(parent) {
        return Err(UiError::Stale);
    }
    let existing = children_of(world, parent)
        .into_iter()
        .find(|e| world.get::<Node>(*e).is_some_and(|n| n.id == id));
    let handle = if let Some(root) = existing {
        let node = *world.get::<Node>(root).unwrap();
        if !same_family(node.kind, kind) {
            return Err(UiError::IdentityConflict);
        }
        let handle = UiHandle {
            root,
            content: node.content,
        };
        handle.validate(world)?;
        handle
    } else {
        let root = world.spawn((Layout {
            width: Size::Fill,
            ..Layout::column()
        },));
        set_parent(world, root, parent);
        let title = world.spawn((
            Layout {
                width: Size::Fill,
                ..Layout::column()
            },
            Label::new("").overflow(TextOverflow::Wrap),
            Owned { root },
        ));
        set_parent(world, title, root);
        let content = world.spawn((
            Layout {
                width: Size::Fill,
                ..Layout::column()
            },
            Owned { root },
        ));
        set_parent(world, content, root);
        world.insert(root, Owned { root });
        world.insert(
            root,
            Node {
                id,
                kind,
                content,
                title,
            },
        );
        UiHandle { root, content }
    };
    let node = *world.get::<Node>(handle.root).unwrap();
    world.get_mut::<Node>(handle.root).unwrap().kind = kind;
    let (background, foreground) = match kind {
        Kind::Surface(surface) => (surface.token(), Token::Text),
        Kind::Card => (Token::Raised, Token::Text),
        Kind::Callout(feedback) => feedback.palette(),
    };
    world.insert(
        handle.root,
        UiRecipe(UiStyle {
            background: Some(background.into()),
            padding: Some(Space::X16),
            gap: Some(Space::X8),
            corner: Some(Corner::Group),
            ..UiStyle::default()
        }),
    );
    let text = match kind {
        Kind::Callout(feedback) => format!("{}: {}", feedback.name(), title),
        _ => title.to_owned(),
    };
    world
        .get_mut::<Label>(node.title)
        .ok_or(UiError::Stale)?
        .text = text;
    if matches!(kind, Kind::Surface(_)) || title.is_empty() && kind == Kind::Card {
        world.insert(node.title, Hidden);
    } else {
        world.remove::<Hidden>(node.title);
    }
    world.insert(
        node.title,
        UiRecipe(UiStyle {
            foreground: Some(foreground.into()),
            typography: Some(Typography::Heading),
            ..UiStyle::default()
        }),
    );
    world.insert(
        handle.content,
        UiRecipe(UiStyle {
            gap: Some(Space::X8),
            ..UiStyle::default()
        }),
    );
    Ok(handle)
}
impl UiHandle {
    /// Validate generations, content slot and ownership before any update.
    pub fn validate(self, world: &World) -> Result<(), UiError> {
        let n = world.get::<Node>(self.root).ok_or(UiError::Stale)?;
        if n.content != self.content
            || !world.is_alive(self.content)
            || !world.is_alive(n.title)
            || world.get::<Owned>(self.content).map(|o| o.root) != Some(self.root)
            || world.get::<Parent>(self.content).map(|p| p.entity) != Some(self.root)
            || world.get::<Parent>(n.title).map(|p| p.entity) != Some(self.root)
        {
            return Err(UiError::Stale);
        }
        Ok(())
    }
    /// Update title without replacing entity, focus, scroll, or caller content.
    pub fn set_title(self, world: &mut World, title: &str) -> Result<(), UiError> {
        self.validate(world)?;
        let node = *world.get::<Node>(self.root).unwrap();
        let text = match node.kind {
            Kind::Callout(feedback) => format!("{}: {}", feedback.name(), title),
            _ => title.to_owned(),
        };
        world
            .get_mut::<Label>(node.title)
            .ok_or(UiError::Stale)?
            .text = text;
        if matches!(node.kind, Kind::Surface(_)) || title.is_empty() && node.kind == Kind::Card {
            world.insert(node.title, Hidden);
        } else {
            world.remove::<Hidden>(node.title);
        }
        Ok(())
    }
    /// Explicitly transfer a plain subtree into library lifetime ownership.
    /// Direct set_parent alone is borrowing; destroy detaches borrowed content.
    /// Existing UI components cannot be adopted; compose them as borrowed handles.
    pub fn adopt(self, world: &mut World, entity: Entity) -> Result<(), UiError> {
        self.validate(world)?;
        fn gather(world: &World, entity: Entity, out: &mut Vec<Entity>) -> bool {
            if !world.is_alive(entity)
                || out.contains(&entity)
                || world.has::<Node>(entity)
                || world.has::<Owned>(entity)
            {
                return false;
            }
            out.push(entity);
            for child in children_of(world, entity) {
                if !gather(world, child, out) {
                    return false;
                }
            }
            true
        }
        let mut entities = vec![];
        if !gather(world, entity, &mut entities) {
            return Err(UiError::InvalidAdoption);
        }
        // No ancestor of this component may become its child.
        let mut current = Some(self.root);
        while let Some(e) = current {
            if entities.contains(&e) {
                return Err(UiError::InvalidAdoption);
            }
            current = world.get::<Parent>(e).map(|p| p.entity);
        }
        for entity in entities {
            world.insert(entity, Owned { root: self.root });
        }
        set_parent(world, entity, self.content);
        Ok(())
    }
    /// Destroy only owned nodes. Borrowed children are detached with their whole
    /// subtree alive; their focus/history/style remains caller-owned.
    pub fn destroy(self, world: &mut World) -> Result<(), UiError> {
        self.validate(world)?;
        fn detach_borrowed(
            world: &mut World,
            entity: Entity,
            owner: Entity,
            owned: &mut Vec<Entity>,
        ) {
            owned.push(entity);
            for child in children_of(world, entity) {
                if world.get::<Owned>(child).map(|o| o.root) == Some(owner) {
                    detach_borrowed(world, child, owner, owned);
                } else {
                    world.remove::<Parent>(child);
                    if let Some(children) = world.get_mut::<Children>(entity) {
                        children.list.retain(|e| *e != child);
                    }
                }
            }
        }
        let mut owned = vec![];
        detach_borrowed(world, self.root, self.root, &mut owned);
        rndr_core::forget_interaction_targets(world, &owned);
        despawn(world, self.root);
        Ok(())
    }
}

/// Decorative separator recipe. Not an essential control boundary or focus ring.
/// Caller-owned until explicitly adopted; no tab stop or automatic announcement.
pub fn separator(world: &mut World, parent: Entity) -> Result<Entity, UiError> {
    if !world.is_alive(parent) || !world.has::<Layout>(parent) {
        return Err(UiError::Stale);
    }
    let entity = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(1.0),
            ..Layout::column()
        },
        UiRecipe(UiStyle {
            background: Some(Token::SubtleStroke.into()),
            ..UiStyle::default()
        }),
    ));
    set_parent(world, entity, parent);
    Ok(entity)
}

/// Passive wrapped text. The caller owns this node until explicitly adopted.
pub fn text(
    world: &mut World,
    parent: Entity,
    value: &str,
    role: Typography,
) -> Result<Entity, UiError> {
    if !world.is_alive(parent) || !world.has::<Layout>(parent) {
        return Err(UiError::Stale);
    }
    let entity = world.spawn((
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
        Label::new(value).overflow(TextOverflow::Wrap),
        UiRecipe(UiStyle {
            foreground: Some(Token::Text.into()),
            typography: Some(role),
            ..UiStyle::default()
        }),
    ));
    set_parent(world, entity, parent);
    Ok(entity)
}
