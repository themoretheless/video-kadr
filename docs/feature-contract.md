# Feature contract: CapCut / Insta360 / FCP / Resolve parity wave

Single source of truth for the parallel feature build-out. Every agent codes
against this document. Nothing here may be renamed by a feature agent; if a
shape is wrong, report it instead of drifting.

> **Status: shipped.** The wave landed. Section 9 records where the shipped code
> deviates from the plan below and why; read it before trusting a detail here.

## 0. Ground rules

- Every new wire field is **optional** and defaults to absent/empty. An
  `EditRequest` that omits all of them must serialize byte-identically to today,
  so existing render-cache keys and every existing test stay valid. Use
  `#[serde(default, skip_serializing_if = "...")]` on all of them.
- `EditRequest` keeps `deny_unknown_fields`. `camelCase` on the wire.
- Wire types live in `backend/src/model.rs`. Validated domain types live in
  `backend/src/domain/edit.rs` (or a new `backend/src/domain/<area>.rs`).
  Wire → domain conversion validates: every `f64` must be `is_finite()` and
  range-clamped, every string length-capped, every collection size-capped.
- Asset references are **ids, never paths and never bytes**. The HTTP adapter
  resolves an id to a private filesystem path before the FFmpeg adapter runs,
  exactly like `LutSelection` does today.
- Collection caps: `clips` ≤ 200, `overlays` ≤ 32, `titles` ≤ 32,
  `audioTracks` ≤ 8, any keyframe track ≤ 64 points, any text ≤ 512 chars.

## 1. Shared primitives

```jsonc
// Keyframe on the OUTPUT timeline, seconds.
Keyframe = { "t": 0.0, "v": 1.0, "interp": "hold" | "linear" | "smooth" }
// A track is a Keyframe[] sorted by t, len 0..=64. Empty = parameter unused.

// Normalized frame coordinates: 0..1 relative to the output frame.
// Sizes are also normalized; a null height means "keep source aspect".

Rgb = { "r": 0.0, "g": 0.0, "b": 0.0 }        // wheel offsets/multipliers
Color = "#RRGGBB"                              // validated hex, no alpha
```

Rust: reuse `domain::keyframes::{Keyframe, KeyframeTrack, Interpolation,
FfmpegKeyframeAdapter}` (already present, currently unused).
TypeScript: `frontend/src/types.ts` gets matching `Keyframe`, `KeyframeTrack`.

## 2. Wire additions to `EditRequest`

