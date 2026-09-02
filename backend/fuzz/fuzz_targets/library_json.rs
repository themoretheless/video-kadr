#![no_main]

use libfuzzer_sys::fuzz_target;
use video_kadr_backend::library::MediaEntry;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<Vec<MediaEntry>>(data);
});
