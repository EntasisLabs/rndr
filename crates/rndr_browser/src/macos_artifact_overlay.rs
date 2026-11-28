//! Trusted host alpha images in native siblings above the isolated WebKit view.
use crate::{ArtifactError, ClientRect, OverlayBitmap};
use objc2::mutability::MainThreadOnly;
use objc2::rc::Retained;
use objc2::{declare_class, ClassType, DeclaredClass};
use objc2_app_kit::{
    NSBitmapFormat, NSBitmapImageRep, NSDeviceRGBColorSpace, NSImage, NSImageScaling, NSImageView,
    NSView,
};
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSPoint, NSRect, NSSize};
use std::cell::Cell;

pub(crate) struct Ivars {
    pointer: Cell<Option<ClientRect>>,
}
declare_class!(
    struct OverlayView;
    unsafe impl ClassType for OverlayView {
        type Super = NSImageView;
        type Mutability = MainThreadOnly;
        const NAME: &'static str = "RndrTrustedArtifactOverlay";
    }
    impl DeclaredClass for OverlayView { type Ivars = Ivars; }
    unsafe impl NSObjectProtocol for OverlayView {}
    unsafe impl OverlayView {
        #[method_id(hitTest:)]
        unsafe fn hit_test(&self, point: NSPoint) -> Option<Retained<NSView>> {
            // Points arrive in the parent's coordinate system. The host popup
            // receives native events through winit's existing parent NSView;
            // the image view itself never becomes an input/keyboard target.
            let parent=self.superview();
            let mut capture = false;
            if let (Some(rect),Some(parent))=(self.ivars().pointer.get(),parent.as_ref()) {
                let local=self.convertPoint_fromView(point,Some(parent));
                let bounds=self.bounds();
                let inside=local.x>=bounds.origin.x && local.y>=bounds.origin.y
                    && local.x<bounds.origin.x+bounds.size.width && local.y<bounds.origin.y+bounds.size.height;
                let pb=parent.bounds();
                let x=point.x-pb.origin.x;
                let y=if parent.isFlipped(){point.y-pb.origin.y}else{pb.origin.y+pb.size.height-point.y};
                if !self.isHidden() && inside
                    && x >= f64::from(rect.x) && x < f64::from(rect.x)+f64::from(rect.width)
                    && y >= f64::from(rect.y) && y < f64::from(rect.y)+f64::from(rect.height) {
                    capture = true;
                }
            }
            if capture { parent } else { None }
        }
    }
);

#[derive(Default)]
pub(crate) struct OverlayPlane {
    revision: Option<u64>,
    views: Vec<Retained<OverlayView>>,
}
impl OverlayPlane {
    pub fn clear(&mut self) {
        for view in self.views.drain(..) {
            unsafe {
                view.removeFromSuperview();
            }
        }
        self.revision = None;
    }
    pub fn apply(
        &mut self,
        parent: &NSView,
        revision: u64,
        images: &[OverlayBitmap],
        visible: bool,
    ) -> Result<(), ArtifactError> {
        if self.revision != Some(revision) {
            let mut next = Vec::with_capacity(images.len());
            for image in images {
                next.push(make_view(image)?);
            }
            // All images have been admitted and allocated before changing the
            // native tree. Old views retire before the new top-to-bottom set.
            self.clear();
            for view in &next {
                unsafe {
                    parent.addSubview(view);
                }
            }
            self.views = next;
            self.revision = Some(revision);
        }
        for (view, image) in self.views.iter().zip(images) {
            let r = image.bounds;
            let pb = parent.bounds();
            let placed = crate::geometry::place_in_parent(
                crate::geometry::ParentBounds {
                    flipped: parent.isFlipped(),
                    origin_x: pb.origin.x,
                    origin_y: pb.origin.y,
                    width: pb.size.width,
                    height: pb.size.height,
                },
                r,
            );
            let frame = NSRect::new(
                NSPoint::new(placed.x, placed.y),
                NSSize::new(placed.width, placed.height),
            );
            unsafe {
                if view.frame() != frame {
                    view.setFrame(frame);
                }
                if view.isHidden() == visible {
                    view.setHidden(!visible);
                }
            }
        }
        Ok(())
    }
}
impl Drop for OverlayPlane {
    fn drop(&mut self) {
        self.clear();
    }
}

fn make_view(image: &OverlayBitmap) -> Result<Retained<OverlayView>, ArtifactError> {
    let mtm = MainThreadMarker::new().ok_or(ArtifactError::Overlay("not on UI thread"))?;
    unsafe {
        // AppKit allocates and owns the copy. No pointer into caller memory is
        // retained, and WebKit never receives this image or its bytes.
        let bitmap=NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            mtm.alloc(), std::ptr::null_mut(), image.width as isize,image.height as isize,
            8,4,true,false,NSDeviceRGBColorSpace,NSBitmapFormat::AlphaNonpremultiplied,image.width as isize*4,32
        ).ok_or(ArtifactError::Overlay("bitmap allocation failed"))?;
        let data = bitmap.bitmapData();
        if data.is_null() {
            return Err(ArtifactError::Overlay("bitmap storage unavailable"));
        }
        std::ptr::copy_nonoverlapping(image.rgba.as_ptr(), data, image.rgba.len());
        let native = NSImage::initWithSize(
            mtm.alloc(),
            NSSize::new(f64::from(image.width), f64::from(image.height)),
        );
        native.addRepresentation(&bitmap);
        let view: Retained<OverlayView> = objc2::msg_send_id![super(mtm.alloc::<OverlayView>().set_ivars(Ivars{pointer:Cell::new(image.pointer_bounds)})),initWithFrame:NSRect::ZERO];
        view.setHidden(true);
        view.setWantsLayer(true);
        view.setImage(Some(&native));
        view.setImageScaling(NSImageScaling::NSImageScaleAxesIndependently);
        Ok(view)
    }
}
