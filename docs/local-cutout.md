# Local object background removal

Current implementation produces a transparent still PNG with the pinned IS-Net model. The native runtime runs on the server CPU; it does not send media to a remote inference service. The source RGB and original dimensions are retained; output alpha multiplies the original alpha. An uncorrected blank mask retains the original image. Still-image and ordinal video-mask brushes, portable project masks, temporal SAM tracking and immediate region continuation are available as detailed below. Sections record successive implementation layers; later workflow sections supersede earlier stage-specific limitations.

## Host configuration

Set all three absolute paths together:

- `CUTOUT_BIN`: built `video-kadr-vision-runtime` executable.
- `CUTOUT_OBJECT_MODEL`: `isnet-general-use.onnx`, 178648008 bytes, SHA256 `60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a`.
- `CUTOUT_ORT_LIBRARY`: compatible ONNX Runtime dynamic library for this host.

No setting means unavailable. Partial/relative settings refuse startup. A configured path is not an installed or verified model: status and admission verify file identity. Weights are not bundled. The editor model manager can download the pinned IS-Net weights, verify their size/SHA, register them for the signed-in owner and persist selection for new jobs. The host executable and ONNX Runtime library still require server configuration; this does not install the runtime. The configured model path may be missing when a verified managed model is selected. Source image admission is bounded to 256 MiB and 64 Mi pixels; the native decoder supports its compiled image formats.

## Authenticated API

- `GET /api/cutout/models`: owner library, pinned public catalog and current selection.
- `POST /api/cutout/models/select` with `{ "modelId": "isnet-general-use" }` or `configured`: select for new jobs.
- `POST /api/cutout/models/remove` with `{ "modelId": "isnet-general-use" }`: remove owner registration/selection; shared weights remain while another owner or retained request needs them.
- `GET/POST /api/cutout/models/downloads`: owner-scoped durable download history or submit `{ "catalogId": "isnet-general-use" }`; generic job observation/cancel/retry applies. Size/hash verification precedes registration. Removal advances an owner epoch so an older download cannot undo removal by registering late.
- `GET /api/cutout/status`: availability, engine and supported media kind.
- `POST /api/cutout/jobs` with `{ "sourceId": "registered-source-id" }`: returns HTTP202 and `jobId`; never accepts filesystem/model paths.
- `GET /api/cutout/jobs?sourceId=...&limit=20&cursor=...`: owner-scoped keyset history; source filter optional, limit1–50.
- Existing `GET /api/jobs/:id`, `POST /api/jobs/:id/cancel`, retry/discard routes use cutout owner/source authorization.
- `GET /api/cutout/jobs/:id/mask`: private original512×512 grayscale probability PNG for new completed jobs, encoded/raw digest and current source ACL checked, no-store. Legacy jobs without a saved sidecar return404. This works without a currently installed inference engine.
- `GET /api/cutout/jobs/:id/image`: completed private PNG, current source authorization and output integrity required, `Cache-Control: no-store`.
- `POST /api/cutout/jobs/:id/asset` with `{ "projectId": "project-id" }`: explicit project-editor acceptance; reserves a stable source ID and stores an independent private project-space PNG. Repeating acceptance reuses the ID and verifies the existing bytes. Project space members can read accepted project media according to normal media permissions; private job output remains job-owner-only.

Analysis resource admission, durable outbox/lease, progress, cancellation and managed shutdown/restart use the existing job system. Recovery rechecks captured source and executable/model/runtime identity and source access; changed invariants fail rather than silently running against another source/model. A recovered incomplete run recomputes the mask; no incremental inference checkpoint exists.

A ready job or accepted asset does not automatically edit the timeline. The image clip inspector and native timeline panel expose “Удалить фон изображения”, explicit engine check, generation/progress/cancel, paginated source history, preview and “Применить обработанную копию”. Application retains the original and uses one replacement transaction/Undo; existing original/copy controls switch the authored source. Observation disconnect/unmount does not cancel server work. Account/source/target changes abort observation and suppress late results; an intervening document edit retains the accepted asset in the library without applying it to the changed document. Hard-kill/multiprocess publication recovery, complete original Concat decoder equivalence, perceptual quality corpus, GPU and long/high-resolution/platform qualification remain open.

## Native correction protocol groundwork

The isolated `--object` schema3 request optionally accepts `maskOutput` (absolute new grayscale PNG path) and `correction` with `custom`, `strokes` and `feather`. Each stroke contains `tool` (`brush`, `eraser`, `smartBrush`, `smartEraser`), normalized `size`, source-fraction `points` and optional source-second `at`. Source-canonical sizes0.005–0.5, points-0.5–1.5 and feather0–0.1 are admitted. `maskHash` still identifies the automatic raw probability bytes; optional `maskFileHash` identifies the grayscale PNG and `resolvedMaskHash` the painted/feathered bytes. The --object CLI uses automatic-confidence fallback and refuses embedded learned-region metadata; retained learned regions use the separate --mask replay protocol.

New server jobs persist and verify this raw-mask sidecar, with owner-only retrieval. Existing older jobs may lack it. The editor exposes brush corrections below. The --object correction path still performs inference; the --mask path below reuses a saved mask. Durable correction jobs and portable project provenance are described below. Normal publication errors roll back the new sidecar, but the two files are not an atomic transaction under abrupt termination.

## Cached correction execution

Native `--mask` schema4 accepts `input`, `mask`, `output`, `sourceSha256`, `maskFileSha256`, `maskSha256` and `correction`. It uses the original image and saved automatic mask without loading the model or ONNX Runtime. Reapplying keep strokes can restore original alpha that a previous cutout removed. Source/mask identities are checked before editing and before publication. Output includes canonical `correctionHash` and `resolvedMaskHash` alongside PNG/source/mask digests.

The host `local_mask_edit` adapter verifies original metadata/mask, captures the executable identity and handles process progress, cancellation/deadline, PNG integrity and source identity. Authenticated correction routes below now supply source/owner authorization and durable execution. The editor brush controls below now use these routes; accepted copies also retain authored project stroke metadata. The adapter alone does not authorize source access or edit a project. Existing raw mask/history routes remain the preceding interface.


## Durable correction jobs

`POST /api/cutout/jobs/:id/corrections` accepts the complete replacement recipe `{ "custom": true, "strokes": [...], "feather": 0.01 }`. The referenced job must be a completed cutout owned by the caller, with a verified saved automatic mask and current source access. Requests never accept file/model/runtime paths. The configured native executable is required; model/ORT paths may be unavailable.

The returned job ID uses existing status/progress/cancel/retry/discard routes, source-filtered cutout history, `/image`, `/mask` and explicit `/asset` project acceptance. `GET /api/cutout/jobs/:id/correction` returns durable `baseJobId` and the captured full correction recipe, including after execution/restart. An original job without a correction recipe returns404 here. The settings remain job records; accepting the resulting image saves authored brush controls and registered original/mask references into the project document.

Each edit owns a separate copy of the original automatic probability mask and reapplies the complete recipe to the original source image. A second edit does not multiply a previously cut alpha again. Use `custom:false,strokes:[],feather:0` to reconstruct the automatic result; supplied strokes replace prior strokes rather than appending implicitly. Completed derived masks survive later damage to a parent mask. Queued/running edits recheck captured source, base-mask identity, executable identity and permissions on managed restart. Stale inputs fail with no accepted output. The result's model/runtime fields describe the original probability-mask generation; editing uses only the executable.


## Brush authoring in the editor

A ready cutout preview exposes “Правка маски” → “Открыть полотно маски”. The authenticated `/api/cutout/jobs/:id/original` endpoint supplies a bounded oriented PNG (up to1024 pixels per axis, no enlargement), verifies the captured source and current permissions and returns no-store. Source coordinates cover the complete image independently of the clip's transform/crop.

Draw keep/erase or confidence-based smart-fallback strokes with pointer capture; diameter is a source-width fraction. Arrow keys move the cursor and Enter/Space add a point. Stroke Undo/Redo, discard, automatic-mask reset and feather controls edit a local draft. Apply/generation/history selection are guarded while the draft differs from the saved recipe. “Пересчитать маску” submits the complete recipe as a new durable job and reconnects the existing progress/cancel/private preview flow. The resulting copy still needs explicit application to the clip, which uses canonical retained-original replacement and one project Undo.

