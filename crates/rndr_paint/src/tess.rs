//! CPU tessellation of [`Path`]s into triangles.
//!
//! Fills and strokes are flattened with [lyon](https://crates.io/crates/lyon).
//! Dashes are applied first, by walking the flattened centerline. The resulting
//! [`Mesh`] is in the path's local space.
//!
//! # Anti-aliasing
//!
//! Edges are not covered here. The renderer multisamples the canvas and layer
//! targets at 4x and resolves them before a blur, an opacity composite, or the
//! UI pass. UI quads stay on the signed-distance pipeline at one sample. A
//! fringe mesh was the other option: it needs its own stroke rasterizer to
//! avoid halos on joins, caps, and dashes, and it diverges from the fill rule
//! on self-intersections. Multisampling the canvas target only keeps one
//! tessellation, and the swapchain itself stays single-sample. A device that
//! cannot resolve 4x falls back to one sample and still draws the same mesh.
//!
//! # Cache
//!
//! [`MeshCache`] keys a mesh by the path, the fill or stroke, and the
//! [`zoom_bucket`](crate::zoom_bucket) of the device scale. The mesh is built
//! at [`bucket_scale`](crate::bucket_scale), so a pan, or a zoom that stays in
//! the bucket, reuses it. A hairline's local width is part of that key because
//! it depends on the bucket.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use lyon::path::math::point;
use lyon::path::Path as LyonPath;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule as LyonRule, FillTessellator, FillVertex, LineCap,
    LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};

use crate::affine::{bucket_scale, zoom_bucket};
use crate::path::{Cap, FillRule, Join, Path, Stroke, Verb};

/// Triangle mesh in local units. Indices are groups of three.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    /// Vertex positions.
    pub vertices: Vec<[f32; 2]>,
    /// Triangle indices into [`Self::vertices`].
    pub indices: Vec<u32>,
}

impl Mesh {
    /// No triangles.
    pub fn is_empty(&self) -> bool {
        self.indices.len() < 3
    }

    /// Absolute area of the triangles. Overlapping triangles count twice.
    pub fn area(&self) -> f32 {
        let mut area = 0.0f32;
        for tri in self.indices.as_chunks::<3>().0 {
            let a = self.vertices[tri[0] as usize];
            let b = self.vertices[tri[1] as usize];
            let c = self.vertices[tri[2] as usize];
            area += ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5;
        }
        area
    }

    /// `true` when `p` lies in any triangle. Lyon fills do not overlap, so this
    /// matches the painted interior for a fill mesh.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        self.indices.as_chunks::<3>().0.iter().any(|tri| {
            let a = self.vertices[tri[0] as usize];
            let b = self.vertices[tri[1] as usize];
            let c = self.vertices[tri[2] as usize];
            point_in_triangle(p, a, b, c)
        })
    }
}

/// Fill `path` at the representative scale of `scale`'s zoom bucket.
pub fn tessellate_fill(path: &Path, rule: FillRule, scale: f32) -> Mesh {
    if path.is_empty() {
        return Mesh::default();
    }
    let scale = representative(scale);
    let tolerance = tolerance_for(scale);
    let lyon_path = to_lyon(path);
    fill_lyon(&lyon_path, rule, tolerance)
}

/// Stroke `path`. Hairline widths use `scale`. Dashes split the centerline first.
pub fn tessellate_stroke(path: &Path, stroke: &Stroke, scale: f32) -> Mesh {
    if path.is_empty() {
        return Mesh::default();
    }
    let scale = representative(scale);
    let width = stroke.local_width(scale);
    if width <= 0.0 {
        return Mesh::default();
    }
    let tolerance = tolerance_for(scale);
    if stroke.dash.iter().any(|len| *len > 0.0) {
        let dashed = dash_path(path, &stroke.dash, stroke.dash_offset, tolerance);
        let mut mesh = Mesh::default();
        for sub in dashed {
            let lyon_path = to_lyon(&sub);
            append_mesh(
                &mut mesh,
                &stroke_lyon(&lyon_path, stroke, width, tolerance),
            );
        }
        return mesh;
    }
    let lyon_path = to_lyon(path);
    stroke_lyon(&lyon_path, stroke, width, tolerance)
}

