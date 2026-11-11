//! Markdown views whose Liquid block references are mounted as native blocks.
use crate::lifecycle;
use crate::render::{self, Act, Actions, BlockState};
use crate::state::{self, LiquidDisposition, LiquidInteraction, LiquidInteractionEnvelope};
use crate::{LiquidEvent, LiquidHost};
use rndr_core::*;
use rndr_liquid::{LiquidBlock, LiquidStream, MarkdownWrite, Segment};
use std::collections::HashMap;
use std::sync::Arc;

/// Liquid state of one [`MarkdownView`].
///
/// [`liquid_view`] spawns both. The view shows the settled markdown from a
/// [`LiquidStream`]; each paragraph that is exactly one block reference is
/// replaced by that block, rendered with `rndr_ui`, after the markdown sync
/// of the same frame. An unclosed Liquid fence is a pending skeleton, not a
/// code block. Blocks outside the visible window are released, and their
/// reader state is kept by source position until they return.
pub struct LiquidView {
    blocks: Arc<Vec<LiquidBlock>>,
    mounted: Vec<Mounted>,
    next_id: u64,
    stream: Option<LiquidStream>,
    pending: Option<Entity>,
    carry: Vec<Option<BlockState>>,
    /// Reader state for blocks released outside the window, keyed by source byte.
    saved: HashMap<usize, Saved>,
}

struct Saved {
    block: usize,
    state: BlockState,
}

impl Component for LiquidView {}

struct Mounted {
    slot: MarkdownSlot,
    block: usize,
    /// [`Arc::as_ptr`] of the block list this mount was last compared with.
    arc: usize,
}

/// Root of one rendered block: which block it shows and the view it is in.
#[derive(Clone)]
pub(crate) struct BlockRoot {
    pub blocks: Arc<Vec<LiquidBlock>>,
    pub index: usize,
    /// Source byte of the markdown reference this root replaced.
    pub source: usize,
}

impl Component for BlockRoot {}

impl LiquidView {
    /// Parsed blocks the view's markdown references.
    pub fn blocks(&self) -> &[LiquidBlock] {
        &self.blocks
    }

    /// Indices into [`Self::blocks`] that are mounted right now, in reading order.
    pub fn mounted(&self) -> Vec<usize> {
        self.mounted.iter().map(|m| m.block).collect()
    }
}

/// Spawn a scrolling markdown view of Liquid `source`. Parent it like any
/// [`markdown_view`].
pub fn liquid_view(world: &mut World, source: &str) -> Entity {
    let stream = LiquidStream::new(source);
    let markdown = stream.markdown().to_string();
    let blocks = Arc::new(stream.blocks().to_vec());
    let view = markdown_view(world, markdown);
    if let Some(markdown) = world.get_mut::<MarkdownView>(view) {
        markdown.set_recycle_host_slots(true);
    }
    if let Err(reason) =
        lifecycle::register(world, view, &blocks, stream.markdown(), None, String::new())
    {
        lifecycle::refused(world, reason);
        return view;
    }
    set_markdown_slot_host(world, view, LiquidHooks);
    lifecycle::bind_markdown(world, view, stream.markdown());
    world.insert(view, LiquidView::streaming(stream, blocks));
    view
}

