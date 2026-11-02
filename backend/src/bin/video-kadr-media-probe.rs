//! Isolated opaque libavformat probe. No private FFmpeg structure layouts are read.
use anyhow::{ensure, Context, Result};
use libloading::Library;
use std::{
    ffi::{c_char, c_int, c_void, CString},
    path::PathBuf,
};
type Open =
    unsafe extern "C" fn(*mut *mut c_void, *const c_char, *const c_void, *mut *mut c_void) -> c_int;
type Info = unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> c_int;
type Best =
    unsafe extern "C" fn(*mut c_void, c_int, c_int, c_int, *mut *const c_void, c_int) -> c_int;
type Close = unsafe extern "C" fn(*mut *mut c_void);
type Set = unsafe extern "C" fn(*mut *mut c_void, *const c_char, *const c_char, c_int) -> c_int;
type Free = unsafe extern "C" fn(*mut *mut c_void);
struct Input {
    pointer: *mut c_void,
    close: Close,
}
impl Drop for Input {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            unsafe { (self.close)(&mut self.pointer) }
        }
    }
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 2,
        "expected trusted libavformat and source paths"
    );
    let library = PathBuf::from(&args[0]);
    let source = PathBuf::from(&args[1]);
    ensure!(
        library.is_absolute() && source.is_absolute() && source.symlink_metadata()?.is_file(),
        "probe requires an absolute regular source and trusted library"
    );
    let url = CString::new(source.as_os_str().as_encoded_bytes())?;
    // The caller provides the trusted library paired with ffprobe. All ABI types
    // below are opaque pointers; the library remains loaded until Input drops.
    unsafe {
        let lib = Library::new(library).context("load trusted libavformat")?;
        let open: Open = *lib.get(b"avformat_open_input\0")?;
        let info: Info = *lib.get(b"avformat_find_stream_info\0")?;
        let best: Best = *lib.get(b"av_find_best_stream\0")?;
        let close: Close = *lib.get(b"avformat_close_input\0")?;
        let set: Set = *lib.get(b"av_dict_set\0")?;
        let free: Free = *lib.get(b"av_dict_free\0")?;
        let version: unsafe extern "C" fn() -> u32 = *lib.get(b"avformat_version\0")?;
        let mut input = Input {
            pointer: std::ptr::null_mut(),
            close,
        };
        let mut options = std::ptr::null_mut();
        ensure!(
            set(
                &mut options,
                c"protocol_whitelist".as_ptr(),
                c"file,pipe,fd,crypto,data".as_ptr(),
                0
            ) >= 0,
            "probe option failed"
        );
        let status = open(
            &mut input.pointer,
            url.as_ptr(),
            std::ptr::null(),
            &mut options,
        );
        free(&mut options);
        ensure!(status >= 0, "open video probe source failed: {status}");
        let status = info(input.pointer, std::ptr::null_mut());
        ensure!(
            status >= 0,
            "probe source stream information failed: {status}"
        );
        let index = best(input.pointer, 0, -1, -1, std::ptr::null_mut(), 0);
        ensure!(index >= 0, "no best video stream: {index}");
        println!("{index} {}", version());
    }
    Ok(())
}
