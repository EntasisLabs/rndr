//! Window chrome the app draws itself: a custom titlebar band, regions that
//! move the window, caption buttons and resize edges for frameless windows.
//!
//! [`WindowConfig::titlebar`](crate::WindowConfig::titlebar) asks the runner
//! to take the native title bar away. macOS keeps its traffic lights over the
//! app's band; Windows and Linux lose the whole native frame, so the app draws
//! [`WindowButton`]s and core turns presses near the window's edges into
//! resizes. [`WindowChrome`] says which of these applies and how much room the
//! native controls take.
//!
//! Core only decides; the runner performs each [`WindowAction`] at the end of
//! the frame that queued it ([`take_window_actions`]). Headless runs queue
//! actions and nothing performs them.
use std::time::Duration;

use crate::input::{CursorStyle, Input, MotionWatch, PointerButton};
use crate::layout::{ancestor_with, hit_test};
use crate::{Component, Entity, Platform, Time, Viewport, World};

/// A titlebar the app draws across the top of the window, `height` points tall.
///
/// - **macOS:** content runs under a transparent title bar. The traffic lights
///   stay, centred vertically in the band unless [`Self::traffic_lights`]
///   places them. The window keeps its native frame, edges and shadow.
/// - **Windows and Linux:** the window is frameless. The app draws its own
///   caption buttons ([`WindowButton`]) and core adds resize edges.
/// - **Browser:** ignored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Titlebar {
    /// Band height in logical points.
    pub height: f32,
    /// macOS: the close button's top-left in points from the window's
    /// top-left. `None` centres the buttons in the band, 16 points in.
    pub traffic_lights: Option<[f32; 2]>,
}

impl Titlebar {
    /// A band `height` points tall with the traffic lights centred in it.
    pub const fn new(height: f32) -> Self {
        Self {
            height,
            traffic_lights: None,
        }
    }

    /// Place the macOS close button's top-left at `x`, `y` points.
    pub const fn traffic_lights(mut self, x: f32, y: f32) -> Self {
        self.traffic_lights = Some([x, y]);
        self
    }
}

/// A material the system draws behind a transparent window.
///
/// Needs [`WindowConfig::transparent`](crate::WindowConfig::transparent); the
/// app's fills draw over it, so leave the root fill clear or translucent where
/// the material should show. Linux and the browser ignore it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Backdrop {
    /// No system material.
    #[default]
    None,
    /// Main window surface: Mica on Windows 11, the under-window-background
    /// vibrancy material on macOS.
    Window,
    /// Short-lived surfaces such as popovers and flyouts: Acrylic on
    /// Windows 11, the popover material on macOS.
    Transient,
    /// Sidebars and tabbed windows: Mica Alt on Windows 11, the sidebar
    /// material on macOS.
    Sidebar,
}

/// Pressing this element, or a non-interactive descendant, moves the window.
///
/// Buttons, fields, focusable elements and selectable text inside keep their
/// presses. A double-click maximizes (macOS follows the system's titlebar
/// double-click setting), and a right-click opens the window menu on Windows.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowDrag;
impl Component for WindowDrag {}

/// A caption button. A primary press on it, or on a descendant, performs the
/// matching [`WindowAction`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowButton {
    /// Minimize the window.
    Minimize,
    /// Maximize, or restore a maximized window.
    Maximize,
    /// Ask the window to close, as the native close button does.
    Close,
}
impl Component for WindowButton {}

/// The window edge or corner a resize grabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResizeEdge {
    /// Top edge.
    North,
    /// Bottom edge.
    South,
    /// Right edge.
    East,
    /// Left edge.
    West,
    /// Top-right corner.
    NorthEast,
    /// Top-left corner.
    NorthWest,
    /// Bottom-right corner.
    SouthEast,
    /// Bottom-left corner.
    SouthWest,
}

impl ResizeEdge {
    /// The cursor shown over this edge.
    pub fn cursor(self) -> CursorStyle {
        match self {
            Self::North => CursorStyle::ResizeNorth,
            Self::South => CursorStyle::ResizeSouth,
            Self::East => CursorStyle::ResizeEast,
            Self::West => CursorStyle::ResizeWest,
            Self::NorthEast => CursorStyle::ResizeNorthEast,
            Self::NorthWest => CursorStyle::ResizeNorthWest,
            Self::SouthEast => CursorStyle::ResizeSouthEast,
            Self::SouthWest => CursorStyle::ResizeSouthWest,
        }
    }
}