Loading a corrected job through source history restores its complete recipe; equality is canonical and independent of JSON key order or omitted/null stroke time. Account/job changes abort pending reads/submission observations, suppress late results and release source Blob URLs. Closing the panel does not cancel admitted work. Smart tools apply original mask-confidence thresholds by default. The SAM section can infer and attach a learned region to a smart stroke. The brush UI currently restores strokes from owner job history. Applied copies also contain portable project provenance; the project-context brush also opens the saved attachment directly from an applied copy.


## Saved project masks and correction admission

Acceptance returns `cutout.snapshot`, `cutout.source` and `cutout.mask` in addition to the processed media fields. The snapshot carries registered source IDs, source/raw-mask/encoded-mask/model/runtime hashes and the complete correction recipe. Input and independently owned512x512 mask assets survive archive export/import, with source IDs rewritten. Applying all references is one project Undo.

`POST /api/composition-projects/:id/cutout/corrections` accepts `{ "clipId": "picture", "compositionId": "root", "correction": { "custom": false, "strokes": [], "feather": 0 } }`. Omit compositionId for an ordinary document; nested documents require an explicit timeline. The caller must be a project owner/editor. The server reads the attachment from the saved document rather than trusting client hashes, checks current source/mask access and bytes, captures the executable and persists a durable correction job. The same project clip attachment, permissions and file identities are rechecked at execution and before success. No original inference job, model or ONNX Runtime is required. An unsaved or changed attachment must be saved before submission.

The result uses the existing private job image/mask/settings and explicit project acceptance routes. Changing the project recipe while a job runs invalidates that captured project correction. The ordinary inspector and native image panel also open project-context editing directly from the applied copy. GET `/api/composition-projects/:id/cutout/:clip` reads the saved attachment and dimensions; append `/original` for a bounded no-store original PNG, with optional `?compositionId=root` on both reads. The brush compares this saved attachment with the selected clip and sends `expectedAttachment` with correction admission. Changed saved settings are refused with409. Save the project after applying a new copy before reopening its project-based mask. Owner job-history editing remains available. Imported-project browser editing is verified on the synthetic qualification image; collaborators and broader export/quality matrices remain open.


Project-mask collaboration qualification uses a second project editor with no access to the initial owner job. It verifies actual correction and project acceptance, queued attachment/access changes refusing without artifacts, and managed queued recovery with restored exact source pixels. Completed jobs remain private to their submitting editor until explicit project acceptance. Browser collaboration, in-progress project-origin process recovery and abrupt publication faults remain unqualified.


## Native learned region groundwork

`video-kadr-vision-runtime --region /absolute/request.json` accepts schemaVersion5, absolute `encoder`, `decoder`, `runtimeLibrary`, `input`, `output`, `sourceSha256` and nonempty source-fraction `points`. SlimSAM encoder/decoder files must match the pinned inventories documented in concat-parity.md. It publishes a private256x256 grayscale region PNG, source/model/runtime/prompt/raw-mask/output hashes and progress0..3. Source/model/runtime identities are rechecked before publication. Source image is oriented with the existing bounded native decoder.

The library supports reusing an embedding across multiple strokes. The CLI currently handles one region per process. Authenticated source jobs and editor controls expose this inference; accepted regions persist as project media and replay without models. A SAM download manager, embedding batching and video tracking remain unfinished.


`services::local_sam::region` now supplies the server-side process adapter for `--region`. It accepts only server-owned Config paths and captured source hash/points, verifies pinned model inventories plus executable/runtime identities, handles bounded execution/progress/cancellation through ProcessRuntime, verifies the private grayscale PNG and raw digest, then publishes without replacement after input/engine rechecks. Caller still owns source/project authorization and Analysis admission. This adapter is exercised with real pinned models; authenticated durable region routes, model lifecycle wiring and brush integration are pending.


## Authenticated learned-region jobs

Configure all four absolute host paths together: `SAM_BIN`, `SAM_ENCODER_MODEL`, `SAM_DECODER_MODEL`, `SAM_ORT_LIBRARY`. With none set, the feature is disabled; partial or relative configurations are rejected. The pinned models must already exist on the server.

- `GET /api/sam/status`: authenticated availability check.
- `POST /api/sam/jobs`: `{ "sourceId": "library-image", "points": [[0.5, 0.5]] }`; returns202/jobId. No model/file paths accepted.
- `GET /api/sam/jobs?sourceId=...&cursor=...&limit=20`: paginated owner history (limit1–50).
- `GET /api/sam/jobs/:id/points`: retained sourceId/complete points, including after restart and completion.
- `GET /api/sam/jobs/:id/mask`: verified private256x256 grayscalePNG/no-store after Done; no active model configuration needed to read.
- Existing `/api/jobs/:id` and cancel/retry/discard routes apply SAM owner/source authorization.

Jobs capture source and engine identities, use Analysis admission and durable outbox/leases, and recheck access and content before execution and success. Managed queued and in-progress recovery are tested with actual models. Stale sources, revoked access or changed engine fail with no result file. These are still source-region jobs: smart brush submission, portable region attachment, project-context SAM editing and model manager are pending.


`POST /api/sam/jobs/:id/asset` with `{ "projectId": "project-id" }` explicitly saves a learned region into private project-Space media. It returns the flat image MediaEntry, `source` original MediaEntry and validated `region` provenance (source/mask IDs, hashes, model pair/runtime, complete points and prompt hash). Job owner/source access and project editor rights are required; project Space members can read accepted media under normal media ACL. Repeating acceptance returns the same independently verified file, even if the temporary job PNG was later damaged. The job/settings records and original source access are still needed for this route.

Smart strokes store this provenance in their optional region field. Correction admission resolves and verifies region media under current source ACL. Accepting a corrected cutout copies region dependencies into the target project Space; archives register and rebind these dependencies. Accepting a standalone region does not itself replace a clip.


`--mask` schema4 now optionally accepts `regions`, aligned with every correction stroke: each slot is null or `{ "path": "/absolute/region.png", "fileSha256": "...", "maskSha256": "..." }`. A supplied region requires a smart tool and verified256x256 grayscalePNG. Files and raw probabilities are verified before painting; encoded bytes are rechecked before publication. The result includes ordered `regionHashes` only when the regions array is nonempty. No region models/runtime are loaded for replay.

Omitting regions preserves the existing protocol and confidence fallback. Host correction jobs populate aligned server-owned paths from validated stroke region references, rechecking source authorization and raw/encoded identities. Project acceptance and archive import preserve dependencies and rewrite media IDs.


`local_mask_edit::correct_with_regions` now exposes region replay to host callers. Supply an aligned array of optional server-resolved RegionInput records. Attachment source hash and exact points must match the original image/stroke; only smart tools accept a region. Private region PNG and raw/encoded identities are verified before launch and before output publication, and returned ordered hashes must match. Existing `correct` remains a compatibility wrapper with no regions. The host caller owns media authorization and ID-to-path resolution.

Standalone vision JSON now uses float_roundtrip like the host, so rational source coordinates survive requests and preserve canonical prompt/settings hashes. This fixes a real interoperability failure without quantizing points. Authenticated correction jobs and brush/project serialization still need wiring to the new host entry point.


## SAM brush workflow

Configure all four absolute SAM paths described above. Open the original image mask canvas, choose a smart tool, draw a stroke, expand “Выделение объекта SAM”, check availability and select “Выделить объект по штриху”. The private preview shows the learned probability region. “Пересчитать маску” creates a corrected copy; applying it retains the original and commits through normal project Undo. History can reuse an accepted source region without fresh model inference. Closing the panel aborts observation, not the durable job.

Actual native editor-component browser qualification uses CUTOUT_TEST_SAM=1 with scripts/qualify-cutout-live.mjs and the documented synthetic37x23 input. It checks real SAM/IS-Net requests, stored stroke provenance, correction, project acceptance/save and RGBA Undo/Redo without route mocks. This is not full-App playback, photographic quality or video segmentation qualification.

## SAM model download primitive

