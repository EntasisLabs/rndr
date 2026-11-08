//! Latest-wins video frames that any thread pushes and a [`SurfaceView`] draws.
//!
//! A [`FrameSurface`] holds one frame. [`FrameSurface::push`] replaces it and
//! wakes each visible window that draws the surface, once until that window's
//! next frame, through the same path as [`crate::Live`]: the frame repaints
//! without running Input, Update or Layout. Frames are never queued, so a
//! slow window shows the newest frame and drops the rest.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::live::Watchers;
use crate::world::Component;

/// Pixel layout of a [`VideoFrame`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// One plane of 8-bit red, green, blue, alpha, straight alpha, sRGB.
    Rgba,
    /// One plane of 8-bit blue, green, red, alpha, straight alpha, sRGB.
    Bgra,
    /// Plane 0 is 8-bit luma, one byte per pixel. Plane 1 is interleaved
    /// 8-bit Cb and Cr at half width and half height, rounded up.
    Nv12,
}

/// YUV to RGB coefficients for [`PixelFormat::Nv12`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum YuvMatrix {
    /// Standard-definition video.
    Bt601,
    /// High-definition video. Most players and cameras produce this.
    #[default]
    Bt709,
}

/// Value range of [`PixelFormat::Nv12`] samples.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum YuvRange {
    /// Luma 16–235, chroma 16–240. Most video uses this.
    #[default]
    Video,
    /// Luma and chroma 0–255, as JPEG and many cameras produce.
    Full,
}

/// Why [`FrameSurface::push`] refused a frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// Width or height is zero.
    Empty,
    /// A plane holds fewer bytes than the format needs.
    PlaneSize {
        /// Plane index.
        plane: usize,
        /// Bytes the format needs.
        expected: usize,
        /// Bytes given.
        actual: usize,
    },
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "frame has zero width or height"),
            Self::PlaneSize {
                plane,
                expected,
                actual,
            } => write!(
                f,
                "plane {plane} has {actual} bytes; the format needs {expected}"
            ),
        }
    }
}

impl std::error::Error for FrameError {}

/// One decoded picture with tightly packed rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Plane layout.
    pub format: PixelFormat,
    /// YUV coefficients. Ignored for RGB formats.
    pub matrix: YuvMatrix,
    /// YUV range. Ignored for RGB formats.
    pub range: YuvRange,
    /// Plane bytes. RGB formats use only plane 0.
    pub planes: [Vec<u8>; 2],
}

impl VideoFrame {
    /// Straight-alpha RGBA rows, `width * 4` bytes each.
    pub fn rgba(width: u32, height: u32, bytes: Vec<u8>) -> Self {
        Self::rgb(width, height, PixelFormat::Rgba, bytes)
    }

    /// Straight-alpha BGRA rows, `width * 4` bytes each.
    pub fn bgra(width: u32, height: u32, bytes: Vec<u8>) -> Self {
        Self::rgb(width, height, PixelFormat::Bgra, bytes)
    }

    /// NV12 luma and interleaved chroma planes, BT.709 video range.
    pub fn nv12(width: u32, height: u32, luma: Vec<u8>, chroma: Vec<u8>) -> Self {
        Self {
            width,
            height,
            format: PixelFormat::Nv12,
            matrix: YuvMatrix::default(),
            range: YuvRange::default(),
            planes: [luma, chroma],
        }
    }

    /// Store `matrix` and `range` and return the frame.
    pub fn with_yuv(mut self, matrix: YuvMatrix, range: YuvRange) -> Self {
        self.matrix = matrix;
        self.range = range;
        self
    }

    fn rgb(width: u32, height: u32, format: PixelFormat, bytes: Vec<u8>) -> Self {
        Self {
            width,
            height,
            format,
            matrix: YuvMatrix::default(),
            range: YuvRange::default(),
            planes: [bytes, Vec::new()],
        }
    }

    /// Bytes each plane needs: `[plane 0, plane 1]`.
    pub fn plane_sizes(&self) -> [usize; 2] {
        let (w, h) = (self.width as usize, self.height as usize);
        match self.format {
            PixelFormat::Rgba | PixelFormat::Bgra => [w * h * 4, 0],
            PixelFormat::Nv12 => [w * h, w.div_ceil(2) * h.div_ceil(2) * 2],
        }
    }

    /// Check the size and plane lengths.
    pub fn validate(&self) -> Result<(), FrameError> {
        if self.width == 0 || self.height == 0 {
            return Err(FrameError::Empty);
        }
        for (plane, expected) in self.plane_sizes().into_iter().enumerate() {
            let actual = self.planes[plane].len();
            if actual < expected {
                return Err(FrameError::PlaneSize {
                    plane,
                    expected,
                    actual,
                });
            }
        }
        Ok(())
    }
}

/// Counters for one [`FrameSurface`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SurfaceStats {
    /// Frames accepted by [`FrameSurface::push`].
    pub pushed: u64,
    /// Frames a renderer took through [`FrameSurface::latest_after`].
    /// `pushed - shown` were replaced before any window drew them.
    pub shown: u64,
}

#[derive(Default)]
struct Slot {
    frame: Option<Arc<VideoFrame>>,
    version: u64,
    stats: SurfaceStats,
}

struct Shared {
    key: u64,
    slot: Mutex<Slot>,
    watchers: Watchers,
}

/// Cloneable, thread-safe, latest-wins frame slot.
///
/// Clones share one frame. Draw it with a [`SurfaceView`].
#[derive(Clone)]
pub struct FrameSurface(Arc<Shared>);