```jsonc
{
  // ---- multi-source composition (CapCut/FCP core) ----
  // When non-empty, `clips` replaces videoId+segments as the timeline source.
  // videoId stays required (first clip's source / backwards compat).
  "clips": [{
    "sourceId": "vid_...",          // library media id
    "start": 0.0, "end": 12.5,      // in-point/out-point in the SOURCE
    "speed": 1.0,                   // 0.25..4
    "volume": 1.0,                  // 0..4
    "muted": false,
    "transitionIn": { "kind": "fade", "duration": 0.5 } // see §3, null on clip 0
  }],
  // Transition applied between plain `segments` when `clips` is absent.
  "segmentTransition": { "kind": "fade", "duration": 0.5 },

  // ---- overlays: watermark, logo, PiP, green screen ----
  "overlays": [{
    "assetId": "ast_...",           // kind image|video asset
    "kind": "image" | "video",
    "x": 0.05, "y": 0.05,           // normalized top-left of the overlay
    "width": 0.25, "height": null,  // normalized; null height keeps aspect
    "opacity": 1.0, "rotation": 0.0,
    "start": 0.0, "end": null,      // output-timeline seconds; null = to end
    "fadeIn": 0.0, "fadeOut": 0.0,
    "chromaKey": { "color": "#00FF00", "similarity": 0.12, "blend": 0.05 },
    "audio": { "enabled": false, "volume": 1.0 }   // kind=video only
  }],

  // ---- titles / lower thirds ----
  "titles": [{
    "text": "Заголовок",
    "fontAssetId": null,            // null = bundled default font
    "fontSize": 48,                 // px at 1080p, scaled to output height
    "color": "#FFFFFF",
    "x": 0.5, "y": 0.85,            // anchor point, normalized
    "align": "center" | "left" | "right",
    "box": { "color": "#000000", "opacity": 0.5, "padding": 12 },
    "borderWidth": 0.0, "borderColor": "#000000",
    "shadowX": 0.0, "shadowY": 0.0, "shadowColor": "#000000",
    "start": 0.0, "end": null, "fadeIn": 0.0, "fadeOut": 0.0,
    "animation": "none" | "fade" | "slide-up" | "typewriter" | "pop"
  }],

  // ---- subtitles ----
  "subtitles": {
    "assetId": "ast_...",           // .srt or .vtt asset
    "burnIn": true,
    "fontSize": 24, "color": "#FFFFFF", "outlineWidth": 2,
    "position": "bottom" | "top", "marginV": 40
  },

  // ---- audio mixing ----
  "audioTracks": [{
    "assetId": "ast_...",
    "role": "music" | "voiceover" | "sfx",
    "gain": 1.0,                    // 0..4
    "start": 0.0,                   // where it lands on the OUTPUT timeline
    "sourceStart": 0.0,             // in-point inside the asset
    "end": null,
    "loop": false,
    "fadeIn": 0.0, "fadeOut": 0.0,
    "ducking": { "enabled": true, "threshold": 0.05, "ratio": 8,
                 "attack": 20, "release": 300 }
  }],
  "audioDynamics": {
    "denoise": 0.0,                 // 0..1 -> afftdn nr
    "dereverb": false,
    "compressor": { "threshold": -18, "ratio": 3, "attack": 20,
                    "release": 250, "makeup": 1.0 },
    "limiter": { "ceiling": -1.0 },
    "gate": { "threshold": -45, "ratio": 2 },
    "deesser": false,
    "highpassHz": null,             // overrides the legacy `highpass` bool
    "lowpassHz": null,
    "bitrateKbps": 128,             // 64..320
    "volumeEnvelope": []            // Keyframe[]; multiplies `volume`
  },

  // ---- animated transform (Ken Burns, animated reframe) ----
  "motion": {
    "zoom": [],                     // Keyframe[]; 1.0 = fit, >1 = punch in
    "panX": [], "panY": [],         // Keyframe[]; -1..1 of the frame
    "rotation": []                  // Keyframe[]; degrees
  },
  "speedRamps": [],                 // Keyframe[]; v = speed multiplier 0.25..4

  // ---- Insta360 / action-cam ----
  "reframe360": {
    "inputProjection": "equirect" | "fisheye" | "dfisheye",
    "outputProjection": "flat" | "equirect" | "fisheye" | "stereographic" | "pannini",
    "fov": [], "yaw": [], "pitch": [], "roll": [],   // Keyframe[] degrees
    "outputWidth": 1920, "outputHeight": 1080,
    "horizonLock": true
  },
  "stabilize": {
    "mode": "off" | "fast" | "precise",   // fast=deshake, precise=vidstab 2-pass
    "smoothing": 10,                       // 1..100
    "zoom": 0.0,                           // 0..20 %
    "horizonLock": false
  },
  "lensCorrection": { "k1": 0.0, "k2": 0.0 },

  // ---- advanced color (Resolve-lite) ----
  "colorAdvanced": {
    "temperature": 0.0, "tint": 0.0,      // -1..1
    "exposure": 0.0,                       // -2..2 stops
    "highlights": 0.0, "shadows": 0.0,     // -1..1
    "lift":  { "r": 0.0, "g": 0.0, "b": 0.0 },   // -0.5..0.5
    "gamma": { "r": 1.0, "g": 1.0, "b": 1.0 },   // 0.1..4
    "gain":  { "r": 1.0, "g": 1.0, "b": 1.0 },   // 0..4
    "hsl": [{ "band": "red"|"orange"|"yellow"|"green"|"cyan"|"blue"|"magenta",
              "hue": 0.0, "saturation": 1.0, "luminance": 1.0 }]
  }
}
```

## 3. Transitions

```jsonc
Transition = { "kind": <id>, "duration": 0.5 }   // 0.05..3.0 seconds
```
`kind` ∈ `fade`, `wipeleft`, `wiperight`, `wipeup`, `wipedown`, `slideleft`,
`slideright`, `slideup`, `slidedown`, `circleopen`, `circleclose`,
`dissolve`, `pixelize`, `radial`, `smoothleft`, `smoothright`, `zoomin`.
These map 1:1 onto FFmpeg `xfade=transition=<kind>`; audio uses `acrossfade`.
A transition consumes `duration` seconds of overlap from both neighbours; the
plan's expected output duration must account for it.

