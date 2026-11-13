//! Renderer-side occurrence correspondence; parser indices are never identities.
use crate::lifecycle::{self, Key};
use crate::{LiquidComponentRecord, LiquidComponentState, LiquidSurfaceHandle, LiquidUpdateError};
use rndr_core::World;
use rndr_liquid::{LiquidBlock, LiquidStream, Segment};

/// Detailed refusal for the additive F2 APIs. Legacy `LiquidUpdateError` is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidF2Error {
    /// Foreign or retired surface.
    StaleOwner,
    /// Retired or missing block incarnation.
    StaleBlock,
    /// Superseded resource lane.
    StaleRequest,
    /// Revision mismatch, with payload-free counters.
    Revision {
        /// Revision supplied by the caller.
        expected: u64,
        /// Current stored revision.
        actual: u64,
    },
    /// State variant or child index is incompatible.
    Kind,
    /// Invalid key vector or unresolved parser reference.
    InvalidKeys,
    /// Admission bound reached.
    Capacity,
    /// Nonwrapping counter exhausted.
    Exhausted,
}
impl From<LiquidUpdateError> for LiquidF2Error {
    fn from(e: LiquidUpdateError) -> Self {
        match e {
            LiquidUpdateError::StaleOwner => Self::StaleOwner,
            LiquidUpdateError::Capacity => Self::Capacity,
            LiquidUpdateError::Exhausted => Self::Exhausted,
        }
    }
}
/// Process-local block incarnation. No public constructor or persistence contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LiquidBlockHandle(pub(crate) Key);
/// Checked source/identity revision, independent of component state revisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiquidSourceRevision(pub(crate) u64);
impl LiquidSourceRevision {
    /// Initial revision of a newly created or reset surface.
    pub const ZERO: Self = Self(0);
}
/// Complete top-level settled reading-order sidecar. `None` selects fallback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiquidKeyMap {
    /// Unique 1–256-byte, control-free UTF-8 labels, scoped to this surface.
    pub keys: Vec<Option<String>>,
}
/// A settled top-level occurrence, not an original-source byte or stable ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidOccurrenceDescription {
    /// Reading-order ordinal.
    pub ordinal: usize,
    /// Canonical parser kind.
    pub kind: &'static str,
}
pub(crate) fn indices(markdown: &str) -> Vec<usize> {
    rndr_liquid::segments(markdown)
        .into_iter()
        .filter_map(|s| match s {
            Segment::Block(i) => Some(i),
            _ => None,
        })
        .collect()
}
/// Inspect exactly the settled occurrences accepted by the keyed setter.
pub fn inspect_liquid_occurrences(
    source: &str,
) -> Result<Vec<LiquidOccurrenceDescription>, LiquidF2Error> {
    let stream = LiquidStream::new(&source.replace("\r\n", "\n"));
    validate_projection(stream.markdown(), stream.blocks())?;
    indices(stream.markdown())
        .into_iter()
        .enumerate()
        .map(|(ordinal, i)| {
            semantic(stream.blocks(), i)?;
            Ok(LiquidOccurrenceDescription {
                ordinal,
                kind: stream.blocks()[i].kind(),
            })
        })
        .collect()
}
pub(crate) fn validate_projection(
    markdown: &str,
    blocks: &[LiquidBlock],
) -> Result<(), LiquidF2Error> {
    for segment in rndr_liquid::segments(markdown) {
        match segment {
            Segment::Block(i) => {
                semantic(blocks, i)?;
            }
            Segment::Markdown(t) if t.contains(rndr_liquid::BLOCK_TOKEN_DELIMITER) => {
                return Err(LiquidF2Error::InvalidKeys)
            }
            _ => {}
        }
    }
    Ok(())
}
pub(crate) fn validate(keys: &LiquidKeyMap, count: usize) -> Result<(), LiquidF2Error> {
    let mut unique = std::collections::HashSet::new();
    if keys.keys.len() != count {
        return Err(LiquidF2Error::InvalidKeys);
    }
    for key in keys.keys.iter().flatten() {
        if key.is_empty()
            || key.len() > 256
            || key.chars().any(char::is_control)
            || !unique.insert(key)
        {
            return Err(LiquidF2Error::InvalidKeys);
        }
    }
    Ok(())
}

