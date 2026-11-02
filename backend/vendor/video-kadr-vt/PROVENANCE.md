# Provenance

MIT adapter copied from local FVid `crates/fvid-vt`, checkout HEAD
`5e8814e76cc0760a90e8f2874cc8e3cccbc7554f`, on 2026-11-02.
The local source may contain uncommitted work; exact input hashes follow.
Original copyright and license are retained in LICENSE.

- `lib.rs` SHA-256: `c16efff3ceb295687fbb96584a64a277bcee83be902b1ae442e45d2dc85195eb`
- `encode.rs` SHA-256: `62b0d83f893a0171547800da7f3a7642d2066e39082b371f5bafe43d14cee9ce`

Changes: package renamed to video-kadr-vt; optional Metal integration and source-specific fixture tests omitted; decoder/encoder production code retained. Added upload.rs to create owned IOSurface-backed NV12 input using the checked video-kadr-hevc-frame CPU conversion. No path dependency on the sibling FVid checkout remains.

Validation on this Mac: `cargo +1.89.0 test --manifest-path backend/vendor/video-kadr-vt/Cargo.toml --test hevc_upload` passed. This requires actual hardware and fails rather than silently skipping if encoder creation is unavailable. It validates upload readback, HEVC encode/decode, studio-range primary luma, Main 8-bit geometry, hvcC, first keyframe and three rationally timed frames. It does not qualify container export, bitrate modes, cancellation or cross-platform availability.

Subsequent local changes: explicit CBR/capped-VBR configuration and checked streaming MP4/MOV writer with prepared PCM/AAC tracks. Encoder is Send for exclusive migration between host CPU workers; callback storage remains at a stable boxed address. Host tests exercise actual frame submission and audio muxing across those worker operations. Source hashes above remain the original copied input hashes, not hashes of the modified adapter.