/// LRU of fill and stroke meshes. `limit` is the number of entries kept.
#[derive(Debug)]
pub struct MeshCache {
    fills: HashMap<u64, Arc<Mesh>>,
    strokes: HashMap<u64, Arc<Mesh>>,
    order: VecDeque<u64>,
    limit: usize,
}

impl Default for MeshCache {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl MeshCache {
    /// Cache at most `limit` meshes. Zero keeps nothing and tessellates every call.
    pub fn new(limit: usize) -> Self {
        Self {
            fills: HashMap::new(),
            strokes: HashMap::new(),
            order: VecDeque::new(),
            limit,
        }
    }

    /// Fill mesh for `path` at `scale`, inserting it if the bucket is new.
    pub fn fill(&mut self, path: &Path, rule: FillRule, scale: f32) -> Arc<Mesh> {
        let key = fill_key(path, rule, scale);
        if let Some(hit) = self.fills.get(&key) {
            return Arc::clone(hit);
        }
        let mesh = Arc::new(tessellate_fill(path, rule, scale));
        self.insert(key, Arc::clone(&mesh), true);
        mesh
    }

    /// Stroke mesh for `path` at `scale`, inserting it if the bucket is new.
    pub fn stroke(&mut self, path: &Path, stroke: &Stroke, scale: f32) -> Arc<Mesh> {
        let key = stroke_key(path, stroke, scale);
        if let Some(hit) = self.strokes.get(&key) {
            return Arc::clone(hit);
        }
        let mesh = Arc::new(tessellate_stroke(path, stroke, scale));
        self.insert(key, Arc::clone(&mesh), false);
        mesh
    }

    /// Stored meshes.
    pub fn len(&self) -> usize {
        self.fills.len() + self.strokes.len()
    }

    /// No stored meshes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn insert(&mut self, key: u64, mesh: Arc<Mesh>, fill: bool) {
        if self.limit == 0 {
            return;
        }
        if fill {
            self.fills.insert(key, mesh);
        } else {
            self.strokes.insert(key, mesh);
        }
        self.order.push_back(key);
        while self.len() > self.limit {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            self.fills.remove(&old);
            self.strokes.remove(&old);
        }
    }
}

fn representative(scale: f32) -> f32 {
    let scale = if scale.is_finite() && scale > 1.0e-6 {
        scale
    } else {
        1.0
    };
    bucket_scale(zoom_bucket(scale)).max(1.0e-3)
}

fn tolerance_for(scale: f32) -> f32 {
    (0.25 / scale).clamp(0.01, 4.0)
}

fn fill_key(path: &Path, rule: FillRule, scale: f32) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    1u8.hash(&mut hasher);
    hash_path(path, &mut hasher);
    (rule as u8).hash(&mut hasher);
    zoom_bucket(scale).hash(&mut hasher);
    hasher.finish()
}

fn stroke_key(path: &Path, stroke: &Stroke, scale: f32) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    2u8.hash(&mut hasher);
    hash_path(path, &mut hasher);
    stroke
        .local_width(representative(scale))
        .to_bits()
        .hash(&mut hasher);
    (stroke.cap as u8).hash(&mut hasher);
    (stroke.join as u8).hash(&mut hasher);
    stroke.miter_limit.to_bits().hash(&mut hasher);
    stroke.dash_offset.to_bits().hash(&mut hasher);
    for dash in &stroke.dash {
        dash.to_bits().hash(&mut hasher);
    }
    zoom_bucket(scale).hash(&mut hasher);
    hasher.finish()
}

fn hash_path(path: &Path, hasher: &mut impl Hasher) {
    for (verb, at) in path.verbs() {
        (verb as u8).hash(hasher);
        let n = match verb {
            Verb::Move | Verb::Line => 1,
            Verb::Quad => 2,
            Verb::Cubic => 3,
            Verb::Close => 0,
        };
        for index in 0..n {
            let p = path.point(at + index);
            p[0].to_bits().hash(hasher);
            p[1].to_bits().hash(hasher);
        }
    }
}

