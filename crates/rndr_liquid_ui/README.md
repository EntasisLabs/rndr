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
| `image(src)` | `media`, `card` and `plan` images; `None` shows alt text |
| `feed(id, datatype)` | `feed` rows, on mount and after `refresh` |
| `tile(feed, field)` | live values for feed-bound `dashboard` tiles |

Intents, URLs and image sources are untrusted document text. Host slots are
for trusted application content, so render documents you would also let
mount controls.

## Block state

Tabs, accordions, carousels, slides and checklists (`steps`, `recipe`,
`kanban`) keep their state in the view and survive `refresh`.
`set_liquid_source` rebuilds every block and resets that state. Checklist
state is not written back to the source.

## Current limits

- Icons (`{{icon:id}}`) and emoji fields are dropped; there are no icon glyphs yet.
- Slide layers, backgrounds and motion are not drawn; slides show their
  label, body and notes with previous/next.
- Styled `block` fonts, sizes and alignment are not applied yet.
- Chart tooltips are shown as a text row under the plot.

## Example

```sh
cargo run -p liquid-view
cargo run -p liquid-view -- --export liquid.png
```
