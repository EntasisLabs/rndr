//! Per-fence grammars: fence body text to typed props.

use std::sync::LazyLock;

use regex::Regex;

use crate::kv::{
    first, has_pipe_kv, is_block_scalar, is_new_field_line, lf, parse_kv_block, parse_kv_line,
    pipe_labels, re, split_header, split_hr, split_kv, split_table, strip_line_chrome,
    take_block_scalar, Fields, Ids,
};
use crate::props::*;

const CALLOUT_TONES: &[&str] = &["note", "warn", "error", "success", "tip", "important"];
const CHIP_TONES: &[&str] = &["default", "accent", "success", "warn"];

pub(crate) fn one_of(raw: Option<&String>, allowed: &[&str]) -> Option<String> {
    let v = raw?.trim().to_lowercase();
    allowed.contains(&v.as_str()).then_some(v)
}

fn set(target: &mut Option<String>, fields: &Fields, key: &str) {
    if let Some(v) = fields.get(key) {
        *target = Some(v.clone());
    }
}

fn point_value(raw: &str) -> Option<CardPoint> {
    let parts: Vec<&str> = raw
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() < 2 {
        return None;
    }
    Some(CardPoint {
        label: parts[0].to_string(),
        body: parts[1].to_string(),
        emoji: parts.get(2).map(|s| s.to_string()),
        icon: None,
    })
}

fn card_field(fields: &mut Fields, points: &mut Vec<CardPoint>, key: String, value: String) {
    match key.as_str() {
        "point" | "points" => points.extend(point_value(&value)),
        "chips" | "badges" => {
            let joined = match fields.get(&key) {
                Some(prev) => format!("{prev} | {value}"),
                None => value,
            };
            fields.insert(key, joined);
        }
        _ => {
            fields.entry(key).or_insert(value);
        }
    }
}

fn card_from_fields(fields: &Fields) -> Option<CardProps> {
    let title = fields.get("title")?.trim().to_string();
    if title.is_empty() {
        return None;
    }
    let mut card = CardProps {
        title,
        ..Default::default()
    };
    set(&mut card.subtitle, fields, "subtitle");
    set(&mut card.body, fields, "body");
    set(&mut card.emoji, fields, "emoji");
    set(&mut card.icon, fields, "icon");
    set(&mut card.image, fields, "image");
    Some(card)
}

pub(crate) fn card_block(block: &str) -> Option<CardProps> {
    let mut fields = Fields::new();
    let mut points = Vec::new();
    let mut freeform: Vec<String> = Vec::new();
    let text = lf(block);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        let line = strip_line_chrome(lines[i]);
        i += 1;
        if line.is_empty() || line.starts_with('#') || line == "---" {
            if line.is_empty() && !freeform.is_empty() {
                freeform.push(String::new());
            }
            continue;
        }
        if has_pipe_kv(&line) {
            for (k, v) in parse_kv_line(&line) {
                card_field(&mut fields, &mut points, k, v);
            }
            continue;
        }
        if !is_new_field_line(&line) {
            freeform.push(line);
            continue;
        }
        let Some((key, value)) = split_kv(&line) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        if is_block_scalar(&value) {
            let joined = take_block_scalar(&lines, &mut i);
            if !joined.is_empty() {
                fields.entry(key).or_insert(joined);
            }
            continue;
        }
        if value.is_empty() {
            continue;
        }
        card_field(&mut fields, &mut points, key, value);
    }

    let prose = freeform.join("\n").trim().to_string();
    if !prose.is_empty() {
        let summary = if let Some(s) = fields.get("summary") {
            format!("{}\n\n{prose}", s.trim())
        } else if let Some(b) = fields.get("body") {
            format!("{}\n\n{prose}", b.trim())
        } else {
            prose
        };
        fields.insert("summary".into(), summary);
    }

    let mut card = card_from_fields(&fields)?;
    set(&mut card.meta, &fields, "meta");
    set(&mut card.summary, &fields, "summary");
    let chips = pipe_labels(fields.get("chips"));
    if !chips.is_empty() {
        card.chips = Some(chips);
    }
    let badges = pipe_labels(fields.get("badges"));
    if !badges.is_empty() {
        card.badges = Some(badges);
    }
    if !points.is_empty() {
        card.points = Some(points);
    }
    Some(card)
}

pub(crate) fn card(body: &str) -> Option<CardProps> {
    let normalized = lf(body);
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    let parts = split_hr(normalized);
    let mut card = card_block(parts[0])?;
    if card.summary.is_none() && parts.len() > 1 {
        let prose = parts[1..]
            .iter()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        if !prose.is_empty() {
            card.summary = Some(prose);
        }
    }
    Some(card)
}

pub(crate) fn carousel(body: &str) -> Vec<CardProps> {
    let normalized = lf(body);
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return Vec::new();
    }
    let parts = split_hr(normalized);
    if parts.len() > 1 {
        return parts[1..].iter().filter_map(|b| card_block(b)).collect();
    }
    let mut cards = Vec::new();
    for raw in normalized.split('\n') {
        let line = strip_line_chrome(raw);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = parse_kv_line(&line);
        let Some(mut card) = card_from_fields(&fields) else {
            continue;
        };
        let badges = pipe_labels(fields.get("badges"));
        if !badges.is_empty() {
            card.badges = Some(badges);
        }
        cards.push(card);
    }
    cards
}