## 4. Assets API (new, owned by the foundation agent)

```
POST   /api/assets      multipart: file + kind=image|audio|video|font|subtitle
                        -> 201 { id, kind, filename, mime, sizeBytes, sha256,
                                 width?, height?, duration? }
GET    /api/assets      -> { assets: [...] }
GET    /api/assets/:id  -> metadata
DELETE /api/assets/:id  -> 204
```
- Stored under `storage/assets/<id>.<ext>`, served read-only at `/files/assets/`.
- Ids match `^ast_[a-zA-Z0-9]{16,}$`; validated with the same allow-list rules
  the LUT store uses. Content is sniffed (magic bytes / bounded `ffprobe`),
  never trusted from the client extension or MIME header.
- Body cap 64 MiB for audio/video/image, 4 MiB for font/subtitle.
- `AssetStore` lives in `backend/src/assets.rs`; handlers in
  `backend/src/handlers/assets.rs`. It exposes
  `fn resolve(&self, id: &str, expected: AssetKind) -> Option<PathBuf>` which
  every render feature uses to turn an id into a path.

## 5. Backend render module layout

`args.rs` stays the entry point but delegates. The foundation agent creates
`backend/src/render/graph/` with a **multi-input filter_complex builder** and
stub modules; each feature agent fills exactly one file and nothing else.

```
backend/src/render/graph/mod.rs        // ComplexPlan + stage ordering (foundation)
backend/src/render/graph/overlays.rs   // agent: overlays  (image/video PiP, chroma key)
backend/src/render/graph/titles.rs     // agent: titles    (drawtext + subtitles)
backend/src/render/graph/audio_mix.rs  // agent: audio     (amix, sidechain, dynamics)
backend/src/render/graph/motion.rs     // agent: motion    (zoompan/rotate, speed ramps)
backend/src/render/graph/spatial.rs    // agent: spatial   (v360, vidstab, lenscorrection)
backend/src/render/graph/transitions.rs// agent: transitions + multi-clip concat
backend/src/render/graph/color.rs      // agent: color     (wb, lift/gamma/gain, HSL)
```

`ComplexPlan` (foundation-owned, in `graph/mod.rs`):

```rust
pub struct InputSpec { pub path: PathBuf, pub loop_still: bool, pub seek: Option<f64> }

pub struct ComplexPlan {
    inputs: Vec<InputSpec>,     // index 0 is always the primary source
    statements: Vec<String>,    // filter_complex statements, joined with ';'
    video_label: String,        // current terminal video pad, starts "0:v"
    audio_label: Option<String>,// current terminal audio pad, starts "0:a"
}

impl ComplexPlan {
    pub fn add_input(&mut self, spec: InputSpec) -> usize;  // returns input index
    pub fn next_label(&mut self, hint: &str) -> String;     // unique pad name
    pub fn push(&mut self, statement: String);
    pub fn chain_video(&mut self, filters: &[String], hint: &str); // [cur]f[new]
    pub fn chain_audio(&mut self, filters: &[String], hint: &str);
    pub fn video_label(&self) -> &str;
    pub fn audio_label(&self) -> Option<&str>;
    pub fn set_video_label(&mut self, label: String);
    pub fn set_audio_label(&mut self, label: Option<String>);
    pub fn render(&self) -> String;      // the -filter_complex value
    pub fn is_trivial(&self) -> bool;    // no extra inputs, no statements
}
```

Stage order in `graph/mod.rs::compose` (fixed, do not reorder):

1. `transitions::apply` - builds the clip/segment concat with xfade
2. `spatial::apply` - 360 reframe, lens correction, stabilization
3. (existing) geometry: censor, crop, rotate, flip, denoise
4. `color::apply` - white balance, exposure, wheels, HSL (before LUT/curves)
5. (existing) look preset, curves, LUT mix
6. `motion::apply` - keyframed zoom/pan/rotate, speed ramps
7. (existing) scale, sharpen, vignette, grain, pad
8. `overlays::apply` - image/video overlays, chroma key
9. `titles::apply` - drawtext titles, then subtitles burn-in
10. (existing) fades
11. `audio_mix::apply` - extra tracks, ducking, dynamics (audio branch)