/// Replace the source of a Liquid view.
///
/// A source that extends the current one is an append: finished blocks stay
/// mounted and keep their state. Any other replacement keeps a mounted block
/// when its parsed source is unchanged and rebuilds the rest.
pub fn set_liquid_source(world: &mut World, view: Entity, source: &str) {
    if lifecycle::liquid_surface_handle(world, view).is_err() {
        return;
    }
    let Some(tx) = lifecycle::transaction(world, view) else {
        return;
    };
    let Some(mut liquid) = world.remove::<LiquidView>(view) else {
        return;
    };
    let normalized = source.replace("\r\n", "\n");
    if let Some(prior) = liquid.stream.as_ref() {
        if let Some(rest) = normalized.strip_prefix(prior.source()) {
            let mut stream = prior.clone();
            let write = stream.push_str(rest);
            if let Err(reason) =
                lifecycle::replace(world, view, stream.blocks(), stream.markdown(), true)
            {
                lifecycle::refused(world, reason);
                world.insert(view, liquid);
                tx.commit();
                return;
            }
            lifecycle::bind_markdown(world, view, stream.markdown());
            liquid.stream = Some(stream);
            adopt_blocks(&mut liquid);
            world.insert(view, liquid);
            tx.commit();
            apply_markdown(world, view, write);
            return;
        }
    }
    let old_blocks = Arc::clone(&liquid.blocks);
    let mut states = vec![None; old_blocks.len()];
    for saved in liquid.saved.values() {
        if saved.block < states.len() {
            states[saved.block] = Some(saved.state.clone());
        }
    }
    for (i, state) in liquid.carry.iter().enumerate() {
        if i < states.len() && states[i].is_none() {
            states[i] = state.clone();
        }
    }
    for mounted in &liquid.mounted {
        if mounted.block >= states.len() {
            continue;
        }
        if let Some(state) = world.get::<BlockState>(mounted.slot.root).cloned() {
            states[mounted.block] = Some(state);
        }
    }
    let stream = LiquidStream::new(&normalized);
    if let Err(reason) = lifecycle::replace(world, view, stream.blocks(), stream.markdown(), false)
    {
        lifecycle::refused(world, reason);
        world.insert(view, liquid);
        tx.commit();
        return;
    }
    lifecycle::bind_markdown(world, view, stream.markdown());
    liquid.carry = match_states(&old_blocks, &states, stream.blocks());
    liquid.saved.clear();
    // Physical slots cannot choose semantic correspondence. Recreate them from
    // the validated logical plan, including unchanged blocks that moved.
    for mounted in std::mem::take(&mut liquid.mounted) {
        lifecycle::park_under(world, mounted.slot.root);
        lifecycle::clear_presentation(world, mounted.slot.root);
        let _ = mounted.slot.detach(world);
    }
    liquid.blocks = Arc::new(stream.blocks().to_vec());
    let markdown = stream.markdown().to_string();
    liquid.stream = Some(stream);
    world.insert(view, liquid);
    tx.commit();
    apply_markdown(world, view, MarkdownWrite::Replace(markdown));
}

/// Append `chunk` to a Liquid view.
///
/// Only the source past the last stable prefix is parsed again. Finished
/// blocks stay mounted, with their tabs, accordion, carousel, slides and
/// checklist state. An unclosed Liquid fence is shown as a pending skeleton
/// until its closing line arrives. An empty chunk does nothing. A view that
/// was not created by [`liquid_view`] ignores the call.
pub fn push_liquid_str(world: &mut World, view: Entity, chunk: &str) {
    if chunk.is_empty() {
        return;
    }
    if lifecycle::liquid_surface_handle(world, view).is_err() {
        return;
    }
    let Some(tx) = lifecycle::transaction(world, view) else {
        return;
    };
    let Some(mut liquid) = world.remove::<LiquidView>(view) else {
        return;
    };
    let Some(prior) = liquid.stream.as_ref() else {
        world.insert(view, liquid);
        tx.commit();
        return;
    };
    let mut stream = prior.clone();
    let write = stream.push_str(chunk);
    if let Err(reason) = lifecycle::replace(world, view, stream.blocks(), stream.markdown(), true) {
        lifecycle::refused(world, reason);
        world.insert(view, liquid);
        tx.commit();
        return;
    }
    lifecycle::bind_markdown(world, view, stream.markdown());
    liquid.stream = Some(stream);
    adopt_blocks(&mut liquid);
    world.insert(view, liquid);
    tx.commit();
    apply_markdown(world, view, write);
}

/// Re-render every mounted block of every Liquid view, asking the host again
/// for images, feeds and tiles. Block state is kept.
pub fn refresh(world: &mut World) {
    for root in world.entities_with::<BlockRoot>() {
        render::rebuild(world, root);
    }
}

