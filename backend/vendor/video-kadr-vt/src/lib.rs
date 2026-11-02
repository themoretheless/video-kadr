//! Hardware H.264 decoding on macOS through VideoToolbox, Apple's interface
//! to the media engine. Frames come back as packed 8-bit planar 4:2:0 in
//! decode order; the caller reorders them for display exactly as it does for
//! the software decoder. The bindings are the plain C API (CoreFoundation,
//! CoreMedia, CoreVideo, VideoToolbox), no Objective-C.
#![cfg(target_os = "macos")]
use std::{ffi::c_void, fmt, ptr};
mod encode;
pub use encode::{EncodedFrame, EncodedTime, Encoder, EncoderCodec};

type OSStatus = i32;
type CFRef = *const c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}
#[repr(C)]
struct CallbackRecord {
    callback: Option<OutputCallback>,
    refcon: *mut c_void,
}
type OutputCallback = unsafe extern "C" fn(
    refcon: *mut c_void,
    source: *mut c_void,
    status: OSStatus,
    info_flags: u32,
    image: CFRef,
    pts: CMTime,
    duration: CMTime,
);
/// Sized stand-ins for the opaque CoreFoundation callback tables; only their
/// addresses are used.
#[repr(C)]
struct KeyCallBacks {
    _words: [usize; 7],
}
#[repr(C)]
struct ValueCallBacks {
    _words: [usize; 6],
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFAllocatorNull: CFRef;
    static kCFBooleanTrue: CFRef;
    fn CFBooleanGetValue(value: CFRef) -> u8;
    static kCFTypeDictionaryKeyCallBacks: KeyCallBacks;
    static kCFTypeDictionaryValueCallBacks: ValueCallBacks;
    fn CFRelease(cf: CFRef);
    fn CFRetain(cf: CFRef) -> CFRef;
    fn CFDictionaryCreate(
        allocator: CFRef,
        keys: *const CFRef,
        values: *const CFRef,
        count: isize,
        key_callbacks: *const KeyCallBacks,
        value_callbacks: *const ValueCallBacks,
    ) -> CFRef;
    fn CFNumberCreate(allocator: CFRef, number_type: isize, value: *const c_void) -> CFRef;
    fn CFDataCreate(allocator: CFRef, bytes: *const u8, length: usize) -> CFRef;
    fn CFStringCreateWithCString(allocator: CFRef, c_str: *const u8, encoding: u32) -> CFRef;
}
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
fn cfstr(s: &str) -> CFRef {
    // SAFETY: s is a valid UTF-8 string that we pass as a C string with a null terminator.
    unsafe {
        let mut buf = s.as_bytes().to_vec();
        buf.push(0);
        CFStringCreateWithCString(ptr::null(), buf.as_ptr(), K_CF_STRING_ENCODING_UTF8)
    }
}
macro_rules! CFSTR {
    ($s:expr) => {
        cfstr($s)
    };
}
#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    fn CMVideoFormatDescriptionCreateFromH264ParameterSets(
        allocator: CFRef,
        count: usize,
        pointers: *const *const u8,
        sizes: *const usize,
        nal_length_size: i32,
        out: *mut CFRef,
    ) -> OSStatus;
    fn CMVideoFormatDescriptionCreateFromHEVCParameterSets(
        allocator: CFRef,
        count: usize,
        pointers: *const *const u8,
        sizes: *const usize,
        nal_length_size: i32,
        parameter_set_strings: *const c_void,
        out: *mut CFRef,
    ) -> OSStatus;
    fn CMVideoFormatDescriptionCreate(
        allocator: CFRef,
        codec_type: u32,
        width: i32,
        height: i32,
        extensions: CFRef,
        out: *mut CFRef,
    ) -> OSStatus;
    fn CMBlockBufferCreateWithMemoryBlock(
        structure_allocator: CFRef,
        memory: *mut c_void,
        block_length: usize,
        block_allocator: CFRef,
        custom_source: *const c_void,
        offset: usize,
        data_length: usize,
        flags: u32,
        out: *mut CFRef,
    ) -> OSStatus;
    fn CMSampleBufferCreateReady(
        allocator: CFRef,
        data: CFRef,
        format: CFRef,
        sample_count: isize,
        timing_count: isize,
        timing: *const c_void,
        size_count: isize,
        sizes: *const usize,
        out: *mut CFRef,
    ) -> OSStatus;
}
#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    static kCVPixelBufferPixelFormatTypeKey: CFRef;
    static kCVPixelBufferMetalCompatibilityKey: CFRef;
    fn CVPixelBufferLockBaseAddress(buffer: CFRef, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(buffer: CFRef, flags: u64) -> i32;
    fn CVPixelBufferGetPixelFormatType(buffer: CFRef) -> u32;
    fn CVPixelBufferGetPlaneCount(buffer: CFRef) -> usize;
    fn CVPixelBufferGetWidthOfPlane(buffer: CFRef, plane: usize) -> usize;
    fn CVPixelBufferGetHeightOfPlane(buffer: CFRef, plane: usize) -> usize;
    fn CVPixelBufferGetBytesPerRowOfPlane(buffer: CFRef, plane: usize) -> usize;
    fn CVPixelBufferGetBaseAddressOfPlane(buffer: CFRef, plane: usize) -> *const u8;
}
#[link(name = "VideoToolbox", kind = "framework")]
unsafe extern "C" {
    static kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder: CFRef;
    static kVTDecompressionPropertyKey_UsingHardwareAcceleratedVideoDecoder: CFRef;
    fn VTSessionCopyProperty(
        session: CFRef,
        key: CFRef,
        allocator: CFRef,
        out: *mut CFRef,
    ) -> OSStatus;
    fn VTDecompressionSessionCreate(
        allocator: CFRef,
        format: CFRef,
        decoder_specification: CFRef,
        destination_attributes: CFRef,
        callback: *const CallbackRecord,
        out: *mut CFRef,
    ) -> OSStatus;
    fn VTDecompressionSessionDecodeFrame(
        session: CFRef,
        sample: CFRef,
        flags: u32,
        source: *mut c_void,
        info_flags: *mut u32,
    ) -> OSStatus;
    fn VTDecompressionSessionInvalidate(session: CFRef);
}