Each feature module exposes exactly:

```rust
pub fn apply(plan: &mut ComplexPlan, spec: &EditSpec, ctx: &RenderContext)
    -> anyhow::Result<()>;
```

`RenderContext` (foundation-owned) carries resolved asset paths, source probe
info (`width`, `height`, `duration`, `has_audio`, `fps`), and output duration.
When the relevant spec field is absent, `apply` returns `Ok(())` untouched -
that is what keeps every existing test green.

## 6. Frontend module layout

Foundation creates these as **stubs**; each feature agent owns exactly one
store module and one panel component.

```
frontend/src/store/assets.ts        // foundation: upload/list/delete assets
frontend/src/store/composition.ts   // agent: timeline  (clips, split, ripple, markers)
frontend/src/store/overlays.ts      // agent: text      (overlays, titles, subtitles)
frontend/src/store/audioMix.ts      // agent: audio
frontend/src/store/motion.ts        // agent: motion    (keyframes, speed ramps)
frontend/src/store/spatial.ts       // agent: spatial   (360, stabilize, lens)
frontend/src/store/colorAdvanced.ts // agent: color

frontend/src/components/edit/TimelinePanel.vue      // agent: timeline
frontend/src/components/edit/TextOverlayPanel.vue   // agent: text
frontend/src/components/edit/AudioMixPanel.vue      // agent: audio
frontend/src/components/edit/MotionPanel.vue        // agent: motion
frontend/src/components/edit/SpatialPanel.vue       // agent: spatial
frontend/src/components/edit/ColorGradePanel.vue    // agent: color
```

Every store module exports the same four symbols (names differ only by prefix):

```ts
export const <name>State = reactive({ ... })         // module-local state
export function reset<Name>(): void                  // back to defaults
export function <name>Payload(): Record<string, unknown>  // {} when inactive
export function apply<Name>Snapshot(raw: unknown): void   // validated restore
```

`frontend/src/domain/edit.ts::buildEditPayload` merges every module payload:
`Object.assign(body, compositionPayload(), overlaysPayload(), ...)`. Modules
return `{}` when they have nothing to contribute, so a default project produces
today's exact payload. `hasMeaningfulChanges` ORs in each module's activity.
Undo/redo snapshots and project autosave include the module states.

`EditPanel.vue` renders the six panels as collapsible sections. Foundation wires
the sections once; feature agents never touch `EditPanel.vue`.

## 7. Definition of done for every agent

- `cd backend && cargo fmt` clean, `cargo clippy --all-targets -- -D warnings`
  clean, `cargo test` green.
- `cd frontend && npm run lint && npm run typecheck && npm run test` green.
- New behaviour covered by unit tests in the module's own `#[cfg(test)]` block
  or its own `*.test.ts` file. Assert on emitted FFmpeg strings, not on
  end-to-end renders.
- No `unwrap()`/`expect()` on user input, no path or shell interpolation of
  untrusted strings. `drawtext` text and every filter path must be escaped.
- Comments match the density and English style of surrounding code.
- No em dashes anywhere.

## 8. Wiring the stages together (integration, done)

- `services/render.rs::map_request` builds `EditExtensions::from_request` and
  attaches it with `EditSpec::with_extensions`, so every wire field reaches the
  spec.
- `RenderResources` carries an `assets: BTreeMap<String, PathBuf>` next to the
  LUT path. `handlers/mod.rs::resolve_edit_assets` fills it from
  `AssetStore::resolve` for overlays, title fonts, subtitles and audio tracks,
  and from `tools::find_source` for every distinct `clips[].sourceId`.
  `args.rs::compose_graph` copies the map into `RenderContext`.
- `graph::compose` calls `overlays::apply_with_audio` and threads the returned
  `Vec<OverlayAudioPad>` into `audio_mix::apply_with_overlays`, so PiP audio is
  mixed rather than dropped.
- `args.rs` uses `audio_mix::{bitrate_argument, legacy_highpass_filter,
  master_tail_filters}`. `loudnorm` now runs before the `volume` slider, which
  changes the emitted args for edits that set both `normalizeAudio` and a
  non-default `volume`.
