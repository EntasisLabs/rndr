//! Window configuration, resident visibility, and surface events.
//!
//! Insert a [`WindowConfig`] before [`crate::App`] is handed to the desktop
//! runner. When it is absent the runner keeps today's window: an 800×600
//! decorated, resizable window titled `rndr`.
//!
//! [`show`], [`hide`], [`focus`], and [`toggle`] queue a command. A [`WindowHandle`]
//! is `Send`, so a task can keep one and call it after `.await`. `App::update`
//! applies commands that arrived from tasks or the platform before Input, and
//! merges the focus and blur events so that stage can read them. Commands a
//! system queues during Update, PostUpdate, or Layout are applied after Layout.
//! The runner performs the platform call after the frame and does not rebuild
//! the GPU surface.
//!
//! Focus and blur are one [`WindowFocus`] queue, so their order is the order
//! they happened. [`Dismiss`] runs at the start of Input, before the keymap,
//! and its blur is readable in Update. [`ScaleChanged`] and [`Resized`] are
//! written by [`publish_metrics`] before the frame, and [`Viewport`] is updated
//! in that same call, so Input and Layout read the new size and scale.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::event::{self, Event};
use crate::input::{Input, Key, Viewport};
use crate::keys::{LogicalKey, NamedKey, Platform};
use crate::runtime::{self, TimerHandle, WakeSlot};
use crate::world::World;

/// How long a blur must remain before `hide_on_blur` hides the window.
///
/// Releasing a grabbed hotkey can deliver focus-out and focus-in a fraction
/// of a millisecond apart. Those are applied together at the end of the
/// batch, and a blur that does not stay for this long does not hide.
const BLUR_SETTLE: Duration = Duration::from_millis(40);

/// After a programmatic show or focus, a blur waits at least this long.
/// The grab that showed the window can release well after the show itself.
const SHOW_GRACE: Duration = Duration::from_millis(200);

/// Physical pixels per point. Non-positive or non-finite values read as 1.
pub fn finite_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

fn differs(a: f32, b: f32) -> bool {
    (a - b).abs() > 0.001
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

/// Width and height in logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogicalSize {
    pub width: f32,
    pub height: f32,
}

impl LogicalSize {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// Where the window opens.
///
/// `At` is the outer top-left in logical points. Wayland ignores every
/// variant and the compositor places the window.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Position {
    /// The platform picks a position.
    #[default]
    Platform,
    /// Centered on the monitor the window opens on.
    Center,
    /// Outer top-left, in logical points.
    At { x: f32, y: f32 },
}

/// What Escape does when nothing else has claimed it.
///
/// This runs at the start of the Input stage, before the keymap and text
/// fields, and only when the config asks. The file-tree plugin keeps its own
/// Escape handling because the default is [`Ignore`](Self::Ignore).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dismiss {
    /// Leave the key for the app.
    #[default]
    Ignore,
    /// Set [`Input::quit`]. The runner exits.
    Close,
    /// Hide the window and keep the process running.
    Hide,
}

/// Ordinary top-level window, or a floating launcher panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowKind {
    /// Decorated top-level window. The default.
    #[default]
    Normal,
    /// Floating overlay.
    ///
    /// On macOS this is a non-activating panel at pop-up level: above other
    /// apps, including full-screen spaces, and on every Space. Focus does not
    /// call `activateIgnoringOtherApps`. [`WindowConfig::accessory`] selects
    /// `NSApplicationActivationPolicyAccessory`.
    ///
    /// Elsewhere the panel is a floating always-on-top window. Platforms
    /// without a window level ignore that part; see [`platform_support`].
    Panel,
}