// Keep typed payloads, not hashes or Debug encodings. Replace nested references
// with canonical child indices in a shared intern table, then use PartialEq on
// the entire cloned payload. Each input is walked child-first and cycles and
// unresolved references are rejected. Every string and typed container field
// is visited; numeric and enumerated values retain ordinary typed equality.
fn text(
    text: &mut String,
    blocks: &[LiquidBlock],
    pool: &mut Vec<LiquidBlock>,
    stack: &mut Vec<usize>,
) -> Result<(), LiquidF2Error> {
    let mut result = String::new();
    for s in rndr_liquid::segments(text) {
        match s {
            Segment::Markdown(t) => {
                if t.contains(rndr_liquid::BLOCK_TOKEN_DELIMITER) {
                    return Err(LiquidF2Error::InvalidKeys);
                }
                result.push_str(t);
            }
            Segment::Block(i) => {
                let child = canonical(blocks, i, pool, stack)?;
                let slot = pool.iter().position(|b| *b == child).unwrap_or_else(|| {
                    pool.push(child);
                    pool.len() - 1
                });
                result.push_str(&rndr_liquid::block_token(slot));
            }
        }
    }
    *text = result;
    Ok(())
}
trait Walk {
    fn walk(
        &mut self,
        b: &[LiquidBlock],
        pool: &mut Vec<LiquidBlock>,
        stack: &mut Vec<usize>,
    ) -> Result<(), LiquidF2Error>;
}
impl Walk for String {
    fn walk(
        &mut self,
        b: &[LiquidBlock],
        p: &mut Vec<LiquidBlock>,
        s: &mut Vec<usize>,
    ) -> Result<(), LiquidF2Error> {
        text(self, b, p, s)
    }
}
impl<T: Walk> Walk for Option<T> {
    fn walk(
        &mut self,
        b: &[LiquidBlock],
        p: &mut Vec<LiquidBlock>,
        s: &mut Vec<usize>,
    ) -> Result<(), LiquidF2Error> {
        if let Some(v) = self {
            v.walk(b, p, s)?;
        }
        Ok(())
    }
}
impl<T: Walk> Walk for Vec<T> {
    fn walk(
        &mut self,
        b: &[LiquidBlock],
        p: &mut Vec<LiquidBlock>,
        s: &mut Vec<usize>,
    ) -> Result<(), LiquidF2Error> {
        for v in self {
            v.walk(b, p, s)?;
        }
        Ok(())
    }
}
impl Walk for std::collections::BTreeMap<String, String> {
    fn walk(
        &mut self,
        b: &[LiquidBlock],
        p: &mut Vec<LiquidBlock>,
        s: &mut Vec<usize>,
    ) -> Result<(), LiquidF2Error> {
        let mut result = Self::new();
        for (mut k, mut v) in std::mem::take(self) {
            k.walk(b, p, s)?;
            v.walk(b, p, s)?;
            if result.insert(k, v).is_some() {
                return Err(LiquidF2Error::InvalidKeys);
            }
        }
        *self = result;
        Ok(())
    }
}
// List every string/container field explicitly. Numeric/enumerated fields stay
// untouched and participate in the final typed PartialEq comparison.
macro_rules! walk {
    ($($name:ident { $($field:ident),* $(,)? })*) => {$ (
        impl Walk for rndr_liquid::$name {
            fn walk(&mut self, b: &[LiquidBlock], p: &mut Vec<LiquidBlock>, s: &mut Vec<usize>) -> Result<(), LiquidF2Error> { $(self.$field.walk(b,p,s)?;)* Ok(()) }
        }
    )*};
}
walk! {
    CardPoint { label, body, emoji, icon }
    CardProps { title, subtitle, body, emoji, icon, image, meta, summary, chips, points, badges }
    CarouselProps { items }
    ActionProps { label, intent, emoji, icon }
    ActionsProps { actions }
    CalloutProps { body, tone, title }
    CiteProps { quote, title, url, source }
    CompareAxis { id, label }
    CompareEntity { id, label, values }
    CompareProps { title, subtitle, recommendation, mode, axes, entities }
    PlanSegment { id, label, time, emoji, icon, image, subtitle, body, badge }
    PlanProps { title, subtitle, grouping, segments }
    TimelineEvent { id, label, ts, detail, lane, emoji, icon, meta, body, image, media }
    TimelineProps { title, subtitle, granularity, layout, events }
    ShortlistItem { id, label, summary, score, meta, emoji, icon, image }
    ShortlistProps { title, subtitle, criteria, density, items }
    DecisionOption { id, label, pros, cons, score, summary }
    DecisionProps { title, subtitle, factors, recommendation, options }
    BriefSection { id, heading, body }
    BriefSource { id, title, url, quote }
    BriefProps { title, subtitle, tone, sections, sources }
    DashboardTile { id, label, value, delta, tone, emoji, icon, hint, unit, feed, field, refresh }
    DashboardProps { title, subtitle, columns, tiles }
    ReportProps { title, subtitle, columns, body }
    SlideLayer { id, src }
    SlideItem { id, label, layout, body, bg, scrim, layers, motion, notes }
    SlidesProps { title, theme, columns, slides }
    SectionProps { title, subtitle, body }
    TabsPanel { id, label, body, emoji, icon }
    TabsProps { title, subtitle, default, panels }
    StepItem { id, label, body, status, emoji, icon }
    StepsProps { title, subtitle, steps }
    RecipeStep { id, label, body, duration_label }
    RecipeProps { title, subtitle, r#yield, ingredients, resources, steps, notes, actions }
    AccordionItem { id, label, body, emoji, icon }
    AccordionProps { title, subtitle, items }
    CodeProps { source, lang, title }
    TreeNode { id, name, children }
    TreeProps { title, subtitle, nodes }
    ChipProps { label, tone, value }
    ChipsProps { chips }
    MediaProps { src, alt, caption, ratio }
    FeedProps { feed_id, datatype, title, empty, refresh }
    StyledBlockProps { body, id, font, size, align, spacing }
    KanbanCard { text }
    KanbanColumn { title, cards }
    KanbanProps { columns }
}
impl Walk for rndr_liquid::chart::ChartProps {
    fn walk(
        &mut self,
        b: &[LiquidBlock],
        p: &mut Vec<LiquidBlock>,
        s: &mut Vec<usize>,
    ) -> Result<(), LiquidF2Error> {
        macro_rules! fields { ($($f:ident),*) => { $(self.$f.walk(b,p,s)?;)* }; }
        fields!(
            categories,
            title,
            description,
            series_marks,
            layout,
            curve,
            center_label,
            center_value,
            trend,
            trend_direction,
            caption,
            labels,
            label_position,
            active_key,
            colors,
            width,
            height,
            surface
        );
        for series in &mut self.series {
            series.key.walk(b, p, s)?;
            series.label.walk(b, p, s)?;
        }
        if let Some(points) = &mut self.points {
            for point in points {
                point.group.walk(b, p, s)?;
            }
        }
        if let Some(matrix) = &mut self.matrix {
            matrix.rows.walk(b, p, s)?;
            matrix.cols.walk(b, p, s)?;
        }
        if let Some(rndr_liquid::chart::ChartLegend::Placement(v)) = &mut self.legend {
            v.walk(b, p, s)?;
        }
        Ok(())
    }
}
fn canonical(
    blocks: &[LiquidBlock],
    index: usize,
    pool: &mut Vec<LiquidBlock>,
    stack: &mut Vec<usize>,
) -> Result<LiquidBlock, LiquidF2Error> {
    if stack.contains(&index) || stack.len() >= blocks.len() {
        return Err(LiquidF2Error::InvalidKeys);
    }
    let mut block = blocks
        .get(index)
        .cloned()
        .ok_or(LiquidF2Error::InvalidKeys)?;
    stack.push(index);
    match &mut block {
        LiquidBlock::Card(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Carousel(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Actions(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Callout(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Section(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Block(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Chips(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Media(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Cite(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Compare(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Plan(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Timeline(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Shortlist(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Decision(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Brief(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Dashboard(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Chart(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Report(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Slides(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Tabs(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Steps(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Recipe(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Accordion(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Code(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Tree(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Kanban(p) => p.walk(blocks, pool, stack)?,
        LiquidBlock::Feed(p) => p.walk(blocks, pool, stack)?,
    }
    stack.pop();
    Ok(block)
}
pub(crate) fn semantic(
    blocks: &[LiquidBlock],
    index: usize,
) -> Result<(LiquidBlock, Vec<LiquidBlock>), LiquidF2Error> {
    let mut pool = Vec::new();
    let block = canonical(blocks, index, &mut pool, &mut Vec::new())?;
    Ok((block, pool))
}
pub(crate) fn markdown_path(markdown: &str, blocks: &[LiquidBlock]) -> String {
    let mut value = markdown.to_owned();
    let mut pool = Vec::new();
    if text(&mut value, blocks, &mut pool, &mut Vec::new()).is_err() {
        return markdown.to_owned();
    }
    // Private path discriminator only, never a public key, serial or hash.
    // The complete typed child pool distinguishes equal ordinal token shapes.
    format!("{value:?}:{pool:?}")
}
pub(crate) fn equal(a: &[LiquidBlock], i: usize, b: &[LiquidBlock], j: usize) -> bool {
    let mut pool = Vec::new();
    let left = canonical(a, i, &mut pool, &mut Vec::new());
    let right = canonical(b, j, &mut pool, &mut Vec::new());
    matches!((left, right), (Ok(a), Ok(b)) if a == b)
}
// Child-index compatibility deliberately compares full ordered child values,
// including IDs. Parent titles/styles are not child schemas.
pub(crate) fn compatible(old: &[LiquidBlock], i: usize, new: &[LiquidBlock], j: usize) -> bool {
    let mut pool = Vec::new();
    let (Ok(a), Ok(b)) = (
        canonical(old, i, &mut pool, &mut Vec::new()),
        canonical(new, j, &mut pool, &mut Vec::new()),
    ) else {
        return false;
    };
    match (a, b) {
        (LiquidBlock::Accordion(a), LiquidBlock::Accordion(b)) => {
            a.items == b.items && a.multiple.unwrap_or(false) == b.multiple.unwrap_or(false)
        }
        (LiquidBlock::Tabs(a), LiquidBlock::Tabs(b)) => a.panels == b.panels,
        (LiquidBlock::Steps(a), LiquidBlock::Steps(b)) => a.steps == b.steps,
        (LiquidBlock::Recipe(a), LiquidBlock::Recipe(b)) => a.steps == b.steps,
        (LiquidBlock::Carousel(a), LiquidBlock::Carousel(b)) => a.items == b.items,
        (LiquidBlock::Slides(a), LiquidBlock::Slides(b)) => a.slides == b.slides,
        (LiquidBlock::Kanban(a), LiquidBlock::Kanban(b)) => a.columns == b.columns,
        (a @ LiquidBlock::Feed(_), b @ LiquidBlock::Feed(_))
        | (a @ LiquidBlock::Dashboard(_), b @ LiquidBlock::Dashboard(_)) => {
            crate::feed::bindings(&a) == crate::feed::bindings(&b)
        }
        (LiquidBlock::Report(a), LiquidBlock::Report(b)) => a.body == b.body,
        (LiquidBlock::Callout(a), LiquidBlock::Callout(b)) => a.body == b.body,
        (LiquidBlock::Section(a), LiquidBlock::Section(b)) => a.body == b.body,
        (LiquidBlock::Block(a), LiquidBlock::Block(b)) => a.body == b.body,
        (LiquidBlock::Card(a), LiquidBlock::Card(b)) => {
            a.body == b.body && a.summary == b.summary && a.points == b.points
        }
        (LiquidBlock::Plan(a), LiquidBlock::Plan(b)) => a.segments == b.segments,
        (LiquidBlock::Timeline(a), LiquidBlock::Timeline(b)) => a.events == b.events,
        (LiquidBlock::Brief(a), LiquidBlock::Brief(b)) => {
            a.sections == b.sections && a.sources == b.sources
        }
        (a, b) => a.kind() == b.kind(),
    }
}
/// One correspondence is consumed by logical resources AND presentation carry.
pub(crate) fn correspondence(
    old: &[LiquidBlock],
    oi: &[usize],
    ok: &[Option<String>],
    new: &[LiquidBlock],
    ni: &[usize],
    nk: &[Option<String>],
    append: bool,
) -> Vec<Option<usize>> {
    ni.iter()
        .enumerate()
        .map(|(n, j)| {
            if let Some(k) = &nk[n] {
                return oi
                    .iter()
                    .enumerate()
                    .find(|(o, i)| ok[*o].as_ref() == Some(k) && old[**i].kind() == new[*j].kind())
                    .map(|(_, i)| *i);
            }
            if append && n < oi.len() && ok[n].is_none() && equal(old, oi[n], new, *j) {
                return Some(oi[n]);
            }
            let candidates: Vec<_> = oi
                .iter()
                .enumerate()
                .filter(|(o, i)| ok[*o].is_none() && equal(old, **i, new, *j))
                .map(|(_, i)| *i)
                .collect();
            if candidates.len() == 1
                && ni
                    .iter()
                    .enumerate()
                    .filter(|(p, i)| nk[*p].is_none() && equal(new, **i, new, *j))
                    .count()
                    == 1
            {
                Some(candidates[0])
            } else {
                None
            }
        })
        .collect()
}
/// Context-bearing observation of an unchanged legacy envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiquidInteractionEnvelopeFor {
    /// Original legacy payload, including historical positional metadata.
    pub envelope: crate::LiquidInteractionEnvelope,
    /// Surface incarnation captured at commit time.
    pub surface: LiquidSurfaceHandle,
    /// Block incarnation captured at commit time.
    pub block: LiquidBlockHandle,
}
/// Receives context-bearing interaction observations. Store handles, not source bytes.
pub trait LiquidInteractionHandlerFor: Send + Sync {
    /// A committed interaction carrying its original logical incarnation.
    fn on_interaction(&mut self, world: &mut World, envelope: &LiquidInteractionEnvelopeFor);
}
/// Refused additive subscription with its detailed reason and untouched handler.
pub struct LiquidSubscribeErrorFor<H> {
    /// Payload-free F2 refusal classification.
    pub reason: LiquidF2Error,
    /// The exact handler still owned by the caller after refusal.
    pub handler: H,
}
impl<H> std::fmt::Debug for LiquidSubscribeErrorFor<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiquidSubscribeErrorFor")
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}
struct ContextHandler<H>(H);
impl<H: LiquidInteractionHandlerFor> crate::LiquidInteractionHandler for ContextHandler<H> {
    fn on_interaction(&mut self, _: &mut World, _: &crate::LiquidInteractionEnvelope) {}
    fn on_interaction_for(&mut self, world: &mut World, envelope: &LiquidInteractionEnvelopeFor) {
        self.0.on_interaction(world, envelope);
    }
}
/// Subscribe without changing legacy envelope literals or handlers. Refusal returns
/// the exact untouched handler, just like the legacy owned subscription API.
pub fn subscribe_liquid_interactions_owned_for<H: LiquidInteractionHandlerFor + 'static>(
    world: &mut World,
    owner: crate::LiquidListenerOwner,
    handler: H,
) -> Result<crate::LiquidInteractionSubscription, LiquidSubscribeErrorFor<H>> {
    crate::subscribe_liquid_interactions_owned(world, owner, ContextHandler(handler)).map_err(|e| {
        LiquidSubscribeErrorFor {
            reason: e.reason.into(),
            handler: e.handler.0,
        }
    })
}
/// Read the current source/identity revision without acquiring mutation authority.
pub fn liquid_source_revision(
    world: &World,
    surface: LiquidSurfaceHandle,
) -> Result<LiquidSourceRevision, LiquidF2Error> {
    lifecycle::source_revision(world, surface)
}
/// Resolve a current top-level occurrence to its non-reusable incarnation.
pub fn liquid_component_handle(
    world: &World,
    surface: LiquidSurfaceHandle,
    occurrence: usize,
) -> Result<LiquidBlockHandle, LiquidF2Error> {
    lifecycle::component_handle(world, surface, occurrence)
}
/// Query by incarnation; stale/foreign handles never fall back to a source byte.
pub fn liquid_component_state_for(
    world: &World,
    block: LiquidBlockHandle,
) -> Result<Option<LiquidComponentRecord>, LiquidF2Error> {
    check(world, block)?;
    Ok(crate::state::read_key(world, block.0))
}
pub(crate) fn check(world: &World, block: LiquidBlockHandle) -> Result<(), LiquidF2Error> {
    if !lifecycle::live(world, block.0 .0) {
        return Err(LiquidF2Error::StaleOwner);
    }
    if !lifecycle::block_live(world, block.0) {
        return Err(LiquidF2Error::StaleBlock);
    }
    Ok(())
}
/// Revisioned, kind-checked synchronous write to exactly this block incarnation.
pub fn put_liquid_component_state_for(
    world: &mut World,
    block: LiquidBlockHandle,
    expected: u64,
    state: LiquidComponentState,
) -> Result<LiquidComponentRecord, LiquidF2Error> {
    check(world, block)?;
    crate::state::put_key(world, block.0, expected, state)
}
pub(crate) fn valid_state(block: &LiquidBlock, state: &LiquidComponentState) -> bool {
    fn distinct(values: &[usize]) -> bool {
        values
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == values.len()
    }
    match (block, state) {
        (LiquidBlock::Accordion(p), LiquidComponentState::Accordion { open }) => {
            distinct(open)
                && open.iter().all(|i| *i < p.items.len())
                && (p.multiple.unwrap_or(false) || open.len() <= 1)
        }
        (LiquidBlock::Tabs(p), LiquidComponentState::Tabs { selected }) => {
            *selected < p.panels.len()
        }
        (LiquidBlock::Steps(p), LiquidComponentState::Checklist { checked }) => {
            distinct(checked) && checked.iter().all(|i| *i < p.steps.len())
        }
        (LiquidBlock::Recipe(p), LiquidComponentState::Checklist { checked }) => {
            distinct(checked) && checked.iter().all(|i| *i < p.steps.len())
        }
        (LiquidBlock::Kanban(p), LiquidComponentState::Checklist { checked }) => {
            distinct(checked)
                && checked
                    .iter()
                    .all(|i| *i < p.columns.iter().map(|c| c.cards.len()).sum())
        }
        (LiquidBlock::Carousel(p), LiquidComponentState::Selection { selected }) => {
            *selected < p.items.len()
        }
        (LiquidBlock::Slides(p), LiquidComponentState::Selection { selected }) => {
            *selected < p.slides.len()
        }
        (
            b @ (LiquidBlock::Feed(_) | LiquidBlock::Dashboard(_)),
            LiquidComponentState::Feeds { slots },
        ) => {
            let bindings = crate::feed::bindings(b);
            let mut seen = std::collections::HashSet::new();
            slots.iter().all(|s| {
                seen.insert(&s.instance)
                    && bindings.get(&s.instance).is_some_and(|(id, _, _)| {
                        *id == s.feed_id && crate::FeedDatatype::parse(&s.datatype).is_some()
                    })
            })
        }
        _ => false,
    }
}
/// Atomic source/key replacement. Rebinding between keyed and unkeyed resets identity.
pub fn set_liquid_source_keyed(
    world: &mut World,
    surface: LiquidSurfaceHandle,
    expected: LiquidSourceRevision,
    source: &str,
    keys: &LiquidKeyMap,
) -> Result<LiquidSourceRevision, LiquidF2Error> {
    crate::view::set_keyed(world, surface, expected, source, keys)
}
/// Reset just one block and its subtree; advance source revision, not its bytes.
pub fn reset_liquid_block(
    world: &mut World,
    block: LiquidBlockHandle,
    expected: u64,
) -> Result<LiquidBlockHandle, LiquidF2Error> {
    lifecycle::reset_block(world, block, expected)
}
/// Reset all incarnations while preserving bytes and key labels. New revision is zero.
pub fn reset_liquid_surface(
    world: &mut World,
    surface: LiquidSurfaceHandle,
    expected: LiquidSourceRevision,
) -> Result<LiquidSurfaceHandle, LiquidF2Error> {
    lifecycle::reset_surface(world, surface, expected)
}
