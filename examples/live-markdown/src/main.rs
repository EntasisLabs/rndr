//! Live markdown editor.
//!
//! The concrete syntax tree is the authority. The window is a typed view:
//! markers stay in the file, and the caret edits the rendered text. A callout
//! fence is Donato's Liquid (`FENCE_LANGS`), edited as fields.
//!
//! ```text
//! cargo run -p live-markdown
//! ```

use std::sync::Arc;

use rndr_core::*;
use rndr_markdown_ui::{
    MarkdownCommand, MarkdownDocuments, MarkdownPlugin, MarkdownSurface, MarkdownSurfaceHandle,
    MarkdownSurfaceProps,
};
use rndr_ui::controls::{host_intent, Button, ButtonProps, HostIntent};
use rndr_ui::UiId;

const SAMPLE: &str = "\
# Live markdown

Type into the paragraph. Marks stay in the file: *italic*, **bold**, and `code`.

- first item
- second item

> A quote keeps its marker.

```
fn main() {}
```

```callout
tone: tip
# keep this comment
body: \"Edit the field, not the fence.\"
```
";

#[derive(Clone, Copy)]
struct Host {
    surface: MarkdownSurfaceHandle,
}

fn main() {
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::new());
    app.world_mut().insert_resource(engine);
    app.world_mut().insert_resource(WindowConfig {
        title: "Live markdown — rndr".into(),
        size: LogicalSize::new(880.0, 720.0),
        ..WindowConfig::default()
    });
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, toolbar);
    rndr_render::run(app);
}

fn setup(world: &mut World) {
    let document = world
        .resource_mut::<MarkdownDocuments>()
        .unwrap()
        .open(SAMPLE);
    let root = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        padding: 16.0,
        gap: 8.0,
        ..Layout::column()
    },));
    let bar = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Hug,
        gap: 6.0,
        ..Layout::row()
    },));
    set_parent(world, bar, root);
    let commands = [
        (1, "Bold"),
        (2, "Italic"),
        (3, "Code"),
        (4, "Strike"),
        (5, "H1"),
        (6, "H2"),
        (7, "List"),
        (8, "Quote"),
        (9, "Fence"),
        (10, "Callout"),
        (11, "Link"),
        (12, "Undo"),
        (13, "Redo"),
    ];
    for (id, label) in commands {
        Button::mount(
            world,
            bar,
            UiId(id),
            ButtonProps::new(label, HostIntent::Command(id)),
        )
        .expect("toolbar button");
    }
    let surface = MarkdownSurface::mount(
        world,
        root,
        UiId(100),
        MarkdownSurfaceProps::new(document, "Live markdown"),
    )
    .expect("surface");
    world.insert_resource(Host { surface });
    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = Some(surface.root);
    }
}

fn toolbar(world: &mut World) {
    let Some(host) = world.resource::<Host>().copied() else {
        return;
    };
    let intents = take_action_intents(world);
    for intent in intents {
        let Some(HostIntent::Command(id)) = host_intent(world, &intent) else {
            continue;
        };
        let command = match id {
            1 => MarkdownCommand::Strong,
            2 => MarkdownCommand::Emphasis,
            3 => MarkdownCommand::Code,
            4 => MarkdownCommand::Strike,
            5 => MarkdownCommand::Heading(1),
            6 => MarkdownCommand::Heading(2),
            7 => MarkdownCommand::Bullet,
            8 => MarkdownCommand::Quote,
            9 => MarkdownCommand::CodeFence,
            10 => MarkdownCommand::Liquid {
                kind: "callout".into(),
            },
            11 => MarkdownCommand::Link("https://example.com".into()),
            12 => MarkdownCommand::Undo,
            13 => MarkdownCommand::Redo,
            _ => continue,
        };
        let _ = host.surface.command(world, command);
    }
}