/// What the app asks the runner to open.
///
/// Defaults match the window `rndr_render` used to hard-code.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowConfig {
    pub title: String,
    pub size: LogicalSize,
    /// Smallest inner size, in logical points. `None` leaves the platform minimum.
    pub min_size: Option<LogicalSize>,
    pub position: Position,
    pub resizable: bool,
    /// `false` is a frameless window.
    pub decorations: bool,
    /// Ask for a transparent framebuffer. The swapchain stays opaque when the
    /// surface cannot composite alpha.
    pub transparent: bool,
    /// Float above normal windows. A [`WindowKind::Panel`] does this too.
    /// Wayland ignores window levels.
    pub always_on_top: bool,
    /// Keep the window off the taskbar where the platform has one.
    /// On macOS there is no taskbar; set [`Self::accessory`] to leave the Dock.
    pub skip_taskbar: bool,
    pub kind: WindowKind,
    /// After the window has been focused once, a blur that stays hides it.
    /// The blur is recorded immediately. The hide waits until the blur
    /// settles, so a focus that returns in the same batch, or within that
    /// settle, keeps the window up. A blur inside the grace period after a
    /// programmatic show waits until the grace ends. A blur that arrives
    /// before the first focus is ignored, so a panel does not dismiss itself
    /// while it is opening. The hide does not emit a second blur.
    pub hide_on_blur: bool,
    pub dismiss: Dismiss,
    /// macOS only: run as an accessory app (no Dock icon) and do not activate
    /// over whatever was in front at launch. Ignored on other platforms.
    pub accessory: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "rndr".to_string(),
            size: LogicalSize::new(800.0, 600.0),
            min_size: None,
            position: Position::Platform,
            resizable: true,
            decorations: true,
            transparent: false,
            always_on_top: false,
            skip_taskbar: false,
            kind: WindowKind::Normal,
            hide_on_blur: false,
            dismiss: Dismiss::Ignore,
            accessory: false,
        }
    }
}

impl WindowConfig {
    /// The config resource, or [`Default`] when the app did not set one.
    pub fn get(world: &World) -> Self {
        world
            .resource::<Self>()
            .cloned()
            .unwrap_or_else(Self::default)
    }
}

/// Whether a platform honors one window option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// The runner asks the OS and the OS can do it.
    Supported,
    /// The command is accepted and the OS ignores it. The logical state
    /// still updates, and the GPU surface is not rebuilt.
    Ignored,
}

/// Per-platform support for the options on [`WindowConfig`].
///
/// Title, inner size, minimum size, `resizable`, and `decorations` are
/// honored on macOS, Windows, X11, and Wayland. Transparency is requested
/// everywhere; the swapchain falls back to opaque if the surface cannot
/// composite. [`platform_support`] is the table the runner documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowSupport {
    pub position: Availability,
    pub hide: Availability,
    pub focus: Availability,
    pub always_on_top: Availability,
    pub skip_taskbar: Availability,
    /// Pop-up window level that sits over full-screen spaces.
    pub panel_above_fullscreen: Availability,
    /// Visible on every virtual desktop / Space.
    pub panel_all_spaces: Availability,
    /// Key focus without activating the app that was in front.
    pub panel_nonactivating: Availability,
    /// Accessory activation policy (no Dock icon on macOS).
    pub accessory: Availability,
}

/// What [`Platform`] can actually do. Wayland positioning, hiding, focusing,
/// window levels, and skip-taskbar are ignored by winit 0.30.
pub fn platform_support(platform: Platform) -> WindowSupport {
    let ignored = WindowSupport {
        position: Availability::Ignored,
        hide: Availability::Ignored,
        focus: Availability::Ignored,
        always_on_top: Availability::Ignored,
        skip_taskbar: Availability::Ignored,
        panel_above_fullscreen: Availability::Ignored,
        panel_all_spaces: Availability::Ignored,
        panel_nonactivating: Availability::Ignored,
        accessory: Availability::Ignored,
    };
    match platform {
        Platform::Mac => WindowSupport {
            position: Availability::Supported,
            hide: Availability::Supported,
            focus: Availability::Supported,
            always_on_top: Availability::Supported,
            panel_above_fullscreen: Availability::Supported,
            panel_all_spaces: Availability::Supported,
            panel_nonactivating: Availability::Supported,
            accessory: Availability::Supported,
            ..ignored
        },
        Platform::Windows => WindowSupport {
            position: Availability::Supported,
            hide: Availability::Supported,
            focus: Availability::Supported,
            always_on_top: Availability::Supported,
            skip_taskbar: Availability::Supported,
            ..ignored
        },
        Platform::X11 => WindowSupport {
            position: Availability::Supported,
            hide: Availability::Supported,
            focus: Availability::Supported,
            always_on_top: Availability::Supported,
            // `_NET_WM_WINDOW_TYPE_UTILITY`, which window managers keep off the taskbar.
            skip_taskbar: Availability::Supported,
            ..ignored
        },
        Platform::Wayland => ignored,
    }
}

