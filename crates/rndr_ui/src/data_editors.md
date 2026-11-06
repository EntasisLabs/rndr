# DataTable and PropertyInspector

These are reusable `rndr_ui::data` compositions, not application-specific
widgets. The host owns records, column order/widths/sort state, object and
property IDs, committed values and any persistence/undo. The components own only
bounded presentation, focus/selection and local drafts. Install `UiPlugin` once;
use the normal `rndr` frame/window lifecycle. No new editor, text shaper,
selection dispatcher or scrolling engine is introduced.

## Table composition

```rust
use rndr_core::*;
use rndr_ui::{collections::Item, data::*, *};
let mut app = App::new();
app.add_plugin(UiPlugin);
let parent = app.world_mut().spawn((Layout::column(),));
let columns = [Column { id: ColumnId(1), label: "Name".into(),
    width: 240.0, alignment: CellAlignment::Start, sortable: true }];
let rows = [Item::new(101, "Project A")];
let table = DataTable::mount(app.world_mut(), parent, UiId(1),
    TableProps::new("Projects"), &columns, &rows, |world, slot, item, _| {
        // Reconciliation callbacks must update/reuse their own content children;
        // blindly appending a label on every mount would duplicate content.
        if let Some(&label) = children_of(world, slot.content).first() {
            world.get_mut::<Label>(label).unwrap().text = item.name.clone();
        } else {
            text(world, slot.content, &item.name, Typography::Label).unwrap();
        }
    }).unwrap();
table.rows.select(app.world_mut(), Some(rows[0].id)).unwrap();
```

* IDs are nonzero and unique in their respective table namespaces. Cell identity
  is `(ItemId, ColumnId)`, not position. Reordering/sorting/width changes preserve
  retained row/cell entities and passive custom content. Removed rows/columns
  retire their owned entities and armed interaction targets.
* Rows compose the existing `ListView`, sharing its single/no-selection policy,
  roving row focus, Up/Down/Home/End, Space-to-select,
  Enter-to-activate, Tab, focus reveal and collection events. Focus, selection
  and activation are distinct. Use `table.rows` for its public handle API.
* Headers are canonical controls: click, Enter or Space release requests sorting
  through `SortRequested`. The host handles that event and remounts with reordered
  records and `TableProps.sort`. The component does **not** sort host data.
  Repeated header requests are relative to the last host-accepted sort state.
* Column widths are finite logical points, **48–2048**. Resize declaratively by
  changing `Column.width` and remounting. The gallery has an interactive resize
  control. There is **no pointer drag handle**, auto-size, pinned columns,
  spreadsheet cell editing/navigation, multi-selection or grouped headers.
* Header and rows share the table's horizontal `Scroll`; the backing list owns
  vertical scrolling. `UiPlugin` installs canonical `apply_scroll` itself, before
  Input focus/activation; existing explicit host registration remains safe.
  Table-body height is 80–2000.
* Cell roots/aligner spacers are component-owned. Compose only under
  `CellSlot.content`. Content is **passive**, not nested interactive controls.
  Start cells fill the column. Center/end cells use intrinsic content widths;
  passive labels with Fill width are normalized to Hug. More complex aligned
  content must supply intrinsic sizing. Long content is clipped to its column;
  the accessible cell name retains its text. No ellipsis/tooltip is promised.
* Table/Row/ColumnHeader/Cell roles reach AccessKit. Cells preserve their own
  values/names; live header IDs are supplementary descriptions, not replacement
  cell labels. Rows expose selected/focus state. This is not live AT certification.
* Explicit bound: **256 supplied rows × 32 columns**. Larger data must be
  application-paged/filtered before mounting. It is **not virtualization**, and
  no large-dataset performance claim or benchmark is made.

## Inspector composition and ownership

