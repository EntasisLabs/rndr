//! Shelf-packed glyph atlases. Coverage is R8; emoji is straight RGBA.
//!
//! Slots the current frame and the previous frame still reference stay. Anything
//! else is eligible for LRU eviction once the atlas cannot grow. A grow throws
//! the shelf away, so callers retry a line whose pixels moved.

use std::collections::HashMap;

use etagere::{size2, AllocId, AtlasAllocator};

pub(super) const SUBPIXEL_BINS: u8 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct GlyphKey {
    pub font: u64,
    pub glyph: u32,
    /// Pixel size, so a scale change does not reuse a mask.
    pub size_bits: u32,
    pub bin: u8,
    pub color: bool,
}

struct Slot {
    id: AllocId,
    /// Ink origin inside the padded allocation.
    ink_x: u32,
    ink_y: u32,
    ink_w: u32,
    ink_h: u32,
    refs: u32,
    stamp: u64,
    /// Shapes in progress stamp this so a full atlas does not evict their glyphs.
    build: u64,
}

#[derive(Clone, Copy)]
struct Dirty {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

pub(super) struct Atlas {
    color: bool,
    size: u32,
    max: u32,
    pixels: Vec<u8>,
    packer: AtlasAllocator,
    glyphs: HashMap<GlyphKey, Slot>,
    dirty: Vec<Dirty>,
    full: bool,
    generation: u64,
    stamp: u64,
    build: u64,
    pub evictions: u64,
    white: Option<GlyphKey>,
}

pub(super) struct Ink {
    pub uv: [f32; 4],
    pub key: GlyphKey,
}

impl Atlas {
    pub(super) fn new(color: bool, size: u32, max: u32) -> Self {
        let size = size.clamp(8, max.max(8));
        let max = max.max(size);
        let pixels = vec![0; (size as usize) * (size as usize) * if color { 4 } else { 1 }];
        let mut atlas = Self {
            color,
            size,
            max,
            pixels,
            packer: AtlasAllocator::new(size2(size as i32, size as i32)),
            glyphs: HashMap::new(),
            dirty: Vec::new(),
            full: true,
            generation: 0,
            stamp: 1,
            build: 1,
            evictions: 0,
            white: None,
        };
        atlas.pin_white();
        atlas
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn begin_build(&mut self) {
        self.build = self.build.wrapping_add(1);
    }

    pub(super) fn solid_uv(&self) -> [f32; 4] {
        let Some(key) = self.white else {
            return [0.0, 0.0, 1.0, 1.0];
        };
        let Some(slot) = self.glyphs.get(&key) else {
            return [0.0, 0.0, 1.0, 1.0];
        };
        self.uv_of(slot.ink_x, slot.ink_y, slot.ink_w, slot.ink_h)
    }

    pub(super) fn snapshot(&self) -> (u32, u32, Vec<u8>) {
        (self.size, self.size, self.pixels.clone())
    }

    /// Copy dirty pixels out and clear the dirty set. `full` replaces the texture.
    pub(super) fn take_dirty(&mut self) -> AtlasDelta {
        let bpp = self.bpp();
        let pending = if self.full {
            Vec::new()
        } else {
            std::mem::take(&mut self.dirty)
        };
        let rects = pending
            .iter()
            .map(|dirty| AtlasRect {
                x: dirty.x,
                y: dirty.y,
                width: dirty.w,
                height: dirty.h,
                bytes: self.copy_rect(dirty.x, dirty.y, dirty.w, dirty.h, bpp),
            })
            .collect();
        let pixels = if self.full {
            self.pixels.clone()
        } else {
            Vec::new()
        };
        let full = self.full;
        self.full = false;
        self.dirty.clear();
        AtlasDelta {
            width: self.size,
            height: self.size,
            bpp,
            full,
            pixels,
            rects,
        }
    }

    pub(super) fn touch(&mut self, keys: &[GlyphKey]) {
        let stamp = self.stamp;
        self.stamp = self.stamp.wrapping_add(1);
        for key in keys {
            if let Some(slot) = self.glyphs.get_mut(key) {
                slot.stamp = stamp;
            }
        }
    }

    pub(super) fn add_refs(&mut self, keys: &[GlyphKey]) {
        let stamp = self.stamp;
        self.stamp = self.stamp.wrapping_add(1);
        for key in keys {
            if let Some(slot) = self.glyphs.get_mut(key) {
                slot.refs = slot.refs.saturating_add(1);
                slot.stamp = stamp;
            }
        }
    }

    pub(super) fn release(&mut self, keys: &[GlyphKey]) {
        for key in keys {
            if let Some(slot) = self.glyphs.get_mut(key) {
                slot.refs = slot.refs.saturating_sub(1);
            }
        }
    }

    /// Raster `image` into the atlas. `None` means it did not fit, or the shelf grew
    /// and the caller must retry the whole line.
    pub(super) fn insert(
        &mut self,
        key: GlyphKey,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Inserted {
        if let Some(slot) = self.glyphs.get(&key) {
            let uv = self.uv_of(slot.ink_x, slot.ink_y, slot.ink_w, slot.ink_h);
            return Inserted::Ready(Ink { uv, key });
        }
        if width == 0 || height == 0 {
            return Inserted::Skip;
        }
        let pad = 1u32;
        let alloc_w = width.saturating_add(pad * 2);
        let alloc_h = height.saturating_add(pad * 2);
        if alloc_w > self.size || alloc_h > self.size {
            if !self.grow_for(alloc_w.max(alloc_h)) {
                return Inserted::Skip;
            }
            return Inserted::Grew;
        }
        let Some((id, x, y)) = self.allocate(alloc_w, alloc_h) else {
            if self.grow_for(self.size.saturating_mul(2)) {
                return Inserted::Grew;
            }
            return Inserted::Skip;
        };
        let ink_x = x + pad;
        let ink_y = y + pad;
        self.zero(x, y, alloc_w, alloc_h);
        self.blit(ink_x, ink_y, width, height, pixels);
        self.dirty.push(Dirty {
            x,
            y,
            w: alloc_w,
            h: alloc_h,
        });
        let stamp = self.stamp;
        self.stamp = self.stamp.wrapping_add(1);
        self.glyphs.insert(
            key,
            Slot {
                id,
                ink_x,
                ink_y,
                ink_w: width,
                ink_h: height,
                refs: 0,
                stamp,
                build: self.build,
            },
        );
        let uv = self.uv_of(ink_x, ink_y, width, height);
        Inserted::Ready(Ink { uv, key })
    }

    fn allocate(&mut self, w: u32, h: u32) -> Option<(AllocId, u32, u32)> {
        loop {
            if let Some(alloc) = self.packer.allocate(size2(w as i32, h as i32)) {
                let rect = alloc.rectangle;
                return Some((alloc.id, rect.min.x as u32, rect.min.y as u32));
            }
            if !self.evict_one() {
                return None;
            }
        }
    }

    fn evict_one(&mut self) -> bool {
        let build = self.build;
        let victim = self
            .glyphs
            .iter()
            .filter(|(key, slot)| {
                slot.refs == 0 && slot.build != build && self.white != Some(**key)
            })
            .min_by_key(|(_, slot)| slot.stamp)
            .map(|(key, _)| *key);
        let Some(key) = victim else {
            return false;
        };
        if let Some(slot) = self.glyphs.remove(&key) {
            self.packer.deallocate(slot.id);
            self.evictions += 1;
        }
        true
    }

    fn grow_for(&mut self, need: u32) -> bool {
        if self.size >= self.max || need > self.max {
            return false;
        }
        let mut next = self.size.saturating_mul(2).max(need);
        next = next.min(self.max);
        if next <= self.size {
            return false;
        }
        self.size = next;
        let bytes = (next as usize) * (next as usize) * self.bpp();
        self.pixels = vec![0; bytes];
        self.packer = AtlasAllocator::new(size2(next as i32, next as i32));
        self.glyphs.clear();
        self.dirty.clear();
        self.full = true;
        self.generation = self.generation.wrapping_add(1);
        self.white = None;
        self.pin_white();
        true
    }

    fn pin_white(&mut self) {
        if self.color {
            return;
        }
        let key = GlyphKey {
            font: 0,
            glyph: 0,
            size_bits: 0,
            bin: 0,
            color: false,
        };
        let Some(alloc) = self.packer.allocate(size2(1, 1)) else {
            return;
        };
        let x = alloc.rectangle.min.x as u32;
        let y = alloc.rectangle.min.y as u32;
        let bpp = self.bpp();
        let index = (y as usize * self.size as usize + x as usize) * bpp;
        if let Some(pixel) = self.pixels.get_mut(index) {
            *pixel = 255;
        }
        self.dirty.push(Dirty { x, y, w: 1, h: 1 });
        self.glyphs.insert(
            key,
            Slot {
                id: alloc.id,
                ink_x: x,
                ink_y: y,
                ink_w: 1,
                ink_h: 1,
                refs: u32::MAX,
                stamp: 0,
                build: 0,
            },
        );
        self.white = Some(key);
    }

    fn bpp(&self) -> usize {
        if self.color {
            4
        } else {
            1
        }
    }

    fn uv_of(&self, x: u32, y: u32, w: u32, h: u32) -> [f32; 4] {
        let s = self.size as f32;
        [
            x as f32 / s,
            y as f32 / s,
            (x + w) as f32 / s,
            (y + h) as f32 / s,
        ]
    }

    fn zero(&mut self, x: u32, y: u32, w: u32, h: u32) {
        let bpp = self.bpp();
        let stride = self.size as usize;
        for row in 0..h as usize {
            let start = ((y as usize + row) * stride + x as usize) * bpp;
            let end = start + w as usize * bpp;
            if let Some(span) = self.pixels.get_mut(start..end) {
                span.fill(0);
            }
        }
    }

    fn blit(&mut self, x: u32, y: u32, w: u32, h: u32, src: &[u8]) {
        let bpp = self.bpp();
        let stride = self.size as usize;
        let row_bytes = w as usize * bpp;
        for row in 0..h as usize {
            let from = row * row_bytes;
            let Some(row_src) = src.get(from..from + row_bytes) else {
                return;
            };
            let start = ((y as usize + row) * stride + x as usize) * bpp;
            let end = start + row_bytes;
            if let Some(span) = self.pixels.get_mut(start..end) {
                span.copy_from_slice(row_src);
            }
        }
    }

    fn copy_rect(&self, x: u32, y: u32, w: u32, h: u32, bpp: usize) -> Vec<u8> {
        let stride = self.size as usize;
        let row_bytes = w as usize * bpp;
        let mut out = vec![0; row_bytes * h as usize];
        for row in 0..h as usize {
            let start = ((y as usize + row) * stride + x as usize) * bpp;
            let end = start + row_bytes;
            let dest = row * row_bytes;
            if let Some(span) = self.pixels.get(start..end) {
                out[dest..dest + row_bytes].copy_from_slice(span);
            }
        }
        out
    }
}

pub(super) enum Inserted {
    Ready(Ink),
    Grew,
    Skip,
}

pub(super) struct AtlasDelta {
    pub width: u32,
    pub height: u32,
    pub bpp: usize,
    pub full: bool,
    pub pixels: Vec<u8>,
    pub rects: Vec<AtlasRect>,
}

pub(super) struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}
