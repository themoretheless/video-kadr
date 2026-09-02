#![no_main]

use libfuzzer_sys::fuzz_target;
use video_kadr_backend::handlers::render_cache_key;
use video_kadr_backend::model::EditRequest;

fuzz_target!(|data: &[u8]| {
    if let Ok(request) = serde_json::from_slice::<EditRequest>(data) {
        let key = render_cache_key(&request);
        assert_eq!(key.len(), 64);
        assert!(key.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
});