/// kCVPixelFormatType_420YpCbCr8Planar ('y420') and its full-range twin ('f420').
const PLANAR_420: u32 = 0x7934_3230;
const PLANAR_420_FULL: u32 = 0x6634_3230;
const NV12_VIDEO: u32 = 0x3432_3076; // 420v
const NV12_FULL: u32 = 0x3432_3066; // 420f
const P010_VIDEO: u32 = 0x7834_3230; // x420: ten bits in the high bits of each word
const P010_FULL: u32 = 0x7866_3230; // xf20
const CF_NUMBER_SINT32: isize = 3;
const READ_ONLY_LOCK: u64 = 1;
const K_CM_VIDEO_CODEC_TYPE_VP9: u32 = 0x7670_3039; // 'vp09'
const K_CM_VIDEO_CODEC_TYPE_AV1: u32 = 0x6176_3031; // 'av01'

#[derive(Debug)]
pub struct Error(String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
fn status(what: &str, code: OSStatus) -> Error {
    Error(format!("VideoToolbox: {what} failed ({code})"))
}

/// A decoded picture as tightly packed 8-bit planes; chroma planes are
/// `width.div_ceil(2)` by `height.div_ceil(2)`.
pub struct Planes {
    pub width: usize,
    pub height: usize,
    pub full_range: bool,
    /// 8-bit bytes, or little-endian 10-bit samples in 16-bit words.
    pub depth: u8,
    pub y: Vec<u8>,
    pub cb: Vec<u8>,
    pub cr: Vec<u8>,
}

/// One frame's output slot, addressed through the source refcon.
/// kVTVideoDecoderReferenceMissingErr: the frame predicts from a picture the
/// decoder never saw (a stream opening mid-GOP, open-GOP leading frames).
/// Nothing can be shown for it, but decoding recovers at the next keyframe.
const REFERENCE_MISSING: OSStatus = -17694;

/// Retained decoder output. Keeping this object alive prevents the decoder pool
/// from recycling the frame. Pixel data is copied only by `download`.
#[derive(Clone)]
pub struct Surface(std::sync::Arc<SurfaceInner>);
struct SurfaceInner {
    image: CFRef,
    width: usize,
    height: usize,
    depth: u8,
    full_range: bool,
    storage_bytes: usize,
}
// SAFETY: the retained decoder output is immutable; CoreVideo reference counting
// and read-only pixel-buffer access support use across threads.
unsafe impl Send for SurfaceInner {}
unsafe impl Sync for SurfaceInner {}
impl Drop for SurfaceInner {
    fn drop(&mut self) {
        // SAFETY: this handle owns precisely one CFRetain reference.
        unsafe { CFRelease(self.image) };
    }
}
impl Surface {
    pub fn width(&self) -> usize {
        self.0.width
    }
    pub fn height(&self) -> usize {
        self.0.height
    }
    pub fn depth(&self) -> u8 {
        self.0.depth
    }
    pub fn full_range(&self) -> bool {
        self.0.full_range
    }
    /// Explicit CPU download, useful for exports or a software fallback.
    /// Decoder-pool bytes retained by this image, including row padding.
    pub fn storage_bytes(&self) -> usize {
        self.0.storage_bytes
    }
    pub fn download(&self) -> Result<Planes, Error> {
        // SAFETY: self retains the immutable image throughout the copy.
        unsafe { copy_planes(self.0.image) }?
            .ok_or_else(|| Error("VideoToolbox: missing retained picture".into()))
    }
    unsafe fn retain(image: CFRef) -> Result<Self, Error> {
        // SAFETY: the callback provides a live immutable pixel buffer.
        unsafe {
            let format = CVPixelBufferGetPixelFormatType(image);
            let (depth, full_range, planes) = match format {
                PLANAR_420 => (8, false, 3),
                PLANAR_420_FULL => (8, true, 3),
                NV12_VIDEO => (8, false, 2),
                NV12_FULL => (8, true, 2),
                P010_VIDEO => (10, false, 2),
                P010_FULL => (10, true, 2),
                _ => return Err(Error("VideoToolbox: unsupported surface format".into())),
            };
            let width = CVPixelBufferGetWidthOfPlane(image, 0);
            let height = CVPixelBufferGetHeightOfPlane(image, 0);
            if width == 0 || height == 0 || CVPixelBufferGetPlaneCount(image) != planes {
                return Err(Error("VideoToolbox: invalid surface geometry".into()));
            }
            for plane in 1..planes {
                if CVPixelBufferGetWidthOfPlane(image, plane) != width.div_ceil(2)
                    || CVPixelBufferGetHeightOfPlane(image, plane) != height.div_ceil(2)
                {
                    return Err(Error(
                        "VideoToolbox: invalid surface chroma geometry".into(),
                    ));
                }
            }
            let storage_bytes = (0..planes).try_fold(0usize, |total, plane| {
                CVPixelBufferGetBytesPerRowOfPlane(image, plane)
                    .checked_mul(CVPixelBufferGetHeightOfPlane(image, plane))
                    .and_then(|n| total.checked_add(n))
                    .ok_or_else(|| Error("VideoToolbox: surface storage overflow".into()))
            })?;
            Ok(Self(std::sync::Arc::new(SurfaceInner {
                storage_bytes,
                image: CFRetain(image),
                width,
                height,
                depth,
                full_range,
            })))
        }
    }
}
struct Slot {
    result: Result<Option<Surface>, Error>,
}

unsafe extern "C" fn output(
    _refcon: *mut c_void,
    source: *mut c_void,
    code: OSStatus,
    _info_flags: u32,
    image: CFRef,
    _pts: CMTime,
    _duration: CMTime,
) {
    // SAFETY: `source` is the `Slot` the synchronous decode call passed and
    // still owns; VideoToolbox invokes this callback before that call returns.
    let slot = unsafe { &mut *(source as *mut Slot) };
    if code == REFERENCE_MISSING {
        slot.result = Ok(None);
        return;
    }
    if code != 0 {
        slot.result = Err(status("frame decode", code));
        return;
    }
    if image.is_null() {
        // A dropped frame: nothing to show for this access unit.
        slot.result = Ok(None);
        return;
    }
    slot.result = unsafe { Surface::retain(image) }.map(Some);
}

unsafe fn copy_planes(image: CFRef) -> Result<Option<Planes>, Error> {
    // SAFETY: `image` is a live CVPixelBuffer for the duration of the callback.
    unsafe {
        let format = CVPixelBufferGetPixelFormatType(image);
        if matches!(format, P010_VIDEO | P010_FULL | NV12_VIDEO | NV12_FULL) {
            return copy_biplanar(
                image,
                matches!(format, P010_FULL | NV12_FULL),
                if matches!(format, NV12_VIDEO | NV12_FULL) {
                    8
                } else {
                    10
                },
            )
            .map(Some);
        }
        let full_range = match format {
            PLANAR_420 => false,
            PLANAR_420_FULL => true,
            other => {
                return Err(Error(format!(
                    "VideoToolbox: unexpected pixel format {other:#x}"
                )));
            }
        };
        if CVPixelBufferGetPlaneCount(image) != 3 {
            return Err(Error(
                "VideoToolbox: planar output has no three planes".into(),
            ));
        }
        let lock = CVPixelBufferLockBaseAddress(image, READ_ONLY_LOCK);
        if lock != 0 {
            return Err(status("pixel buffer lock", lock));
        }
        let mut planes = [Vec::new(), Vec::new(), Vec::new()];
        let mut dims = [(0, 0); 3];
        for (index, plane) in planes.iter_mut().enumerate() {
            let width = CVPixelBufferGetWidthOfPlane(image, index);
            let height = CVPixelBufferGetHeightOfPlane(image, index);
            let stride = CVPixelBufferGetBytesPerRowOfPlane(image, index);
            let base = CVPixelBufferGetBaseAddressOfPlane(image, index);
            if base.is_null() || stride < width {
                CVPixelBufferUnlockBaseAddress(image, READ_ONLY_LOCK);
                return Err(Error("VideoToolbox: invalid plane layout".into()));
            }
            let mut out = Vec::with_capacity(width * height);
            for row in 0..height {
                let line = std::slice::from_raw_parts(base.add(row * stride), width);
                out.extend_from_slice(line);
            }
            *plane = out;
            dims[index] = (width, height);
        }
        CVPixelBufferUnlockBaseAddress(image, READ_ONLY_LOCK);
        let (width, height) = dims[0];
        if dims[1] != (width.div_ceil(2), height.div_ceil(2)) || dims[2] != dims[1] {
            return Err(Error("VideoToolbox: chroma planes are not 4:2:0".into()));
        }
        let [y, cb, cr] = planes;
        Ok(Some(Planes {
            depth: 8,
            width,
            height,
            full_range,
            y,
            cb,
            cr,
        }))
    }
}

/// CoreVideo P010 is interleaved Cb/Cr and stores ten bits in the MSBs.
/// Produce tightly packed LE source-depth planes, retaining every coded bit.
#[cfg(test)]
fn unpack_p010(
    y: &[u8],
    uv: &[u8],
    width: usize,
    height: usize,
    y_stride: usize,
    uv_stride: usize,
    full_range: bool,
) -> Result<Planes, Error> {
    unpack_biplanar(y, uv, width, height, y_stride, uv_stride, full_range, 10)
}
fn unpack_biplanar(
    y: &[u8],
    uv: &[u8],
    width: usize,
    height: usize,
    y_stride: usize,
    uv_stride: usize,
    full_range: bool,
    depth: u8,
) -> Result<Planes, Error> {
    let bpp = if depth == 8 { 1 } else { 2 };
    let cw = width.div_ceil(2);
    let ch = height.div_ceil(2);
    let need = |w: usize, h: usize, bytes: usize, stride: usize| {
        w.checked_mul(bytes)
            .filter(|&row| stride >= row)
            .and_then(|row| {
                h.checked_sub(1)
                    .and_then(|h| h.checked_mul(stride))
                    .and_then(|n| n.checked_add(row))
            })
    };
    let yn = need(width, height, bpp, y_stride)
        .ok_or_else(|| Error("VideoToolbox: invalid P010 luma layout".into()))?;
    let cn = need(cw, ch, bpp * 2, uv_stride)
        .ok_or_else(|| Error("VideoToolbox: invalid P010 chroma layout".into()))?;
    if width == 0 || height == 0 || y.len() < yn || uv.len() < cn {
        return Err(Error("VideoToolbox: truncated P010 planes".into()));
    }
    let mut luma = Vec::new();
    let mut cb = Vec::new();
    let mut cr = Vec::new();
    luma.try_reserve_exact(
        width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(bpp))
            .ok_or_else(|| Error("P010 size overflow".into()))?,
    )
    .map_err(|e| Error(e.to_string()))?;
    let chroma_bytes = cw
        .checked_mul(ch)
        .and_then(|n| n.checked_mul(bpp))
        .ok_or_else(|| Error("P010 size overflow".into()))?;
    cb.try_reserve_exact(chroma_bytes)
        .map_err(|e| Error(e.to_string()))?;
    cr.try_reserve_exact(chroma_bytes)
        .map_err(|e| Error(e.to_string()))?;
    let append = |out: &mut Vec<u8>, bytes: &[u8]| {
        if depth == 8 {
            out.push(bytes[0]);
        } else {
            out.extend((u16::from_le_bytes([bytes[0], bytes[1]]) >> 6).to_le_bytes());
        }
    };
    for row in 0..height {
        for bytes in y[row * y_stride..row * y_stride + width * bpp].chunks_exact(bpp) {
            append(&mut luma, bytes);
        }
    }
    for row in 0..ch {
        for bytes in uv[row * uv_stride..row * uv_stride + cw * 2 * bpp].chunks_exact(2 * bpp) {
            append(&mut cb, &bytes[..bpp]);
            append(&mut cr, &bytes[bpp..]);
        }
    }
    Ok(Planes {
        width,
        height,
        full_range,
        depth,
        y: luma,
        cb,
        cr,
    })
}
unsafe fn copy_biplanar(image: CFRef, full_range: bool, depth: u8) -> Result<Planes, Error> {
    // SAFETY: callback retains image; the read-only lock stays live through
    // unpacking and is released on every result. Validate plane extents before
    // constructing slices; row padding is excluded from the returned samples.
    unsafe {
        if CVPixelBufferGetPlaneCount(image) != 2 {
            return Err(Error("VideoToolbox: P010 needs two planes".into()));
        }
        let lock = CVPixelBufferLockBaseAddress(image, READ_ONLY_LOCK);
        if lock != 0 {
            return Err(status("P010 buffer lock", lock));
        }
        let result = (|| {
            let width = CVPixelBufferGetWidthOfPlane(image, 0);
            let height = CVPixelBufferGetHeightOfPlane(image, 0);
            let cw = CVPixelBufferGetWidthOfPlane(image, 1);
            let ch = CVPixelBufferGetHeightOfPlane(image, 1);
            if cw != width.div_ceil(2) || ch != height.div_ceil(2) || width == 0 || height == 0 {
                return Err(Error("VideoToolbox: invalid P010 geometry".into()));
            }
            let ys = CVPixelBufferGetBytesPerRowOfPlane(image, 0);
            let cs = CVPixelBufferGetBytesPerRowOfPlane(image, 1);
            let yp = CVPixelBufferGetBaseAddressOfPlane(image, 0);
            let cp = CVPixelBufferGetBaseAddressOfPlane(image, 1);
            let ylen = ys
                .checked_mul(height)
                .filter(|&n| n <= isize::MAX as usize)
                .ok_or_else(|| Error("P010 extent overflow".into()))?;
            let clen = cs
                .checked_mul(ch)
                .filter(|&n| n <= isize::MAX as usize)
                .ok_or_else(|| Error("P010 extent overflow".into()))?;
            if yp.is_null() || cp.is_null() {
                return Err(Error("VideoToolbox: null P010 plane".into()));
            }
            unpack_biplanar(
                std::slice::from_raw_parts(yp, ylen),
                std::slice::from_raw_parts(cp, clen),
                width,
                height,
                ys,
                cs,
                full_range,
                depth,
            )
        })();
        CVPixelBufferUnlockBaseAddress(image, READ_ONLY_LOCK);
        result
    }
}