`services::sam_model` defines the pinned SlimSAM77-uniform pair at Xenova revision5850ab45f587c112167512ffef949107115e26a0. The bundle contains vision_encoder.onnx (23276014bytes) and prompt_encoder_mask_decoder.onnx (16557892bytes), with the same exact SHA identities required by local_sam. Download uses the existing bounded SHA-checked per-file cache; complete files can be reused offline, incomplete staging never counts as an installed pair, and bundle verification rejects extra entries/symlinks/corruption. The caller must serialize its private staging directory. This primitive does not yet provide authenticated installation, owner selection/removal or editor model controls.

## Owner-managed SAM installation

The SAM brush section now includes model catalog/download progress/history, account selection and removal controls. Authenticated GET /api/sam/models lists installed owner registrations and the pinned pair; POST /api/sam/models/downloads admits an Ingest job, while GET lists owner history. POST /api/sam/models/select and /remove take modelId. New region jobs capture selected model_id in their durable work; legacy snapshots default to configured server models. Execution resolves the captured ID and checks full engine identity rather than consulting later selection. Generic job reads/cancel/retry/discard authorize SAM download ownership.

Host SAM executable and ORT still need configuration. Managed selection replaces encoder/decoder paths only. Downloads verify a private staged pair before publication and registration. Removal advances the owner epoch, resets selection and retains files needed by another owner, a configured path or retained region-job payload. Network installation and recovery/browser qualification results are recorded in the parity ledger; the controls alone do not establish those gates.

Managed-restart evidence now covers queued and in-transfer model downloads plus queued/running selected-model inference. Region requests retain the admitted model ID across selection reset/removal; removed files stay pinned until the request reaches terminal success, then normal model-list collection can release them. Recovery recreates AppState/DB/library after orderly task shutdown. Abrupt kill/power-loss and partial-file byte-range resume are not established by these tests.

## Isolated Selfie still protocol

Native `--selfie /absolute/request.json` uses schemaVersion6 and the same host-owned model/runtime/input/output/sourceSha256 fields as object cutout, with optional maskOutput and correction. It requires the pinned462352byte Selfie model documented in docs/licenses/selfie-model-NOTICE.md. Outputs preserve original dimensions/RGB and multiply original alpha by the resolved256square probability mask; an uncorrected blank person mask preserves the original picture. Optional raw-mask PNG and source/model/runtime/raw/encoded digests support later replay. Embedded learned-region strokes are refused and must use the separate cached protocol.

This is an isolated executable protocol. Server Selfie configuration and authenticated jobs are connected below; project/editor selection and video/RVM integration remain unfinished. The ordinary object route remains schema3/IS-Net.

## Selfie ProcessRuntime adapter

`services::local_selfie` accepts server-owned Config/Input and calls isolated --selfie through ProcessRuntime with one operation deadline, cancellation/reaping and private staging. It validates the pinned model, source/runtime/executable identity, output dimensions/RGB/source-alpha contract and optional256grayscale mask's raw/encoded identity before no-replace publication. `read_verified_mask` rechecks encoded/raw digests and blank-mask metadata before reuse. RGB/alpha decode verification shares local_cutout::verify_pixels.

Configure the complete absolute SELFIE_BIN/SELFIE_MODEL/SELFIE_ORT_LIBRARY triple through AppConfig/main/AppState. The authenticated Selfie service described below owns source authorization and Analysis admission. An editor model selector is not yet connected.


## Durable private Selfie jobs

Authenticated GET /api/selfie/status checks actual configured engine inventory. POST /api/selfie/jobs takes only sourceId and admits an Analysis job; GET lists owner/source-filtered history with cursor/limit. GET /api/selfie/jobs/:id/image, /mask and /original return private no-store PNGs after owner/source-access checks and byte validation. The original authoring preview is oriented and bounded to1024pixels; completed image preserves full source dimensions. Generic jobs endpoints provide authorized status/cancel/retry/discard. Clients cannot submit model/runtime/output paths.

Work captures source and engine identity; execution and publication recheck current source access/hash. Outbox/lease/recovery uses the existing common job mechanism. Managed queued and in-progress recovery is tested, including a2048x1024frame; changed source, revoked membership and changed engine refuse without published image/mask. Completed reads require source access but do not require models to remain configured. Project acceptance and model-free256-mask attachment/correction are connected below; editor selection and source-mask controls are connected below; broader photographic/video qualification remains unfinished.


## Selfie project acceptance and saved-mask correction

POST /api/selfie/jobs/:id/asset takes projectId, requires the job owner plus target-project editor access, and reuses the normal generated-media reservation/copy path. It returns an independent image copy, original source, project-owned256mask and cutout provenance. Repeated acceptance retains IDs. Explicit project/native/frontend validation expects256only for the pinned Selfie model hash; other existing masks retain512requirements.

POST /api/selfie/jobs/:id/corrections accepts the usual correction recipe and creates a common CutoutEdit job. Its image/mask/asset APIs use /api/cutout/jobs/:newId. Cached native --mask/schema4 accepts optional maskSize256or512; omission defaults512 for existing clients. The host derives it from verified original metadata. Project-context correction resolves the saved snapshot and requires only the executable; a missing Selfie model does not prevent replay. This supports backend project persistence/re-editing, a Selfie editor selector and the browser workflow below.


## Selfie editor workflow

In image background removal select “Человек — Selfie”, check availability and create a copy. Initial person jobs use the Selfie endpoints; after a brush correction the panel follows the common correction job/image/asset endpoints. The mask canvas reads the original from the proper family. History in Selfie mode merges person-inference jobs and common corrections with separate pagination cursors. Changing source/account/selection invalidates pending observations through the existing scope guard.

Actual native editor-component browser qualification uses CUTOUT_TEST_SELFIE=1, CUTOUT_TEST_BRUSH=1 and CUTOUT_TEST_PORTABLE=1. It covers real person-model admission, keyboard/mouse strokes, recipe restore, acceptance, native RGBA Undo/Redo, project save, archive import into a new Space and correction from the imported256mask. Synthetic input does not establish photographic segmentation quality or complete App/player/movie-export behavior.

## Native recurrent person-matting core

`vision-runtime::person_matting::PersonSegmenter` loads the pinned RVM MobileNetV3 FP32 pair-free model described in docs/licenses/rvm-model-NOTICE.md. `begin()` starts a new source run. `mask(image, ordinal, cancel)` accepts consecutive ordinals from zero, stable source dimensions and bounded RGBA data. Working dimensions match original Concat:512long edge, nearest multiples of16, minimum64per edge. RGB planes are0..1 and internal downsample_ratio remains1.

Four verified finite recurrent tensors carry between frames. Refused/cancelled calls retain the previous tensors/next ordinal, so the same frame can be retried; successful calls commit alpha/state/ordinal together. Cancellation is checked around ORT inference; this core does not interrupt an already running kernel. Test-only instrumentation verifies cancellation after entry to inference does not commit state. Native raw alpha and all four recurrent tensors match the pinned original implementation bit-for-bit on a short changing synthetic sequence.

This library core now has the streaming child protocol below; authenticated video jobs, movie encoding, project acceptance and editor controls remain unfinished. No photographic video quality/performance claim follows from the synthetic sequence oracle.


## RVM raw sequence child protocol

Native `--person-sequence /absolute/request.json` accepts schemaVersion7, pinned model/runtimeLibrary, raw RGBA input/output, sourceSha256, width/height/frames and optional maskOutput. One loaded PersonSegmenter processes consecutive source frames from ordinal0. RGBA output keeps original dimensions (including odd sides), RGB and source alpha multiplied by the learned matte; blank masks preserve the source frame. Optional maskOutput stores unencoded probability bytes at the working dimensions for every frame.

Metadata binds source/model/runtime/output/mask hashes, original/working dimensions, counts/byte lengths and blankFrames. Counts are1..360000; source/frame dimensions and total raw image/mask staging each have the existing64GiB limits. Input bytes/hash are checked before and during processing and again before no-replace publication, alongside model/runtime rechecks. Child outputs are intended to stay inside host-owned staging until a supervising adapter validates them. SIGTERM cannot run Rust destructors; the host must reap and remove owned staging, as tested by the standalone qualifier.

