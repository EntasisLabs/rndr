# Live markdown CST

**Agreed direction, 2026-11-14.** Medousa edits markdown files that contain rndr Liquid fences as a typed document, with the file bytes as the only durable text. This document is the design for that editor. The headless milestones M1–M5 now live in the unpublished crate `rndr_markdown` (`publish = false`, no workspace version change). Names below that the crate has shipped are the crate's real types. The UI surface and native IME/AT gates have not shipped, and this document does not ratify the experimental editing APIs already in `rndr_core`.

The decision in one pass: a new document authority holds the markdown bytes and a lossless concrete syntax tree over those bytes. The typed surface, including every Liquid fence, is a view of that tree. A visual edit becomes a small text splice. A canonical printer runs only when the command creates text that has no source span yet. After the splice, the editor reparses the edited block and its siblings, checks the result against the tree the command expected, and widens to the parent when the local parse disagrees. Crash recovery is a per-document journal of those splices. Concurrent editing is out of scope; the last section says what would sit under this tree later.

## 1. The authority

Live markdown is a new authority. It does not extend `SourceDocuments`.

`CONTRIBUTING.md` already separates the two authorities that exist today: a source session and a structured tree, with disposable projections that are not a third undo owner. Live markdown is a third durable document, with the same rule. One file has one writer.

| Authority | Where it lives | What it owns | What it continues to own |
|---|---|---|---|
| Exact source | `SourceDocuments` / `SourceSession` in `crates/rndr_core/src/editing.rs`, mounted by `EditorSurface` in `crates/rndr_ui/src/editing.rs` | UTF-8 bytes, `SourceBatch` splices, one history, composition, per-view selections | Code, scripts, logs, and any buffer whose display is the bytes. The accepted E7 staged projection in `crates/rndr_core/src/source_projection.rs` stays a narrow line grammar over this session. |
| Structured document | `StructuredDocument` in `crates/rndr_core/src/structured.rs`, stored in `StructuredDocuments` in `crates/rndr_core/src/structured_native.rs` | Schema blocks, marks, stable `BlockId`s, its own history | Notes that are a schema tree. The structured example formats a tree. It has no markdown codec. |
| Single-line field | `TextField` in `crates/rndr_core/src/field.rs`, history in `EditHistory` in `crates/rndr_core/src/typing.rs` | The entity's `Label` | One line, no file, no document id. `FieldTransactions` notifies a host; it does not own a file. |
| Canvas text | `Content::Text` in `crates/rndr_canvas/src/schema.rs`, transactions on `CanvasRecord` | A string inside a Create v3 scene | Scene text. `CanvasEditor::type_text` in `crates/rndr_canvas_ui/src/editor.rs` appends and erases at the end of that string. The facade documents that limit in `crates/rndr/src/lib.rs`. |

A markdown file opened as a live note is none of these.

Extending `SourceDocuments` would make `SourceSession` carry two displays. Its contract is that the bytes are the text: selections are source offsets, copy reads those bytes, and `EditorSurface` is "not structured editing or persistence." E7 is the rendered layer that session already has, and its own header says it is not a Markdown codec. It reparses the whole buffer, edits one literal segment, and refuses a structural change. A block-scoped lossless tree, a parent-widen check, fence-field views, and a crash journal are a different admission owner. Folding them into `SourceSession::commit` would change the E7 contract by overloading it. E7 stays as it is.

Extending the structured document would make the schema tree the authority and the file a serialization. `structured.rs` addresses stable blocks, not source bytes. Marks merge. Runs are canonical. That is the normalized policy in `docs/design/native-document-editing-e7-comparison.md`, and it is the third policy in architecture §5. Architecture §1 already rejects a lossless serializer as the common model of every document. This design keeps that rejection. The printer described below is a constructor for new text, not the document.

`TextField` has no file and no session. Canvas text is scene JSON with its own revision and history. A canvas node that someday embeds a note is the foreign-document boundary in architecture §4: the canvas stores a reference, and the markdown session owns the bytes. That embedding is out of scope here. The two histories stay separate.

The preview path stays a reader. `MarkdownView` in `crates/rndr_core/src/markdown.rs` parses with `pulldown-cmark` 0.13.6 (`ENABLE_TABLES | ENABLE_STRIKETHROUGH | ENABLE_TASKLISTS`) and paints a window of blocks. `LiquidDocument::parse` and `LiquidStream` in `crates/rndr_liquid/src/document.rs` and `stream.rs` rewrite the source: innermost fences become `\u{E001}` tokens, documentation fences are unwrapped, CRLF is normalized, and `{{icon:…}}` shortcodes are rewritten. `liquid_view` in `crates/rndr_liquid_ui/src/view.rs` mounts that stream. The facade states that this view writes nothing back. Those types remain the streaming preview. The editor never calls them to obtain a tree it will write.

Proposed home, measured the way the architecture asks before a new crate proliferates:

