# rndr_liquid_ui

Native rendering of [Liquid Markdown](../rndr_liquid) in rndr markdown views.

`liquid_view` parses Liquid source with `rndr_liquid`, shows the ordinary
markdown with `rndr_core`'s markdown view, and mounts each Liquid block in a
markdown host slot, built from `rndr_ui` cards, callouts, buttons and tabs.
Charts are painted by `rndr_chart` in a paint surface.

```rust
use rndr_core::*;
use rndr_liquid_ui::{dispatch, liquid_view, LiquidUiPlugin, NoHost};

let mut app = App::new();
app.add_plugin(rndr_ui::UiPlugin);
app.add_plugin(MarkdownPlugin);
app.add_plugin(LiquidUiPlugin::new(NoHost)); // after MarkdownPlugin
let view = liquid_view(app.world_mut(), "```callout\ntone: tip\nbody: Hi\n```\n");
app.add_system(Schedule::Update, |world: &mut World| {
    for request in take_action_intents(world) {
        if !dispatch(world, &request) {
            // not from a Liquid block: your own controls
        }
    }
});
```

## Host contract

The view never acts on document text. Everything that leaves it goes through
a `LiquidHost`, and every method defaults to doing nothing:

| Method | Called for |
| --- | --- |
| `event(LiquidEvent::Action)` | `actions` and recipe buttons, with the document's `intent` |
| `event(LiquidEvent::OpenLink)` | `cite` and `brief` source links |
| `event(LiquidEvent::Copy)` | the copy button of `code` blocks |
| `event(LiquidEvent::StartTimer)` | recipe steps with a duration |
| `event(LiquidEvent::RefreshFeed)` | the refresh button of `feed` blocks |
| `load_feed(request)` | async `feed` and dashboard-tile loads; `Pending` until ready or failed |
| `image(src)` | `media`, `card`, `plan` and slide-layer images; `None` shows alt text |
| `feed(id, datatype)` | sync `feed` rows when `load_feed` defers |
| `tile(feed, field)` | sync values for feed-bound `dashboard` tiles when `load_feed` defers |

Intents, URLs and image sources are untrusted document text. Host slots are
for trusted application content, so render documents you would also let
mount controls.

## Block state

Tabs, accordions, carousels, slides and checklists (`steps`, `recipe`,
`kanban`) keep their state in the view and survive `refresh`.
The same state is a `LiquidComponentRecord` keyed by the view and the
block's source byte, so it is still there while recycling has released the
block and after `push_liquid_str` appends more source. `subscribe_liquid_interactions`
receives a `LiquidInteractionEnvelope` for each reader change (`select`,
`expand`, `collapse`, `edit`). `put_liquid_component_state` replaces a record
only when its expected revision matches.

Recipe steps with a duration have a timer: Start, Pause, Resume and Reset.
The timer is a `LiquidTimerRecord` on the same source key, so it keeps running
or stays paused while the recipe is scrolled offscreen. Start, pause, reset
and completion are `LiquidInteractionEnvelope` events (`timer_start`,
`timer_pause`, `timer_reset`, `timer_complete`). Start still also reports
`LiquidEvent::StartTimer`.
`load_feed` is the async path for `md`, `text`, `json`, `csv`, and `image`
feeds. `Pending` shows a loading line. A later failure keeps the last ready
body and shows the error beside it. Dashboard tiles with a `feed` poll on
an interval (15s unless `interval:` says otherwise) and can be refreshed
by hand. Those records use the same source-keyed component state as the
other blocks, and a reload publishes `feed_refresh`.

`push_liquid_str` appends source and reparses only the open tail, so a
finished block is built once and keeps that state. `set_liquid_source` does
the same when the new source extends the current one, and on any other
replacement it keeps a block whose parsed source is unchanged. Checklist
state is not written back to the source.

An unclosed Liquid fence is a pending skeleton (`Pending callout`, and so
on) until the closing line arrives. It is not shown as a code block.
Pushing the fence into the markdown view directly, instead of through
`push_liquid_str`, still leaves it as code: the markdown view does not know
about Liquid.

## Visual text and overlays

A styled `block` sets the nested prose's font (`sans-serif`, `serif` or
`monospace`), size, alignment and line spacing. `justify` uses the start
edge; the text engine aligns start, center and end.

A `code` block colors keywords, strings, numbers and comments, and exposes
the source as a code role named by its language or title. Fenced markdown
outside Liquid still colors only when `rndr_core`'s `highlight` feature is on.

A chart tooltip floats over the plot and describes it. It stays hidden until
the pointer hits a point, so it does not reserve a row under the plot.

Slide image layers are pinned in a 640×360 point stage. `x`, `y`, `w` and
`h` are fractions of that stage from 0 to 1, and each layer is an image
named by its id.

## Current limits

- Icons (`{{icon:id}}`) and emoji fields are dropped; there are no icon glyphs yet.
- Slide washes, scrims and motion are not drawn. A slide with no layers still
  shows its label, body and notes with previous/next.
- Card detail (summary, points, chips and meta) stays on the face. A detail
  sheet is not opened.

## Example

```sh
cargo run -p liquid-view
cargo run -p liquid-view -- --export liquid.png
```
