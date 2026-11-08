//! Runs a [`CanvasEditor`] inside an `rndr_core` world: reads pointer, pen,
//! wheel and key input over a [`SurfaceSlot`], and paints the scene and
//! overlay into that slot.

use crate::editor::{
    self, CanvasEditor, CursorHint, EditorInput, Key, Modifiers, PointerInput, Sample,
};
use crate::overlay::{paint_overlay, OverlayTheme};
use crate::paint::{affine, paint_scene, EngineMeasure, ImageIds};
use rndr_canvas::text::{EstimateMeasure, TextMeasure};
use rndr_core::{
    platform_of, CursorStyle, Input, LogicalKey, MotionWatch, NamedKey, PointerButton,
    PointerKind as CoreKind, Pointers, Rect, SurfaceSlot, World,
};
use rndr_paint::{Canvas, CanvasPainter, CanvasRegistry};

/// The editor and what painting it needs. Insert it as a resource with
/// [`install`].
pub struct CanvasView {
    pub editor: CanvasEditor,
    /// Texture ids for the document's image assets.
    pub images: ImageIds,
    pub overlay: OverlayTheme,
    /// The [`SurfaceSlot`] id the canvas is laid out in.
    pub slot: u64,
    captured: bool,
}

impl CanvasView {
    pub fn new(editor: CanvasEditor, slot: u64) -> Self {
        Self {
            editor,
            images: ImageIds::new(),
            overlay: OverlayTheme::default(),
            slot,
            captured: false,
        }
    }
}

/// Inserts `view` and registers its painter for `view.slot`. Spawn a
/// [`SurfaceSlot`] with that id where the canvas belongs, and run
/// [`update`] each frame.
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

/// Feeds this frame's input to the installed [`CanvasView`].
pub fn update(world: &mut World) {
    let Some(slot) = world.resource::<CanvasView>().map(|v| v.slot) else {
        return;
    };
    let Some(rect) = slot_rect(world, slot) else {
        return;
    };
    let captured = world.resource::<CanvasView>().is_some_and(|v| v.captured);
    let input = editor_input(world, rect, captured);
    let measure = measure(world);
    let Some(view) = world.resource_mut::<CanvasView>() else {
        return;
    };
    let response = view.editor.handle(&input, measure.as_ref());
    if let Some(p) = &input.pointer {
        view.captured = p.down && !p.released;
    }
    if input.pointer.is_some() {
        world.insert_resource(match response.cursor {
            CursorHint::Text => CursorStyle::Text,
            CursorHint::Grab | CursorHint::Move => CursorStyle::Hand,
            _ => CursorStyle::Arrow,
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
        let origin = slot_rect(world, view.slot).map_or([0.0, 0.0], |r| [r.x, r.y]);
        let to_window = rndr_canvas::Matrix::translate(f64::from(origin[0]), f64::from(origin[1]));
        canvas.set_camera(affine(to_window.then(view.editor.camera.matrix())));
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
