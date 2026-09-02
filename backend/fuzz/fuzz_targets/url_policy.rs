#![no_main]

use libfuzzer_sys::fuzz_target;
use video_kadr_backend::tools::validate_url_structure;

fuzz_target!(|data: &[u8]| {
    if let Ok(url) = std::str::from_utf8(data) {
        let _ = validate_url_structure(url);
    }
});
