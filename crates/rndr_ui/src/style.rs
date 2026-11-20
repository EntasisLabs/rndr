//! Opt-in style ownership. Only attached UiStyle entities are resolved.
use crate::tokens::*;
use rndr_core::motion::MotionSettings;
use rndr_core::*;
use std::time::Duration;

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
    /// Control face 5: one-point inset from a six-point border.
    ControlFace,
    /// Focus gap 8: two-point inset from a ten-point focus ring.
    FocusGap,
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
            Self::ControlFace => 5.0,
            Self::FocusGap => 8.0,
            Self::Group => 10.0,
            Self::Overlay => 12.0,
        }
    }
}

/// Optional frost for a menu, palette, tooltip, toast, or other popover.
///
/// The default draws neither a backdrop blur nor a shadow. [`Self::frost`]
/// blurs pixels the app has already drawn behind the popup and adds a drop
/// shadow. It does not blur the desktop. That remains [`rndr_core::Backdrop`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OverlayChrome {
    /// Backdrop-blur sigma in logical points. `None`, zero, and non-finite
    /// values draw no blur.
    pub blur: Option<f32>,
    /// Drop shadow under the popup.
    pub shadow: bool,
}

impl OverlayChrome {
    /// Twenty-point backdrop blur and a drop shadow.
    pub fn frost() -> Self {
        Self {
            blur: Some(20.0),
            shadow: true,
        }
    }
}

/// Which corner radius a control should take from the theme.
///
/// The numbers are [`Corner`]'s. [`RadiusScale`] can replace them for one app
/// without inventing a second scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadiusRole {
    /// Buttons and other controls. [`Corner::Control`].
    Control,
    /// Text fields and steppers. [`Corner::Control`] unless a scale says otherwise.
    Input,
    /// Menus, palettes, tooltips, toasts, and other popovers. 12 points.
    Overlay,
    /// Cards and grouped surfaces. [`Corner::Group`].
    Card,
    /// Chips and other compact tags. [`Corner::Compact`].
    Chip,
}

/// Corner radii in logical points. Absent from the world, [`Corner`] is used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadiusScale {
    /// [`RadiusRole::Control`].
    pub control: f32,
    /// [`RadiusRole::Input`].
    pub input: f32,
    /// [`RadiusRole::Overlay`].
    pub overlay: f32,
    /// [`RadiusRole::Card`].
    pub card: f32,
    /// [`RadiusRole::Chip`].
    pub chip: f32,
}

impl Default for RadiusScale {
    fn default() -> Self {
        Self {
            control: Corner::Control.points(),
            input: Corner::Control.points(),
            overlay: Corner::Overlay.points(),
            card: Corner::Group.points(),
            chip: Corner::Compact.points(),
        }
    }
}

impl RadiusScale {
    /// Points for one role. Non-finite values fall back to the built-in corner.
    pub fn of(self, role: RadiusRole) -> f32 {
        let value = match role {
            RadiusRole::Control => self.control,
            RadiusRole::Input => self.input,
            RadiusRole::Overlay => self.overlay,
            RadiusRole::Card => self.card,
            RadiusRole::Chip => self.chip,
        };
        if value.is_finite() && value >= 0.0 {
            value
        } else {
            RadiusScale::default().of(role)
        }
    }

    fn corner(self, corner: Corner, focus_width: f32, input: bool) -> f32 {
        let control = if input {
            self.of(RadiusRole::Input)
        } else {
            self.of(RadiusRole::Control)
        };
        match corner {
            Corner::Square => 0.0,
            Corner::Compact => self.of(RadiusRole::Chip),
            Corner::Control => control,
            Corner::ControlFace => (control - 1.0).max(0.0),
            Corner::FocusGap => (self.of(RadiusRole::Card) - focus_width).max(0.0),
            Corner::Group => self.of(RadiusRole::Card),
            Corner::Overlay => self.of(RadiusRole::Overlay),
        }
    }
}

impl UiTheme {
    /// Radius for `role`, before a [`RadiusScale`] resource replaces it.
    pub fn radius(&self, role: RadiusRole) -> f32 {
        RadiusScale::default().of(role)
    }

