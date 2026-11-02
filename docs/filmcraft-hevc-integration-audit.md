# HEVC native integration audit — 2026-11-02

Status: implementation pending; HEVC must remain unavailable in the native format catalog.

Inspected checkout: video-kadr HEAD `0a97f8887ae38920cffe74fce5e8b8791562996a`; FVid HEAD `5e8814e76cc0760a90e8f2874cc8e3cccbc7554f`. Both worktrees may contain uncommitted changes; these hashes alone do not reproduce their current files.

## Source contract

Pinned Filmcraft `5231852443363f001c3f6b396dd9b1e6461ae2be`, `crates/export/src/lib.rs`, defines HEVC through registered platform encoder factories/probes. It does not supply a built-in HEVC encoder. Its validation rejects two-pass HEVC. `settings.rs` exposes Main hardware encoding, target/max bitrate, CBR/VBR, keyframe interval, and MP4/MOV selection.

## Reusable implementation and actual gap

FVid `crates/fvid-vt/src/encode.rs` exports `EncoderCodec::Hevc`, `Encoder::new_with_bitrate`, `set_colour`, and `encode_with_keyframe`. Output includes length-prefixed compressed samples, exact hvcC bytes, parameter sets, keyframe flag, rational PTS and duration. Session creation requires confirmed hardware acceleration; frame reordering is disabled; each submitted frame is synchronously completed. Drop invalidates the compression session.

This is not yet a usable RGBA export adapter. `Encoder::encode` consumes `fvid_vt::Surface`, whose retained CoreVideo storage is private. Surface creation currently comes from decoder output; there is no public host RGBA/NV12 upload constructor. Encoder submission additionally requires an IOSurface backing. Passing ordinary CPU RGBA bytes or advertising availability merely because this crate exists would be incorrect.

The existing bitrate method sets AverageBitRate. This does not establish strict CBR or a maximum bitrate constraint. Forced keyframes are possible, but the adapter must explicitly schedule them from the resolved interval. Main10 support in FVid does not establish Filmcraft HDR export parity.

The local `backend/vendor/filmcraft-isobmff/src/codec.rs` already provides `HevcConfig::parse` and `SampleEntry::hevc`, so a second container muxer is unnecessary.

## Implementation sequence and evidence required

1. Add an owned IOSurface-backed NV12 upload boundary with checked geometry, strides, buffer lengths, range/color conversion, allocation errors, and lifetime tests. Keep unsafe platform bindings in a dedicated crate. Avoid modifying the sibling FVid checkout as a hidden dependency.
2. Resolve CBR/VBR and maximum bitrate against actual VideoToolbox properties; reject unsupported settings explicitly. Schedule keyframes and validate returned PTS/duration/configuration.
3. Connect the adapter to the existing guarded native writer, using hvcC from actual encoded output. Reuse PCM/AAC, source authorization, no-overwrite publication, deadline and cancellation/drain ownership.
4. Probe by creating and encoding a real supported surface; only then expose HEVC on supported macOS hosts. Other platforms must report unavailable until they have a tested encoder.
5. Qualify MP4/MOV, rational frame rates, packet timing, independent decode/color checks, audio priming, quality/rate controls, keyframe boundaries, saved presets/quick/queue, cancellation, hardware failure, owner access, and existing-destination preservation. Add UI controls only for admitted settings.

No HEVC availability or parity status was promoted by this audit. No runtime HEVC encoding test has run yet.

## Implemented upload preparation

`backend/hevc-frame` now implements checked opaque RGBA8 to limited-range BT.709 NV12 conversion, 2x2 chroma averaging before quantization, fallible allocation and stride-aware plane copies. Both mapped destinations are validated before any write; row padding remains unchanged. The type owns its buffers and prevents constructing inconsistent dimensions externally. Nonopaque input is rejected: the renderer must perform its existing black composition first. This is a CPU preparation boundary, not an encoder or an availability probe.

Validation: `cargo +1.89.0 test --manifest-path backend/hevc-frame/Cargo.toml` — 4 passed, 0 failed. Tests cover independently tabulated primary/black/white codes, mixed 2x2 chroma, size overflow, alpha rejection, invalid lengths/strides, padding preservation and no partial writes on destination validation failure. CoreVideo allocation, upload and real HEVC encoding remain pending.

## Hardware upload and encoder proof

`backend/vendor/video-kadr-vt` now owns a local MIT copy of the FVid adapter (source hashes and modifications in PROVENANCE.md). `Surface::from_nv12` allocates IOSurface-backed studio-range NV12, checks retained surface geometry, locks exclusively for row-wise plane copies, and releases locks/references through RAII. No sibling repository dependency is used.

The physical-Mac `hevc_upload` integration test passed: CPU frame upload/readback, hardware HEVC Main encoding, actual decode, hvcC/parameter sets, first keyframe, three frame PTS and 1001/30000 duration. This resolves the missing upload prerequisite. Native guarded MOV/MP4 writer integration and the remaining settings/ownership/cancellation qualification are still pending; format availability remains unchanged.

## Streaming container implementation