The real qualifier scripts/qualify-person-sequence.py uses generated original Concat RVM+apply oracle artifacts and compares all four nonblank masks and RGBA frames exactly, including optional sidecar/no-sidecar replay, alpha/RGB bounds, private files, progress, source mutation, cancellation and occupied image/mask paths. This establishes raw sequence behavior, not movie decode/encode, retained audio or photographic temporal quality.

## Host RVM sequence adapter

`services::local_person` defines the pinned RVM inventory and complete absolute PERSON_BIN/PERSON_MODEL/PERSON_ORT_LIBRARY parser. `local_person_sequence::segment` invokes schema7 through ProcessRuntime with private owned staging, one operation deadline, cancellation/reaping, strict metadata and source/engine rechecks. The parser is not yet wired to AppConfig/main/AppState.

The host always asks the child for raw masks, even when no mask destination is requested. It independently streams source/output RGBA in64KiB chunks, reconstructs every alpha from the mask (including blank fallback), verifies unchanged RGB, exact frame/byte counts, raw hashes and private0600 files, then publishes image/mask without replacement. This avoids trusting just the child's claimed hash or an alpha upper bound. Optional external mask publication rolls back if image publication fails. Caller still owns source authorization, Analysis admission and video decode/encode staging.

This adapter does not yet expose a video job, retain a movie's audio or encode a transparent movie. Those are required follow-on integration steps.


## RVM video host service status (2026-10-20)

`services::local_person_video::segment` now has authenticated durable server jobs; editor controls are available for selected video in both composition modes. Set `PERSON_BIN`, `PERSON_MODEL` and `PERSON_ORT_LIBRARY` together as absolute paths. Application startup now parses these into `AppConfig` and carries the configuration into `AppState`; absence leaves the feature unavailable, while partial/relative settings refuse startup. The pinned RVM model is 14975696 bytes, SHA256 `88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828`. Configured paths alone do not prove the model/runtime is installed: the service checks actual identity before processing. It runs ordered recurrent RVM once per sequence and produces lossless alpha QTRLE MOV with all source audio tracks plus an optional raw mask stream. Admission currently requires CFR, SDR and source components at most8bits. VFR, PQ/HLG and deeper components refuse explicitly. Decoded output RGBA, audio hashes, frame timestamps and admitted source color tags are checked before no-replace publication. This does not establish photographic matting quality, hardware acceleration or complete video feature parity. Actual synthetic video and MOV metadata contract evidence: docs/concat-parity.md, RVM whole-video section.


### RVM server API

`GET /api/person/status` requires authentication and reports verified availability plus current CFR/SDR8 admission limits; it does not disclose configured host paths.

- `POST /api/person/video-jobs` with `{ "sourceId": "..." }` admits an owner-scoped job for an accessible source video. The request cannot select arbitrary host paths.
- `GET /api/person/video-jobs?sourceId=...&limit=20&cursor=...` lists owner history; limit1–50.
- Common `/api/jobs/:id` status and cancel/retry/discard apply with owner checks.
- `GET /api/person/video-jobs/:id/video` reads a completed private lossless alpha MOV, including HTTP Range; current source access and actual result hash are checked.
- `POST /api/person/video-jobs/:id/asset` with `{ "projectId": "..." }` requires project owner/editor permission and explicitly stores an independent MOV source in that project's private space. Repeated acceptance reuses the reservation and verifies existing bytes.

Completed reads/acceptance use retained files and do not require loading RVM again. Current admission remains CFR/SDR up to8bits; The ordinal video-mask editor is available; full source/platform qualification remains unfinished. Expanded project acceptance qualification is tracked in docs/concat-parity.md.


### RVM editor

Select a video (linked audio is included by the existing replacement command), open “Удалить фон видео”, check availability and create the copy. The panel supports explicit cancellation, reconnect and owner/source history. “Применить обработанную копию” stores a project source and applies it using the normal original/copy and Undo/Redo workflow. If the document changes during saving, the asset remains in the library for manual selection. Closing the panel detaches observation without cancelling background work.

The actual native-editor synthetic qualification verifies original-reference alpha in native preview, first-frame native export pixels, exact audible PCM, linked AV Undo/Redo and saved-project readback. Legacy editor state commands are tested; browser HTML-video playback of QTRLE and wider codecs/platforms are not qualified by that native workflow. See the RVM video editor section of docs/concat-parity.md for evidence and remaining scope.


### Retained RVM video masks

New person-video jobs retain a private raw learned-mask sidecar. `GET /api/person/video-jobs/:id/masks` returns row-major u8 planes in recurrent source-frame order; frame count, mask dimensions and SHA256 are in the completed job result. Range supports bounded reads. `GET /api/person/video-jobs/:id/masks/:frame` returns one zero-based ordinal as grayscale PNG. Both routes require the job owner and current source access, check actual mask bytes and return no-store. Completed reads do not load RVM. This is the retained-mask foundation; editable video correction recipes, project-owned sidecar acceptance/archive transfer and brush UI are not yet implemented.


### Native cached video-mask correction protocol

`video-kadr-vision-runtime --person-correct /absolute/request.json` accepts schemaVersion8 with absolute `input` raw original RGBA, `mask` raw automatic-mask sequence, new `output` RGBA and `maskOutput`, sourceSha256/maskSha256, width/height/frames and sparse `corrections: [{frame, correction}]`. Ordinals are zero-based source-frame indices. Correction uses the existing custom/strokes/feather schema. Timed strokes and attached learned regions currently refuse. No model/ORT arguments are needed. Outputs and canonical correction recipe are hash-bound in one metadata record; new files publish no-replace with rollback. Retain original automatic masks and authored recipes when storing corrected outputs: all-zero automatic masks and intentional authored erasures have different fallback semantics.

This protocol now feeds authenticated correction jobs and the ordinal video-mask brush. Current real qualification is synthetic fourframe/rawstream only; durable model-free MOV correction/editor/project/archive workflows remain unfinished.


### Server raw-sequence correction adapter

`services::local_person_correct::correct` invokes schema8 through ProcessRuntime using only the native executable path. Input binds original source RGBA/mask hashes, dimensions/count and sparse typed FrameCorrection recipes. One deadline covers preparation, request staging, child process and verification; cancellation kills/reaps before removing owned staging. The adapter checks canonical correction identity, independently verifies output RGB/alpha against returned masks, checks unchanged ordinal masks and computes blank fallback from the original automatic mask plus authored recipe. It publishes private outputs no-replace, with sidecar rollback. Caller must supply source authorization, Analysis admission and decode/encode staging. This raw adapter underlies the authenticated corrected-MOV jobs and ordinal brush workflow.


### Model-free corrected MOV service

`services::local_person_video::correct` accepts `CorrectionInput { video, masks, mask_hash, corrections }` and only the native executable path. `video.source` must be the original source movie, not the previously alpha-multiplied result; `masks` must be the retained original automatic sequence. It shares the lossless QTRLE MOV pipeline with `segment`, including sourcePTS, all copied audio tracks, original-color tags and exact decodedRGBA/audio verification. Returned metadata includes correction provenance (original mask SHA and canonical recipe SHA) and a distinct engine digest. Optional output mask publication retains paired rollback/no-overwrite semantics. Source/mask/executable identities are rechecked before publication.

Six synthetic source variants and authored full-frame erasure are qualified, including source delay, rotation, fractionalFPS, BT709 and two audio codecs. Stale/in-flight changed original mask streams refuse publication. CFR/SDR8 admission remains; correction HTTP jobs and ordinal video brush are available; time authoring and portable project correction-mask ownership remain unfinished.


### Authenticated video correction jobs

`POST /api/person/video-jobs/:id/corrections` accepts `{ "corrections": [{ "frame": 0, "correction": { "custom": true, "strokes": [], "feather": 0 } }] }` and returns a new jobId. Use a completed owned RVM or corrected video job; the server resolves its original automatic mask sequence and original source video. The body replaces the full sparse recipe, so an empty corrections array restores the original learned result rather than reprocessing an already alpha-multiplied video. Current frame ordinals are zero-based; timed strokes and attached SAM-region records still refuse.