fn to_lyon(path: &Path) -> LyonPath {
    let mut builder = LyonPath::builder();
    let mut open = false;
    for (verb, at) in path.verbs() {
        match verb {
            Verb::Move => {
                if open {
                    builder.end(false);
                }
                builder.begin(point(path.point(at)[0], path.point(at)[1]));
                open = true;
            }
            Verb::Line => {
                builder.line_to(point(path.point(at)[0], path.point(at)[1]));
            }
            Verb::Quad => {
                builder.quadratic_bezier_to(
                    point(path.point(at)[0], path.point(at)[1]),
                    point(path.point(at + 1)[0], path.point(at + 1)[1]),
                );
            }
            Verb::Cubic => {
                builder.cubic_bezier_to(
                    point(path.point(at)[0], path.point(at)[1]),
                    point(path.point(at + 1)[0], path.point(at + 1)[1]),
                    point(path.point(at + 2)[0], path.point(at + 2)[1]),
                );
            }
            Verb::Close => {
                if open {
                    builder.end(true);
                    open = false;
                }
            }
        }
    }
    if open {
        builder.end(false);
    }
    builder.build()
}

fn fill_lyon(path: &LyonPath, rule: FillRule, tolerance: f32) -> Mesh {
    let lyon_rule = match rule {
        FillRule::NonZero => LyonRule::NonZero,
        FillRule::EvenOdd => LyonRule::EvenOdd,
    };
    let options = FillOptions::tolerance(tolerance).with_fill_rule(lyon_rule);
    let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut tess = FillTessellator::new();
    let mut builder = BuffersBuilder::new(&mut buffers, |vertex: FillVertex| {
        let p = vertex.position();
        [p.x, p.y]
    });
    let _ = tess.tessellate_path(path, &options, &mut builder);
    Mesh {
        vertices: buffers.vertices,
        indices: buffers.indices,
    }
}

fn stroke_lyon(path: &LyonPath, stroke: &Stroke, width: f32, tolerance: f32) -> Mesh {
    let cap = match stroke.cap {
        Cap::Butt => LineCap::Butt,
        Cap::Square => LineCap::Square,
        Cap::Round => LineCap::Round,
    };
    let join = match stroke.join {
        Join::Miter => LineJoin::Miter,
        Join::Bevel => LineJoin::Bevel,
        Join::Round => LineJoin::Round,
    };
    let miter = stroke.miter_limit.max(1.0);
    let options = StrokeOptions::tolerance(tolerance)
        .with_line_width(width)
        .with_line_cap(cap)
        .with_line_join(join)
        .with_miter_limit(miter);
    let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut tess = StrokeTessellator::new();
    let mut builder = BuffersBuilder::new(&mut buffers, |vertex: StrokeVertex| {
        let p = vertex.position();
        [p.x, p.y]
    });
    let _ = tess.tessellate_path(path, &options, &mut builder);
    Mesh {
        vertices: buffers.vertices,
        indices: buffers.indices,
    }
}

fn append_mesh(into: &mut Mesh, extra: &Mesh) {
    let base = into.vertices.len() as u32;
    into.vertices.extend_from_slice(&extra.vertices);
    into.indices
        .extend(extra.indices.iter().map(|index| index + base));
}

struct Contour {
    points: Vec<[f32; 2]>,
    closed: bool,
}

fn flatten_path(path: &Path, tolerance: f32) -> Vec<Contour> {
    let mut contours = Vec::new();
    let mut current: Option<Contour> = None;
    let mut start = [0.0, 0.0];
    let mut cursor = [0.0, 0.0];
    let mut pen = false;
    for (verb, at) in path.verbs() {
        match verb {
            Verb::Move => {
                if let Some(contour) = current.take() {
                    if contour.points.len() >= 2 {
                        contours.push(contour);
                    }
                }
                start = path.point(at);
                cursor = start;
                pen = true;
                current = Some(Contour {
                    points: vec![start],
                    closed: false,
                });
            }
            Verb::Line => {
                let p = path.point(at);
                if let Some(contour) = current.as_mut() {
                    contour.points.push(p);
                }
                cursor = p;
            }
            Verb::Quad => {
                let ctrl = path.point(at);
                let end = path.point(at + 1);
                if let Some(contour) = current.as_mut() {
                    flatten_quad(&mut contour.points, cursor, ctrl, end, tolerance, 0);
                }
                cursor = end;
            }
            Verb::Cubic => {
                let c1 = path.point(at);
                let c2 = path.point(at + 1);
                let end = path.point(at + 2);
                if let Some(contour) = current.as_mut() {
                    flatten_cubic(&mut contour.points, cursor, c1, c2, end, tolerance, 0);
                }
                cursor = end;
            }
            Verb::Close => {
                if let Some(contour) = current.as_mut() {
                    contour.closed = true;
                    if contour.points.last().copied() != Some(start) {
                        contour.points.push(start);
                    }
                }
                cursor = start;
                pen = false;
            }
        }
        let _ = pen;
    }
    if let Some(contour) = current {
        if contour.points.len() >= 2 {
            contours.push(contour);
        }
    }
    contours
}