/// What the runner does to the OS window at the end of the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowAction {
    /// Move the window with the held pointer.
    Drag,
    /// Resize from an edge with the held pointer.
    Resize(ResizeEdge),
    /// Minimize.
    Minimize,
    /// Maximize, or restore a maximized window.
    ToggleMaximize,
    /// Ask to close, through the same path as the native close button.
    Close,
    /// A double-click on a drag region: maximize, or on macOS what the
    /// system's titlebar double-click setting says.
    TitlebarDoubleClick,
    /// Windows: open the window menu at this point in logical points.
    WindowMenu([f32; 2]),
}

/// Queue `action` for the runner.
pub fn request_window_action(world: &mut World, action: WindowAction) {
    if !world.has_resource::<WindowActions>() {
        world.insert_resource(WindowActions::default());
    }
    world
        .resource_mut::<WindowActions>()
        .unwrap()
        .0
        .push(action);
}

/// Take the actions queued since the last call, oldest first.
pub fn take_window_actions(world: &mut World) -> Vec<WindowAction> {
    world
        .resource_mut::<WindowActions>()
        .map(|queue| std::mem::take(&mut queue.0))
        .unwrap_or_default()
}

#[derive(Default)]
struct WindowActions(Vec<WindowAction>);

/// How the window's frame is drawn this frame. The runner publishes it;
/// [`Default`] (a native frame) applies headless and before the first frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowChrome {
    /// Points from the window's left edge that native controls cover in the
    /// titlebar band (macOS traffic lights). Add your own spacing after it.
    pub leading: f32,
    /// Points from the right edge that native controls cover.
    pub trailing: f32,
    /// The native frame is gone and the app should draw [`WindowButton`]s.
    pub draws_controls: bool,
    /// Core turns presses near the window's edges into resizes.
    pub resize_edges: bool,
    /// The window is maximized.
    pub maximized: bool,
    /// The [`WindowButton`] under the pointer, for hover styling.
    pub hovered: Option<Entity>,
}

impl WindowChrome {
    /// What `config` gets on a desktop `platform` before the runner measures
    /// anything. The browser runner keeps [`Default`].
    pub fn expected(config: &crate::WindowConfig, platform: Platform) -> Self {
        let custom = config.titlebar.is_some() && config.decorations;
        let mac = platform == Platform::Mac;
        let frameless = !config.decorations || (custom && !mac);
        Self {
            draws_controls: custom && !mac,
            resize_edges: frameless && config.resizable,
            ..Self::default()
        }
    }
}

/// Width of the band along each edge that resizes a frameless window.
pub const RESIZE_EDGE: f32 = 6.0;
/// How far along an edge from a corner a press still grabs the corner.
pub const RESIZE_CORNER: f32 = 12.0;
const DOUBLE_CLICK: Duration = Duration::from_millis(500);
const DOUBLE_CLICK_SLOP: f32 = 4.0;

/// The edge or corner at `at` in a `width` by `height` window, if any.
pub fn resize_edge_at(at: [f32; 2], width: f32, height: f32) -> Option<ResizeEdge> {
    let [x, y] = at;
    if x < 0.0 || y < 0.0 || x > width || y > height {
        return None;
    }
    let west = x < RESIZE_EDGE;
    let east = x > width - RESIZE_EDGE;
    let north = y < RESIZE_EDGE;
    let south = y > height - RESIZE_EDGE;
    let near_west = x < RESIZE_CORNER;
    let near_east = x > width - RESIZE_CORNER;
    let near_north = y < RESIZE_CORNER;
    let near_south = y > height - RESIZE_CORNER;
    Some(match () {
        _ if (north && near_west) || (west && near_north) => ResizeEdge::NorthWest,
        _ if (north && near_east) || (east && near_north) => ResizeEdge::NorthEast,
        _ if (south && near_west) || (west && near_south) => ResizeEdge::SouthWest,
        _ if (south && near_east) || (east && near_south) => ResizeEdge::SouthEast,
        _ if north => ResizeEdge::North,
        _ if south => ResizeEdge::South,
        _ if west => ResizeEdge::West,
        _ if east => ResizeEdge::East,
        _ => return None,
    })
}

/// The [`WindowDrag`] region a press at `hit` moves, unless something
/// interactive between them keeps the press.
fn drag_region(world: &World, hit: Entity) -> Option<Entity> {
    let mut current = Some(hit);
    while let Some(entity) = current {
        if world.has::<WindowDrag>(entity) {
            return Some(entity);
        }
        if keeps_press(world, entity) {
            return None;
        }
        current = world
            .get::<crate::Parent>(entity)
            .map(|parent| parent.entity);
    }
    None
}

