//! Opt-in style ownership. Only attached UiStyle entities are resolved.
use crate::tokens::*;
use rndr_core::*;

/// Explicit colors, including transparent, are distinct from semantic tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiColor {
    /// Semantic theme lookup.
    Token(Token),
    /// Authored sRGB/straight-alpha override, no contrast guarantee.
    Explicit(Color),
}
impl From<Token> for UiColor {
    fn from(token: Token) -> Self {
        Self::Token(token)
    }
}
impl UiColor {
    /// Resolve without substituting explicit transparent colors.
    pub fn resolve(self, theme: &UiTheme) -> Color {
        match self {
            Self::Token(t) => theme.color(t),
            Self::Explicit(c) => c,
        }
    }
}
/// Semantic corner sizes, in logical points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    /// Square.
    Square,
    /// Compact 4.
    Compact,
    /// Control 6.
    Control,
    /// Content group 10.
    Group,
    /// Overlay 12.
    Overlay,
}
impl Corner {
    /// Logical points.
    pub fn points(self) -> f32 {
        match self {
            Self::Square => 0.0,
            Self::Compact => 4.0,
            Self::Control => 6.0,
            Self::Group => 10.0,
            Self::Overlay => 12.0,
        }
    }
}
/// Semantic recipe on existing primitives. Authored primitive values win.
/// Use Explicit within this component for an override of a semantic recipe.
/// Removing UiStyle releases/restores only still-owned values on the next Layout.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiStyle {
    /// Optional background (absence is not transparent).
    pub background: Option<UiColor>,
    /// Optional Label foreground.
    pub foreground: Option<UiColor>,
    /// Optional font/type role.
    pub typography: Option<Typography>,
    /// Optional padding; pre-existing nonzero padding wins.
    pub padding: Option<Space>,
    /// Optional gap; pre-existing nonzero gap wins.
    pub gap: Option<Space>,
    /// Optional corner radius; pre-existing Radius wins.
    pub corner: Option<Corner>,
}
impl Component for UiStyle {}

/// Lower-precedence component recipe. App UiStyle overrides survive recipe updates.
#[derive(Clone, Debug, Default)]
pub struct UiRecipe(pub UiStyle);
impl Component for UiRecipe {}
impl UiStyle {
    /// Readable source/editor surface on core's existing Editor + Label.
    /// Only styling is owned here: buffer, caret, input, selection and history
    /// remain with core. Native IME/selection appearance still needs OS review.
    pub fn editor() -> Self {
        Self {
            background: Some(Token::Content.into()),
            foreground: Some(Token::Text.into()),
            typography: Some(Typography::Code),
            padding: Some(Space::X8),
            corner: Some(Corner::Control),
            ..Self::default()
        }
    }

    fn over(self, recipe: Self) -> Self {
        Self {
            background: self.background.or(recipe.background),
            foreground: self.foreground.or(recipe.foreground),
            typography: self.typography.or(recipe.typography),
            padding: self.padding.or(recipe.padding),
            gap: self.gap.or(recipe.gap),
            corner: self.corner.or(recipe.corner),
        }
    }
}

#[derive(Clone)]
struct Slot<T> {
    baseline: Option<T>,
    last: Option<T>,
    blocked: bool,
    observed: bool,
}
impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self {
            baseline: None,
            last: None,
            blocked: false,
            observed: false,
        }
    }
}
impl<T: Clone + PartialEq> Slot<T> {
    // None means no write. Some(None) means remove the owned primitive.
    fn resolve(
        &mut self,
        current: Option<T>,
        desired: Option<T>,
        preserve: bool,
    ) -> Option<Option<T>> {
        if desired.is_none() {
            let restore = self
                .last
                .as_ref()
                .is_some_and(|last| current.as_ref() == Some(last));
            let baseline = self.baseline.clone();
            *self = Self::default();
            return if restore { Some(baseline) } else { None };
        }
        if !self.observed {
            self.observed = true;
            self.baseline = current.clone();
            self.blocked = preserve && current.is_some();
        }
        if let Some(last) = &self.last {
            if current.as_ref() != Some(last) {
                self.blocked = true;
                self.last = None;
            }
        }
        if self.blocked {
            return None;
        }
        if let Some(desired) = desired {
            self.last = Some(desired.clone());
            if current.as_ref() == Some(&desired) {
                None
            } else {
                Some(Some(desired))
            }
        } else if self.last.take().is_some() {
            Some(self.baseline.clone())
        } else {
            None
        }
    }
}
#[derive(Clone, Default)]
struct Managed {
    background: Slot<Color>,
    foreground: Slot<Color>,
    text: Slot<TextStyle>,
    padding: Slot<f32>,
    gap: Slot<f32>,
    corner: Slot<f32>,
}
impl Component for Managed {}

/// Preview the same background ownership/cascade used by `resolve_styles`,
/// without mutation. Controls resolve before styles; host-plane inheritance
/// must account for same-update themes *and* explicit Color takeovers.
pub(crate) fn background_at(
    world: &World,
    entity: rndr_core::Entity,
    theme: &UiTheme,
) -> Option<Color> {
    let desired = world
        .get::<UiStyle>(entity)
        .and_then(|s| s.background)
        .or_else(|| world.get::<UiRecipe>(entity).and_then(|s| s.0.background))
        .map(|c| c.resolve(theme));
    let current = world.get::<Color>(entity).copied();
    let mut slot = world
        .get::<Managed>(entity)
        .map(|m| m.background.clone())
        .unwrap_or_default();
    slot.resolve(current, desired, true).unwrap_or(current)
}

