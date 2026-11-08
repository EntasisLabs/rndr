# Liquid view

Ordinary **Markdown** flows around native blocks. Every block below is parsed by `rndr_liquid` and built from `rndr_ui` parts.

```callout
tone: tip
title: Host-owned behavior
body: Buttons never act on their own. Each one sends a `LiquidEvent` to the app's **LiquidHost**.
```

```actions
Approve | approve
Request changes | changes
```

```dashboard
title: Service health
columns: 3

---
label: Uptime
value: 99.9%
feed: ops
field: uptime
---
label: p95 latency
value: 182
unit: ms
delta: -12 ms
---
label: Open incidents
value: 1
tone: warn
```

```chart
type: bar
title: Weekly active developers

| Week | Alpha | Beta |
| ---- | ----- | ---- |
| W1   | 186   | 142  |
| W2   | 305   | 198  |
| W3   | 237   | 260  |
| W4   | 290   | 221  |
```

```tabs
title: Setup paths
default: Cloud

---
label: Local
body: Run `cargo run -p liquid-view` on any desktop.
---
label: Cloud
body: Export a PNG with `--export` on a headless runner.
```

```accordion
title: FAQ

---
label: Does the parser depend on UI code?
body: No. `rndr_liquid` is headless; this crate adds the views.
open: true
---
label: Who loads images and feeds?
body: The host, through `LiquidHost::image` and `LiquidHost::feed`.
```

```code
lang: rust
title: main.rs
---
let view = liquid_view(world, source);
dispatch(world, &request);
```

```kanban
## Todo
- [ ] Canvas model
## Doing
- [ ] Liquid views
## Done
- [x] Charts
```

```feed
id: releases
datatype: md
title: Releases
```