    /// Hover and press color ease. Reduced motion snaps this to zero.
    pub fn interaction_duration(&self) -> Duration {
        Duration::from_millis(120)
    }
}

/// Turns on the hover and press color ease. Absent, or with a zero duration,
/// those fills settle on the same frame. [`UiPlugin`] installs the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InteractionMotion {
    /// Ease length. Zero snaps.
    pub duration: Duration,
}

impl Default for InteractionMotion {
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(120),
        }
    }
}

impl InteractionMotion {
    /// Settled color on the frame the state changes.
    pub const fn snap() -> Self {
        Self {
            duration: Duration::ZERO,
        }
    }
}

/// Marks a field whose [`Corner::Control`] radius is [`RadiusRole::Input`].
pub(crate) struct UseInputRadius;
impl Component for UseInputRadius {}

#[derive(Clone, Copy)]
struct FillFade {
    from: Color,
    to: Color,
    start: Duration,
}
impl Component for FillFade {}

struct ManagedRound;
impl Component for ManagedRound {}

/// Keyboard focus ring. Width is [`UiTheme::focus_width`]; ink is [`Token::Focus`].
pub(crate) fn keyboard_focus_ring(theme: &UiTheme) -> Border {
    Border {
        color: theme.color(Token::Focus),
        width: theme.focus_width(),
    }
}