re!(LABEL_PREFIX, r"(?i)^label\s*:\s*");
re!(INTENT_PREFIX, r"(?i)^intent\s*:\s*");
re!(
    EMOJI_LEAD,
    r"^(\p{Extended_Pictographic}|\p{Emoji_Presentation})\s+(.+)$"
);
re!(CHIP_META_LABEL, r"(?i)^(tone|value)\s*:");

pub(crate) fn actions(body: &str) -> Vec<ActionProps> {
    let mut out = Vec::new();
    for raw in body.split('\n') {
        let line = strip_line_chrome(raw);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = LABEL_PREFIX.replace(&line, "").into_owned();
        let (mut label, intent) = match line.find('|') {
            Some(p) => (line[..p].trim().to_string(), Some(line[p + 1..].trim())),
            None => (line.clone(), None),
        };
        let intent = intent
            .filter(|s| !s.is_empty())
            .map(|s| INTENT_PREFIX.replace(s, "").trim().to_string())
            .filter(|s| !s.is_empty());
        let mut emoji = None;
        if let Some(caps) = EMOJI_LEAD.captures(&label) {
            emoji = Some(caps[1].to_string());
            label = caps[2].trim().to_string();
        }
        if label.is_empty() {
            continue;
        }
        out.push(ActionProps {
            label,
            intent,
            emoji,
            icon: None,
        });
    }
    out
}

pub(crate) fn callout(body: &str) -> Option<CalloutProps> {
    let fields = parse_kv_block(body);
    let text = fields.get("body")?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(CalloutProps {
        body: text,
        tone: one_of(fields.get("tone"), CALLOUT_TONES),
        title: fields.get("title").cloned(),
    })
}

pub(crate) fn section(body: &str) -> Option<SectionProps> {
    let normalized = lf(body);
    let (header, rest) = split_header(&normalized);
    let prose = rest.map(str::trim).filter(|s| !s.is_empty());
    let fields = parse_kv_block(header);
    let title = fields.get("title")?.trim().to_string();
    if title.is_empty() {
        return None;
    }
    Some(SectionProps {
        title,
        subtitle: fields.get("subtitle").cloned(),
        body: fields.get("body").cloned().or(prose.map(String::from)),
    })
}

pub(crate) fn chips(body: &str) -> Vec<ChipProps> {
    let mut out = Vec::new();
    for raw in body.split('\n') {
        let line = strip_line_chrome(raw);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = LABEL_PREFIX.replace(&line, "").into_owned();
        let (label, rest) = match line.find('|') {
            Some(p) => (line[..p].trim(), line[p + 1..].trim()),
            None => (line.as_str(), ""),
        };
        if label.is_empty() || CHIP_META_LABEL.is_match(label) {
            continue;
        }
        let mut meta = Fields::new();
        for part in rest.split('|').map(str::trim).filter(|p| !p.is_empty()) {
            if let Some((k, v)) = split_kv(part) {
                if !k.is_empty() && !v.is_empty() {
                    meta.insert(k, v);
                }
            }
        }
        out.push(ChipProps {
            label: label.to_string(),
            tone: one_of(meta.get("tone"), CHIP_TONES),
            value: meta.get("value").cloned(),
        });
    }
    out
}

pub(crate) fn media(body: &str) -> Option<MediaProps> {
    let fields = parse_kv_block(body);
    let src = fields.get("src")?.trim().to_string();
    if src.is_empty() {
        return None;
    }
    Some(MediaProps {
        src,
        alt: fields.get("alt").cloned(),
        caption: fields.get("caption").cloned(),
        ratio: fields.get("ratio").cloned(),
    })
}

pub(crate) fn feed(body: &str) -> Option<FeedProps> {
    let fields = parse_kv_block(body);
    let feed_id = first(&fields, &["id", "feed", "feed_id"]).filter(|s| !s.is_empty())?;
    let datatype = one_of(
        fields.get("datatype"),
        &["md", "text", "json", "csv", "image"],
    )?;
    let trimmed = |k: &str| {
        fields
            .get(k)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    Some(FeedProps {
        feed_id,
        datatype,
        title: trimmed("title"),
        empty: trimmed("empty"),
        refresh: one_of(fields.get("refresh"), &["manual", "load"]),
    })
}

pub(crate) fn cite(body: &str) -> Option<CiteProps> {
    let fields = parse_kv_block(body);
    let cite = CiteProps {
        quote: fields.get("quote").or(fields.get("body")).cloned(),
        title: fields.get("title").cloned(),
        url: fields.get("url").cloned(),
        source: fields.get("source").cloned(),
    };
    (cite.quote.is_some() || cite.title.is_some() || cite.url.is_some()).then_some(cite)
}

pub(crate) fn compare(body: &str) -> Option<CompareProps> {
    let mut ids = Ids::default();
    let (preamble, rows) = split_table(body);
    if rows.len() < 2 || rows[0].len() < 3 {
        return None;
    }
    let labels: Vec<&str> = rows[0][1..]
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    if labels.len() < 2 {
        return None;
    }
    let mut entities: Vec<CompareEntity> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| CompareEntity {
            id: ids.slug(label, "entity", i),
            label: label.to_string(),
            values: Default::default(),
        })
        .collect();
    let mut axes: Vec<CompareAxis> = Vec::new();
    for cells in &rows[1..] {
        let axis_label = cells.first().map_or("", |c| c.trim());
        if axis_label.is_empty() {
            continue;
        }
        let id = ids.slug(axis_label, "axis", axes.len());
        for (e, entity) in entities.iter_mut().enumerate() {
            let value = cells.get(e + 1).map_or("", |c| c.trim());
            if !value.is_empty() {
                entity.values.insert(id.clone(), value.to_string());
            }
        }
        axes.push(CompareAxis {
            id,
            label: axis_label.to_string(),
        });
    }
    if axes.is_empty() {
        return None;
    }
    let fields = parse_kv_block(&preamble);
    let mode = fields.get("mode").map(|m| {
        MODE_SEP
            .replace_all(&m.trim().to_lowercase(), "-")
            .into_owned()
    });
    Some(CompareProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        recommendation: first(&fields, &["recommendation", "highlight"]).filter(|s| !s.is_empty()),
        mode: match mode.as_deref() {
            Some("faceoff" | "face-off") => Some("faceoff".into()),
            Some("matrix") => Some("matrix".into()),
            _ => None,
        },
        axes,
        entities,
    })
}

