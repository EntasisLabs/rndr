//! One retained subtree per Liquid block, built from `rndr_ui` parts.
use crate::chart;
use crate::state::LiquidComponentState;
use crate::view::{nested, with_host, BlockRoot};
use crate::{LiquidEvent, LiquidImage};
use rndr_core::*;
use rndr_liquid::*;
use rndr_ui::controls::{Button, ButtonProps, ButtonVariant, HostIntent, ModeSelector};
use rndr_ui::{text, Callout, Card, Feedback, Surface, Typography, UiId};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

/// What activating one control inside a block does.
#[derive(Clone, Debug)]
pub(crate) enum Act {
    Host(LiquidEvent),
    Prev,
    Next,
    Toggle(usize),
    Check(usize),
}

/// Control root → action, on the block root.
#[derive(Default)]
pub(crate) struct Actions(pub HashMap<Entity, Act>);

impl Component for Actions {}

/// Reader state that survives a rebuild of the block.
#[derive(Clone, Default)]
pub(crate) struct BlockState {
    ready: bool,
    single: bool,
    len: usize,
    pub(crate) selected: usize,
    pub(crate) open: BTreeSet<usize>,
    pub(crate) checked: BTreeSet<usize>,
}

impl Component for BlockState {}

impl BlockState {
    pub(crate) fn apply(&mut self, act: &Act) {
        match *act {
            Act::Prev => self.selected = self.selected.saturating_sub(1),
            Act::Next => self.selected = (self.selected + 1).min(self.len.saturating_sub(1)),
            Act::Toggle(i) => {
                if !self.open.remove(&i) {
                    if self.single {
                        self.open.clear();
                    }
                    self.open.insert(i);
                }
            }
            Act::Check(i) => {
                if !self.checked.remove(&i) {
                    self.checked.insert(i);
                }
            }
            Act::Host(_) => {}
        }
    }

    fn init(&mut self, block: &LiquidBlock) {
        if self.ready {
            return;
        }
        self.ready = true;
        match block {
            LiquidBlock::Carousel(p) => self.len = p.items.len(),
            LiquidBlock::Slides(p) => self.len = p.slides.len(),
            LiquidBlock::Accordion(p) => {
                self.single = !p.multiple.unwrap_or(false);
                self.open = p
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| i.open.unwrap_or(false))
                    .map(|(n, _)| n)
                    .collect();
                if self.single {
                    let first = self.open.iter().next().copied();
                    self.open = first.into_iter().collect();
                }
            }
            LiquidBlock::Steps(p) => {
                self.checked = p
                    .steps
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| matches!(s.status.as_deref(), Some("done" | "complete")))
                    .map(|(n, _)| n)
                    .collect();
            }
            LiquidBlock::Kanban(p) => {
                self.checked = p
                    .columns
                    .iter()
                    .flat_map(|c| &c.cards)
                    .enumerate()
                    .filter(|(_, c)| c.checked)
                    .map(|(n, _)| n)
                    .collect();
            }
            _ => {}
        }
    }

    pub(crate) fn select(&mut self, index: usize) {
        self.selected = index;
        self.ready = true;
    }

    pub(crate) fn snapshot(
        &self,
        block: &LiquidBlock,
    ) -> Option<(&'static str, LiquidComponentState)> {
        let state = match block {
            LiquidBlock::Accordion(_) => LiquidComponentState::Accordion {
                open: self.open.iter().copied().collect(),
            },
            LiquidBlock::Tabs(_) => LiquidComponentState::Tabs {
                selected: self.selected,
            },
            LiquidBlock::Steps(_) | LiquidBlock::Recipe(_) | LiquidBlock::Kanban(_) => {
                LiquidComponentState::Checklist {
                    checked: self.checked.iter().copied().collect(),
                }
            }
            LiquidBlock::Carousel(_) | LiquidBlock::Slides(_) => LiquidComponentState::Selection {
                selected: self.selected,
            },
            _ => return None,
        };
        Some((block.kind(), state))
    }

    pub(crate) fn absorb(&mut self, block: &LiquidBlock, snap: &LiquidComponentState) {
        self.ready = false;
        self.init(block);
        match snap {
            LiquidComponentState::Accordion { open } => {
                self.open = open.iter().copied().collect();
            }
            LiquidComponentState::Tabs { selected }
            | LiquidComponentState::Selection { selected } => {
                self.selected = *selected;
            }
            LiquidComponentState::Checklist { checked } => {
                self.checked = checked.iter().copied().collect();
            }
        }
        self.ready = true;
    }
}

