//! Minimize, maximize and close buttons for a window with a custom titlebar
//! ([`rndr_core::Titlebar`]).
//!
//! [`window_controls`] mounts the row at the end of the app's titlebar band.
//! It shows only where [`WindowChrome::draws_controls`] says the native frame
//! is gone (Windows and Linux); on macOS the traffic lights stay and the row
//! is hidden. Core performs the presses ([`WindowButton`]) and reports hover.
use rndr_core::{
    Absolute, Align, Border, Color, Component, Entity, Hidden, Input, Layout, Semantics, Size,
    Transform, WindowButton, WindowChrome, World,
};

use crate::{Token, UiColor, UiStyle, UiTheme};

/// Width of one caption button, as on Windows 11.
pub const CAPTION_WIDTH: f32 = 46.0;

/// The row [`window_controls`] mounts.
#[derive(Clone, Copy, Debug)]
pub struct WindowControls;
impl Component for WindowControls {}

#[derive(Clone, Debug)]
struct Caption {
    glyphs: Vec<Entity>,
}
impl Component for Caption {}

/// Mount minimize, maximize and close buttons under `parent`, filling its
/// height, and return the row.
pub fn window_controls(world: &mut World, parent: Entity) -> Entity {
    let row = world.spawn((
        Layout {
            width: Size::Hug,
            height: Size::Fill,
            ..Layout::row()
        },
        WindowControls,
    ));
    rndr_core::set_parent(world, row, parent);
    for (button, name) in [
        (WindowButton::Minimize, "Minimize"),
        (WindowButton::Maximize, "Maximize"),
        (WindowButton::Close, "Close"),
    ] {
        let entity = world.spawn((
            Layout {
                width: Size::Px(CAPTION_WIDTH),
                height: Size::Fill,
                ..Layout::row()
            },
            Align::center(),
            button,
            Semantics {
                name: Some(name.into()),
                ..Semantics::default()
            },
        ));
        rndr_core::set_parent(world, entity, row);
        let glyphs = glyph(world, entity, button);
        world.insert(entity, Caption { glyphs });
    }
    row
}

fn bar(world: &mut World, parent: Entity, width: f32, height: f32) -> Entity {
    let bar = world.spawn((Layout {
        width: Size::Px(width),
        height: Size::Px(height),
        ..Layout::row()
    },));
    rndr_core::set_parent(world, bar, parent);
    bar
}

fn glyph(world: &mut World, button: Entity, kind: WindowButton) -> Vec<Entity> {
    match kind {
        WindowButton::Minimize => vec![bar(world, button, 10.0, 1.0)],
        WindowButton::Maximize => {
            let square = bar(world, button, 10.0, 10.0);
            world.insert(square, Color::rgba(0.0, 0.0, 0.0, 0.0));
            vec![square]
        }
        WindowButton::Close => {
            let cross = bar(world, button, 14.0, 14.0);
            [1.0, -1.0]
                .map(|turn| {
                    let stroke = world.spawn((
                        Layout {
                            width: Size::Fill,
                            height: Size::Px(1.0),
                            ..Layout::row()
                        },
                        Absolute {
                            left: Some(-2.0),
                            top: Some(6.5),
                            right: Some(-2.0),
                            bottom: None,
                        },
                        Transform::rotate(turn * std::f32::consts::FRAC_PI_4),
                    ));
                    rndr_core::set_parent(world, stroke, cross);
                    stroke
                })
                .to_vec()
        }
    }
}

const CLOSE_HOVER: Color = Color::rgba(0.769, 0.169, 0.110, 1.0);
const CLOSE_PRESSED: Color = Color::rgba(0.698, 0.180, 0.129, 1.0);

/// Layout: hide the row where the system draws controls, and style hover and
/// press from [`WindowChrome::hovered`].
pub(crate) fn styles(world: &mut World) {
    let chrome = world
        .resource::<WindowChrome>()
        .copied()
        .unwrap_or_default();
    for row in world.entities_with::<WindowControls>() {
        match (chrome.draws_controls, world.has::<Hidden>(row)) {
            (true, true) => {
                world.remove::<Hidden>(row);
            }
            (false, false) => world.insert(row, Hidden),
            _ => {}
        }
    }
    let down = world
        .resource::<Input>()
        .is_some_and(|input| input.pointer.down);
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    for button in world.entities_with::<Caption>() {
        let Some(kind) = world.get::<WindowButton>(button).copied() else {
            continue;
        };
        let lit = chrome.hovered == Some(button);
        let close = kind == WindowButton::Close;
        let background: UiColor = match (lit, down, close) {
            (false, ..) => UiColor::Explicit(Color::rgba(0.0, 0.0, 0.0, 0.0)),
            (true, false, true) => UiColor::Explicit(CLOSE_HOVER),
            (true, true, true) => UiColor::Explicit(CLOSE_PRESSED),
            (true, false, false) => Token::ControlHover.into(),
            (true, true, false) => Token::ControlPressed.into(),
        };
        world.insert(
            button,
            UiStyle {
                background: Some(background),
                ..UiStyle::default()
            },
        );
        let ink = if lit && close {
            Color::rgba(1.0, 1.0, 1.0, 1.0)
        } else {
            theme.color(Token::Text)
        };
        let glyphs = world.get::<Caption>(button).unwrap().glyphs.clone();
        for glyph in glyphs {
            if kind == WindowButton::Maximize {
                world.insert(
                    glyph,
                    Border {
                        color: ink,
                        width: 1.0,
                    },
                );
            } else {
                world.insert(glyph, ink);
            }
        }
    }
}