/// Logical visibility. The runner mirrors this onto the OS window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowPhase {
    pub visible: bool,
    pub focused: bool,
    /// Set after the first real focus. Earlier blur events are ignored.
    pub armed: bool,
}

impl Default for WindowPhase {
    fn default() -> Self {
        Self {
            visible: true,
            focused: false,
            armed: false,
        }
    }
}

impl WindowPhase {
    /// Visible, focused, and past the opening blur guard.
    pub fn focused() -> Self {
        Self {
            visible: true,
            focused: true,
            armed: true,
        }
    }
}

/// Something that moves [`WindowPhase`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowSignal {
    Show,
    Hide,
    Focus,
    Toggle,
    /// The platform reported key focus.
    GainedFocus,
    /// The platform reported that key focus left.
    LostFocus,
}

/// Focus and blur, in the order they were applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowFocus {
    Focused,
    Blurred,
}

impl Event for WindowFocus {}

/// What the runner should do to the OS window. The GPU surface stays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowEffect {
    Show,
    Hide,
    Focus,
}

/// Result of applying one [`WindowSignal`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowTransition {
    pub phase: WindowPhase,
    pub events: Vec<WindowFocus>,
    pub effects: Vec<WindowEffect>,
}

/// Step the visibility state machine once.
///
/// `Show` and `Focus` are separate. `Focus` while hidden does nothing.
/// `Toggle` hides a visible window, or shows and focuses a hidden one.
/// `LostFocus` emits [`WindowFocus::Blurred`] and leaves the window visible.
/// When [`WindowConfig::hide_on_blur`] is set, the hide is decided after the
/// queued signals for the frame have all been applied.
pub fn transition(
    mut phase: WindowPhase,
    _config: &WindowConfig,
    signal: WindowSignal,
) -> WindowTransition {
    let mut events = Vec::new();
    let mut effects = Vec::new();
    match signal {
        WindowSignal::Show => {
            if !phase.visible {
                phase.visible = true;
                effects.push(WindowEffect::Show);
            }
        }
        WindowSignal::Hide => apply_hide(&mut phase, &mut events, &mut effects),
        WindowSignal::Focus => request_focus(&mut phase, &mut events, &mut effects),
        WindowSignal::Toggle => {
            if phase.visible {
                apply_hide(&mut phase, &mut events, &mut effects);
            } else {
                phase.visible = true;
                effects.push(WindowEffect::Show);
                request_focus(&mut phase, &mut events, &mut effects);
            }
        }
        WindowSignal::GainedFocus => {
            if phase.visible && !phase.focused {
                phase.focused = true;
                phase.armed = true;
                events.push(WindowFocus::Focused);
            }
        }
        WindowSignal::LostFocus => {
            if phase.armed && phase.focused {
                phase.focused = false;
                events.push(WindowFocus::Blurred);
            }
        }
    }
    WindowTransition {
        phase,
        events,
        effects,
    }
}

fn apply_hide(
    phase: &mut WindowPhase,
    events: &mut Vec<WindowFocus>,
    effects: &mut Vec<WindowEffect>,
) {
    if phase.focused {
        phase.focused = false;
        events.push(WindowFocus::Blurred);
    }
    if phase.visible {
        phase.visible = false;
        effects.push(WindowEffect::Hide);
    }
}