/// A decompression session for one video stream (parameter sets from avcC or hvcC).
pub struct Session {
    format: CFRef,
    session: CFRef,
}
// SAFETY: the session is only ever driven from one thread at a time; the
// handle itself may move between threads.
unsafe impl Send for Session {}

impl Session {
    /// Create an H.264 session. `sps` and `pps` are NAL units including their
    /// header byte, as stored in avcC; `nal_length_size` is the sample's
    /// length-prefix width (1, 2, 4).
    pub fn new(sps: &[&[u8]], pps: &[&[u8]], nal_length_size: u8) -> Result<Self, Error> {
        Self::new_avc_format(sps, pps, nal_length_size, PLANAR_420)
    }
    /// H.264 session producing shared, Metal-compatible NV12 surfaces.
    pub fn new_avc_surface(
        sps: &[&[u8]],
        pps: &[&[u8]],
        nal_length_size: u8,
        full_range: bool,
    ) -> Result<Self, Error> {
        Self::new_avc_format(
            sps,
            pps,
            nal_length_size,
            if full_range { NV12_FULL } else { NV12_VIDEO },
        )
    }
    fn new_avc_format(
        sps: &[&[u8]],
        pps: &[&[u8]],
        nal_length_size: u8,
        pixel_format: u32,
    ) -> Result<Self, Error> {
        if sps.is_empty() || pps.is_empty() || !matches!(nal_length_size, 1 | 2 | 4) {
            return Err(Error("VideoToolbox: invalid parameter sets".into()));
        }
        let sets: Vec<&[u8]> = sps.iter().chain(pps).copied().collect();
        let pointers: Vec<*const u8> = sets.iter().map(|s| s.as_ptr()).collect();
        let sizes: Vec<usize> = sets.iter().map(|s| s.len()).collect();
        // SAFETY: plain C calls with valid pointers; every created object is
        // released on failure or in `Drop`.
        unsafe {
            let mut format = ptr::null();
            let code = CMVideoFormatDescriptionCreateFromH264ParameterSets(
                ptr::null(),
                sets.len(),
                pointers.as_ptr(),
                sizes.as_ptr(),
                i32::from(nal_length_size),
                &mut format,
            );
            if code != 0 || format.is_null() {
                return Err(status("format description", code));
            }
            Self::from_format_with_pixel_format(format, pixel_format)
        }
    }

