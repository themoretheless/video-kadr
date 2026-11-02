# FilmCraft: source-backed feature audit ledger

Snapshot: `5231852443363f001c3f6b396dd9b1e6461ae2be`. Generated from actual `command_specs()`, `effect_defs()`, `Format::ALL`, `builtin_presets()` and transition tree. The Rust emitter compiled and executed against the reference using Rust1.98.1. Default feature set only. MCP tools/package features/doc headings are inventoried from source, not executed.

[Complete machine-readable ledger](filmcraft-catalog.json) contains every captured command id/menu/shortcut/parameter contract, effect kind/category/default/range/animation flag, export preset settings,47 package manifests and1064 tracked-file hashes. These are inventory and audit prompts;2695 entries are not2695 proven independent product capabilities. Declaration indexing is a lightweight source index, not an exhaustive Rust AST/API qualification.

Status: **1029 partial,9 missing,1657 unreviewed,0 verified by this ledger.** This does not assert that all unreviewed items are absent in video-kadr. Existing host behavior needs operation-specific comparison. Similar names/endpoints and upstream registry presence cannot count as parity. Source FilmCraft itself includes approximations/unsupported paths; compare its actual implemented behavior and limitations, not the aspirational Premiere roadmap.

| Catalog | Count |
|---|---:|
| Engine commands |675|
| Video effects |130|
| Audio effects |58|
| Video transitions |112|
| Audio transitions |3|
| Effect/transition parameters |1426|
| Export formats |14|
| Built-in export presets |30|
| Source MCP tools |17|
| Package build features |30|
| Documentation sections to review |200|

## Command groups

| Group | Commands |
|---|---:|
|audio|5|
|captions|22|
|clip|57|
|clipMixer|4|
|color|1|
|command|1|
|edit|37|
|effects|13|
|essentialSound|9|
|events|2|
|export|19|
|file|56|
|fonts|1|
|graphics|82|
|help|1|
|history|1|
|jobs|2|
|lumetri|7|
|lut|4|
|markers|31|
|masks|11|
|media|16|
|mediaBrowser|10|
|mediaCache|2|
|metadata|2|
|mixer|20|
|multicam|35|
|perf|1|
|playhead|14|
|prefs|4|
|presets|7|
|project|38|
|scopes|1|
|sequence|44|
|shortcuts|16|
|source|5|
|state|1|
|timeline|55|
|transcript|14|
|trim|24|

## How to reproduce

`python3 scripts/audit-filmcraft-catalog.py --reference /path/to/filmcraft --toolchain 1.98.1`

Use a toolchain meeting the source's declared Rust1.95 minimum. The tool creates a temporary standalone emitter crate; it does not edit the reference. `--runtime-catalog /path/to/catalog.json` can reuse an earlier emitter result, but only a rebuild establishes runtime provenance independently. `--check` compares against the saved ledger without writing it. No automatic upstream command execution or product mutation occurs.

The generator refuses a different reference commit, tracked reference changes, wrong runtime schema/provenance, duplicate requirement ids, unknown overrides and nonexistent host evidence. `docs/filmcraft-parity-overrides.json` is the explicit mapping/review input. A `verified` override requires a source contract, verification command, scope, existing result artifact and evidence paths. Those fields still require human/agent review of what they prove; their presence alone cannot certify equivalent behavior. A fully green ledger would still need integration/performance/crash/media/visual qualification at the requested scope.

Fresh rebuild and deterministic saved-ledger comparison PASS for the final mappings (21partial/2674unreviewed).

Verifier regression: `python3 -m unittest discover -s scripts/tests -p test_filmcraft_catalog.py -v` (4PASS). Covers inventory never auto-promoting presence, duplicate/unknown ids, missing qualification, wrong snapshot and missing host evidence. No application code changed in this audit stage, so prior application test counts remain prior-stage evidence.

## Remaining source contracts discovered concretely

- Unified Lumetri includes60 parameters. Current SDR curves/looks/wheels/LUT account for19 partially mapped parameters; basic controls, creative tint/faded-film/sharpen/vibrance, integrated section toggles/ordering, HDR, HSL key/refine and vignette need qualification or implementation.
- Source `lumetri.setLook` selects a LUT reference/path, not the procedural look-menu index. Its host command parity remains unreviewed; the procedural-look inspector is not evidence for this command.
- LUT workflow commands are import/list/remove/export plus Input LUT/Creative Look selection and Color Match. Current inline imported effect does not cover shared library/builtins, large-project storage or those full command workflows.
- Effect tree includes112 video transitions and58 audio effects. Existing generic fade/effects or PCM mix cannot establish that catalog's behavior.
- All14 export formats include APV, TIFF/BMP sequences, AIFF and MXF OP1a/OPAtom; existing export job capabilities require direct format/settings/codec/timing verification.
-675 commands span graphics/templates, captions/transcripts, media browser/metadata, project management/relink/proxy/cache, mixer/inserts/sends, multicam, trim/edit/source monitors, shortcuts/workspaces/settings, automation and journaling. Each contract and source implementation/test must be compared before closing its entry.

