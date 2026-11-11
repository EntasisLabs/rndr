# rndr-flow: architecture and migration-readiness analysis

**Status: review proposal, documentation only.** No crate, API, renderer fix, Medousa migration or native qualification is implemented or authorized by this document. Read the [implementation epic](rndr-flow-epic.md) for bounded follow-on work and gates. `rndr-flow` is a provisional package name; Rust would normally import `rndr_flow`.

## Decision in brief

Recommend an **optional companion crate for ownership and transitions**, after targeted correctness work in the existing owners. rndr already has a headless application world, cancellable tasks, epoch-checked feeds, retained semantic identities, hidden tabs, and asynchronous native close protection. It does not need a second component tree, a new state store, a mandatory executor, or a Rake clone.

The useful missing layer is coordination: who owns a task/subscription across route changes, whether a hidden document remains alive, which exact incarnation can accept a result, and when a guarded transition may irrevocably dispose resources. Liquid cleanup/identity must be fixed in Liquid itself. A flow crate cannot fix its reconciliation, canvas text editing/IME, or an HTML artifact security contract.

**Readiness recommendation:** proceed with a scoped foundation-hardening undertaking, then prove the existing application-side `apps/rndr-home` workflow. This review does **not** newly establish readiness to start chat/shell migration: Medousa source and its reproducible preview were unavailable within the admitted worktree. Full Tauri replacement has additional independent gates.

## 1. Baseline, freshness and evidence vocabulary

Reviewed on **2026-11-11 UTC** in Forge work `work-1d8e0e0accafa2c514b5becd2384e121`, isolated branch `worktree/rndr-flow-lifecycle-analysis-and-implementation-epic`.

