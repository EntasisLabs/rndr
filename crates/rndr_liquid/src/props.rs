//! Typed payloads for every Liquid block. Field names serialize in camelCase,
//! matching the portable JSON contract of the original TypeScript grammar.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

macro_rules! props {
    ($(#[$meta:meta])* $name:ident { $($(#[$fmeta:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $name {
            $(
                $(#[$fmeta])*
                pub $field: $ty,
            )*
        }
    };
}

props!(
    /// A labeled point on a card face.
    CardPoint {
        /// Short label.
        label: String,
        /// Supporting text.
        body: String,
        /// Optional emoji glyph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
        /// Optional icon id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
    }
);

props!(
    /// ` ```card ` and carousel items.
    CardProps {
        /// Card title (required).
        title: String,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Face teaser.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        /// Emoji glyph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
        /// Icon id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        /// Image path or URL.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image: Option<String>,
        /// Metadata line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<String>,
        /// Long-form detail (markdown).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        /// Chips.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chips: Option<Vec<String>>,
        /// Labeled points.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        points: Option<Vec<CardPoint>>,
        /// Badges.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        badges: Option<Vec<String>>,
    }
);

impl CardProps {
    /// True when the card carries detail beyond its face.
    pub fn has_detail(&self) -> bool {
        let filled = |s: &Option<String>| s.as_deref().is_some_and(|s| !s.trim().is_empty());
        filled(&self.meta)
            || filled(&self.summary)
            || self.chips.as_ref().is_some_and(|c| !c.is_empty())
            || self.points.as_ref().is_some_and(|p| !p.is_empty())
    }
}

props!(
    /// ` ```carousel ` payload.
    CarouselProps {
        /// Cards in order.
        items: Vec<CardProps>,
    }
);

props!(
    /// A button the host may act on.
    ActionProps {
        /// Visible label.
        label: String,
        /// Host-interpreted intent; never executed by this crate.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
        /// Emoji glyph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
        /// Icon id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
    }
);

props!(
    /// ` ```actions ` payload.
    ActionsProps {
        /// Actions in order.
        actions: Vec<ActionProps>,
    }
);

props!(
    /// ` ```callout ` payload.
    CalloutProps {
        /// Markdown body.
        body: String,
        /// note | warn | error | success | tip | important.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tone: Option<String>,
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    }
);

props!(
    /// ` ```cite ` payload; at least one of quote, title or url is set.
    CiteProps {
        /// Quoted text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        quote: Option<String>,
        /// Source title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Source URL.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        /// Attribution.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    }
);

props!(
    /// A compare row.
    CompareAxis {
        /// Stable id.
        id: String,
        /// Row label.
        label: String,
    }
);

props!(
    /// A compare column.
    CompareEntity {
        /// Stable id.
        id: String,
        /// Column label.
        label: String,
        /// Axis id to cell text.
        values: BTreeMap<String, String>,
    }
);

props!(
    /// ` ```compare ` payload.
    CompareProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Highlighted recommendation.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recommendation: Option<String>,
        /// matrix (default) | faceoff.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<String>,
        /// Rows.
        axes: Vec<CompareAxis>,
        /// Columns.
        entities: Vec<CompareEntity>,
    }
);

props!(
    /// A plan segment.
    PlanSegment {
        /// Stable id.
        id: String,
        /// Label.
        label: String,
        /// Time label.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time: Option<String>,
        /// Emoji glyph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
        /// Icon id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        /// Image.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Body.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        /// Badge.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        badge: Option<String>,
    }
);

props!(
    /// ` ```plan ` payload.
    PlanProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// day | phase | milestone.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        grouping: Option<String>,
        /// Two or more segments.
        segments: Vec<PlanSegment>,
    }
);

props!(
    /// A timeline event.
    TimelineEvent {
        /// Stable id.
        id: String,
        /// Label.
        label: String,
        /// Timestamp label.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ts: Option<String>,
        /// Detail.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        /// Lane.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lane: Option<String>,
        /// Emoji glyph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
        /// Icon id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        /// Metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<String>,
        /// Body.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        /// Image.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image: Option<String>,
        /// Media.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media: Option<String>,
    }
);

props!(
    /// ` ```timeline ` payload.
    TimelineProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// day | hour | event.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        granularity: Option<String>,
        /// rail | snapshot.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layout: Option<String>,
        /// Two or more events.
        events: Vec<TimelineEvent>,
    }
);

