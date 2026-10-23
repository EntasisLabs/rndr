//! Streamed markdown answer.
//!
//! A background feed sends a canned answer a few characters at a time.
//! The view appends each chunk and stays pinned to the bottom until the
//! pointer scrolls up. A link click writes the destination into the status line.
//!
//! ```text
//! cargo run -p stream
//! ```

use std::sync::Arc;
use std::time::Duration;

use rndr_core::window::WindowConfig;
use rndr_core::{
    events, feed, markdown_view, set_parent, theme_of, App, Axis, Entity, Event, FeedConfig, Label,
    Layout, LinkActivated, LogicalSize, MarkdownPlugin, MarkdownView, Schedule, Size, TextFace,
    Theme,
};
use rndr_paint::LatinFace;
use rndr_render::run;

const ANSWER: &str = "\
# Answer

The daemon finished the pass. **Weights** stayed put, *routing* moved, and the `palette` filter is warm.

See [the notes](https://example.com/notes) for the source.

- open the session
  - keep the pin
  - leave the prefix
- copy the selection

> The quote stays on its own line while the tail is still arriving.

---

```
fn main() {
    let n = 1;
}
```

The first section is committed once a blank line closes it.


The rest keeps appending. A longer answer keeps the viewport full so the pin has somewhere to go.

- [x] parse the prefix
- [ ] open a browser

Tail.
";

/// One slice of the canned answer.
struct Token(String);

impl Event for Token {}

struct Stream {
    view: Entity,
    status: Entity,
    href: Option<String>,
}

fn main() {
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "stream".to_string(),
        size: LogicalSize::new(760.0, 640.0),
        ..WindowConfig::default()
    });
    app.world_mut()
        .insert_resource(TextFace(Arc::new(LatinFace)));
    app.world_mut().insert_resource(Theme::default());
    app.add_plugin(MarkdownPlugin);
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, pump);
    // Held across `run`. Dropping it tells the producer to stop.
    let _feed = feed::spawn_thread(app.world_mut(), FeedConfig::queue(32), |tx| {
        let mut rest = ANSWER;
        while !rest.is_empty() {
            if tx.is_cancelled() {
                return;
            }
            let (chunk, tail) = take_chunk(rest);
            if tx.send(Token(chunk.to_string())).is_err() {
                return;
            }
            rest = tail;
            std::thread::sleep(Duration::from_millis(36));
        }
    });
    run(app);
}

fn setup(world: &mut rndr_core::World) {
    let theme = theme_of(world);
    let root = world.spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        theme.pane,
    ));
    let status = world.spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 8.0,
            width: Size::Fill,
            height: Size::Px(36.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        theme.body,
        Label::new("Streaming").with_color(theme.text),
    ));
    let view = markdown_view(world, "");
    set_parent(world, status, root);
    set_parent(world, view, root);
    world.insert_resource(Stream {
        view,
        status,
        href: None,
    });
}

fn pump(world: &mut rndr_core::World) {
    let chunks: Vec<String> = events::<Token>(world)
        .iter()
        .map(|token| token.0.clone())
        .collect();
    let href = events::<LinkActivated>(world)
        .last()
        .map(|link| link.href.clone());
    let Some(stream) = world.resource::<Stream>() else {
        return;
    };
    let view = stream.view;
    let status = stream.status;
    if let Some(href) = href {
        if let Some(stream) = world.resource_mut::<Stream>() {
            stream.href = Some(href);
        }
    }
    for chunk in &chunks {
        if let Some(doc) = world.get_mut::<MarkdownView>(view) {
            doc.push_str(chunk);
        }
    }
    let (source_len, shown) = {
        let doc = world.get::<MarkdownView>(view);
        let href = world
            .resource::<Stream>()
            .and_then(|stream| stream.href.clone());
        let len = doc.map(|doc| doc.source().len()).unwrap_or(0);
        (len, href)
    };
    let text = if let Some(href) = shown {
        href
    } else if source_len >= ANSWER.len() {
        "Done".to_string()
    } else {
        "Streaming".to_string()
    };
    if let Some(label) = world.get_mut::<Label>(status) {
        if label.text != text {
            label.text = text;
        }
    }
}

fn take_chunk(rest: &str) -> (&str, &str) {
    let mut end = 4.min(rest.len());
    while end > 0 && !rest.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        end = rest.chars().next().map(|ch| ch.len_utf8()).unwrap_or(0);
    }
    rest.split_at(end)
}
