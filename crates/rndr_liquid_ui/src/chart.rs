//! `chart` blocks: an `rndr_chart` painter in a paint surface, with the
//! title, legend and caption as ordinary text around it. The tooltip is an
//! absolute overlay on the plot, and the plot is described by that overlay.
use crate::render::text;
use rndr_chart::{Chart, ChartPainter, ChartScene, ChartTheme};
use rndr_core::*;
use rndr_liquid::chart::ChartProps;
use rndr_paint::{Canvas, CanvasPainter, CanvasRegistry};
use rndr_ui::{Appearance, Typography, UiTheme};
use std::collections::HashMap;

const DEFAULT_HEIGHT: f32 = 260.0;
const SLOT_BASE: u64 = 0x4c51_0000_0000_0000;

/// The plot surface of one chart block.
#[derive(Clone)]
pub(crate) struct LiquidChart {
    chart: Chart,
    theme: ChartTheme,
    tip: Option<Entity>,
    hover: Option<String>,
}

impl Component for LiquidChart {}

/// Paint slot ids per block root. Freed ids are reused, and the painter is
/// removed from [`CanvasRegistry`].
#[derive(Default)]
pub(crate) struct ChartSlots {
    by_root: HashMap<Entity, u64>,
    free: Vec<u64>,
    next: u64,
}

struct Plot(Entity);

impl CanvasPainter for Plot {
    fn paint(&self, world: &World, canvas: &mut Canvas) {
        let (Some(c), Some(rect)) = (world.get::<LiquidChart>(self.0), world.get::<Rect>(self.0))
        else {
            return;
        };
        let mut painter = ChartPainter::new(c.chart.clone(), c.theme.clone(), *rect);
        painter.hover = c.hover.clone();
        painter.paint(world, canvas);
    }
}

fn theme(world: &World) -> ChartTheme {
    match world.resource::<UiTheme>().map(|t| t.appearance) {
        Some(Appearance::Light) => ChartTheme::light(),
        _ => ChartTheme::dark(),
    }
}

fn height(props: &ChartProps) -> f32 {
    props
        .height
        .as_deref()
        .unwrap_or_default()
        .trim()
        .trim_end_matches("px")
        .parse::<f32>()
        .ok()
        .filter(|h| h.is_finite() && (80.0..=1200.0).contains(h))
        .unwrap_or(DEFAULT_HEIGHT)
}

pub(crate) fn mount(world: &mut World, root: Entity, parent: Entity, props: &ChartProps) {
    let line = |world: &mut World, value: &str, role| {
        let value = crate::plain(value);
        if !value.is_empty() {
            let _ = text(world, parent, &value, role);
        }
    };
    let chart = match Chart::from_liquid(props) {
        Ok(chart) => chart,
        Err(err) => {
            release(world, root);
            line(
                world,
                props.title.as_deref().unwrap_or_default(),
                Typography::Heading,
            );
            line(
                world,
                &format!("Chart unavailable: {err}"),
                Typography::Secondary,
            );
            return;
        }
    };
    line(world, &chart.title, Typography::Heading);
    line(world, &chart.description, Typography::Secondary);
    let theme = theme(world);
    let slot = {
        if !world.has_resource::<ChartSlots>() {
            world.insert_resource(ChartSlots::default());
        }
        let slots = world.resource_mut::<ChartSlots>().expect("inserted");
        if let Some(id) = slots.by_root.get(&root).copied() {
            id
        } else {
            let id = slots.free.pop().unwrap_or_else(|| {
                let id = SLOT_BASE + slots.next;
                slots.next += 1;
                id
            });
            slots.by_root.insert(root, id);
            id
        }
    };
    let interactive = chart.interactive && chart.tooltip;
    let legend = ChartScene::new(&chart, &theme, 400.0, height(props)).legend;
    let caption = [chart.trend.as_str(), chart.caption.as_str()]
        .iter()
        .filter(|s| !s.trim().is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ");
    let plot = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(height(props)),
            ..Layout::column()
        },
        SurfaceSlot { id: slot },
        AccessMark(AccessRole::Group),
        Semantics {
            name: Some(if chart.title.trim().is_empty() {
                "Chart".to_string()
            } else {
                chart.title.clone()
            }),
            ..Semantics::default()
        },
    ));
    set_parent(world, plot, parent);
    if !legend.is_empty() {
        let row = world.spawn((Layout {
            gap: 12.0,
            width: Size::Fill,
            ..Layout::row()
        },));
        set_parent(world, row, parent);
        for entry in &legend {
            if let Ok(e) = text(world, row, &format!("■ {}", entry.label), Typography::Label) {
                if let Some(label) = world.get_mut::<Label>(e) {
                    label.color = Some(entry.color);
                    label.overflow = TextOverflow::Visible;
                }
                if let Some(layout) = world.get_mut::<Layout>(e) {
                    layout.width = Size::Hug;
                }
            }
        }
    }
    let tip = interactive.then(|| overlay_tip(world, plot)).flatten();
    line(world, &caption, Typography::Secondary);
    world.insert(
        plot,
        LiquidChart {
            chart,
            theme,
            tip,
            hover: None,
        },
    );
    if !world.has_resource::<CanvasRegistry>() {
        world.insert_resource(CanvasRegistry::default());
    }
    world
        .resource_mut::<CanvasRegistry>()
        .expect("inserted")
        .insert(slot, Plot(plot));
    if interactive {
        world.insert_resource(MotionWatch { active: true });
    }
}