- One headless crate, proposed name `rndr_markdown`, `publish = false`. It owns bytes, the CST, the edit compiler, session ids, undo, and the journal. It does not depend on `rndr_ui`, `rndr_paint`, or `rndr_render`.
- It may depend on `rndr_liquid` for `FENCE_LANGS` and, when a view needs a semantic value, `parse_fence`. `rndr_liquid` does not depend on it. `rndr_liquid` still owns no markdown engine.
- `rndr_core` does not depend on it. Workspace version `0.6.0` in the root `Cargo.toml` stays put. The README's published crate list stays put. Publishing the crate is a later release decision, not part of any milestone below.
- The UI mount is `rndr_markdown_ui`, unpublished, a sibling crate of `rndr_ui`. `MarkdownSurface` is a sibling of `EditorSurface`, not a flag on `EditorSurfaceProps`. It does not call `attach_source`. The published `rndr_ui` crate does not depend on `rndr_markdown`, so the crates.io list is unchanged.

Opening a path chooses one writer. Source mode for that path is a `SourceSession`. Live mode is a `MarkdownSession`. Switching modes flushes, ends composition, and closes the session that is losing the file. Architecture §7 requires that flush and cancellation before a mode swap, and it does not keep two writable full-document authorities unless an ordering bridge exists. Closing one session before opening the other is that bridge. The host owns the choice of mode. The toolkit enforces the single writer.

## 2. Bytes and the concrete syntax tree

The session stores the file as UTF-8 bytes, including a leading BOM, mixed `\n` and `\r\n`, and a missing final newline. A splice is a UTF-8 scalar range, the same boundary rule `SourceBatch` uses. Carets snap to graphemes with the same `floor_grapheme` behavior `snapped_selection` uses in `editing.rs`. Logical affinity and bidi visual affinity stay distinct, as E7 already records; this design does not certify native bidi motion.

The tree is a rowan-style green/red tree. Green nodes are immutable and own their width. Red nodes carry parent pointers and offsets and are rebuilt when a green subtree is grafted. Trivia lives on tokens. `rowan` is not a workspace dependency (`rndr_core` depends on `pulldown-cmark` only). M1 uses a crate-local green/red tree of that shape, in `crates/rndr_markdown/src/green.rs`. It does not add `rowan`, and it does not put the tree inside `rndr_core`.

Lossless means the concatenation of the green children of the root equals the bytes. Parsing does not rewrite a delimiter, a line ending, an escape, an entity, or a fence. Two parses of the same bytes build equal green trees. Session ids are not part of green equality.

### 2.1 Block nodes

The grammar is CommonMark, plus the three extensions `markdown.rs` already enables: tables, strikethrough, and task lists. Liquid fences are fenced code blocks whose info string is in `FENCE_LANGS`, typed by a view (§3), not a second grammar.

| Node | What it owns |
|---|---|
| Document | The root. A BOM is its leading trivia. |
| Paragraph | Inline children and the line ending of its last line. |
| Heading | ATX or setext. The marker characters, the ATX closing marks, and the setext underline are trivia. The kind is heading either way. |
| Block quote | The `>` prefixes as per-line trivia, and the child blocks. |
| List | Bullet, ordered, or task. The marker character, indent, start number, and task letter case are trivia of the items. Tight or loose is derived from `BlockGap` children, not stored. |
| List item | The marker and the child blocks. |
| Fenced code | The fence character, length, info string, and body bytes. A Liquid view may sit on top. |
| Indented code | The indented lines. A blank line ends the block and is not part of it. |
| HTML block | The CommonMark start and end conditions, so the span is right. The interior bytes are one opaque child. |
| Thematic break | The exact marker (`---`, `***`, `___`, and longer runs). |
| Table, row, cell | A GFM table. Alignment colons and padding spaces are trivia. |
| Link reference definition | The label, destination, and title bytes, in place. |
| Block gap | One blank line owned by the parent. §6. |
| Opaque block | Bytes the grammar does not structure, copied through. |

An HTML block is opaque inside and precise at the edges. CommonMark types 1–5 run until their closing delimiter and may contain blank lines, which are interior. Types 6 and 7 end at a blank line, and that blank line is the following `BlockGap`, not content. Getting this boundary wrong is why HTML termination is one of the widen cases in §5.

A link reference definition stays where it was written. Link inlines store their source spans. Which definition a label resolves to is a view over the definition nodes. Editing a definition does not rewrite later paragraphs. It does change how those paragraphs parse, so §5 has a separate pass for that.

### 2.2 Inline nodes

| Node | Trivia that must survive an edit that does not touch it |
|---|---|
| Text | The bytes. |
| Emphasis, strong | The delimiter character (`*` or `_`) and its length. `_a_` stays underscores. `*a*` stays asterisks. |
| Strikethrough | The `~~` markers. |
| Inline code | The backtick run length, and the padding space CommonMark requires when the body starts or ends with a backtick. |
| Link, image | Inline versus reference form, title quotes, and the exact destination bytes. |
| Autolink | The `<…>` bytes. |
| Hard break | Two trailing spaces, or a backslash. The one present is the one that stays. |
| Soft break | The newline, plus the indent of a lazy continuation line. |
| Escape | The backslash and the escaped character. |
| Entity | The `&name;` or numeric form. The view shows the decoded character. The bytes stay encoded. |
| Icon shortcode | `{{icon:…}}` as written. `normalize_icon_id` remains a render concern of `rndr_liquid`. |
| HTML inline, opaque inline | The raw bytes. |