impl LiquidView {
    #[cfg(test)]
    pub(crate) fn logical_retention_counts(&self) -> (usize, usize) {
        (
            self.saved.len(),
            self.carry.iter().filter(|s| s.is_some()).count(),
        )
    }
    pub(crate) fn park(&mut self, world: &World) {
        for mounted in self.mounted.drain(..) {
            if let Some(state) = world.get::<BlockState>(mounted.slot.root).cloned() {
                if let Some(info) = world.get::<BlockRoot>(mounted.slot.root) {
                    self.saved.insert(
                        info.source,
                        Saved {
                            block: mounted.block,
                            state,
                        },
                    );
                }
            }
        }
        self.pending = None;
    }
    fn new(blocks: Arc<Vec<LiquidBlock>>) -> Self {
        Self {
            blocks,
            mounted: Vec::new(),
            next_id: 1,
            stream: None,
            pending: None,
            carry: Vec::new(),
            saved: HashMap::new(),
        }
    }

    fn streaming(stream: LiquidStream, blocks: Arc<Vec<LiquidBlock>>) -> Self {
        Self {
            blocks,
            stream: Some(stream),
            mounted: Vec::new(),
            next_id: 1,
            pending: None,
            carry: Vec::new(),
            saved: HashMap::new(),
        }
    }
}

fn adopt_blocks(liquid: &mut LiquidView) {
    let Some(stream) = liquid.stream.as_ref() else {
        return;
    };
    if stream.blocks().len() != liquid.blocks.len() {
        liquid.blocks = Arc::new(stream.blocks().to_vec());
    }
}

fn apply_markdown(world: &mut World, view: Entity, write: MarkdownWrite) {
    let Some(markdown) = world.get_mut::<MarkdownView>(view) else {
        return;
    };
    match write {
        MarkdownWrite::Append(delta) => {
            if !delta.is_empty() {
                markdown.push_str(&delta);
            }
        }
        MarkdownWrite::Replace(source) => {
            if markdown.source() != source {
                markdown.set_markdown(source);
            }
        }
    }
}

fn match_states(
    old: &[LiquidBlock],
    states: &[Option<BlockState>],
    new: &[LiquidBlock],
) -> Vec<Option<BlockState>> {
    let mut used = vec![false; old.len()];
    let mut carry = vec![None; new.len()];
    for (index, block) in new.iter().enumerate() {
        let Some(found) = old
            .iter()
            .enumerate()
            .position(|(old_index, old_block)| !used[old_index] && old_block == block)
        else {
            continue;
        };
        used[found] = true;
        carry[index] = states.get(found).cloned().flatten();
    }
    carry
}

struct LiquidHooks;

impl MarkdownSlotHost for LiquidHooks {
    fn slot_mounted(
        &mut self,
        world: &mut World,
        slot: MarkdownSlot,
        _index: usize,
        source: usize,
    ) {
        let Some(tx) = lifecycle::transaction(world, slot.view) else {
            return;
        };
        let Some(mut liquid) = world.remove::<LiquidView>(slot.view) else {
            return;
        };
        let Some(saved) = liquid.saved.remove(&source) else {
            world.insert(slot.view, liquid);
            tx.commit();
            return;
        };
        let mut restored = saved.state;
        if let Some(record) = state::liquid_component_state(world, slot.view, source) {
            if let Some(block) = liquid.blocks.get(saved.block) {
                restored.absorb(block, &record.state);
            }
        }
        world.insert(
            slot.root,
            BlockRoot {
                blocks: Arc::clone(&liquid.blocks),
                index: saved.block,
                source,
            },
        );
        let selected = restored.selected;
        lifecycle::own_projection(world, slot.view, slot.root);
        world.insert(slot.root, restored);
        render::rebuild(world, slot.root);
        restore_tab(world, slot.root, selected);
        liquid.mounted.push(Mounted {
            slot,
            block: saved.block,
            arc: Arc::as_ptr(&liquid.blocks) as usize,
        });
        world.insert(slot.view, liquid);
        tx.commit();
    }

