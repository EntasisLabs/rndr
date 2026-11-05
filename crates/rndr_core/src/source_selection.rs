//! Directed multi-selections in source byte coordinates.
use crate::editing::{EditError, SourceSelection};

/// Nonempty, sorted, disjoint directed selections with a stable primary range.
/// Overlaps and duplicate starts merge; adjacent ranges remain independent.
/// The primary range's direction wins when ranges merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceSelections {
    ranges: Vec<SourceSelection>,
    primary: usize,
}
impl SourceSelections {
    /// Construct a single selection.
    pub fn single(selection: SourceSelection) -> Self {
        Self {
            ranges: vec![selection],
            primary: 0,
        }
    }
    /// Validate UTF-8 endpoints, snap to graphemes, sort and merge duplicates,
    /// duplicate starts and overlaps. Empty input or an invalid primary refuses.
    pub fn new(
        text: &str,
        ranges: Vec<SourceSelection>,
        primary: usize,
    ) -> Result<Self, EditError> {
        if ranges.is_empty()
            || primary >= ranges.len()
            || ranges
                .iter()
                .any(|s| !text.is_char_boundary(s.anchor) || !text.is_char_boundary(s.head))
        {
            return Err(EditError::InvalidRange);
        }
        let mut ranges: Vec<_> = ranges
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                let s = if s.anchor == s.head {
                    let at = crate::edit::floor_grapheme(text, s.head);
                    SourceSelection {
                        anchor: at,
                        head: at,
                    }
                } else {
                    let (a, b) = crate::edit::grapheme_selection(text, s.anchor, s.head);
                    if s.anchor <= s.head {
                        SourceSelection { anchor: a, head: b }
                    } else {
                        SourceSelection { anchor: b, head: a }
                    }
                };
                (s, i == primary)
            })
            .collect();
        ranges.sort_by_key(|(s, _)| (s.anchor.min(s.head), s.anchor.max(s.head)));
        let mut merged: Vec<(SourceSelection, bool)> = Vec::new();
        for (s, primary) in ranges {
            if let Some((last, has_primary)) = merged.last_mut() {
                if s.anchor.min(s.head) < last.anchor.max(last.head)
                    || s.anchor.min(s.head) == last.anchor.min(last.head)
                {
                    let start = last.anchor.min(last.head).min(s.anchor.min(s.head));
                    let end = last.anchor.max(last.head).max(s.anchor.max(s.head));
                    let reverse = if primary {
                        s.anchor > s.head
                    } else {
                        last.anchor > last.head
                    };
                    *last = if reverse {
                        SourceSelection {
                            anchor: end,
                            head: start,
                        }
                    } else {
                        SourceSelection {
                            anchor: start,
                            head: end,
                        }
                    };
                    *has_primary |= primary;
                    continue;
                }
            }
            merged.push((s, primary));
        }
        let primary = merged.iter().position(|(_, p)| *p).unwrap();
        Ok(Self {
            ranges: merged.into_iter().map(|(s, _)| s).collect(),
            primary,
        })
    }
    /// Source-ordered directed ranges.
    pub fn ranges(&self) -> &[SourceSelection] {
        &self.ranges
    }
    /// Primary range index in source order.
    pub fn primary_index(&self) -> usize {
        self.primary
    }
    /// Selection projected to the legacy caret and native IME.
    pub fn primary(&self) -> SourceSelection {
        self.ranges[self.primary]
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.ranges.capacity() * std::mem::size_of::<SourceSelection>()
    }
    pub(crate) fn mapped(&self, text: &str, map: &crate::editing::SourceMap) -> Self {
        use crate::editing::Affinity;
        let ranges = self
            .ranges
            .iter()
            .map(|s| SourceSelection {
                anchor: map.map(s.anchor, Affinity::After),
                head: map.map(s.head, Affinity::After),
            })
            .collect();
        Self::new(text, ranges, self.primary).expect("source maps retain valid scalar endpoints")
    }
}

