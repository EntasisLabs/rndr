# Contributing to rndr

Optimize for the next person understanding a change, not for minimum line count.
Keep each slice reviewable: characterize behavior, extract a coherent operation,
verify its callers, then reorganize modules separately where practical. Do not
mix a deliberate behavior fix into a supposedly equivalent refactor.

## Names and boundaries

- Name domain roles (`world`, `selection`, `block_index`, `viewport_height`). Tiny
  iterator bindings and mathematical coordinates can stay short; long-lived
  single-letter state should not require a reader to reconstruct its meaning.
- Split mixed responsibilities, not every long function. Exhaustive operation
  matches and adjacent GPU descriptors can be clearer together.
- Use private semantic enums for choices such as axis and edit kind, rather
  than boolean pairs. Distinguish **handled**, **unhandled**, and **refused** input.
  A consumed no-op must not fall through into another interpreter.
- Prefer helpers owned by a domain module to a universal `helpers`/`validate` bag.
  Share mechanisms only after testing that the consumers' contracts agree.

## Mutation and lifecycle rules

- Source sessions and structured trees are separate document authorities. Their
  disposable `Buffer` projections and scratch histories are not another undo
  authority. Do not route structured edits through a source session.
- Keep readonly, composition, backpressure, revision and incarnation checks at
  their owner's mutation boundary. Successful lookup is not durable permission.
  `SourceSession::admit` already owns source admission; do not add a rival gate.
- Validate before side effects, preserving existing refusal precedence. Test the
  allowed effects of each refusal rather than assuming all refusals are alike.
- A private lifecycle helper can resolve a keyed child, but the component still
  owns family conflict policy, stale handles, borrowed children, detached popup
  retirement, focus and interaction cleanup. See `rndr_ui/src/lifecycle.rs`.
- Preserve publication points. Table/inspector callbacks observe the previous
  published model; virtual-list renderers observe the newly published model.
  See the reconciliation contract tests, not an imagined universal policy.
- Preserve input precedence: history, table, node selection, structural text,
  vertical motion, ordinary scratch interpretation, then typed publication.
  See `rndr_core/src/structured_native.rs` and its dispatch characterizations.
- Byte offsets, grapheme boundaries, structural IDs, and ECS identities are not
  interchangeable. Preserve directed selection and affinity through edits.

## Navigation and dependency direction

| Owner | Responsibility |
| --- | --- |
| `rndr_core::editing` | Source authority, admission, revisions/history |
| `rndr_core::structured` | Structured schema, typed transactions/history |
| `rndr_core::structured_native` | Attachment, projection, input translation |
| `rndr_ui::data` | Public table/inspector facade over private implementation |
| `rndr_ui::lifecycle` (private) | Shared retained lookup/admission mechanics |
| `rndr_paint` | Read-only world-to-paint projection and display operations |
| `rndr_render` | Native shaping/GPU realization and caches |

UI can depend on core; core must not depend on UI or renderer. Keep new helpers
private until genuine public reuse and a stable contract justify exposure.
Keep established public paths, feature-off builds and facade exports working
when splitting files. Do not relocate tests merely to claim shorter functions.

## Review and verification

Before extraction, identify callers and record relevant invariants: refusal
order, IDs, no-op history/revision behavior, event consumption, callback timing,
composition, readonly, selection, focus, and publication. Add missing tests and
run them against the original implementation first. Prefer exact assertions to
only checking a green test count.

For a core/UI maintainability slice, use the existing gates:

```sh
cargo fmt --all --check
git diff --check
cargo test -p rndr_core -p rndr_ui -p rndr_paint -p rndr_render
cargo test -p source-session
cargo clippy -p rndr_core -p rndr_ui -p rndr_paint -p rndr_render -p source-session --lib --bins -- -D warnings
cargo check --workspace
cargo check -p rndr_core -p rndr_ui -p rndr_paint --no-default-features
cargo test -p rndr --no-default-features --test editing_compatibility
cargo test -p rndr --no-default-features --features ui --test editing_compatibility
RUSTDOCFLAGS='-D warnings' cargo doc -p rndr_core -p rndr_ui --no-deps
```

These are local slice gates, not a replacement for the broader cross-platform,
all-target/all-feature matrix in `.github/workflows/ci.yml`. Run affected focused
tests first. When changing construction/projection, compare paint/accessibility
and pixels against an independently compiled immutable baseline. When changing
allocation, virtualization, snapshotting or shaping, retain the associated cost
probes; correctness tests alone do not establish performance equivalence. Do not
claim native IME/clipboard/AT certification from headless/offscreen evidence.

Complexity and argument-count warnings are review signals, not arbitrary line
limits. Trial them locally before adopting scoped warnings. Record existing debt
without adding global lint floods or broad `allow` attributes to get green CI.
Do not weaken existing strict Clippy gates. Report exact commands, outcomes,
ignored probes and unresolved platform/performance limits with the change.