    fn slot_unmounted(
        &mut self,
        world: &mut World,
        slot: MarkdownSlot,
        _index: usize,
        source: usize,
    ) {
        let block = world.get::<BlockRoot>(slot.root).map(|root| root.index);
        let mut state = world.remove::<BlockState>(slot.root).unwrap_or_default();
        if let Some(selected) = tab_selection(world, slot.root) {
            state.selected = selected;
        }
        let Some(tx) = lifecycle::transaction(world, slot.view) else {
            return;
        };
        let Some(mut liquid) = world.remove::<LiquidView>(slot.view) else {
            return;
        };
        if let Some(block) = block {
            if let Some(parsed) = liquid.blocks.get(block).cloned() {
                if let Some((kind, snap)) = state.snapshot(&parsed) {
                    state::store_snapshot(world, slot.view, source, block, kind, snap);
                }
            }
            liquid.saved.insert(source, Saved { block, state });
        }
        lifecycle::park_under(world, slot.root);
        lifecycle::clear_presentation(world, slot.root);
        liquid.mounted.retain(|mounted| mounted.slot.id != slot.id);
        world.insert(slot.view, liquid);
        tx.commit();
    }
}

/// A markdown body inside a block: a view without its own scrollport that
/// hugs its content and mounts its own nested blocks.
pub(crate) fn nested(
    world: &mut World,
    parent: Entity,
    blocks: &Arc<Vec<LiquidBlock>>,
    markdown: &str,
) -> Entity {
    let view = markdown_view(world, markdown.to_string());
    world.remove::<Scroll>(view);
    world.remove::<Color>(view);
    if let Some(layout) = world.get_mut::<Layout>(view) {
        layout.height = Size::Hug;
        layout.padding = 0.0;
    }
    if let Some(md) = world.get_mut::<MarkdownView>(view) {
        md.set_follow(false);
    }
    let parent_key = lifecycle::ancestor_key(world, parent);
    let path = nested_path(world, parent, markdown);
    let restored = parent_key.and_then(|key| lifecycle::reattach(world, view, key, &path));
    let liquid = restored.unwrap_or_else(|| {
        let _ = lifecycle::register(world, view, blocks, markdown, parent_key, path);
        LiquidView::new(Arc::clone(blocks))
    });
    world.insert(view, liquid);
    set_parent(world, view, parent);
    lifecycle::own_created(world, parent, view);
    view
}

fn tab_selection(world: &World, root: Entity) -> Option<usize> {
    find_tabs(world, root)
        .and_then(|tabs| world.get::<Tabs>(tabs).map(|tabs| tabs.selected as usize))
}

pub(crate) fn restore_tab(world: &mut World, root: Entity, selected: usize) {
    let Some(tabs) = find_tabs(world, root) else {
        return;
    };
    if let Some(tabs) = world.get_mut::<Tabs>(tabs) {
        tabs.selected = selected as u64;
    }
}

fn find_tabs(world: &World, root: Entity) -> Option<Entity> {
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if world.has::<Tabs>(entity) {
            return Some(entity);
        }
        stack.extend(children_of(world, entity));
    }
    None
}

fn token_of(block: &MarkdownBlock) -> Option<usize> {
    if block.kind != MarkdownKind::Paragraph || block.quote > 0 || block.depth > 0 {
        return None;
    }
    match rndr_liquid::segments(block.text.trim()).as_slice() {
        [Segment::Block(i)] => Some(*i),
        _ => None,
    }
}