fn keeps_press(world: &World, entity: Entity) -> bool {
    world.has::<WindowButton>(entity)
        || world.has::<crate::Button>(entity)
        || world.has::<crate::CapturesKeys>(entity)
        || world.has::<crate::TextField>(entity)
        || world.has::<crate::Selectable>(entity)
        || world.has::<crate::ContextTarget>(entity)
        || crate::interaction::access_details(world, entity).focus
}

#[derive(Clone, Copy, Default)]
struct ChromeState {
    /// Time and place of the last press that started a drag.
    last_drag: Option<(Duration, [f32; 2])>,
    /// This system set a resize cursor and must put the arrow back.
    edge_cursor: bool,
}

/// Front of Input: resize edges, caption buttons and drag regions. A press
/// these take is removed from [`Input`], so nothing under it also activates.
/// [`Chrome`](crate::Chrome) installs it; a plugin that adds its own front
/// Input systems adds it again after them so window edges still win. A second
/// run in the same frame finds the press already taken and changes nothing.
pub fn apply_window_chrome(world: &mut World) {
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    let mut chrome = world
        .resource::<WindowChrome>()
        .copied()
        .unwrap_or_default();
    let mut state = world.resource::<ChromeState>().copied().unwrap_or_default();
    let pointer = input.pointer;
    let at = pointer
        .press_at
        .filter(|_| pointer.pressed)
        .unwrap_or(pointer.position);
    let (width, height) = world
        .resource::<Viewport>()
        .map(|viewport| (viewport.width, viewport.height))
        .unwrap_or_default();

    let edge = (chrome.resize_edges && !chrome.maximized)
        .then(|| resize_edge_at(pointer.position, width, height))
        .flatten();
    if chrome.resize_edges || chrome.draws_controls {
        world.insert_resource(MotionWatch { active: true });
    }
    if chrome.resize_edges && !pointer.down {
        if let Some(edge) = edge {
            world.insert_resource(edge.cursor());
            state.edge_cursor = true;
        } else if state.edge_cursor {
            state.edge_cursor = false;
            world.insert_resource(CursorStyle::Arrow);
        }
    }

    let hit = hit_test(world, at[0], at[1]);
    let button = hit.and_then(|hit| {
        world
            .has::<WindowButton>(hit)
            .then_some(hit)
            .or_else(|| ancestor_with::<WindowButton>(world, hit))
    });
    chrome.hovered = if pointer.down || edge.is_some() {
        chrome.hovered.filter(|_| pointer.down)
    } else {
        button
    };

    let mut taken = None;
    if pointer.pressed && pointer.button == PointerButton::Left {
        let press_edge = (chrome.resize_edges && !chrome.maximized)
            .then(|| resize_edge_at(at, width, height))
            .flatten();
        if let Some(edge) = press_edge {
            taken = Some(WindowAction::Resize(edge));
        } else if let Some(button) = button {
            taken = world
                .get::<WindowButton>(button)
                .map(|button| match button {
                    WindowButton::Minimize => WindowAction::Minimize,
                    WindowButton::Maximize => WindowAction::ToggleMaximize,
                    WindowButton::Close => WindowAction::Close,
                });
        } else if hit.and_then(|hit| drag_region(world, hit)).is_some() {
            let now = world.resource::<Time>().map(|t| t.now).unwrap_or_default();
            let double = state.last_drag.is_some_and(|(then, first)| {
                now.saturating_sub(then) <= DOUBLE_CLICK
                    && (first[0] - at[0]).abs() <= DOUBLE_CLICK_SLOP
                    && (first[1] - at[1]).abs() <= DOUBLE_CLICK_SLOP
            });
            if double {
                state.last_drag = None;
                taken = Some(WindowAction::TitlebarDoubleClick);
            } else {
                state.last_drag = Some((now, at));
                taken = Some(WindowAction::Drag);
            }
        }
    } else if pointer.pressed
        && pointer.button == PointerButton::Right
        && hit.and_then(|hit| drag_region(world, hit)).is_some()
    {
        taken = Some(WindowAction::WindowMenu(at));
    }

    if let Some(action) = taken {
        request_window_action(world, action);
        if let Some(input) = world.resource_mut::<Input>() {
            input.pointer.pressed = false;
            input.pointer.press_at = None;
        }
    }
    world.insert_resource(chrome);
    world.insert_resource(state);
}