/// Resolve all owned properties before hierarchy measurement and paint.
/// Repeated installation adds no systems; arbitrary app-owned entities are untouched.
pub fn resolve_styles(world: &mut World) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let mut entities = world.entities_with::<UiStyle>();
    for entity in world.entities_with::<UiRecipe>() {
        if !entities.contains(&entity) {
            entities.push(entity);
        }
    }
    for entity in world.entities_with::<Managed>() {
        if !entities.contains(&entity) {
            entities.push(entity);
        }
    }
    for entity in entities {
        let attached = world.has::<UiStyle>(entity) || world.has::<UiRecipe>(entity);
        let recipe = world
            .get::<UiRecipe>(entity)
            .map(|r| r.0.clone())
            .unwrap_or_default();
        let style = world
            .get::<UiStyle>(entity)
            .cloned()
            .unwrap_or_default()
            .over(recipe);
        let mut managed = world.get::<Managed>(entity).cloned().unwrap_or_default();
        if let Some(value) = managed.background.resolve(
            world.get::<Color>(entity).copied(),
            style.background.map(|v| v.resolve(&theme)),
            true,
        ) {
            if let Some(value) = value {
                world.insert(entity, value);
            } else {
                world.remove::<Color>(entity);
            }
        }
        if world.has::<Label>(entity) {
            if let Some(value) = managed.foreground.resolve(
                world.get::<Label>(entity).and_then(|l| l.color),
                style.foreground.map(|v| v.resolve(&theme)),
                true,
            ) {
                world.get_mut::<Label>(entity).unwrap().color = value;
            }
        }
        if let Some(value) = managed.text.resolve(
            world.get::<TextStyle>(entity).cloned(),
            style.typography.map(|t| theme.typography(t)),
            true,
        ) {
            if let Some(value) = value {
                world.insert(entity, value);
            } else {
                world.remove::<TextStyle>(entity);
            }
        }
        if let Some(layout) = world.get::<Layout>(entity).copied() {
            let padding = if layout.padding == 0.0 && managed.padding.last.is_none() {
                None
            } else {
                Some(layout.padding)
            };
            if let Some(value) =
                managed
                    .padding
                    .resolve(padding, style.padding.map(|s| theme.space(s)), true)
            {
                world.get_mut::<Layout>(entity).unwrap().padding = value.unwrap_or(0.0);
            }
            let gap = if layout.gap == 0.0 && managed.gap.last.is_none() {
                None
            } else {
                Some(layout.gap)
            };
            if let Some(value) = managed
                .gap
                .resolve(gap, style.gap.map(|s| theme.space(s)), true)
            {
                world.get_mut::<Layout>(entity).unwrap().gap = value.unwrap_or(0.0);
            }
        }
        if let Some(value) = managed.corner.resolve(
            world.get::<Radius>(entity).map(|r| r.0),
            style.corner.map(|c| c.points()),
            true,
        ) {
            if let Some(value) = value {
                world.insert(entity, Radius(value));
            } else {
                world.remove::<Radius>(entity);
            }
        }
        if attached {
            world.insert(entity, managed);
        } else {
            world.remove::<Managed>(entity);
        }
    }
}

/// Detach the semantic recipe and release owned primitives immediately.
/// Caller takeovers are never rolled back. Does not remove layout/label/content.
pub fn detach_style(world: &mut World, entity: Entity) {
    world.remove::<UiStyle>(entity);
    world.remove::<UiRecipe>(entity);
    // Resolve this entity only by releasing all properties, without re-theming others.
    let Some(mut m) = world.get::<Managed>(entity).cloned() else {
        return;
    };
    if let Some(v) = m
        .background
        .resolve(world.get::<Color>(entity).copied(), None, true)
    {
        if let Some(v) = v {
            world.insert(entity, v);
        } else {
            world.remove::<Color>(entity);
        }
    }
    if let Some(v) =
        m.foreground
            .resolve(world.get::<Label>(entity).and_then(|l| l.color), None, true)
    {
        if let Some(l) = world.get_mut::<Label>(entity) {
            l.color = v;
        }
    }
    if let Some(v) = m
        .text
        .resolve(world.get::<TextStyle>(entity).cloned(), None, true)
    {
        if let Some(v) = v {
            world.insert(entity, v);
        } else {
            world.remove::<TextStyle>(entity);
        }
    }
    if let Some(v) = m
        .corner
        .resolve(world.get::<Radius>(entity).map(|r| r.0), None, true)
    {
        if let Some(v) = v {
            world.insert(entity, Radius(v));
        } else {
            world.remove::<Radius>(entity);
        }
    }
    if let Some(layout) = world.get::<Layout>(entity).copied() {
        if let Some(v) = m.padding.resolve(Some(layout.padding), None, true) {
            world.get_mut::<Layout>(entity).unwrap().padding = v.unwrap_or(0.0);
        }
        if let Some(v) = m.gap.resolve(Some(layout.gap), None, true) {
            world.get_mut::<Layout>(entity).unwrap().gap = v.unwrap_or(0.0);
        }
    }
    world.remove::<Managed>(entity);
}
