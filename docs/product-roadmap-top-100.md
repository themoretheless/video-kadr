# Video Kadr: Top‑100 функций до полноценного видеоредактора

Дата среза: 3 августа 2026. Это gap-анализ относительно consumer/prosumer
редакторов, а не обещание реализовать все пункты одновременно.

## Текущая точка

Video Kadr уже умеет канонический project document, неразрушающий per-clip trim,
структурный undo/redo, crop/scale/rotate, скорость, базовый
звук, цветовые пресеты, 3D LUT, RGB-кривые, маску-прямоугольник и экспорт через
native FFmpeg либо `ffmpeg.wasm`. Не хватает главного признака NLE: проекта с
несколькими клипами и дорожками, временной шкалы, слоёв, текста, переходов,
субтитров, keyframes, развитого audio workflow и устойчивого browser media cache.
Reverse и loudness normalization уже реализованы и поэтому не включены как gaps.

Ориентиры: [CapCut video editor](https://www.capcut.com/tools/video-editing-software),
[Adobe Premiere features](https://www.adobe.com/products/premiere/features.html),
[DaVinci Resolve](https://www.blackmagicdesign.com/products/davinciresolve),
[DaVinci Resolve Edit](https://www.blackmagicdesign.com/products/davinciresolve/edit),
[DaVinci Resolve Fairlight](https://www.blackmagicdesign.com/products/davinciresolve/fairlight),
[Microsoft Clipchamp](https://support.microsoft.com/en-us/clipchamp/what-is-clipchamp).

## Как читать рейтинг

- **P0** — без этого нельзя уверенно закончить обычный ролик.
- **P1** — parity с массовыми creator-редакторами.
- **P2** — prosumer-возможности и работа с большими проектами.
- **P3** — AI, collaboration и конкурентные ускорители.
- Размер: **S** до недели, **M** 1–3 недели, **L** 1–2 месяца, **XL** крупная подсистема.
- Если обязательна parity browser WASM и native backend, оценка увеличивается на
  один размер либо DoD явно становится capability-gated.
- DoD — минимальный проверяемый критерий готовности, а не полный дизайн.

Release bands без пересечений: P0 — №1–21, 23, 25, 64, 81, 84–85; P1 — №22,
24, 26–47, 50, 56–59, 61, 65–77, 87, 96–98; P2 — №48, 51–55, 60, 78–80,
82–83, 86, 88–95; P3/capability-gated — №49, 62–63, 99–100. При конфликте
эта строка приоритетнее заголовка секции.

## Top‑100

### P0 — фундамент полноценного NLE

| № | Функция | Сейчас | Минимальный Definition of Done | Размер |
|---:|---|---|---|:---:|
| 1 | Многодорожечный timeline | Backend Timeline v1 — однорядный `Vec<Clip>` без tracks/positions | 4 video + 4 audio tracks, overlaps/z-order, stable IDs и schema round-trip | XL |
| 2 | Несколько клипов в одном проекте | Один source clip | 20+ mixed-fps/resolution clips, playback/export и A/V drift ≤1 project frame | XL |
| 3 | Split/blade по playhead | Только trim/cut одного клипа | Граница на project frame; соседние source in/out без duplicate/gap, включая VFR fixture | M |
| 4 | Перетаскивание и перестановка клипов | Domain имеет `reorder_clip`/`MoveClipCommand`, UI integration нет | Drag с ghost/drop indicator, insert/overwrite modes и keyboard move | M |
| 5 | Ripple delete и закрытие gaps | Нет | Track targeting, immunity locked tracks, автоматический сдвиг и undo | M |
| 6 | Snapping | Нет | Playhead/edges/clip markers, threshold в px/frames и временное отключение | M |
| 7 | Timeline zoom и горизонтальная навигация | Нет | Zoom around cursor, fit project, scroll/trackpad без потери позиции | M |
| 8 | Frame-accurate playhead/timecode | Частично в preview | Project FPS, drop/non-drop timecode, mixed-FPS sources и одинаковый кадр preview/export | L |
| 9 | Неразрушающая trim-модель клипа ✅ | Реализованы независимые source in/out, source-duration limits, left trim и exact undo | In/out каждого timeline clip без перезаписи source и с handle limits | L |
| 10 | Единый project document ✅ | Реализован canonical v2 в Rust/TypeScript, SQLite/IndexedDB CAS, legacy migration, quarantine и structured conflicts | Canonical versioned schema, N→N+1 migration, unknown-field preservation/version rejection | XL |
| 11 | Autosave и crash recovery | Server autosave частично; Pages — память вкладки | Atomic IndexedDB snapshot/journal, recovery prompt и rollback повреждённой записи | L |
| 12 | Персистентное локальное media storage | Pages теряет media после reload | OPFS/FSA matrix для Chrome/Firefox/Safari, private mode, revoked handles, eviction и relink | XL |
| 13 | Timeline undo/redo ✅ | Реализованы atomic batch-команды, focus-scoped undo/redo, byte budget, CAS-safe autosave и recovery после failed command | Structural command history, grouped drags, memory budget и recovery после failed command | L |
| 14 | Thumbnail strip на timeline | Нет UI | Асинхронные thumbnails с cache, отменой и bounded memory | L |
| 15 | Audio waveform на timeline | Нет UI | Channel/downmix policy, peaks-per-pixel levels, zoom-aware cache и known-signal golden test | L |
| 16 | Track controls | Нет | Mute/solo/lock/hide, rename, reorder и состояние в project schema | M |
| 17 | Linked video/audio clips | Нет | Move/trim together, unlink/relink, sync offset indicator | L |
| 18 | Gap management | Нет | Select/delete/close пустых временных диапазонов без разрушения других tracks | M |
| 19 | Надёжный timeline preview engine | `HTMLVideoElement` + частичный CSS preview; LUT/curves и ряд effects не previewed | Golden projects, seek/scrub latency, edit invalidation, deterministic layers и A/V sync ≤1 frame | XL |
| 20 | Render graph из project timeline | Typed `EditPlan`/FFmpeg compiler есть; нет Timeline → EditPlan integration | Golden overlays/transitions/audio, rational timestamps и native/WASM tolerance matrix | XL |
| 21 | Базовые crossfade-переходы | Нет | Video/audio handles, cross dissolve/crossfade, duration controls и UX нехватки handles | L |
| 22 | Библиотека переходов (P1) | Нет | Wipe/slide/zoom/blur categories, accessible preview, drag-to-edit и parity fixtures | L |
| 23 | Text layer | Нет | Text clips, font/size/color/alignment/position, font loading/embedding policy и fallback | XL |
| 24 | Базовая анимация текста | Нет | Fade/slide/scale presets с editable duration и preview | M |
| 25 | Базовый timeline export | Durable jobs/cache/chunks частично есть; timeline export нет | 10‑минутный golden project, drift ≤1 frame, progress/cancel cleanup и retry | XL |

### P1 — creator parity с CapCut/Clipchamp/Canva

| № | Функция | Сейчас | Минимальный Definition of Done | Размер |
|---:|---|---|---|:---:|
| 26 | Canvas transform controls | Crop overlay частично | Anchor point, aspect lock, multi-select drag/resize/rotate, numeric controls и undo | L |
| 27 | Layer opacity | Нет | 0–100%, keyframe-ready, preview/export parity | S |
| 28 | Picture-in-picture | Нет multitrack | Несколько видео на canvas, resize/position/round corners/shadow | L |
| 29 | Blend modes | Нет | Normal/multiply/screen/overlay/add с явно заданным working color space | L |
| 30 | Alignment guides и safe areas | Нет | Center/edge guides, title/action safe, snap и toggle | M |
| 31 | Shapes и color backgrounds | Только censor rectangle | Rectangle/circle/line/background clips с fill/stroke/radius | M |
| 32 | Image/GIF/SVG overlays | Нет | Import, timeline duration, transform, transparency и animated GIF playback | L |
| 33 | Adjustment layers | Нет | Effect stack применяется к clips ниже на заданном диапазоне; z-order, trim и export parity | XL |
| 34 | Rich text styles | Нет | Stroke, shadow, background, spacing, line height и reusable style | L |
| 35 | Title/lower-third templates | Нет | 5–8 curated templates, editable fields, schema migration, missing-font/assets UX | L |
| 36 | Keyframes для transform/opacity | Есть generic scalar `KeyframeTrack`, но нет project/render/UI integration | Multi-property serialization, trim/split time remap, interpolation и parity | XL |
| 37 | Keyframe curve/easing editor | Domain умеет hold/linear/cubic expressions; UI/Bezier нет | Graph handles, monotonic constraints, copy/paste и exact sampling agreement | L |
| 38 | Keyframes для effects/audio | Domain adapter есть, edit/timeline integration нет | Animatable registry: transform/opacity/gain + минимум 3 effects; unsupported rejection | XL |
| 39 | Motion paths | Нет | Path overlay, editable points, auto-orient и easing | L |
| 40 | Copy/paste attributes | Нет | Выборочный paste transform/color/audio/effects на несколько clips | M |
| 41 | Effect stack на clip | Набор плоских полей | Stable effect IDs/schema migrations, ordered add/remove/reorder/enable | XL |
| 42 | Effect presets | Partial preset хранит speed/transform/audio/color/LUT/curves, но не stack | Stack round-trip, version migration и missing LUT/font/assets handling | M |
| 43 | Green screen/chroma key | Нет | Working-space color picker, similarity/spill/edge, garbage matte linkage к №44 | L |
| 44 | Shape masks | Только прямоугольник censor/crop | Stable point IDs; rectangle/ellipse/polygon, invert, feather и effect linkage | XL |
| 45 | Mask tracking | Нет | После №36–37/44/84: tracking job и ручная коррекция keyframes | XL |
| 46 | Freeze frame | Есть still export, не timeline | Создать still clip выбранной длины в timeline | M |
| 47 | Speed ramp | Только постоянная скорость | Curve, pitch/audio policy, split at discontinuity и duration recalculation | XL |
| 48 | Optical-flow retiming (P2) | Нет | Interpolation quality, WebGPU/optional-server fallback и warning | XL |
| 49 | AI auto reframe (P3) | Только crop presets | После №36/45/99: subject path 16:9↔9:16/1:1 с editable keyframes | XL |
| 50 | Несколько sequences/timelines | Нет | Create/duplicate/rename sequences с независимыми settings | L |
| 51 | Markers | Нет | Clip/sequence markers, color, note, keyboard navigation и export metadata | M |
| 52 | Grouping clips | Нет | Group/ungroup, group move/trim, nested selection semantics | M |
| 53 | Compound/nested clips | Нет | После №20/50: nested render, open/edit, cycle detection и deterministic output | XL |
| 54 | Пользовательские горячие клавиши | Есть фиксированные | Command registry, remapping, conflicts, import/export preset | L |
| 55 | Контекстные trim modes | Нет | Ripple, roll, slip, slide с frame-accurate preview and undo | XL |

### P1/P2 — субтитры, речь и звук

| № | Функция | Сейчас | Минимальный Definition of Done | Размер |
|---:|---|---|---|:---:|
| 56 | Subtitle tracks | Нет | Несколько subtitle tracks, cue timing, inline text edit и preview | XL |
| 57 | SRT/VTT import | Нет | Parse/validate/offset, VTT cue settings, Unicode/newline round-trip | M |
| 58 | SRT/VTT export | Нет | Sidecar сохраняет text/timing/language/cue settings без drift | M |
| 59 | Burn-in subtitles | Нет | Style/position/background, line wrapping, preview/export parity | L |
| 60 | Auto captions (P2) | Нет | Local/remote modes, model storage/offline UX, word timestamps и review | XL |
| 61 | Caption styling presets | Нет | 12 presets, active-word highlight, per-track style override | L |
| 62 | Перевод субтитров (P3) | Нет | Provider abstraction, consent/cost/data-egress UI, side-by-side review | XL |
| 63 | Transcript-based editing | Нет | Delete transcript range → ripple video edit, confidence and undo | XL |
| 64 | Audio clip/media semantics | №1 задаёт tracks, но audio-specific model отсутствует | Music/voice/SFX clips, channel layout, drag/move/trim и mix semantics | XL |
| 65 | Detach/replace audio | Нет | Channel-layout/sample-rate policy, sync preservation и drift test | M |
| 66 | Clip gain и track volume | Есть clip volume | Pre-fader clip gain, track/master dB faders, meters и clipping | L |
| 67 | Volume automation | Нет | Keyframed volume envelope on timeline with preview/export parity | L |
| 68 | Pan/stereo balance | Нет | Constant-power pan vs balance, mono/stereo policy и meters | M |
| 69 | Audio mixer | Нет | После №64/66/68: track strips, master bus/limiter и meter ballistics | XL |
| 70 | Parametric EQ | Только highpass | 4+ bands, presets, realtime graph, bypass and export | L |
| 71 | Compressor/limiter | Нет | Threshold/ratio/attack/release/makeup, gain reduction meter | L |
| 72 | Noise suppression | Простой denoise для видео; audio нет | Voice noise reduction with strength, preview and fallback | XL |
| 73 | Voice enhancement | Нет | De-esser, de-hum, clarity preset and loudness-safe output | L |
| 74 | Auto ducking | Нет | Dialogue detection создаёт редактируемые automation points, не hidden effect | L |
| 75 | Silence detection/removal | Нет | Non-destructive review list, threshold/min duration и ripple apply | L |
| 76 | Voiceover recording | Нет | Permissions/device disconnect, echo policy, countdown, monitoring и takes | L |
| 77 | Beat detection и markers | Нет | Analyze music, editable beat markers и интеграция со snapping №6 | L |

### P2 — media workflow, цвет и надёжность

| № | Функция | Сейчас | Минимальный Definition of Done | Размер |
|---:|---|---|---|:---:|
| 78 | Media bins/folders/tags | Плоская медиатека | Folders, tags, multi-select, rename и project persistence | L |
| 79 | Metadata inspector | Preview уже показывает duration/dimensions/fps/codecs/size при наличии | Full probe: VFR/timebase, rotation, pixel aspect, HDR/color/audio channels и fixtures | M |
| 80 | Поиск и фильтры media | Backend FTS5 API работает; browser/UI parity нет | Pagination, normalized query contract и 10k-item latency profile | M |
| 81 | Relink missing media (P0/P1) | Checksum/relink primitives есть, production UX/wiring нет | Locate replacement, fingerprint false-positive tests и batch relink | L |
| 82 | Proxy media generation | `ProxyService`/FFmpeg adapter есть, project association UX нет | Source/proxy provenance/timebase, stale invalidation, toggle и export originals | XL |
| 83 | Optimized preview cache | Frame-cache/artifact graph primitives есть, UI wiring нет | После №20/84: key = source fingerprint + graph version + render settings | XL |
| 84 | Background conform/analyze queue | Durable backend queue есть, derived-task frontend orchestration нет | Dependency DAG, persistent priority, restart/idempotency/fairness и cancel | XL |
| 85 | Browser memory/quota architecture | Нет preflight; input/output целиком materialize в JS/WASM memory | Численные memory fixtures, quota/eviction UX, capability-gated streaming/fallback и OOM recovery | XL |
| 86 | Portable project archive | Нет | Project+manifest+optional proxies/media, validation и relink on import | XL |
| 87 | Temperature/tint/highlights/shadows (P1) | Базовые brightness/contrast | Neutral reset, порядок относительно LUT/curves и preview/export parity | L |
| 88 | Color wheels | Нет | Lift/gamma/gain wheels and numeric controls | L |
| 89 | HSL selective color | Нет | Hue range picker, H/S/L adjustments and mask preview | XL |
| 90 | Video scopes | Нет | Signal pre/post effects, sampling precision/rate budget, waveform/parade/vector/histogram | XL |
| 91 | Минимальный SDR color pipeline → color management | Нет | P0/P1: BT.709/sRGB range/matrix/transfer policy до blend/key/LUT; HDR расширение позже | XL |
| 92 | LUT browser/export | Upload есть | Preview/search/favorites и отдельный capability-gated `.cube` 33³ baker | XL |
| 93 | Multicam editing (P2) | Нет | Sync по audio/timecode/marker, angle viewer, live switching и flattened export | XL |

### P2/P3 — delivery, templates, capture, AI и collaboration

| № | Функция | Сейчас | Минимальный Definition of Done | Размер |
|---:|---|---|---|:---:|
| 94 | Export queue и batch variants | Одна задача | Persist job definitions, sequential memory policy; restart, не resume WASM process | L |
| 95 | File-size estimate и target size | Нет | VBR estimate с error band; optional bitrate solve for target MB | M |
| 96 | Screen/camera recording | Нет | Screen/camera/mic, permissions, optional simultaneous composition и timeline insert | XL |
| 97 | Templates и brand kit | Только effect presets | Project templates, fonts/colors/logos, placeholders and safe migrations | XL |
| 98 | Video stabilization | Нет | Analyze job, crop/strength controls, benchmark shaky clips и browser/server capability fallback | XL |
| 99 | AI job/provider platform | Нет | Consent/progress/cancel/cost, local/remote execution и один end-to-end artifact job | XL |
| 100 | Sharing, review и version history | Нет | Backend/auth: ownership/revoke, immutable revision, timecoded comments и named snapshots | XL×3 |

## Зависимости и порядок поставки

1. **Project spine:** 10 → 1 → 2/9/13 → 64 → 16–17.
2. **Media spine:** 11–12 → 81 → 85 → 84 → 14–15/82.
3. **Playback spine:** 20 → 19 → 83 → 25.
4. **Motion/color spine:** 91 → 26–29/43; 26–27 → 36–37 → 41 → 38; 50 → 53.
5. **Captions/AI spine:** 56 → 57–59; 99 platform → remote 60/62 → 61/63.
6. **Scale and moat:** creator parity, затем P2/P3 и optional backend функции.

Первые релизные ворота: проект из 20 клипов; 4 video/audio tracks; 10 минут
timeline; 30 минут исходников; reload recovery; frame-accurate trim; A/V drift
меньше одного frame; отмена любого structural edit; export не требует держать
весь результат одновременно в JS heap.

## Агентская проверка

Каждая строка прошла основной продуктовый анализ и пять независимых полных ревью:

| Проход | Фокус | Охват |
|---|---|---|
| Основной | Код, продукт, ранжирование | 1–100 |
| Reviewer A | Resolve/Canva/Clipchamp и dependency spine | 1–100 |
| Reviewer B | Фактические gaps по frontend/backend | 1–100 |
| Reviewer C | CapCut/Premiere parity | 1–100 |
| Reviewer D | Effort и тестируемость DoD | 1–100 |
| Reviewer E | Browser limits и полнота NLE baseline | 1–100 |

По замечаниям исправлены dependency spines, browser memory gap, приоритет color
pipeline, граница baseline/large-project export и добавлены adjustment layers,
multicam и stabilization. Reverse и normalization отклонены как gaps, потому что
они уже реализованы.