    /// Create an HEVC session. `vps`, `sps`, and `pps` are NAL units including
    /// their header byte, as stored in hvcC; `nal_length_size` is the sample's
    /// length-prefix width (1, 2, 4).
    pub fn new_hevc(
        vps: &[&[u8]],
        sps: &[&[u8]],
        pps: &[&[u8]],
        nal_length_size: u8,
    ) -> Result<Self, Error> {
        Self::new_hevc_with_depth(vps, sps, pps, nal_length_size, 8, false)
    }
    /// Require hardware decode with an output format preserving the source depth.
    pub fn new_hevc_with_depth(
        vps: &[&[u8]],
        sps: &[&[u8]],
        pps: &[&[u8]],
        nal_length_size: u8,
        depth: u8,
        full_range: bool,
    ) -> Result<Self, Error> {
        let pixel_format = match (depth, full_range) {
            (8, false) => PLANAR_420,
            (8, true) => PLANAR_420_FULL,
            (10, false) => P010_VIDEO,
            (10, true) => P010_FULL,
            _ => {
                return Err(Error(
                    "VideoToolbox: hardware HEVC output supports 8 or 10 bits".into(),
                ));
            }
        };
        Self::new_hevc_format(vps, sps, pps, nal_length_size, pixel_format)
    }
    /// HEVC session producing shared NV12 or P010 surfaces for Metal.
    pub fn new_hevc_surface(
        vps: &[&[u8]],
        sps: &[&[u8]],
        pps: &[&[u8]],
        nal_length_size: u8,
        depth: u8,
        full_range: bool,
    ) -> Result<Self, Error> {
        let pixel_format = match (depth, full_range) {
            (8, false) => NV12_VIDEO,
            (8, true) => NV12_FULL,
            (10, false) => P010_VIDEO,
            (10, true) => P010_FULL,
            _ => {
                return Err(Error(
                    "VideoToolbox: shared HEVC output supports 8 or 10 bits".into(),
                ));
            }
        };
        Self::new_hevc_format(vps, sps, pps, nal_length_size, pixel_format)
    }
    fn new_hevc_format(
        vps: &[&[u8]],
        sps: &[&[u8]],
        pps: &[&[u8]],
        nal_length_size: u8,
        pixel_format: u32,
    ) -> Result<Self, Error> {
        if vps.is_empty()
            || sps.is_empty()
            || pps.is_empty()
            || !matches!(nal_length_size, 1 | 2 | 4)
        {
            return Err(Error("VideoToolbox: invalid HEVC parameter sets".into()));
        }
        let sets: Vec<&[u8]> = vps.iter().chain(sps).chain(pps).copied().collect();
        let pointers: Vec<*const u8> = sets.iter().map(|s| s.as_ptr()).collect();
        let sizes: Vec<usize> = sets.iter().map(|s| s.len()).collect();
        // SAFETY: plain C calls with valid pointers; every created object is
        // released on failure or in `Drop`.
        unsafe {
            let mut format = ptr::null();
            let code = CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                ptr::null(),
                sets.len(),
                pointers.as_ptr(),
                sizes.as_ptr(),
                i32::from(nal_length_size),
                ptr::null(),
                &mut format,
            );
            if code != 0 || format.is_null() {
                return Err(status("HEVC format description", code));
            }
            Self::from_format_with_pixel_format(format, pixel_format)
        }
    }

    /// Create a VP9 session. `config` is the vpcC configuration box contents;
    /// `width` and `height` are the coded dimensions.
    pub fn new_vp9(config: &[u8], width: u32, height: u32) -> Result<Self, Error> {
        Self::new_vp9_format(config, width, height, PLANAR_420)
    }

    /// VP9 session producing shared NV12 or P010 surfaces for Metal.
    pub fn new_vp9_surface(
        config: &[u8],
        width: u32,
        height: u32,
        depth: u8,
        full_range: bool,
    ) -> Result<Self, Error> {
        let pixel_format = match (depth, full_range) {
            (8, false) => NV12_VIDEO,
            (8, true) => NV12_FULL,
            (10, false) => P010_VIDEO,
            (10, true) => P010_FULL,
            _ => {
                return Err(Error(
                    "VideoToolbox: shared VP9 output supports 8 or 10 bits".into(),
                ));
            }
        };
        Self::new_vp9_format(config, width, height, pixel_format)
    }

    fn new_vp9_format(
        config: &[u8],
        width: u32,
        height: u32,
        pixel_format: u32,
    ) -> Result<Self, Error> {
        if config.is_empty() {
            return Err(Error("VideoToolbox: empty VP9 configuration".into()));
        }
        unsafe {
            let config_data = CFDataCreate(ptr::null(), config.as_ptr(), config.len());
            if config_data.is_null() {
                return Err(Error(
                    "VideoToolbox: VP9 config data creation failed".into(),
                ));
            }
            let keys = [CFSTR!("vpcC")];
            let values = [config_data];
            let extensions = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            CFRelease(config_data);
            let mut format = ptr::null();
            let code = CMVideoFormatDescriptionCreate(
                ptr::null(),
                K_CM_VIDEO_CODEC_TYPE_VP9,
                width as i32,
                height as i32,
                extensions,
                &mut format,
            );
            if !extensions.is_null() {
                CFRelease(extensions);
            }
            if code != 0 || format.is_null() {
                return Err(status("VP9 format description", code));
            }
            Self::from_format_with_pixel_format(format, pixel_format)
        }
    }

    /// Create an AV1 session. `config` is the av1C configuration box contents;
    /// `width` and `height` are the coded dimensions.
    pub fn new_av1(config: &[u8], width: u32, height: u32) -> Result<Self, Error> {
        Self::new_av1_format(config, width, height, PLANAR_420)
    }

    /// AV1 session producing shared NV12 or P010 surfaces for Metal.
    pub fn new_av1_surface(
        config: &[u8],
        width: u32,
        height: u32,
        depth: u8,
        full_range: bool,
    ) -> Result<Self, Error> {
        let pixel_format = match (depth, full_range) {
            (8, false) => NV12_VIDEO,
            (8, true) => NV12_FULL,
            (10, false) => P010_VIDEO,
            (10, true) => P010_FULL,
            _ => {
                return Err(Error(
                    "VideoToolbox: shared AV1 output supports 8 or 10 bits".into(),
                ));
            }
        };
        Self::new_av1_format(config, width, height, pixel_format)
    }

    fn new_av1_format(
        config: &[u8],
        width: u32,
        height: u32,
        pixel_format: u32,
    ) -> Result<Self, Error> {
        if config.is_empty() {
            return Err(Error("VideoToolbox: empty AV1 configuration".into()));
        }
        unsafe {
            let config_data = CFDataCreate(ptr::null(), config.as_ptr(), config.len());
            if config_data.is_null() {
                return Err(Error(
                    "VideoToolbox: AV1 config data creation failed".into(),
                ));
            }
            let keys = [CFSTR!("av1C")];
            let values = [config_data];
            let extensions = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            CFRelease(config_data);
            let mut format = ptr::null();
            let code = CMVideoFormatDescriptionCreate(
                ptr::null(),
                K_CM_VIDEO_CODEC_TYPE_AV1,
                width as i32,
                height as i32,
                extensions,
                &mut format,
            );
            if !extensions.is_null() {
                CFRelease(extensions);
            }
            if code != 0 || format.is_null() {
                return Err(status("AV1 format description", code));
            }
            Self::from_format_with_pixel_format(format, pixel_format)
        }
    }

    #[allow(dead_code)]
    unsafe fn from_format(format: CFRef) -> Result<Self, Error> {
        // SAFETY: the owned description is passed to the shared session constructor.
        unsafe { Self::from_format_with_pixel_format(format, PLANAR_420) }
    }
    unsafe fn from_format_with_pixel_format(
        format: CFRef,
        pixel_format: u32,
    ) -> Result<Self, Error> {
        // SAFETY: format is a valid CMVideoFormatDescription; all created objects
        // are released on failure or in `Drop`.
        unsafe {
            let pixel_format: i32 = pixel_format as i32;
            let number = CFNumberCreate(
                ptr::null(),
                CF_NUMBER_SINT32,
                &pixel_format as *const i32 as *const c_void,
            );
            let keys = [
                kCVPixelBufferPixelFormatTypeKey,
                kCVPixelBufferMetalCompatibilityKey,
            ];
            let values = [number, kCFBooleanTrue];
            // Bi-planar NV12/P010 can be imported by the Metal adapter. Preserve
            // compatibility of the legacy three-plane CPU output.
            let attribute_count = if matches!(
                pixel_format as u32,
                P010_VIDEO | P010_FULL | NV12_VIDEO | NV12_FULL
            ) {
                2
            } else {
                1
            };
            let attributes = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                attribute_count,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            CFRelease(number);
            let hardware_keys =
                [kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder];
            let hardware_values = [kCFBooleanTrue];
            let specification = CFDictionaryCreate(
                ptr::null(),
                hardware_keys.as_ptr(),
                hardware_values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            if specification.is_null() {
                if !attributes.is_null() {
                    CFRelease(attributes);
                }
                CFRelease(format);
                return Err(Error(
                    "VideoToolbox: hardware specification allocation failed".into(),
                ));
            }
            let record = CallbackRecord {
                callback: Some(output),
                refcon: ptr::null_mut(),
            };
            let mut session = ptr::null();
            let code = VTDecompressionSessionCreate(
                ptr::null(),
                format,
                specification,
                attributes,
                &record,
                &mut session,
            );
            CFRelease(specification);
            if !attributes.is_null() {
                CFRelease(attributes);
            }
            if code != 0 || session.is_null() {
                CFRelease(format);
                return Err(status("session creation", code));
            }
            // SAFETY: the Copy API returns an owned CFBoolean for this property.
            // Requiring hardware prevents platform software fallback; query as
            // well so a successful session is never merely assumed accelerated.
            let mut accelerated = ptr::null();
            let code = VTSessionCopyProperty(
                session,
                kVTDecompressionPropertyKey_UsingHardwareAcceleratedVideoDecoder,
                ptr::null(),
                &mut accelerated,
            );
            let hardware =
                code == 0 && !accelerated.is_null() && CFBooleanGetValue(accelerated) != 0;
            if !accelerated.is_null() {
                CFRelease(accelerated);
            }
            if !hardware {
                VTDecompressionSessionInvalidate(session);
                CFRelease(session);
                CFRelease(format);
                return Err(Error(
                    "VideoToolbox: session did not confirm hardware acceleration".into(),
                ));
            }
            Ok(Self { format, session })
        }
    }
    /// Decode one length-prefixed access unit synchronously. `None` means the
    /// decoder produced no picture for it.
    pub fn decode(&mut self, packet: &[u8]) -> Result<Option<Planes>, Error> {
        self.decode_surface(packet)?
            .map(|surface| surface.download())
            .transpose()
    }
    /// Decode without locking or copying pixel memory. The returned surface
    /// remains valid after subsequent decode calls and after session destruction.
    pub fn decode_surface(&mut self, packet: &[u8]) -> Result<Option<Surface>, Error> {
        if packet.is_empty() {
            return Err(Error("VideoToolbox: empty access unit".into()));
        }
        let mut slot = Slot { result: Ok(None) };
        // SAFETY: the block buffer borrows `packet` (kCFAllocatorNull, never
        // freed by CoreMedia) only for this synchronous call, and `slot`
        // outlives the decode call that writes to it.
        unsafe {
            let mut block = ptr::null();
            let code = CMBlockBufferCreateWithMemoryBlock(
                ptr::null(),
                packet.as_ptr() as *mut c_void,
                packet.len(),
                kCFAllocatorNull,
                ptr::null(),
                0,
                packet.len(),
                0,
                &mut block,
            );
            if code != 0 || block.is_null() {
                return Err(status("block buffer", code));
            }
            let sizes = [packet.len()];
            let mut sample = ptr::null();
            let code = CMSampleBufferCreateReady(
                ptr::null(),
                block,
                self.format,
                1,
                0,
                ptr::null(),
                1,
                sizes.as_ptr(),
                &mut sample,
            );
            if code != 0 || sample.is_null() {
                CFRelease(block);
                return Err(status("sample buffer", code));
            }
            let mut info = 0u32;
            let code = VTDecompressionSessionDecodeFrame(
                self.session,
                sample,
                0,
                &mut slot as *mut Slot as *mut c_void,
                &mut info,
            );
            CFRelease(sample);
            CFRelease(block);
            if code == REFERENCE_MISSING {
                return Ok(None);
            }
            if code != 0 {
                return Err(status("decode", code));
            }
        }
        slot.result
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: both handles were created by this session and are released once.
        unsafe {
            VTDecompressionSessionInvalidate(self.session);
            CFRelease(self.session);
            CFRelease(self.format);
        }
    }
}

mod upload;

pub mod movie;

pub mod rate;