fn request_focus(
    phase: &mut WindowPhase,
    events: &mut Vec<WindowFocus>,
    effects: &mut Vec<WindowEffect>,
) {
    if !phase.visible || phase.focused {
        return;
    }
    phase.focused = true;
    phase.armed = true;
    events.push(WindowFocus::Focused);
    effects.push(WindowEffect::Focus);
}

/// Last logical size and scale published from the OS window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowMetrics {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
}

impl Default for WindowMetrics {
    fn default() -> Self {
        Self {
            width: 800.0,
            height: 600.0,
            scale: 1.0,
        }
    }
}

/// Difference between two published metrics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricsChange {
    pub metrics: WindowMetrics,
    pub scale_changed: bool,
    pub resized: bool,
}

/// A zero physical size keeps the previous logical size, so a hide that
/// reports 0×0 does not collapse layout. Scale still updates.
pub fn advance_metrics(
    current: WindowMetrics,
    width_px: u32,
    height_px: u32,
    scale: f32,
) -> MetricsChange {
    let scale = finite_scale(scale);
    let (width, height) = if width_px == 0 || height_px == 0 {
        (current.width, current.height)
    } else {
        (width_px as f32 / scale, height_px as f32 / scale)
    };
    let metrics = WindowMetrics {
        width,
        height,
        scale,
    };
    MetricsChange {
        metrics,
        scale_changed: differs(current.scale, scale),
        resized: differs(current.width, width) || differs(current.height, height),
    }
}

/// The window's scale factor changed. Layout also reads [`Viewport::scale`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleChanged {
    pub scale: f32,
}

impl Event for ScaleChanged {}

/// The logical size changed. `scale` is the factor used to compute it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resized {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
}

impl Event for Resized {}

/// Write [`Viewport`] and, when the value changed, queue [`ScaleChanged`]
/// then [`Resized`]. Call this before `App::update` so the flip makes the
/// events visible to that frame's layout.
pub fn publish_metrics(world: &mut World, width_px: u32, height_px: u32, scale: f32) {
    let current = world
        .resource::<WindowMetrics>()
        .copied()
        .unwrap_or_default();
    let change = advance_metrics(current, width_px, height_px, scale);
    world.insert_resource(change.metrics);
    if let Some(viewport) = world.resource_mut::<Viewport>() {
        viewport.width = change.metrics.width;
        viewport.height = change.metrics.height;
        viewport.scale = change.metrics.scale;
    }
    if change.scale_changed {
        event::send(
            world,
            ScaleChanged {
                scale: change.metrics.scale,
            },
        );
    }
    if change.resized {
        event::send(
            world,
            Resized {
                width: change.metrics.width,
                height: change.metrics.height,
                scale: change.metrics.scale,
            },
        );
    }
}

#[derive(Clone)]
struct WindowInbox {
    signals: Arc<Mutex<Vec<WindowSignal>>>,
}

