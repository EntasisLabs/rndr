# rndr_liquid

Liquid Markdown for rndr: paste-first UI blocks written as ordinary fenced
code. A fence whose language names a component (` ```card `, ` ```chart `,
` ```plan `, ` ```tabs `, …) and whose body is forgiving `key: value` text,
`---`-separated items, or a GFM table becomes a typed block. Anything that
does not parse stays a code fence, so documents always degrade to readable
Markdown.

This crate is the headless grammar only: no Markdown engine, renderer,
navigation or network access. Intents, feed ids, image paths and URLs are
carried as data for the host to act on.

```rust
use rndr_liquid::{LiquidBlock, LiquidDocument, Segment};

let doc = LiquidDocument::parse("```callout\ntone: tip\nbody: Ship it\n```\n");
for segment in doc.segments() {
    match segment {
        Segment::Markdown(md) => { /* hand to the Markdown renderer */ }
        Segment::Block(i) => match &doc.blocks[i] {
            LiquidBlock::Callout(c) => assert_eq!(c.body, "Ship it"),
            other => unreachable!("{}", other.kind()),
        },
    }
}
```

## Fences

`card` `carousel` `actions` (`action_row`) `callout` `section` `block`
`chips` (`chip_group`) `media` `feed` `cite` `compare` `plan` `timeline`
`shortlist` `decision` `brief` `dashboard` `chart` `report` `slides` `tabs`
`steps` `recipe` (`procedure`) `accordion` `code` `tree` `kanban`

Charts cover `bar`, `line`, `area`, `pie`, `donut`, `radar`, `radial`,
`scatter`, `combo` and `heatmap`. Inline `{{icon:name}}` shortcodes are
limited to [`ICON_ALLOWLIST`](src/icons.rs) and canonicalized.

## Document model

`LiquidDocument::parse` resolves fences innermost-first, so a ` ```chart `
inside a ` ```report ` becomes its own block and the report body holds a
reference to it. Each block is replaced by a line `\u{E001}N\u{E001}`;
`segments` splits Markdown (including block bodies) into text and block
references. Non-Liquid documentation fences are left untouched, so
` ```markdown ` examples of Liquid stay literal.

Every block serializes with serde as `{"kind": …, "props": {…}}` using the
same camelCase property names as the reference TypeScript implementation.
`tests/medousa_golden.rs` checks parity against that implementation's output.