- Exact rndr baseline: **`82707632302c67a88bd84220b9ca262f63a25197`**, version 0.5.0. Commit timestamp `2026-11-11T06:11:39-07:00`, subject `Merge pull request #148 from EntasisLabs/cursor/liquid-visual-fidelity-e145`.
- Initial `git status --short` was empty. `git ls-remote origin refs/heads/main` returned that same OID during this review. This is a point-in-time freshness check, not a continuing upstream guarantee. No fetch, pull, rebase, reset or baseline change was performed.
- All rndr paths and symbols below describe that baseline, not the later documentation commit. Pinned source root: [rndr at the reviewed revision](https://github.com/EntasisLabs/rndr/tree/82707632302c67a88bd84220b9ca262f63a25197).
- Rake reference: **`e03e0c09bfcf5ad3b271b8185280284a22823e05`**, resolved from the public GitHub commits API. Rake was read, not built or executed; its documented behavior is not independent runtime verification.
- The assignment supplied the previous migration review, including its claimed 126 rndr tests, 50 preview tests and old probe. Those are historical reports, **not** test receipts from this pass. No independent old review artifact or Medousa checkout exists in this isolated worktree; `apps/rndr-home` is not a workspace member or local app. External checkout reads were not attempted outside the admitted workspace. No Medousa file, lockfile or dependency was changed.

Labels used throughout:

- **Fact (source):** inspected implementation or explicit documentation contract; not necessarily exercised natively.
- **Fact (test/probe):** observed by a named automated command in §7.
- **Recommendation:** proposed architecture or policy, not existing public API.
- **Hypothesis/unverified:** needs a new repro, application checkout, platform session, or owner decision. Absence from inspected paths is not proof that no application has its own equivalent.

## 2. What rndr already supplies

| Existing owner/seam | Evidence at baseline | Consequence for flow design |
| --- | --- | --- |
| Application-owned world | `crates/rndr_core/src/world.rs`: `World`, components, resources and UI-thread locals; `app.rs`: `App`, `Plugin`, `Schedule`, `System = fn(&mut World)` | Store coordination metadata as a resource. Domain models and editing authorities stay where they are. Install an opt-in plugin rather than another runtime tree. |
| Frame publication | `app.rs` schedule contract; `event.rs`: `Events`, `send`, flip/merge | Input sends are merged before Update; Update/PostUpdate sends wait until the next frame. Guard result application at the consuming mutation boundary, not just when a worker queues it. |
| Generational entity identity | `entity.rs`: `Entity { index, generation }`; `world.rs`: `is_alive`, `despawn_entity`, dead insert refusal | Reject stale ECS handles already. Do not claim rndr has no identities. Entity identity alone cannot distinguish two requests for the same still-live document or a source replacement. Generations wrap; do not export an ECS index as a durable identifier. |
| Hierarchical destruction | `hierarchy.rs`: `despawn`; `world.rs`: `despawn_entity` removes stored components | Component Drop can release owned handles. World resources keyed by entity do not automatically disappear. Cascade destruction is not a generic domain teardown hook that can mutate arbitrary resources. |
| Task ownership by handle | `runtime.rs`: `TaskHandle::{cancel,detach}`, Drop; `spawn`, `spawn_local`, `spawn_detached`, `wrap_output`, `drain_inbox` | Drop cancels and suppresses an uncommitted result; keep handles deliberately. `spawn_detached` still returns a cancelling handle; the name is not an ownership policy. Flow should retain/group existing handles. |
| Executors and headless driver | `runtime.rs`: `Executor`, inline scheduling; `pool.rs`; `app.rs`: `set_executor`; `sim.rs`: `Sim`, seeded scheduling and manual clock | Use the existing injected executor and fake time. No Tokio requirement. Local futures may be non-Send; components/resources are Send + Sync, UI-thread locals are not. |
| Feeds and backpressure | `feed.rs`: `Feed`, `Sender`, `FeedConfig`, `Epoch`, `advance`, `send_at`, Drop/cancel; `tests/feed.rs` | Epoch rejection, bounded/latest policies, producer wakeup and cancellation already exist. Use `send_at` with a captured epoch, not a freshly read epoch at send time. A worker blocked in unrelated IO is not forcibly stopped. |
| Presentation visibility | `tabs.rs`: `Tabs`, `TabPanel`, `Hidden`; retained UI collections and `UiId` | Hidden skips layout/hit-testing/paint; it is not disposal. Visibility-specific task policy can be added without destroying the underlying model. |
| Semantic component reconciliation | `crates/rndr_ui/src/lifecycle.rs`: private keyed-child lookup/admission; collection/data `mount`/`unmount`; `CONTRIBUTING.md` lifecycle rules | Reuse public component APIs. Private shared lookup is not a new universal lifecycle API: family admission, focus, borrowed children and popup cleanup still belong to each component. |
| Native asynchronous safe close | `close.rs`: `CloseGuard`, `CloseRequest`, `CloseState`; `docs/safe-close.md`; runner implementation in `crates/rndr_render/src` | Already opt-in, coalesces repeated requests, rejects stale completions, retains state on failure, supports retry/cancel/informed discard. Do not introduce a competing native-close gate. Flow adds document/tab coordination and feeds this gate. |
| OS app lifecycle | `lifecycle.rs`: `UrlOpened`, `Reopened`; `examples/lifecycle` | These are OS delivery hooks, not a general mounted/disposed scope model. Host validates external links. |
| Starter application layer | `crates/rndr_app/src/lib.rs`; `examples/desktop` | Existing small form/navigation/preferences helpers are not a typed retained route/history engine. Compare placement here before accepting a new public crate. |
| Markdown host-slot identity | `markdown/host.rs`: `MarkdownSlotId`, `MarkdownSlot`, `mount_markdown_slot`, `MarkdownSlotHost` | IDs are caller-selected, scoped to a view and never reused; stale/replaced handles refuse. Anchored source ranges follow insertions above and streaming append. This existing identity is distinct from Liquid's resource-record key; preserve it rather than claiming all embedded UI lacks stable handles. |
| Authoritative editing | `editing.rs`: `SourceSession`; `source_projection.rs`; structured editing modules; `CONTRIBUTING.md` | Existing revision/incarnation/admission checks remain authoritative. Flow may coordinate a transition but never bypass them or own a rival undo history. |

### Task cancellation is real, but is not a complete application protocol

Source inspection and existing tests show cancellation checks both after awaiting a result and again while draining the runtime inbox. A result **already committed** into the typed event queues cannot be recalled by dropping a handle. A consumer may also receive external events not mediated by rndr tasks. The host must validate exact window/tab/document incarnation and request generation immediately before mutating its model.

A background `Job::poll` catches panics and marks the task cancelled. The documented local-future path catches a polling panic but does not necessarily mark `TaskHandle::is_cancelled`; it is not a uniform completion/error status API. Neither behavior implies save success, guard approval, durable cancellation, or a user-visible failure. Future diagnostics need explicit terminal outcomes; they must not reinterpret a missing result as successful completion.

Native safe close is stronger than a simple boolean veto: `docs/safe-close.md` describes draining all participating worlds, including hidden and newly admitted windows, before exit. Flow must preserve that contract. It cannot assume a single shared World across windows merely because Rake uses a shared runtime.

### Liquid has three different lifetimes today

1. **Mounted presentation:** `view.rs` `LiquidHooks::{slot_mounted,slot_unmounted}` and core `markdown/host.rs` host slots.
2. **Retained offscreen reader state:** `LiquidView.saved`, carried `BlockState`, timers/feed state. Recycling and streaming append intentionally preserve state; existing tests pass.
3. **World resource records/listeners:** `state.rs` `ComponentStore.records: HashMap<(Entity, usize), LiquidComponentRecord>` and `Listeners.handlers`. Records survive independently of the component tree; `subscribe_liquid_interactions` appends a boxed handler and returns no removal handle.

These are not interchangeable. Cleaning a virtualized slot must not reset a timer, while removing a semantic block or destroying its owning view needs a defined terminal cleanup. Core `markdown/host.rs` explicitly documents notification before recycling and that `MarkdownSlot::detach` itself does not notify the host. The current hook is not a complete terminal ownership protocol.

Another concrete adapter seam is `LiquidHost::load_feed` in `host.rs`, called by `feed.rs::poll`: `FeedRequest` carries feed ID, datatype, field and instance, while the host returns `Defer`, `Pending`, `Ready` or `Failed`. It does not carry the view/scope incarnation or a cancellation token. Existing revisioned feed-state update APIs/tests are useful, but do not establish ownership of host-started IO. A host adapter must preserve exact owner/request identity externally, or an upstream extension must supply it; do not assume flow can cancel opaque Pending work through the current request alone.

## 3. Revalidation ledger for the prior review

Categories describe each finding, not blanket application readiness. **Fixed** requires evidence for the old defect, not merely nearby improvements. No whole prior finding can be newly marked fixed by this pass.

| Prior finding | Classification now | Current evidence and bounded conclusion | Next owner/gate |
| --- | --- | --- | --- |
| Liquid mounted-surface cleanup | **Verified open** | Probe P1 (§7): opened accordion, replaced source with prose, then cascade-despawned view and pumped frames. One record remained queryable for each dead view across three cycles. Resources remain until World/App drop. `state.rs` writes records but has no record retirement path; hierarchy destruction removes components only. | Liquid owner: distinguish recycling, removal, replacement and terminal disposal; prune records at semantic ownership boundaries. |
| Stable Liquid identity/reconciliation, including replacement state | **Partially addressed; verified open** | Streaming append and recycling tests pass. `state.rs` keys by view + source byte; `set_liquid_source` uses parsed-block equality for carry. `reconcile` consults positional records before carry. Probe P2: change accordion label/body at the same source; `open: [0]` survives and the changed body renders expanded despite unequal parsed content. | Liquid owner: explicit durable semantic identity/reset policy, duplicate/reorder tests, source-key compatibility. State preservation itself is not always wrong; unspecified transfer between different semantic blocks is the risk. |
| Accumulating/stale state, tasks and subscriptions | **Partially addressed** | Task/feed Drop cancellation and feed stale-epoch tests pass; no global task leak was reproduced. P1 verifies retained Liquid records and one extra listener per repeated subscription (1/2/3), freed on App drop. World-global listeners may intentionally be app-owned; repeated per-view registration has no unsubscribe token. | Renderer/API cleanup plus flow ownership; do not extrapolate this probe to all tasks or all subscriptions. |
| Canvas append/end-delete editing | **Verified open** | `crates/rndr_canvas_ui/src/editor.rs`: `CanvasEditor::type_text` uses `String::pop`, newline push and `push_str`; no caret/selection/IME protocol in this path. README Limits agrees. End deletion is a Unicode scalar pop, not a full grapheme-editing contract. | Canvas editing track; not flow. No native interaction run here. |
| Unfinished host image import | **Verified open** | `crates/rndr_canvas_ui/README.md` Limits: textures are host-supplied in `CanvasView::images`, editor does not import images; `view.rs` owns `ImageIds`. Existing image rendering is not missing. | Host import/asset policy and canvas adapter; not a reason to add storage/transport to flow. |
| Source-preserving Live notes versus full Markdown + Liquid | **Partially addressed** | `source_projection.rs` and `docs/design/native-document-editing-e7.md` implement a deliberately small line grammar with exact byte-splice guarantees. Literal text, a single nonnested emphasis pair and task prefixes are supported; unsupported fences/lines are opaque reveal atoms. Cross-segment/structural rendered edits refuse. This is not full CommonMark, embedded Liquid editing, or absence of a real source editor. | Editing/host syntax track. Preserve source fallback, revision admission and existing history. |
| Restricted browser versus interactive HTML artifacts | **Partially addressed; artifact parity unverified** | `crates/rndr_browser/README.md`: optional/off by default, host-owned sessions/intents, guest cannot call app commands; script/find unsupported; native matrix unverified. Real navigation/show/hide/bounds/download/file-intent machinery exists. Medousa artifact contract not inspected here. | Host security/artifact track. Flow owns a UI lifetime, not script authority or a privileged bridge. |
| Icons, slide effects, card detail sheet | **Verified open** | `crates/rndr_liquid_ui/README.md` Current limits explicitly retains all three. `render.rs::plain` strips icon shortcodes; render path shows card detail on face and slide layers/navigation, not a detail sheet/effect engine. A recent visual-fidelity merge does not erase these documented limits. | Liquid visual work, lower priority than cleanup and identity. No screenshot/native visual certification here. |
| Native keyboard/IME/accessibility/DPI qualification | **Unverified** | Core input, editor/IME, accessibility and scale machinery exists. `docs/desktop-readiness.md`, `docs/design/native-document-editing-epic.md` and browser matrix separate automated evidence from native qualification. This pass ran only headless tests, not platform keyboard, IME, screen reader, clipboard or multi-scale sessions. | Native acceptance matrix with actual OS/device/input-method evidence. Do not label absent features based on absent certification. |
| Medousa preview dependency/lock/offline reproducibility | **Unverified** | Medousa checkout and `apps/rndr-home` unavailable in admitted worktree. Prior isolated refreshed-lock result was not rerun; current lock drift is not asserted as newly proven. rndr's own targeted `--locked --offline` tests succeeded, which does not prove a Medousa build. | Application owner obtains exact app commit, lock digest, dependency source/revision and reproducible offline preview evidence before migration. |

### Replacement identity: what is and is not proved

P2 proves state transfer to changed content at the same position, not a crash, a cross-chat leak, or an agreed intent to reset every content edit. `match_states` greedily matches equal parsed blocks with a used bitmap; duplicate/reordered blocks, edits that move source bytes, offscreen replacement, kind changes and remove/reinsert need a full compatibility matrix. Their precise misrouting is **hypothesis**, not a claimed repro. A lifetime scope cannot infer semantic block identity from source bytes; the renderer/parser and host must agree on identity and reset semantics.

## 4. Rake lessons, without changing rndr's identity

Pinned references (all at `e03e0c09bfcf5ad3b271b8185280284a22823e05`):

- [README](https://github.com/1bye/rake/blob/e03e0c09bfcf5ad3b271b8185280284a22823e05/README.md): typed routes, managed work, standalone roots, headless services and broader application framework.
- [Routing](https://github.com/1bye/rake/blob/e03e0c09bfcf5ad3b271b8185280284a22823e05/docs/routing.md): `Router`, `Navigator`, `TabHandle`, retained entries, exact targets resolved at submission, guard revisions, loaders and histories.
- [Guarded quit](https://github.com/1bye/rake/blob/e03e0c09bfcf5ad3b271b8185280284a22823e05/docs/guarded-quit.md): save/discard/cancel, revision revalidation, explicit wait/cancel work policies and irreversible stopping boundary.
- [Runtime control](https://github.com/1bye/rake/blob/e03e0c09bfcf5ad3b271b8185280284a22823e05/docs/runtime-control.md): shared typed operations for UI/CLI, operation/decision identities, bounded diagnostics and retention, stale-choice rejection. Control disabled by default. Transport does not become authority.
- [Support](https://github.com/1bye/rake/blob/e03e0c09bfcf5ad3b271b8185280284a22823e05/docs/support.md): alpha APIs; headless caller-supplied executor; macOS source apps versus unqualified Windows/Linux; browser GPUI canvas is not HTML layout. Keyboard/IME/AT/scale qualification remains broader work.

These are architectural references, not a dependency mandate or proof Rake would solve Medousa. Adopt the principles of exact ownership, retained-versus-disposed distinction, revision-checked decisions and shared typed operations. Do **not** import its GPUI tree, service bundle, router implementation, native close behavior, packaging/update tooling, control server or authentication assumptions. Rake's last-window/runtime behavior must not silently replace rndr's documented primary-window close contract. A guessed `docs/tasks.md` URL returned 404; no task semantics are attributed to that nonexistent document.

## 5. Recommended responsibility and dependency boundaries

| Concern | rndr renderer/core/UI | Proposed flow companion | Application/adapter |
| --- | --- | --- | --- |
| Layout, paint, widgets, focus, input, DPI, IME, accessibility | Authoritative | No replacement | Composition and platform acceptance |
| Entity/component reconciliation, Liquid records/timers and semantic block identity | Authoritative cleanup and identity contracts | Associates an owner; requests explicit disposal | Supplies semantic document IDs/reset intent |
| Domain model, text/source/structured editing and undo | Existing mutation owners | No mirror; stores only coordination metadata | Business model, save/recovery and dirty revision |
| Lifetimes | Existing entity/window mechanics | App/window/entry/tab/document scope relations, visibility policy, disposal state | Decides which domain objects outlive views |
| Tasks/feeds | Existing runtime, executor and cancellation | Owned handle groups, request tokens, guarded completion admission | Executes IO; classifies durable jobs versus UI observation |
| Subscriptions | Existing producers and removable API hooks where available | Owns cleanup registrations/tokens | Unsubscribes external SDK/transport streams |
| Navigation/guards | Renders tabs/modals; native `CloseGuard` | Typed route/history coordination, deferred transition state | Validates routes, authorizes actions, renders choices and actually saves |
| Diagnostics | Existing runtime observations | Bounded counts/events, stale-result/disposal reasons | Redaction, operational telemetry and leak budgets |
| Security, persistence, transport, OS packaging | Provides relevant optional primitives | None of their policy | Approval/security policy, credentials, artifact trust, backend jobs, persistence and packaging remain host-owned |

Proposed dependency direction:

```text
application (e.g. Medousa/rndr-home)
  ├── rndr facade / rndr_ui / rndr_liquid_ui
  └── rndr-flow (optional coordination, depends on headless rndr_core)
        └── rndr_core (World, App/Plugin, runtime, feeds, events)
```

Keep any renderer-aware wiring in an opt-in adapter/module that depends upward on the necessary UI crate, or initially in the application. No `rndr_core -> rndr-flow` dependency. Core logic must work with `App`/`Sim` without a native window, renderer, browser, Tokio or network. Prefer concrete existing `TaskHandle`/feed ownership over a generic executor facade with no demonstrated need. A pure state-machine internal module can remain independent of runtime polling without requiring another published crate.

### Alternatives and decision conditions

| Alternative | Benefit | Cost/decision |
| --- | --- | --- |
| Only application helpers | Smallest immediate change; host-specific policies stay local | Appropriate for one consumer, but repeats exact stale-result/cleanup invariants. Start proof adapters here if public API uncertainty remains. |
| Extend `rndr_app` | Already a starter layer; no new package | Worth reviewing at API gate. It currently mixes form/preferences helpers and is not the minimal ownership core. Select it instead if independent reuse is not demonstrated. |
| Put flow in `rndr_core` | Easy access to internals | Avoid: couples opt-in routing/policy to every core consumer and risks bypassing existing owners. Only narrow correctness seams belong in core. |
| Optional companion | Isolates adoption and testing; reuses headless foundation | Preferred if the four proofs establish a reusable contract. Cost is a new public compatibility surface; keep API narrow and name provisional. |
| Replace with Rake / wrap its full runtime | Existing broader framework | Wrong scope and dependency direction; GPUI/service/policy coupling, second runtime/tree and a migration instead of strengthening rndr. No such evaluation was executed. |

## 6. Required semantics and unresolved design choices

**Recommendations, not existing API:**

- App is a root lifetime. Windows are children for UI cleanup, but independently owned backend jobs are outside the UI tree. A tab owns retained route entries; a document may outlive several route presentations. Represent one teardown owner and explicit borrowed references, not a DAG of competing disposers.
- Hidden retains model/resources. Suspended explicitly pauses/restarts selected activity; it does not imply disposed or automatically cancel durable work. Re-entering a suspended activity gets a new request generation. Virtualized Liquid slots remain presentation details under their semantic surface owner.
- Logical disposal closes admission and invalidates tokens **before** cancellation/cleanup callbacks; drain UI events with stale validation, then dispose children and consume cleanup registrations once. Report physical worker quiescence separately: cooperative cancellation cannot promise that blocked IO has stopped.
- A completion carries owner identity/incarnation and request generation, plus the domain key/revision needed by the existing model. Check at mutation commit even if runtime cancellation already filtered it. Capture the target at submission, never resolve to whichever chat is selected when completion arrives.
- Subscription cleanup uses consumable tokens/registrations, removed before invoking user code so reentrant dispose cannot invoke them twice. Unsupported producer APIs require upstream removal seams; flow cannot unsubscribe an append-only listener by wishing it so. A single app-owned dispatcher is a possible interim adapter, not cleanup of arbitrary prior registrations.
- Guards are asynchronous owned state, never a World borrow across await. Capture operation/target/dirty revision; save completion revalidates current state. Cancel preserves model/history/resources; a failed save never permits destruction. Duplicate close coalesces; stale decisions fail; conflicting operations serialize per target while unrelated tabs progress. Native exit composes with existing `CloseGuard` rather than replacing it.
- Errors and panics do not authorize disposal or turn dirty state clean. Cleanup errors are recorded while remaining registrations are attempted. Panic recovery applies only where unwind is supported; abort/crash/forced termination cannot promise exactly-once cleanup or save durability.
- Leak diagnostics compare warmed steady-state counts for live scopes, registrations, Liquid records, owned handles and retained outcomes after known disposal/drain points. Counts returning to baseline are meaningful; instantaneous RSS or a task's cancellation flag alone is not proof of quiescence.

Open decisions for review: scope/ID representation and exhaustion; shared document ownership; suspended buffering; history budgets/refusal rather than silent eviction; deterministic cleanup ordering; close mutation-freeze versus revision invalidation; cross-window plugin coordination; source-key compatibility and reset rules; how runtime panic/terminal status becomes observable without breaking public APIs. See the epic for acceptance gates and a concrete implementation order.

## 7. Validation performed and reproducible probes

Environment: local macOS path-based worktree; `rustc 1.98.0 (88d9e12ae 2026-08-18)`, `cargo 1.98.0 (797e8a9bc 2026-08-05)`. No GUI run. Each Cargo invocation below was wrapped in a Python subprocess timeout (120 seconds existing tests, 90 seconds probes); none timed out. Dependencies were available offline; no lockfile update was needed.

| Command | Observed outcome |
| --- | --- |
| `cargo test --locked --offline -p rndr_core --test runtime --test feed` | **26 passed**: runtime 15, feed 11; no failures/ignored. Includes cancellation, timers, local futures, seeded ordering, feed epoch rejection/backpressure/drop cleanup. |
| `cargo test --locked --offline -p rndr_core --lib close::tests` | **3 passed**, 134 filtered. Stale/foreign/duplicate results, retry/cancel/invalidate and discard refusal. |
| `cargo test --locked --offline -p rndr_liquid_ui --test state --test recycle --test feeds --test timer` | **12 passed**: state 2, recycle 4, feeds 5, timer 1; no failures/ignored. Does not cover terminal cleanup. |
| Disposable P1: `cargo test --locked --offline -p rndr_liquid_ui --test flow_analysis_probe -- --nocapture` | **1 passed**, deliberately asserting current retention, not desired cleanup. Three dead views each retained one record; listener ownership count grew 1/2/3, dropped on App destruction. |
| Disposable P2: `cargo test --locked --offline -p rndr_liquid_ui --test flow_replacement_probe -- --nocapture` | **1 passed**, characterizing same-position changed-block state inheritance. Before/after `Accordion { open: [0] }`, changed body visible. |

**41 existing tests + 2 disposable characterizations passed.** The probes reused the helper prefix before the first `#[test]` in `crates/rndr_liquid_ui/tests/state.rs`: `boot`, `press`, `has`, `Log` and `ACCORDION`. Temporary files were removed in Python `finally`; no tests/source changes remain. Unused copied helpers produced four dead-code warnings per probe; no warning suppression was added. To reproduce, append the following bodies to that same helper prefix in a disposable integration-test file, run the corresponding command above, then remove only that created file:

```rust,ignore
// P1: characterization of current retention, NOT a desired-regression assertion.
#[test]
fn records_and_listeners_outlive_removed_views() {
    let (mut app, first) = boot(ACCORDION);
    let log = Arc::new(Mutex::new(Vec::new()));
    for i in 0..3 {
        let view = if i == 0 { first }
                   else { liquid_view(app.world_mut(), ACCORDION) };
        rndr_paint::frame(&mut app);
        rndr_paint::frame(&mut app);
        subscribe_liquid_interactions(app.world_mut(), Log(Arc::clone(&log)));
        press(&mut app, view, "+ One");
        assert_eq!(liquid_component_states(app.world(), view).len(), 1);
        set_liquid_source(app.world_mut(), view, "replacement prose");
        rndr_paint::frame(&mut app);
        assert_eq!(liquid_component_states(app.world(), view).len(), 1);
        despawn(app.world_mut(), view);
        rndr_paint::frame(&mut app);
        assert!(!app.world().is_alive(view));
        assert_eq!(liquid_component_states(app.world(), view).len(), 1);
        assert_eq!(Arc::strong_count(&log), i + 2);
    }
    drop(app);
    assert_eq!(Arc::strong_count(&log), 1);
}

// P2: current positional state transfer, not a specified future reset policy.
#[test]
fn same_source_replacement_reuses_record() {
    let (mut app, view) = boot(ACCORDION);
    press(&mut app, view, "+ One");
    let before = liquid_component_states(app.world(), view)[0].clone();
    let changed = ACCORDION.replace("One", "New")
                           .replace("First body", "Changed body");
    set_liquid_source(app.world_mut(), view, &changed);
    rndr_paint::frame(&mut app);
    rndr_paint::frame(&mut app);
    let after = liquid_component_state(app.world(), view, before.source).unwrap();
    assert_eq!(after.state, before.state);
    assert!(has(&app, view, "Changed body"));
}
```

Not run: workspace-wide builds, native/browser qualification, performance probes, Rake tests, Medousa preview builds, original full migration suite, fault-injected worker panic tests, or a new real A→B chat repro. Those omissions bound the recommendation. Documentation links/path checks and diff hygiene are separate from runtime validation. No remote publication/push, deployment, credential use, packaging, backend jobs or migration occurred.