`GET` on the same URL returns baseJobId, original maskHash and stored corrections after checking recipe digest/provenance/current root masks. Job status/cancellation/history/private MOV/mask reads and explicit project asset acceptance use the existing person-video endpoints. The native executable must be configured, while the RVM model/ORT files may be absent; `/api/person/status` reports correctionAvailable separately from learned available. Worker revalidates original source/masks/executable/tools at execution and publication. Source/mask Validation failures require a new explicit admission once inputs are restored, following existing nonretryable-job policy. Rate-limited or failed admissions remove their unqueued recipe records.

Ordinal video brush controls are available. Source-time/range controls and portable project-owned recipes are available below; SAM video tracking remains unfinished. Actual synthetic HTTP qualification covers model/ORT absence, corrected pixels/masks, recipe integrity, chained reset, project acceptance, queue refusal cleanup, managed queued recovery and running cancellation. See docs/concat-parity.md for exact gates and remaining scope.


### Video brush in the editor

On a completed person-video job, open “Правка видеомаски” → “Открыть видеомаски”. Select the zero-based source-frame ordinal, paint the original automatic grayscale mask, choose brush/eraser/confidence tools and per-frame feather. Other frame recipes are retained. Pointer and keyboard input, undo/redo, frame reset, discard and failed-frame-read retry are supported. Stroke overlays describe pending edits; “Пересчитать видеомаски” creates a new private corrected movie and reconnects the parent panel to that job. Apply explicitly after completion. Unsaved strokes block applying the stale copy or switching jobs.

Saved recipes reopen through owner job history or directly from the saved project attachment, including imported archives. Source-image preview, source-time and range authoring are available below; SAM video tracking remains unfinished. Native editor pointer/recipe/alpha/export/audio/Undo/Redo/save/read evidence and desktop/mobile screenshots are recorded in docs/concat-parity.md.


### Portable video-mask snapshot format

Native and frontend mediaVersions now support optional `videoCutout` with sourceId/maskSourceId, sourceHash (original movie), maskHash (raw learned mask planes), maskFileHash (lossless encoded mask video), oriented width/height, maskWidth/maskHeight, frames, rational frameRate and sparse corrections. The mask is a silent video resource; source-frame ordinals retain exact u8 grayscale values. The original source, copy and mask are independently registered resources. Validation refuses missing/wrong-kind/audible/mismatched mask resources and invalid recipes; source dependency collection retains all files through template/clipboard/archive paths. Archive import rewrites IDs while content hashes remain stable.

Actual synthetic native project/archive transport is qualified with a real FFV1 grayscale mask video and exact decoded raw-mask SHA. New job result acceptance now returns the processed movie plus a videoCutout bundle with private project-owned original/mask resources and snapshot. The editor attaches that bundle to linked clips in one Undo/Redo operation; project-owned correction execution and editor reopening are connected below. Synthetic schema/transport evidence does not establish photographic-quality or platform parity.


### Prepared automatic mask videos and acceptance bundle

New PersonVideo jobs prepare a verified silent FFV1/Matroska automatic-mask sequence before success. `automaticMaskVideo` in job results contains width/height/count/rate, raw maskHash, encoded fileHash and sizeBytes. Corrected jobs retain ORIGINAL learned planes here; their separate raw `/masks` endpoint contains the authored result. `GET /api/person/video-jobs/:id/automatic-mask-video` supports private no-store Range reads with owner/source access and actual file/hash verification.

`POST /api/person/video-jobs/:id/asset` now returns the usual processed media fields plus optional `videoCutout: {snapshot,source,mask}`. Source and mask entries are independent, project-owned immutable copies. Repeated acceptance reuses reserved IDs and verifies existing bytes; changed copies refuse without overwrite. This runs without RVM/ORT when the prepared job resources exist. Old records lacking prepared automatic video preserve prior movie-only acceptance.

The response is typed as SavedPersonVideo. The editor replacement callback registers the processed copy, project-owned original, silent automatic-mask source and videoCutout snapshot atomically on linked legacy/native clips. A locked linked partner refuses the entire edit. Original/copy switching uses the project-owned original. Actual pointer-authored native editor acceptance, Undo/Redo, project save/read and archive import into a new Space are qualified with exact preview/export frames and audible PCM. Project-owned brush replay after import is qualified below. No hard-kill/multi-process/Windows/photographic-quality guarantee is inferred from these synthetic managed-job gates.


### Restoring accepted automatic video masks

The host local_person_mask_video::decode service restores verified raw gray planes from a saved FFV1/Matroska resource. It verifies encoded identity, exact layout/rate/count and decoded raw-mask SHA; it supports cancellation/deadlines/private staging/no-replace publication. Callers must provide source authorization and an Analysis lease. Actual imported-archive service tests restore the masks, replay original pixels without the model and erase one selected frame while preserving the others. The public project-origin queue and editor use this primitive below; the primitive itself does not infer caller authorization.


### Saved/imported project video correction API

GET `/api/composition-projects/:id/video-cutout/:clip?compositionId=root` returns `{snapshot}` to an owner/editor after checking registered original/mask identities. POST `/api/composition-projects/:id/video-cutout/corrections` accepts `{clipId, compositionId?, expectedAttachment, corrections}` and returns202 `{jobId}`. The expected attachment must equal the saved clip snapshot; save the current project first. This creates a private managed PersonVideo correction task without the original inference job, RVM model or ORT. It restores automatic mask planes under the Analysis lease, applies sparse edits, prepares the processed movie and original-mask transport, and rechecks saved snapshot/access/file identity.

Observe/accept through existing job and person-video asset endpoints. Project-origin recipe records survive terminal request-payload erasure; correction recipe returns projectOrigin as extra context. The recipe baseJobId points to the project-derived job itself; its ordinal PNG reads ORIGINAL automatic planes, while `/masks` streams authored planes. Continued job corrections return to the current project context and refuse changed learned resource identities. Real imported-archive HTTP tests cover model-free edits, stale/outsider refusal, acceptance, reset, queued source/snapshot/access changes and managed queued restart. The editor exposes saved-project reopening through these routes as described below.


### Open saved video masks in the editor

Select a versioned video (and its linked audio) in the legacy or native editor. In “Удалить фон видео”, click “Открыть сохранённые видеомаски”. Save current project changes first: the server requires the saved attachment to equal the editor snapshot. Opening prepares a private model-free replay job; it does not alter the timeline or require model availability/history. Normal job progress, cancel/reconnect and original automatic-mask brush controls apply. On completion open the ordinal brush, edit/reset frames, recalculate, then explicitly apply. Applying registers original/mask/copy plus the new recipe in one Undo/Redo; save the project to persist it. Dirty strokes block applying/reopening.

Real mounted native editor/API browser qualification imports an archive into a new Space, reloads a fresh editor, deletes only private test copies of RVM/ORT, reopens the two saved eraser strokes, resets to exact original maskHash, paints full foreground on frame1, applies and saves/reads the new brush recipe. Painted preview frame1 is byte-equal to the original source; exported frame1 equals preview; audible PCM is exact and linked AV Undo/Redo is atomic. Desktop/mobile views were inspected. The synthetic RVM oracle has zero visible alpha in all frames; this is transport/authoring evidence, not photographic matting quality or whole app-shell playback/performance/platform proof.


### Source time and range authoring

For jobs with an exact source frameRate, “Время от начала исходника, с” selects the presented source frame at that time. Decimal comma is accepted. Previous/next buttons and the original ordinal field stay synchronized. Time is relative to the start of this source, not to the timeline placement. End-of-video point selection refuses; old jobs without a source clock keep ordinal controls.

In “Правка диапазона видео”, enter start/end seconds and click “Заменить правки диапазона”. It replaces overrides on every frame intersecting the half-open interval with the current frame recipe; the exclusive end on a frame boundary leaves that frame untouched. Empty end means the exact source end. Selecting an untouched/automatic frame and applying it resets overrides in that range. Other frames are retained; the whole range edit has one local Undo/Redo and remains pending until recalculation/application. Oversized frame/stroke/point ranges fail atomically before copying.