```rust
use rndr_core::*;
use rndr_ui::{data::*, *};
let mut app = App::new();
app.add_plugin(UiPlugin);
let parent = app.world_mut().spawn((Layout::column(),));
let groups = [PropertyGroup { id: GroupId(1), label: "General".into() }];
let fields = [Property { id: PropertyId(1), group: GroupId(1),
    label: "Title".into(), description: "At most 48 characters".into(),
    kind: PropertyKind::Text { max_chars: 48 },
    value: PropertyValue::Text("Project A".into()), enabled: true,
    readonly: false, validation: None, validate: None }];
let inspector = PropertyInspector::mount(app.world_mut(), parent, UiId(2),
    InspectorProps::new("Project properties", ObjectId(101)),
    &groups, &fields, |_, _, _| {}).unwrap();
// A real host reads events::<PropertyEvent> in Update. For CommitRequested it
// validates object/property identities, applies `after` to its model, then mounts
// again with the accepted Property.value. An ignored request never mutates model.
assert_eq!(inspector.draft(app.world(), PropertyId(1)).unwrap().raw, "Project A");
```

* Groups/properties are ordered by the supplied arrays, identified by nonzero
  stable IDs. Group moves/reordering preserve an otherwise compatible draft and
  editor identity. `ObjectId` is part of transaction identity; hosts must change
  it when selection changes, never reuse it for a different logical object.
