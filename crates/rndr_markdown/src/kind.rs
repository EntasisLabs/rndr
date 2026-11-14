//! Syntax kinds for the markdown concrete syntax tree.
//!
//! Kinds name structure. The bytes live on the leaves. A kind never rewrites
//! a delimiter, a line ending, or a fence.

/// A node in the concrete syntax tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SyntaxKind {
    /// The root. A leading BOM is its first child when the file has one.
    Document,
    /// The UTF-8 BOM, kept as leading trivia of the document.
    Bom,
    /// A paragraph. Its children are inlines and the line ending of its last line.
    Paragraph,
    /// An ATX or setext heading. Markers and the setext underline are trivia.
    Heading,
    /// A block quote. `>` prefixes are [`SyntaxKind::LinePrefix`] tokens.
    BlockQuote,
    /// A bullet, ordered, or task list. Tight or loose is derived from gaps.
    List,
    /// One list item: a [`SyntaxKind::ListMarker`] plus child blocks.
    ListItem,
    /// A fenced code block whose info string is not a Liquid language.
    FencedCode,
    /// A fenced code block whose info string is in `FENCE_LANGS`.
    LiquidFence,
    /// An indented code block. The indent bytes are inside the node.
    IndentedCode,
    /// A CommonMark HTML block. The bytes are one opaque child.
    HtmlBlock,
    /// A thematic break. The marker bytes are exact.
    ThematicBreak,
    /// A GFM table.
    Table,
    /// One row of a table, including its line ending.
    TableRow,
    /// One cell. Padding spaces stay in the cell.
    TableCell,
    /// A link reference definition, in place.
    LinkRefDef,
    /// One blank line owned by the parent block container.
    BlockGap,
    /// Bytes the grammar does not structure.
    OpaqueBlock,
    /// Literal text.
    Text,
    /// Emphasis. The delimiter character and length are the delimiter children.
    Emphasis,
    /// Strong emphasis.
    Strong,
    /// GFM strikethrough.
    Strikethrough,
    /// An inline code span, including its backticks.
    InlineCode,
    /// An inline or reference link, including its brackets and destination.
    Link,
    /// An image, including its marker and destination.
    Image,
    /// An autolink `<...>`.
    Autolink,
    /// A hard line break: two spaces or a backslash, plus the line ending.
    HardBreak,
    /// A soft line break inside a paragraph.
    SoftBreak,
    /// A backslash escape, including the backslash.
    Escape,
    /// A character reference, including the `&` and `;`.
    Entity,
    /// A `{{icon:…}}` shortcode, as written.
    IconShortcode,
    /// Inline HTML, kept as raw bytes.
    HtmlInline,
    /// Inline bytes the inline grammar does not structure.
    OpaqueInline,
    /// A quote prefix or a list continuation indent.
    LinePrefix,
    /// A list marker, including the separator spaces after it.
    ListMarker,
    /// An opening or closing fence line, including its line ending.
    FenceLine,
    /// ATX `#` markers, including the space that follows the opening run.
    AtxMarker,
    /// A setext underline, including its line ending.
    SetextUnderline,
    /// A line ending that belongs to a block and is not a soft or hard break.
    LineEnding,
    /// The `|` characters of a table row.
    Pipe,
    /// A `key: value` field inside a Liquid fence.
    Field,
    /// The key of a field, in its original spelling.
    FieldKey,
    /// The colon and the spaces around it.
    FieldSep,
    /// A literal field value, including its quotes when those quotes are the value's trivia.
    FieldValue,
    /// A `#` comment line inside a fence.
    FieldComment,
    /// A `---` item separator inside a fence.
    FieldSeparator,
    /// A `| key: value | key: value` row inside a fence.
    PipeRow,
    /// The `|` or `>` indicator of a YAML block scalar.
    BlockScalar,
    /// A delimiter run (`*`, `_`, `**`, `~~`, backticks of a code span).
    Delimiter,
}

impl SyntaxKind {
    /// Block containers whose children are blocks, gaps, and items.
    pub fn is_block(self) -> bool {
        matches!(
            self,
            Self::Document
                | Self::Paragraph
                | Self::Heading
                | Self::BlockQuote
                | Self::List
                | Self::ListItem
                | Self::FencedCode
                | Self::LiquidFence
                | Self::IndentedCode
                | Self::HtmlBlock
                | Self::ThematicBreak
                | Self::Table
                | Self::TableRow
                | Self::LinkRefDef
                | Self::BlockGap
                | Self::OpaqueBlock
        )
    }
}
