//! ` ```kanban `: `##` columns of Markdown task items.

use crate::kv::{lf, re};
use crate::props::{KanbanCard, KanbanColumn};

re!(HEADING, r"^##\s+(.+?)\s*$");
re!(TASK, r"^\s*-\s+\[([ xX])\]\s+(.*)$");

pub(crate) fn columns(body: &str) -> Vec<KanbanColumn> {
    let mut columns = Vec::new();
    let mut current: Option<KanbanColumn> = None;
    for line in lf(body).split('\n') {
        if let Some(caps) = HEADING.captures(line) {
            columns.extend(current.take());
            current = Some(KanbanColumn {
                title: caps[1].trim().to_string(),
                cards: Vec::new(),
            });
            continue;
        }
        if let (Some(caps), Some(column)) = (TASK.captures(line), current.as_mut()) {
            column.cards.push(KanbanCard {
                checked: caps[1].eq_ignore_ascii_case("x"),
                text: caps[2].trim_end().to_string(),
            });
        }
    }
    columns.extend(current);
    if columns.is_empty() {
        columns = ["Backlog", "Doing", "Done"]
            .map(|title| KanbanColumn {
                title: title.into(),
                cards: Vec::new(),
            })
            .into();
    }
    columns
}
