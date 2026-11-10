//! Markdown views whose Liquid block references are mounted as native blocks.
use crate::render::{self, Act, Actions, BlockState};
use crate::{LiquidEvent, LiquidHost};
use rndr_core::*;
use rndr_liquid::{LiquidBlock, LiquidDocument, Segment};
use std::collections::HashMap;
use std::sync::Arc;

/// Liquid state of one [`MarkdownView`].
///
/// [`liquid_view`] spawns both. The view shows [`LiquidDocument::markdown`];
/// each paragraph that is exactly one block reference is replaced by that
/// block, rendered with `rndr_ui`, after the markdown sync of the same frame.
pub struct LiquidView {
    blocks: Arc<Vec<LiquidBlock>>,
    mounted: Vec<Mounted>,
    next_id: u64,
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
}

/// Root of one rendered block: which block it shows and the view it is in.
#[derive(Clone)]
pub(crate) struct BlockRoot {
    pub blocks: Arc<Vec<LiquidBlock>>,
    pub index: usize,
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
    let doc = LiquidDocument::parse(source);
    let view = markdown_view(world, doc.markdown);
    if let Some(markdown) = world.get_mut::<MarkdownView>(view) {
        markdown.set_recycle_host_slots(true);
    }
    set_markdown_slot_host(world, view, LiquidHooks);
    world.insert(view, LiquidView::new(Arc::new(doc.blocks)));
    view
}

/// Replace the whole source of a Liquid view. Every mounted block is rebuilt
/// on the next frame; block state such as the open tab is reset.
pub fn set_liquid_source(world: &mut World, view: Entity, source: &str) {
    let doc = LiquidDocument::parse(source);
    detach_all(world, view);
    if let Some(liquid) = world.get_mut::<LiquidView>(view) {
        liquid.blocks = Arc::new(doc.blocks);
    }
    if let Some(md) = world.get_mut::<MarkdownView>(view) {
        md.set_markdown(doc.markdown);
    }
}

/// Re-render every mounted block of every Liquid view, asking the host again
/// for images, feeds and tiles. Block state is kept.
pub fn refresh(world: &mut World) {
    for root in world.entities_with::<BlockRoot>() {
        render::rebuild(world, root);
    }
}

impl LiquidView {
    fn new(blocks: Arc<Vec<LiquidBlock>>) -> Self {
        Self {
            blocks,
            mounted: Vec::new(),
            next_id: 1,
            saved: HashMap::new(),
        }
    }
}

fn detach_all(world: &mut World, view: Entity) {
    if let Some(liquid) = world.get_mut::<LiquidView>(view) {
        liquid.mounted.clear();
        liquid.saved.clear();
    }
    clear_markdown_slots(world, view);
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
        let Some(mut liquid) = world.remove::<LiquidView>(slot.view) else {
            return;
        };
        let Some(saved) = liquid.saved.remove(&source) else {
            world.insert(slot.view, liquid);
            return;
        };
        world.insert(
            slot.root,
            BlockRoot {
                blocks: Arc::clone(&liquid.blocks),
                index: saved.block,
            },
        );
        let selected = saved.state.selected;
        world.insert(slot.root, saved.state);
        render::rebuild(world, slot.root);
        restore_tab(world, slot.root, selected);
        liquid.mounted.push(Mounted {
            slot,
            block: saved.block,
        });
        world.insert(slot.view, liquid);
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
        let Some(mut liquid) = world.remove::<LiquidView>(slot.view) else {
            return;
        };
        if let Some(block) = block {
            liquid.saved.insert(source, Saved { block, state });
        }
        liquid.mounted.retain(|mounted| mounted.slot.id != slot.id);
        world.insert(slot.view, liquid);
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
    world.insert(view, LiquidView::new(Arc::clone(blocks)));
    set_parent(world, view, parent);
    view
}

fn tab_selection(world: &World, root: Entity) -> Option<usize> {
    find_tabs(world, root)
        .and_then(|tabs| world.get::<Tabs>(tabs).map(|tabs| tabs.selected as usize))
}

fn restore_tab(world: &mut World, root: Entity, selected: usize) {
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
        let blocks: Vec<MarkdownBlock> = md.blocks().to_vec();
        let mut liquid = world.remove::<LiquidView>(view).expect("listed");
        liquid.mounted.retain(|m| m.slot.is_mounted(world));
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
            liquid.next_id += 1;
            world.insert(
                slot.root,
                BlockRoot {
                    blocks: Arc::clone(&liquid.blocks),
                    index: target,
                },
            );
            render::rebuild(world, slot.root);
            liquid.mounted.push(Mounted {
                slot,
                block: target,
            });
        }
        liquid.mounted.sort_by_key(|m| {
            world
                .get::<Rect>(m.slot.root)
                .map(|r| r.y as i64)
                .unwrap_or(0)
        });
        world.insert(view, liquid);
    }
}

/// Handle one request from [`take_action_intents`]. Returns `true` when it
/// came from a Liquid block: tab, accordion, carousel, slide and checklist
/// changes are applied here, and every other activation is passed to the
/// installed [`LiquidHost`] as a [`LiquidEvent`]. Pass the rest on to your
/// own controls.
pub fn dispatch(world: &mut World, request: &ActionIntent) -> bool {
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
    match act {
        Act::Host(event) => emit(world, event),
        act => {
            if let Some(state) = world.get_mut::<BlockState>(root) {
                state.apply(&act);
            }
            render::rebuild(world, root);
        }
    }
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
    let out = f(host.as_mut(), world);
    if let Some(slot) = world.resource_mut::<HostSlot>() {
        slot.0 = Some(host);
    }
    out
}
