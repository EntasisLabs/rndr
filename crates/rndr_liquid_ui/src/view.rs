//! Markdown views whose Liquid block references are mounted as native blocks.
use crate::render::{self, Act, Actions, BlockState};
use crate::{LiquidEvent, LiquidHost};
use rndr_core::*;
use rndr_liquid::{LiquidBlock, LiquidStream, MarkdownWrite, Segment};
use std::sync::Arc;

/// Liquid state of one [`MarkdownView`].
///
/// [`liquid_view`] spawns both. The view shows the settled markdown from a
/// [`LiquidStream`]; each paragraph that is exactly one block reference is
/// replaced by that block, rendered with `rndr_ui`, after the markdown sync
/// of the same frame. An unclosed Liquid fence is a pending skeleton, not a
/// code block.
pub struct LiquidView {
    blocks: Arc<Vec<LiquidBlock>>,
    mounted: Vec<Mounted>,
    next_id: u64,
    stream: Option<LiquidStream>,
    pending: Option<Entity>,
    carry: Vec<Option<BlockState>>,
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
    world.insert(view, LiquidView::streaming(stream, blocks));
    view
}

/// Replace the source of a Liquid view.
///
/// A source that extends the current one is an append: finished blocks stay
/// mounted and keep their state. Any other replacement keeps a mounted block
/// when its parsed source is unchanged and rebuilds the rest.
pub fn set_liquid_source(world: &mut World, view: Entity, source: &str) {
    let Some(mut liquid) = world.remove::<LiquidView>(view) else {
        return;
    };
    let normalized = source.replace("\r\n", "\n");
    if let Some(stream) = liquid.stream.as_mut() {
        if let Some(rest) = normalized.strip_prefix(stream.source()) {
            let write = stream.push_str(rest);
            adopt_blocks(&mut liquid);
            world.insert(view, liquid);
            apply_markdown(world, view, write);
            return;
        }
    }
    let old_blocks = Arc::clone(&liquid.blocks);
    let mut states = vec![None; old_blocks.len()];
    for mounted in &liquid.mounted {
        if mounted.block >= states.len() {
            continue;
        }
        if let Some(state) = world.get::<BlockState>(mounted.slot.root).cloned() {
            states[mounted.block] = Some(state);
        }
    }
    let stream = LiquidStream::new(&normalized);
    liquid.carry = match_states(&old_blocks, &states, stream.blocks());
    liquid.blocks = Arc::new(stream.blocks().to_vec());
    let markdown = stream.markdown().to_string();
    liquid.stream = Some(stream);
    world.insert(view, liquid);
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
    let Some(mut liquid) = world.remove::<LiquidView>(view) else {
        return;
    };
    let Some(stream) = liquid.stream.as_mut() else {
        world.insert(view, liquid);
        return;
    };
    let write = stream.push_str(chunk);
    adopt_blocks(&mut liquid);
    world.insert(view, liquid);
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
    fn new(blocks: Arc<Vec<LiquidBlock>>) -> Self {
        Self {
            blocks,
            mounted: Vec::new(),
            next_id: 1,
            stream: None,
            pending: None,
            carry: Vec::new(),
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
        let blocks: Vec<MarkdownBlock> = md.blocks().to_vec();
        let mut liquid = world.remove::<LiquidView>(view).expect("listed");
        let ptr = Arc::as_ptr(&liquid.blocks) as usize;
        let mut kept = Vec::new();
        for mut mounted in std::mem::take(&mut liquid.mounted) {
            if !mounted.slot.is_mounted(world) {
                if world.is_alive(mounted.slot.root) {
                    let _ = mounted.slot.detach(world);
                }
                continue;
            }
            if mounted.block >= liquid.blocks.len() {
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
            if let Some(state) = liquid.carry.get_mut(target).and_then(|state| state.take()) {
                world.insert(slot.root, state);
            }
            render::rebuild(world, slot.root);
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
    }
    crate::chart::reclaim(world);
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