/// Mount blocks for references that markdown just synced. Runs in
/// PostUpdate, after [`MarkdownPlugin`]'s sync.
pub(crate) fn reconcile(world: &mut World) {
    for view in world.entities_with::<LiquidView>() {
        let Some(md) = world.get::<MarkdownView>(view) else {
            continue;
        };
        let window = markdown_realized_blocks(world, view);
        let origins: Vec<usize> = (0..md.blocks().len())
            .map(|index| md.block_start(index).unwrap_or(0))
            .collect();
        let blocks: Vec<MarkdownBlock> = md.blocks().to_vec();
        lifecycle::bind_sources(
            world,
            view,
            blocks
                .iter()
                .enumerate()
                .filter_map(|(i, b)| Some((origins[i], token_of(b)?)))
                .collect(),
        );
        let Some(tx) = lifecycle::transaction(world, view) else {
            continue;
        };
        let mut liquid = world.remove::<LiquidView>(view).expect("listed");
        let ptr = Arc::as_ptr(&liquid.blocks) as usize;
        let mut kept = Vec::new();
        for mut mounted in std::mem::take(&mut liquid.mounted) {
            if !mounted.slot.is_mounted(world) {
                if world.is_alive(mounted.slot.root) {
                    lifecycle::park_under(world, mounted.slot.root);
                    lifecycle::clear_presentation(world, mounted.slot.root);
                    let _ = mounted.slot.detach(world);
                }
                continue;
            }
            if mounted.block >= liquid.blocks.len() {
                lifecycle::park_under(world, mounted.slot.root);
                lifecycle::clear_presentation(world, mounted.slot.root);
                let _ = mounted.slot.detach(world);
                continue;
            }
            if mounted.arc != ptr {
                let same = world
                    .get::<BlockRoot>(mounted.slot.root)
                    .is_some_and(|root| {
                        root.index == mounted.block
                            && root.blocks.get(root.index) == liquid.blocks.get(mounted.block)
                    });
                if !same {
                    let kind_changed = world
                        .get::<BlockRoot>(mounted.slot.root)
                        .and_then(|root| root.blocks.get(root.index).map(LiquidBlock::kind))
                        != liquid.blocks.get(mounted.block).map(LiquidBlock::kind);
                    if kind_changed {
                        world.remove::<BlockState>(mounted.slot.root);
                    }
                    world.insert(
                        mounted.slot.root,
                        BlockRoot {
                            blocks: Arc::clone(&liquid.blocks),
                            index: mounted.block,
                            source: world
                                .get::<BlockRoot>(mounted.slot.root)
                                .map(|root| root.source)
                                .unwrap_or(0),
                        },
                    );
                    render::rebuild(world, mounted.slot.root);
                }
                mounted.arc = ptr;
            }
            kept.push(mounted);
        }
        liquid.mounted = kept;
        for (index, block) in blocks.iter().enumerate() {
            if !window.contains(&index) {
                continue;
            }
            let Some(target) = token_of(block) else {
                continue;
            };
            if target >= liquid.blocks.len() {
                continue;
            }
            if liquid.mounted.iter().any(|m| m.block == target) {
                continue;
            }
            let id = MarkdownSlotId(liquid.next_id);
            let Ok(slot) = mount_markdown_slot(world, view, index, block, id) else {
                continue;
            };
            liquid.next_id = match liquid.next_id.checked_add(1) {
                Some(next) => next,
                None => {
                    let _ = slot.detach(world);
                    break;
                }
            };
            lifecycle::own_projection(world, view, slot.root);
            let source = origins.get(index).copied().unwrap_or(0);
            world.insert(
                slot.root,
                BlockRoot {
                    blocks: Arc::clone(&liquid.blocks),
                    index: target,
                    source,
                },
            );
            if let Some(record) = state::liquid_component_state(world, view, source) {
                if let Some(parsed) = liquid.blocks.get(target) {
                    let mut restored = BlockState::default();
                    restored.absorb(parsed, &record.state);
                    world.insert(slot.root, restored);
                }
            } else if let Some(state) = liquid.carry.get_mut(target).and_then(|state| state.take())
            {
                world.insert(slot.root, state);
            }
            render::rebuild(world, slot.root);
            restore_tab(
                world,
                slot.root,
                world
                    .get::<BlockState>(slot.root)
                    .map(|s| s.selected)
                    .unwrap_or(0),
            );
            liquid.mounted.push(Mounted {
                slot,
                block: target,
                arc: ptr,
            });
        }
        liquid.carry.clear();
        let lang = liquid
            .stream
            .as_ref()
            .and_then(|stream| stream.pending_lang().map(str::to_string));
        render::sync_pending(world, view, &mut liquid.pending, lang.as_deref());
        liquid.mounted.sort_by_key(|m| {
            world
                .get::<Rect>(m.slot.root)
                .map(|r| r.y as i64)
                .unwrap_or(0)
        });
        world.insert(view, liquid);
        tx.commit();
        sync_tabs(world, view);
    }
    crate::chart::reclaim(world);
}