Exact decimal rational tests cover fractional and extreme u32 clocks. Actual4fps browser qualification paints source frames1/2 through[0.25,0.75), keeps frame3 unchanged, compares exported/native pixels and exact PCM, and persists the sparse range recipe after imported-project model-free reopening. This does not establish a fractional-media browser or photographic RVM quality matrix.


### Original source under the video brush

“Основа полотна” selects automatic mask (default), original frame, or original plus mask. Switching views preserves pending strokes and their Undo/Redo history. Overlay uses the same source-normalized coordinates as the mask brush. A failed original-frame read can be retried or switched back to mask view without discarding edits. Original images use private object URLs which are revoked on frame/view/job/account changes and unmount.

GET `/api/person/video-jobs/:id/source/:frame` returns private no-store PNG from the ORIGINAL video at a zero-based decoder ordinal, including project-derived jobs after archive import. It requires job ownership and current source access; no model/ORT or inference history is loaded. The host holds an Analysis permit in an owned cancellable task, verifies immutable source SHA and pinned FFmpeg/FFprobe, checks oriented geometry, decodes by ordinal without timestamp rounding/resizing and rechecks identities/access before returning PNG. Admission/decoding has a15s deadline; bounded PNG encoding is joined before releasing the permit and late results refuse. Full-resolution source admission is64MP; this is not a high-resolution/late-frame latency guarantee.

Actual source PNG tests compare every RGBA byte for four ordinary, four30000/1001 and four rotated90° source frames, including transparent RGB. Authenticated imported-project tests cover original pixels, outsider403, ordinal400 and changed-source409 with absent model/ORT. Browser qualification compares the actual SVG source Blob bytes to the source oracle, then paints/propagates/recalculates/applies the imported project, preserving exact native/export pixels, audible PCM and saved recipes. Photographic quality, HDR/VFR/deep sources and performance/platform matrices remain open.


### Native SAM tracking protocol foundation

The isolated vision runtime now supports `--sam-track <request.json>` with schemaVersion9. Host-owned absolute paths identify pinned encoder/decoder/ORT, raw ordered RGBA input, exact256x256gray seed, and raw region-stream output. Include width/height/frames/seedFrame and sourceSha256/seedSha256. The seed is retained at its exact ordinal; each direction propagates until IoU falls below0.25 or no confident prompt remains. Later planes in that direction are zero. Metadata distinguishes loss positions and reports trackedFrames including the seed. Publication is private/no-replace with file identities rechecked.

Static regions still choose model confidence; tracking chooses temporal overlap. Actual pinned-original comparisons qualify retained AND lost regions, same/moving frames, portrait/landscape/square sources and complete bidirectional stream bytes. This is a native core/protocol foundation. A ProcessRuntime raw adapter is available below; movie decoding and a source-authorized durable queue are available through the services/API below; video-region recipe persistence/replay and editor tracking controls remain unfinished. Forced process termination cleanup belongs to the host-owned staging scope; the CLI does not claim an OS kill recovery guarantee.


### Host raw tracking service

`services::local_sam_track::track` accepts local_sam Config, immutable raw RGBA source/seed paths and SHA identities, source shape/count/seed ordinal, an output destination, cancellation/deadline and progress. Callers must own source ACL, an Analysis lease and movie staging. It launches the isolated protocol through ProcessRuntime, checks strict result metadata, independently validates seed/loss/temporal-overlap stream invariants and all raw bytes/hash/private permissions, then fsyncs and publishes without replacing an existing file. Cancellation uses the existing process-group kill/reap path before dropping the owned staging directory.

Real-process tests match the pinned original streams for portrait/landscape/square sources and exercise empty seeds, active cancellation, changed inputs/executable and late foreign destinations. A separate adversarial-child suite checks malformed/duplicated metadata and inconsistent raw masks; it is contract validation, not a learned-model comparison. This raw service does not decode a movie; the movie wrapper below does. The durable API below wraps the movie service. Portable video smart-region recipe application remains unfinished. No editor tracking action is exposed by this stage.


### SAM movie tracking service

`services::local_sam_video::track` accepts the original encoded movie and SHA, source decoder seed ordinal, normalized prompt points, local_sam Config, FFmpeg/FFprobe Tools, staging budget, cancellation/deadline and progress. It decodes oriented RGBA without frame resampling, creates the prompted seed and tracks independently in both directions. Output is a silent256x256 FFV1/Matroska region-mask sequence; optional mask_destination retains the exact ordered raw gray planes. It does not rewrite the source or create a foreground-alpha movie. Metadata binds source/decoded/engine hashes, source geometry/count/rational rate, seed, tracking losses and encoded region identity.

Caller must authorize source access and hold an Analysis permit. CFR SDR8-bit source admission includes fractional rates, display rotation and nonzero initial timestamps; VFR/HDR/deep sources refuse. Exact-byte original Concat comparisons and independent mask-video decoding qualify synthetic landscape/portrait/square inputs. Cancellation, changed inputs, budgets and foreign destination cleanup are tested. The durable API is documented below; portable per-frame SAM-region recipes and editor tracking controls remain unavailable. /sam/status reports video availability only with an available model and codecs.


### Durable SAM video tracking API

POST `/api/sam/jobs` accepts `{sourceId, points, seedFrame}` for a registered video. Omit seedFrame for existing still-image region inference. SeedFrame is a zero-based decoder ordinal in the original source, below360000; actual source count is checked during analysis. Points use the same normalized original-source coordinates as still SAM. Callers supply no paths/models/output destinations. The selected owner model and codec identities are captured; jobs hold Analysis admission and expose standard status/progress/cancel/recovery. This analyzes the full original video, independently of timeline placement, speed or trimming.

GET `/api/sam/jobs?video=true&sourceId=...` lists owner video tracking history; omitting video=true preserves still-only history. GET `/api/sam/jobs/:id/points` returns the persisted source/points/seed recipe. GET `/api/sam/jobs/:id/video` returns the verified silent region sequence with Range and no-store. The latter rechecks owner/current source access, original content identity and encoded output identity; it needs no loaded model after success. This endpoint serves masks, not a composited foreground movie. Existing PNG/asset routes do not accept video results. Actual API tests qualify10scenarios: exact original region streams/private Range delivery, retained recipes/history, queued/running managed recovery and cancellation, queued/in-flight source mutation/access revocation and changed engine refusal. Existing still-image job regressions also pass. Recovery reruns processing; it is not per-frame tracking checkpointing or abrupt-process recovery proof. Portable video-region application and tracking controls in the editor remain unfinished.


### Accept and transport tracked SAM regions

POST `/api/sam/jobs/:id/video-asset` with `{projectId}` returns `{source,mask,snapshot}`. It requires ownership of the tracking job, current source access and owner/editor access to the target project. Both files are independent project-owned private copies; original video bytes and lossless256x256 region-video bytes are preserved. Acceptance does not edit the timeline. Repeated acceptance reuses reserved IDs after hash verification; changed saved bytes produce a conflict. A successful job can be accepted without a configured SAM model.

The snapshot retains original/region/file/model/runtime/prompt identities, normalized prompts, seed ordinal, oriented dimensions, source frame count/exact rate and tracking loss positions/count. It contains no local paths or job IDs. Optional `videoCutout.trackedRegions` stores this available region inventory in a project snapshot. Every referenced original/region resource must be registered; clocks/content identities must match the parent video mask. Templates, clipboard and relinking retain these dependencies. This inventory alone does not paint a correction: ordinal smart-stroke binding/model-free tracked-region application and editor authoring remain unfinished.


Actual temporal-region transport qualification saves an accepted learned SAM region inventory in a native project, exports it and imports into a fresh Space without configured SAM. Both resource IDs are remapped while hashes/clock/prompts/loss positions remain exact. Restored FFV1 planes decode through the model-free mask-video service byte-for-byte equal to the original Concat reference. The archive holder uses synthetic foreground automatic masks and an original-pixel copy; this is transport proof, not application of tracked smart strokes. All1897frontend tests, typecheck and production build pass for the inventory/dependency changes. No tracking-editor/browser workflow is claimed.


### Native model-free tracked-region replay protocol