props!(
    /// A shortlist item.
    ShortlistItem {
        /// Stable id.
        id: String,
        /// Label.
        label: String,
        /// Summary.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
        /// Score.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score: Option<String>,
        /// Metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<String>,
        /// Emoji glyph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
        /// Icon id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        /// Image.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    }
);

props!(
    /// ` ```shortlist ` payload.
    ShortlistProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Criteria.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        criteria: Option<String>,
        /// comfortable | compact.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        density: Option<String>,
        /// Two or more items.
        items: Vec<ShortlistItem>,
    }
);

props!(
    /// A decision option.
    DecisionOption {
        /// Stable id.
        id: String,
        /// Label.
        label: String,
        /// Pros.
        pros: Vec<String>,
        /// Cons.
        cons: Vec<String>,
        /// Score.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        score: Option<String>,
        /// Summary.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    }
);

props!(
    /// ` ```decision ` payload.
    DecisionProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Factors.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        factors: Option<String>,
        /// Recommendation.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recommendation: Option<String>,
        /// Two or more options.
        options: Vec<DecisionOption>,
    }
);

props!(
    /// A brief section.
    BriefSection {
        /// Stable id.
        id: String,
        /// Heading.
        heading: String,
        /// Markdown body.
        body: String,
    }
);

props!(
    /// A brief source.
    BriefSource {
        /// Stable id.
        id: String,
        /// Title.
        title: String,
        /// URL.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        /// Quote.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        quote: Option<String>,
    }
);

props!(
    /// ` ```brief ` payload.
    BriefProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// research | brief | memo.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tone: Option<String>,
        /// Sections.
        sections: Vec<BriefSection>,
        /// Sources.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sources: Option<Vec<BriefSource>>,
    }
);

props!(
    /// A dashboard tile.
    DashboardTile {
        /// Stable id.
        id: String,
        /// Label.
        label: String,
        /// Value text.
        value: String,
        /// Delta text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delta: Option<String>,
        /// default | accent | success | warn | error.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tone: Option<String>,
        /// Emoji glyph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
        /// Icon id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        /// Hint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
        /// Unit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
        /// Host feed id that may update the value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        feed: Option<String>,
        /// Field within the feed payload.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<String>,
        /// manual | interval | both. Feed-bound tiles default to both.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refresh: Option<String>,
        /// Poll period in milliseconds when interval refresh is on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interval_ms: Option<u64>,
    }
);

props!(
    /// ` ```dashboard ` payload.
    DashboardProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// 2 | 3 | 4.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        columns: Option<String>,
        /// Two or more tiles.
        tiles: Vec<DashboardTile>,
    }
);

props!(
    /// ` ```report `: narrative plus nested figures.
    ReportProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Figure columns: 1 | 2 | 3.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        columns: Option<String>,
        /// Markdown body; may reference nested blocks.
        body: String,
    }
);

props!(
    /// An image layer positioned in slide-relative units (0..1).
    SlideLayer {
        /// Stable id.
        id: String,
        /// Image path or URL.
        src: String,
        /// Left.
        x: f64,
        /// Top.
        y: f64,
        /// Width.
        w: f64,
        /// Height.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        h: Option<f64>,
    }
);

props!(
    /// One slide.
    SlideItem {
        /// Stable id.
        id: String,
        /// Label.
        label: String,
        /// hero | split | stack.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layout: Option<String>,
        /// Markdown body.
        body: String,
        /// Named wash or image path/URL.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bg: Option<String>,
        /// dark | light | none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scrim: Option<String>,
        /// Image layers.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layers: Option<Vec<SlideLayer>>,
        /// fade | fade-up.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        motion: Option<String>,
        /// Speaker notes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
    }
);

props!(
    /// ` ```slides ` payload.
    SlidesProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// paper | dusk | ink | mist | ember.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        theme: Option<String>,
        /// Figure columns: 1 | 2 | 3.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        columns: Option<String>,
        /// Slides.
        slides: Vec<SlideItem>,
    }
);

props!(
    /// ` ```section ` payload.
    SectionProps {
        /// Title.
        title: String,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Markdown body.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
    }
);

props!(
    /// A tab panel.
    TabsPanel {
        /// Stable id.
        id: String,
        /// Label.
        label: String,
        /// Markdown body.
        body: String,
        /// Emoji glyph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
        /// Icon id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
    }
);