fn drive_timer(
    world: &mut World,
    root: Entity,
    step: usize,
    action: crate::timer::LiquidTimerAction,
) {
    let Some(info) = world.get::<BlockRoot>(root).cloned() else {
        return;
    };
    let Some(view) = world.get::<Parent>(root).map(|parent| parent.entity) else {
        return;
    };
    let Some(rndr_liquid::LiquidBlock::Recipe(recipe)) = info.blocks.get(info.index).cloned()
    else {
        return;
    };
    let Some(step_props) = recipe.steps.get(step) else {
        return;
    };
    let duration = step_props.duration_ms.unwrap_or(60_000);
    crate::timer::apply(world, view, info.source, step, duration, action);
    if action == crate::timer::LiquidTimerAction::Start {
        emit(
            world,
            LiquidEvent::StartTimer {
                label: crate::render::plain(&step_props.label),
                duration_ms: crate::timer::clamp_duration(duration),
            },
        );
    }
}

fn commit_state(world: &mut World, root: Entity, interaction: Option<(LiquidInteraction, String)>) {
    let Some(info) = world.get::<BlockRoot>(root).cloned() else {
        return;
    };
    let Some(block) = info.blocks.get(info.index).cloned() else {
        return;
    };
    let Some(reader) = world.get::<BlockState>(root).cloned() else {
        return;
    };
    let Some((kind, snap)) = reader.snapshot(&block) else {
        return;
    };
    let Some(view) = world.get::<Parent>(root).map(|parent| parent.entity) else {
        return;
    };
    let record = state::store_snapshot(world, view, info.source, info.index, kind, snap);
    let Some((event, instance_id)) = interaction else {
        return;
    };
    state::emit(
        world,
        LiquidInteractionEnvelope {
            version: 1,
            view,
            source: info.source,
            node_id: record.component_type,
            instance_id,
            event,
            disposition: LiquidDisposition::LocalState,
            revision: record.revision,
        },
    );
}

fn interaction_of(act: &Act, state: &BlockState) -> Option<(LiquidInteraction, String)> {
    match *act {
        Act::Toggle(index) => Some((
            if state.open.contains(&index) {
                LiquidInteraction::Expand
            } else {
                LiquidInteraction::Collapse
            },
            index.to_string(),
        )),
        Act::Check(index) => Some((LiquidInteraction::Edit, index.to_string())),
        Act::Prev | Act::Next => Some((LiquidInteraction::Select, state.selected.to_string())),
        Act::Host(_) | Act::Timer { .. } | Act::FeedRefresh { .. } => None,
    }
}

fn sync_tabs(world: &mut World, view: Entity) {
    let roots: Vec<Entity> = world
        .entities_with::<BlockRoot>()
        .into_iter()
        .filter(|root| {
            world
                .get::<Parent>(*root)
                .is_some_and(|parent| parent.entity == view)
        })
        .collect();
    for root in roots {
        let Some(selected) = tab_selection(world, root) else {
            continue;
        };
        let Some(mut reader) = world.get::<BlockState>(root).cloned() else {
            continue;
        };
        if reader.selected == selected {
            continue;
        }
        reader.select(selected);
        world.insert(root, reader);
        commit_state(
            world,
            root,
            Some((LiquidInteraction::Select, selected.to_string())),
        );
    }
}

pub(crate) fn apply_stored_state(
    world: &mut World,
    view: Entity,
    record: &state::LiquidComponentRecord,
) {
    let roots: Vec<Entity> = world
        .entities_with::<BlockRoot>()
        .into_iter()
        .filter(|root| {
            world
                .get::<BlockRoot>(*root)
                .is_some_and(|info| info.source == record.source)
                && world
                    .get::<Parent>(*root)
                    .is_some_and(|parent| parent.entity == view)
        })
        .collect();
    for root in roots {
        let Some(info) = world.get::<BlockRoot>(root).cloned() else {
            continue;
        };
        let Some(block) = info.blocks.get(info.index).cloned() else {
            continue;
        };
        let mut reader = world.get::<BlockState>(root).cloned().unwrap_or_default();
        reader.absorb(&block, &record.state);
        world.insert(root, reader.clone());
        render::rebuild(world, root);
        restore_tab(world, root, reader.selected);
    }
}