[Grouped implementation history and limitations](filmcraft-parity.md) remains useful, while this ledger supplies the concrete list for the continuing full transfer. The combined WeftCut/FilmCraft goal remains open.


## Stage 389 — HSL Secondary actual media export and ledger refresh

Extended real rawvideo/NUT source → native nested composition → FFV1/Matroska → decoded RGBA fixture with HSL hue-shift animation at 0/500000us, nested instance split at250000us and White/Black mask with maximum denoise/blur. Exact expected pixels pass for opaque/translucent source, red child background and half-opacity group. Both unsplit and split animation preserve the original child clock. This qualifies those fixtures only, not upstream render equivalence or HDR. Existing curves/wheels/LUT portions also pass in the same export test.

Four color inspector files / nine DOM tests PASS; catalog verifier four tests PASS; diff check PASS. Added fourteen HSL parameter mappings as explicitly partial, refreshed and checked pinned source/runtime ledger:35partial /2660unreviewed /0verified. Source-render oracle, full frontend/API regression, preview-resolution scaling and unified Lumetri/HDR remain open. No full-feature completion claim.


## Stage 394 — authenticated color preview and broad application regression

Extended existing actual original-video API preview fixture with HSL Secondary and ColorVignette authored via wire JSON. The source clip starts at500000us; keys at local0/500000us verify effect clocks relative to clip placement (rather than global timeline). Exact decoded preview pixels pass for half-alpha blue over red background, hue-shifted opaque red, neutral vignette, dark corner and preserved center. Invalid static hueShift181 and feather-1 are rejected BAD_REQUEST even with animation present, confirming full authored admission rather than sampled-only acceptance.

Full updated API integration70PASS. Full frontend132files/834testsPASS with maxWorkers1. Previous stage393 backend library619PASS3ignored remains latest library evidence (this stage changed only API tests/evidence). Ledger refresh/check and diff check PASS; statuses remain40partial/2655unreviewed/0verified. These API fixtures use real authorized backend/FFmpeg, while component frontend tests mock transport; combined authenticated browser E2E, HDR and unified Lumetri order are still unqualified. Full feature goal remains active.


## Stage401 — authenticated latest color effect preview and full regression

Extended actual original-video API preview with BasicGrade saturation100→0, CreativeAdjust fadedFilm0→100 and ColorSharpen amount0→100. Clip placement begins500000us; key times0/500000us verify local effect sampling. Exact pixels qualify half-alpha source/background and opaque second-frame results. Invalid authored exposure5.1, shadowRed1.1 and sharpen100.1 are rejected even with valid animated overrides. Sharpen API fixture is a spatially constant source: transport/admission coverage only; patterned sharpening math is separately covered by stage400 upstream oracle/actual FFV1 fixture.

Updated full API70PASS; full frontend135files/837testsPASS (maxWorkers1); ledger regeneration/check and diff checkPASS. Latest full library631PASS3ignored remains stage400 evidence, as this stage changed API tests/docs only. Ledger56partial/2639unreviewed/0verified. Combined authenticated browser E2E, unified float Lumetri/HDR and whole-feature parity remain unqualified. Source processing contract for the next unified integration is documented in filmcraft-parity.md; goal remains active.


## Stage409 — authenticated shared float color pipeline regression

Extended real original-video authenticated API preview with two0.9% faded-film corrections separated by neutral vignette/looks/curves/LUT and disabled HSL. Exact opaque source pixelRGB[1,1,254] confirms sub-byte correction accumulation survives all these native section boundaries through API admission/authorized decoder/render/response; byte-rounding per section would lose the red/green correction. Existing single-effect/clip-local animation/range refusal/alpha preview fixtures retained.

Full API70PASS after shared frame changes; explicit updated preview regression rerunPASS (69other tests filtered), with fresh Cargo source-state check and no recompilation needed beyond1.89s. Latest full library639PASS3ignored remains stage408 evidence; frontend837PASS remains stage401 evidence because no frontend code changed in float stages. Ledger regeneration/check and diff checkPASS;56partial/2639unreviewed/0verified. Full authenticated browser E2E, unified single Lumetri combined-stage oracle/order/clamping, HDR and remaining feature catalog remain incomplete. Full goal stays active.


2026-11-02 export workflow audit: seven preset-library commands and Quick Export
now have explicit partial status backed by native storage/HTTP/UI and regression
files. Nine commands remain missing: export.queue.{add,list,start,stop,cancel,
retry,remove,move,clear}. Their source contracts were inspected in the pinned
engine/export_tools.rs; independent background jobs/history do not supply the
serial queue. Quick Export native/exact-range and full regression qualification passed; queue core tests are pending.
The catalog generator and --check pass against the unchanged pinned runtime
inventory and 1064 source hashes. Counts classify ledger entries, not independent
product features or a completion percentage.
