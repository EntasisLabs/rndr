//! Safe HTML subset for structured clipboard exchange. Import keeps marks,
//! links, mention pills, headings, lists and quotes; everything else degrades to
//! text. Scripts, styles and unsafe link schemes never enter the document.
use super::*;

impl StructuredDocument {
    /// Safe HTML for the current nonempty text selection, or `None` when
    /// nothing textual is selected.
    pub fn selection_html(&self) -> Option<String> {
        let range = self.text_selection()?;
        let (a, z) = ordered(&self.state, range).ok()?;
        (a != z).then(|| export(&self.state, a, z))
    }
    /// Safe HTML for the whole document.
    pub fn to_html(&self) -> String {
        let last = self.state.blocks.len().saturating_sub(1);
        let end = self.state.blocks.get(last).map_or(0, Block::len);
        export(&self.state, (0, 0), (last, end))
    }
    /// Replace the selection with pasted HTML as one undoable transaction.
    /// Text and inline marks always land; headings, lists and quotes apply where
    /// the destination allows them and otherwise stay plain paragraphs.
    pub fn paste_html(&mut self, html: &str, origin: EditOrigin) -> Result<bool, StructuredError> {
        let blocks = parse(html);
        let text = blocks
            .iter()
            .map(|b| b.runs.iter().map(|r| r.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        let Some(range) = self.text_selection() else {
            return self.replace_selection(&text, origin);
        };
        if blocks.is_empty() {
            return Ok(false);
        }
        let start = offset(&self.state, range.anchor)?.min(offset(&self.state, range.head)?);
        let mut sim = Simulation {
            state: self.state.clone(),
            next: self.next_block,
            ops: vec![],
        };
        if !sim.try_op(Operation::Replace { range, text }) {
            return self.replace_selection("", origin).map(|_| false);
        }
        let mut line = start;
        let mut lines = vec![];
        for block in &blocks {
            lines.push(line);
            let mut n = line;
            for run in &block.runs {
                let (Ok(anchor), Ok(head)) = (
                    position_at(&sim.state, n, Affinity::After),
                    position_at(&sim.state, n + run.text.len(), Affinity::After),
                ) else {
                    break;
                };
                let mut marks = run.marks.clone();
                if valid_marks(&marks).is_err() {
                    marks.link = None;
                }
                sim.try_op(Operation::SetMarks {
                    range: TextSelection { anchor, head },
                    marks,
                });
                n += run.text.len();
            }
            if let (BlockKind::Heading(_), Ok(at)) =
                (block.kind, position_at(&sim.state, line, Affinity::After))
            {
                sim.try_op(Operation::SetBlockKind {
                    block: at.block,
                    kind: block.kind,
                });
            }
            line = n + 1;
        }
        let mut i = 0;
        while i < blocks.len() {
            let kind = blocks[i].lists.first().copied();
            let quote = blocks[i].quote && kind.is_none();
            let mut j = i;
            while j + 1 < blocks.len()
                && blocks[j + 1].lists.first().copied() == kind
                && (blocks[j + 1].quote && blocks[j + 1].lists.is_empty()) == quote
            {
                j += 1;
            }
            let wrap = kind
                .map(ContainerKind::List)
                .or(quote.then_some(ContainerKind::Quote));
            if let Some(wrap) = wrap {
                let end = lines[j] + blocks[j].runs.iter().map(|r| r.text.len()).sum::<usize>();
                if let (Ok(anchor), Ok(head)) = (
                    position_at(&sim.state, lines[i], Affinity::After),
                    position_at(&sim.state, end, Affinity::After),
                ) {
                    let range = TextSelection { anchor, head };
                    if sim.try_op(Operation::Wrap { range, kind: wrap }) {
                        for k in i..=j {
                            for _ in 1..blocks[k].lists.len() {
                                if let Ok(at) = position_at(&sim.state, lines[k], Affinity::After) {
                                    sim.try_op(Operation::Indent { block: at.block });
                                }
                            }
                        }
                    }
                }
            }
            i = j + 1;
        }
        self.command(sim.ops, origin)
    }
}

// Each candidate operation is trialled on a scratch tree; refused structure is
// dropped so the published transaction replays exactly what succeeded.
struct Simulation {
    state: State,
    next: u64,
    ops: Vec<Operation>,
}
impl Simulation {
    fn try_op(&mut self, op: Operation) -> bool {
        let mut state = self.state.clone();
        let mut next = self.next;
        if apply_operation(&mut state, &mut next, op.clone()).is_err()
            || normalize(&mut state.clone()).is_err()
        {
            return false;
        }
        self.state = state;
        self.next = next;
        self.ops.push(op);
        true
    }
}

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
}

fn container_tags(kind: ContainerKind) -> (String, &'static str) {
    match kind {
        ContainerKind::Table => ("<table>".into(), "</table>"),
        ContainerKind::Row => ("<tr>".into(), "</tr>"),
        ContainerKind::Cell { header: true } => ("<th>".into(), "</th>"),
        ContainerKind::Cell { header: false } => ("<td>".into(), "</td>"),
        ContainerKind::Quote => ("<blockquote>".into(), "</blockquote>"),
        ContainerKind::List(ListKind::Ordered { start }) if start != 1 => {
            (format!("<ol start=\"{start}\">"), "</ol>")
        }
        ContainerKind::List(ListKind::Ordered { .. }) => ("<ol>".into(), "</ol>"),
        ContainerKind::List(_) => ("<ul>".into(), "</ul>"),
        ContainerKind::Item {
            checked: Some(true),
        } => ("<li data-checked=\"true\">".into(), "</li>"),
        ContainerKind::Item {
            checked: Some(false),
        } => ("<li data-checked=\"false\">".into(), "</li>"),
        ContainerKind::Item { checked: None } => ("<li>".into(), "</li>"),
    }
}

fn run_html(run: &Run, out: &mut String) {
    let m = &run.marks;
    let mut close = vec![];
    if let Some(href) = &m.link {
        out.push_str("<a href=\"");
        escape(href, out);
        out.push_str("\">");
        close.push("</a>");
    }
    for (on, open, end) in [
        (m.strong, "<strong>", "</strong>"),
        (m.emphasis, "<em>", "</em>"),
        (m.underline, "<u>", "</u>"),
        (m.strike, "<s>", "</s>"),
        (m.code, "<code>", "</code>"),
    ] {
        if on {
            out.push_str(open);
            close.push(end);
        }
    }
    if let Some(id) = &m.pill {
        out.push_str("<span data-pill=\"");
        escape(id, out);
        out.push_str("\">");
        close.push("</span>");
    }
    escape(&run.text, out);
    for end in close.into_iter().rev() {
        out.push_str(end);
    }
}

fn export(s: &State, a: LocalPosition, z: LocalPosition) -> String {
    let mut out = String::new();
    let mut open: Vec<(ContainerId, &'static str)> = vec![];
    for i in a.0..=z.0 {
        let block = &s.blocks[i];
        let mut chain = vec![];
        let mut parent = block.parent;
        while let Some(id) = parent {
            let Ok(c) = container(s, id) else { break };
            chain.push(c);
            parent = c.parent;
        }
        chain.reverse();
        let keep = open
            .iter()
            .zip(&chain)
            .take_while(|((id, _), c)| *id == c.id)
            .count();
        for (_, end) in open.drain(keep..).rev() {
            out.push_str(end);
        }
        for c in &chain[keep..] {
            let (start, end) = container_tags(c.kind);
            out.push_str(&start);
            open.push((c.id, end));
        }
        let (start, end) = (
            if i == a.0 { a.1 } else { 0 },
            if i == z.0 { z.1 } else { block.len() },
        );
        let tag = match block.kind {
            BlockKind::HorizontalRule => {
                out.push_str("<hr>");
                continue;
            }
            BlockKind::Heading(level) => format!("h{}", level.clamp(1, 6)),
            BlockKind::Paragraph | BlockKind::Atomic => "p".into(),
        };
        out.push_str(&format!("<{tag}>"));
        if block.is_atomic() {
            escape(&block.copy_fallback(), &mut out);
        }
        let mut n = 0;
        for run in &block.runs {
            let (from, to) = (start.max(n), end.min(n + run.text.len()));
            if from < to {
                let part = Run {
                    text: run.text[from - n..to - n].into(),
                    marks: run.marks.clone(),
                };
                run_html(&part, &mut out);
            }
            n += run.text.len();
        }
        out.push_str(&format!("</{tag}>"));
    }
    for (_, end) in open.into_iter().rev() {
        out.push_str(end);
    }
    out
}

/// One imported paragraph or heading with its list nesting and quote context.
#[derive(Debug, Default)]
struct Imported {
    kind: Option<BlockKind>,
    lists: Vec<ListKind>,
    quote: bool,
    runs: Vec<Run>,
}

#[derive(Debug)]
struct Parsed {
    kind: BlockKind,
    lists: Vec<ListKind>,
    quote: bool,
    runs: Vec<Run>,
}

const SKIPPED: [&str; 6] = ["script", "style", "head", "title", "template", "noscript"];
const BLOCKS: [&str; 18] = [
    "p",
    "div",
    "li",
    "ul",
    "ol",
    "blockquote",
    "pre",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "tr",
    "table",
    "hr",
    "section",
    "article",
];

#[derive(Default)]
struct Importer {
    out: Vec<Parsed>,
    current: Option<Imported>,
    marks: Vec<(String, Marks)>,
    lists: Vec<ListKind>,
    quotes: usize,
    pre: usize,
    heading: Option<BlockKind>,
    space: bool,
}

impl Importer {
    fn marks(&self) -> Marks {
        self.marks
            .last()
            .map(|(_, m)| m.clone())
            .unwrap_or_default()
    }
    fn flush(&mut self) {
        if let Some(mut block) = self.current.take() {
            if let Some(last) = block.runs.last_mut() {
                if self.pre == 0 {
                    let trimmed = last.text.trim_end().len();
                    last.text.truncate(trimmed);
                }
            }
            block.runs.retain(|r| !r.text.is_empty());
            if !block.runs.is_empty() || !block.lists.is_empty() {
                self.out.push(Parsed {
                    kind: block.kind.unwrap_or(BlockKind::Paragraph),
                    lists: block.lists,
                    quote: block.quote,
                    runs: block.runs,
                });
            }
        }
        self.space = false;
    }
    fn block(&mut self) -> &mut Imported {
        let (lists, quote, kind) = (self.lists.clone(), self.quotes > 0, self.heading);
        self.current.get_or_insert_with(|| Imported {
            kind,
            lists,
            quote,
            runs: vec![],
        })
    }
    fn push(&mut self, text: &str) {
        let marks = self.marks();
        if self.pre > 0 {
            for (i, line) in text.split('\n').enumerate() {
                if i > 0 {
                    self.block();
                    self.flush();
                }
                if !line.is_empty() {
                    let marks = marks.clone();
                    push_run(self.block(), line.replace('\r', ""), marks);
                }
            }
            return;
        }
        let mut collapsed = String::new();
        let leading = self.current.as_ref().is_none_or(|b| b.runs.is_empty());
        let mut space = self.space || leading;
        for c in text.chars() {
            if c.is_whitespace() && c != '\u{a0}' {
                if !space {
                    collapsed.push(' ');
                }
                space = true;
            } else {
                collapsed.push(if c == '\u{a0}' { ' ' } else { c });
                space = false;
            }
        }
        self.space = space;
        if !collapsed.is_empty() {
            push_run(self.block(), collapsed, marks);
        }
    }
    fn open(&mut self, tag: &str, attrs: &[(String, String)]) {
        if BLOCKS.contains(&tag) {
            self.flush();
        }
        let attr = |name: &str| {
            attrs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        };
        match tag {
            "br" => {
                self.block();
                self.flush();
                return;
            }
            "ul" => self.lists.push(ListKind::Bullet),
            "ol" => self.lists.push(ListKind::Ordered {
                start: attr("start")
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(1),
            }),
            "blockquote" => self.quotes += 1,
            "pre" => self.pre += 1,
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.heading = Some(BlockKind::Heading(tag.as_bytes()[1] - b'0'));
            }
            "td" | "th" => self.push(" "),
            _ => {}
        }
        let mut marks = self.marks();
        let before = marks.clone();
        match tag {
            "b" | "strong" => marks.strong = true,
            "i" | "em" => marks.emphasis = true,
            "u" | "ins" => marks.underline = true,
            "s" | "del" | "strike" => marks.strike = true,
            "code" | "kbd" | "samp" => marks.code = true,
            "a" => {
                let mut probe = Marks {
                    link: attr("href").map(|h| h.trim().to_owned()),
                    ..Marks::default()
                };
                if valid_marks(&probe).is_err() {
                    probe.link = None;
                }
                marks.link = probe.link.or(marks.link);
            }
            _ => {}
        }
        if self.pre > 0 && tag == "pre" {
            marks.code = true;
        }
        if let Some(id) = attr("data-pill") {
            let probe = Marks {
                pill: Some(id.to_owned()),
                ..Marks::default()
            };
            if valid_marks(&probe).is_ok() {
                marks.pill = probe.pill;
            }
        }
        if let Some(style) = attr("style") {
            apply_style(style, &mut marks);
        }
        if marks != before || is_inline_container(tag) || tag == "pre" {
            self.marks.push((tag.to_owned(), marks));
        }
    }
    fn close(&mut self, tag: &str) {
        if let Some(i) = self.marks.iter().rposition(|(t, _)| t == tag) {
            self.marks.truncate(i);
        }
        if BLOCKS.contains(&tag) {
            self.flush();
        }
        match tag {
            "ul" | "ol" => {
                self.lists.pop();
            }
            "blockquote" => self.quotes = self.quotes.saturating_sub(1),
            "pre" => self.pre = self.pre.saturating_sub(1),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => self.heading = None,
            _ => {}
        }
    }
}

fn is_inline_container(tag: &str) -> bool {
    matches!(tag, "span" | "font")
}

fn push_run(block: &mut Imported, text: String, marks: Marks) {
    match block.runs.last_mut() {
        Some(last) if last.marks == marks && marks.pill.is_none() => last.text.push_str(&text),
        _ => block.runs.push(Run { text, marks }),
    }
}

fn apply_style(style: &str, marks: &mut Marks) {
    for decl in style.split(';') {
        let Some((name, value)) = decl.split_once(':') else {
            continue;
        };
        let value = value.trim().to_ascii_lowercase();
        match name.trim().to_ascii_lowercase().as_str() {
            "font-weight" => {
                marks.strong = value == "bold"
                    || value == "bolder"
                    || value.parse::<u32>().is_ok_and(|w| w >= 600);
            }
            "font-style" => marks.emphasis = value == "italic" || value == "oblique",
            "text-decoration" | "text-decoration-line" => {
                marks.underline |= value.contains("underline");
                marks.strike |= value.contains("line-through");
            }
            _ => {}
        }
    }
}

fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest[1..].find(';').map(|e| e + 1).filter(|e| *e <= 10);
        let decoded = end.and_then(|e| {
            let name = &rest[1..e];
            let c = match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => '\u{a0}',
                _ => {
                    let n = if let Some(hex) = name.strip_prefix("#x").or(name.strip_prefix("#X")) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        name.strip_prefix('#').and_then(|d| d.parse().ok())
                    };
                    n.and_then(char::from_u32).filter(|c| *c != '\0')?
                }
            };
            Some((c, e + 1))
        });
        if let Some((c, len)) = decoded {
            out.push(c);
            rest = &rest[len..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn attributes(mut s: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    loop {
        s = s.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if s.is_empty() {
            return out;
        }
        let name_end = s
            .find(|c: char| c.is_whitespace() || c == '=' || c == '/')
            .unwrap_or(s.len());
        let name = s[..name_end].to_ascii_lowercase();
        s = s[name_end..].trim_start();
        let mut value = String::new();
        if let Some(rest) = s.strip_prefix('=') {
            let rest = rest.trim_start();
            let (v, next) = match rest.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let body = &rest[1..];
                    let end = body.find(q).unwrap_or(body.len());
                    (&body[..end], &body[(end + 1).min(body.len())..])
                }
                _ => {
                    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                    (&rest[..end], &rest[end..])
                }
            };
            value = decode(v);
            s = next;
        }
        if !name.is_empty() {
            out.push((name, value));
        }
    }
}