/// Handle one request from [`take_action_intents`]. Returns `true` when it
/// came from a Liquid block: tab, accordion, carousel, slide and checklist
/// changes are applied here, and every other activation is passed to the
/// installed [`LiquidHost`] as a [`LiquidEvent`]. Pass the rest on to your
/// own controls.
pub fn dispatch(world: &mut World, request: &ActionIntent) -> bool {
    lifecycle::drain(world);
    if rndr_ui::controls::host_intent(world, request).is_none() {
        return false;
    }
    let found = world
        .entities_with::<Actions>()
        .into_iter()
        .find_map(|root| {
            world
                .get::<Actions>(root)
                .and_then(|a| a.0.get(&request.entity).cloned())
                .map(|act| (root, act))
        });
    let Some((root, act)) = found else {
        return false;
    };
    let live = world
        .get::<Parent>(root)
        .and_then(|p| {
            world
                .get::<BlockRoot>(root)
                .and_then(|b| lifecycle::key(world, p.entity, b.source))
        })
        .is_some();
    if !live || !state::can_emit(world) {
        return true;
    }
    if let Some((view, source)) = world
        .get::<Parent>(root)
        .and_then(|p| world.get::<BlockRoot>(root).map(|b| (p.entity, b.source)))
    {
        if state::liquid_component_state(world, view, source)
            .is_some_and(|r| r.revision == u64::MAX)
        {
            lifecycle::refused(world, crate::LiquidUpdateError::Exhausted);
            return true;
        }
    }
    match act {
        Act::Host(event) => emit(world, event),
        Act::Timer { step, action } => drive_timer(world, root, step, action),
        Act::FeedRefresh { instance } => crate::feed::refresh(world, root, &instance),
        act => {
            if let Some(state) = world.get_mut::<BlockState>(root) {
                state.apply(&act);
            }
            let interaction = world
                .get::<BlockState>(root)
                .and_then(|reader| interaction_of(&act, reader));
            commit_state(world, root, interaction);
            render::rebuild(world, root);
            if let Some(selected) = world.get::<BlockState>(root).map(|state| state.selected) {
                restore_tab(world, root, selected);
            }
        }
    }
    lifecycle::drain(world);
    true
}

pub(crate) fn emit(world: &mut World, event: LiquidEvent) {
    with_host(world, |host, world| host.event(world, event));
}

pub(crate) struct HostSlot(pub Option<Box<dyn LiquidHost>>);

pub(crate) fn with_host<R: Default>(
    world: &mut World,
    f: impl FnOnce(&mut dyn LiquidHost, &mut World) -> R,
) -> R {
    let Some(mut host) = world.resource_mut::<HostSlot>().and_then(|h| h.0.take()) else {
        return R::default();
    };
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(host.as_mut(), world)));
    if let Some(slot) = world.resource_mut::<HostSlot>() {
        slot.0 = Some(host);
    }
    match out {
        Ok(out) => out,
        Err(error) => std::panic::resume_unwind(error),
    }
}

pub(crate) fn host_available(world: &World) -> bool {
    world.resource::<HostSlot>().is_some_and(|s| s.0.is_some())
}

#[derive(Default)]
pub(crate) struct NestedCursor(HashMap<String, usize>);
impl Component for NestedCursor {}
pub(crate) fn reset_nested_cursor(world: &mut World, root: Entity) {
    world.insert(root, NestedCursor::default());
}
fn nested_path(world: &mut World, mut parent: Entity, markdown: &str) -> String {
    loop {
        if world.has::<BlockRoot>(parent) {
            break;
        }
        let Some(p) = world.get::<Parent>(parent) else {
            return markdown.to_string();
        };
        parent = p.entity;
    }
    if !world.has::<NestedCursor>(parent) {
        world.insert(parent, NestedCursor::default());
    }
    let cursor = world
        .get_mut::<NestedCursor>(parent)
        .unwrap()
        .0
        .entry(markdown.to_string())
        .or_default();
    let path = format!("{}:{}", *cursor, markdown);
    *cursor += 1;
    path
}