The isolated `--person-correct` schema8 accepts optional `regions:[{input,attachment}]` and `regionSourceSha256` alongside the existing ordered RGBA/automatic-mask inputs and sparse corrections. These are host-only paths. Each stream is raw256x256gray at the same frame count/ordinal clock; attachment.maskHash is the whole-stream hash. A frame stroke.region uses the same provenance but maskHash is that selected ordinal's plane hash. All other fields and smart-stroke points must match. No model is loaded. SourceHash in ordinary result metadata identifies decodedRGBA, while regionSourceHash is the host-declared original encoded identity; the host must verify the actual encoded movie and source permissions.

The child verifies stream lengths/digests before/after, exact frame-plane digests and bound smart tools/untimed points, then applies existing kernels in order. Unbound/unused/duplicate/mismatched annotations refuse. Legacy requests and metadata remain compatible with no region fields.57actual raw-protocol cases qualify byte-exact independent scalar composition using retained original Concat region streams, zero regions, ordering, malformed inputs and mutation/no-replace cleanup. This is a native protocol prerequisite. The raw host adapter below accepts trusted bindings. Project-origin admission and safe reopening of tracked strokes are documented below. Ordinary job-only corrections without a saved region inventory remain strict; new tracking-authoring controls are unfinished.


### Trusted host raw tracked-region replay

`local_person_correct::correct_with_regions` takes the usual raw sequence Input plus `local_person_regions::Context {encoded_source_hash,frame_rate,regions}`. Each RegionInput binds a private raw256gray stream to its portable video_region::Attachment. The caller owns actual encoded-movie/FFV1 checks, source access, Analysis admission and decoder ordinal staging. Context source identity/clock/oriented shape/count must equal every attachment; no unreferenced or duplicate region stream is admitted. Each frame's smart stroke.region uses matching provenance and that ordinal's individual mask SHA.

Host verifies whole streams and selected planes before/after execution, pins executable identity, launches the native schema8 extension through owned ProcessRuntime, verifies strict region/source/result metadata and existing raw pixels/alpha/fallback contracts, then publishes both outputs without replacement. Cancellation/deadlines await process-group termination/reaping before cleaning private staging. Invalid schema-bearing metadata cannot be hidden before a valid result. Legacy `correct` stays strict and refuses learned strokes without this explicit context.

19actual scenarios qualify raw replay against independent retained masks/RGBA, source/region/runtime changes, clock/provenance/hash/budget failures, cancellation and foreign destinations.10controlled-child cases additionally prove malformed metadata refusal and actual sleeping-child cancellation/timeout with PID absent before return. These are raw-service gates; the encoded whole-video wrapper is available below; project-origin queue binding is available below; new tracking brush authoring controls remain unfinished.


### Whole-video replay of encoded tracked regions

`local_person_video_regions::correct` accepts the existing CorrectionInput (original encoded movie, immutable raw automatic masks and sparse corrections), native executable, FFmpeg/FFprobe Tools and used `EncodedRegion {video,attachment}` resources. Caller owns source/project access and an Analysis lease. No inference models or ORT configuration are required. This wrapper verifies actual original and encoded-region hashes, decodes exact silent FFV1 region sequences and checks raw/plane bindings. Shared movie processing probes the real original source and refuses clock/oriented-size/count mismatches, VFR/HDR/deep sources under its current admission.

Result is a lossless QTRLE alpha MOV with original audio tracks/PCM and video clock preserved by the existing verified pipeline. Correction provenance includes the used region hashes. Source RGB and authored source alpha follow the existing mask kernel. Region staging is bounded/private and included in the global budget; caller outputs appear only after final identity checks, with no-replace publication/rollback and cancellation/deadline checks.22actual scenarios include exact independent frames/masks, ordinary/portrait/square/fractional/rotation/nonzero-start media, exact audible PCM and failure cleanup. Existing RVM/movie/job/project/archive regressions pass after shared-path changes.

This is an encoded-video service boundary. Project-origin correction admission is available below; job-only corrections without a saved inventory remain strict. No tracking brush UI or photographic-quality/performance/platform completion is implied.


### Inventory-bound smart strokes in project correction jobs

A saved VideoCutout may now contain `corrections[].correction.strokes[].region` bound to one entry of trackedRegions. The source/mask IDs, original/encoded region/model/runtime/prompt hashes and points must match that entry. The stroke maskHash is the SHA of that frame ordinal's256gray plane; the inventory maskHash is the SHA of the complete raw sequence. SmartBrush/SmartEraser and untimed ordinal recipes are required. Actual plane hashes are checked under Analysis during execution. An unused retained region may stay available in the project.

Existing project-origin POST `/api/composition-projects/:id/video-cutout/corrections` accepts these recipes with expectedAttachment. The server resolves saved IDs, verifies source/project access and hashes, runs the encoded model-free replay and rechecks snapshot/resources before success. Status/video/recipe reads require current project/resource permission; changed completed region files conflict. The owner can still cancel their own job after losing project editing access. Queued restart retains snapshots and annotations in SQLite and works without SAM/RVM/ORT models. Continue/reset and asset acceptance preserve the inventory; archives remap both inventory and per-stroke references.

The video brush can reopen and retain these saved annotations while adding plain strokes, undoing/redoing and submitting. Canonical dirty checks and relinking include region provenance. A region-plane annotation is not copied onto other ordinals by the ordinary range command; doing that needs separate frame bindings. Creating/selecting tracked strokes and launching tracking directly in the brush remain unfinished.

Authenticated API/native qualification covers a second export/import of an authored recipe and identical replay masks, first imported replay RGBA, managed queued recovery, tampering/access/stale queue refusals, repeated acceptance and automatic reset. Component tests use mocked frame transport; no new full browser/photographic-quality or platform/performance claim is made.


### Preview and apply saved temporal regions

For a project-origin video job, GET `/api/person/video-jobs/:id/regions/:region/:frame` returns a private no-store PNG for the exact decoder ordinal and `x-mask-sha256` for its raw 256x256 gray plane. It verifies project/resource permissions and the saved region resource under Analysis admission. The frontend verifies the returned ordinal and hash before showing the plane. No SAM/RVM model is required to reopen saved regions. Whole-stream raw hashing is performed by full replay, rather than by this one-frame preview.

In the video-mask brush, select a saved region, explicitly show its current frame, then use “Сохранить область кадра” or “Удалить область кадра”. This authors an ordinal-bound smart stroke; recompute creates the separate corrected copy. Undo/Redo retains the annotation. Changing frame/region/account clears stale preview. Launching new tracking from this panel and applying learned regions across a range remain unfinished. Long-clip preview performance and photographic segmentation quality are not qualified by the synthetic browser fixture.


### Author a tracked region from a native project

The native project video-mask brush exposes “Новая отслеживаемая область” for a saved project-origin mask. Select original/overlay view, choose an object point and start SAM tracking from the current source-frame ordinal. Tracking uses managed video SAM availability, progress/cancellation and history reconnect. Compute or discard pending brush edits before starting. On completion, the editor validates and adds the independent source/mask resources and region inventory to every linked AV member in one Undo/Redo action. It does not change the current processed movie. The shared panel prepares a session continuation and automatically opens the new region without a project save/reopen step. The flat editor now supplies the same explicit attachment action through its own source registry, link and history model. A real Chromium/native SAM/ORT/API fixture now qualifies source-ordinal keyboard prompting, exact original temporal mask planes, both linked AV inventories, one Undo/Redo, revision-checked project save and saved-region reopening. This mounts real product components/state rather than the full editor shell and uses synthetic input; photographic and long-clip quality remain unqualified.


The flat clip media panel also exposes tracked-region creation for saved project-origin video masks. It registers both project-owned resources and their private media bindings in one Undo/Redo action, updates every linked member and refuses a locked clip or track atomically. Both editors use the shared automatic continuation panel; actual learned-model browser qualification covers both native project state and the flat clip-media panel, including flat private media-binding Undo/Redo.


### Apply tracked-region strokes over a range

“Заменить правки диапазона” supports saved tracked-region strokes as well as plain brush edits. The editor first checks aggregate limits, then fetches each distinct region plane for every target ordinal using the private frame API. Each copied smart stroke receives that ordinal's own SHA; repeated uses of a region share one read per frame. Only a fully prepared and validated range becomes a single local Undo/Redo entry. Progress and explicit cancellation are available; errors/cancellation/context changes leave existing edits intact. No inference models are required.