### 2.3 The typed view

The view the user edits is a projection of the red tree. A paragraph is text with marks. A heading shows its level. A list shows markers as structure. A fence whose info string is in `FENCE_LANGS` shows as a Liquid block. The projection borrows spans; it does not own bytes, history, or ids.

Commands name view operations: insert text, split block, toggle a mark, set a fence field, insert a new fence. The compiler in §4 turns each command into a splice. The view is then rebuilt from the grafted tree. There is no path that writes the view and later serializes the document.

`parse_fence` in `crates/rndr_liquid/src/block.rs` remains the semantic reading of a fence body for rendering: `LiquidBlock` with its props. `parse_kv_block` in `crates/rndr_liquid/src/kv.rs` lowercases keys, drops `#` comments and `---` lines, trims, and runs `lf`, which replaces `\r\n`. That map is the right input to the existing renderers. It is the wrong store for an edit. The editor's field nodes are the store. After a fence's green node changes, the host may call `parse_fence` again to refresh a render value. Writing that value back is not an edit.

## 3. Liquid fences

`FENCE_LANGS` in `crates/rndr_liquid/src/block.rs` is the catalog, aliases included (`action_row`, `chip_group`, `procedure`). A fenced block whose info string matches an entry, compared the way `parse_fence` compares (trim, lowercase), is a Liquid node in the view. The info-string bytes stay as written: `procedure` is not rewritten to `recipe`, and `Chart` is not rewritten to `chart`. The view resolves the alias with the same match arms `parse_fence` uses.

The fence body is a field tree, in source order:

| Field node | Source it covers |
|---|---|
| Field | A `key: value` line. The key's original spelling, the spaces around `:`, and the line ending are trivia. |
| Quoted value | The quote character is trivia. The interior is the value span. |
| Pipe row | One `key: value \| key: value` line, the form `parse_kv_line` accepts. Fields stay in that row, in order. |
| Block scalar | The indicator (`\|`, `\|-`, `>`, and the other `YAML_BLOCK_SCALAR` forms) is trivia. The following body lines are the value. |
| Comment | A line `kv.rs` skips because it starts with `#`. |
| Separator | A `---` line, the item separator `split_hr` uses. |
| Gap | A blank line inside the fence. |
| Opaque line | Anything else, including an unknown key, kept verbatim. |

These nodes are views over the fence's body span. Their concatenation is the body. Comments, field order, quoting, and key spelling survive a field edit because the edit splices the value span and leaves the rest of the field node alone.

A markdown-valued field contains a nested CST over its value span. The fields `props.rs` documents as markdown are:

| Payload | Field |
|---|---|
| `CardProps` | `summary` ("long-form detail (markdown)") |
| `CalloutProps` | `body` |
| `BriefSection` | `body` |
| `ReportProps` | `body` (nested blocks allowed) |
| `SlideItem` | `body` |
| `SectionProps` | `body` |
| `TabsPanel` | `body` |
| `StepItem` | `body` |
| `RecipeStep` | `body` |
| `AccordionItem` | `body` |
| `StyledBlockProps` | `body` |

Every other field is literal text. A fence inside a markdown field is a nested fenced node of that inner CST, which is how `LiquidDocument::parse` already finds innermost fences. A fence-shaped string inside a title stays characters of the title. Milestone M6 lists these fields in a test that fails when a new `props.rs` field is documented as markdown and is missing from the list.

`parse_fence` returns `None` for an unknown language and for a known language whose body is not valid Liquid. An unknown language stays an ordinary code fence. A known language with an invalid body stays a fence with that info string, shown as its field text rather than as a card, and becomes a typed block when a later splice makes `parse_fence` succeed. The editor does not rewrite it into a different fence to force a parse.

A new fence inserted from the UI is brand-new content. The printer emits a canonical fence: three backticks, the canonical kind from `LiquidBlock::kind` (aliases are recognized on input and are not the spelling of a new fence), LF or the document newline rule in §4, fields in the order the command supplies as `key: value`, a block scalar only when the value contains a newline, and a closing fence. The printed bytes are parsed and must match the requested node. Later edits to that fence are splices.

## 4. From a visual edit to a splice

Every accepted visual edit compiles to one `SourceBatch`-shaped splice list: ordered, nonoverlapping, scalar-bounded ranges against the current revision. The session's admission order follows `SourceSession::admit` and `commit`:

1. The document id matches.
2. The base revision is current.
3. The session is writable (readonly and freeze refuse).
4. No composition is active, except the composition commit itself.
5. The event queue is under the same kind of ceiling `EVENT_LIMIT` (256) imposes. The host drains events. Backpressure refuses the edit.
6. The revision counter can advance.
7. Ranges are in order, nonoverlapping, and on scalar boundaries.

