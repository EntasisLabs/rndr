//! Runs a [`CanvasEditor`] inside an `rndr_core` world: reads pointer, pen,
//! wheel and key input over a [`SurfaceSlot`], and paints the scene and
//! overlay into that slot.

use crate::editor::{
    self, CanvasEditor, CursorHint, EditorInput, Key, Modifiers, PointerInput, Sample,
};
use crate::overlay::{paint_overlay, OverlayTheme};
use crate::paint::{affine, paint_scene, EngineMeasure, ImageIds};
use rndr_canvas::text::{EstimateMeasure, TextMeasure};
use rndr_canvas::Vec2;
use rndr_core::{
    platform_of, Color, CursorStyle, Input, LogicalKey, MotionWatch, NamedKey, PointerButton,
    PointerKind as CoreKind, Pointers, Rect, SurfaceSlot, World,
};
use rndr_paint::{Canvas, CanvasPainter, CanvasRegistry, FillRule, Path};

/// The editor and what painting it needs. Insert it as a resource with
/// [`install`].
pub struct CanvasView {
    pub editor: CanvasEditor,
    /// Texture ids for the document's image assets.
    pub images: ImageIds,
    pub overlay: OverlayTheme,
    /// Painted under the scene across the whole slot, so a transparent scene
    /// background still reads as a canvas. Transparent shows the window.
    pub backdrop: Color,
    /// The [`SurfaceSlot`] id the canvas is laid out in.
    pub slot: u64,
    captured: bool,
    wheel: [f32; 2],
}

impl CanvasView {
    pub fn new(editor: CanvasEditor, slot: u64) -> Self {
        Self {
            editor,
            images: ImageIds::new(),
            overlay: OverlayTheme::default(),
            backdrop: Color::rgb(0.976, 0.98, 0.984),
            slot,
            captured: false,
            wheel: [0.0, 0.0],
        }
    }
}

/// Inserts `view` and registers its painter for `view.slot`. Spawn a
/// [`SurfaceSlot`] with that id where the canvas belongs, run
/// [`take_wheel`] at the front of the Input schedule and [`update`] each
/// frame.
pub fn install(world: &mut World, view: CanvasView) {
    let slot = view.slot;
    world.insert_resource(view);
    if world.resource::<CanvasRegistry>().is_none() {
        world.insert_resource(CanvasRegistry::new());
    }
    if let Some(registry) = world.resource_mut::<CanvasRegistry>() {
        registry.insert(slot, ViewPainter);
    }
    world.insert_resource(MotionWatch { active: true });
}

fn measure(world: &World) -> Box<dyn TextMeasure> {
    match EngineMeasure::from_world(world) {
        Some(m) => Box::new(m),
        None => Box::new(EstimateMeasure),
    }
}

pub(crate) fn slot_rect(world: &World, slot: u64) -> Option<Rect> {
    world
        .entities_with::<SurfaceSlot>()
        .into_iter()
        .find(|e| world.get::<SurfaceSlot>(*e).is_some_and(|s| s.id == slot))
        .and_then(|e| world.get::<Rect>(e).copied())
}

fn key(logical: &LogicalKey) -> Option<Key> {
    Some(match logical {
        LogicalKey::Named(NamedKey::Escape) => Key::Escape,
        LogicalKey::Named(NamedKey::Enter) => Key::Enter,
        LogicalKey::Named(NamedKey::Backspace) => Key::Backspace,
        LogicalKey::Named(NamedKey::Delete) => Key::Delete,
        LogicalKey::Named(NamedKey::ArrowLeft) => Key::Left,
        LogicalKey::Named(NamedKey::ArrowRight) => Key::Right,
        LogicalKey::Named(NamedKey::ArrowUp) => Key::Up,
        LogicalKey::Named(NamedKey::ArrowDown) => Key::Down,
        LogicalKey::Character(c) => Key::Char(c.to_ascii_lowercase()),
        _ => return None,
    })
}