re!(MODE_SEP, r"[_ ]+");

/// Preamble fields plus one field map per `---` block.
fn blocks(body: &str) -> Option<(Fields, Vec<Fields>)> {
    let normalized = lf(body);
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    let parts = split_hr(normalized);
    Some((
        parse_kv_block(parts[0]),
        parts[1..].iter().map(|b| parse_kv_block(b)).collect(),
    ))
}

fn label_of(fields: &Fields) -> Option<String> {
    first(fields, &["label", "title"]).filter(|s| !s.is_empty())
}

pub(crate) fn plan(body: &str) -> Option<PlanProps> {
    let mut ids = Ids::default();
    let (fields, items) = blocks(body)?;
    let mut segments = Vec::new();
    for f in &items {
        let Some(label) = label_of(f) else { continue };
        let mut seg = PlanSegment {
            id: ids.slug(&label, "segment", segments.len()),
            label,
            ..Default::default()
        };
        set(&mut seg.time, f, "time");
        set(&mut seg.emoji, f, "emoji");
        set(&mut seg.icon, f, "icon");
        set(&mut seg.image, f, "image");
        set(&mut seg.subtitle, f, "subtitle");
        set(&mut seg.body, f, "body");
        set(&mut seg.badge, f, "badge");
        segments.push(seg);
    }
    if segments.len() < 2 {
        return None;
    }
    Some(PlanProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        grouping: one_of(fields.get("grouping"), &["day", "phase", "milestone"]),
        segments,
    })
}

pub(crate) fn timeline(body: &str) -> Option<TimelineProps> {
    let mut ids = Ids::default();
    let (fields, items) = blocks(body)?;
    let mut events = Vec::new();
    for f in &items {
        let Some(label) = label_of(f) else { continue };
        let mut ev = TimelineEvent {
            id: ids.slug(&label, "event", events.len()),
            label,
            ..Default::default()
        };
        ev.ts = f.get("ts").or(f.get("time")).cloned();
        set(&mut ev.detail, f, "detail");
        if let Some(b) = f.get("body") {
            ev.body = Some(b.clone());
            ev.detail.get_or_insert_with(|| b.clone());
        }
        set(&mut ev.lane, f, "lane");
        set(&mut ev.emoji, f, "emoji");
        set(&mut ev.icon, f, "icon");
        set(&mut ev.meta, f, "meta");
        if let Some(img) = f.get("image") {
            ev.image = Some(img.clone());
        } else {
            set(&mut ev.media, f, "media");
        }
        events.push(ev);
    }
    if events.len() < 2 {
        return None;
    }
    Some(TimelineProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        granularity: one_of(fields.get("granularity"), &["day", "hour", "event"]),
        layout: one_of(fields.get("layout"), &["rail", "snapshot"]),
        events,
    })
}

pub(crate) fn shortlist(body: &str) -> Option<ShortlistProps> {
    let mut ids = Ids::default();
    let (fields, blocks) = blocks(body)?;
    let mut items = Vec::new();
    for f in &blocks {
        let Some(label) = label_of(f) else { continue };
        let mut item = ShortlistItem {
            id: ids.slug(&label, "item", items.len()),
            label,
            ..Default::default()
        };
        item.summary = f.get("summary").or(f.get("body")).cloned();
        set(&mut item.score, f, "score");
        set(&mut item.meta, f, "meta");
        set(&mut item.emoji, f, "emoji");
        set(&mut item.icon, f, "icon");
        set(&mut item.image, f, "image");
        items.push(item);
    }
    if items.len() < 2 {
        return None;
    }
    Some(ShortlistProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        criteria: fields.get("criteria").cloned(),
        density: one_of(fields.get("density"), &["comfortable", "compact"]),
        items,
    })
}

re!(TRADEOFF_SPLIT, r"[|\n;]");
re!(TRADEOFF_PREFIX, r"(?i)^(pros?|cons?)\s*:\s*");