An empty splice, or a splice whose replacement equals the deleted bytes, is a consumed no-op. It does not advance the revision, push history, append a journal op, or emit an event. Callers test that, the same way source commits do.

The compiler proposes the smallest splice that can produce the command's expected tree.

| Command | Splice |
|---|---|
| Insert a literal character inside a text node | Those bytes, at that offset. |
| Insert a character that would change the parse (`*`, `_`, `` ` ``, `[`, `]`, `\`, and a line-start `#` or list marker) | The character, if the reparse matches the expected text node. Otherwise the same character preceded by a backslash, if that reparse matches. |
| Toggle emphasis on a span already wrapped in `*` or `_` | Delete or restore that delimiter pair. The character already in the file is the character used. |
| Toggle emphasis on a span with no delimiter | Printer inserts `*` around the span. This is new markup. |
| Toggle strong, strike, or code on an existing span | The same rule, with the existing delimiters. New strong uses `**`, new strike uses `~~`, new code uses a backtick run long enough for the body. |
| Insert a hard break where there is none | Two spaces before the newline. An existing backslash hard break is left as a backslash. |
| Edit a Liquid field value | Replace the value span. Key, colon, quotes, indicator, and comments stay. |
| Insert a new field into an existing fence | Printer emits one new field line, spliced at the insertion point. The rest of the fence is not reprinted. |
| Insert a new block that has no source | Printer emits that block. Neighboring gaps are copied, not reprinted (§6). |
| Split a block | A newline, plus a gap when the command expects one. |

New bytes use `\n` when the file has any `\n` that is not part of `\r\n`, or when the file is empty. A file whose line endings are all `\r\n` gets `\r\n` in new bytes. Existing lines keep the endings they have. A BOM is never invented and never removed.

The printer's other new-node spellings, used only when no sibling exists to copy:

| New node | Bytes |
|---|---|
| Heading | ATX, `#{level} ` and the text. Level 1 through 6. |
| Bullet item | `- `. |
| Ordered item | `1. ` when the list is new. Otherwise the next number, with the sibling's delimiter. |
| Task item | `- [ ] ` or `- [x] `. |
| Thematic break | `---`. |
| Block quote | `> ` on each new line. |
| Link | `[text](destination)`. A title is emitted, in double quotes, only when the command supplies one. |
| Code fence | Three backticks, the info string, the body, three backticks. |
| Table | A GFM pipe table. |

When a sibling of the same kind exists, the new node copies that sibling's marker, indent, fence character, and fence length instead of the canonical spelling. Copying is how a `+` list stays a `+` list.

The compiler applies the proposed splice to a scratch copy of the bytes. It does not touch the session, the journal, or the undo stack until §5 accepts the scratch. A command that cannot be given a splice whose reparse equals the expected tree is refused, and the session is unchanged. The editor does not fall back to reprinting the file.

## 5. Reparse, check, and widen

A full parse on every keystroke would make the performance targets in §12 unreachable, and a purely local parse is wrong for markdown. The check is local, and it widens until it agrees with the grammar.

The expected tree is part of the command. Inserting text expects the same block with different text. Splitting a paragraph expects two paragraphs. Toggling a mark expects the same text with different delimiter trivia. Neighbors outside that footprint are expected to be green-equal to their previous selves.

After the scratch splice:

1. Find the deepest block that covered the splice. A splice inside a `BlockGap` starts at that gap.
2. Reparse the span of that block plus its immediate previous and next siblings, in the parent container's context (quote prefixes and list indents taken from the parent, not rediscovered by scanning the file).
3. Compare that parse with the expected tree on the same span. Comparison is green structure and bytes. Ids are not compared.
4. On agreement, graft the parsed green nodes into the live tree, keep ids of nodes outside the replaced span, and commit the splice to the session.
5. On disagreement, widen the span to the parent block, including that parent's immediate siblings, and parse again. The expected footprint stays what the command named; nodes outside it must still be green-equal.
6. Repeat until the span is the root. If the root parse disagrees with the expected footprint, discard the scratch and refuse the command. If the command was a literal insert, retry once with the escape form from §4 before refusing.

The root parse of an accepted scratch must equal a from-scratch parse of the scratch bytes. The widen loop grafts a window only when that graft's concatenation matches and the grafted document equals a from-scratch parse. A mismatch widens. The confirming parse is what the implementation uses today so a bad resync cannot publish; the fuzzer in §11 is the same check from outside. Dropping the confirming parse is allowed only after the resync rules alone satisfy the fuzzer.

These are the mismatches the widen exists to catch, and the span that actually decides them:

| Phenomenon | Why the first span is too small | Where it is decided |
|---|---|---|
| Lazy list continuation | A line can belong to the previous item once the indent is visible. | The previous sibling, then the list. |
| Tight/loose flip | Adding or removing one blank line changes a property of the whole list. | The list parent. |
| Setext heading | A line of `-` or `=` is a thematic break, or the previous paragraph's underline, depending on that paragraph. | The previous sibling. |
| Reference definition | A paragraph can become a definition, or the reverse, at a block boundary. | The definition's siblings. Resolution of later labels is the pass below. |
| Emphasis rebinding | Marker runs bind across a whole paragraph (`*a *b* c*`). | The paragraph. Widening further is required only when the edit also moves the block boundary. |
| HTML block termination | A blank line ends a type 6 or 7 block and is outside it, or sits inside a type 1–5 block. | The HTML block plus the next sibling. |

Reference definitions also affect link structure later in the file, outside the parent chain. When a reparse adds, removes, or changes a link reference definition, the session reparses later blocks that contain `[` or `]` before it publishes the tree. An index of those spans makes the pass proportional to candidate blocks, not to every paragraph. Blocks whose reparse is green-equal are left alone, ids included. A block whose link structure changes is grafted with the same id rules as any other edit. This pass is part of commit, not a background repair.

An unclosed fence at the end of the file is a fence node that runs to EOF, the same practical choice `LiquidStream::pending_lang` makes for the preview. Closing it is an ordinary splice.

## 6. Who owns the blank lines

A blank line is a line whose content is empty or only spaces and tabs (CommonMark's definition), including its line ending. It has exactly one owner. The concatenation rule makes the owner observable: the bytes sit in that node's green span and nowhere else.

**The parent owns blank lines between its children.** Each such line is a `BlockGap` node, a sibling of the blocks it separates. A paragraph does not contain the blank line that ended it. A list item does not contain the blank line that separates it from the next item. The document root, a block quote, a list, and a list item are the parents that hold gaps.

That ownership is what keeps a deletion local.

- Deleting a block removes that block's span and one adjacent gap of the same parent: the following gap if there is one, otherwise the preceding gap. The other neighbor's bytes are unchanged. The splice covers the block and that one gap.
- Inserting a block at a gap leaves the existing gap's bytes in place and copies them for the second side. The new block's bytes come from the printer. A gap of `\r\n` yields `\r\n`, the new block, and another `\r\n`. The printer does not invent a blank-line style.
- Inserting a sibling into a tight list copies the marker and does not invent a gap. The list stays tight. A command that asks for a loose list inserts one gap and expects the parent reparse to report the list loose.
- The newline that ends a non-blank line belongs to that line's block. A file that does not end in a newline is a last node that does not end in a newline. Inserting after it splices the missing newline onto that last node and then the new node.
- Trailing blank lines after the last block are `BlockGap` children of the root, so deleting the last block can remove one of them under the deletion rule and leaves the others.

Interior blank lines are not gaps of the outer parent:

- Blank lines inside a fenced body, an HTML block of types 1–5, or a Liquid field tree belong to that block.
- A blank line that ends an indented code block or an HTML block of type 6 or 7 is the next `BlockGap`, because CommonMark excludes it from the block. The widen check is what moves the boundary when a local edit gets this wrong.
- A blank line between two blocks inside one loose list item is a `BlockGap` owned by that item, which is why it contributes to the list being loose. The flag is derived by walking the item's gaps. It is not a field anyone writes.

The typed view does not paint a gap as an empty paragraph. Changing the space between two blocks inserts or deletes one `BlockGap` through the rules above. The raw view shows the bytes.

## 7. Node identities

Ids are session-scoped. They are not written to the file, the journal, or a checkpoint.

At load, and after a recovery that builds a tree from bytes, ids are a preorder numbering of the red tree starting at 1. The same bytes produce the same ids. The session allocator then starts at one past the highest loaded id and only increases.

An edit grafts a span. Nodes outside that span keep their ids. A node inside the span keeps its id when the reparse maps it to the same kind and the same role among the command's surviving children (the paragraph that was split keeps the left id; the right paragraph is new). New nodes take fresh ids from the allocator. The history step records the ids it minted and the ids it retired, so undo drops the minted ids and redo restores them, without renumbering the rest of the tree. Undo does not run the load-time numbering again.

A typed/raw flip is a view change. It does not reparse, does not graft, and does not allocate. Ids are unchanged while the user is typing, including across a flip between keystrokes. A flip is refused while a composition is open, the same way `SourceSession::set_readonly` refuses with `EditError::Composing`. The caret is a byte offset with an affinity; both views map that offset through the same tree. Hidden delimiters use `Affinity::Before` and `Affinity::After` so a caret on either side of a marker is addressable, which is the E7 rule carried onto real tokens.

Ids die with the session. Crash recovery assigns load-time ids to the recovered bytes. Agents and hosts must not store an id across a flush boundary and expect it to resolve later. The durable handle is the file's bytes, addressed by the checkpoint hash in §10.

## 8. One history, IME, and the raw view

The session has one undo stack. Typed edits, raw edits, paste, field edits, structural commands, and IME commits are entries on it. The stack stores inverse splices and the id note from §7, not full-buffer snapshots. Its retained bytes are capped by the same ceiling `HISTORY_BYTES` documents in `editing.rs` (16 MiB). The oldest steps are dropped. History belongs to the session. A view does not have its own stack, which is the rule architecture §2 states for source sessions and the rule `SourceSession`'s header repeats.

Grouping follows `EditHistory` in `typing.rs`, applied to splices rather than to label snapshots. Characters typed in a row share one step. The run ends at whitespace, when the caret moves, and after a pause of 600 ms. A deletion, a paste, a newline, a structural command, and an IME commit are each their own step. Changing focus, flipping views, and acknowledging a save are not entries. An undo is a new forward splice on the journal (§10); the journal is append-only and is not rewound. After a crash, the recovered session has an empty undo stack. The bytes are current. The stack was session state.

Composition uses `resolve_ime` in `crates/rndr_core/src/ime.rs`. One composition is owned by the focused view of one session generation. It records a base revision and a replacement range. Preedit text is painted and is excluded from the bytes, the CST, the undo stack, and the journal. While it is open, the IME filter already drops character keys, Space, Enter, Backspace, Delete, the arrows, Home, End, and Page Up/Down unless Ctrl, Alt, or Super is held. Editor commands that would splice (toggle a mark, split, flip views, set a field) refuse with the composing error. A commit replaces the captured range once, through §4 and §5, and is one undo step and one journal op. A cancel writes nothing. Focus loss cancels or commits according to the platform's own event; the session does not guess. A readonly transition waits until the composition ends.

The raw view displays the bytes and sends the same splices. It is not a `SourceSession` opened on a copy. Selections in the undo stack are byte ranges, so a flip between an edit and its undo still restores the caret. Copy has two explicit policies, matching the E7 split: source copy reads the byte range, syntax included; rendered copy reads the view's plain text and is not pasteable as a round trip. Paste of plain text is an insert-text command. Paste of a structured fragment from `StructuredDocument` is brand-new content: the printer runs, then §5 checks the printed span. The editor does not parse HTML from the clipboard.

v1 has one editing view, which can flip. The session owns the stack so a second view, if a later milestone adds one, shares it the way `SourceSession` views do. That second view is not part of v1.

## 9. Accessibility

The typed view publishes a tree of the CST, not a markdown dump and not a single text value.

- The root's accessible name is a host-supplied name, required the way `EditorSurfaceProps.name` is required. It is not the document text.
- Blocks expose their roles: heading level, list, list item, paragraph, table, link, image. The image name is its alt span.
- A Liquid fence is a group named by its canonical kind. Visible fields are children (the card's title, the callout's body). The fence source is not the name.
- An opaque block exposes its text content.
- Bounds come from the layout revision used to paint, in window coordinates, clipped to the viewport the way `AccessTextSnapshot` in `crates/rndr_core/src/access_text.rs` already describes. Offscreen blocks are present as text and roles without shaped bounds.

The raw view publishes the source buffer through the existing source snapshot path: full bytes, directed selection, revision.

Mutations from assistive technology go through session admission as splices. `AccessCommand::SetValue` stays `UnsupportedSetValue` in `crates/rndr_core/src/access.rs`. A published snapshot carries the document id and revision, and a selection action against a stale snapshot refuses, which is what `AccessTextState` is for. Headless tests in M6 cover the tree, the refusal, and the stale snapshot. VoiceOver, NVDA, and Orca are milestone M7. They are not implied by the headless tree. `docs/native-observation.md` draws that same line for the toolkit today.

## 10. Durability

Undo is memory. The journal is how a crash, or an agent read, sees the bytes. They are different stores.

The host supplies the document path and the journal path. The toolkit does not choose a vault layout. Tests use a temporary directory. A Medousa host can put the journal beside the note or in a recovery directory it already owns; the bytes of the protocol do not depend on which.

The journal is append-only, little-endian, version 1:

- A fixed header: magic `rndr-md-journal`, version, the checkpoint sequence, and the SHA-256 of the checkpoint bytes.
- Appended frames, one per committed splice: sequence, start, end, deleted bytes, replacement bytes, the SHA-256 of the document after the splice, and a checksum of the frame.

A torn tail (a partial last frame, or a bad checksum) is ignored. Sequences start at 1. Sequence 0 is the file as opened: its hash is recorded in the header when the first real edit is journaled, and the file is not rewritten to record it.

Opening a file and editing nothing creates no journal, appends no frame, and does not change a byte. A no-op splice does the same. If a journal is already caught up with the file, opening it also appends nothing and writes nothing.

**Group commit.** Frames are buffered and fsynced as a group. The group waits at least 50 ms and is on disk within 100 ms of the oldest unflushed frame. The default delay inside that window is 80 ms. The crate does not start a timer thread. The host calls `Session::poll` with the frame clock, which matches the toolkit's explicit frame lifecycle. `Session::flush` group-commits immediately. A crash can lose the group still in the buffer. That window is the durability of typing. Undo steps are frames too: an undo appends the inverse splice at the next sequence. Replaying the log reproduces the undos. It does not rebuild the undo stack.

**Checkpoint.** On idle (one second after the last edit, if the journal is ahead of the header), on focus leaving the document, on the host's save, and on `flush`, the session writes the bytes out:

1. Hash the destination file. If it exists and its hash equals neither the header hash nor the hash of the bytes about to be written, stop with a conflict. Do not rename.
2. Write a temporary file in the same directory.
3. fsync the temporary file.
4. Rename it over the destination.
5. fsync the directory.
6. Store the new hash and the sequence of the last frame included in those bytes into the journal header, and fsync the journal.

The journal group is fsynced before step 2, so a crash during the checkpoint still has the frames. Each frame's post-hash is what makes step 6's crash window recoverable: if the rename landed and the header is still old, the file's hash equals the last frame's post-hash, and recovery resumes there instead of treating the file as foreign.

On Windows the directory is opened for write with backup semantics, which is what `FlushFileBuffers` accepts for a directory. If a platform cannot fsync a directory, the checkpoint returns an error that names that step and does not report success. Milestone M5 tests the crash order on the operating systems CI already runs. `std::fs::rename` replaces the destination, including on Windows.

**Recovery.** Hash the file with SHA-256.

- If the hash equals the header, replay frames with a greater sequence, in order.
- If the hash equals a frame's post-hash, or the sequence-0 hash, replay only the frames after that sequence.
- Otherwise the file was changed by someone else. Do not replay. Quarantine the journal (rename it aside; do not delete it), load the file, assign load-time ids, and return a conflict. The in-memory bytes of a session that is still alive are kept so the host can show them.

An op is idempotent. If the current hash already equals the frame's post-hash, skip it. If `bytes[start..end]` equals the deleted bytes, splice the replacement; the hash must then equal the post-hash. Any other span is a conflict: stop, quarantine, reload. A replay never guesses a splice position.

**External changes.** The comparison is the hash. Modification time is not an input. The session checks the hash when it becomes focused, when it checkpoints, when it flushes, and at recovery. A mismatch refuses replay and refuses the rename in checkpoint step 1.

**Conflict value.** The toolkit returns the path, the file hash, the session hash, the session sequence, and whether the session still holds bytes in memory. Medousa renders the UI. The session accepts no further splices until the host calls one of two resolutions: reload, which takes the file and leaves the quarantined journal on disk, or replace, which checkpoints the in-memory bytes over the file and is available only while those bytes are still in memory. There is no automatic merge.

**Agent reads.** `flush` group-commits, checkpoints, and returns the bytes, the hash, and the sequence. The runtime that serves document text to an agent calls `flush` first and serves what it returns. A read of the path that skipped `flush` is a host bug. The toolkit has no agent type of its own; the method is the gate.

`flush` of a clean session is a no-op that returns the current hash and sequence. It still re-reads the file hash, so an external change is reported instead of being overwritten.

## 11. Invariants and the fuzzer

These are the acceptance tests, independent of the UI.

1. **Parse is lossless.** For every byte string the fuzzer and the corpus produce, the green concatenation equals the input, and a second parse equals the first.
2. **Edits commute with parsing.** For a document and a command sequence `E`, `parse(apply_text(bytes, E))` equals `apply_view(E, tree)`. The left side applies the compiled splices and parses from scratch. The right side runs the view, including widen and the reference-definition pass. Equality is the green tree.
3. **A clean open is silent.** Opening a document and issuing no command appends zero frames and changes zero file bytes. A refused command and a no-op splice do the same.
4. **Opaque bytes round-trip.** A construct the grammar does not structure is one opaque node. Its bytes equal its source span. An edit whose footprint does not cover that span leaves the span's bytes unchanged.
5. **Refusal is atomic.** Readonly, composing, stale revision, overlap, backpressure, and a failed widen check leave bytes, tree, ids, history, and journal as they were.
6. **Ids follow §7.** A flip allocates nothing. An edit keeps ids outside the graft. Reloading the same bytes reproduces the load-time ids.

The fuzzer follows `crates/rndr_liquid_ui/tests/fuzz_gallery.rs`. That harness drives seeded input, pins one configuration so runners agree, fails with the seed and the step in the panic, runs a fixed seed in CI, and hides a longer soak behind an ignored test:

```text
RNDR_FUZZ_SEED=1 RNDR_FUZZ_FRAMES=20000 \
  cargo test -p rndr_liquid_ui --test fuzz_gallery -- --ignored fuzz_gallery_soak --exact
```

The edit fuzzer is its own test in `rndr_markdown`, not a new scene inside the UI gallery. It keeps the same shape. A fixed seed runs in CI for a bounded number of edit steps. An ignored soak reads `RNDR_FUZZ_SEED` and `RNDR_FUZZ_FRAMES`. A failure names the seed and the step index, and prints the command and the byte range. The input space is the command list in §4, the hazard list in §5, opaque spans, Liquid fences drawn from `FENCE_LANGS`, and the byte-level fixtures already used to tell projection apart from custody in `docs/design/native-document-editing-e7-comparison.md` (BOM, CRLF, `*old*  `, `__keep__`, a tilde fence). The corpus parse of those fixtures is lossless even though the editor is not E7.

The fuzzer also counts how often a random edit widens to the root. The count is reported. A widen that the oracle then disagrees with is a test failure, not a statistic.

## 12. Performance targets

These are targets for milestone M6 to measure and record. They are not claims about the current tree, and a miss is reported with the number and the cause.

The large document is 1 MiB of mixed notes, on the order of 10,000 blocks, including Liquid fences. A second probe at 5 MiB checks that cost stays flat per edit.

| Path | Target |
|---|---|
| Open, full parse, no journal write, no shaping | Under 50 ms at 1 MiB, file IO excluded. |
| A keystroke inside a paragraph | Reparse of that paragraph and its two neighbor siblings, under 1 ms at 1 MiB, shaping and fsync excluded. |
| Untouched green nodes | Shared. The edit does not allocate proportional to the file. |
| Paint | The viewport plus the overscan `MarkdownView` already uses. Offscreen blocks are not entities. |
| Journal | Off the key path. One group fsync per 50–100 ms window. |
| Checkpoint | Off the key path. Idle, blur, save, or `flush`. |

A root widen is allowed and must be correct. It is not the steady path. If the fuzzer's widen fraction says ordinary typing reparses the file, the grammar context is wrong and that milestone fixes the parser. `MarkdownView`'s existing incremental rule (a stable boundary after two blank lines, for append-only streaming) is a preview strategy. The editor's unit is the edited block, then its parent, and the two are not interchangeable.

Shaping stays on the text engine the editor already uses. This design does not choose a rope or a piece table for the bytes. The green tree is the shared structure. If M1's measurements show the byte buffer copying on every splice, the session adopts whatever buffer representation meets the allocation target, behind the same splice API.

## 13. Concurrency, later

v1 is one writer. There is no CRDT, no OT, and no shared undo with another process. `SourceSession` already rejects a stale revision rather than rebasing, and this session does the same.

The later path, if a milestone is ever opened for it: a text CRDT (Loro or Yrs) under the CST. The document the CRDT stores is the markdown bytes. Agents write plain markdown. The host diffs that markdown into text splices, which enter through §4 and §5 like any other edit. Node ids become anchors into the CRDT positions so a remote splice maps to the same graft local edits use. The typed view stays a view. The printer stays limited to new content. The journal remains the single-writer crash log until the CRDT's own persistence replaces it; they are not stacked silently.

That path is a note so the byte-level splice stays the edit. It is not a design for collaboration, and no milestone below builds it.

## 14. Milestones

Each milestone is one PR-sized result with one owner. None of them change `workspace.package.version`, the versions in `workspace.dependencies`, or the published crate list in `README.md`. The new crate stays `publish = false`.

| Milestone | Result | Depends on | Done when |
|---|---|---|---|
| M0 | This document. | — | The design is in the tree. |
| M1 | The `rndr_markdown` crate and a lossless parse of §2, including opaque nodes and Liquid fences recognized via `FENCE_LANGS`. | M0 | Done. Concatenation and reparse-equality tests. Crate-local green/red tree, recorded above. |
| M2 | Scratch reparse of a block and its siblings, parent widen, and the reference-definition pass. | M1 | Done. Hazard tests cover the six cases in §5. A graft is published only when it equals a full parse. |
| M3 | The edit compiler and the printer for new content. | M2 | Done. `tests/fuzz.rs` runs the CI seed. The ignored soak is wired. A no-op insert records nothing. |
| M4 | `Session`: load-time ids, the undo stack, composition, the raw flip. The headless typed view, including Liquid fields. | M3 | Done for the headless session. One stack serves both modes. Preedit writes no frame. Field order, comments, and quoting are views over the fence. |
| M5 | The journal, checkpoint, recovery, and conflict value. | M4 | Done. Torn tail, replay, idempotent second replay, external hash quarantine, clean open writes nothing, clean flush does not rewrite. |
| M6 | The surface: viewport paint, caret, selection, the access tree wired to `access.rs`. `EditorSurface` stays the source mount. | M4 and M5. | The retained surface is `rndr_markdown_ui::MarkdownSurface`, with `examples/live-markdown`. Headless tests cover editing, composition, and the access tree. Native IME and assistive technologies are not certified. §12 was measured: a 1 MiB keystroke is 133 ms because the confirming full parse in §5 is still on the edit path. The fuzzer rejects a graft that skips it. The edited block itself measures in 19 µs. |
| M7 | Native IME and assistive-technology passes on real platforms. `resolve_ime` is not yet called by the session. | M6 | Not started. Headless composition refuses edits and keeps the preedit out of the bytes. |

M5 and M6 can proceed together after M4 because one owns the file and the other owns the view. A conflict at their boundary (the flush API, the composition refusal) comes back to a single owner before either merges. M7 does not gate the headless invariants.

## 15. What stays where it is

- `SourceDocuments`, `EditorSurface`, and `source_projection` keep their current contracts. Live notes do not go through `Projection::replace`.
- `StructuredDocument` stays the schema editor. Paste from it into a note is new text, printed once.
- `TextField` stays a label editor. A Liquid title that looks like a field is a value-span splice on the markdown session.
- Canvas text stays scene content, edited at the end of the string, with canvas history.
- `MarkdownView`, `LiquidStream`, and `liquid_view` stay the read-only preview.
- `AccessCommand::SetValue` stays unsupported.
- Workspace version 0.6.0 stays 0.6.0 through every milestone above.