fn parse(html: &str) -> Vec<Parsed> {
    let mut im = Importer::default();
    let mut rest = html;
    let mut skip: Option<String> = None;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            if skip.is_none() {
                im.push(&decode(rest));
            }
            break;
        };
        if lt > 0 && skip.is_none() {
            im.push(&decode(&rest[..lt]));
        }
        rest = &rest[lt..];
        if let Some(body) = rest.strip_prefix("<!--") {
            rest = body.find("-->").map_or("", |e| &body[e + 3..]);
            continue;
        }
        let Some(gt) = rest.find('>') else { break };
        let tag = &rest[1..gt];
        rest = &rest[gt + 1..];
        if tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        let closing = tag.starts_with('/');
        let tag = tag.trim_start_matches('/');
        let name_end = tag
            .find(|c: char| c.is_whitespace() || c == '/')
            .unwrap_or(tag.len());
        let name = tag[..name_end].to_ascii_lowercase();
        if let Some(skipped) = &skip {
            if closing && name == *skipped {
                skip = None;
            }
            continue;
        }
        if SKIPPED.contains(&name.as_str()) {
            if !closing && !tag.ends_with('/') {
                skip = Some(name);
            }
            continue;
        }
        if closing {
            im.close(&name);
        } else {
            im.open(&name, &attributes(&tag[name_end..]));
            if tag.ends_with('/') && name != "br" && name != "hr" {
                im.close(&name);
            }
        }
    }
    im.flush();
    im.out
}

#[cfg(test)]
#[path = "html_tests.rs"]
mod tests;
