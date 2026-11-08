//! Markdown views whose Liquid block references are mounted as native blocks.
use crate::render::{self, Act, Actions, BlockState};
use crate::{LiquidEvent, LiquidHost};
use rndr_core::*;
use rndr_liquid::{LiquidBlock, LiquidDocument, Segment};
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
        }
    }
}

fn detach_all(world: &mut World, view: Entity) {
    let Some(liquid) = world.get_mut::<LiquidView>(view) else {
        return;
    };
    let mounted = std::mem::take(&mut liquid.mounted);
    for m in mounted {
        let _ = m.slot.detach(world);
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
        liquid.mounted.retain(|m| m.slot.is_mounted(world));
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
