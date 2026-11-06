# Working in rndr

rndr is a native desktop application toolkit for Rust, built around an
application-owned world. Correctness, clear ownership, tasteful usable defaults,
and understandable internals make that world dependable. Progress means a
verified outcome integrated into the application—not activity, code volume,
token savings, or the number of agents involved.

This guide applies throughout the repository and works with a single contributor
or a coordinated crew, independently of agent runtime. The engineering loop is:

**Understand → decompose → allocate deliberately → implement → integrate → verify → reconcile.**

## Repository orientation

- [README.md](README.md): product identity, onboarding, architecture and current boundaries.
- [CONTRIBUTING.md](CONTRIBUTING.md): detailed mutation/lifecycle contracts,
  dependency direction, refactoring practices and verification commands.
- [.github/workflows/ci.yml](.github/workflows/ci.yml): cross-platform build and test matrix.
- [crates/rndr_ui/README.md](crates/rndr_ui/README.md): semantic UI composition.
- [docs/design/](docs/design/): design rationale and implementation plans;
  current source and tests establish what is implemented.

The workspace separates core state and editing (`rndr_core`), semantic composition
(`rndr_ui`), read-only paint projection (`rndr_paint`), native rendering
(`rndr_render`), and the application facade (`rndr`). Public paths, optional
features and headless use are contracts alongside desktop behavior.

## Outcomes and ownership

Work starts with the requested outcome, acceptance evidence and relevant
constraints. Reading follows the behavior through its implementation, callers,
mutation owner and tests. Existing uncommitted work remains intact; changes stay
within the authorized scope and workspace. Destructive operations, publishing,
and integration into another branch require their corresponding authorization.

Each state transition has an authoritative owner. Admission, revision, lifecycle
and publication checks belong at that owner's boundary. A convenient projection,
lookup or helper carries neither independent mutation authority nor durable
permission. Source-backed and structured documents retain distinct authorities
and histories; their scratch projections remain disposable.

A coherent change resolves the requested behavior across affected boundaries.
Smallest means the smallest **complete** solution, rather than the fewest lines
or a broad cleanup. Refactors preserve observable behavior; intentional behavior
changes receive explicit scope and evidence of their own.

An abstraction earns its place by reducing the knowledge needed to make a correct
change. Domain names, explicit phases and private semantic types make ownership
visible. Shared mechanisms retain each consumer's policy, ordering and publication
contract. New public APIs carry a compatibility obligation and need a genuine use.

## Work as a dependency graph

For nontrivial work, a lightweight plan identifies coherent results, dependencies,
shared interfaces, uncertain assumptions, and integration/verification tasks.
A work unit has an owner and a testable result; a file list is only a boundary.
Small changes can keep this reasoning brief. Discoveries and failed checks update
the plan, including dependencies that were initially unknown.

For a planned directed acyclic work graph, let `w(v)` be estimated effort,
`W = sum(w(v))` total work, and `L` the longest dependency-path effort. With `n`
identical workers and ideal scheduling:

```text
T(n) >= max(W / n, L)
```

This is a lower bound under idealized assumptions, not a delivery forecast.
Graph width measures structural concurrency; currently ready work, interface
stability and interference determine dispatchable work. Structural capacity and
economically useful concurrency are different quantities.

Parallel work earns its coordination cost when independent progress exceeds
context transfer, contention, integration and verification overhead. Shared
mutation owners, unstable interfaces and tightly ordered changes favor sequential
implementation. Independent contract review, test investigation or isolated
implementation slices can benefit from concurrency. A critical-path bottleneck
usually needs attention to its dependency or design, rather than a larger crew.

Allocation uses the smallest crew justified by the ready, sufficiently independent
work. More workers become useful as boundaries stabilize or new independent work
appears; contention or rework is evidence to reduce concurrency or repartition.
Estimates stay labeled as estimates. Measured duration, compute use, rework and
human intervention inform later decisions; complexity scores acquire cost meaning
through observed calibration, rather than invented numerical precision.

## Handoffs and integration

Each delegated slice carries a compact contract:

| Field | Required clarity |
| --- | --- |
| Outcome | Result and acceptance criteria |
| Ownership | Semantic responsibility, allowed paths and shared interfaces |
| Dependencies | Ready inputs, assumptions and blockers |
| Verification | Checks and evidence expected with the result |
| Return | Changes, findings, uncertainty and conditions needing coordination |

Concurrent mutation has explicit ownership; shared-file or interface changes
are coordinated before overlapping edits. A newly discovered cross-owner change
returns to coordination for an agreed scope. The integrating contributor retains
responsibility for the whole outcome, including compatibility between slices.

A worker's completion report is input to integration, rather than acceptance of
the undertaking. Combined diff review and cross-boundary verification establish
completion. Independent review has a specific question or risk to examine. Where
delegation is unavailable, the same decomposition guides sequential execution.

## Evidence and completion

Verification follows the affected contracts, beginning with focused tests and
expanding to the relevant contributor-guide and CI gates. Behavior-preserving
extractions benefit from characterization tests run before and after the change.
Refusal order, consumed no-ops, history/revision effects, callback timing,
composition, selection, focus and publication can be observable behavior.

UI work includes visual evidence where appearance is part of the outcome.
Performance claims need relevant cost probes or measurements. Headless checks,
pixels and native interaction tests establish different things; IME, clipboard
and assistive-technology claims remain bounded by actual platform testing.
Existing strict gates remain meaningful: scoped fixes or clearly reported debt
preserve their signal, while blanket suppressions obscure it.

Completion includes review of the final diff, appropriate combined verification,
relevant documentation, and separation of intended changes from generated
artifacts. Existing work and unrelated files remain preserved. Commits, pushes
and merges follow the requested workflow and authorization.

The final report distinguishes implemented results, exact checks and outcomes,
unrun or blocked checks, and remaining risks. A concrete blocker includes what
is missing and the decision or resource needed to proceed. Verified, accepted
work—not a handoff, a green test count alone, or apparent busyness—is the unit
of progress.