/// A visual row supplied by shaped editor geometry. Stops are `(x, byte)` in
/// left-to-right visual order, not logical byte order. Tabs use their actual
/// shaped width; bidi runs may reverse byte order. Wrapped rows remain distinct.
/// Include both row endpoints. Folded/hidden rows must not be supplied.
#[derive(Clone, Debug)]
pub struct RectangleRow {
    /// Visual caret stops; grapheme boundary bytes in the current document.
    pub stops: Vec<(f32, usize)>,
}
/// Make one directed range per visual row. X coordinates snap to the nearest
/// real caret stop (ties choose the left stop); short rows clamp, without virtual
/// space insertion. Bidi rows whose selected visual span is logically disjoint
/// refuse, rather than silently editing intervening unselected text. Supply
/// separate wrapped rows explicitly, and only rows intersecting the rectangle.
pub fn rectangular_selections(
    text: &str,
    rows: &[RectangleRow],
    anchor_x: f32,
    head_x: f32,
    primary_row: usize,
) -> Result<SourceSelections, EditError> {
    if rows.is_empty() || primary_row >= rows.len() || !anchor_x.is_finite() || !head_x.is_finite()
    {
        return Err(EditError::InvalidRange);
    }
    let mut ranges = Vec::new();
    for row in rows {
        if row.stops.is_empty()
            || row.stops.iter().any(|(x, b)| {
                !x.is_finite()
                    || !text.is_char_boundary(*b)
                    || crate::edit::floor_grapheme(text, *b) != *b
            })
            || row.stops.windows(2).any(|s| s[0].0 > s[1].0)
        {
            return Err(EditError::InvalidRange);
        }
        let nearest = |x: f32| {
            row.stops
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| (a.0 - x).abs().total_cmp(&(b.0 - x).abs()))
                .unwrap()
                .0
        };
        let a = nearest(anchor_x);
        let h = nearest(head_x);
        let span = &row.stops[a.min(h)..=a.max(h)];
        let forward = span.windows(2).all(|s| s[0].1 <= s[1].1);
        let reverse = span.windows(2).all(|s| s[0].1 >= s[1].1);
        if !forward && !reverse {
            return Err(EditError::InvalidRange);
        }
        // Require consecutive logical graphemes, not merely monotone endpoints.
        if span
            .windows(2)
            .any(|s| crate::edit::next_grapheme(text, s[0].1.min(s[1].1)) != s[0].1.max(s[1].1))
        {
            return Err(EditError::InvalidRange);
        }
        ranges.push(SourceSelection {
            anchor: row.stops[a].1,
            head: row.stops[h].1,
        });
    }
    SourceSelections::new(text, ranges, primary_row)
}

