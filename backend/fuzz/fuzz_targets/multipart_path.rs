#![no_main]

use libfuzzer_sys::fuzz_target;
use video_kadr_backend::project_archive::safe_archive_filename;

fuzz_target!(|data: &[u8]| {
    if let Ok(filename) = std::str::from_utf8(data) {
        let accepted = safe_archive_filename(filename);
        if accepted {
            let path = std::path::Path::new(filename);
            assert_eq!(path.components().count(), 1);
            assert!(!path.is_absolute());
        }
    }
});
