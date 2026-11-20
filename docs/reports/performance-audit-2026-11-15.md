# RNDR interaction performance: initial audit

Baseline: `8ad9688ce472bd25755bb5f27d5854e66ed0e83d`. Apple M4, arm64,
Rust 1.100.0. Release build. The initial investigation below predates runtime
changes; the implementation follow-up at the end records the approved changes
and final measurements.

## Reproduction and scope

```sh
cargo test -p rndr_paint --release --test frame_scale_cost -- --include-ignored --nocapture --test-threads=1
cargo test -p rndr_core --release --test editor_scale_cost -- --ignored --nocapture
```

The new probe retains ordinary fixed-height labels beneath a scrollport. It
compares 100/1,000/5,000 rows and a hidden content subtree. Three warmup frames,
41 samples per operation; p50 is sorted index 20 and p95 index 38. The fixture
contract verifies that 100 and 1,000 retained rows produce identical rounded
quad snapshots, while the hidden fixture retains its labels and paints nothing.
This is deliberately **not** the virtual-list path.

These are fallback-engine, headless CPU measurements. They exclude Parley,
application systems, native input dispatch, accessibility synchronization, GPU
submission, surface acquisition and actual display latency. Sample percentiles
are local characterizations, not statistical performance guarantees. Different
phases are measured separately, so subphase medians are not additive to update.

## Results

Median times, milliseconds:

| Retained rows | Hidden | App update | Layout alone | Paint alone | Hit test | Text preparation alone | Quads |
| ---: | :---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | no | 0.137 | 0.086 | 0.093 | 0.014 | 0.048 | 881 |
| 1,000 | no | 1.288 | 0.755 | 0.842 | 0.126 | 0.450 | 881 |
| 5,000 | no | 6.463 | 3.926 | 5.066 | 0.639 | 2.348 | 881 |
| 100 | yes | 0.055 | 0.005 | 0.001 | 0.0002 | 0.045 | 0 |
| 1,000 | yes | 0.519 | 0.033 | 0.001 | 0.0002 | 0.443 | 0 |
| 5,000 | yes | 2.652 | 0.173 | 0.001 | 0.0002 | 2.260 | 0 |

At 5,000 visible-tree rows, update p95 was 6.842 ms and paint p95 5.270 ms.
This is CPU cost despite unchanged, screenful-sized output; it is not a claim
that real applications retain 5,000 rows or that their frame time is this sum.

Existing editor probe, repeated-paragraph source, fallback engine:

| Source bytes | Scroll/update p50, µs | Edit + layout p50, µs | Resize p50, µs |
| ---: | ---: | ---: | ---: |
| 10,260 | 12 | 15 | 72 |
| 1,048,629 | 11 | 207 | 1,709 |
| 10,485,777 | 10 | 2,114 | 16,548 |

The existing probe's resize sample runs **two** frames; its cold sample runs
three. Scroll checks that paragraph assembly and shaped-byte counters stay
unchanged. The repeated text does not characterize unique-line shaping; use
`editor_scale_unique` and the Parley-backed renderer probe before making native
text-engine decisions. These results do not establish the precise cause of edit
or resize scaling.

## Findings and recommended order

1. **Hidden text preparation.** `font::apply_text_layout` enumerates all labels;
   `shape_labels` prepares styles, clones source/spans and calculates keys even
   for a hidden ancestor. A cache hit avoids rebuilding the final layout, not
   this preparation. The hidden probe attributes most remaining update cost to
   this pass. First implementation candidate: make presentation preparation
   visibility-aware, preserving source/session updates independently. Acceptance
   needs ancestor hiding, edits while hidden, first unhide, resize/scale/font
   changes, IME, structured projections and atlas-cache lifetime tests. Do not
   merely skip all hidden document lifecycle work.

2. **Offscreen traversal and host virtualization.** Layout measures, refines
   widths and places the retained tree every full update. Paint clips output but
   still visits ordinary descendants; hit testing walks them in paint order.
   This explains work growing while output stays fixed. Use the existing virtual
   list/tree facilities for large host lists before inventing another API.
   Toolkit culling must distinguish an ancestor clip from a parent's own rect:
   overflowing children, transforms, rounded/effect groups, overlays and surface
   slots make naive rect-based subtree rejection incorrect. Verify equivalent
   paint/display operations, accessibility and hit results before cost claims.

3. **Transient traversal/query allocations.** `World::query`, `entities_with`,
   `children_of`, `shown_children` and `flex::paint_order` produce owned vectors;
   ordering is recomputed, and layout queries its roots twice. This is confirmed
   source structure, not a measured allocation profile. Start with immutable
   borrowed traversal/private fast paths, preserving mutation snapshots,
   generational filtering and stable sticky/absolute ordering. Do not change the
   public owned-query contract or add an invalidation cache without evidence.

4. **Native frame observability.** `Runner::redraw_frame` crosses input/clipboard
   synchronization, update/paint, atlas/image ingestion and GPU presentation.
   `Live` already has an update/layout bypass; do not duplicate that mechanism.
   Before broad dirty-subtree layout, instrument input receipt, redraw start,
   application update, layout/text, paint, atlas upload, surface acquire and
   submit/present separately. CPU present-call completion is not physical
   display presentation. Native p50/p95/p99 traces on actual Chat/Notes/Code
   interactions are still required to identify remaining perceived hitches.