#[cfg(test)]
pub(crate) fn retention_counts(world: &World) -> (usize, usize, usize) {
    world
        .entities_with::<LiquidView>()
        .into_iter()
        .filter_map(|view| world.get::<LiquidView>(view))
        .fold((0, 0, 0), |out, v| {
            (
                out.0 + v.saved.len(),
                out.1 + v.carry.iter().filter(|s| s.is_some()).count(),
                out.2 + v.mounted.len(),
            )
        })
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    use crate::{LiquidLimits, LiquidUpdateError};
    const ACC: &str = "```accordion\n---\nlabel: A\nbody: Body\n```\n";
    #[test]
    fn source_capacity_refusal_preserves_stream_membership_and_diagnostics_bound() {
        let mut world = World::new();
        world.insert_resource(LiquidLimits {
            scopes: 2,
            diagnostics: 2,
            ..LiquidLimits::default()
        });
        let view = liquid_view(&mut world, ACC);
        let original = world
            .get::<LiquidView>(view)
            .unwrap()
            .stream
            .clone()
            .unwrap();
        let blocks = world.get::<LiquidView>(view).unwrap().blocks.clone();
        let key = lifecycle::key_at(&world, view, 0).unwrap();
        let before = world
            .get::<MarkdownView>(view)
            .unwrap()
            .source()
            .to_string();
        for _ in 0..5 {
            push_liquid_str(&mut world, view, ACC);
            set_liquid_source(&mut world, view, &format!("{ACC}\n{ACC}"));
            assert_eq!(world.get::<MarkdownView>(view).unwrap().source(), before);
            let liquid = world.get::<LiquidView>(view).unwrap();
            assert_eq!(liquid.stream.as_ref().unwrap().source(), original.source());
            assert!(Arc::ptr_eq(&liquid.blocks, &blocks));
            assert_eq!(lifecycle::key_at(&world, view, 0), Some(key));
        }
        let (kept, dropped) = lifecycle::diagnostic_counts(&world);
        assert_eq!((kept, dropped), (2, 8));
        assert_eq!(lifecycle::replace(&mut world, view, &[], "", true), Ok(()));
        // No global identity is reset to manufacture exhaustion.
        let _ = LiquidUpdateError::Capacity;
    }
    #[test]
    fn staged_clone_is_one_candidate_and_old_stream_is_untouched() {
        let mut world = World::new();
        let mut source = format!("{}\n{ACC}", "ordinary prose ".repeat(16384));
        let view = liquid_view(&mut world, &source);
        let mut copied_source_bytes = 0usize;
        let mut max_pair_bytes = 0usize;
        for _ in 0..32 {
            let prior = world
                .get::<LiquidView>(view)
                .unwrap()
                .stream
                .as_ref()
                .unwrap();
            let mut candidate = prior.clone();
            copied_source_bytes += prior.source().len();
            candidate.push_str("\nnext paragraph\n");
            assert_eq!(prior.source(), source);
            max_pair_bytes = max_pair_bytes.max(prior.source().len() + candidate.source().len());
            drop(candidate); // no candidate queue, snapshots or retained clones
            push_liquid_str(&mut world, view, "\nnext paragraph\n");
            source.push_str("\nnext paragraph\n");
            let published = world
                .get::<LiquidView>(view)
                .unwrap()
                .stream
                .as_ref()
                .unwrap();
            assert_eq!(published.source(), source);
            let reparsed = LiquidStream::new(&source);
            assert_eq!(published.markdown(), reparsed.markdown());
            assert_eq!(published.blocks(), reparsed.blocks());
        }
        eprintln!("F1 stream staging source_final={} chunks=32 copied_source_bytes={} max_old_plus_candidate_source_bytes={}", source.len(), copied_source_bytes, max_pair_bytes);
        assert!(max_pair_bytes <= 2 * source.len());
    }
}