/// Select a window-logical rectangle in the current shaped source editor.
/// This explicit host command uses actual tab widths, bidi caret edges and soft-wrap
/// rows, excludes folded rows, clamps short rows, and inserts no virtual spaces.
/// It requires the current focused noncomposing attachment. Readonly selection is allowed.
/// A discontiguous mixed-bidi visual slice refuses atomically rather than selecting extra bytes.
pub fn select_source_rectangle(
    world: &mut crate::World,
    view: crate::Entity,
    anchor: [f32; 2],
    head: [f32; 2],
) -> Result<(), EditError> {
    if anchor.iter().chain(head.iter()).any(|x| !x.is_finite()) {
        return Err(EditError::InvalidRange);
    }
    let document =
        crate::editing::attached_source(world, view).ok_or(EditError::MissingDocument)?;
    let snapshot = crate::access_text(world, view).ok_or(EditError::StaleRevision)?;
    if !snapshot.selectable {
        return Err(EditError::Composing);
    }
    let text = snapshot.state.text();
    // Accessibility and native hit testing share the already fold-projected geometry.
    let layout = snapshot.layout;
    let local_y = |y: f32| y - snapshot.origin.1;
    let nearest_row = |y: f32| -> Option<usize> {
        layout
            .lines
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let distance = |line: &crate::TextLine| {
                    if y < line.y {
                        line.y - y
                    } else {
                        (y - line.y - line.h).max(0.0)
                    }
                };
                distance(a).total_cmp(&distance(b))
            })
            .map(|(i, _)| i)
    };
    let first = nearest_row(local_y(anchor[1])).ok_or(EditError::InvalidRange)?;
    let last = nearest_row(local_y(head[1])).ok_or(EditError::InvalidRange)?;
    let start = first.min(last);
    let rows: Vec<RectangleRow> = layout.lines[start..=first.max(last)]
        .iter()
        .map(|line| {
            let mut stops: Vec<_> = line
                .edges
                .iter()
                .filter(|(b, _)| crate::edit::floor_grapheme(text, *b) == *b)
                .map(|(b, x)| (*x, *b))
                .collect();
            stops.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            stops.dedup();
            if stops.is_empty() {
                stops.push((0.0, line.start));
            }
            RectangleRow { stops }
        })
        .collect();
    let selections = rectangular_selections(
        text,
        &rows,
        anchor[0] - snapshot.origin.0,
        head[0] - snapshot.origin.0,
        last - start,
    )?;
    let doc = world
        .resource_mut::<crate::editing::SourceDocuments>()
        .and_then(|d| d.get_mut(document))
        .ok_or(EditError::MissingDocument)?;
    doc.set_view_selections(view, doc.revision(), selections)?;
    crate::editing::project_source(world, view);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalize_overlaps_direction_primary_and_unicode() {
        let s = SourceSelections::new(
            "ae\u{301}z",
            vec![
                SourceSelection { anchor: 4, head: 1 },
                SourceSelection { anchor: 0, head: 2 },
                SourceSelection { anchor: 5, head: 5 },
            ],
            0,
        )
        .unwrap();
        assert_eq!(
            s.ranges(),
            &[
                SourceSelection { anchor: 4, head: 0 },
                SourceSelection { anchor: 5, head: 5 }
            ]
        );
        assert_eq!(s.primary_index(), 0);
        assert!(
            SourceSelections::new("é", vec![SourceSelection { anchor: 1, head: 1 }], 0).is_err()
        );
    }
    #[test]
    fn adjacent_wrapped_rows_and_rtl_direction_are_preserved() {
        let wrapped = vec![
            RectangleRow {
                stops: vec![(0., 0), (1., 1), (2., 2)],
            },
            RectangleRow {
                stops: vec![(0., 2), (1., 3), (2., 4)],
            },
        ];
        let set = rectangular_selections("abcd", &wrapped, 0., 2., 1).unwrap();
        assert_eq!(
            set.ranges(),
            &[
                SourceSelection { anchor: 0, head: 2 },
                SourceSelection { anchor: 2, head: 4 }
            ]
        );
        assert_eq!(set.primary_index(), 1);
        let rtl = vec![RectangleRow {
            stops: vec![(0., 4), (1., 2), (2., 0)],
        }];
        let set = rectangular_selections("אב", &rtl, 0., 2., 0).unwrap();
        assert_eq!(set.primary(), SourceSelection { anchor: 4, head: 0 });
    }
    #[test]
    fn rectangles_use_visual_tabs_wrap_and_refuse_mixed_bidi() {
        let rows = vec![
            RectangleRow {
                stops: vec![(0., 0), (4., 1), (5., 2)],
            },
            RectangleRow {
                stops: vec![(0., 2), (1., 3), (2., 4)],
            },
        ];
        let s = rectangular_selections("\tabc", &rows, 0., 4., 0).unwrap();
        assert_eq!(
            s.ranges(),
            &[
                SourceSelection { anchor: 0, head: 1 },
                SourceSelection { anchor: 2, head: 4 }
            ]
        );
        let bidi = vec![RectangleRow {
            stops: vec![(0., 0), (1., 2), (2., 1), (3., 3)],
        }];
        assert!(rectangular_selections("abc", &bidi, 0., 3., 0).is_err());
    }
}
