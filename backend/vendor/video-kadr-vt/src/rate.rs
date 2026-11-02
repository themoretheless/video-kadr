//! Explicit VideoToolbox rate control. Unsupported properties propagate errors.
use super::*;
#[derive(Clone, Copy, Debug)]
pub enum Mode {
    Cbr,
    Vbr { max_bps: u32 },
}
#[derive(Clone, Copy, Debug)]
pub struct RateControl {
    pub mode: Mode,
    pub target_bps: u32,
    pub rate_num: u32,
    pub rate_den: u32,
}
impl RateControl {
    pub fn validate(self) -> Result<(), Error> {
        if self.target_bps == 0
            || self.target_bps > i32::MAX as u32
            || self.rate_num == 0
            || self.rate_den == 0
            || u64::from(self.rate_num) > 1000 * u64::from(self.rate_den)
        {
            return Err(Error(
                "invalid HEVC rate-control target or frame rate".into(),
            ));
        }
        if let Mode::Vbr { max_bps } = self.mode {
            if max_bps < self.target_bps || max_bps > i32::MAX as u32 {
                return Err(Error(
                    "HEVC maximum bitrate must be at least target and fit i32".into(),
                ));
            }
        }
        Ok(())
    }
}
#[link(name = "VideoToolbox", kind = "framework")]
unsafe extern "C" {
    fn VTSessionSetProperty(session: CFRef, key: CFRef, value: CFRef) -> OSStatus;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeArrayCallBacks: [usize; 5];
    fn CFArrayCreate(
        allocator: CFRef,
        values: *const CFRef,
        count: isize,
        callbacks: *const c_void,
    ) -> CFRef;
}
struct Owned(CFRef);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) }
    }
}
fn number(value: f64) -> Result<Owned, Error> {
    let ptr = unsafe { CFNumberCreate(ptr::null(), 13, (&value as *const f64).cast()) };
    if ptr.is_null() {
        Err(Error("rate-control number allocation failed".into()))
    } else {
        Ok(Owned(ptr))
    }
}
fn set(session: CFRef, name: &str, value: CFRef) -> Result<(), Error> {
    let key = Owned(cfstr(name));
    if key.0.is_null() {
        return Err(Error("rate-control key allocation failed".into()));
    }
    let code = unsafe { VTSessionSetProperty(session, key.0, value) };
    if code != 0 {
        return Err(status(name, code));
    }
    Ok(())
}
pub(super) fn apply(session: CFRef, control: RateControl) -> Result<(), Error> {
    control.validate()?;
    let fps = number(f64::from(control.rate_num) / f64::from(control.rate_den))?;
    set(session, "ExpectedFrameRate", fps.0)?;
    let target = number(f64::from(control.target_bps))?;
    match control.mode {
        Mode::Cbr => set(session, "ConstantBitRate", target.0)?,
        Mode::Vbr { max_bps } => {
            set(session, "AverageBitRate", target.0)?;
            // VideoToolbox specifies [bytes, seconds], not bits/second.
            let bytes = number(f64::from(max_bps) / 8.0)?;
            let seconds = number(1.0)?;
            let values = [bytes.0, seconds.0];
            let array = unsafe {
                CFArrayCreate(
                    ptr::null(),
                    values.as_ptr(),
                    2,
                    (&kCFTypeArrayCallBacks as *const [usize; 5]).cast(),
                )
            };
            if array.is_null() {
                return Err(Error("rate-control limits allocation failed".into()));
            }
            let array = Owned(array);
            set(session, "DataRateLimits", array.0)?;
        }
    }
    Ok(())
}
