//! ` ```slides `: a deck split on top-level `---` lines.

use crate::kv::{lf, re, Fields};
use crate::props::{SlideItem, SlideLayer, SlidesProps};

re!(FENCE_TICKS, r"^(`{3,})[^\s`]*");
re!(FIELD, r"^([a-zA-Z][a-zA-Z0-9_-]*)\s*:\s*(.*)$");
re!(INDENTED, r"^\s+\S");
re!(HTTP, r"(?i)^https?://");
re!(IMAGE_EXT, r"(?i)\.(png|jpe?g|gif|webp|svg|avif)(\?.*)?$");
re!(SLUG, r"[^a-z0-9]+");

const WASHES: &[&str] = &["paper", "dusk", "ink", "mist", "ember"];

/// Splits on `---` lines outside fenced code.
pub fn split_section_breaks(source: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    let mut ticks = 0usize;
    let text = lf(source);
    for line in text.split('\n') {
        if ticks > 0 {
            lines.push(line);
            let run = line.bytes().take_while(|&b| b == b'`').count();
            if run >= ticks && line[run..].trim().is_empty() {
                ticks = 0;
            }
            continue;
        }
        if let Some(caps) = FENCE_TICKS.captures(line) {
            ticks = caps[1].len();
            lines.push(line);
            continue;
        }
        let t = line.trim();
        if t.starts_with("---") && t[3..].trim().is_empty() {
            parts.push(lines.join("\n"));
            lines.clear();
            continue;
        }
        lines.push(line);
    }
    parts.push(lines.join("\n"));
    parts
}

fn preamble(block: &str) -> (Fields, String) {
    let text = lf(block);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut fields = Fields::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() {
            if !fields.is_empty() {
                index += 1;
                break;
            }
            index += 1;
            continue;
        }
        let Some(caps) = FIELD.captures(line).filter(|_| !line.starts_with('|')) else {
            break;
        };
        if !caps[2].is_empty() {
            fields.insert(caps[1].to_lowercase(), caps[2].trim().to_string());
        }
        index += 1;
    }
    (
        fields,
        lines[index.min(lines.len())..]
            .join("\n")
            .trim()
            .to_string(),
    )
}

fn unit(value: Option<&String>, fallback: f64) -> f64 {
    let Some(v) = value else { return fallback };
    let t = v.trim();
    let n = if t.is_empty() {
        Some(0.0)
    } else {
        crate::chart::chart_number_strict(t)
    };
    n.map_or(fallback, |n| n.clamp(0.0, 1.0))
}

fn section(block: &str) -> (Fields, Vec<SlideLayer>, String) {
    let text = lf(block);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut fields = Fields::new();
    let mut layers = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() {
            index += 1;
            if !fields.is_empty() || !layers.is_empty() {
                break;
            }
            continue;
        }
        let Some(caps) = FIELD.captures(line) else {
            break;
        };
        if line.starts_with('|') || line.starts_with('#') || line.starts_with("```") {
            break;
        }
        let key = caps[1].to_lowercase();
        let value = caps[2].trim().to_string();
        index += 1;
        if key != "layer" {
            if !value.is_empty() {
                fields.insert(key, value);
            }
            continue;
        }
        let id = if value.is_empty() {
            format!("layer-{}", layers.len() + 1)
        } else {
            value
        };
        let mut props = Fields::new();
        while index < lines.len() && INDENTED.is_match(lines[index]) {
            let Some(nested) = FIELD.captures(lines[index].trim()) else {
                break;
            };
            props.insert(nested[1].to_lowercase(), nested[2].trim().to_string());
            index += 1;
        }
        if let Some(src) = props.get("src").filter(|s| !s.is_empty()) {
            layers.push(SlideLayer {
                id,
                src: src.clone(),
                x: unit(props.get("x"), 0.0),
                y: unit(props.get("y"), 0.0),
                w: unit(props.get("w"), 0.2),
                h: props
                    .get("h")
                    .filter(|h| !h.is_empty())
                    .map(|h| unit(Some(h), 0.2)),
            });
        }
    }
    (
        fields,
        layers,
        lines[index.min(lines.len())..]
            .join("\n")
            .trim()
            .to_string(),
    )
}

fn image_background(bg: &str) -> bool {
    !bg.is_empty()
        && !WASHES.contains(&bg.to_lowercase().as_str())
        && (bg.starts_with('.')
            || bg.starts_with('/')
            || HTTP.is_match(bg)
            || IMAGE_EXT.is_match(bg))
}

fn slug(label: &str, index: usize) -> String {
    let lower = label.to_lowercase();
    let s = SLUG.replace_all(&lower, "-");
    let s: String = s.trim_matches('-').chars().take(40).collect();
    if s.is_empty() {
        format!("slide-{}", index + 1)
    } else {
        s
    }
}

pub(crate) fn deck(body: &str) -> Option<SlidesProps> {
    let normalized = lf(body);
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return None;
    }
    let parts = split_section_breaks(normalized);
    let (fields, pre_body) = preamble(&parts[0]);
    let mut slides = Vec::new();
    if parts.len() == 1 {
        let fallback = if pre_body.is_empty() {
            normalized
        } else {
            &pre_body
        };
        if !fallback.trim().is_empty() {
            slides.push(SlideItem {
                id: "slide-1".into(),
                label: fields
                    .get("title")
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| "Slide".into()),
                layout: Some("stack".into()),
                body: fallback.trim().to_string(),
                ..Default::default()
            });
        }
    } else {
        for (index, part) in parts[1..].iter().enumerate() {
            let (f, layers, body) = section(part);
            let label = crate::kv::first(&f, &["label", "title"])
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| format!("Slide {}", index + 1));
            let layout = f
                .get("layout")
                .filter(|l| ["hero", "split", "stack"].contains(&l.as_str()))
                .cloned()
                .unwrap_or_else(|| "split".into());
            let bg = f
                .get("bg")
                .map(|b| b.trim().to_string())
                .filter(|b| !b.is_empty());
            let scrim =
                crate::parse::one_of(f.get("scrim"), &["dark", "light", "none"]).or_else(|| {
                    bg.as_deref()
                        .filter(|b| image_background(b))
                        .map(|_| "none".to_string())
                });
            slides.push(SlideItem {
                id: slug(&label, index),
                label,
                layout: Some(layout),
                body,
                bg,
                scrim,
                motion: crate::parse::one_of(f.get("motion"), &["fade", "fade-up"]),
                notes: f
                    .get("notes")
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty()),
                layers: (!layers.is_empty()).then_some(layers),
            });
        }
    }
    if slides.is_empty() {
        return None;
    }
    let theme = fields
        .get("theme")
        .map_or("paper".to_string(), |t| t.to_lowercase());
    let columns = fields.get("columns").filter(|c| *c == "1" || *c == "3");
    Some(SlidesProps {
        title: fields
            .get("title")
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty()),
        theme: Some(if WASHES.contains(&theme.as_str()) {
            theme
        } else {
            "paper".into()
        }),
        columns: Some(columns.cloned().unwrap_or_else(|| "2".into())),
        slides,
    })
}