/// Clear `root` and render its block again from [`BlockRoot`] and
/// [`BlockState`].
pub(crate) fn rebuild(world: &mut World, root: Entity) {
    let Some(BlockRoot { blocks, index, .. }) = world.get::<BlockRoot>(root).cloned() else {
        return;
    };
    let Some(block) = blocks.get(index).cloned() else {
        return;
    };
    if !matches!(block, LiquidBlock::Chart(_)) {
        chart::release(world, root);
    }
    for child in children_of(world, root) {
        despawn(world, child);
    }
    let mut state = world.remove::<BlockState>(root).unwrap_or_default();
    state.init(&block);
    if let Some(layout) = world.get_mut::<Layout>(root) {
        layout.gap = 8.0;
    }
    let mut cx = Cx {
        world,
        blocks: Arc::clone(&blocks),
        root,
        actions: Actions::default(),
        state,
        next: 1,
    };
    cx.block(root, &block);
    let Cx { actions, state, .. } = cx;
    world.insert(root, actions);
    world.insert(root, state);
}

/// Marker on the stand-in for an unclosed Liquid fence.
pub(crate) struct PendingMark;

impl Component for PendingMark {}

/// Show `lang` as a skeleton under `view`, or remove the skeleton.
pub(crate) fn sync_pending(
    world: &mut World,
    view: Entity,
    slot: &mut Option<Entity>,
    lang: Option<&str>,
) {
    let Some(lang) = lang else {
        if let Some(entity) = slot.take() {
            if world.is_alive(entity) {
                despawn(world, entity);
            }
        }
        return;
    };
    let text_value = if lang.is_empty() {
        "Pending block".to_string()
    } else {
        format!("Pending {lang}")
    };
    let entity = match *slot {
        Some(entity) if world.is_alive(entity) => entity,
        _ => {
            let entity = world.spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Hug,
                    gap: 4.0,
                    ..Layout::column()
                },
                PendingMark,
            ));
            set_parent(world, entity, view);
            let _ = text(world, entity, &text_value, Typography::Secondary);
            *slot = Some(entity);
            entity
        }
    };
    if let Some(label) = children_of(world, entity)
        .into_iter()
        .find(|child| world.has::<Label>(*child))
    {
        if let Some(label) = world.get_mut::<Label>(label) {
            if label.text != text_value {
                label.text = text_value;
            }
        }
    }
    let mut list = world
        .get::<Children>(view)
        .map(|children| children.list.clone())
        .unwrap_or_default();
    if list.last() != Some(&entity) {
        list.retain(|child| *child != entity);
        list.push(entity);
        world.insert(view, Children { list });
    }
    world.insert(entity, Parent { entity: view });
}