fn tradeoffs(raw: Option<&String>) -> Vec<String> {
    let Some(raw) = raw.filter(|r| !r.trim().is_empty()) else {
        return Vec::new();
    };
    TRADEOFF_SPLIT
        .split(raw)
        .map(|s| {
            TRADEOFF_PREFIX
                .replace(&strip_line_chrome(s), "")
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn decision_option_fields(block: &str) -> Fields {
    let mut fields = Fields::new();
    for raw in block.split('\n') {
        let line = strip_line_chrome(raw);
        if line.is_empty() || line.starts_with('#') || line == "---" {
            continue;
        }
        let Some((key, value)) = split_kv(&line) else {
            continue;
        };
        if key.is_empty() || value.is_empty() {
            continue;
        }
        if matches!(key.as_str(), "pros" | "pro" | "cons" | "con") {
            let joined = match fields.get(&key) {
                Some(prev) => format!("{prev} | {value}"),
                None => value,
            };
            fields.insert(key, joined);
        } else {
            fields.insert(key, value);
        }
    }
    fields
}

pub(crate) fn decision(body: &str) -> Option<DecisionProps> {
    let mut ids = Ids::default();
    let normalized = lf(body);
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    let parts = split_hr(normalized);
    let fields = parse_kv_block(parts[0]);
    let mut options = Vec::new();
    for block in &parts[1..] {
        let f = decision_option_fields(block);
        let Some(label) = label_of(&f) else { continue };
        options.push(DecisionOption {
            id: ids.slug(&label, "option", options.len()),
            label,
            pros: tradeoffs(f.get("pros").or(f.get("pro"))),
            cons: tradeoffs(f.get("cons").or(f.get("con"))),
            score: f.get("score").cloned(),
            summary: f.get("summary").or(f.get("body")).cloned(),
        });
    }
    if options.len() < 2 {
        return None;
    }
    Some(DecisionProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        factors: fields.get("factors").cloned(),
        recommendation: first(&fields, &["recommendation", "highlight"]).filter(|s| !s.is_empty()),
        options,
    })
}

re!(BRIEF_SEP, r"(?m)^[ \t]*---[ \t]*$");
re!(BRIEF_SEP_FIRST, r"(?m)^[ \t]*---[ \t]*$");
re!(BRIEF_SOURCES, r"(?m)^[ \t]*===[ \t]*$");
re!(LEAD_WS, r"^[ \t]*");
re!(FIELD_LINE, r"^([a-zA-Z][a-zA-Z0-9_-]*)\s*:(.*)$");
re!(LIST_LEAD, r"^[-*+]\s+");
re!(ORDERED_LEAD, r"^\d+\.\s+");
re!(ATX, r"^#{1,6}\s+(.+?)(?:\s+#*)?$");
re!(ATX_TAIL, r"\s+#*$");
re!(HAS_ATX, r"(?m)^[ \t]*#{1,6}\s+\S");
re!(BRIEF_CHROME, r"(?i)^(title|subtitle|tone)\s*:");
re!(BRIEF_TONE_SPLIT, r"[,/|]");

fn indent_of(line: &str) -> usize {
    LEAD_WS.find(line).map_or(0, |m| m.end())
}

fn brief_field_line(raw: &str, allow_list_marker: bool) -> Option<(String, String, usize)> {
    let indent = indent_of(raw);
    let mut line = raw[indent..].to_string();
    if allow_list_marker {
        line = LIST_LEAD.replace(&line, "").into_owned();
        line = ORDERED_LEAD.replace(&line, "").into_owned();
    }
    let caps = FIELD_LINE.captures(&line)?;
    Some((caps[1].to_lowercase(), caps[2].trim().to_string(), indent))
}

fn dedent(lines: &[&str]) -> String {
    let common = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| indent_of(l))
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                ""
            } else {
                &l[common.min(indent_of(l))..]
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn brief_record(block: &str, allowed: &[&str], content: &[&str]) -> Fields {
    let mut fields = Fields::new();
    let text = lf(block);
    let mut active: Option<(String, String)> = None;
    let mut continuation: Vec<&str> = Vec::new();
    let mut record_indent: Option<usize> = None;

    let mut flush = |active: &mut Option<(String, String)>, continuation: &mut Vec<&str>| {
        let Some((key, value)) = active.take() else {
            return;
        };
        if content.contains(&key.as_str()) {
            let cont = dedent(continuation);
            if is_block_scalar(&value) {
                if !cont.is_empty() {
                    fields.insert(key, cont);
                }
            } else if !cont.is_empty() {
                let v = if value.is_empty() {
                    cont
                } else {
                    format!("{value}\n\n{cont}").trim().to_string()
                };
                fields.insert(key, v);
            } else if !value.is_empty() {
                fields.insert(key, value);
            }
        } else if !value.is_empty() {
            fields.insert(key, value);
        }
        continuation.clear();
    };

    for raw in text.split('\n') {
        let field = brief_field_line(raw, active.is_none());
        if let Some((key, value, indent)) = field.filter(|(k, _, i)| {
            allowed.contains(&k.as_str()) && record_indent.is_none_or(|r| *i <= r)
        }) {
            record_indent.get_or_insert(indent);
            flush(&mut active, &mut continuation);
            active = Some((key, value));
            continue;
        }
        if active
            .as_ref()
            .is_some_and(|(k, _)| content.contains(&k.as_str()))
        {
            continuation.push(raw);
        }
    }
    flush(&mut active, &mut continuation);
    fields
}

fn atx_heading(line: &str) -> Option<String> {
    let caps = ATX.captures(line.trim())?;
    let h = ATX_TAIL.replace(&caps[1], "").trim().to_string();
    (!h.is_empty()).then_some(h)
}

fn atx_sections(block: &str, start: usize) -> Vec<BriefSection> {
    let mut ids = Ids::default();
    let text = lf(block);
    let mut sections: Vec<BriefSection> = Vec::new();
    let mut heading: Option<String> = None;
    let mut body: Vec<&str> = Vec::new();
    let mut lead: Vec<&str> = Vec::new();
    let mut flush =
        |heading: &Option<String>, body: &mut Vec<&str>, sections: &mut Vec<BriefSection>| {
            let text = body.join("\n").trim().to_string();
            body.clear();
            if let Some(h) = heading.as_ref().filter(|_| !text.is_empty()) {
                sections.push(BriefSection {
                    id: ids.slug(h, "section", start + sections.len()),
                    heading: h.clone(),
                    body: text,
                });
            }
        };
    for line in text.split('\n') {
        if let Some(h) = atx_heading(line) {
            flush(&heading, &mut body, &mut sections);
            heading = Some(h);
            continue;
        }
        if heading.is_some() {
            body.push(line);
        } else if !line.trim().is_empty() {
            lead.push(line);
        }
    }
    flush(&heading, &mut body, &mut sections);
    let lead = lead.join("\n").trim().to_string();
    if !lead.is_empty() {
        if let Some(first) = sections.first_mut() {
            first.body = format!("{lead}\n\n{}", first.body).trim().to_string();
        }
    }
    sections
}

fn brief_section_block(block: &str, index: usize) -> Option<BriefSection> {
    let normalized = lf(block);
    let normalized = normalized.trim_end();
    if normalized.trim().is_empty() {
        return None;
    }
    let (header, prose) = match BRIEF_SEP.find(normalized) {
        Some(m) => {
            let rest = &normalized[m.start()..];
            let rest = BRIEF_SEP_FIRST.replace(rest, "");
            let rest = rest.trim();
            (
                &normalized[..m.start()],
                (!rest.is_empty()).then(|| rest.to_string()),
            )
        }
        None => (normalized, None),
    };
    let fields = brief_record(header, &["heading", "title", "body"], &["body"]);
    if let Some(heading) = first(&fields, &["heading", "title"]).filter(|h| !h.is_empty()) {
        let body = fields
            .get("body")
            .cloned()
            .or(prose)
            .map(|b| b.trim().to_string());
        if let Some(body) = body.filter(|b| !b.is_empty()) {
            return Some(BriefSection {
                id: Ids::default().slug(&heading, "section", index),
                heading,
                body,
            });
        }
    }
    atx_sections(normalized, index).into_iter().next()
}

fn brief_source_block(block: &str, index: usize) -> Option<BriefSource> {
    let fields = brief_record(
        block,
        &["title", "label", "url", "quote", "body", "source"],
        &["quote", "body"],
    );
    let title = first(&fields, &["title", "label"]).filter(|t| !t.is_empty())?;
    Some(BriefSource {
        id: Ids::default().slug(&title, "source", index),
        title,
        url: fields.get("url").cloned(),
        quote: fields.get("quote").or(fields.get("body")).cloned(),
    })
}

pub(crate) fn brief(body: &str) -> Option<BriefProps> {
    let mut ids = Ids::default();
    let normalized = lf(body);
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    let eq: Vec<&str> = BRIEF_SOURCES.split(normalized).collect();
    let main = eq[0].trim();
    let sources_part = if eq.len() > 1 {
        eq[1..].join("\n===\n").trim().to_string()
    } else {
        String::new()
    };
    let parts: Vec<&str> = BRIEF_SEP.split(main).collect();
    let fields = parse_kv_block(parts[0]);
    let section_blocks = &parts[1..];
    let mut sections: Vec<BriefSection> = Vec::new();
    let mut pending: Option<String> = None;

    for block in section_blocks {
        if HAS_ATX.is_match(block) {
            let many = atx_sections(block, sections.len());
            if !many.is_empty() {
                sections.extend(many);
                pending = None;
                continue;
            }
        }
        if let Some(section) = brief_section_block(block, sections.len()) {
            sections.push(section);
            pending = None;
            continue;
        }
        let bf = brief_record(block, &["heading", "title", "body"], &["body"]);
        if let Some(h) = first(&bf, &["heading", "title"]).filter(|h| !h.is_empty()) {
            if !bf.contains_key("body") {
                pending = Some(h);
                continue;
            }
        }
        if let Some(h) = pending.clone() {
            let prose = lf(block).trim().to_string();
            if !prose.is_empty() {
                sections.push(BriefSection {
                    id: ids.slug(&h, "section", sections.len()),
                    heading: h,
                    body: prose,
                });
                pending = None;
            }
        }
    }

    if sections.is_empty() {
        if section_blocks.is_empty() && HAS_ATX.is_match(main) {
            let has_chrome = ["title", "subtitle", "tone"]
                .iter()
                .any(|k| fields.contains_key(*k));
            let source = if has_chrome {
                main.split('\n')
                    .filter(|line| {
                        let t = strip_line_chrome(line);
                        t.is_empty() || !BRIEF_CHROME.is_match(&t)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                main.to_string()
            };
            sections.extend(atx_sections(&source, 0));
        } else if !section_blocks.is_empty() {
            let rest = section_blocks
                .iter()
                .map(|b| b.trim())
                .filter(|b| !b.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
            if !rest.is_empty() {
                let atx = if HAS_ATX.is_match(&rest) {
                    atx_sections(&rest, 0)
                } else {
                    Vec::new()
                };
                if !atx.is_empty() {
                    sections.extend(atx);
                } else {
                    let heading = fields
                        .get("title")
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| "Overview".into());
                    sections.push(BriefSection {
                        id: ids.slug(&heading, "section", 0),
                        heading,
                        body: rest,
                    });
                }
            }
        }
    }
    if sections.is_empty() {
        return None;
    }

    let mut sources = Vec::new();
    if !sources_part.is_empty() {
        for block in BRIEF_SEP
            .split(&sources_part)
            .filter(|b| !b.trim().is_empty())
        {
            sources.extend(brief_source_block(block, sources.len()));
        }
    }

    let tone = fields
        .get("tone")
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty());
    let tones = ["research", "brief", "memo"];
    let tone = tone.map(|t| {
        let primary = BRIEF_TONE_SPLIT
            .split(&t)
            .next()
            .unwrap_or(&t)
            .trim()
            .to_string();
        if tones.contains(&primary.as_str()) {
            primary
        } else if tones.contains(&t.as_str()) {
            t
        } else {
            "research".to_string()
        }
    });
    Some(BriefProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        tone,
        sections,
        sources: (!sources.is_empty()).then_some(sources),
    })
}

pub(crate) fn dashboard(body: &str) -> Option<DashboardProps> {
    let mut ids = Ids::default();
    let (fields, blocks) = blocks(body)?;
    let mut tiles = Vec::new();
    for f in &blocks {
        let Some(label) = label_of(f) else { continue };
        let Some(value) = f
            .get("value")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        let mut tile = DashboardTile {
            id: ids.slug(&label, "tile", tiles.len()),
            label,
            value,
            ..Default::default()
        };
        set(&mut tile.delta, f, "delta");
        set(&mut tile.emoji, f, "emoji");
        set(&mut tile.icon, f, "icon");
        tile.hint = f.get("hint").or(f.get("body")).cloned();
        set(&mut tile.unit, f, "unit");
        tile.tone = one_of(
            f.get("tone"),
            &["default", "accent", "success", "warn", "error"],
        );
        tile.feed = f
            .get("feed")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        tile.field = f
            .get("field")
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        tile.refresh = one_of(f.get("refresh"), &["manual", "interval", "both"]);
        tile.interval_ms = f.get("interval").and_then(|raw| duration_ms(Some(raw)));
        tiles.push(tile);
    }
    if tiles.len() < 2 {
        return None;
    }
    Some(DashboardProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        columns: one_of(fields.get("columns"), &["2", "3", "4"]),
        tiles,
    })
}

re!(KEY_LEAD, r"^[a-zA-Z][a-zA-Z0-9_-]*\s*:");

pub(crate) fn report(body: &str) -> Option<ReportProps> {
    let normalized = lf(body);
    let (preamble, rest) =
        crate::kv::leading_preamble(&normalized, |s| KEY_LEAD.is_match(s) && !s.starts_with('|'));
    let fields = parse_kv_block(&preamble);
    let markdown = rest.trim().to_string();
    if markdown.is_empty() && !fields.contains_key("title") {
        return None;
    }
    Some(ReportProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        columns: Some(
            one_of(fields.get("columns"), &["1", "2", "3"]).unwrap_or_else(|| "2".into()),
        ),
        body: markdown,
    })
}