5. **Large-document edit/resize follow-up.** Existing scrolling behavior is
   already bounded in this fixture. The source-size-dependent edit/resize costs
   justify profiling revision-driven source snapshots, paragraph indexes and
   width invalidation. Preserve source admission, revision/history, composition
   and event backpressure contracts; do not bypass them for a local speedup.

Broad dirty-subtree layout is a later architectural project: public raw
component mutation, topology, viewport callbacks, wrapping, popup anchors and
font/scale changes all affect invalidation. The measured, narrower candidates
above should come first. No renderer rewrite or toolkit speedup is claimed by
this audit.

## Verification of the audit additions

- Release frame probe and fixture contract: 2 passed, explicitly including the
  normally ignored timing probe, serial execution.
- Existing release core editor-scale probe: 1 passed.
- `cargo test -p rndr_paint`: passed (ordinary timing probes remain ignored).
- `cargo clippy -p rndr_paint --test frame_scale_cost -- -D warnings`: passed.
- `cargo fmt --all --check` and `git diff --check`: passed.

Cargo emitted existing manifest warnings (unused workspace dependencies and
redundant homepage metadata); no lint suppression or manifest cleanup was made.
No production behavior was changed. Native/GPU/Parley measurements, full-workspace
and cross-platform gates were not run for this audit-only addition.

## Implemented follow-up

The approved first pass is implemented, not committed. It preserves public
owned query/snapshot APIs and does not add broad layout invalidation caching.

- Hidden labels/ancestors defer text presentation preparation. Old TextLayout
  and LayoutEngine projections are discarded once, forcing safe engine/atlas
  revalidation on unhide. Source-session synchronization, live values and undo
  remain active. Regression tests cover edits/IME while hidden, replacement
  engines, width/style/scale changes, source-session undo, and fresh layout equality.
- Paint skips only ordinary entity fill/text work when an explicit clip has
  nonpositive area. It still visits descendants and independent display paths.
  Gradient/shadow/backdrop/structured-gutter entities keep the original path.
  Exact quad/display-operation characterization covers overflow, effects,
  transforms, missing clips and independently clipped children. This is not
  viewport-box or subtree culling.
- Private borrowed hit traversal avoids child snapshots/sorting for ordinary
  insertion order; sticky/absolute children keep the established stable ordered
  snapshot fallback. Dead generational IDs remain filtered. Layout also reuses
  its root snapshot rather than querying it twice.
- Opt-in bounded [CPU frame timings](../performance/frame-timings.md) separate
  update/layout/text, paint, ingestion, surface acquire, upload/encode and
  submit/present. Native samples distinguish failed presentation, live-only
  updates, and no-input frames. The oldest input receipt survives failed frames.
  Collection is disabled unless the application inserts FrameTimings; there is
  no automatic logging, listener or document/input-content capture.

Final serial release characterization (same probe/host; separate invocations,
not paired statistical trials), median milliseconds:

| Workload | Initial baseline | Final | Interpretation |
| --- | ---: | ---: | --- |
| 5,000 retained rows, visible paint | 5.066 | 1.804 | about 64% less CPU paint time |
| 5,000 retained rows, hidden update | 2.652 | 0.485 | about 82% less CPU update time |
| 5,000 retained rows, hidden text preparation | 2.260 | 0.393 | about 83% less CPU preparation time |
| 5,000 retained rows, visible update | 6.463 | 6.389 | essentially unchanged; full layout remains |

Visible paint p95 was 1.935 ms; hidden update p95 was 0.493 ms. Output remains
881 visible quads (zero for hidden content). These are still fallback-engine,
headless CPU results, not Parley/GPU/live input-to-display claims.

The new serial allocation characterization:

```sh
cargo test -p rndr_core --test traversal_allocations -- --ignored --nocapture --test-threads=1
```

A nested 1,000-row hit traversal went from 1,011 to 1 counted allocation calls
(alloc/zeroed/realloc). The remaining allocation is the root snapshot. This is
allocation count, not allocated bytes or a guarantee of faster wall time; flat
hit traversal remains approximately 0.66 ms at 5,000 rows in this sample.

Verification: combined core/UI/paint/renderer suite 1,478 passed, 12 opt-in tests
ignored; the release frame probe and allocation probe were explicitly run and
passed. The added paint timing boundary test also passes. Source-session tests,
headless core/UI/paint checks, both facade editing compatibility configurations,
workspace check, strict contributor Clippy, strict core/UI rustdoc, formatting
and diff whitespace checks passed. Native timing worker additionally verified
renderer default/observe/no-default-feature configurations and 108 renderer lib
tests. Existing Cargo manifest/dependency warnings remain unchanged.

Remaining work: measure a real application's native timing history with Parley
and representative retained/virtualized host lists. No native foreground latency
session or cross-platform runtime certification was performed. Large visible
retained-tree layout and text preparation still scale with row count; host
virtualization remains preferable to a speculative dirty-layout cache.