/// Forget the paint slot for `root` and return its id to the free list.
pub(crate) fn release(world: &mut World, root: Entity) {
    if !world.has_resource::<ChartSlots>() {
        return;
    }
    let Some(id) = world
        .resource_mut::<ChartSlots>()
        .and_then(|slots| slots.by_root.remove(&root))
    else {
        return;
    };
    if let Some(registry) = world.resource_mut::<CanvasRegistry>() {
        registry.remove(id);
    }
    if let Some(slots) = world.resource_mut::<ChartSlots>() {
        slots.free.push(id);
    }
}

/// Drop slots whose block root has been despawned.
pub(crate) fn reclaim(world: &mut World) {
    let roots: Vec<Entity> = world
        .resource::<ChartSlots>()
        .map(|slots| slots.by_root.keys().copied().collect())
        .unwrap_or_default();
    for root in roots {
        if !world.is_alive(root) {
            release(world, root);
        }
    }
}

fn overlay_tip(world: &mut World, plot: Entity) -> Option<Entity> {
    let tip = text(world, plot, "", Typography::Secondary).ok()?;
    if let Some(layout) = world.get_mut::<Layout>(tip) {
        layout.width = Size::Hug;
        layout.height = Size::Hug;
    }
    world.insert(tip, Hidden);
    world.insert(
        tip,
        Absolute {
            left: Some(8.0),
            top: Some(8.0),
            ..Absolute::default()
        },
    );
    world.insert(tip, AccessMark(AccessRole::Tooltip));
    Some(tip)
}

/// Track the pointer over chart plots: the hovered series is highlighted
/// and its tooltip floats over the plot, which is described by that tooltip.
pub(crate) fn hover(world: &mut World) {
    reclaim(world);
    let plots = world.entities_with::<LiquidChart>();
    if plots.is_empty() {
        return;
    }
    let pointer = world.resource::<Input>().map(|i| i.pointer.position);
    for plot in plots {
        let (rect, tip, chart, theme) = {
            let (Some(c), Some(rect)) = (
                world.get::<LiquidChart>(plot),
                world.get::<Rect>(plot).copied(),
            ) else {
                continue;
            };
            let Some(tip) = c.tip else { continue };
            (rect, tip, c.chart.clone(), c.theme.clone())
        };
        let hit = pointer.and_then(|[x, y]| {
            let scene = ChartScene::new(&chart, &theme, rect.w, rect.h);
            let inside = x >= rect.x && y >= rect.y && x < rect.x + rect.w && y < rect.y + rect.h;
            inside
                .then(|| scene.hit([x - rect.x, y - rect.y]).cloned())
                .flatten()
        });
        let key = hit.as_ref().and_then(|h| h.key.clone());
        let line = hit.as_ref().map(|h| {
            let rows: Vec<String> = h
                .lines
                .iter()
                .map(|l| format!("{}: {}", l.label, l.value))
                .collect();
            if rows.is_empty() {
                h.title.clone()
            } else {
                format!("{} — {}", h.title, rows.join(", "))
            }
        });
        if let Some(c) = world.get_mut::<LiquidChart>(plot) {
            c.hover = key;
        }
        let Some(line) = line else {
            world.insert(tip, Hidden);
            world.remove::<DescribedBy>(plot);
            if let Some(label) = world.get_mut::<Label>(tip) {
                if !label.text.is_empty() {
                    label.text.clear();
                }
            }
            continue;
        };
        world.remove::<Hidden>(tip);
        if let Some(label) = world.get_mut::<Label>(tip) {
            if label.text != line {
                label.text = line;
            }
        }
        let (left, top) = pointer
            .map(|[x, y]| {
                (
                    (x - rect.x + 12.0).clamp(0.0, rect.w.max(0.0)),
                    (y - rect.y + 12.0).clamp(0.0, rect.h.max(0.0)),
                )
            })
            .unwrap_or((8.0, 8.0));
        world.insert(
            tip,
            Absolute {
                left: Some(left),
                top: Some(top),
                ..Absolute::default()
            },
        );
        world.insert(plot, DescribedBy(vec![tip]));
    }
}

#[cfg(test)]
pub(crate) fn count(world: &World) -> usize {
    world
        .resource::<ChartSlots>()
        .map_or(0, |r| r.by_root.len())
}

fn set_parent(world: &mut World, child: Entity, parent: Entity) {
    rndr_core::set_parent(world, child, parent);
    crate::lifecycle::own_created(world, parent, child);
}