fn flatten_quad(
    out: &mut Vec<[f32; 2]>,
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    tolerance: f32,
    depth: u32,
) {
    if depth > 16 || quad_flat(p0, p1, p2, tolerance) {
        out.push(p2);
        return;
    }
    let a = midpoint(p0, p1);
    let b = midpoint(p1, p2);
    let m = midpoint(a, b);
    flatten_quad(out, p0, a, m, tolerance, depth + 1);
    flatten_quad(out, m, b, p2, tolerance, depth + 1);
}

fn flatten_cubic(
    out: &mut Vec<[f32; 2]>,
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    p3: [f32; 2],
    tolerance: f32,
    depth: u32,
) {
    if depth > 16 || cubic_flat(p0, p1, p2, p3, tolerance) {
        out.push(p3);
        return;
    }
    let a = midpoint(p0, p1);
    let b = midpoint(p1, p2);
    let c = midpoint(p2, p3);
    let d = midpoint(a, b);
    let e = midpoint(b, c);
    let m = midpoint(d, e);
    flatten_cubic(out, p0, a, d, m, tolerance, depth + 1);
    flatten_cubic(out, m, e, c, p3, tolerance, depth + 1);
}

fn quad_flat(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], tolerance: f32) -> bool {
    let dx = p2[0] - p0[0];
    let dy = p2[1] - p0[1];
    let dist = ((p1[0] - p0[0]) * dy - (p1[1] - p0[1]) * dx).abs();
    dist <= tolerance * (dx * dx + dy * dy).sqrt().max(1.0e-6) * 2.0
}

fn cubic_flat(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], tolerance: f32) -> bool {
    quad_flat(p0, p1, p3, tolerance) && quad_flat(p0, p2, p3, tolerance)
}

