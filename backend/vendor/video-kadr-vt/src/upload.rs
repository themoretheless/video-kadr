//! Owned IOSurface-backed NV12 upload for CPU-rendered export frames.
use super::*;
use video_kadr_hevc_frame::Nv12;

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    static kCVPixelBufferIOSurfacePropertiesKey: CFRef;
    fn CVPixelBufferCreate(
        allocator: CFRef,
        width: usize,
        height: usize,
        format: u32,
        attributes: CFRef,
        out: *mut CFRef,
    ) -> OSStatus;
    fn CVPixelBufferGetIOSurface(image: CFRef) -> CFRef;
    fn CVPixelBufferGetDataSize(image: CFRef) -> usize;
}
struct Owned(CFRef);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}
struct Locked(CFRef);
impl Drop for Locked {
    fn drop(&mut self) {
        unsafe { CVPixelBufferUnlockBaseAddress(self.0, 0) };
    }
}
impl Surface {
    pub fn from_nv12(frame: &Nv12) -> Result<Self, Error> {
        // All objects below are owned, and every early return releases them.
        unsafe {
            let empty = CFDictionaryCreate(
                ptr::null(),
                ptr::null(),
                ptr::null(),
                0,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            if empty.is_null() {
                return Err(Error("IOSurface attributes allocation failed".into()));
            }
            let empty = Owned(empty);
            let attrs = CFDictionaryCreate(
                ptr::null(),
                &kCVPixelBufferIOSurfacePropertiesKey,
                &empty.0,
                1,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            if attrs.is_null() {
                return Err(Error("pixel buffer attributes allocation failed".into()));
            }
            let attrs = Owned(attrs);
            let mut image = ptr::null();
            let code = CVPixelBufferCreate(
                ptr::null(),
                frame.width(),
                frame.height(),
                NV12_VIDEO,
                attrs.0,
                &mut image,
            );
            if code != 0 || image.is_null() {
                if !image.is_null() {
                    CFRelease(image);
                }
                return Err(Error(format!("NV12 allocation failed: {code}")));
            }
            let image = Owned(image);
            if CVPixelBufferGetIOSurface(image.0).is_null() {
                return Err(Error("NV12 allocation has no IOSurface".into()));
            }
            let surface = Self::retain(image.0)?;
            if CVPixelBufferLockBaseAddress(image.0, 0) != 0 {
                return Err(Error("NV12 upload lock failed".into()));
            }
            let _lock = Locked(image.0);
            // Row-wise copies avoid constructing mutable slices over plane padding
            // or assuming that independently mapped planes form one allocation.
            for (plane, rows, bytes) in [
                (0, frame.height(), frame.y()),
                (1, frame.height() / 2, frame.uv()),
            ] {
                let stride = CVPixelBufferGetBytesPerRowOfPlane(image.0, plane);
                let base = CVPixelBufferGetBaseAddressOfPlane(image.0, plane).cast_mut();
                let extent = stride
                    .checked_mul(rows)
                    .ok_or_else(|| Error("NV12 upload stride overflow".into()))?;
                if base.is_null()
                    || stride < frame.width()
                    || extent > isize::MAX as usize
                    || extent > CVPixelBufferGetDataSize(image.0)
                {
                    return Err(Error("invalid NV12 upload plane".into()));
                }
                for row in 0..rows {
                    ptr::copy_nonoverlapping(
                        bytes.as_ptr().add(row * frame.width()),
                        base.add(row * stride),
                        frame.width(),
                    );
                }
            }
            Ok(surface)
        }
    }
}