/// Shared `---` item blocks for tabs / steps / accordion.
fn labeled_blocks(body: &str) -> Option<(Fields, Vec<Fields>)> {
    let (fields, items) = blocks(body)?;
    let mut out = Vec::new();
    for mut f in items {
        if label_of(&f).is_none() {
            continue;
        }
        if !f.contains_key("body") {
            if let Some(s) = f.get("summary").cloned() {
                f.insert("body".into(), s);
            }
        }
        out.push(f);
    }
    (!out.is_empty()).then_some((fields, out))
}

fn body_of(f: &Fields) -> Option<String> {
    first(f, &["body", "summary"]).filter(|b| !b.is_empty())
}

pub(crate) fn tabs(body: &str) -> Option<TabsProps> {
    let mut ids = Ids::default();
    let (fields, blocks) = labeled_blocks(body)?;
    let mut panels = Vec::new();
    for f in &blocks {
        let (Some(label), Some(body)) = (label_of(f), body_of(f)) else {
            continue;
        };
        panels.push(TabsPanel {
            id: ids.slug(&label, "tab", panels.len()),
            label,
            body,
            emoji: f.get("emoji").cloned(),
            icon: f.get("icon").cloned(),
        });
    }
    if panels.len() < 2 {
        return None;
    }
    Some(TabsProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        default: first(&fields, &["default", "active"]).filter(|d| !d.is_empty()),
        panels,
    })
}