impl std::fmt::Debug for FrameSurface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FrameSurface").field(&self.0.key).finish()
    }
}

impl Default for FrameSurface {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for FrameSurface {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl FrameSurface {
    /// Empty surface. A view of it draws nothing until the first push.
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(Arc::new(Shared {
            key: NEXT.fetch_add(1, Ordering::Relaxed),
            slot: Mutex::new(Slot::default()),
            watchers: Watchers::default(),
        }))
    }

    /// Process-unique id shared by clones. Renderers key GPU textures by it.
    pub fn key(&self) -> u64 {
        self.0.key
    }

    /// Replace the frame and wake windows that draw this surface. A frame that
    /// fails [`VideoFrame::validate`] is refused and the old frame stays.
    pub fn push(&self, frame: VideoFrame) -> Result<(), FrameError> {
        frame.validate()?;
        {
            let mut slot = lock(&self.0.slot);
            slot.frame = Some(Arc::new(frame));
            slot.version += 1;
            slot.stats.pushed += 1;
        }
        self.0.watchers.touch();
        Ok(())
    }

    /// Drop the frame, so views draw nothing.
    pub fn clear(&self) {
        {
            let mut slot = lock(&self.0.slot);
            if slot.frame.take().is_none() {
                return;
            }
            slot.version += 1;
        }
        self.0.watchers.touch();
    }

    /// `[width, height]` of the current frame.
    pub fn size(&self) -> Option<[u32; 2]> {
        lock(&self.0.slot)
            .frame
            .as_ref()
            .map(|frame| [frame.width, frame.height])
    }

    /// The current frame.
    pub fn frame(&self) -> Option<Arc<VideoFrame>> {
        lock(&self.0.slot).frame.clone()
    }

    /// Changes so far: each push and each clear of a frame adds one.
    pub fn version(&self) -> u64 {
        lock(&self.0.slot).version
    }

    /// Frames pushed and shown.
    pub fn stats(&self) -> SurfaceStats {
        lock(&self.0.slot).stats
    }

    /// For renderers: the version and frame when [`Self::version`] is past
    /// `seen`, counting the frame as shown. The frame is `None` after
    /// [`Self::clear`].
    pub fn latest_after(&self, seen: u64) -> Option<(u64, Option<Arc<VideoFrame>>)> {
        let mut slot = lock(&self.0.slot);
        if slot.version <= seen {
            return None;
        }
        if slot.frame.is_some() {
            slot.stats.shown += 1;
        }
        Some((slot.version, slot.frame.clone()))
    }

    pub(crate) fn watchers(&self) -> &Watchers {
        &self.0.watchers
    }
}

/// How a frame fills its [`SurfaceView`] box when their shapes differ.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SurfaceFit {
    /// Show the whole frame, centred, leaving empty bars.
    #[default]
    Contain,
    /// Fill the box, centred, cropping the frame.
    Cover,
    /// Stretch the frame to the box.
    Stretch,
}

/// Draws a [`FrameSurface`]'s latest frame in the entity's [`crate::Rect`].
///
/// The frame is drawn in paint order like an image, inside any clip and any
/// faded or transformed group around the entity.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceView {
    /// Frames to draw.
    pub surface: FrameSurface,
    /// How the frame fills the box.
    pub fit: SurfaceFit,
    /// Flip the frame left to right, as for a front camera.
    pub mirrored: bool,
    /// Corner radius in logical points, applied to the drawn frame.
    pub radius: f32,
}

impl Component for SurfaceView {}

impl SurfaceView {
    /// Contained, unmirrored, square view of `surface`.
    pub fn new(surface: &FrameSurface) -> Self {
        Self {
            surface: surface.clone(),
            fit: SurfaceFit::Contain,
            mirrored: false,
            radius: 0.0,
        }
    }

    /// Store `fit` and return the view.
    pub fn with_fit(mut self, fit: SurfaceFit) -> Self {
        self.fit = fit;
        self
    }

    /// Store `mirrored` and return the view.
    pub fn with_mirrored(mut self, mirrored: bool) -> Self {
        self.mirrored = mirrored;
        self
    }

    /// Store `radius` and return the view.
    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Where the frame lands in `rect`, and its crop as `(u0, v0, u1, v1)`.
    /// `None` without a frame or with an empty box.
    pub fn placement(&self, rect: crate::Rect) -> Option<(crate::Rect, [f32; 4])> {
        let [fw, fh] = self.surface.size()?;
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return None;
        }
        let frame = fw as f32 / fh as f32;
        let space = rect.w / rect.h;
        let full = [0.0, 0.0, 1.0, 1.0];
        Some(match self.fit {
            SurfaceFit::Stretch => (rect, full),
            SurfaceFit::Contain if frame > space => {
                let h = rect.w / frame;
                let y = rect.y + (rect.h - h) * 0.5;
                (crate::Rect { y, h, ..rect }, full)
            }
            SurfaceFit::Contain => {
                let w = rect.h * frame;
                let x = rect.x + (rect.w - w) * 0.5;
                (crate::Rect { x, w, ..rect }, full)
            }
            SurfaceFit::Cover if frame > space => {
                let keep = space / frame;
                let u0 = (1.0 - keep) * 0.5;
                (rect, [u0, 0.0, u0 + keep, 1.0])
            }
            SurfaceFit::Cover => {
                let keep = frame / space;
                let v0 = (1.0 - keep) * 0.5;
                (rect, [0.0, v0, 1.0, v0 + keep])
            }
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}