* Text/number use the canonical `TextField`, caret/selection, clipboard, typing
  history and IME filter. Boolean/choice/multiple-choice use reusable
  [Checkbox, SingleSelect and MultiSelect](https://github.com/EntasisLabs/rndr/blob/main/docs/selection-controls.md), changing only
  the **draft**, not committing. Choice IDs are separate from display labels.
  The controls reuse core activation and anchored-popup/focus behavior.
* Text has a positive Unicode-scalar-count limit (up to 65,536); numbers accept
  only finite `f64` values within finite inclusive min/max. Intermediate numeric
  strings (such as `-`) remain editable, visible and invalid. Choice IDs must
  exist. Deterministic `validate` runs after type parsing and again at commit.
  `validation` is a host-owned error that blocks commit until reconciled away.
* `DraftChanged` includes raw text, parsed value if valid and validation error.
  `CommitRequested { before, after }` is emitted only for a valid changed value,
  carries inspector/object/property identity, and **never changes host values**.
  Requests identical to the latest outstanding request are suppressed; later edits
  do not erase outstanding acknowledgement identities.
  The host should process frame events in `Update`, not replay them every frame.
* Text/number: non-repeat plain Enter requests commit at that exact queued-key
  boundary; Escape cancels. Commit/Cancel buttons are explicit for every editor.
  Invalid commit leaves the draft editable and shows feedback. The inspector has
  no implicit commit-on-blur. IME confirmation Enter is not a property commit.
* Focus moving from the editor to the **same property's** Commit/Cancel controls
  retains its draft. Leaving that property (including window blur) cancels to the
  host value. An opt-in canonical `FocusBoundary` on the property root observes
  each actual departure synchronously, before the next queued key, including
  Boolean/choice buttons, Commit/Cancel and custom editor descendants. A leave
  and return in one batch still cancels at departure; final containment is only
  a fallback for out-of-band focus changes.
* Changing object, kind, committed value, enable/read-only state, validation or
  validator identity safely reconciles/resets drafts and local typing history.
  Removal/unmount emits explicit cancellation for dirty drafts and clears owned
  interaction targets. A value-only acknowledgement matching an outstanding
  request advances the host baseline without replacing later draft text, caret
  or local undo history (including later invalid drafts). Acknowledged and older
  requests are retired; later requests remain pending. Unrelated value changes
  or contract changes cancel/reset rather than guessing a merge.
  Pending requests already emitted still retain their original object identity.
* Read-only text is focusable/selectable/copyable but cannot edit/submit. Disabled
  controls cannot focus/activate. Feedback uses semantic danger/secondary tokens;
  focused builtin fields use the Info foreground/background pair plus caret.
  Theme, density and independent reading/UI text scales remain shared primitives.
* Custom kinds are **explicit caller compositions** under `PropertySlot.editor`.
  Host controls invoke `set_draft`, `commit`/`cancel` and validate their typed
  `PropertyValue::Custom` string. Custom controls should keep focus in that
  property's subtree while editing. No reflection, automatic object discovery,
  hidden data mutation, external schema system, persistence or app undo exists.
* Bounds: **128 properties, 32 groups, 64 choices per choice field**, 65,536 raw
  draft bytes per `set_draft`. Input definitions are validated before mutation.
  Invalid props/duplicate IDs return `InvalidProps`, cross-family key reuse
  returns `IdentityConflict`, damaged/stale handles return `Stale`. Callbacks must
  be infallible and respect owned slots; this is not rollback for callback code.

## Canonical transaction prerequisite

`rndr_core::FieldTransactions` opts an existing `TextField` into chronological
queued-key editing under `GenericInteraction` (installed by `UiPlugin`). Each key
is routed to the focus current at that key boundary. The ordinary canonical
edit functions still own graphemes, selection, clipboard and typing history.
`FieldTransaction::{Changed, Submit, Cancel, FocusLost}` are synchronous hooks;
`Submitted` remains available with the exact boundary snapshot. No complete
field stage is replayed per key. The per-key filter preserves `ime_enabled`,
preedit and commit from the public `Input` snapshot; aggregate commit text is
inserted once, not submitted. This is not proof of real OS IME operation.
Callbacks must not reinstall/replay input.

`FocusBoundary` is independently opt-in on a subtree, using the same canonical
focus-transition notifications; it does not replace traversal, introduce a second
input loop or migrate legacy fields. Custom focusable descendants inherit the
property boundary automatically. Callbacks must not replay focus or input.

`reset_field_draft` is an explicit owner-driven replacement for opted-in fields:
it clears caret/local undo/preedit and emits no typing or property commit event.
It is not application undo. Existing unopted fields intentionally keep legacy
behavior for compatibility; apps that need these transactions must opt in.

All five saved boundary reproductions are maintained in
`crates/rndr_ui/tests/field_transactions.rs` (snapshot-before-later-text, two
snapshots, held Enter, text around Tab, CapturesKeys Escape). The first four
assertions retain the original boundary expectations; Escape uses the new typed
Cancel hook instead of asking a captured editor to leak into tree navigation.
Historical baseline probes and both owner reproduction directories stay unchanged.
The old ignored probes describe **legacy** behavior, not the opt-in API contract.

SetValue is still deliberately **unsupported** by the accessibility dispatcher;
this candidate does not advertise unsafe history-bypassing accessible edits.
Focus/activation/roles are tested, but live assistive technology, real physical
input and OS IME integration require independent hands-on follow-up.

## Run and exercise the joint gallery

From this rndr checkout (not Blooey):

```sh
# Optionally export CARGO_TARGET_DIR to the existing compatible cache first.
source scripts/data-gallery-env.sh
cargo run --offline --locked -p rndr --example data_editors_gallery --features observe
```

Use row click/Space to select, Up/Down to move focus, Enter to activate; click or
keyboard-activate a sortable header. Edit a field, try invalid progress `-` or
`101`, then use Commit/Cancel or Enter/Escape. Switch rows with an uncommitted
edit to observe cancellation. Change boolean/choice drafts and commit explicitly.
Use Reset/Empty/Remove/Filter/Resize/Read only/Disable/theme/density/text controls.
Narrow/200% modes stack panes and, when needed, toolbar buttons; the outer view
and inspector scroll independently. The synthetic eight-record dataset has no
filesystem or network access.

The gallery Text scale control and `--200` preset scale both UI and reading/code
text; library hosts may still set those theme scales independently.

Capture-only CLI presets include `--dark`, `--compact`, `--hc`, `--narrow`,
`--200`, `--invalid`, `--readonly`, `--disabled`, `--empty`, `--filtered`,
`--inspector-scroll` and `--inspector-bottom`. Scroll presets force an offset each
frame for screenshots; **omit them for manual scrolling**.

```sh
bash scripts/verify-data-editors.sh /tmp/rndr-data-editors-verification
bash scripts/capture-data-editors.sh /tmp/rndr-data-editors-native
```

The verification script requires a fresh output directory and records exact
commands/exits plus disk state. Capture evidence is actual native macOS GPU
**pre-present surface-copy readback**, with exact PNG/metadata SHA bindings, not
HTML or a mocked screenshot. The current observation protocol explicitly reports
`presented_confirmation=false`: no after-present/compositor confirmation is claimed. Preset model/focus/scroll state is synthetic, **not proof of
human input**, real IME or AT operation. The report records any visual limits.
Neither script approves, integrates, pushes, publishes or deploys anything.