fn midpoint(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

fn dash_path(path: &Path, dash: &[f32], offset: f32, tolerance: f32) -> Vec<Path> {
    let mut pattern: Vec<f32> = dash.iter().copied().filter(|len| *len > 0.0).collect();
    if pattern.len() % 2 == 1 {
        let once = pattern.clone();
        pattern.extend(once);
    }
    let sum: f32 = pattern.iter().sum();
    if sum <= 0.0 {
        return vec![path.clone()];
    }
    let mut phase = offset.rem_euclid(sum);
    let mut index = 0usize;
    while phase > pattern[index] {
        phase -= pattern[index];
        index = (index + 1) % pattern.len();
    }
    let mut on = index.is_multiple_of(2);
    let mut remain = pattern[index] - phase;
    let mut out = Vec::new();
    for contour in flatten_path(path, tolerance) {
        let segments = contour.points.len().saturating_sub(1);
        let mut pen: Option<[f32; 2]> = None;
        let mut current: Vec<[f32; 2]> = Vec::new();
        for seg in 0..segments {
            let mut a = contour.points[seg];
            let b = contour.points[seg + 1];
            let mut left = distance(a, b);
            if left <= 1.0e-6 {
                continue;
            }
            while left > 1.0e-6 {
                let step = remain.min(left);
                let t = step / distance(a, b).max(1.0e-6);
                let next = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                if on {
                    if current.is_empty() {
                        current.push(a);
                    }
                    current.push(next);
                    pen = Some(next);
                } else if current.len() >= 2 {
                    out.push(polyline_path(&current));
                    current.clear();
                    pen = None;
                }
                a = next;
                left -= step;
                remain -= step;
                if remain <= 1.0e-6 {
                    index = (index + 1) % pattern.len();
                    on = index.is_multiple_of(2);
                    remain = pattern[index];
                    if !on && current.len() >= 2 {
                        out.push(polyline_path(&current));
                        current.clear();
                        pen = None;
                    }
                    if on {
                        if let Some(at) = pen {
                            current.push(at);
                        }
                    }
                }
            }
        }
        if current.len() >= 2 {
            out.push(polyline_path(&current));
        }
    }
    out
}

fn polyline_path(points: &[[f32; 2]]) -> Path {
    let mut path = Path::builder();
    path.move_to(points[0]);
    for point in &points[1..] {
        path.line_to(*point);
    }
    path.build()
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    (dx * dx + dy * dy).sqrt()
}

fn point_in_triangle(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let v0x = c[0] - a[0];
    let v0y = c[1] - a[1];
    let v1x = b[0] - a[0];
    let v1y = b[1] - a[1];
    let v2x = p[0] - a[0];
    let v2y = p[1] - a[1];
    let dot00 = v0x * v0x + v0y * v0y;
    let dot01 = v0x * v1x + v0y * v1y;
    let dot02 = v0x * v2x + v0y * v2y;
    let dot11 = v1x * v1x + v1y * v1y;
    let dot12 = v1x * v2x + v1y * v2y;
    let denom = dot00 * dot11 - dot01 * dot01;
    if denom.abs() < 1.0e-12 {
        return false;
    }
    let u = (dot11 * dot02 - dot01 * dot12) / denom;
    let v = (dot00 * dot12 - dot01 * dot02) / denom;
    u >= -1.0e-4 && v >= -1.0e-4 && u + v <= 1.0 + 1.0e-4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Path;

    #[test]
    fn square_fill_covers_the_center_and_not_the_outside() {
        let path = Path::polygon(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
        let mesh = tessellate_fill(&path, FillRule::NonZero, 1.0);
        assert!(mesh.contains([5.0, 5.0]));
        assert!(!mesh.contains([-1.0, 5.0]));
        assert!((mesh.area() - 100.0).abs() < 0.5);
    }

    #[test]
    fn even_odd_punches_a_hole_nonzero_does_not() {
        let mut path = Path::builder();
        path.move_to([0.0, 0.0])
            .line_to([30.0, 0.0])
            .line_to([30.0, 30.0])
            .line_to([0.0, 30.0])
            .close();
        path.move_to([10.0, 10.0])
            .line_to([20.0, 10.0])
            .line_to([20.0, 20.0])
            .line_to([10.0, 20.0])
            .close();
        let path = path.build();
        let even = tessellate_fill(&path, FillRule::EvenOdd, 1.0);
        let nonzero = tessellate_fill(&path, FillRule::NonZero, 1.0);
        assert!(!even.contains([15.0, 15.0]), "even-odd hole");
        assert!(even.contains([5.0, 5.0]));
        assert!(
            nonzero.contains([15.0, 15.0]),
            "same winding fills the hole"
        );
        assert!(even.area() < nonzero.area() - 50.0);
    }

    #[test]
    fn dashed_stroke_breaks_into_separated_runs() {
        let path = Path::line([0.0, 0.0], [40.0, 0.0]);
        let stroke = Stroke::solid(2.0).dashed(vec![4.0, 4.0]);
        let mesh = tessellate_stroke(&path, &stroke, 1.0);
        assert!(!mesh.is_empty());
        let mut xs: Vec<f32> = mesh.vertices.iter().map(|p| p[0]).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let span = xs.last().unwrap() - xs.first().unwrap();
        assert!(span > 20.0 && span < 42.0, "span {span}");
        // A gap around the middle of an off-interval should miss the mesh.
        assert!(!mesh.contains([6.0, 0.0]), "inside a dash gap");
    }

    #[test]
    fn cache_reuses_a_bucket_and_rebuilds_across_it() {
        let path = Path::ellipse([0.0, 0.0], [8.0, 4.0]);
        let mut cache = MeshCache::new(8);
        let a = cache.fill(&path, FillRule::NonZero, 1.2);
        let b = cache.fill(&path, FillRule::NonZero, 1.8);
        assert!(Arc::ptr_eq(&a, &b));
        let c = cache.fill(&path, FillRule::NonZero, 2.5);
        assert!(!Arc::ptr_eq(&a, &c));
    }

    #[test]
    fn hairline_mesh_thins_when_the_bucket_jumps() {
        let path = Path::line([0.0, 0.0], [20.0, 0.0]);
        let stroke = Stroke::hairline(2.0);
        let near = tessellate_stroke(&path, &stroke, 1.0);
        let far = tessellate_stroke(&path, &stroke, 8.0);
        assert!(near.area() > far.area() * 2.0);
    }
}