/// Text with `{{icon:id}}` shortcodes removed. Icons have no glyphs here.
pub fn plain(text: &str) -> String {
    let mut out = String::new();
    for part in split_icons(text) {
        if let InlineIcon::Text(t) = part {
            out.push_str(t);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn filled(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

struct Cx<'w> {
    world: &'w mut World,
    blocks: Arc<Vec<LiquidBlock>>,
    root: Entity,
    actions: Actions,
    state: BlockState,
    next: u64,
}

impl Cx<'_> {
    fn id(&mut self) -> UiId {
        self.next += 1;
        UiId(self.next)
    }

    fn text(&mut self, parent: Entity, value: &str, role: Typography) {
        let value = plain(value);
        if !value.is_empty() {
            let _ = text(self.world, parent, &value, role);
        }
    }

    fn opt(&mut self, parent: Entity, value: &Option<String>, role: Typography) {
        if let Some(v) = filled(value) {
            self.text(parent, v, role);
        }
    }

    fn stack(&mut self, parent: Entity, axis: Axis, gap: f32) -> Entity {
        let e = self.world.spawn((Layout {
            axis,
            gap,
            width: Size::Fill,
            ..Layout::column()
        },));
        set_parent(self.world, e, parent);
        e
    }

    fn column(&mut self, parent: Entity) -> Entity {
        self.stack(parent, Axis::Column, 6.0)
    }

    fn row(&mut self, parent: Entity) -> Entity {
        self.stack(parent, Axis::Row, 8.0)
    }

    fn surface(&mut self, parent: Entity) -> Entity {
        let id = self.id();
        match Surface::Raised.mount(self.world, parent, id) {
            Ok(h) => h.content,
            Err(_) => self.column(parent),
        }
    }

    fn card(&mut self, parent: Entity, title: &str) -> Entity {
        let id = self.id();
        let title = plain(title);
        match Card::mount(self.world, parent, id, &title) {
            Ok(h) => h.content,
            Err(_) => {
                let c = self.surface(parent);
                self.text(c, &title, Typography::Heading);
                c
            }
        }
    }

    fn callout(&mut self, parent: Entity, tone: Option<&str>, title: &str) -> Entity {
        let id = self.id();
        let meaning = match tone.unwrap_or("info").to_ascii_lowercase().as_str() {
            "success" | "ok" | "done" | "good" | "positive" => Feedback::Success,
            "warn" | "warning" | "caution" | "important" => Feedback::Warning,
            "danger" | "error" | "critical" | "negative" | "bad" => Feedback::Danger,
            _ => Feedback::Info,
        };
        let title = plain(title);
        let title = if title.is_empty() {
            "Note".to_string()
        } else {
            title
        };
        match Callout::mount(self.world, parent, id, meaning, &title) {
            Ok(h) => h.content,
            Err(_) => self.surface(parent),
        }
    }

    fn button(&mut self, parent: Entity, label: &str, variant: ButtonVariant, act: Act) {
        let label = plain(label);
        if label.is_empty() {
            return;
        }
        let id = self.id();
        let mut props = ButtonProps::new(label, HostIntent::Command(id.0));
        props.variant = variant;
        if let Ok(h) = Button::mount(self.world, parent, id, props) {
            self.actions.0.insert(h.root, act);
        }
    }

    fn markdown(&mut self, parent: Entity, body: &str) {
        if !body.trim().is_empty() {
            nested(self.world, parent, &self.blocks, body);
        }
    }

    fn heading(&mut self, parent: Entity, title: &Option<String>, subtitle: &Option<String>) {
        self.opt(parent, title, Typography::Heading);
        self.opt(parent, subtitle, Typography::Secondary);
    }

    fn image(&mut self, parent: Entity, src: &str, alt: Option<&str>, height: f32) -> bool {
        let src = src.to_string();
        let Some(LiquidImage { texture, aspect }) =
            with_host(self.world, |host, world| host.image(world, &src))
        else {
            if let Some(alt) = alt {
                self.text(parent, alt, Typography::Secondary);
            }
            return false;
        };
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        let e = self.world.spawn((
            Layout {
                width: Size::Px(height * aspect),
                height: Size::Px(height),
                ..Layout::column()
            },
            ImageView::new(texture).with_radius(8.0),
        ));
        set_parent(self.world, e, parent);
        true
    }

    fn list(&mut self, parent: Entity, items: &[String], marker: &str) {
        for item in items {
            self.text(parent, &format!("{marker} {item}"), Typography::Reading);
        }
    }

    fn pager(&mut self, parent: Entity, at: usize, len: usize) {
        if len < 2 {
            return;
        }
        let row = self.row(parent);
        if at > 0 {
            self.button(row, "Previous", ButtonVariant::Secondary, Act::Prev);
        }
        self.text(row, &format!("{} / {}", at + 1, len), Typography::Secondary);
        if at + 1 < len {
            self.button(row, "Next", ButtonVariant::Secondary, Act::Next);
        }
    }

    fn block(&mut self, parent: Entity, block: &LiquidBlock) {
        match block {
            LiquidBlock::Card(p) => self.card_block(parent, p),
            LiquidBlock::Carousel(p) => {
                let at = self.state.selected.min(p.items.len().saturating_sub(1));
                if let Some(item) = p.items.get(at) {
                    self.card_block(parent, item);
                }
                self.pager(parent, at, p.items.len());
            }
            LiquidBlock::Actions(p) => self.actions(parent, &p.actions),
            LiquidBlock::Callout(p) => {
                let c = self.callout(parent, p.tone.as_deref(), p.title.as_deref().unwrap_or(""));
                self.markdown(c, &p.body);
            }
            LiquidBlock::Section(p) => {
                self.text(parent, &p.title, Typography::Title);
                self.opt(parent, &p.subtitle, Typography::Secondary);
                if let Some(body) = &p.body {
                    self.markdown(parent, body);
                }
            }
            LiquidBlock::Block(p) => self.markdown(parent, &p.body),
            LiquidBlock::Chips(p) => {
                let row = self.row(parent);
                for chip in &p.chips {
                    let s = self.surface(row);
                    let label = match filled(&chip.value) {
                        Some(v) => format!("{} · {v}", chip.label),
                        None => chip.label.clone(),
                    };
                    self.text(s, &label, Typography::Label);
                }
            }
            LiquidBlock::Media(p) => {
                let alt = filled(&p.alt).or(filled(&p.caption)).map(str::to_string);
                let shown = self.image(parent, &p.src, alt.as_deref(), 240.0);
                if shown {
                    self.opt(parent, &p.caption, Typography::Secondary);
                }
            }
            LiquidBlock::Cite(p) => {
                let s = self.surface(parent);
                if let Some(q) = filled(&p.quote) {
                    self.text(s, &format!("“{q}”"), Typography::Emphasis);
                }
                self.opt(s, &p.source, Typography::Secondary);
                if let Some(url) = filled(&p.url) {
                    let label = filled(&p.title).unwrap_or(url).to_string();
                    let act = Act::Host(LiquidEvent::OpenLink { href: url.into() });
                    self.button(s, &label, ButtonVariant::Quiet, act);
                } else {
                    self.opt(s, &p.title, Typography::Label);
                }
            }
            LiquidBlock::Compare(p) => self.compare(parent, p),
            LiquidBlock::Plan(p) => {
                self.heading(parent, &p.title, &p.subtitle);
                for seg in &p.segments {
                    let row = self.row(parent);
                    if let Some(t) = filled(&seg.time) {
                        self.text(row, t, Typography::Secondary);
                    }
                    let col = self.column(row);
                    let label = match filled(&seg.badge) {
                        Some(b) => format!("{} · {b}", seg.label),
                        None => seg.label.clone(),
                    };
                    self.text(col, &label, Typography::Emphasis);
                    self.opt(col, &seg.subtitle, Typography::Secondary);
                    if let Some(src) = filled(&seg.image) {
                        let src = src.to_string();
                        self.image(col, &src, None, 120.0);
                    }
                    self.opt(col, &seg.body, Typography::Reading);
                }
            }
            LiquidBlock::Timeline(p) => {
                self.heading(parent, &p.title, &p.subtitle);
                for event in &p.events {
                    let row = self.row(parent);
                    let ts = filled(&event.ts).unwrap_or("-").to_string();
                    self.text(row, &ts, Typography::Secondary);
                    let col = self.column(row);
                    self.text(col, &event.label, Typography::Emphasis);
                    self.opt(col, &event.detail, Typography::Reading);
                    self.opt(col, &event.body, Typography::Reading);
                    self.opt(col, &event.meta, Typography::Secondary);
                }
            }
            LiquidBlock::Shortlist(p) => {
                self.heading(parent, &p.title, &p.subtitle);
                self.opt(parent, &p.criteria, Typography::Secondary);
                for (n, item) in p.items.iter().enumerate() {
                    let s = self.surface(parent);
                    let label = match filled(&item.score) {
                        Some(score) => format!("{}. {} — {score}", n + 1, item.label),
                        None => format!("{}. {}", n + 1, item.label),
                    };
                    self.text(s, &label, Typography::Emphasis);
                    self.opt(s, &item.summary, Typography::Reading);
                    self.opt(s, &item.meta, Typography::Secondary);
                }
            }
            LiquidBlock::Decision(p) => {
                self.heading(parent, &p.title, &p.subtitle);
                self.opt(parent, &p.factors, Typography::Secondary);
                let row = self.row(parent);
                for option in &p.options {
                    let title = match filled(&option.score) {
                        Some(s) => format!("{} · {s}", option.label),
                        None => option.label.clone(),
                    };
                    let c = self.card(row, &title);
                    self.opt(c, &option.summary, Typography::Reading);
                    self.list(c, &option.pros, "+");
                    self.list(c, &option.cons, "−");
                }
                if let Some(r) = filled(&p.recommendation) {
                    let r = r.to_string();
                    let c = self.callout(parent, Some("success"), "Recommendation");
                    self.text(c, &r, Typography::Reading);
                }
            }
            LiquidBlock::Brief(p) => {
                self.heading(parent, &p.title, &p.subtitle);
                for section in &p.sections {
                    self.text(parent, &section.heading, Typography::Heading);
                    self.markdown(parent, &section.body);
                }
                if let Some(sources) = p.sources.as_ref().filter(|s| !s.is_empty()) {
                    self.text(parent, "Sources", Typography::Label);
                    for source in sources {
                        match filled(&source.url) {
                            Some(url) => {
                                let act = Act::Host(LiquidEvent::OpenLink { href: url.into() });
                                self.button(parent, &source.title, ButtonVariant::Quiet, act);
                            }
                            None => self.text(parent, &source.title, Typography::Reading),
                        }
                        self.opt(parent, &source.quote, Typography::Secondary);
                    }
                }
            }
            LiquidBlock::Dashboard(p) => self.dashboard(parent, p),
            LiquidBlock::Chart(p) => chart::mount(self.world, self.root, parent, p),
            LiquidBlock::Report(p) => {
                self.heading(parent, &p.title, &p.subtitle);
                self.markdown(parent, &p.body);
            }
            LiquidBlock::Slides(p) => {
                self.opt(parent, &p.title, Typography::Label);
                let at = self.state.selected.min(p.slides.len().saturating_sub(1));
                if let Some(slide) = p.slides.get(at) {
                    let s = self.surface(parent);
                    self.text(s, &slide.label, Typography::Title);
                    self.markdown(s, &slide.body);
                    self.opt(parent, &slide.notes, Typography::Secondary);
                }
                self.pager(parent, at, p.slides.len());
            }
            LiquidBlock::Tabs(p) => self.tabs(parent, p),
            LiquidBlock::Steps(p) => {
                self.heading(parent, &p.title, &p.subtitle);
                for (n, step) in p.steps.iter().enumerate() {
                    let done = self.state.checked.contains(&n);
                    let row = self.row(parent);
                    let mark = if done { "[x]" } else { "[ ]" };
                    self.button(row, mark, ButtonVariant::Quiet, Act::Check(n));
                    let col = self.column(row);
                    self.text(
                        col,
                        &format!("{}. {}", n + 1, step.label),
                        Typography::Emphasis,
                    );
                    self.opt(col, &step.body, Typography::Reading);
                }
            }
            LiquidBlock::Recipe(p) => self.recipe(parent, p),
            LiquidBlock::Accordion(p) => {
                self.heading(parent, &p.title, &p.subtitle);
                for (n, item) in p.items.iter().enumerate() {
                    let open = self.state.open.contains(&n);
                    let label = format!("{} {}", if open { "−" } else { "+" }, item.label);
                    self.button(parent, &label, ButtonVariant::Quiet, Act::Toggle(n));
                    if open {
                        let s = self.column(parent);
                        self.markdown(s, &item.body);
                    }
                }
            }
            LiquidBlock::Code(p) => {
                let row = self.row(parent);
                let title = filled(&p.title)
                    .or(filled(&p.lang))
                    .unwrap_or("Code")
                    .to_string();
                self.text(row, &title, Typography::Label);
                if p.copy.unwrap_or(true) {
                    let act = Act::Host(LiquidEvent::Copy {
                        text: p.source.clone(),
                    });
                    self.button(row, "Copy", ButtonVariant::Quiet, act);
                }
                let s = self.surface(parent);
                let _ = text(self.world, s, &p.source, Typography::Code);
            }
            LiquidBlock::Tree(p) => {
                self.heading(parent, &p.title, &p.subtitle);
                let s = self.surface(parent);
                let mut lines = Vec::new();
                tree_lines(&p.nodes, 0, &mut lines);
                let _ = text(self.world, s, &lines.join("\n"), Typography::Code);
            }
            LiquidBlock::Kanban(p) => {
                let row = self.row(parent);
                let mut n = 0;
                for column in &p.columns {
                    let c = self.card(row, &column.title);
                    for card in &column.cards {
                        let done = self.state.checked.contains(&n);
                        let label = format!("{} {}", if done { "[x]" } else { "[ ]" }, card.text);
                        self.button(c, &label, ButtonVariant::Quiet, Act::Check(n));
                        n += 1;
                    }
                }
            }
            LiquidBlock::Feed(p) => {
                let row = self.row(parent);
                self.text(
                    row,
                    p.title.as_deref().unwrap_or(&p.feed_id),
                    Typography::Heading,
                );
                let act = Act::Host(LiquidEvent::RefreshFeed {
                    feed_id: p.feed_id.clone(),
                });
                self.button(row, "Refresh", ButtonVariant::Quiet, act);
                let (id, kind) = (p.feed_id.clone(), p.datatype.clone());
                let rows = with_host(self.world, |host, world| host.feed(world, &id, &kind));
                if rows.is_empty() {
                    let empty = filled(&p.empty).unwrap_or("Nothing here yet.").to_string();
                    self.text(parent, &empty, Typography::Secondary);
                }
                for entry in rows {
                    let s = self.surface(parent);
                    self.text(s, &entry.title, Typography::Emphasis);
                    self.opt(s, &entry.detail, Typography::Reading);
                    self.opt(s, &entry.meta, Typography::Secondary);
                }
            }
        }
    }

    fn card_block(&mut self, parent: Entity, p: &CardProps) {
        let c = self.card(parent, &p.title);
        self.opt(c, &p.subtitle, Typography::Secondary);
        if let Some(src) = filled(&p.image) {
            let src = src.to_string();
            self.image(c, &src, None, 160.0);
        }
        self.opt(c, &p.body, Typography::Reading);
        self.opt(c, &p.summary, Typography::Reading);
        for point in p.points.iter().flatten() {
            self.text(
                c,
                &format!("{} — {}", point.label, point.body),
                Typography::Reading,
            );
        }
        let tags: Vec<String> = p.chips.iter().chain(&p.badges).flatten().cloned().collect();
        if !tags.is_empty() {
            self.text(c, &tags.join(" · "), Typography::Label);
        }
        self.opt(c, &p.meta, Typography::Secondary);
    }

    fn actions(&mut self, parent: Entity, actions: &[ActionProps]) {
        let row = self.row(parent);
        for (n, a) in actions.iter().enumerate() {
            let variant = if n == 0 {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Secondary
            };
            let act = Act::Host(LiquidEvent::Action {
                label: plain(&a.label),
                intent: a.intent.clone(),
            });
            self.button(row, &a.label, variant, act);
        }
    }

    fn compare(&mut self, parent: Entity, p: &CompareProps) {
        self.heading(parent, &p.title, &p.subtitle);
        let s = self.surface(parent);
        let header = self.row(s);
        let _ = text(self.world, header, " ", Typography::Label);
        for entity in &p.entities {
            self.text(header, &entity.label, Typography::Emphasis);
        }
        for axis in &p.axes {
            let row = self.row(s);
            self.text(row, &axis.label, Typography::Label);
            for entity in &p.entities {
                let v = entity
                    .values
                    .get(&axis.id)
                    .map(String::as_str)
                    .unwrap_or("—");
                let _ = text(self.world, row, &plain(v), Typography::Reading);
            }
        }
        if let Some(r) = filled(&p.recommendation) {
            let r = r.to_string();
            let c = self.callout(parent, Some("success"), "Recommendation");
            self.text(c, &r, Typography::Reading);
        }
    }

    fn dashboard(&mut self, parent: Entity, p: &DashboardProps) {
        self.heading(parent, &p.title, &p.subtitle);
        let columns = p
            .columns
            .as_deref()
            .and_then(|c| c.trim().parse::<usize>().ok())
            .filter(|c| (1..=6).contains(c))
            .unwrap_or(3);
        for chunk in p.tiles.chunks(columns) {
            let row = self.row(parent);
            for tile in chunk {
                let s = self.surface(row);
                self.text(s, &tile.label, Typography::Secondary);
                let live = tile.feed.as_deref().and_then(|feed| {
                    let (feed, field) = (feed.to_string(), tile.field.clone());
                    with_host(self.world, |host, world| {
                        host.tile(world, &feed, field.as_deref())
                    })
                });
                let value = live.unwrap_or_else(|| tile.value.clone());
                let value = match filled(&tile.unit) {
                    Some(u) => format!("{value} {u}"),
                    None => value,
                };
                self.text(s, &value, Typography::Title);
                self.opt(s, &tile.delta, Typography::Label);
                self.opt(s, &tile.hint, Typography::Secondary);
            }
            for _ in chunk.len()..columns {
                self.column(row);
            }
        }
    }

    fn tabs(&mut self, parent: Entity, p: &TabsProps) {
        self.heading(parent, &p.title, &p.subtitle);
        let labels: Vec<String> = p
            .panels
            .iter()
            .enumerate()
            .map(|(n, t)| {
                let l = plain(&t.label);
                if l.is_empty() {
                    format!("Tab {}", n + 1)
                } else {
                    l
                }
            })
            .collect();
        let initial = p
            .default
            .as_deref()
            .and_then(|d| p.panels.iter().position(|t| t.id == d || t.label == d))
            .unwrap_or(0);
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let name = p
            .title
            .as_deref()
            .map(plain)
            .unwrap_or_else(|| "Tabs".into());
        let id = self.id();
        let Ok(handle) = ModeSelector::mount(
            self.world,
            parent,
            id,
            &name,
            &refs,
            TabOrientation::Horizontal,
            initial,
        ) else {
            return;
        };
        for (panel, tab) in handle.panels.iter().zip(&p.panels) {
            self.markdown(*panel, &tab.body);
        }
    }

    fn recipe(&mut self, parent: Entity, p: &RecipeProps) {
        self.text(parent, &p.title, Typography::Title);
        self.opt(parent, &p.subtitle, Typography::Secondary);
        if let Some(y) = filled(&p.r#yield) {
            self.text(parent, &format!("Yield: {y}"), Typography::Label);
        }
        for (title, items) in [("Ingredients", &p.ingredients), ("Resources", &p.resources)] {
            if let Some(items) = items.as_ref().filter(|i| !i.is_empty()) {
                self.text(parent, title, Typography::Heading);
                self.list(parent, items, "-");
            }
        }
        self.text(parent, "Steps", Typography::Heading);
        for (n, step) in p.steps.iter().enumerate() {
            let done = self.state.checked.contains(&n);
            let row = self.row(parent);
            self.button(
                row,
                if done { "[x]" } else { "[ ]" },
                ButtonVariant::Quiet,
                Act::Check(n),
            );
            let col = self.column(row);
            self.text(
                col,
                &format!("{}. {}", n + 1, step.label),
                Typography::Emphasis,
            );
            self.opt(col, &step.body, Typography::Reading);
            if let Some(ms) = step.duration_ms.filter(|ms| *ms > 0) {
                let label = filled(&step.duration_label)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{} min", ms.div_ceil(60_000)));
                let act = Act::Host(LiquidEvent::StartTimer {
                    label: plain(&step.label),
                    duration_ms: ms,
                });
                self.button(
                    row,
                    &format!("Timer {label}"),
                    ButtonVariant::Secondary,
                    act,
                );
            }
        }
        if let Some(notes) = filled(&p.notes) {
            let notes = notes.to_string();
            let c = self.callout(parent, Some("info"), "Notes");
            self.markdown(c, &notes);
        }
        if let Some(actions) = &p.actions {
            self.actions(parent, actions);
        }
    }
}

fn tree_lines(nodes: &[TreeNode], depth: usize, out: &mut Vec<String>) {
    for node in nodes {
        let name = match node.kind {
            TreeKind::Folder => format!("{}/", node.name),
            TreeKind::File => node.name.clone(),
        };
        out.push(format!("{}{}", "  ".repeat(depth), name));
        if let Some(children) = &node.children {
            tree_lines(children, depth + 1, out);
        }
    }
}