pub(crate) fn steps(body: &str) -> Option<StepsProps> {
    let mut ids = Ids::default();
    let (fields, blocks) = labeled_blocks(body)?;
    let mut steps = Vec::new();
    for f in &blocks {
        let Some(label) = label_of(f) else { continue };
        steps.push(StepItem {
            id: ids.slug(&label, "step", steps.len()),
            label,
            body: body_of(f),
            status: one_of(f.get("status"), &["done", "current", "pending"]),
            emoji: f.get("emoji").cloned(),
            icon: f.get("icon").cloned(),
        });
    }
    if steps.len() < 2 {
        return None;
    }
    Some(StepsProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        steps,
    })
}

re!(
    DURATION,
    r"(\d+(?:\.\d+)?)\s*(h(?:ours?)?|m(?:in(?:utes?)?)?|s(?:ec(?:onds?)?)?)"
);

/// `"5 min"`, `"1h 30m"`, `"90s"` to milliseconds, within 1 s ..= 24 h.
pub(crate) fn duration_ms(raw: Option<&str>) -> Option<u64> {
    let source = raw?.trim().to_lowercase();
    if source.is_empty() {
        return None;
    }
    let mut total = 0.0;
    let mut matched = 0;
    for caps in DURATION.captures_iter(&source) {
        let Ok(value) = caps[1].parse::<f64>() else {
            continue;
        };
        let unit = match caps[2].as_bytes()[0] {
            b'h' => 3_600_000.0,
            b'm' => 60_000.0,
            _ => 1_000.0,
        };
        total += value * unit;
        matched += caps[0].len();
    }
    if matched == 0 || !DURATION.replace_all(&source, "").trim().is_empty() {
        return None;
    }
    (1_000.0..=24.0 * 3_600_000.0)
        .contains(&total)
        .then(|| total.round() as u64)
}