props!(
    /// ` ```tabs ` payload.
    TabsProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Initial panel id, label, or 1-based index.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        /// Two or more panels.
        panels: Vec<TabsPanel>,
    }
);

props!(
    /// A step.
    StepItem {
        /// Stable id.
        id: String,
        /// Label.
        label: String,
        /// Markdown body.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        /// done | current | pending.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        /// Emoji glyph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
        /// Icon id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
    }
);

props!(
    /// ` ```steps ` payload.
    StepsProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Two or more steps.
        steps: Vec<StepItem>,
    }
);

props!(
    /// A recipe step, optionally timed.
    RecipeStep {
        /// Stable id.
        id: String,
        /// Label.
        label: String,
        /// Markdown body.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<String>,
        /// Timer length in milliseconds (1 s ..= 24 h).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
        /// Original duration text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_label: Option<String>,
    }
);

props!(
    /// ` ```recipe ` / ` ```procedure ` payload.
    RecipeProps {
        /// Title.
        title: String,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Yield / servings.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        r#yield: Option<String>,
        /// Ingredients.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ingredients: Option<Vec<String>>,
        /// Resources.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resources: Option<Vec<String>>,
        /// Steps.
        steps: Vec<RecipeStep>,
        /// Notes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        notes: Option<String>,
        /// Up to three follow-up actions.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actions: Option<Vec<ActionProps>>,
    }
);

props!(
    /// An accordion item.
    AccordionItem {
        /// Stable id.
        id: String,
        /// Label.
        label: String,
        /// Markdown body.
        body: String,
        /// Initially open.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        open: Option<bool>,
        /// Emoji glyph.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        emoji: Option<String>,
        /// Icon id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
    }
);

props!(
    /// ` ```accordion ` payload.
    AccordionProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Allow several open panels.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        multiple: Option<bool>,
        /// Items.
        items: Vec<AccordionItem>,
    }
);

props!(
    /// Liquid ` ```code ` payload (with `lang:` / `title:` / `---` chrome).
    CodeProps {
        /// Source text.
        source: String,
        /// Language badge.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lang: Option<String>,
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Render as a unified diff.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff: Option<bool>,
        /// Show a copy control (default true).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        copy: Option<bool>,
    }
);

/// File or folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TreeKind {
    /// Leaf.
    File,
    /// Has (or may have) children.
    Folder,
}

/// A file tree node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TreeNode {
    /// Stable id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// File or folder.
    pub kind: TreeKind,
    /// Children (folders only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<TreeNode>>,
}

props!(
    /// ` ```tree ` payload.
    TreeProps {
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Subtitle.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
        /// Root nodes.
        nodes: Vec<TreeNode>,
    }
);

props!(
    /// A chip.
    ChipProps {
        /// Label.
        label: String,
        /// default | accent | success | warn.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tone: Option<String>,
        /// Trailing value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    }
);

props!(
    /// ` ```chips ` payload.
    ChipsProps {
        /// Chips.
        chips: Vec<ChipProps>,
    }
);

props!(
    /// ` ```media ` payload.
    MediaProps {
        /// Image path or URL.
        src: String,
        /// Alt text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alt: Option<String>,
        /// Caption.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
        /// Aspect ratio.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ratio: Option<String>,
    }
);

props!(
    /// ` ```feed `: live content the host loads by id.
    FeedProps {
        /// Host feed id.
        feed_id: String,
        /// md | text | json | csv | image.
        datatype: String,
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Empty-state text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        empty: Option<String>,
        /// manual | load.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refresh: Option<String>,
    }
);

props!(
    /// ` ```block `: styled prose.
    StyledBlockProps {
        /// Markdown body.
        body: String,
        /// Anchor id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// sans | serif | mono.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font: Option<String>,
        /// sm | md | lg | xl | px/rem/em length.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<String>,
        /// left | center | right | justify.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        align: Option<String>,
        /// tight | normal | relaxed | 1..=3.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spacing: Option<String>,
    }
);

props!(
    /// A kanban card.
    KanbanCard {
        /// Text.
        text: String,
        /// Task checked.
        checked: bool,
    }
);

props!(
    /// A kanban column.
    KanbanColumn {
        /// Title.
        title: String,
        /// Cards.
        cards: Vec<KanbanCard>,
    }
);

props!(
    /// ` ```kanban ` payload (static board).
    KanbanProps {
        /// Columns.
        columns: Vec<KanbanColumn>,
    }
);
