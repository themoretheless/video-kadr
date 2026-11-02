//! Browser preview and HTTP sampling share the exact product color evaluator.
#[path = "../../src/domain/animated_color.rs"]
mod animated_color;
pub use animated_color::{color_command_target, Color, ColorExportFrames, ColorSampleRequest, PreparedColorTrack};

/// Strict JSON admission and sampling, shared by the host-independent ABI.
pub fn sample_json(input: &[u8]) -> Result<Vec<u8>, String> {
    let request = serde_json::from_slice::<animated_color::ColorSampleRequest>(input)
        .map_err(|error| error.to_string())?;
    let colors = animated_color::sample_colors(&request)?;
    serde_json::to_vec(&serde_json::json!({ "colors": colors })).map_err(|error| error.to_string())
}

// No host pointers or unsafe memory views. The host uploads bounded JSON bytes
// then copies response bytes synchronously before the next resident request.
#[cfg(target_arch = "wasm32")]
mod abi {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    const MAX_BYTES: usize = 2 * 1024 * 1024;
    thread_local! {
        static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        static TRACKS: RefCell<BTreeMap<u32,super::PreparedColorTrack>> = const { RefCell::new(BTreeMap::new()) };
        static TRACK: RefCell<Option<super::animated_color::PreparedColorTrack>> = const { RefCell::new(None) };
        static PACKED: RefCell<u32> = const { RefCell::new(0) };
        static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }
    #[no_mangle]
    pub extern "C" fn color_begin() {
        INPUT.with(|input| input.borrow_mut().clear());
        OUTPUT.with(|output| output.borrow_mut().clear());
    }
    #[no_mangle]
    pub extern "C" fn color_push(byte: u32) -> u32 {
        INPUT.with(|input| {
            let mut input = input.borrow_mut();
            if byte > 255 || input.len() >= MAX_BYTES {
                return 1;
            }
            input.push(byte as u8);
            0
        })
    }
    #[no_mangle]
    pub extern "C" fn color_sample() -> u32 {
        let result = INPUT.with(|input| super::sample_json(&input.borrow()));
        let (status, bytes) = match result {
            Ok(bytes) => (0, bytes),
            Err(error) => (1, error.into_bytes()),
        };
        OUTPUT.with(|output| *output.borrow_mut() = bytes);
        status
    }
    #[no_mangle]
    pub extern "C" fn color_prepare() -> u32 {
        TRACK.with(|track| *track.borrow_mut() = None);
        let result = INPUT.with(|input| {
            serde_json::from_slice(&input.borrow())
                .map_err(|error| error.to_string())
                .and_then(super::animated_color::PreparedColorTrack::new)
        });
        match result {
            Ok(value) => {
                TRACK.with(|track| *track.borrow_mut() = Some(value));
                0
            }
            Err(error) => {
                OUTPUT.with(|output| *output.borrow_mut() = error.into_bytes());
                1
            }
        }
    }
    #[no_mangle]
    pub extern "C" fn color_at(time: f64) -> u32 {
        let result = TRACK.with(|track| {
            track
                .borrow()
                .as_ref()
                .ok_or_else(|| "Color track is not prepared".to_string())
                .and_then(|track| track.sample(time))
        });
        match result {
            Ok(color) => {
                PACKED.with(|packed| {
                    *packed.borrow_mut() = u32::from_be_bytes([color.r, color.g, color.b, color.a])
                });
                0
            }
            Err(error) => {
                OUTPUT.with(|output| *output.borrow_mut() = error.into_bytes());
                1
            }
        }
    }
    #[no_mangle]
    pub extern "C" fn color_track_prepare(id: u32) -> u32 {
        // Replacing an ID first invalidates its old track, including failures.
        TRACKS.with(|tracks| tracks.borrow_mut().remove(&id));
        if id == 0 {
            OUTPUT.with(|output| *output.borrow_mut() = b"Color track ID must be nonzero".to_vec());
            return 1;
        }
        let result = INPUT.with(|input| {
            serde_json::from_slice(&input.borrow())
                .map_err(|error| error.to_string())
                .and_then(super::PreparedColorTrack::new)
        });
        match result {
            Ok(track) => {
                TRACKS.with(|tracks| tracks.borrow_mut().insert(id, track));
                0
            }
            Err(error) => {
                OUTPUT.with(|output| *output.borrow_mut() = error.into_bytes());
                1
            }
        }
    }
    #[no_mangle]
    pub extern "C" fn color_track_drop(id: u32) {
        TRACKS.with(|tracks| tracks.borrow_mut().remove(&id));
    }
    #[no_mangle]
    pub extern "C" fn color_track_count() -> u32 {
        TRACKS.with(|tracks| tracks.borrow().len() as u32)
    }
    #[no_mangle]
    pub extern "C" fn color_track_at(id: u32, time: f64) -> u32 {
        let result = TRACKS.with(|tracks| {
            tracks
                .borrow()
                .get(&id)
                .ok_or_else(|| "Color track is not prepared".to_string())
                .and_then(|track| track.sample(time))
        });
        match result {
            Ok(color) => {
                PACKED.with(|packed| {
                    *packed.borrow_mut() = u32::from_be_bytes([color.r, color.g, color.b, color.a])
                });
                0
            }
            Err(error) => {
                OUTPUT.with(|output| *output.borrow_mut() = error.into_bytes());
                1
            }
        }
    }
    #[no_mangle]
    pub extern "C" fn color_packed() -> u32 {
        PACKED.with(|packed| *packed.borrow())
    }
    #[no_mangle]
    pub extern "C" fn color_output_len() -> u32 {
        OUTPUT.with(|output| output.borrow().len() as u32)
    }
    #[no_mangle]
    pub extern "C" fn color_output_byte(index: u32) -> u32 {
        OUTPUT.with(|output| {
            output
                .borrow()
                .get(index as usize)
                .copied()
                .map(u32::from)
                .unwrap_or(256)
        })
    }
}