re!(RECIPE_SECTION, r"^([a-zA-Z][A-Za-z0-9_-]*)\s*:\s*$");
re!(RECIPE_INGREDIENT, r"(?i)^ingredient\s*:\s*(.+)$");
re!(RECIPE_RESOURCE, r"(?i)^resource\s*:\s*(.+)$");
re!(RECIPE_ACTION, r"(?i)^(?:[-*+]\s*)?action\s*:\s*(.+)$");

fn recipe_list(preamble: &str, heading: &str, repeated: &Regex) -> Vec<String> {
    let mut values = Vec::new();
    let mut active = false;
    for raw in lf(preamble).split('\n') {
        let line = raw.trim();
        if let Some(caps) = RECIPE_SECTION.captures(line) {
            active = caps[1].to_lowercase() == heading;
            continue;
        }
        if let Some(caps) = repeated.captures(line) {
            values.push(caps[1].trim().to_string());
            active = false;
            continue;
        }
        let is_item = LIST_MARKER_RE.is_match(line);
        if active && is_item {
            values.push(LIST_MARKER_RE.replace(line, "").trim().to_string());
        } else if !line.is_empty() && !is_item {
            active = false;
        }
    }
    values.retain(|v| !v.is_empty());
    values.truncate(64);
    values
}

static LIST_MARKER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[-*+]\s+").unwrap());

fn recipe_actions(preamble: &str) -> Vec<ActionProps> {
    let mut out = Vec::new();
    for raw in lf(preamble).split('\n') {
        let Some(caps) = RECIPE_ACTION.captures(raw.trim()) else {
            continue;
        };
        let mut parts = caps[1].split('|').map(str::trim);
        let label = parts.next().unwrap_or("");
        let intent = parts.next().filter(|s| !s.is_empty()).unwrap_or(label);
        if !label.is_empty() {
            out.push(ActionProps {
                label: label.to_string(),
                intent: Some(intent.to_string()),
                ..Default::default()
            });
        }
    }
    out.truncate(3);
    out
}

pub(crate) fn recipe(body: &str) -> Option<RecipeProps> {
    let normalized = lf(body);
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    let parts = split_hr(normalized);
    let preamble = parts[0];
    let fields = parse_kv_block(preamble);
    let title = first(&fields, &["title", "name"]).filter(|t| !t.is_empty())?;
    let mut ids = Ids::default();
    let mut steps = Vec::new();
    for block in &parts[1..] {
        let f = parse_kv_block(block);
        let Some(label) = label_of(&f) else { continue };
        let mut step = RecipeStep {
            id: ids.slug(&label, "step", steps.len()),
            label,
            body: body_of(&f),
            ..Default::default()
        };
        let label = first(&f, &["duration", "timer"]).filter(|d| !d.is_empty());
        if let Some(ms) = duration_ms(label.as_deref()) {
            step.duration_label = label;
            step.duration_ms = Some(ms);
        }
        steps.push(step);
    }
    if steps.is_empty() {
        return None;
    }
    let nonempty = |v: Vec<String>| (!v.is_empty()).then_some(v);
    let actions = recipe_actions(preamble);
    Some(RecipeProps {
        title,
        subtitle: fields.get("subtitle").cloned(),
        r#yield: fields.get("yield").or(fields.get("servings")).cloned(),
        ingredients: nonempty(recipe_list(preamble, "ingredients", &RECIPE_INGREDIENT)),
        resources: nonempty(recipe_list(preamble, "resources", &RECIPE_RESOURCE)),
        steps,
        notes: fields.get("notes").or(fields.get("note")).cloned(),
        actions: (!actions.is_empty()).then_some(actions),
    })
}

re!(ACCORDION_ID, r"^[a-zA-Z][A-Za-z0-9_:-]*$");