/// Apply or clear [`OverlayChrome`] on `entity`.
pub(crate) fn paint_overlay(world: &mut World, entity: Entity, chrome: OverlayChrome) {
    match chrome.blur {
        Some(sigma) if sigma.is_finite() && sigma > 0.0 => {
            world.insert(entity, BackdropBlur(sigma));
        }
        _ => {
            world.remove::<BackdropBlur>(entity);
        }
    }
    if chrome.shadow {
        let dark = world
            .resource::<UiTheme>()
            .is_some_and(|theme| theme.appearance == Appearance::Dark);
        let alpha = if dark { 0.46 } else { 0.18 };
        world.insert(
            entity,
            Shadows(vec![Shadow::drop(
                12.0,
                28.0,
                Color::rgba(0.0, 0.0, 0.0, alpha),
            )]),
        );
    } else {
        world.remove::<Shadows>(entity);
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
    resolve_entities(world, &theme, entities);
}

// Viewport-born content can be created after the normal Layout style pass, or
// during scroll hit-geometry refresh. Resolve only that bounded subtree using
// the exact same property-ownership rules; never rescan unrelated UI per list.
pub(crate) fn resolve_subtree_styles(world: &mut World, root: Entity) {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let mut entities = Vec::new();
    let mut pending = vec![root];
    while let Some(e) = pending.pop() {
        if world.has::<UiStyle>(e) || world.has::<UiRecipe>(e) || world.has::<Managed>(e) {
            entities.push(e);
        }
        pending.extend(children_of(world, e));
    }
    resolve_entities(world, &theme, entities);
}
fn resolve_entities(world: &mut World, theme: &UiTheme, entities: Vec<Entity>) {
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
        let presented = world.get::<Color>(entity).copied();
        let current = world
            .get::<FillFade>(entity)
            .map(|fade| fade.to)
            .or(presented);
        if let Some(value) =
            managed
                .background
                .resolve(current, style.background.map(|v| v.resolve(theme)), true)
        {
            match value {
                Some(target) => present_fill(world, entity, presented, target),
                None => {
                    world.remove::<FillFade>(entity);
                    world.remove::<Color>(entity);
                }
            }
        } else {
            advance_fill(world, entity);
        }
        if world.has::<Label>(entity) {
            if let Some(value) = managed.foreground.resolve(
                world.get::<Label>(entity).and_then(|l| l.color),
                style.foreground.map(|v| v.resolve(theme)),
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
        let scale = world.resource::<RadiusScale>().copied().unwrap_or_default();
        let input = world.has::<UseInputRadius>(entity);
        if let Some(value) = managed.corner.resolve(
            world.get::<Radius>(entity).map(|r| r.0),
            style
                .corner
                .map(|corner| scale.corner(corner, theme.focus_width(), input)),
            true,
        ) {
            if let Some(value) = value {
                world.insert(entity, Radius(value));
                mask_round(world, entity, value);
            } else {
                world.remove::<Radius>(entity);
                clear_mask(world, entity);
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
        world.remove::<FillFade>(entity);
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
            mask_round(world, entity, v);
        } else {
            world.remove::<Radius>(entity);
            clear_mask(world, entity);
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

fn radii_duration(world: &World) -> Duration {
    world
        .resource::<InteractionMotion>()
        .map(|motion| motion.duration)
        .or_else(|| {
            world
                .resource::<UiTheme>()
                .map(|theme| theme.interaction_duration())
        })
        .unwrap_or_else(|| UiTheme::default().interaction_duration())
}

fn snaps_motion(world: &World) -> bool {
    let reduced = world
        .resource::<MotionSettings>()
        .is_some_and(|settings| settings.reduced_motion)
        || system_appearance(world).reduced_motion;
    let eased = world
        .resource::<InteractionMotion>()
        .is_some_and(|motion| !motion.duration.is_zero());
    reduced
        || !eased
        || world
            .resource::<Time>()
            .map(|time| time.delta.is_zero())
            .unwrap_or(true)
}

fn present_fill(world: &mut World, entity: Entity, presented: Option<Color>, target: Color) {
    let Some(from) = presented.filter(|color| *color != target) else {
        world.remove::<FillFade>(entity);
        world.insert(entity, target);
        return;
    };
    if snaps_motion(world) {
        world.remove::<FillFade>(entity);
        world.insert(entity, target);
        return;
    }
    let time = world.resource::<Time>().copied().unwrap_or_default();
    let start = time.now.saturating_sub(time.delta);
    let color = sample_fill(from, target, start, time.now, radii_duration(world));
    if color == target {
        world.remove::<FillFade>(entity);
        world.insert(entity, target);
    } else {
        world.insert(
            entity,
            FillFade {
                from,
                to: target,
                start,
            },
        );
        world.insert(entity, color);
    }
}

fn advance_fill(world: &mut World, entity: Entity) {
    let Some(fade) = world.get::<FillFade>(entity).copied() else {
        return;
    };
    if snaps_motion(world) {
        world.remove::<FillFade>(entity);
        world.insert(entity, fade.to);
        return;
    }
    let time = world.resource::<Time>().copied().unwrap_or_default();
    let color = sample_fill(
        fade.from,
        fade.to,
        fade.start,
        time.now,
        radii_duration(world),
    );
    if color == fade.to {
        world.remove::<FillFade>(entity);
        world.insert(entity, fade.to);
    } else {
        world.insert(entity, color);
    }
}

fn sample_fill(
    from: Color,
    to: Color,
    start: Duration,
    now: Duration,
    duration: Duration,
) -> Color {
    if duration.is_zero() {
        return to;
    }
    let elapsed = now.saturating_sub(start).as_secs_f32();
    let t = ease_out_cubic(elapsed / duration.as_secs_f32());
    if t >= 1.0 {
        to
    } else {
        mix_color(from, to, t)
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        1.0
    };
    1.0 - (1.0 - t).powi(3)
}

fn mix_color(from: Color, to: Color, t: f32) -> Color {
    Color::rgba(
        from.r + (to.r - from.r) * t,
        from.g + (to.g - from.g) * t,
        from.b + (to.b - from.b) * t,
        from.a + (to.a - from.a) * t,
    )
}

fn mask_round(world: &mut World, entity: Entity, radius: f32) {
    if radius > 0.0 && (!world.has::<ClipRounded>(entity) || world.has::<ManagedRound>(entity)) {
        world.insert(entity, ClipRounded::radius(radius));
        world.insert(entity, ManagedRound);
    } else if radius <= 0.0 {
        clear_mask(world, entity);
    }
}

fn clear_mask(world: &mut World, entity: Entity) {
    if world.has::<ManagedRound>(entity) {
        world.remove::<ClipRounded>(entity);
        world.remove::<ManagedRound>(entity);
    }
}
