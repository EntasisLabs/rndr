# DataTable and PropertyInspector

The contract ships inside `rndr_ui` so the published crate and docs.rs can
build it. Read [crates/rndr_ui/src/data_editors.md](../crates/rndr_ui/src/data_editors.md).

The host owns records, ids, committed values, and persistence. The components
own bounded presentation, focus, selection, and local drafts. There is no
reflection, virtualization, spreadsheet editing, or application undo.