impl Default for WindowInbox {
    fn default() -> Self {
        Self {
            signals: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[derive(Default)]
struct WindowEffects {
    pending: Vec<WindowEffect>,
}

#[derive(Default)]
struct BlurWatch {
    blur_since: Option<Duration>,
    grace_until: Option<Duration>,
    timer: Option<TimerHandle>,
}

pub(crate) fn install(world: &mut World) {
    world.insert_resource(WindowInbox::default());
    world.insert_resource(WindowPhase::default());
    world.insert_resource(WindowMetrics::default());
    world.insert_resource(WindowEffects::default());
    world.insert_resource(BlurWatch::default());
}

/// Cloneable command sender. Safe to move into a `Send` task.
#[derive(Clone)]
pub struct WindowHandle {
    inbox: Arc<Mutex<Vec<WindowSignal>>>,
    wake: WakeSlot,
}

impl WindowHandle {
    pub fn show(&self) {
        self.push(WindowSignal::Show);
    }

    pub fn hide(&self) {
        self.push(WindowSignal::Hide);
    }

    pub fn focus(&self) {
        self.push(WindowSignal::Focus);
    }

    pub fn toggle(&self) {
        self.push(WindowSignal::Toggle);
    }

    fn push(&self, signal: WindowSignal) {
        lock(&self.inbox).push(signal);
        lock(&self.wake).wake();
    }
}

/// Handle for [`show`], [`hide`], [`focus`], and [`toggle`] from a task.
pub fn handle(world: &World) -> WindowHandle {
    let inbox = world
        .resource::<WindowInbox>()
        .expect("window installed by App")
        .signals
        .clone();
    WindowHandle {
        inbox,
        wake: runtime::wake_slot(world),
    }
}

/// Queue a show. The OS window is shown after this frame, without rebuilding the surface.
pub fn show(world: &World) {
    handle(world).show();
}

/// Queue a hide. A focused window emits [`WindowFocus::Blurred`].
pub fn hide(world: &World) {
    handle(world).hide();
}

/// Queue key focus. Does nothing while the window is hidden.
pub fn focus(world: &World) {
    handle(world).focus();
}

/// Hide if visible. Show and focus if hidden.
pub fn toggle(world: &World) {
    handle(world).toggle();
}

/// The platform gave the window key focus.
pub fn platform_focused(world: &World) {
    handle(world).push(WindowSignal::GainedFocus);
}

/// The platform took key focus away.
pub fn platform_blurred(world: &World) {
    handle(world).push(WindowSignal::LostFocus);
}

pub fn is_visible(world: &World) -> bool {
    world
        .resource::<WindowPhase>()
        .map(|phase| phase.visible)
        .unwrap_or(true)
}

pub fn is_focused(world: &World) -> bool {
    world
        .resource::<WindowPhase>()
        .map(|phase| phase.focused)
        .unwrap_or(false)
}

/// Effects the runner should apply to the OS window, in order.
pub fn drain_effects(world: &mut World) -> Vec<WindowEffect> {
    world
        .resource_mut::<WindowEffects>()
        .map(|effects| std::mem::take(&mut effects.pending))
        .unwrap_or_default()
}

/// Apply queued show / hide / focus signals, then decide whether a blur has
/// settled. Does not honor [`Dismiss`] and does not merge events. The frame
/// merges before Input, and again after it.
pub(crate) fn drive(world: &mut World) {
    for signal in drain_inbox(world) {
        apply_signal(world, signal);
    }
    settle_blur(world);
}

/// Input stage. A bare Escape hides or quits when [`WindowConfig::dismiss`]
/// asks, before the keymap and text fields see the key. A hide's blur is
/// sent here and becomes readable in Update with the other Input events.
pub(crate) fn apply_dismiss(world: &mut World) {
    if let Some(signal) = take_dismiss(world) {
        apply_signal(world, signal);
    }
}

fn drain_inbox(world: &World) -> Vec<WindowSignal> {
    let Some(inbox) = world.resource::<WindowInbox>() else {
        return Vec::new();
    };
    std::mem::take(&mut *lock(&inbox.signals))
}

fn apply_signal(world: &mut World, signal: WindowSignal) {
    let config = WindowConfig::get(world);
    let phase = world.resource::<WindowPhase>().copied().unwrap_or_default();
    let step = transition(phase, &config, signal);
    world.insert_resource(step.phase);
    for event in step.events {
        event::send(world, event);
    }
    if let Some(effects) = world.resource_mut::<WindowEffects>() {
        effects.pending.extend(step.effects.iter().copied());
    }
    note_watch(world, signal, phase, step.phase, &step.effects);
}

fn note_watch(
    world: &mut World,
    signal: WindowSignal,
    before: WindowPhase,
    after: WindowPhase,
    effects: &[WindowEffect],
) {
    if !world.has_resource::<BlurWatch>() {
        world.insert_resource(BlurWatch::default());
    }
    let now = runtime::clock_elapsed(world);
    let hide_on_blur = WindowConfig::get(world).hide_on_blur;
    let watch = world.resource_mut::<BlurWatch>().expect("blur watch");
    if effects
        .iter()
        .any(|effect| matches!(effect, WindowEffect::Show | WindowEffect::Focus))
    {
        watch.grace_until = Some(now.saturating_add(SHOW_GRACE));
    }
    if signal == WindowSignal::LostFocus
        && hide_on_blur
        && before.focused
        && !after.focused
        && watch.blur_since.is_none()
    {
        watch.blur_since = Some(now);
    }
    if after.focused || !after.visible {
        watch.blur_since = None;
        watch.timer.take();
    }
}

fn settle_blur(world: &mut World) {
    if !WindowConfig::get(world).hide_on_blur {
        if let Some(watch) = world.resource_mut::<BlurWatch>() {
            watch.blur_since = None;
            watch.timer.take();
        }
        return;
    }
    let phase = world.resource::<WindowPhase>().copied().unwrap_or_default();
    let now = runtime::clock_elapsed(world);
    enum Plan {
        Hide,
        Arm(Duration),
        Wait,
    }
    let plan = {
        let Some(watch) = world.resource_mut::<BlurWatch>() else {
            return;
        };
        if phase.focused || !phase.visible || watch.blur_since.is_none() {
            watch.blur_since = None;
            watch.timer.take();
            return;
        }
        let since = watch.blur_since.expect("blur start");
        let settle_at = since.saturating_add(BLUR_SETTLE);
        let deadline = match watch.grace_until {
            Some(grace) if grace > settle_at => grace,
            _ => settle_at,
        };
        if now >= deadline {
            watch.blur_since = None;
            watch.timer.take();
            Plan::Hide
        } else if watch.timer.is_none() {
            Plan::Arm(deadline.saturating_sub(now))
        } else {
            Plan::Wait
        }
    };
    match plan {
        Plan::Hide => apply_signal(world, WindowSignal::Hide),
        Plan::Arm(delay) => {
            let handle = runtime::call_after(world, delay, |world| {
                if let Some(watch) = world.resource_mut::<BlurWatch>() {
                    watch.timer = None;
                }
            });
            if let Some(watch) = world.resource_mut::<BlurWatch>() {
                watch.timer = Some(handle);
            }
        }
        Plan::Wait => {}
    }
}

fn take_dismiss(world: &mut World) -> Option<WindowSignal> {
    let action = WindowConfig::get(world).dismiss;
    if action == Dismiss::Ignore || !bare_escape(world) {
        return None;
    }
    consume_escape(world);
    match action {
        Dismiss::Ignore => None,
        Dismiss::Close => {
            if let Some(input) = world.resource_mut::<Input>() {
                input.quit = true;
            }
            None
        }
        Dismiss::Hide => Some(WindowSignal::Hide),
    }
}

fn bare_escape(world: &World) -> bool {
    let Some(input) = world.resource::<Input>() else {
        return false;
    };
    if input.ctrl || input.alt || input.super_key {
        return false;
    }
    if input.keys.is_empty() {
        return input.key == Some(Key::Escape);
    }
    input.keys.iter().any(|event| {
        matches!(event.logical, LogicalKey::Named(NamedKey::Escape))
            && !event.modifiers.ctrl
            && !event.modifiers.alt
            && !event.modifiers.super_key
    })
}

fn consume_escape(world: &mut World) {
    let Some(input) = world.resource_mut::<Input>() else {
        return;
    };
    input.keys.retain(|event| !is_bare_escape_event(event));
    if input.key == Some(Key::Escape) {
        input.key = None;
    }
}

fn is_bare_escape_event(event: &crate::keys::KeyEvent) -> bool {
    matches!(event.logical, LogicalKey::Named(NamedKey::Escape))
        && !event.modifiers.ctrl
        && !event.modifiers.alt
        && !event.modifiers.super_key
}