- `ComplexPlan` gained `mark_timeline_consumed()` / `timeline_consumed()`.
  `transitions::apply` sets it; `args.rs` uses it instead of comparing the video
  label to `0:v`, and suppresses the input-side `-ss`/`-t` when a stage owns the
  timeline. `transitions::apply` also stitches plain `segments` whenever any
  non-timeline extension is active, so cuts can no longer be silently dropped.
- `CompiledExportCommand` gained `prepass: Option<CompiledPrepass>`. The job
  runner registers a per-job `vidstab-<outputId>.trf` under
  `spatial::TRANSFORMS_CONTEXT_KEY`, runs `tools::run_ffmpeg_prepass` first, and
  removes the file afterwards.
- `SourceMediaMetadata`/`SourceMediaSpec` carry the probed source frame rate
  (`fps_milli`, skipped when absent). Composed stages need the source rate, not
  the output override.
- `handlers/mod.rs::validate_parity_capabilities` rejects a request whose
  FFmpeg filter is missing from the local build, with a Russian message, before
  a job is queued.

## 9. Where the shipped code differs from this document

1. **Per-keyframe `interp` is not honoured.** `domain::keyframes::KeyframeTrack`
   carries one interpolation per track; `domain::edit::keyframe_track` takes it
   from the first keyframe and ignores the rest. The wire shape in section 1 is
   unchanged, but only `points[0].interp` has any effect.
2. **`buildEditPayload` does not import the store modules.** `eslint.config.js`
   forbids `src/domain/**` importing `**/store*`. The six-way merge lives in
   `store/modules.ts::featureModulePayload()`, and `store.ts` passes the merged
   object into `domain/edit.ts::buildEditPayload(edit, video, modulePayload)`.
3. **HSL band `orange` renders as nothing.** `huesaturation` selects only
   r/y/g/c/b/m. The band stays on the wire but the colour stage emits no filter
   for it, and the UI does not offer it.
4. **HSL saturation/luminance saturate at 2.0.** `huesaturation` takes a shift
   in -1..1, so the documented 0..4 range is only honoured up to 2x.
5. **`ComplexPlan` has two more fields** than section 5 lists: a private
   `label_sequence` (uniqueness of `next_label`) and `timeline_consumed`.
   `chain_video`/`chain_audio` return `anyhow::Result<()>`.
6. **`spatial::prepass_command` takes the plan too** (`(&ComplexPlan, &EditSpec,
   &RenderContext)`): the source path only exists in `plan.inputs()[0]`.
7. **`motion` uses `zoompan`, not `scale`+`crop` expressions.** A filter link
   carries one fixed frame size, so only `zoompan` accepts a zoom expression.
   It re-stamps output at a constant rate, so the chain forces `fps` first.
8. **`speedRamps` keyframe times are SOURCE-timeline seconds**, not output
   timeline: `setpts`'s `T` is the input timestamp, and reading them on the
   output timeline would be circular. A `smooth` ramp segment compiles with the
   linear-speed form (the cubic ease has no elementary integral).
9. **`stabilize.mode=precise` refuses a cut timeline.** The transform file is
   frame-indexed, so `clips`/`segments` return `PreciseNeedsUncutTimeline`.
10. **`reframe360.horizonLock` zeroes roll** rather than levelling against IMU
    metadata, which the wire does not carry.
11. **2.39:1 padding is sent as `98:41`.** `AspectRatio::parse` takes `u32:u32`
    with both sides <= 100.
12. **`subtitles.burnIn: false` is a no-op.** Soft subtitle output would need
    `-c:s`/`-map` changes that are out of scope.
13. **No font is bundled.** Titles without a `fontAssetId` resolve a host font
    via `VIDEO_EDITOR_DEFAULT_FONT` or a candidate list, and fail with
    `DefaultFontMissing` when none exists.
14. **Multi-source clips are normalized to one size, rate and timebase**
    (`scale`+`pad`+`fps`+`setsar=1`+`settb=AVTB`), because `xfade` refuses
    inputs that disagree on any of the three.
15. **`deshake` radii are quantized to multiples of 16**, which FFmpeg requires.
16. **Bundle budgets were re-baselined.** `scripts/check-bundle-budget.mjs` now
    checks the initial (entry) chunk separately from the total, since the six
    editor panels are lazily loaded.
