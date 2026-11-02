#[allow(dead_code)]
pub fn trim(frames: u32, step_ms: f64, offset_ms: f64) -> Vec<u8> {
    let mut out = vec![];
    for value in [32u32, 1, frames * 16, frames] {
        out.extend(value.to_le_bytes());
    }
    // Native reserves a machine word; it is opaque, never a dereferenced pointer.
    out.extend(0x7fff12345678u64.to_le_bytes());
    let end = (frames - 1) as f64 * step_ms;
    for value in [0., end, offset_ms, offset_ms + end] {
        out.extend(value.to_le_bytes());
    }
    for i in 0..frames {
        let clip = i as f64 * step_ms;
        out.extend(clip.to_le_bytes());
        out.extend((clip + offset_ms).to_le_bytes());
    }
    out
}