pub(crate) fn accordion(body: &str) -> Option<AccordionProps> {
    let mut ids = Ids::default();
    let (fields, blocks) = labeled_blocks(body)?;
    let mut items = Vec::new();
    for f in &blocks {
        let (Some(label), Some(body)) = (label_of(f), body_of(f)) else {
            continue;
        };
        let explicit = f
            .get("id")
            .map(|v| v.trim().strip_prefix('#').unwrap_or(v.trim()).to_string())
            .unwrap_or_default();
        let id = if !explicit.is_empty() && ACCORDION_ID.is_match(&explicit) {
            ids.claim(explicit)
        } else {
            ids.slug(&label, "item", items.len())
        };
        items.push(AccordionItem {
            id,
            label,
            body,
            open: crate::kv::bool_loose(f.get("open"))
                .or_else(|| crate::kv::bool_loose(f.get("default")))
                .or_else(|| crate::kv::bool_loose(f.get("expanded"))),
            emoji: f.get("emoji").cloned(),
            icon: f.get("icon").cloned(),
        });
    }
    if items.is_empty() {
        return None;
    }
    Some(AccordionProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        multiple: crate::kv::bool_loose(fields.get("multiple")),
        items,
    })
}

re!(CODE_KEY, r"(?i)^(lang|language|title|diff|copy)\s*:");
re!(TREE_KEY, r"(?i)^(title|subtitle)\s*:");
re!(TRAILING_WS, r"\s+$");
re!(LEADING_NL, r"^\n+");

pub(crate) fn code(body: &str) -> Option<CodeProps> {
    let normalized = lf(body);
    let (header, source, separated) = match split_header(&normalized) {
        (h, Some(rest)) => (h.to_string(), rest.to_string(), true),
        (_, None) => {
            let (h, rest) = crate::kv::leading_preamble(&normalized, |s| {
                CODE_KEY.is_match(s) && !s.starts_with('|')
            });
            (h, rest, false)
        }
    };
    let fields = parse_kv_block(&header);
    let source = TRAILING_WS.replace(&source, "");
    let source = LEADING_NL.replace(&source, "").into_owned();
    if source.is_empty() {
        return None;
    }
    let chrome = ["lang", "language", "title", "diff", "copy"]
        .iter()
        .any(|k| fields.contains_key(*k));
    if !separated && !chrome {
        return None;
    }
    let lang = first(&fields, &["lang", "language"]).filter(|l| !l.is_empty());
    let diff = crate::kv::bool_loose(fields.get("diff")).or_else(|| {
        lang.as_deref()
            .is_some_and(|l| l.to_lowercase() == "diff")
            .then_some(true)
    });
    Some(CodeProps {
        source,
        title: fields.get("title").cloned(),
        diff,
        copy: crate::kv::bool_loose(fields.get("copy")),
        lang,
    })
}

re!(TREE_INDENT, r"^( *)(.*)$");
re!(TREE_CHROME, r"^[|\\+\-`]+[─\-]*\s*");

fn tree_lines(raw: &str) -> Vec<TreeNode> {
    struct Slot {
        node: TreeNode,
        children: Vec<usize>,
    }
    let mut arena: Vec<Slot> = Vec::new();
    let mut roots = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for line in lf(raw).split('\n') {
        if line.trim().is_empty() || line.trim().starts_with('#') {
            continue;
        }
        let expanded = line.replace('\t', "  ");
        let Some(caps) = TREE_INDENT.captures(expanded.trim_end()) else {
            continue;
        };
        let indent = caps[1].len();
        let name = LIST_MARKER_RE.replace(caps[2].trim(), "").into_owned();
        let mut name = TREE_CHROME.replace(&name, "").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let mut kind = TreeKind::File;
        if let Some(stripped) = name.strip_suffix('/') {
            kind = TreeKind::Folder;
            name = stripped.trim().to_string();
        }
        let idx = arena.len();
        arena.push(Slot {
            node: TreeNode {
                id: format!("tree-{idx}"),
                name,
                kind,
                children: (kind == TreeKind::Folder).then(Vec::new),
            },
            children: Vec::new(),
        });
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }
        match stack.last() {
            None => roots.push(idx),
            Some(&(_, parent)) => {
                let p = &mut arena[parent];
                p.node.kind = TreeKind::Folder;
                p.node.children.get_or_insert_with(Vec::new);
                p.children.push(idx);
            }
        }
        stack.push((indent, idx));
    }
    fn build(arena: &mut [Option<Slot>], idx: usize) -> TreeNode {
        let slot = arena[idx].take().expect("tree nodes are visited once");
        let mut node = slot.node;
        if let Some(children) = node.children.as_mut() {
            children.extend(slot.children.into_iter().map(|c| build(arena, c)));
        }
        node
    }
    let mut arena: Vec<Option<Slot>> = arena.into_iter().map(Some).collect();
    roots.into_iter().map(|r| build(&mut arena, r)).collect()
}

pub(crate) fn tree(body: &str) -> Option<TreeProps> {
    let normalized = lf(body);
    let (header, rest) = match split_header(&normalized) {
        (h, Some(rest)) => (h.to_string(), rest.to_string()),
        (_, None) => crate::kv::leading_preamble(&normalized, |s| TREE_KEY.is_match(s)),
    };
    let fields = parse_kv_block(&header);
    let nodes = tree_lines(&rest);
    if nodes.is_empty() {
        return None;
    }
    Some(TreeProps {
        title: fields.get("title").cloned(),
        subtitle: fields.get("subtitle").cloned(),
        nodes,
    })
}