/// Builds this frame's [`EditorInput`] from the world. The pointer counts
/// while it is over `rect` or while a press that began there is held.
pub fn editor_input(world: &World, rect: Rect, captured: bool) -> EditorInput {
    let input = world.resource::<Input>().cloned().unwrap_or_default();
    let platform = platform_of(world);
    let origin = [rect.x, rect.y];
    let local = |p: [f32; 2]| [p[0] - origin[0], p[1] - origin[1]];
    let inside = |p: [f32; 2]| {
        p[0] >= rect.x && p[1] >= rect.y && p[0] < rect.x + rect.w && p[1] < rect.y + rect.h
    };
    let pen = world.resource::<Pointers>().and_then(|pointers| {
        pointers
            .iter()
            .find(|c| matches!(c.kind, CoreKind::Pen | CoreKind::Eraser) && (c.down || c.released))
            .cloned()
    });
    let pointer = match pen {
        Some(contact) => Some(PointerInput {
            position: local(contact.position),
            pressed: contact.pressed,
            down: contact.down,
            released: contact.released,
            button: editor::Button::Primary,
            kind: if contact.kind == CoreKind::Eraser {
                editor::PointerKind::Eraser
            } else {
                editor::PointerKind::Pen
            },
            samples: contact
                .samples
                .iter()
                .map(|s| Sample {
                    position: local(s.position),
                    pressure: s.pressure,
                    tilt: s.tilt,
                    time_ms: s.time.as_secs_f64() * 1000.0,
                })
                .collect(),
        }),
        None => {
            let p = input.pointer;
            let touch = world
                .resource::<Pointers>()
                .and_then(|ps| ps.get(p.id).map(|c| c.kind == CoreKind::Touch))
                .unwrap_or(false);
            let samples = world
                .resource::<Pointers>()
                .and_then(|ps| ps.get(p.id))
                .map(|c| {
                    c.samples
                        .iter()
                        .map(|s| Sample {
                            position: local(s.position),
                            pressure: s.pressure,
                            tilt: s.tilt,
                            time_ms: s.time.as_secs_f64() * 1000.0,
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(PointerInput {
                position: local(p.position),
                pressed: p.pressed,
                down: p.down,
                released: p.released,
                button: match p.button {
                    PointerButton::Middle => editor::Button::Middle,
                    PointerButton::Right => editor::Button::Secondary,
                    _ => editor::Button::Primary,
                },
                kind: if touch {
                    editor::PointerKind::Touch
                } else {
                    editor::PointerKind::Mouse
                },
                samples,
            })
        }
    };
    let pointer = pointer.filter(|p| {
        let window = [p.position[0] + origin[0], p.position[1] + origin[1]];
        if p.pressed {
            inside(window)
        } else {
            captured || inside(window)
        }
    });
    let over = pointer.is_some();
    let modifiers = Modifiers {
        shift: input.shift,
        alt: input.alt,
        primary: if platform.primary_is_super() {
            input.super_key
        } else {
            input.ctrl
        },
    };
    EditorInput {
        pointer,
        scroll: if over { input.scroll } else { [0.0, 0.0] },
        modifiers,
        keys: input
            .keys
            .iter()
            .filter(|e| !e.released)
            .filter_map(|e| {
                key(&e.logical).map(|k| {
                    (
                        k,
                        Modifiers {
                            shift: e.modifiers.shift,
                            alt: e.modifiers.alt,
                            primary: e.modifiers.primary(platform),
                        },
                    )
                })
            })
            .collect(),
        text: if modifiers.primary {
            String::new()
        } else {
            input.chars.clone()
        },
        viewport: [rect.w, rect.h],
        time_ms: world
            .resource::<Pointers>()
            .and_then(|ps| ps.primary().map(|c| c.time.as_secs_f64() * 1000.0))
            .unwrap_or(0.0),
    }
}

/// Claims the wheel while the pointer is over the canvas (or a press that
/// began there is held), before layout's scroll pass consumes it for the
/// surrounding scrollports. `rndr_ui` runs [`rndr_core::apply_scroll`] at the
/// front of [`rndr_core::Schedule::Input`], so add this with
/// `add_system_front` after the plugins. The next [`update`] pans or zooms
/// with it.
pub fn take_wheel(world: &mut World) {
    let Some((slot, captured)) = world.resource::<CanvasView>().map(|v| (v.slot, v.captured))
    else {
        return;
    };
    let Some(rect) = slot_rect(world, slot) else {
        return;
    };
    let Some(input) = world.resource_mut::<Input>() else {
        return;
    };
    let [x, y] = input.pointer.position;
    let inside = x >= rect.x && y >= rect.y && x < rect.x + rect.w && y < rect.y + rect.h;
    if input.scroll == [0.0, 0.0] || !(inside || captured) {
        return;
    }
    let scroll = std::mem::take(&mut input.scroll);
    if let Some(view) = world.resource_mut::<CanvasView>() {
        view.wheel[0] += scroll[0];
        view.wheel[1] += scroll[1];
    }
}

/// Feeds this frame's input to the installed [`CanvasView`].
pub fn update(world: &mut World) {
    let Some(slot) = world.resource::<CanvasView>().map(|v| v.slot) else {
        return;
    };
    let Some(rect) = slot_rect(world, slot) else {
        return;
    };
    let captured = world.resource::<CanvasView>().is_some_and(|v| v.captured);
    let mut input = editor_input(world, rect, captured);
    let measure = measure(world);
    let Some(view) = world.resource_mut::<CanvasView>() else {
        return;
    };
    let wheel = std::mem::take(&mut view.wheel);
    input.scroll[0] += wheel[0];
    input.scroll[1] += wheel[1];
    let response = view.editor.handle(&input, measure.as_ref());
    if let Some(p) = &input.pointer {
        view.captured = p.down && !p.released;
    }
    if input.pointer.is_some() {
        world.insert_resource(match response.cursor {
            CursorHint::Text => CursorStyle::Text,
            CursorHint::Grab => CursorStyle::Grab,
            CursorHint::Move => CursorStyle::Move,
            CursorHint::Crosshair => CursorStyle::Crosshair,
            CursorHint::Default => CursorStyle::Arrow,
        });
    }
}

struct ViewPainter;

impl CanvasPainter for ViewPainter {
    fn paint(&self, world: &World, canvas: &mut Canvas) {
        let Some(view) = world.resource::<CanvasView>() else {
            return;
        };
        let measure = measure(world);
        // Surfaces paint in window coordinates; the editor works relative
        // to the slot.
        let Some(rect) = slot_rect(world, view.slot) else {
            return;
        };
        let to_window = rndr_canvas::Matrix::translate(f64::from(rect.x), f64::from(rect.y));
        let camera = view.editor.camera;
        canvas.set_camera(affine(to_window.then(camera.matrix())));
        if view.backdrop.a > 0.0 {
            let corner = |x: f32, y: f32| {
                let p = camera.view_to_scene(Vec2::new(f64::from(x), f64::from(y)));
                [p.x as f32, p.y as f32]
            };
            canvas.fill(
                Path::polygon(&[
                    corner(0.0, 0.0),
                    corner(rect.w, 0.0),
                    corner(rect.w, rect.h),
                    corner(0.0, rect.h),
                ]),
                view.backdrop,
                FillRule::NonZero,
            );
        }
        paint_scene(
            canvas,
            world,
            &view.editor.scene(),
            &view.images,
            measure.as_ref(),
        );
        paint_overlay(canvas, &view.editor, measure.as_ref(), &view.overlay);
    }
}