This currently reads frames sequentially through the PNG endpoint; long-clip/bulk performance remains unqualified. A real browser/API/native-process fixture qualifies four-frame range submission, Undo/Redo, persisted ordinal hashes and every resulting raw-mask/RGBA byte against an independent scalar f32 reference. It does not qualify photographic model accuracy, the full editor shell or browser-native QTRLE playback. The synchronous plain copier continues refusing unbound learned ranges.


Reproduce range qualification with a disposable descriptor from the explicitly enabled `person_region_project` integration test (`VIDEO_KADR_TEST_REGION_BROWSER_FIXTURE`, `--ignored`). Generate expected bytes using `scripts/generate-person-region-range-oracle.py --fixture <descriptor> --output <private-directory>`, run a disposable authenticated backend/Vite, then set `REGION_BROWSER_FIXTURE`/`REGION_BROWSER_OUTPUT` and run `frontend/scripts/qualify-video-region-range-live.mjs`. The descriptor contains a synthetic authentication token; retain it privately and remove it plus disposable media storage after stopping owned servers. The fixture deliberately configures nonexistent inference model/ORT paths, proving cached range replay requires neither.


Actual SAM authoring qualifier: `frontend/scripts/qualify-sam-video-author-live.mjs`, using the disposable project descriptor described above, a real configured SAM encoder/decoder/ORT and HMR-disabled Vite. It also exercises the native project transaction and persisted reopening. Choose the point with a pointer, or focus the canvas and use arrows plus Enter/Space while point-picking is enabled; selection does not author a brush stroke. The native fixture deliberately leaves RVM model/ORT paths absent during saved correction reopening.


### Continue with a tracked region before saving the project

`POST /api/person/video-jobs/:id/regions` accepts `{samJobId,corrections}` for a completed project-origin correction and a completed owner SAM tracking job. It verifies current access and saved baseline, accepts independent project-owned tracking resources through the existing checked asset path, and returns a NEW queued correction job. The stored effective inventory includes the added region; optional `projectOrigin.savedSnapshot` retains the immutable persisted baseline. No project write or old job mutation occurs. Follow the new job to Done, then use its existing region-preview/correction routes. Client helper: `continueVideoWithRegion`.

Only region additions are allowed relative to the baseline. Source/mask/clock/shape and existing region changes refuse; exact current baseline/effective snapshots and all resource access/hashes are rechecked. Subsequent saved effective inventory can become the baseline through ordinary continuation. The product panel now performs automatic handoff: it waits for the continuation, attaches the local region inventory once, lets the scope reset settle, checks account/clip and exact inventory, then opens the new job and selects its region. No project save/reopen is required for this step. Real SAM/browser HTTP qualification proves unsaved plane access, exact smart-correction masks/RGBA and unchanged persisted project. Managed queued and running restart of this baseline variant is qualified, including smart keep/erase recipes: the same job reconciles and completes without SAM configuration, retaining its exact baseline/inventory/recipe and matching independent mask/RGBA references. This restarts replay rather than resuming a frame checkpoint; hard-kill/power-loss and editor-session restoration remain separate gates.


### Automatic tracking handoff in the product panel

The SAM tool passes its completed tracking-job ID along with accepted resources. PersonVideoPanel captures current corrections, admits or reconnects to the same continuation job, and offers cancellation of that job. It adopts the result only after completion and the explicit local region transaction, verifies the live account/target and resulting inventory, and opens VideoMaskBrushEditor automatically with the new region selected. A dropped observation does not admit a second continuation. Account/clip changes abort waiting; intervening document changes refuse adoption.

Automatic recipe opening runs once per job context after the panel is enabled. Brush disclosure uses local bind:open state so both user opening and automatic opening survive recipe/frame updates. Aborted callbacks short-circuit before reading destroyed component state. Native real-browser qualification (`frontend/scripts/qualify-sam-video-handoff-live.mjs`) covers actual SAM, automatic preferred-region opening, linked AV inventory Undo/Redo, unchanged saved document and immediate smart keep/erase masks/RGBA against independent references. This mounts product panel/state/API rather than the full shell. Saved-session restart/history restoration and long-clip/photographic/platform quality remain separate gates. The flat real-model clip-media path is now qualified separately below.


### Keep brush navigation while processing

The panel carries the current source-frame ordinal and mask/source/overlay selection through tracking continuation and subsequent corrections. A completed SAM region reopens at the captured frame/view (or its verified seed ordinal for callers without navigation); a normal correction also reopens at the frame/view where it was submitted. This works for ordinary job-history masks without SAM inventory. Navigation is ephemeral UI state and does not alter the portable recipe or project clock. Invalid requested context falls back to ordinal0/mask before frame fetching.

Runtime qualifier asserts1/source after actual SAM and2/source after smart keep/erase, alongside exact output-mask/RGBA comparisons. Component coverage also verifies ordinal3/overlay for ordinary job-history correction, bounds, safe fallback and no duplicate recipe reads.


### Flat clip-media panel runtime qualification

`frontend/scripts/qualify-sam-video-flat-live.mjs` creates a real authored flat project in the disposable source Space, uses its actual linked video/audio records and mounts ClipMediaReplacement. Actual SAM tracking, automatic unsaved continuation, ordinal1/source restoration, subsequent ordinal2/source correction and all output mask/RGBA bytes are checked through authenticated HTTP/native processes without route mocks. The persisted project stays unchanged while local inventory and private media bindings update in one history entry; Undo/Redo restores both maps exactly.

The source picker/file input now stay within their container with long source labels: scoped grid tracks have a zero minimum and controls use border-box width constraints. Real390px browser overflow assertion and desktop/mobile visual review qualify this layout, without container overflow clipping. Full editor-shell, photographic, long-clip and platform coverage remain separate.


### Local inventory history and completed brush sessions

While PersonVideoPanel remains mounted, Undo/Redo of a region-inventory transaction can restore the matching completed correction job, preferred region and handoff frame/view. The panel keeps at most32 exact attachment scopes, clears them when account/project/source/clip changes and on unmount, and rechecks job status/access before reopening a restored brush. It does not start replacement processing or cancel durable work. An access failure leaves the brush closed and surfaces the server error. This cache is not persisted and does not retain unsubmitted brush edits; panel remount/page reload/full-editor history qualification remain separate.

Current frame/view/region changes now update the retained completed-session handle as well, independently of processing. Undo/Redo can restore later navigation such as3/overlay and an older selected region, not only the position captured by the last handoff. A completed recalculation retains the selected region. Events are job/displayed-snapshot scoped and ignored during snapshot transitions; initialization props are only changed when restoring/adopting a session. Unsubmitted mask strokes are still not retained by this cache.

### Reopen a completed project session from history

After closing/reopening the panel, use “Обновить историю удаления фона” and choose the completed job. For project-origin jobs the panel rechecks its private recipe before exposing the brush: project, timeline, clip and the current effective or saved baseline snapshot must match. This can reopen an unsaved-region session from its original saved baseline without rerunning inference. A different binding is refused, and changing account/clip aborts pending checks. This is explicit server-backed history reopening; automatic restoration of navigation/drafts after page reload is not implemented. Source-only jobs without project origin retain the ordinary explicit history flow.

When the selected clip shows a processed copy, saved-mask history uses the original ID from snapshot.sourceId rather than the displayed copy ID. This is required to find its durable correction sessions. Actual native panel/backend/SAM browser qualification now covers closing/reopening the panel, selecting the exact completed smart-correction job, project binding checks and retained regions without new processing. Existing raw masks/MOV RGBA match independent references. Full editor shell, flat-panel history remount and automatic page-reload/navigation/draft restoration remain separate gates.

The flat ClipMediaReplacement path now also has actual SAM/backend/browser remount evidence: both authored smart-correction frames and retained regions reopen from the exact durable history job; a fresh region-frame read matches its independent plane hash. Local document and private media-binding maps stay unchanged and no processing job is admitted. Both native and flat mounted product panels are covered; full shell and automatic page-reload/navigation/draft persistence remain separate.