`video_kadr_vt::movie::HevcMovie<W: Write + Seek>` now streams opaque CPU-rendered frames to HEVC Main MP4 or MOV through the existing Filmcraft ISO BMFF muxer. Track initialization uses the actual encoder hvcC from the first packet. It verifies unchanged decoder configuration, first/requested sync frames, exact rational PTS/duration, checks timestamp overflow and poisons the writer after any error. Empty or failed exports cannot finish successfully. The generic output permits the host's guarded file abstraction; this component does not publish paths itself.

Hardware integration checks `hevc_movie` (2 passed) and `hevc_upload` (1 passed) validate both containers with ffprobe and decode all three frames independently with FFmpeg, checking RGB color, hvc1, dimensions, BT.709 tags, frame count and 30000/1001 timing. External tools are used only as test oracles; production encoding/muxing runs in Rust/VideoToolbox.

This movie writer currently has video only and an average bitrate target. Host export jobs, PCM/AAC, CBR/maximum-rate controls, presets/UI/queue and cancellation/publication integration remain open. HEVC catalog availability has not been enabled.

## Prepared audio integration

The movie writer now accepts a source-prepared PCM/AAC track before its first video frame and streams audio packets after hvcC initialization. Movie timescale follows audio sample rate; caller-supplied AAC media_start is retained. Declared media duration is checked packet-by-packet and again before finish. PCM packet byte counts must match their declared frame count; malformed packets poison the writer.

`hevc_audio` passed 3 hardware tests: byte-exact PCM24 six-channel sample recovery; byte-exact packets from the pinned Rust AAC encoder at 44.1 kHz, preserving priming edit and 1764-sample presentation duration; malformed PCM packet refusal. Audio encoding/preparation remains the caller's existing responsibility. Application job wiring and its authorization/cancellation/publication gates are still pending.

## Explicit rate-control implementation

Added `RateControl` and `Mode::{Cbr,Vbr { max_bps }}` to the owned platform adapter, plus `HevcMovie::new_with_rate_control`. CBR sets ConstantBitRate and ExpectedFrameRate, without combining incompatible AverageBitRate/DataRateLimits. VBR sets AverageBitRate and a one-second DataRateLimits window expressed as bytes/seconds. Property and allocation failures propagate; no fallback to another mode occurs. Targets/frame rates/maxima are validated before creating a session.

The contract was checked against the installed macOS SDK VideoToolbox `VTCompressionProperties.h`: ConstantBitRate is incompatible with AverageBitRate/DataRateLimits; ExpectedFrameRate is recommended for CBR; DataRateLimits is an array of byte counts and time windows. Dynamic CFString property keys avoid requiring newer constant symbols at load time. Unsupported older systems still fail at property assignment.

`hevc_rate` passed two tests, including physical hardware creation/encoding for both modes at 30000/1001. A 30-frame static CBR stream exhibits padding; the capped VBR stream stays below the configured short-stream byte limit. This is basic functional evidence, not a full sliding-window/high-motion rate-control qualification. Combined export, audio, upload and rate suites: 8 passed, 0 failed. Application export-job integration remains pending.

## Host guarded exporter integration

The backend now depends on the owned VideoToolbox adapter on macOS and exposes an internal `RasterEncoding::NativeHevc` route. It enters the existing native export lifecycle and uses GuardedFile, the shared CPU pool/scoped physical drain, source-prepared audio, NativeAac, deadline/cancellation checks, same-directory staging and hard-link publication without overwrite. Non-macOS native initialization refuses HEVC. The session is movable between exclusive CPU tasks; its callback targets stable boxed mutex storage and Drop invalidates the session before releasing it.

`backend/tests/filmcraft_hevc.rs` passed 2 actual host tests: guarded MOV/PCM24 and MP4/AAC export at 44.1 kHz with exact presentation duration; cancellation followed by physical CPU drain, staging cleanup, peer-file preservation and existing-destination refusal. The test disables any usable FFmpeg path and uses the shared host CPU pool, exercising encoder ownership across worker operations.

No HTTP preset, saved-settings mapping, UI option or format-catalog availability has been enabled yet. The current internal route explicitly refuses non-square pixels and supplied metadata rather than losing those settings. These restrictions must be implemented or surfaced during request admission before exposing the format. Full HEVC/app parity remains incomplete.

Final host regression: `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo +1.89.0 test --manifest-path backend/Cargo.toml --test filmcraft_hevc --test filmcraft_mjpeg` — 6 passed, 0 failed. Log: `/tmp/kadr-hevc-host-regression.log`. This is a focused native-export regression, not the whole backend suite.

## HTTP, catalog, presets and panel

HEVC is now admitted through authenticated native jobs, saved preset resolution, Quick and MCP catalog. Catalog and admission use an actual CPU-pool hardware encode probe. Source HEVC two-pass settings are rejected before job creation. Metadata and pixel aspect are retained. The panel offers HEVC Main, MP4/MOV, existing audio controls and CBR/VBR; H.264 profiles are hidden and stale two-pass state blocks submission.

Final host/API/MCP tests: 6 passed, 0 failed (`/tmp/kadr-hevc-api-tests-final.log`), including real default exports for all fourteen formats and owner-only downloads. Frontend panel/project: 130 passed; queue: 3 passed; type check: 0 errors and 0 warnings; scoped ESLint passed. No full-suite or live-browser equivalence claim. Other-platform HEVC, distribution and exhaustive profile/settings/rate-window behavior remain unqualified. Earlier pending statements above record milestone history and are superseded by this section.
