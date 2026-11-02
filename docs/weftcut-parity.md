# Перенос возможностей WeftCut в Video Kadr

Срез источника: 8 октября 2026, `d1db8020164c51ee49df9cf0143a5cbf250951a4`.
Источник: https://github.com/WeftCut/WeftCut. Исходник изучен локально, это не проверка запущенного WeftCut. Полный паритет пока не достигнут. Существующие возможности Video Kadr ниже установлены по коду и README, их полное интеграционное подтверждение остаётся отдельным этапом.

## Очерёдность реализации и критерии приёмки

| Область | Текущее покрытие Video Kadr | Что переносим / приёмка |
|---|---|---|
| Субтитры | SRT/TXT, text tracks, редактор стиля; в этом изменении VTT и ASS/SSA text/time | ASS styles, inline overrides, word timing, karaoke, caption packing на свободные дорожки; импорт одним Undo, сохранение/экспорт |
| MCP | Реализован opt-in HTTP MCP bridge к открытому редактору: sessions, project/history resources и batch tools | Streamable HTTP, loopback + bearer + Origin policy, resources/project/media/history, typed tools; действия в открытом проекте, live change feed |
| Агент и история | Undo/redo, in-memory checkpoints, dry-run, owner lock, Started/Ok/Err log | Sessions, журнал Started/Ok/Err, checkpoints, dry-run clone, history lock с владельцем; disconnect releases lock, batch rollback |
| Motifs | Не найден runtime HTML-слоёв | HTML/CSS/SVG/canvas/WebGL, manifest-driven inspector, изолированный runtime, чистая функция времени; одинаковые кадры seek/preview/export, перенос assets |
| Монтаж | Multitrack, split/trim/slip/ripple, snapping, markers | Проверить A/B-roll, gap selection, frame/SMPTE grid, cross-track selection, links/groups с атомарным отказом для locks |
| Вложенные композиции | Не подтверждено | CompositionRef, move-to-composition, flatten, корректные source clocks, исключить циклы; сохранение + export |
| Keyframes | Transform/opacity, multi-layer selection и curves; per-key interpolation, clipboard/retime/Alt-scale, Hold/Continue/Loop/PingPong/Offset; Bounce, Elastic и Bézier с числовыми controls | Graphical tangents/continuity, mixed-selection curve parameters, motion paths, colour keyframes, smooth; общий evaluator preview/export |
| Audio | Gain/pan automation, fades, denoise, ducking, detach audio; четыре роли, role gain/mute/solo, live meters, audition faders/reset, master mute/peak hold/compressor | Preview animated pan ещё не интерполирует коэффициенты по сетке 10 мс; квалифицировать общий audible clock и полный mixer export, включая master processing |
| Pauses | Локальный RMS detector и атомарный ripple | Сопоставить padding, source time и retiming; один Undo, preview detected regions |
| Speech | Ручные субтитры и voiceover recording; ASR/TTS не подтверждены | Опциональные whisper.cpp/FunASR/cloud adapters, extraction, transcription, TTS, word timing; явный выбор backend и ошибка без silent fallback |
| Footage understanding | Point tracker, beat detection; native shot scan/review, measures и frame comparison реализованы; vision descriptions не перенесены | Shot boundaries, brightness/motion/sharpness, frame comparison, shot review, local/endpoint scene descriptions и source-range cache |
| Text/effects/transitions | Text templates, chroma key, masks, blend modes, effect chains, 24 transitions | Сопоставить effect parameters/order, eyedropper из live frame, text bounds/outline и font portability |
| Поиск | Реализован Ctrl/⌘K поиск commands/media/clips/captions/markers, keyboard navigation и focus restoration | Проверить весь большой corpus, локализацию и parity ranking |
| Preview | Composition preview/proxies/filmstrip | Сверить zoom/pan, follow playhead, on-canvas gizmo, live edits during playback, accurate clock/colour |
| Export | H264/HEVC/AV1/ProRes/WebM, audio-only, range, bitrate/size | Encoder capability UI, HW/SW selection, streamed mux, background progress/cancel; real corpus frame/audio/colour conformance |
| Desktop | Web frontend + Rust/Axum | Отдельно решить shell/platform packaging, window memory, OS keyring и model installation; не копировать Electron архитектуру автоматически |

## Проверка первого изменения

- 59 тестов: subtitle parsers, composition state и reachable workspace controls.
- Проверка Svelte/TypeScript: 0 ошибок, 0 предупреждений; lint изменённых файлов, tokens, listener lifecycle и `git diff --check` прошли.
- Production build прошёл. Bundle gate не прошёл: исходный HEAD также превышает лимит (212.01 KiB JS / 228.36 KiB total), после изменения 213.36 / 229.71 KiB. Ограничения не повышались.
- Live browser и реальный burn-in export новых форматов пока не проверены.

## Второй этап: live MCP и поиск

- Rust `/mcp` (JSON HTTP, protocol 2025-03-26), bearer/peer/Host/Origin checks, отдельные ограниченные MCP sessions, DELETE отзывает сессию.
- Project/history resources читают фактически открытый документ; редактирование работает через polling bridge, один browser editor, stale revision rejection и одноразовую выдачу команды.
- Atomic batches: add/delete/reorder track, add/update/delete/move/trim/split layer, markers и transitions. `update_layer` использует существующие clip fields, включая keyframes/effects/masks; id/kind изменять нельзя. Каждая правка проходит текущую domain validation.
- Checkpoints (максимум 20, пока в памяти), restore как undoable edit; owner lock с TTL 60 секунд; disconnect releases lock через heartbeat; Started/Ok/Err журнал.
- Ctrl/⌘K search: commands/media/clips/captions/markers, bounded fuzzy results, клавиатура и переход к цели.
- 70 frontend tests, 5 Rust MCP tests, typecheck/lint/tokens/lifecycle и production build прошли. Реальный Chromium с новым backend и пустым временным storage подтвердил dry-run, batch, live text clip, stale-revision refusal, Undo/Redo, restore, caption search, resources и session disconnect.
- Bundle gate остаётся красным: 217.63 KiB JS, 234.17 KiB total, workspace 84.00 KiB. Лимиты не повышены; исходный HEAD также превышал их, но новые функции добавили размер.
- Полного MCP-паритета пока нет: compute/render/library tools, live change feed, prompts и расширенная agent UX остаются открыты. Нет реализованного Motif runtime, nested compositions и ASR/TTS/VLM adapters.
- Подключение и API: [agent-mcp.md](agent-mcp.md). Воспроизводимая браузерная проверка: `cargo build --manifest-path backend/Cargo.toml`, затем `npm --prefix frontend run test:e2e:agent` (нужен Playwright Chromium; можно задать `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH`).

## Третий этап: анализ и просмотр сцен

- Rust/FFmpeg whole-source scene floor scan 0.05, cache по полному hash файла и FFmpeg version; повреждённый кеш пересчитывается. Проверка изменения файла во время расчёта запрещает запись устаревших результатов.
- Порог, минимальная длина, отключение границ и выбор сохраняемых сцен; source clocks учитывают reverse, constant speed и speed ramp. Разбиение, маркеры и исключение используют общие frame-snapped cuts и один Undo. Настройки обзора не создают Undo и сохраняются через историю/checkpoints.
- Три кадра на сцену: brightness, 1-MSSIMSimple motion, Laplacian variance sharpness, black/freeze/fade; отдельный вызов измерений. Сравнение кадров с PNG before/after, сохранение пропорций и rotation metadata. UI открывается лениво; выбор клипа не запускает анализ.
- MCP analyze_clip, measure_shots, compare_frames, auto_split_by_shot, mark_shots, apply_shot_cuts. Во время расчётов heartbeat поддерживает editor lease; late edits после disconnect/expiration/revision change запрещены.
- Проверено: 3 native metric/FFmpeg tests, 2 HTTP source/cache/frame tests, 5 MCP tests, 21 frontend tests, TypeScript/Svelte, lint, tokens и lifecycle. Реальный Chromium + FFmpeg: agent scan/measure/compare, PNG review, atomic split и Undo через MCP и кнопку интерфейса.
- Ещё открыты: linked A/V partners (общая links модель пока не перенесена), автоматические cover thumbnails, описания сцен через VLM, live/export qualification всего корпуса. Полный Shot review parity не отмечен закрытым.

Production build проходит. Bundle gate остаётся красным: 227.09 KiB JS / 243.96 KiB total, workspace 84.74 KiB; исходный HEAD уже превышал лимиты, но перенос увеличил размер. Лимиты не повышались.

## Четвёртый этап: связи клипов (в работе)

- Flat links с уникальными участниками, create/reassign/remove, валидация при каждом commit, автоматическое растворение после локального удаления до одного участника.
- Move переносит общую дельту, только основной клип меняет дорожку, clamp у начала применяется ко всему набору. Итог проверяется атомарно, включая collisions и locks. Split делит каждого участника, пересекающего время, и сохраняет все части в связи.
- Панель «Связи клипов» создаёт/разрывает связь одним Undo. MCP batch имеет create_link/remove_link и escapeLink для move_layer/split_layer.
- 452 frontend tests прошли, включая новые link regressions; 5 MCP tests прошли. Typecheck/lint и build прошли. Исправлен прежний first-poll backoff 1000→500 ms по существующему regression test.
- Ещё не перенесены: enabled selection, sample-lattice nudges/resync, import auto-pair и shot rejection linked spans. Groups остаются отдельной задачей nested compositions. Раздел Links пока не закрыт.
- Real Chromium прогон прошёл: создание/разрыв связи кнопками, MCP linked move/split, Undo и disconnect; HMR отключён в harness, чтобы обновление кода не прерывало сценарий. Bundle gate: 228.63 KiB JS / 245.52 KiB total / workspace 85.38 KiB; лимиты не повышались.

## Пятый этап: совместная обрезка

- Trim распространяется только на края с точным совпадением времени; несинхронный участник с несовпадающим краем остаётся на месте. Общая дельта расширения ограничена source bounds всех затронутых участников, с учётом constant speed и reverse. Для speed ramp расширение за authored window пока ограничено существующим evaluator; slicing внутри окна доступен.
- Lock затронутой дорожки отклоняет всю правку. trim_layer получил escapeLink; стандартный trimClip используется теми же UI-командами и MCP.
- Проверено: 454 frontend tests, 5 MCP tests, typecheck/lint и production build; Chromium подтвердил linked move/trim/split, explicit escape и Undo. Отдельный regression проверяет reverse source bounds. Bundle gate остаётся красным: 228.92 KiB JS / 245.81 KiB total / workspace 85.37 KiB.

## Шестой этап: копирование связей

- Duplicate создаёт копии всех участников с общей дельтой времени. Только seed может перейти на другую дорожку. Копии получают отдельную связь; исходная связь сохраняется. escapeLink:true создаёт одну независимую копию.
- UI «Копировать связь после исходных» и стандартный duplicate выбранного клипа используют единый evaluator, с размещением после исходного span. Locks/collisions/duplicate IDs отклоняют весь результат. MCP batch duplicate_layer имеет clipId/id, trackId?, startTicks?, linkId?, escapeLink?.
- 456 frontend tests прошли, 5 MCP tests; typecheck/lint/build прошли. Chromium проверил копирование кнопкой и MCP, relative phase, отдельную связь, один Undo. Отдельные regressions покрывают locked partner и collision только у партнёра. Bundle gate: 229.25 KiB JS / 246.14 KiB total / workspace 85.57 KiB, лимиты не повышены.
- Ещё открыты: exact-subject paste_layers, linked selection/Alt gestures и ghosts, sample-lattice snapping, auto-paired import, shot rejection linked spans, nested compositions. Links не отмечен полностью закрытым.

## Седьмой этап: временное отключение связей

- Session-only link override: UI-кнопка «Править клипы отдельно», Alt+Shift+G, переназначаемая команда и запись в поиске. Move/trim/split/duplicate из UI получают escapeLink; переключатель не меняет документ, revision или Undo и не сохраняется в проект.
- MCP не consult-ит session override: агент продолжает редактировать связь, если не указал escapeLink. Undo сохраняет текущий session switch.
- Участники одной связи отмечены 2 px цветным краем, цвет стабилен от link id; override снижает alpha до 0.4. Статус виден в таймлайне.
- 458 frontend tests прошли; typecheck/lint/build/tokens/lifecycle прошли. Chromium подтвердил Alt+Shift+G, отдельный nudge, сохранение override после Undo и linked MCP edits при включённом override. Клавиатурные проверки фокусируют timeline lane: существующие глобальные shortcuts не перехватываются из полей и кнопок.
- Bundle gate остаётся красным: 229.76 KiB JS / 246.65 KiB total / workspace 85.83 KiB. Alt duplicate gestures/ghosts, exact-subject paste, sample nudges/resync, AV import auto-pair, linked shot rejection и Groups остаются открытыми.

## Восьмой этап: выделение набора клипов

- UI хранит primary clip для инспектора и точный selectedClipIds set. Plain select расширяется по связи, Shift toggle снимает/добавляет набор, Alt выбирает одного участника, session override также ограничивает выбор одним. Deselecting Shift-click не начинает drag. Visual highlights и semantic aria-pressed согласованы.
- Выбор одного участника сохраняется после move и отключает fan-out UI move/trim/split/duplicate. MCP использует только explicit escapeLink. Delete берёт точный selection set, фиксирует один Undo и отказывает атомарно при locked member track. Исчезнувшие ids удаляются из selection, undersized links растворяются.
- 460 frontend tests прошли, включая selection/toggle, local deletion, whole-link deletion/Undo и locked partner. Typecheck/lint/build/tokens/lifecycle прошли. Real Chromium подтвердил plain/Shift/Alt selection, local delete и whole-link delete одной записью Undo; остальные MCP/shot/override сценарии также прошли. HMR явно отключён в E2E harness.
- Bundle gate: 230.26 KiB JS / 247.15 KiB total / workspace 85.99 KiB, лимиты не повышались.
- Открыты: marquee, shared movement/duplicate arbitrary cross-track selections, exact-subject paste, enable/disable и clip locks, Alt duplicate drag/ghosts вместо прежнего slip gesture, multi-selection ripple semantics, sample lattice, AV auto-pair, linked shot rejection, nested compositions. Полный Timeline selection/Links parity ещё не подтверждён.

## Девятый этап: Link / Unlink для выделения

- Ctrl/⌘L — единый toggle: selection внутри общей связи разрывает её целиком; два или более unlinked clips создают новую связь. Mixed linked/unlinked selection или разные связи блокируются с причиной и требуют явного reassign. Locked track любого затронутого участника запрещает правку, включая невыбранных участников разрываемой связи.
- Команда доступна в toolbar с динамической надписью, поиске и переназначаемых shortcuts. Использует общую валидацию links и один Undo. UI selection сохраняется; повтор команды может создать связь заново.
- 463 frontend tests прошли. Chromium подтвердил Control+L unlink/relink и по одному Undo; unit tests покрывают mixed selection и locked unselected partner. Typecheck/lint/build прошли. Bundle gate остаётся красным: 230.78 KiB JS / 247.67 KiB total / workspace 86.04 KiB; лимиты не повышались. Full Links parity остаётся открытым: enabled/clip locks, samples/resync, AV auto-pair, linked shot rejection, drag duplicate/ghosts, exact-subject paste и произвольное перемещение нескольких клипов пока не перенесены.

## Десятый этап: точный набор при копировании

- pasteClips / MCP batch paste_layers копирует ровно subjects [{clipId,id}]. Первый subject задаёт clock anchor и единственный изменяемый target track; остальные сохраняют дорожки и относительные смещения. Отрицательная общая дельта ограничивается началом timeline для всего набора.
- Исходные link subsets восстанавливаются между копиями только при 2+ выбранных участниках; связь копий независима от исходной. Один участник копируется unlinked, невыбранные партнёры не расширяют subjects и их locks не мешают операции. Locks/collisions/invalid ids отклоняют весь paste без commit.
- Ctrl/⌘D теперь копирует произвольное selectedClipIds выделение с нескольких дорожек одним Undo и выделяет все новые клипы. No-op move/Shift-click не схлопывает selection до primary clip. MCP options: trackId?, startTicks?, linkIds? (по порядку исходных links, содержащих 2+ subjects).
- 465 frontend tests, 5 MCP tests, typecheck/lint и frontend/backend build прошли. Chromium подтвердил single/pair paste по MCP, независимую связь, phase, один Undo, а также Ctrl+D двух unlinked clips и Shift selection. Unit test покрывает subset из 2 участников исходной связи из 3 и locked unselected partner.
- Bundle gate остаётся красным: 231.21 KiB JS / 248.10 KiB total / workspace 86.07 KiB. Shared move/ripple для произвольного набора, marquee, enabled и clip locks, sample lattice/resync, drag duplicate/ghosts, AV auto-pair, linked shot rejection и nested compositions остаются открытыми.

## Одиннадцатый этап: перемещение выделенного набора

- moveClips / MCP batch move_layers перемещает ровно clipIds общим delta. Первый id — anchor и единственный клип, которому меняется track. Clamp у начала timeline действует на весь набор; links не расширяют explicit subjects.
- Новый evaluator заменил прежний fan-out moveClip для links и строит все назначения одновременно, поэтому adjacent clips на одной дорожке не вызывают промежуточный overlap. Итоговые collisions, locks, incompatible tracks и transitions при смене дорожки запрещают весь commit.
- UI move/nudge использует selectedClipIds; выделение сохраняется. Locked selected member отключает delete/duplicate/nudge с причиной. Для single selection сохраняются explicit escape и session override.
- 467 frontend tests и 5 MCP tests прошли; targeted timeline/selection/link tests также прошли (24). Typecheck/lint и frontend/backend build прошли. Chromium подтвердил exact-subject move_layers по MCP, UI nudge двух unlinked clips с сохранением phase, один Undo и прежние сценарии.
- Bundle gate остаётся красным: 231.35 KiB JS / 248.24 KiB total / workspace 86.10 KiB. Открыты marquee/несколько drag ghosts и preview admissibility, multi-selection ripple, enabled/clip locks, sample nudges/resync, AV import auto-pair, linked shot rejection, nested compositions, Motifs и speech/vision adapters.

## Правила переноса

Использовать текущие composition commands, history и Rust render pipeline. Новые функции обязаны иметь UI и программный путь с одинаковой валидацией. Не считать импорт ASS полным: сейчас оформление намеренно упрощается, интерфейс сообщает об этом. VTT settings/regions также не переносятся в text clips. Не считать feature inventory доказательством поведения. Каждый закрытый пункт требует regression и, для preview/export, реального media fixture.

При копировании MIT-кода сохранить copyright/license notice. Парсер субтитров написан для типов Video Kadr. Метрики сцен адаптированы из WeftCut с атрибуцией в Rust-модуле; полный MIT notice сохранён в docs/licenses/weftcut-MIT.txt.

## Инвентарь контрактов источника

Ниже все разделы функциональных документов выбранного среза. Использовать их как checklist детализации, чтобы не ограничиваться README. Статус каждого пока **не проверен на полный паритет**.

### [features](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/features.md)

- [ ] Undo-stack scope
- [ ] Timeline selection
- [ ] Keyframe curves
- [ ] Colour keyframing
- [ ] Links
- [ ] Groups
- [ ] Split at the playhead
- [ ] Ripple delete
- [ ] Track placement
- [ ] Timeline scrolling
- [ ] Timeline zoom
- [ ] Preview zoom and pan
- [ ] Follow playhead
- [ ] Markers
- [ ] Shot review
- [ ] Transcribe and voiceover
- [ ] Pauses
- [ ] Global search palette
- [ ] Color picker (eyedropper)
- [ ] On-canvas transform (gizmo)
- [ ] Window geometry memory
- [ ] macOS window caption
- [ ] Rectangular video crop

### [mcp](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/mcp.md)

- [ ] Transport & deployment
- [ ] Authentication
- [ ] Connection UX
- [ ] Multi-agent semantics
- [ ] Tool surface
- [ ] Tool description quality
- [ ] Error model
- [ ] Change feed
- [ ] Speech (optional, user-supplied)
- [ ] Video understanding (optional, user-supplied)
- [ ] Observability
- [ ] Concurrency policy
- [ ] Security
- [ ] Time remapping

### [motifs](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/motifs.md)

- [ ] The principle, and the obstacle
- [ ] Boundaries
- [ ] Authoring contract
- [ ] Parameter UI
- [ ] Determinism: owning the clock
- [ ] Capture harness
- [ ] Raster cache and escalation
- [ ] Compositor integration
- [ ] Export
- [ ] User Motifs
- [ ] Agent surface
- [ ] Security

### [motif-authoring](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/motif-authoring.md)

- [ ] The one law
- [ ] Lifecycle: `motif.define`
- [ ] The manifest
- [ ] Runtime environment
- [ ] Parameter UI
- [ ] Authoring over MCP

### [motif-workspaces](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/motif-workspaces.md)

- [ ] Local directory workflow
- [ ] Remote/file-only workflow
- [ ] Review, publication and export

### [audio](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/audio.md)

- [ ] The authoring grid
- [ ] The conform cache
- [ ] The envelope contract
- [ ] Preview mixer
- [ ] Roles
- [ ] Clip effects (baked)
- [ ] Export mixer
- [ ] Readiness and errors
- [ ] MCP surface
- [ ] Testing
- [ ] Out of scope here, designed elsewhere or later
- [ ] Constant-rate time remapping

### [captions](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/captions.md)

- [ ] The model, in one paragraph
- [ ] Ingestion — one parser, one mutation
- [ ] Text correction / 文字校正
- [ ] Cue layout and ASS support
- [ ] Fonts and burn-in determinism
- [ ] Editing — the Caption panel
- [ ] Scope
- [ ] Pointers

### [export](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/export.md)

- [ ] Export settings and range
- [ ] Audio-only export
- [ ] Final mux
- [ ] Coverage
- [ ] Proxies
- [ ] Background jobs

### [preview](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/preview.md)

- [ ] Component tree
- [ ] Clock and transport
- [ ] Decode
- [ ] Decode engine
- [ ] Scrub
- [ ] Audio
- [ ] Proxies
- [ ] Motifs
- [ ] Diagnostics
- [ ] Render & Play

### [data-model](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/data-model.md)

- [ ] Foundational decisions
- [ ] Top-level shape
- [ ] `Composition`
- [ ] `MediaItem`
- [ ] `Track`
- [ ] `Layer`
- [ ] Animated values
- [ ] `Marker`
- [ ] `Link`
- [ ] Read surface: `ProjectSummary`
- [ ] History
- [ ] Concurrency: single-writer actor
- [ ] `ChangeEvent`
- [ ] Validation invariants (enforced inside the actor on every commit)
- [ ] Mutation surface
- [ ] On-disk format: workspace folder
- [ ] App-level storage: the data root
- [ ] Versioning
- [ ] Pitfalls
- [ ] Time remapping (schema 3)

### [status-log](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/status-log.md)

- [ ] System shape
- [ ] Backend
- [ ] Entry schema
- [ ] Producers
- [ ] Frontend
- [ ] Agent activity is a separate stream
- [ ] Deferred

### [performance-settings](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/performance-settings.md)

- [ ] Authority and lifecycle
- [ ] Coverage
- [ ] Disk safety
- [ ] Compatibility and optional measurements
- [ ] Verification

### [timeline-content-preview](https://github.com/WeftCut/WeftCut/blob/d1db8020164c51ee49df9cf0143a5cbf250951a4/docs/timeline-content-preview.md)

- [ ] Layer Rendering Rules
- [ ] Label and Chrome Rules
- [ ] Width-Based Degradation
- [ ] Loading and Failure Behavior
- [ ] Data and IPC
- [ ] Frontend Structure
- [ ] Visibility and Caching
- [ ] Coverage
- [ ] Pointers


## Этап 12: ripple delete выделения

`rippleDeleteClips` удаляет точный набор клипов и закрывает объединение реально освобождённых интервалов на всех дорожках. Общий диапазон парных клипов учитывается один раз; соседние существующие пробелы сохраняются. Заблокированная дорожка мешает только при удалении или фактическом сдвиге. Клипы внутри отверстия, связи через его границу, изменённые переходы и пересечения после сдвига отклоняются до изменения редактора. UI использует всё выделение, Shift+Delete / Shift+Backspace; MCP batch предоставляет `ripple_delete_layers`.

Ограничения: текущая модель сохраняет длительности при сдвиге в тиках; сетки кадров/48 kHz, перенормировка переходов при дробном fps, anchored markers, pinned duration и Groups ещё не перенесены. Это частичный перенос раздела Ripple delete.

## Этап 13: закрытие одного пробела

`compositionGapAt` определяет точный свободный полуоткрытый интервал между границами клипов, включая начальный пробел; пустая дорожка и хвост после последнего клипа не являются пробелами. `rippleDeleteGap` проверяет точные актуальные границы и использует общий с удалением клипов алгоритм сдвига/отказов. UI предоставляет кнопку закрытия пробела у плейхеда для каждой дорожки; действие записывается одним Undo. MCP batch: `ripple_delete_gap`.

Проверяется разрешение границ, отклонение устаревших/частичных запросов, сдвиг всех дорожек без удаления клипов, блокировки, клипы внутри пробела и UI Undo. Выделение пробела кликом с подсветкой и клавиатурная навигация по пробелам ещё не перенесены. Ограничения сеток, Groups и связанных маркеров этапа 12 сохраняются.

## Этап 14: выделение пробела

Клик по пустому месту дорожки выделяет точный ограниченный пробел и очищает выделение клипов. Диапазон подсвечивается; Delete / Backspace закрывают его на всех дорожках одним Undo, Escape снимает выделение. Выбор клипа и смена проекта очищают пробел; `repairSelection` после изменения документа удаляет устаревший диапазон. Выделение хранится только в UI, не в документе/истории. Автоматический UI тест проверяет клик, подсветку, удаление, Undo и изменение соседней границы.

## Этап 15: клавиатурный выбор и проверка пробела

Alt+стрелки на дорожке выбирают предыдущий/следующий ограниченный пробел, не двигают плейхед и не создают историю. Диапазоны перечисляются независимо от порядка клипов и учитывают перекрывающиеся хвосты. Кнопка закрытия выделенного пробела заранее вызывает тот же чистый алгоритм, показывает отказ и не выполняет запрещённое действие. UI tests подтверждают выбор и Escape; context menu и полный реестр переназначаемых gap-команд остаются открытыми.

## Этап 16: модель включения клипов и предпросмотр

Необязательный `enabled` для всех типов клипов сохраняется в проекте; старые документы означают `true`. `setClipsEnabled` и MCP `set_layers_enabled {clipIds,enabled}` используют точный набор без расширения связей, проверяют всю выборку до изменения. Заблокированный выбранный участник отменяет весь набор, невыбранный связанный участник не мешает. Команда состояния записывает одно Undo. Предпросмотр исключает отключённые визуальные и аудиоклипы, не создаёт переход с отключённым концом.

Экспорт, UI переключатели и блокировки отдельных клипов ещё предстоит подключить; этот этап не доказывает завершённый Enable/Disable.

## Этап 17: переключатели включения в UI

Инспектор меняет `enabled` только показанного клипа, включая multi-selection. Кнопка монтажной линии меняет точное выделение: смешанное или отключённое предлагает «Включить», полностью включённое — «Выключить», показывает число клипов и блокируется при выбранной locked-дорожке. Отключённые клипы визуально приглушены. Проверяется Undo и независимость инспектора от multi-selection. Пока UI явно предупреждает, что отключение действует в предпросмотре, а экспорт ещё не подключён. Контекстное меню и блокировки отдельных клипов также остаются открытыми.

## Этап 18: флаг включения в экспортном контракте

Wire payload всех четырёх видов клипов передаёт `enabled` вместо постоянного true; отсутствие поля в старом проекте означает true. Переход с отключённым концом исключается только из экспортного payload, авторский документ сохраняется. Клиентская проверка допустимости рендера рассматривает активные клипы и переходы; выбор основной дорожки совпадает с серверным enabled-набором. Сервер уже фильтрует disabled-клипы. Новый native test проверяет отсутствие декодерных inputs отключённого видео и аудио и сохранение авторской длительности, frontend test — wire-флаги четырёх типов.

Ограничения: экспортёр требует непрерывную основную видеодорожку, поэтому отключение среднего участка может потребовать будущей поддержки фоновых промежутков. Реальный экспорт с измерением кадров/звука этого сценария ещё не проверен; завершение Enable/Disable и всего переноса не заявляется.

## Этап 19: реальное доказательство disabled overlay/audio

`backend/tests/composition_render.rs::real_disabled_overlay_and_audio_change_pixels_and_sound_without_changing_clock` создаёт красное основное видео, синее видео оверлея и отдельный 440 Hz WAV. Компилирует и выполняет экспорт с двумя состояниями enabled, затем читает RGB и stereo PCM из готовых MP4. При true синий канал доминирует, stereo RMS ≈0.06574619; при false красный канал доминирует, RMS [0,0]. Оба файла имеют измеренную длительность ровно 1.000000 s. Проверка выполнена локальным FFmpeg/ffprobe, не пропущена; её отсутствие инструментов завершает тест ошибкой. Временные источники и результаты удаляются через tempdir.

Это доказательство покрывает отключение визуального оверлея и независимого аудиоклипа с неизменной непрерывной основной дорожкой. Оно не доказывает отключение среднего основного клипа, фоновые промежутки, Groups, весь экспорт или полный перенос WeftCut.

## Этап 20: фон и тишина в пробелах основной дорожки

Экспортёр добавляет цветные фоновые и тихие audio-сегменты в начальные и внутренние пробелы, а также сохраняет авторский хвост с отключёнными клипами. Активные клипы не сдвигаются. Клиент больше не запрещает пробелы без переходов; overlaps остаются запрещены. Реальный FFmpeg-тест дополнен пятисекундной дорожкой: чёрный фон и RMS <0.0001 на 0.5/2.5/4.5 s, красные активные кадры на 1.5/3.5 s, измеренная длительность 5.000000 s.

Открыто: пробелы одновременно с primary transitions, checker/blur фон без исходного кадра, полностью отключённая основная дорожка, квалификация дробных границ кадров. Эти случаи не объявлены перенесёнными.

## Этап 21: пробелы одновременно с переходами

Primary concat/xfade теперь включает фоновые видео-сегменты и тихие audio-сегменты в правильном порядке. Начальный пробел входит в текущую видеопоследовательность до первого перехода; внутренний пробел добавляется между участками; хвост присоединяется после последнего клипа. Xfade offset остаётся абсолютной точкой монтажной линии. Клиентский запрет сочетания пробелов с переходами снят.

Реальный FFmpeg regression: начальный пробел 0–0.5 s, красный/синий dissolve в 1 s, внутренний пробел 1.5–2 s, красный участок 2–2.5 s и отключённый хвост 2.5–3 s. Фон на 0.25/1.75/2.75 s чёрный; красный и синий участки сохраняют размещение; RGB в 1 s [127,0,127]; длительность результата 3.000000 s. Native 49 tests остаются зелёными. Полный перенос не завершён: остаются checker/blur пустых участков, полностью отключённая основная дорожка, дробные границы и другие разделы inventory.

## Этап 22: шахматный фон пустых участков

Primary gap-сегменты теперь используют выбранный Checker вместо сплошного canvas color. Цвет и шаг клеток совпадают с существующим checker фоном активного primary: клетки 64 px, light = channel + 28 с ограничением 255. Аудиопробел остаётся тихим. Реальный FFmpeg regression на canvas 128×64 проверяет разные соседние клетки на 0.25/1.75/2.75 s (начало, середина, хвост) и длительность 3.000000 s на дорожке с dissolve. Тест выполнен и не пропущен.

Для Blur в отсутствие активного исходного кадра остаётся сплошной canvas color; требуется сверка полной политики фона с WeftCut. Полностью отключённая основная дорожка, дробные границы и остальные разделы inventory остаются открытыми.

## Этап 23: контекстное меню клипов

Правый клик и Shift+F10/ContextMenu открывают меню точного текущего выделения. Если клип уже выбран, меню не разрушает multi-selection; если не выбран, используется обычная link-aware selection (Alt позволяет одиночную). Enable/Disable показывает количество и смешанное состояние, удаление и ripple работают одним Undo. Locked-набор отключает действия; ripple заранее вызывает тот же чистый алгоритм и показывает причину отказа. Стрелки/Home/End перемещают фокус, Escape закрывает и возвращает его клипу; клик снаружи закрывает меню. Правая кнопка не начинает drag. UI tests проверяют multi-selection, Undo, keyboard opening, locks и Escape. Отдельные clip locks ещё не перенесены.

## Этап 24: блокировки отдельных клипов

Необязательный `locked` сохраняется для всех типов клипов, старые документы считаются unlocked. Команды move/trim/split/slip/duplicate/delete/enable проверяют выбранных участников; UI commit и MCP проверяют неизменность содержимого, источника, связей и переходов защищённого клипа. MCP выполняет проверку после каждой операции, поэтому lock-only unlock и последующее редактирование могут быть отдельными операциями одного атомарного batch. Lock-only patch/toggle доступен на locked track. Исторический restore/Undo/redo восстанавливает снимки. Автоматическое растворение связи при локальном удалении невыбранного соседа допускается.

Инспектор предоставляет lock toggle вне read-only fieldset; body click защищённого клипа сохраняет inspection/selection, но не начинает drag. Timeline показывает badge, context menu/command availability учитывают clip locks. Tests проверяют linked fan-out отказ, MCP прямую patch/unlink защиту, Undo и unlock на locked track. Полная квалификация косвенных операций, source changes, transitions и всех жестов ещё не завершена.

## Этап 25: косвенные изменения защищённых клипов

Regression tests подтверждают MCP отказ при смене/удалении перехода защищённого участника и при удалении его незащищённого партнёра, защиту метаданных источника, обязательный lock-only unlock перед content patch и разрешённые free markers. Checkpoint restore поверх более позднего защищённого содержимого остаётся undoable. Дополнена commit-защита медиа-привязки: даже при неизменном sourceId и одинаковых source metadata замена URL/filename/прочих binding полей защищённого источника отклоняется до истории/документа. Исторический restore намеренно восстанавливает binding.

Полный lock audit, жесты и остальные функции WeftCut всё ещё требуют продолжения; покрытие этих сценариев не означает завершение inventory.

## Этап 26: прямые команды и косвенная защита

Прямые domain-команды удаления дорожки, добавления/замены/удаления перехода проверяют защищённые участники. Удаление клипа дополнительно проверяет косвенное изменение перехода его защищённого партнёра, а локальное растворение связи сохраняет ранее проверенное исключение. Magnet/ripple выполняют проверку содержимого защищённых клипов до возврата результата; удаление тишины проверяет сдвигаемых downstream participants. Создание/reassign связи проверяет результат, включая невыбранных защищённых участников старой связи.

Regression tests подтверждают direct transition/track refusal, отказ уплотнения защищённого downstream и разрешённый неподвижный protected head. Это расширение lock qualification, а не завершение всей миграции.

## Этап 27: атомарное создание импортной пары видео/аудио

Добавлена чистая операция пары из видео с аудиопотоком: отдельный AudioClip наследует источник, временной диапазон, положение, скорость/reverse, gain/pan и enabled; встроенный звук VideoClip отключается, участники связываются. Импорт с `pairAudio: true` публикует пару и медиа-привязку одним commit/Undo. Регистрация binding теперь происходит только после успешного построения документа, без предварительного изменения живого состояния.

Regression tests проверяют фазу и reverse, отсутствие мутации входа при отказах (нет аудио, locked, занятые идентификаторы) и восстановление документа/binding одним Undo. Обычный импорт пока сохраняет прежний режим: WeftCut default-on настройка и её UI, подключение всех import entrypoints и реальная квалификация экспорта пары остаются открытыми. Этап не закрывает полный раздел import auto-pair и общий inventory.

## Этап 28: default-on import pairing и настройка

Обычный `addMediaInfoToComposition` теперь использует default-on app preference; его вызовы из медиатеки (`addLibraryEntryToComposition`) и capture используют ту же политику. Видео без аудиопотока не получает пустую аудиодорожку. Явный `pairAudio` остаётся override для вызывающего кода. Компактный checkbox в workspace сохраняет переключение в localStorage, отдельно от проекта/Undo; недоступное хранилище не мешает текущей сессии, отсутствующее/невалидное значение трактуется как enabled.

Существующие tests встроенного звука явно выбирают выключенную политику; добавлены проверки default preference, сохранения opt-out, обоих import entrypoints и отсутствия аудиопары у silent video. UI regression проверяет checkbox и отсутствие изменений документа/истории. Реальная FFmpeg-квалификация импортной пары и остальные разделы inventory остаются открытыми.

## Этап 29: реальный экспорт импортной AV-пары

Native integration test `real_imported_audio_pair_has_one_audio_owner_and_preserves_offset` генерирует MP4 с видео и AAC, использует общий video source для VideoClip и AudioClip, source-in 0.5 s, source-out 1.5 s и timeline-start 0.5 s. FFmpeg экспорт проверяет три варианта: embedded-only, paired-only, намеренно включённые оба владельца. Измеренный stereo RMS соответственно 0.06250934 / 0.06580650 / 0.13163521. Paired/embedded сопоставляются с допуском 8%, double/paired должен быть >1.8: контроль подтверждает чувствительность проверки к двойному звуку. Во всех вариантах длительность 1.500000 s, первые 0.1–0.3 s тихие; видео в активном участке красное. Тест выполнен, не skipped.

Это квалификация постоянной скорости/обычного воспроизведения пары через native compiler, а не доказательство всех playback/ramp/preview режимов или всего inventory. Полный перенос остаётся незавершённым.

## Этап 30: точная позиция копирования выделения

`duplicateSelectedCompositionClip` принимает необязательный drag destination (`trackId`, `timelineStartTicks`) и передаёт его одной atomic paste операции без поиска другого свободного места. Копируются точные выбранные subjects, новые связи независимы от исходных, destination track применяется к anchor, остальные участники сохраняют дорожки/фазу. Обычный Ctrl+D сохраняет прежний поиск свободного места. Regression test проверяет связанную копию в 3 s, выделение clones, один Undo и атомарный отказ при collision неякорного участника.

Это подготовка state API: body Alt+drag пока остаётся slip в UI; жест, ghosts и предварительный collision indicator ещё открыты. Полный раздел gesture parity не закрыт.

## Этап 31: body Alt-drag создаёт копию

Timeline body Alt-drag вызывает exact-destination paste через state API этапа 30 для всех clip kinds. Если anchor уже выделен, точный subject set сохраняется (включая заранее Alt-selected одиночный member); иначе применяется обычная link-aware selection. Anchor под pointer становится primary. Смещение менее 3 px трактуется как Alt-click одиночного участника без истории; остальные release копируют одним commit. Trim Alt escape и keyboard slip остаются отдельными механизмами. UI regression проверяет pointer down/move/up, неизменность исходного старта, копию в 11 s и один Undo.

Multi-ghost rendering, предварительный collision/locked indicator и полный linked/single/zero-distance/cancel жестовый regression audit остаются открытыми. Это не завершение source gesture parity.

## Этап 32: ghosts и предварительный admission копирования

Во время duplicate gesture исходные клипы остаются на месте, каждый selected subject получает отдельный dashed ghost в своей дорожке; anchor ghost следует destination track. Общий delta повторяет origin clamp pure paste. Предварительная проверка вызывает чистую `pasteClips` с временными незанятыми stable IDs, без commit/Undo; её collision/locked/kind ошибки окрашивают все ghosts красным. Исчезновение clip во время gesture обрабатывается без исключения из reactive preview. Release по-прежнему вызывает настоящую atomic paste с новыми IDs и повторной проверкой текущего документа.

UI regression подтверждает invalid ghost над исходником, valid ghost после перемещения и отсутствие history до release; 13 timeline tests проходят. Typecheck и lint пройдены. Полная linked multi-ghost, cross-track, Alt-click/cancel и keyboard/viewport qualification ещё открыта, остальные разделы inventory также не закрыты.

## Этап 33: linked/solo/cancel/collision gesture regression

Дополнена UI qualification pointer down/move/up/cancel на реальной импортной video/audio паре. Fresh member selection раскрывается в два subjects при Alt-drag, отображаются два ghosts; clones получают независимую связь без исходных IDs и один Undo. Alt-click без движения выбирает один member без истории; следующий Alt-drag даёт один ghost, один unlinked clone и сохраняет исходную пару. Pointercancel и collision release удаляют preview, не меняют документ/историю. Fixtures восстанавливают hit-test и unmount через finally.

17 timeline tests проходят. Эти проверки не покрывают физический browser pointer capture, autoscroll/viewport, cross-track drop и все locked variants; они остаются открытыми вместе с остальным inventory.

## Этап 34: вертикальный drop и locked sibling

Alt-click threshold теперь использует расстояние по X/Y: чисто вертикальное перемещение на другую дорожку больше не считается кликом. UI regression проверяет заранее одиночно выделенный video member с vertical-only drop (копия на target в 0 s), cross-track linked copy с двумя участниками и locked audio sibling. Locked sibling отображает invalid ghosts и отклоняет release без изменения документа/истории; успешные drops оставляют исходник, имеют один Undo. 20 timeline tests проходят.

Реальный pointer capture/autoscroll и другие source gesture cases остаются в qualification backlog; полный inventory ещё не закрыт.

## Этап 35: gap context menu

Right-click пустого bounded gap выбирает его без seek и открывает меню `Удалить пробел со сдвигом`. Shift+F10/ContextMenu на lane открывает меню ранее выбранного gap. Admission использует pure rippleDeleteGap и показывает refusal через aria-disabled/title; action вызывает существующий state commit одним Undo. Shared menu поддерживает Escape/focus restoration/outside dismissal. Snapshot gap в menu сравнивается с актуальной selection: изменённый/исчезнувший gap закрывает stale menu. Right-click вне gap закрывает старое меню.

UI regression подтверждает right-click открытие, успешное закрытие 3–5 s gap/Undo, disabled action при protected downstream, отсутствие истории при отказе и возврат фокуса Escape. 22 timeline tests и typecheck проходят. Source gap меню с более широкими действиями, keyboard qualification и остальные inventory sections ещё требуют продолжения.

## Этап 36: keyboard gap-menu и stale span qualification

Сверен source gap context behavior (`docs/features.md`, A selected gap closes under the same closing): контекстная строка закрывает пустой span той же ripple операцией, что Delete/Backspace/Shift+Delete. Дополнительных действий в этой source спецификации не требуется. UI regression проверяет gap selection Alt+Arrow, Shift+F10/ContextMenu opening, auto-focus единственной action, End navigation, Escape/focus restoration, отсутствие seek/history от открытия, закрытие меню при изменении границы gap и отсутствие меню без selected span. 23 timeline tests проходят.

Source также требует frame/sample lattice, pinned duration и Groups/CompositionRef ripple поведение; текущий tick planner не доказывает эти требования. Они и общий inventory остаются открытыми.

## Этап 37: rational time-grid arithmetic

Добавлен чистый `timeGrid.ts`: rational rate numerator/denominator, integer BigInt округление canonical microsecond boundaries, независимый snap обоих endpoints и audio lattice 48 kHz. Tests показывают смену длины fractional frame при переносе (30000/1001: 33367→33366 ticks), разные результаты одного 21 µs shift для visual/sample grids, точность длинного timeline и invalid/overflow отказ.

Это arithmetic foundation, ещё не подключённый к ripple/move/export. Текущая canvas fps хранится числом: rational rate persistence, изменение landed durations/source windows/transitions и admission на этих grids должны быть перенесены вместе. Три unit tests не закрывают lattice parity или общий inventory.

## Этап 38: exact canvas frame-rate metadata

Canvas допускает optional `fpsRate` numerator/denominator; positive safe integers должны давать тот же numeric fps с допуском 1e-9. Normalize/JSON сохраняют fraction. `canvasTimeGrid` предпочитает explicit rate, старый fps трактует как authored decimal с denominator 1e6 без подмены 29.97 на 30000/1001. Test проверяет roundtrip explicit NTSC rate, inconsistent/zero denominator refusal и legacy decimal.

Exact metadata пока не передаётся backend wire (`fpsMilli` остаётся прежним), не подключено к canvas controls и ripple. Эти связанные изменения необходимы для полной lattice parity; новый optional field сам по себе её не доказывает.

## Этап 39: exact frame-rate controls

Canvas FPS select предоставляет explicit 24000/1001, 30000/1001, 60000/1001 наряду с прежними authored decimal/integer rates. Изменение numeric fps через state API автоматически очищает fpsRate, если patch не задаёт её явно; width/height/background patches её сохраняют. First-video canvas adoption также очищает старую fraction при смене numeric fps. UI test проверяет exact selection, numeric switch без stale metadata, Undo и восстановленное select value. 21 inspector test и typecheck проходят.

Wire/native compiler пока использует fpsMilli; точное backend lowering, clock consumers и ripple endpoints ещё требуют переноса. UI control не означает export lattice parity.

## Этап 40: native exact frame-rate metadata

Rust CanvasSpec допускает optional fpsRate (`FrameRateRatio` с u32 numerator/denominator), сериализует его только при наличии. Validation запрещает zero numerator/denominator и несовпадение округлённого rational milli с legacy fpsMilli. Старые документы без fraction получают None. Domain regression проверяет exact 30000/1001 roundtrip, conflicting 30 fps refusal, zero denominator и legacy omission.

Пока frontend wire не посылает fraction и native compiler не использует её: exact field admission не закрывает export parity. Точное fps lowering во всех generator/interpolation/stabilization/frame-count consumers, затем реальная квалификация и ripple интеграция остаются необходимыми.

## Этап 41: rational native frame-work accounting

CanvasSpec::frame_rate возвращает explicit fraction либо legacy fpsMilli/1000. Native compiler pixel-frame budgets optical flow/reverse/stabilization теперь рассчитывают ceiling frame counts через rational numerator/denominator в u128 вместо округлённого fpsMilli. 49 compiler unit tests проходят; domain metadata test дополнен проверкой выбранного exact/legacy rate.

FFmpeg filter/generator rate strings ещё используют старое milli; frontend wire также ещё не передаёт fraction. Эта замена accounting не доказывает exact exported fps. Все незакрытые inventory требования сохраняются.

## Этап 42: rational FFmpeg rate lowering

Native compiler передаёт FrameRateRatio в source/playback/frame/interpolation/stabilization helpers. Image inputs, color/checker gaps/backgrounds, text/overlay generators, fps/minterpolate filters и vidstab detect используют numerator/denominator строку. Display legacy rate сохраняет fpsMilli/1000, поэтому 49 compiler tests/golden остались зелёными. Handler vidstab detection и real-test caller обновлены под rational signature.

Frontend wire ещё не передаёт fpsRate, real exact-fps export ещё не квалифицирован, service text frame clocks и ripple ещё требуют продолжения. Green legacy tests подтверждают compatibility, а не полный exact rate end-to-end результат.

## Этап 43: exact fps wire и реальный MP4

Frontend WireCanvas/payload передаёт optional fpsRate без округления; inspector UI test проверяет 30000/1001 в render request. Native integration импортной AV-пары теперь рендерит explicit 30000/1001, и ffprobe подтверждает именно эту r_frame_rate для embedded/paired/double вариантов. Длительность каждого 1.501500 s (frame-quantized output), leading silence/pixels проходят. Тон fixture 40 Hz: RMS 0.06264259 / 0.06595246 / 0.13165162; double-owner control >1.8 сохранился.

При первоначальном 440 Hz fixture double RMS был 0.10143328 вместо близкого удвоения: дробная начальная граница ~1.5 ms даёт фазочувствительный mix. Смена контрольного тона не доказывает sample-exact синхронизацию embedded и paired; этот timing audit остаётся открытым. Service text clocks/admission fpsMilli, ripple и прочие inventory sections также требуют продолжения. 21 inspector test, typecheck и реальный FFmpeg test проходят.

## Этап 44: rational service admission

Shared arithmetic добавляет checked `rational_frame_count_ceil`; legacy frame_count_ceil делегирует с denominator 1000. Service optical/reverse/stabilization pixel-frame admission использует CanvasSpec::frame_rate numerator/denominator, согласуя расчёт с compiler. Regression показывает граничную разницу на 333667 µs: exact 30000/1001 требует ceiling 11 frames, legacy 29970/1000 даёт 10. Дополнены zero duration/denominator, long exact count и overflow cases. 23 service planner tests проходят.

Text frame clocks, ripple endpoint grid integration и sample-exact AV timing остаются открытыми, как и общий inventory. Existing Kani legacy harness сохранён; отдельного formal proof новой rational функции в этом этапе нет.

## Этап 45: frame stepping без накопленного дрейфа

Актуальный audit backend composition service/handler не обнаружил дополнительных text consumers fpsMilli; прежняя запись backlog была слишком общей. Frontend framePrevious/frameNext и nudge используют stepTimeGridTicks по canvasTimeGrid вместо прибавления постоянного rounded frame duration. Границы вычисляются от frame index rational rate; nudge применяет шаг от текущего старта anchor, а не плейхеда. Regression 30000 последовательных шагов 30000/1001 даёт точно 1001000000 ticks, проверены обратный шаг и origin clamp. 10 tests (timeGrid/commandRegistry/App keyboard) проходят.

Ripple duration/source-window reconciliation, иные frame-clock consumers, sample-exact audio/export и прочие inventory sections остаются открытыми. Эти keyboard tests не доказывают полную lattice parity.

## Этап 46: timeline keyboard trim/move grids

Arrow body move и Arrow trim handles используют stepTimeGridTicks по exact canvas grid вместо постоянного rounded frame delta. Каждый trim edge считается от своей текущей границы; source command выполняет обычный trim/reconciliation. UI regression two-step start trim на 30000/1001 подтверждает 33367→66733 ticks и оба Undo. 24 timeline tests проходят.

Shift+Arrow slip пока использует прежний source delta и требует самостоятельной source-grid проверки; pointer snapping, ripple/duration reconciliation и остальные grid consumers остаются открытыми. Полный inventory не завершён.

## Этап 47: command-path fractional clock regression

Command registry regression проходит реальную state/import/selection/history цепочку: exact 30000/1001, десять frameNext дают 333667 ticks без истории, framePrevious даёт 300300. Два nudgeNext linked AV selection дают canonical anchor starts 33367/66733, selection сохраняет двух участников, два Undo восстанавливают размещение и не двигают плейхед. Fixture reset изолирован через resetCompositionForTests и null storage. Оба registry tests проходят.

Это regression текущего общего linked delta. Source требует audio member snap на собственной 48 kHz lattice; тест не доказывает это, own-grid linked placement ещё нужно реализовать, затем расширить expectations. Ripple/grid reconciliation и весь оставшийся inventory не закрыты.

## Этап 48: mixed-member own-grid placement planner

Чистый shiftTimeGridMembers принимает точные subjects/endpoints и общий total delta, floor at origin применяется к earliest member всего набора. Затем start/end каждого участника округляются один раз на его собственной grid (audio 48 kHz, остальные exact canvas rate). Regression mixed 30000/1001 AV даёт video start 33367 и audio start 33375; source phase у origin сохраняется общим floor, duplicate subjects отвергаются. Шесть timeGrid tests проходят.

Planner пока не меняет реальные clips: местная модель вычисляет media duration из source window, тогда как source явно хранит landed endpoints. Подключение требует согласовать source windows/speed ramps/animations/transitions, а не просто заменить start. Own-grid movement/ripple parity остаётся открытой, как и весь inventory.

## Этап 49: own-grid collapsed landing admission

shiftTimeGridSpan отклоняет end<=start после округления, а member planner проверяет допустимый kind. Regression mixed set с valid audio и tiny visual участником подтверждает atomic отказ без частичной map и без изменения input; отдельные проверки sample collapse, unsafe tick addition и unknown kind. Семь timeGrid tests проходят.

Это защита подготовленного planner, не его подключение к editor mutation. Полный перенос own-grid move/ripple всё ещё требует модели landed duration/source windows и transitions reconciliation; общий inventory остаётся открытым.

## Этап 50: реальные static visual move endpoints

moveClip/moveClips для text/image применяют canonical start/end grid через withVisualGridStart; durationTicks становится разницей landed endpoints. Неподвижный participant не переснапивается. Regression one-frame title 30000/1001 после shift имеет start 33367, duration 33366, valid document и неизменный input. Все 534 frontend tests / 94 files, typecheck и changed lint проходят.

Это подключение для static visual kinds. Media kinds ещё сохраняют прежние source-derived durations; animations на изменившемся на tick endpoint требуют дополнительной reconciliation qualification, transitions/ripple/paste и полный inventory остаются открытыми. Не считать этот этап full own-grid placement parity.

## Этап 51: static visual paste и authored ghosts

duplicateClip/pasteClips применяют тот же withVisualGridStart для text/image, что move: обе landed boundaries canonical, durationTicks переизмерена по позиции. Copy planner UI берёт ghosts из результата pure paste и временных copy IDs, а не сохраняет source duration в отдельной арифметике; successful preview совпадает с authored landing. Invalid plan остаётся red fallback outline. Regression static one-frame fractional paste подтверждает duration 33366, исходник неизменный и валидный документ. 43 focused tests (grid/timeline/link state) проходят.

Media own-grid source-window reconciliation, static animation endpoint qualification, ripple и все остальные inventory requirements остаются открытыми.

## Этап 52: static landing animation reconciliation

withVisualGridStart согласует optional visual animation с новой durationTicks через существующий sliceVisualAnimation (start=0). На сокращённом endpoint curve получает sampled boundary key; внутренние key times не сдвигаются, при расширении старый endpoint сохраняет authored behavior. Regression animated one-frame title проверяет move/paste valid documents, final key tick 33366 и sampled linear value 33366/33367, input не мутируется. Девять timeGrid tests проходят.

Применён локальный curve-slice contract; полная source keyframe parity и easing/editor qualification требуют отдельной сверки. Media own-grid placements, ripple/transitions и общий inventory остаются незавершёнными.

## Этап 53: shared canvas patch и MCP operation

Agent batch поддерживает advertised `update_canvas {patch}`. UI updateCompositionCanvas и agent evaluator вызывают одну чистую patchCompositionCanvas, с allowlist canvas fields и полной document validation. Numeric fps patch очищает старую fraction, unrelated fields сохраняют её; failed later operation не меняет input. 23 focused tests (canvas/inspector), typecheck/lint и пять native MCP tests проходят.

Это дополнительное покрытие настройки canvas через агентский путь. Full source settings/history-lock/canvas-lock parity, реальный MCP exact-fps roundtrip, own-grid media/ripple и остальные inventory requirements остаются открытыми.

## Этап 54: canvas agent runtime qualification и source-history boundary

Runtime regression update_canvas проверяет exact fps dry-run без revision/document/history изменений, успешный двухоперационный commit одним Undo, stale revision refusal, malformed second-operation atomic rollback, Undo/redo exact snapshot. Десять agent runtime tests проходят.

Source features.md 55–70 поясняет default-on UI canvas lock через unrecorded composition envelope/history transform-per-snapshot. Локальный canvas пока обычный undoable commit; внедрить только lock checkbox не закрыло бы это требование. Source history transform, envelope geometry re-snap и default lock остаются открытыми; текущий runtime test фиксирует локальный контракт до полного переноса. Остальной inventory не завершён.

## Этап 55: exact-rate wire integer admission

Frontend fpsRate validation ограничивает numerator/denominator positive u32, как native FrameRateRatio. Ранее Number.isSafeInteger допускал rate, который serde не мог прочитать. Regression допускает u32::MAX/u32::MAX (1 fps), отвергает 2^32 fractions в shared patch и agent evaluator, input остаётся неизменным. Двенадцать canvas/grid tests проходят.

Это wire admission исправление, не завершение exact canvas/history/movement parity. Полный inventory остаётся открытым.

## Этап 56: static visual ripple endpoints

Общий ripple closing применяет withVisualGridStart к downstream clips, как move/paste. Text/image start/end canonical и duration/optional animation пересогласуются; нулевой delta оставляет participant неизменным. Fractional regression закрывает title gap 33367–66733, tail lands at33367 with33366 duration; locked неподвижный head unchanged и input immutable. 47 focused tests (ripple/locks/timeline), typecheck/lint проходят.

Media ripple own-grid source duration/window, переходы с дробными borrowed handles и anchored markers/Groups/pinned duration ещё не перенесены. Этап не закрывает полный source ripple contract или inventory.

## Этап 57: static visual packing cursor

Magnet использует тот же static visual grid placement; cursor вычисляется после actual landed end, поэтому следующий title не наследует rounded-duration микропробел. Regression три one-frame title 30000/1001 lands starts0/33367/66733, compositionGaps пуст, повторное packing idempotent, input immutable. Двадцать четыре productivity/locks tests проходят.

Media packing/source-window grid, linked magnet semantics и полный source timeline contract остаются открытыми. Этот этап расширяет static-kind integration и не закрывает inventory.

## Этап 58: whole-target compile и актуальный bundle gate

`cargo test --no-run` успешно собрал все lib/bin/integration test targets после optional CanvasSpec fpsRate; это compile evidence, не запуск всей native test suite. Frontend build, design token checker и global listener lifecycle checker проходят.

Bundle gate FAIL: all JS237.52KiB >211.75, CSS17.02>17.00, total254.54>228.00, workspace async89.05>79.25 gzip. Limits не изменены. Это актуальное превышение после новых features (предыдущий этап11 снимок уже имел FAIL), требуется уменьшение bundle перед квалифицированным завершением. Полный inventory и runtime/export/source behavior требования остаются открытыми.

## Этап 59: измеренное safe build compression

Terser compress passes=3 вместо default single pass, без unsafe transforms/property mangling. На том же текущем source production build успешен; sourceAdapter contract checker сохраняет8 references. All JS gzip уменьшен237.52→236.26KiB, workspace89.05→88.61, total254.54→253.29. CSS17.02 не изменился. Build time3.22→4.21s в этих двух локальных запусках (не стабильный benchmark).

Bundle gate всё ещё FAIL (JS211.75,total228,workspace79.25,CSS17.00 limits сохранены). Это ограниченное сокращение bytes, не восстановление gate; production behavioral browser qualification и остальные parity requirements остаются открытыми.

## Этап 60: exact-set raise на новую типизированную дорожку

moveClipsToNewTrack переносит точный набор одного kind на одну новую верхнюю дорожку без округления времени и без расширения links. Subjects сортируются; только опустевшие исходные дорожки удаляются, unrelated empty tracks сохраняются. Locks, duplicate ids/subjects и video transition endpoints проверяются до результата. Новая дорожка наследует flags первой исходной дорожки — это локальный контракт, не подтверждённая source semantics для разных flags.

Контекстное меню «На новую дорожку» использует pure preflight и commit одним Undo; агентская операция move_layers_to_new_track {clipIds,trackId} использует тот же evaluator и рекламируется native MCP schema. 28 focused tests проходят, включая точные non-grid timestamps, multi-source lane cleanup, отсутствие link fanout, mixed-kind refusal, agent batch rollback и menu Undo восстановления удалённой дорожки. Typecheck и focused lint проходят.

Source actor.ts 953–984 и actor.test.ts 2120+ требуют одну универсальную lane для смешанных video/audio/image/text, сохранение reserved lanes, а drop strip — anchor/time landing в той же операции. Локальная модель и native wire имеют только typed tracks; mixed subjects сейчас явно отклоняются, drop landing не реализован. Это частичная реализация raise, не завершение source move_layers_to_new_track или общего inventory. Для полного переноса требуется общая lane model с сохранением визуального порядка, audio routing и render admission.

## Этап 61: anchor landing в shared raise command

moveClipsToNewTrack и агентская операция принимают optional anchor {clipId,startTicks} одним объектом. Anchor обязан входить в exact subjects и иметь safe nonnegative start. Requested head сначала snaps на own visual/sample grid, затем общий delta floors по earliest subject, и shared shiftTimeGridMembers независимо округляет оба endpoint каждого участника. Text/image duration и visual animation согласуются с landed span; без anchor по-прежнему verbatim raise без resnap. Audio/video landing принимается только если rounded endpoint duration совпадает с source-derived duration; иначе атомарный отказ с source-window reconciliation причиной. Source media-window reconciliation ещё требуется, отказ не считается её переносом.

29 focused raise/timeline tests прошли, typecheck/lint прошли; дополнительный pure regression подтверждает audio start33375 при запросе33367, отказ off-grid source span и landing через agent evaluator. Tests покрывают anchor-first snapping, set-wide floor, invalid/nonmember anchors, no mutation. Это вычислительная часть drop landing. UI drop strip, универсальные mixed lanes, reserved transient semantics и весь остальной inventory остаются открытыми.

## Этап 62: native authored media extent

Сверка source move.ts187–267 показывает, что landing изменяет t_start_us/t_end_us, сохраняя params/source clock. Следовательно, изменение source window для подгонки duration было бы неправильным переносом. Native ClipPlacement теперь принимает optional timelineDurationTick, независимо от sourceInTick/sourceOutTick/speed; при отсутствии поля сохраняет legacy derived duration. Сначала проверяются placement и speed ramp, затем authored extent; нулевая duration и overflow endpoint отклоняются. Serde legacy omission и authored roundtrip покрыты regression, исходный source window unchanged. Native fixtures сохраняют None и прежнее поведение.

19 native composition domain tests проходят. Эта возможность пока не подключена к frontend authoring/wire payload, поэтому media landing refusal этапа61 остаётся. Реальный render endpoint/FFmpeg pacing, EOF padding, speed-ramp mapping при независимом endpoint и reverse/freeze qualification ещё не подтверждены. Domain acceptance само по себе не доказывает source preview/export parity или завершение inventory.

## Этап 63: authored extent в реальном native export

Для authored timelineDurationTick обычный Duplicate playback теперь явно pads последним кадром, приводит fps, trims requested duration и сбрасывает PTS. Legacy absent field сохраняет прежнюю filter graph. Constant-speed audio и final speed-ramp audio pad silence перед final trim, поэтому исходная короткая дорожка не сокращает authored extent. Source window не меняется.

Все49 compiler tests прошли. Реальный FFmpeg integration real_imported_audio_pair_has_one_audio_owner_and_preserves_offset расширен short/long authored cases: исходный диапазон500000–1500000 сохранён; при extent500000 duration файла1.001s, при extent1500000 —2.002s (offset0.5s, exact30000/1001 cadence). ffprobe сохраняет exact fps; RGB1.8s у long остаётся red, RMS1.7–1.9s <0.001 (silence padding). Embedded/paired/double controls сохранили прежние RMS0.0626426/0.0659525/0.131652. Test выполнен, не skipped.

Это квалификация forward constant-speed primary video + paired audio. Она не доказывает arbitrary ramp/reverse/freeze/transition/overlay conformance, browser preview совпадение или source EOF policy. Frontend authored extent, trim/split/retime reconciliation, media raise integration, mixed lanes/drop strip и весь inventory ещё открыты.

## Этап 64: frontend authored extent и media raise

SourceClipTiming optional timelineDurationTicks и WireClipPlacement timelineDurationTick подключены к shared duration/end evaluator, runtime validation, project normalization и строгому payload copier. Positive safe admission, 24h envelope и legacy omission сохранены. Raise landing теперь записывает округлённую duration, сохраняя sourceIn/sourceOut/speed/ramp; прежний temporary source-window refusal удалён. Visual/audio animations и fades согласуются существующим slice helper. Imported AV pairing и detach audio сохраняют extent.

Trim/split обновляют explicit extents вместо копирования старой полной duration в оба результата. UI speed update масштабирует constant-speed extent oldSpeed/newSpeed; для ramp — отношением новых/старых derived durations. Regression: fractional video source span33367, landing start33367/end66733, authored duration33366, source windowunchanged; normalization+render wire roundtrip; split16633/16733 и end66733; trim20000; speed2x+Undo; invalid duration admission. Focused authored/raise/importPair tests, typecheck и changed-file lint проходят.

Это integration в редактор, не полная qualification независимого endpoint. Split/trim внутри padded source tail, arbitrary ramps, reverse/freeze, transitions, browser audio EOF и linked per-grid movement требуют дальнейшей сверки. Mixed lanes/drop strip и общий inventory всё ещё открыты.

После финальных изменений полный frontend suite: 550 tests /97 files PASS. Это regression evidence текущего тестового покрытия; оно не закрывает перечисленные unqualified paths или source inventory.

## Этап 65: own-grid media move/paste/ripple/packing

withVisualGridStart заменён withClipGridStart: media, как static kinds, получают независимо rounded start/end; audio использует48000/1, video canvas rational fps. При изменении duration записывается authored extent и согласуются optional curves/fades, source window/speed/ramp не меняются. Move set, linked duplicate и paste сначала snap requested anchor head на его own grid, затем floors общий delta и rounds участников; это устраняет ошибку raw visual delta, переносившую audio на целые миллисекунды при near-frame drop. Ripple closing и magnet packing используют тот же helper и actual landed cursor.

Regression fractional linked AV получает starts33367/33375, ends66733/66729, source span33367 unchanged; paste requested1016000 snaps visual frame30=1001000 и audio follows1001000, независимая copied link сохранена. Повторные nudges own-grid ожидают starts66733/66750, Undo33367/33375 и затем0/0. Старый test ожидал обе дорожки на visual grid; заменён explicit sample-boundary expectation. Полный frontend suite551 tests/97 files PASS, changed-file lint PASS.

Неизменный start helper пока оставляет verbatim snapshot (для неподвижных ripple participants); source resnap при explicit zero-delta move требует отдельного сопоставления. Existing transition handles/curve semantics, padded-tail trimming, arbitrary source timing, real browser preview и весь source inventory не считаются завершёнными этим suite.

## Этап 66: drop strip и explicit mover resnap

При обычном body drag появляется верхняя «Новая дорожка» область. Hit-test pointermove выбирает её, shared moveClipsToNewTrack preflight строит exact landed ghosts и refusal; pointerup повторно hit-tests и рассчитывает requested head из актуальной release координаты. Commit через raiseSelectedCompositionClips(anchor) — одна history entry, cleanup+Undo исходной дорожки покрыты component regression. Pointer cancel использует существующее gesture teardown без commit. Alt copy по-прежнему отдельный gesture, strip для него не показывается.

В source applyMoveLayer target resnaps даже при delta0, а link siblings только при nonzero delta. Shared helper получил explicit resnap flag для mover/copy, no-op guard сравнивает actual result. Regression source span33366 при start0 gets33367 endpoint; unshifted audio partner unchanged, повтор canonical move idempotent. Generic moveClips anchor также uses resnap; точный zero-delta set applyMoveLayers/applyShiftLayers contract требует дальнейшего разграничения от singular link fanout.

Добавлены semantic CSS для drop strip и shared copy/raise ghosts (absolute placement, extent, dashed border, invalid state, pointer-events none); ранее copy ghost class не имел CSS rule в текущем source. Design token check PASS, changed-file lint PASS. Full frontend suite553 tests/97 files PASS; final pointerup calculation повторно проверен focused32 tests. Физический браузер/pointer capture/autoscroll и actual screenshots ещё не квалифицированы. Mixed kind strip preflight отклоняется до commit, универсальные lanes остаются обязательной незавершённой частью общего inventory.

## Этап 67: exact placements и singular linked zero-delta distinction

Source move.ts16–42 applyMoveLayers вызывает applyShiftLayers, который rounds обоих endpoint всех exact subjects даже при delta0. Source applyMoveLayer resnaps только target при zero delta и оставляет linked siblings unchanged. Локальный private moveClipSet теперь разделяет эти пути: exported moveClips resnaps весь exact set, singular linked fanout сохраняет stationary partners. Regression показывает audio extent33366→33375 для exact zero move, а singular partner остаётся33366.

moveClipPlacements принимает per-subject destinations и named anchor независимо от порядка placements. Shared kind/lock/transition/range/overlap guards выполняются для каждого destination, duplicates/nonmember anchors/missing targets отклоняются атомарно; source links не расширяются. Agent operation move_layer_placements {placements,anchorId,startTicks} рекламируется native MCP schema и вызывает тот же evaluator. Mixed video/audio subjects можно переместить каждый на свою typed lane одной операцией. Это ещё не универсальная mixed lane из source.

Full frontend554 tests/97 files PASS, typecheck/lint PASS; native MCP5 tests PASS. Дополнительный runtime suite11 tests PASS: two destination apply — one Undo, dry-run doc/history/revision unchanged, stale revision refusal, invalid second destination rollback, Undo/redo snapshots. Source transient/reserved lane pruning, full source move wire naming/schema, unified mixed lanes и browser qualification остаются открытыми. Весь inventory не завершён.

## Этап 68: multi-subject move ghosts и physical drag qualification

planCompositionClipMove — read-only rehearsal того же editor move, включая UI selection/link override, snapping и command guards. Commit вызывает этот planner; timeline обычного body drag теперь показывает всех exact subjects по actual planned tracks/start/end. Originals остаются на authored местах до release; invalid set получает red ghosts, cancel не меняет project/history. New target hit-test обновляется для ordinary move, как для duplicate. Regression linked/unlinked AV, fractional own grids, protected sibling refusal и cancel покрывает projection=commit, original position и один Undo.

Chromium mouse-down/move/up на реальном UI выявил две ошибки, пропущенные mocked hit-test тестами: pointerdown схлопывал preselected unlinked set, а появление drop strip сдвигало строки под курсором. Begin gesture сохраняет preselected exact subjects (Shift selection остаётся отдельным путём), primary становится clicked anchor. Drop strip теперь постоянно занимает компактные28px, подсвечиваясь during move; отдельный new-track-lane class не смешивает drop target с authored track lane. Поэтому gesture не изменяет layout перед hit-test.

Расширенный real MCP/browser harness после исправлений PASS: физический drag двух unlinked text subjects, two non-invalid ghosts, unchanged live document/history before release, ghost pixel positions=landed ticks, one Undo restores both; остальной existing MCP/search/analysis/link suite также прошёл. Linked text fixture явно author fps30 перед созданием half-second phases (scene analysis source остаётся25fps); fractional/sample placement отдельно проверяется unit tests. Backend binary rebuilt. Full frontend559 tests/97 files PASS, typecheck/lint/design token check и diff check PASS.

Это конкретная physical mouse qualification обычного same-lane move. Physical new-lane drop, browser linked audio sample clock/EOF, автопрокрутка/scroll anchoring, full marquee, universal mixed lanes и остальные пункты source inventory ещё открыты. Dev browser harness не является production minified bundle qualification; bundle gate остаётся ранее зафиксированным FAIL.

## Этап 69: clip marquee selection

Перенесено выделение клипов рамкой из пустой области timeline: live пересчёт пересечений с несколькими displayed lanes, уменьшение рамки снимает клипы, release сохраняет выбор, Escape/pointercancel возвращает selection/primary/track/gap на момент press. Surviving primary сохраняется. Locked клипы/дорожки исключены из прямого hit; доступный linked member расширяет выбор на весь link, включая locked partners; session link override отключает expansion. Проверяются все клипы отображаемой дорожки, включая неотрисованные из-за virtualization. Рамка меняет только UI selection, без project/history изменений.

Horizontal edge autoscroll сохраняет начало рамки в content coordinates, пересчитывает hits при scroll, не прокручивает вертикально. RAF прекращается после release/cancel/unmount. Pointer capture начинается после threshold движения, чтобы обычный blank-lane click продолжал seek/select gap. Browser regression проверяет leading bounded gap; unbounded tail по существующему compositionGapAt не является выбираемым gap.

Chromium physical mouse suite PASS: two-lane selection, shrinking, Escape restoration, release persistence, bounded-gap click, project/history unchanged; дальнейший multi-subject drag/Undo и existing MCP/analysis suite также PASS. Focused timeline34 tests включают mocked edge-scroll/cancel lifecycle; pure helper3 tests покрывают intersection/locks/links/primary/reverse sweep. Full frontend566 tests/98 files PASS; typecheck0 errors/warnings, lint, semantic design token check, diff check PASS.

Дополнительная Chromium physical edge-autoscroll qualification PASS: scrollWidth > clientWidth, реальное удержание мыши у правого края сдвигает scrollLeft более чем на40px, scrollTop неизменен; Escape/mouseup удаляет рамку, два следующих RAF не изменяют scrollLeft, selection/document/history восстановлены. Keyframe sublane marquee, combined AV slice-aware rows, A/B roll filters, Shift-union semantics и остальные source inventory пункты не завершены. Этот этап реализует clip marquee в текущих typed lanes; полного переноса всех функций нет. Production bundle gate остаётся ранее зафиксированным FAIL, minified runtime отдельно не проверен.

## Этап 70: относительный сдвиг выбранного блока по displayed lanes

По source features.md Move вертикальный drag сдвигает всех видимых участников на одинаковое число отображаемых дорожек. moveClipsByTrackOffset вычисляет offset anchor→destination и explicit placements для каждого exact subject; участники на отсутствующих в displayed list дорожках сохраняют свои lanes. Общий moveClipPlacements проверяет locks, compatibility, overlap/range до commit; выход любого участника за пределы списка отклоняет весь блок. Duplicated/unknown displayed ids и невходящий в selection/неотображаемый anchor отклоняются.

Editor read-only planner использует этот путь при изменении дорожки: preview ghosts и release commit получают одни destinations; same-lane путь сохраняет ранее квалифицированные semantics. Все текущие timeline lanes отображаются в document order, поэтому editor передаёт их полный список. Hidden render flag не равен скрытию строки timeline; будущий A/B roll filter должен передавать свой displayed list.

Pure tests2 PASS: visible two-member block shifts one row, filtered-out partner retains lane/timing delta, out-of-bounds/locked/occupied target refusal and immutable input. Component regression PASS: ghosts на обеих новых дорожках, unchanged document during drag, commit переносит обеих участников, одна history entry, Undo восстанавливает snapshot. Full frontend568 tests/99 files PASS перед добавлением этой component regression; дополнительный focused37 tests PASS. Chromium physical cross-lane qualification PASS: mouse drag A→B строит ghosts на B/C без invalid state, document unchanged до release; landed A→B и B→C, одна history entry, Undo восстанавливает полный snapshot. Existing MCP/analysis/marquee/edge-autoscroll/same-lane drag suite также PASS. Final typecheck0 errors/warnings, lint и diff check PASS.

Typed lane compatibility остаётся ограничением текущей модели; source unified mixed lanes, перенос authorized transition pairs между дорожками и source reserved/transient pruning пока не перенесены. Весь inventory не завершён.

## Этап 71: перенос transition pair на другую videolane

Exact multi-subject move больше не отклоняет любой endpoint transition при смене lane. Все transition endpoints разрешаются в итоговые destinations: пара должна оставаться на одной дорожке, иначе атомарный transition-conflict. Transition сохраняет id/kind/duration/endpoints и переезжает на общую destination lane; source lane очищает reference. Untouched transitions остаются на месте, отсутствующее optional transitions поле не создаётся без необходимости. Все ранее действующие lock/kind/overlap/range guards сохраняются.

После relocation применяется compositionTransitionUnavailableReason к итоговой паре: соседство, точная граница, forward playback и source handles обязаны остаться допустимыми. Pure regression проверяет перенос whole pair, starts2s/6s, transition availability, refusal одного endpoint и occupied destination, immutable input. Full frontend570 tests/99 files PASS; итоговый focused42 tests PASS с semantic revalidation. Final typecheck0 errors/warnings, lint и diff check PASS.

Это перенос существующей локальной adjacent/handle transition модели. Source overlapping transition geometry, UI moving transition chip preview, raising whole pair на новую lane и physical browser transition drag ещё не квалифицированы/не перенесены. Весь source inventory остаётся незавершённым.

## Этап 72: raise transition pair и moving transition preview

moveClipsToNewTrack переносит transitions, если оба endpoint входят в exact subject set. Одиночный endpoint отклоняется атомарно. Source lanes удаляют только carried references; untouched transition pairs сохраняются. Новая видеодорожка получает исходные transition ids/kinds/durations/endpoints, а итоговая transition availability проверяет adjacency, source handles и playback. Empty touched source lane pruning остаётся прежним; all-pair raise не оставляет потерянные transition references.

Timeline строит transitionPreview из landed clip preview: чип на общей destination lane получает координату incoming start minus half transition duration, а исходный чип временно скрывается. Для new-track drop чип строится из raisePreview. Invalid move оставляет original transition без ложного valid preview. Ghost pointer-events:none, existing transition z-index сохраняет положение над clip/link chrome. Release/cancel убирает projection без отдельного изменения history.

Full frontend574 tests/99 files PASS; final typecheck0 errors/warnings, lint, semantic design token check и diff check PASS. Focused43 tests PASS: raise complete pair и untouched pair, one-endpoint refusal, all-pairs source cleanup, editor one Undo snapshot; component move/raise/cancel — transition ghost следует landed position, document unchanged до release, source chip скрыт только during valid gesture, commit сохраняет transition, Undo restores snapshot. Реальное physical browser transition drag/drop отдельно не квалифицировано. Source overlapping transition geometry, universal mixed lanes и остальной inventory не завершены.

## Этап 73: background deselection и locked gap selection

Source features.md Background clicks требует снятия clip selection также на scroll body/band below last track. Pointerup на clip surface вне lanes/ruler/headers/controls снимает clips/primary/track/gap, не seek и не history edit. Активная marquee release не очищает полученный выбор; lane click остаётся отдельным seek/gap путём. Регрессия проверяет scroll-body release с unchanged playhead/history.

Gap на locked lane не выбирается; repairGapSelection снимает существующий highlight при блокировке дорожки, даже если span boundaries не изменились. Lane keyboard gap test обновлён под этот source contract: stale gap сразу dropped, Delete/close не изменяет history. Focused timeline40 tests PASS, включая marquee/move/transition projection регрессии. Keyframe per-kind background selection и keyframe marquee остаются незавершёнными; полный inventory не закрыт.

## Этап 74: selected-key translation foundation и групповые операции в Attribute editor

moveSelectedKeyframeTracks принимает несколько named animation tracks, selected key ticks и duration каждого layer. Один signed delta snap на composition frame grid применяется ко всему набору, затем зажимается по tightest [0,duration] wall. Stationary neighbours не служат walls; collisions заменяются moved keys, остальные не меняются. Stale/duplicate subjects и collapsed moved keys отклоняются. Inputs immutable, selected ticks возвращаются вместе с результатом для сохранения выбора.

Attribute keyframe editor получил checkbox selection и backward/forward frame nudge/delete выбранных ключей текущего параметра. State API выполняет одну запись animation value через существующий locked-safe resolver, поэтому gesture — одна history entry. Delete preflights весь tick set до изменения. Переключение clip/property/mask очищает локальный выбор. Pure/state regression проверяет общую дельту нескольких tracks, tightest wall, collision replacement, stale refusal, один Undo для move/delete с восстановлением удалённого stationary neighbour. Full frontend579 tests/100 files PASS перед дополнительной UI regression; focused24 tests (keyframe foundation + Inspector) PASS; существующий state39 tests PASS. Дополнительный Inspector22 tests PASS: выбор двух checkboxes, nudge сохраняет выбор, delete — ещё одна history entry, два Undo восстанавливают moved и original snapshots. Проверка выявила и исправила сброс selection при обновлении animation: context reset теперь зависит от стабильного clip/property/mask key, а не изменяемого clip object. Stale selected ticks удаляются после смены track/Undo.

Этот этап — основа multi-property/multi-layer selection, а не полный source Keyframe selection. UI selection пока local одному параметру; timeline sublanes/diamonds, marquee per-kind, global Delete precedence, right-click interpolation per-key, cross-layer atomic gestures, retime drag, Alt scale и clipboard keyframe operations ещё обязательны. Local model interpolation по-прежнему track-level; source per-key easing требует отдельного model/native изменения. Полный inventory не завершён.

## Этап 75: атомарные операции keyframe selection между clips/properties

CompositionKeyframeSubject именует target visual/audio/mask, clip/mask id, property и exact ticks. moveCompositionKeyframeSelection собирает все resolved tracks, применяет один shared translation с tightest wall через moveSelectedKeyframeTracks, затем записывает все animation values одним commitAutomationValues. Возвращает landed ticks по subjects. Duplicate property subjects/stale ticks отказ до commit; locked participant refusal через общий content admission без изменения document/history. Existing single-property nudge использует тот же batch path.

deleteCompositionKeyframeSelection preflights весь набор, рассчитывает patches без изменения live project, затем один commit. Existing single-property selected delete delegates ему. Mask после удаления последнего ключа сохраняет sampled constant fallback; visual/audio опциональный параметр снимается. Batch writer updates visual animation/audio animation/masks в одном immutable document pass и существующей history/lock admission.

Full frontend581 tests/100 files PASS; typecheck0 errors/warnings, lint и diff check PASS. Focused26 tests PASS: cross-clip X/Y selection shared tightest wall, одна history entry, Undo whole document; mixed selected delete — одна entry+Undo, stale second subject rollback, locked second participant refusal для move/delete, duplicate subject refusal; existing Inspector selection/nudge/delete regression также проходит. Global UI selection, timeline diamond sublanes/marquee, Delete precedence и source per-key easing всё ещё не реализованы: этот этап даёт общий transactional API, local Inspector UI остаётся выбором одного параметра. Полный inventory не завершён.

## Этап 76: keyframe time-scale calculation и editor actions

Сверены source renderer/keyframe/useKeyframeDrag.ts83–124 и batchRetime.ts170–215: Alt на end key фиксирует противоположный конец; между ends остаётся translate. Scale factor ограничивается frame/minimum distinct selected composition-time gap и layer walls; source no-merge floor выигрывает при конфликте с wall cap. Scale выполняется в composition coordinates с учётом каждого layer start, затем каждый результат rounds на frame grid, затем возвращается в authored track timeBase. Collision со stationary key replaces его, selected key merge отклоняется.

scaleSelectedKeyframeTracks implements этот batch calculation; scaleCompositionKeyframeSelection writes все subjects одной commitAutomationValues и возвращает landed ticks, factor1 не создаёт commit. Attribute editor предлагает сжать ×0.5/растянуть ×2 относительно earliest выбранного ключа. Locked admission/Undo используют общий state path. Pure tests покрывают cross-group composition offset, floor при negative factor, whole-set cap, right-anchor scale, single-time no-op и immutable source. State test scale collision+one Undo restores stationary key. Full frontend582 tests/100 files PASS; дополнительный Inspector22 tests PASS проверяет actual stretch button, сохранение checkbox selection, one Undo и последующий nudge/delete. Typecheck0 errors/warnings, lint и diff check PASS.

Сам Alt-drag gesture, endpoint picking/preview/cancel, normalized per-key tangents и global selection UI ещё не перенесены. Source floor-wins-over-wall extreme и дробные authored timeBase/negative resulting keys отдельно не квалифицированы в native export. UI actions используют current parameter selection, полный source inventory остаётся открытым.

## Этап 77: global keyframe selection state и command priority

UI selectedKeyframes хранит subjects target/property/ticks в общем composition state. selectCompositionKeyframe выполняет replace/toggle exact key, excludes locked/missing keys, сохраняет clip primary и playhead. Inspector checkboxes читают общий набор: изменение property больше не уничтожает предыдущий выбор, nudge/delete/scale работают над всем набором. Scale anchor берётся из earliest composition time across subjects через compositionKeyframeSelectionExtent, а не текущего editor property. Selection repair после document edits/Undo removes stale/locked/missing key ticks; project activation clears selection.

Обычный clip selection clears keyframes. Clip marquee clears их на live update; Escape/pointercancel restores snapshot keyframe selection вместе с clips/gap/primary. Delete и nudge command availability/execution дают keyframe selection приоритет над clip selection; ключи разных properties перемещаются одним frame delta, clip heads остаются неизменны. Delete keeps authored clips and performs one selected-key transaction. Local table UI still displays current property's keys but shared action count includes весь набор.

Full frontend583 tests/100 files PASS до дополнительной command-nudge regression; дополнительный keyframe/registry7 tests PASS. Inspector regression selects Gain then switches to Pan and selects another key, both properties move once with retained selection and one Undo. State command regression proves Delete removes keys rather than clips, nudge keeps clip positions, both one Undo. Timeline marquee regressions prove keys cleared during sweep and restored on Escape/cancel. Lint/diff check PASS; Final typecheck0 errors/warnings after command-nudge addition.

Global selection теперь доступна через API и Inspector properties; cross-clip user picking по diamond timeline ещё отсутствует. Timeline sublanes/curves, keyframe marquee, retime drag/Alt scale gesture, context easing/per-key model, clipboard и remaining source inventory не завершены.

## Этап 78: timeline keyframe sublane и physical multi-layer drag

keyframeChannels собирает visual/audio/mask keyed parameters всех clips одной track, сохраняя authored timeBase/start и lock state. TimelineKeyframes показывает compact24px sublane с diamonds; развёрнутый режим разделяет channels по строкам. Клики/Shift toggles используют общий selectedKeyframes, keyboard Delete/nudge dispatches shared registry. Blank sublane clears только keys, оставляя inspected clip/playhead/history. Locked diamonds disabled. Sublane исключён из clip marquee/background hit; keyframe marquee здесь ещё отсутствует.

Pointer gesture snapshots subjects/groups/document. Live projection переносит весь набор с одним frame-snapped delta; Alt на earliest/latest composition-time key масштабирует вокруг opposite end, interior key остаётся translate. keyframePreviews в UI shared между sublane components, отображённые diamonds получают predicted tracks/ticks без изменения project/history. Release пересчитывает последнюю координату и commits один shared transaction; Escape/pointercancel/unmount clear previews без commit, stale document refuses release. Capture принадлежит стабильной lane, а не diamond, который пересоздаётся при смене tick.

Full frontend587 tests/101 files PASS перед дополнительной blank-sublane test; focused sublane5 tests PASS: multi-layer move/scale/cancel, projection=commit, one Undo, cross-lane Shift selection, Delete сохраняет clips, background preserves inspected clip. Existing timeline40 tests PASS. Final accessible listbox/options semantics typecheck0 errors/warnings, lint/diff check PASS; focused sublane5 tests repeated PASS after semantic markup change. Chromium real mouse suite PASS: picks keyA then Shift keyB, live two-subject previews with unchanged document/history, move and Alt scale landed pixel positions=preview, one Undo full snapshot, Escape cancellation. Each test iteration explicitly resets UI key selection: clicking an already-selected key intentionally preserves the set, so another Shift click would toggle its partner off. Existing MCP/search/analysis/clip marquee/edge scroll/physical clip drag suite also PASS.

Expanded channels пока horizontal rows без drawn value curves/normalized value axis; source expanded/collapsed keyframe marquee, curve graph/tangents, right-click per-key easing, clipboard operations и preview picture integration with ephemeral animation tracks ещё не перенесены. Full inventory остаётся незавершённым. Dev Chromium qualification не является production minified bundle proof; bundle gate ранее FAIL.

## Этап 79: keyframe marquee и фиксированный selection kind

Marquee kind определяется на pointerdown и сохраняется: keyframe-lane→keys, clip lane/drop strip/scroll body→clips. Keyframe row больше не исключается из marquee begin; buttons/headers/ruler остаются отдельными gestures. Key box собирает drawn diamond centres отображённых sublanes, excludes disabled/locked keys, пересчитывает exact grouped target/property/ticks с нуля. Collapsed rows test x within swept lane; expanded rows test both centre coordinates. Shrinking releases keys. Clip selection/primary/playhead/document/history сохраняются при key sweep. Clip sweep по-прежнему clears key selection.

Escape/pointercancel restore press snapshot, including key selection. Blank sublane release очищается timeline parent с учётом suppressMarqueeClick, чтобы mouseup после Escape не удалял restored keys. Key drag capture release обрабатывается owner lane до background path; global keyframePointerReleasePending подавляет stray background mouseup после Escape/unmount. Keyframe sublane Escape без local drag больше не останавливает event propagation: parent marquee owner должен получить Escape даже при focus на diamond/plot. Horizontal edge RAF используется тем же content-anchored marquee path, вертикальная прокрутка отсутствует.

Full frontend592 tests/101 files PASS; typecheck0 errors/warnings. Pure marquee4 tests проверяют collapsed x-only/expanded centre-y, row intersection, locked exclusion и shrinking. Sublane8 tests проверяют multi-lane marquee, primary preservation, shrink, cancellation, keyboard-focus Escape+subsequent mouseup retaining press selection, no document/history edit. Physical Chromium suite PASS с two-key box/shrink/keep и nonempty Escape restoration: identity assertion подтверждает возврат исходного keyA (вместо оставшегося после shrink keyB), рамка исчезает сразу после Escape, subsequent mouseup сохраняет keyA. Existing physical key move/Alt-scale/Escape и остальные MCP/analysis regressions также PASS. Lint/diff check PASS before final documentation.

Expanded UI пока разделён на property rows, а не source value-curve graph; drawn-centre key marquee реализован для текущей геометрии. Keyframe-specific physical edge scroll, source value axis/tangents, key context easing/clipboard, ephemeral picture preview и остальной source inventory не завершены. Полного переноса всех функций нет.

## Этап 80: expanded value curves и value-axis geometry

Сверены source KeyframeCurveGraph/curveGraph computeValueRange/valueToY: каждая curve имеет свой value range по rendered key/segment samples,10% pad, all-equal nominal band ±max(1,abs(value)*.1). keyframeValueRange и keyframeCurvePoints используют тот же local sampler, что editor preview, с authored timeBase и composition start conversion. Linear/ease sampling32 segments, Hold содержит exact duplicate-x corner (нет ложной diagonal ramp). Source/local текущая default extrapolation Hold показана dashed tails до clip boundaries.

Timeline expanded mode заменяет property-index rows общим120px plot. SVG curves overlay; diamonds располагаются по actual key value на своей padded axis, keyframe marquee автоматически считывает новую centre geometry. Head legend показывает property/min/max и clip id в tooltip. Свернутый24px режим сохраняется. Shared keyframePreviews projected curves и diamonds читают единый animation value. Diamonds выше curve strokes и доступны как listbox options.

Full frontend596 tests/102 files PASS; typecheck0 errors/warnings, lint, design-token check, diff check PASS. Pure curve3 tests проверяют authored timeBase/start, padded linear axis/midpoint, cubic ease sample, Hold exact corners, constant large-value band и immutable inputs. Focused curve+sublane12 tests PASS: expanded curve/diamond positions, key marquee выбирает only upper-value diamond по обеим осям, без project/history edits. Chromium physical expanded-curve qualification PASS: два opacity keys с values0.8/0.2 имеют top10/110px, SVG curve присутствует, real box выбирает только верхний key по value axis, document/history unchanged. Screenshot /tmp/video-kadr-keyframe-curve-qa.png inspected: curve, selected upper diamond, lower diamond, dashed Hold tails и readable range legend отображаются корректно. Existing MCP/analysis/physical key+clip gestures also PASS.

Это отрисовка существующей local track-level interpolation модели. Source per-key spline/procedural Elastic/Bounce, tangent handles/continuity, non-Hold extrapolation, channel colours/visibility, curve context menu и native per-key model ещё не перенесены. Full source inventory не завершён.

## Этап 81: outgoing interpolation отдельных ключей

Frontend и native Rust keyframe model принимают optional interpolation каждого ключа; отсутствие сохраняет track default и legacy serialization. Sampler, SVG curve и FFmpeg expression выбирают interpolation левого ключа сегмента. Upsert/retick/retained slice keys сохраняют metadata. Render payload теперь передаёт override и нормализует legacy alias вместо потери поля.

Контекстное меню diamond применяет Hold/Linear/EaseIn/EaseOut/EaseInOut к глобальному набору выбранных ключей одной atomic transaction; right-click выбранного ключа сохраняет набор. Locked participants отклоняются без history edit. Общий параметр interpolation остаётся fallback, explicit overrides сохраняются.

Frontend598 tests/103 files PASS, включая2 per-key tests и9 существующих timeline tests; native5 keyframe tests PASS, включая serde roundtrip и сравнение native sampler с actual FFmpeg gray pixels на четырёх timestamps. Native49 composition compiler tests PASS; all-targets test compilation PASS. Typecheck0 errors/warnings и lint PASS; diff check PASS. 

Контекстное меню ещё не проверено physical browser gesture. Новый backend executable для browser/export suite ещё не пересобран; real FFmpeg квалификация относится к domain adapter test. Synthetic slice boundaries не сохраняют фазу cubic segment. Source spline/tangents/Elastic/Bounce, clipboard, ephemeral picture preview и полный inventory ещё не завершены. Полного переноса всех функций нет.

## Этап 82: regression context easing и atomic admission

Component tests подтверждают right-click выбранного diamond сохраняет cross-lane selection/playhead; multi-key menu не отмечает случайный общий preset. Выбор EaseIn меняет keys обеих properties при неизменном track default, закрывает меню и даёт ровно один Undo, который полностью восстанавливает проект. Right-click unselected key заменяет key selection и отображает fallback Linear; Escape dismisses без project/history mutation. Shared setter теперь отклоняет duplicate target/property subjects до transaction; regression подтверждает отсутствие изменений.

Focused13 tests PASS; typecheck0 errors/warnings. Physical browser context-menu qualification и прочие открытые пункты этапа81 остаются незавершёнными.

## Этап 83: invalid keyframe gesture release

Timeline gesture теперь хранит validity последнего pointer projection. Ошибка planner сбрасывает projected tracks и запрещает commit последнего успешного delta/factor; следующий допустимый move/release восстанавливает validity. Previously invalid release мог commit stale projection. Component regression создаёт valid preview, затем invalid time conversion и проверяет отказ без project/history mutation; recovery scenario проверяет final200ms delta на обеих properties, one Undo и полное восстановление.

Focused sublane13 tests PASS; typecheck0 errors/warnings, lint PASS. Backend executable rebuild PASS; новый executable включает per-key interpolation для последующей real browser/export qualification. Full feature parity остаётся незавершённым.

## Этап 84: physical context easing qualification

Chromium real MCP/browser suite PASS с freshly built backend. Physical right-click выбранного expanded opacity diamond показывает Linear fallback; click EaseIn закрывает menu, project wire содержит override только first key, stationary second key metadata absent. History увеличивается ровно на1, playhead сохраняется. Repeat physical right-click показывает EaseIn checked, Escape closes и MCP Undo полностью восстанавливает исходный project. Existing physical key marquee/drag/scale/curve и real FFmpeg analysis regressions также PASS.

Source KeyframeCurveGraph.tsx уточняет: right-click unselected key также вызывает focus seek; локальный openMenu пока лишь заменяет selection. Это оставшийся behavioral gap, как и menu hover preview, curve-context entry, continuity/tangent presets и весь остальной открытый inventory. Проверка per-key final composition export пока ограничена domain FFmpeg test этапа81, browser suite проверяет editor/project wire и legacy analysis, не новый final composition render.

## Этап 85: unselected-key context focus seek

Source KeyframeCurveGraph right-click path выбирает и focus-seeks только previously unselected key. Local openMenu теперь повторяет это правило: selected diamond сохраняет selection/playhead; unselected заменяет key selection и вызывает transport setter с composition timestamp (authored timeBase + clip start). Project/history не меняются.

Focused sublane13 tests PASS, включая key B local500ms + clipstart1s => playhead1.5s. Typecheck0 errors/warnings, lint/diff check PASS. Physical Chromium suite PASS: right-click lower unselected key после Undo выбирает именно local1.5s key, seeks clipstart+1.5s, Escape закрывает menu, document/history unchanged. Existing physical selected-key easing/playhead preservation и остальные regressions PASS. EasingMenu source дополнительно подтверждает открытые hover previews, full gallery, Smooth/tangents и extrapolation submenus; они пока не перенесены. Full parity не достигнута.

## Этап 86: easing menu hover/focus curve previews

Source EasingMenu arming row preview применяется к каждому selected group без document commit. Local menu теперь вычисляет immutable per-key overrides для всех selected subjects при pointerenter/focus и публикует shared keyframePreviews. SVG curves/diamonds читают эти tracks. Pointerleave/blur, Escape, outside press, successful commit и component unmount очищают owned menu preview; остальные timeline instances не очищают чужую projection.

Focused sublane14 tests PASS: two-lane hover previews содержат selected outgoing EaseOut; actual two-key SVG points меняются при preview и возвращаются на Escape, project/history остаются byte-equal. Keyboard focus также arms preview. Typecheck/lint PASS до финального расширения test assertions; diff check PASS. Physical hover preview и picture-renderer consumption пока не квалифицированы; full gallery/tangents/extrapolation и полный inventory ещё не перенесены.

## Этап 87: ephemeral tracks в picture preview

Project automation patching выделен в immutable projectAutomationValues(document, patches), используемый commit path и preview projection. CompositionPreview вычисляет derived document из authored document + keyframePreviews; active visual/audio/transition clips, transform/opacity/mask sampling читают эту projection. Export, project storage и history продолжают читать authored document. При отсутствии previews возвращается original document.

Focused31 tests PASS: mounted preview opacity изменяется0.5→0.9 при transient override и возвращается0.5 после clear, authored JSON/history unchanged. Existing visual/mask/media tests и key menu/drag tests PASS. Typecheck0 errors/warnings, lint/diff check PASS. Full frontend603 tests/103 files PASS после shared commit refactor. Physical picture preview, audio/mask transient projection и source scoped ownership пока требуют дополнительных проверок; full parity не достигнута.

## Этап 88: physical hover picture preview и Escape selection preservation

Chromium suite расширен actual row.hover(): selected opacity segment sampled на midlocal875ms, picture opacity0.5; armed EaseIn меняет SVG points и rendered text opacity0.725 без project/history mutation. Escape возвращает exact original curve и picture0.5. После этого применение EaseIn даётone Undo и сохраняетplayhead.

Qualification выявила Escape на focused diamond: локальный keydown очищал key selection перед menu dismissal; repeat context считался unselected и выполнял seek. Исправлено menu-first Escape handler со stopPropagation; focused-diamond component regression проверяет сохранение выбора. Final physical Chromium suite PASS с hover/curve/picture/Escape/commit/Undo и existing regressions; focused14 tests PASS, typecheck0 errors/warnings, lint/diff check PASS. Первый failed run выбирал only media DOM вместо текстового fixture, selector исправлен; второй выявил описанную Escape ошибку. Full gallery/tangents/extrapolation/scoped preview ownership и общий inventory ещё открыты.

## Этап 89: context entry на curve segment

Сверен source KeyframeCurveGraph segment hit: transparent12px polyline поверх rendered segment открывает menu outgoing owner, в отличие от diamond path не вызывает focus seek. Local expanded graph теперь содержит hit polyline каждого authored adjacent-key segment, использующий тот же sampler/range, включая Hold corner. Right-click открывает существующее selected-key menu; unselected owner выбирается без seek, selected owner сохраняет набор. Locked channel refuses.

Focused sublane15 tests PASS: curve hit соответствуетleft100ms key, сохраняет playhead, Hold меняет only owner outgoing override одним history action. Lint/diff check PASS. SVG pointer hit находится в aria-hidden graph; клавиатурный context path доступен на owner diamond. Initial typecheck предупреждение о static SVG context handler устранено documented directive. Physical segment hit qualification и source selection nuances ещё требуют проверки; full feature inventory остаётся открытым.

## Этап 90: scoped easing preview lifecycle

Source KeyframeLane openInterpMenu подтверждает stage89 selection rule: outside selection заменяет набор outgoing owner, curve path не seeks. Source easingPreviewStore scoped clearing также подтверждает требование сохранять чужие временные tracks. Local menu теперь merges armed subject previews, сохраняя unrelated target/property entries. Menu cleanup сравнивает принадлежавшие ему value references и addresses; newer replacement того же subject не стирается Escape/unmount старого menu.

Focused sublane16 tests PASS: unrelated opacity preview переживает menu hover, newer x preview переживает dismissal; existing hover/curve/picture and atomic gesture component tests PASS. Typecheck0 errors/warnings. Lint выявил неправильный текст пояснения внутри svelte-ignore stage89; пояснение вынесено отдельным comment, suppression содержит только diagnostic code. Physical scoped ownership, key-drag scoped clearing и segment hit qualification ещё открыты; full feature parity отсутствует.

## Этап 91: scoped key-drag preview lifecycle

Menu и drag используют общие publishPreviews/clearOwnedPreviews. Drag merges projected subjects в shared previews и хранит конкретные опубликованные values. Invalid projection, release, Escape/pointercancel и unmount очищают только принадлежащие gesture values; unrelated subjects и newer replacement одинакового target/property сохраняются. Это устраняет глобальную очистку предыдущего drag path.

Focused timeline+picture33 tests PASS: Escape и pointercancel после multi-layer preview сохраняют unrelated opacity и newer x replacement, удаляя original другого selected subject; authored document/history unchanged. Existing move/scale/invalid recovery/menu/curve/picture regressions PASS. Typecheck0 errors/warnings, lint/diff check PASS. Reference equality ownership не имеет отдельного token для повторной публикации exact same value reference; concurrent same-reference ownership и physical segment hit ещё не квалифицированы. Full feature parity не достигнута.

## Этап 92: keyboard context entry и physical segment hit

Owner diamond теперь открывает easing menu через ContextMenu или Shift+F10, используя координаты focused key. Selected multi-key набор сохраняется; keyboard path использует то же selection/seek admission, что pointer diamond. Focused20 tests PASS, включая обе клавиши. Typecheck0 errors/warnings, lint PASS.

Physical Chromium suite PASS: right click реальными координатами середины transparent segment polyline открывает menu без seek; focused diamond Shift+F10 также открывает menu. Existing hover curve/picture/Escape/commit/Undo и scoped gesture behavior regressions проходят. Первый browser run выявил неоднозначный locator после появления transparent hit contour; visual-curve locator исключает hit contour, повторный run PASS. Keyboard menu navigation/focus management, tangent/extrapolation/full preset gallery и общий inventory ещё открыты.

## Этап 93: keyboard preset navigation и focus return

Keyboard context entry ждёт DOM menu и переносит focus на first preset, который arms existing ephemeral preview. ArrowUp/ArrowDown циклически переходят между preset buttons, Home/End выбирают first/last; events consumed до timeline shortcuts. Escape внутри menu clears owned preview и возвращает focus connected opener, preserving selected keys. Successful preset application также возвращает focus. Outside press и unmount закрывают без focus stealing.

Focused21 sublane tests PASS: Shift+F10 initial focus, ArrowUp wrap, Home/End, Escape opener restoration, selection preservation и cleared preview. Existing menu/gesture/curve tests PASS. Typecheck0 errors/warnings, lint/diff check PASS. Physical keyboard navigation/Enter selection и Tab dismissal ещё не квалифицированы; full source gallery/tangents/extrapolation и общий inventory остаются открытыми.

## Этап 94: Tab dismissal и physical keyboard preset commit

Tab/Shift+Tab menu keydown closes через тот же owned-preview cleanup и возвращает исходную точку focus перед default browser traversal; default event не блокируется. Focused23 tests PASS, включая оба направления Tab с unchanged document/history. Typecheck0 errors/warnings, lint/diff check PASS.

Physical Chromium suite PASS: Shift+F10 фокусирует Hold; ArrowUp wraps на EaseInOut, Home возвращает Hold, End и Enter применяют EaseInOut к outgoing key. Menu closes, focus возвращается diamond, history увеличивается на1 и Undo полностью восстанавливает project. Повторное keyboard opening и physical Tab закрывают menu и очищают shared preview. Existing physical segment/hover/picture и прочие regressions PASS. Full preset gallery, procedural easing, tangents, extrapolation и общий inventory остаются незавершёнными.

## Этап 95: authored keyframe clipboard snapshot

Сверен source renderer/keyframe/clipboard.ts: one shared copy slot, immutable snapshot, property grouping без layer identity, общий earliest composition timestamp; paste в selected layers относительно playhead, static lifting без invented constant key, pasted-last collision, retained out-of-range keys.

Добавлен snapshotSelectedKeyframes foundation: groups объединяются по parameter, authored timeBase переводится на composition clock, earliest key всего набора становится0, остальные сохраняют cross-layer temporal offsets; keys cloned, outgoing override/default materialized, source addresses отсутствуют. Это предотвращает потерю fallback interpolation при переносе на target с другим default. Stale selection refuses до выдачи partial snapshot. Focused2 tests PASS на different layer starts/timeBases, outgoing easing и immutable inputs; typecheck0 errors/warnings, lint PASS.

Это foundation, ещё не пользовательская clipboard feature: shared slot, Ctrl+C/V routing, selected target mapping, collision merge и atomic paste не реализованы. Source сохраняет negative/out-of-range keys, local admission требует отдельного решения для соответствия. Full feature parity не достигнута.

## Этап 96: keyframe paste planning

planKeyframePaste сопоставляет copied groups с target parameter, converts shared composition paste point через каждый target start/timeBase и возвращает immutable entries + landed selection ticks + skipped parameters. Static targets lifted только pasted keys, без invented constant key. Keyed targets сохраняют default и existing keys; pasted-last Map collision policy заменяет совпадения, repeated copied groups fold одним address. No duration clamp: beyond-end и negative local keys сохраняются в planner result, как source clipboard.

Focused4 clipboard tests PASS: cross-base/start offsets, static lifting, interpolation retention, pasted collision, retained negative/beyond-end ticks, same-parameter group fold, duplicate target refusal и immutable source. Typecheck0 errors/warnings, lint/diff check PASS. Native tick u64 и frontend admission пока отвергают negative local keys — planner fidelity не означает executable negative-key paste. Shared clipboard slot, Ctrl+C/V, target descriptor mapping и one atomic commit/selection ещё не подключены. Full parity отсутствует.

## Этап 97: editor keyframe copy slot и Ctrl+C

Composition UI имеет один typed clipboard slot (keyframes либо layer reference). copyCompositionKeyframeSelection snapshots все selected subjects; parameter address содержит target kind/property без clip/mask identity, time shape материализуется через stage95. Empty selection не заменяет slot. Timeline key lane/diamond Ctrl+C и Meta+C используют copy path; copy не commit, не persisted/history. Snapshot переживает authored key edit и Undo. Test reset очищает slot, ordinary project activation не сохраняет его в project storage.

Focused24 sublane tests PASS: Ctrl+C заменяет layer slot двумя selected parameter groups, earliest100ms rebased0 и second global1.5s rebased1.4s; document/history unchanged; later value42 edit и Undo не меняют copied snapshot. Typecheck0 errors/warnings, lint/diff check PASS. Layer copy entry, global command routing, Ctrl+V target mapping/atomic commit и signed native keyframe support ещё не подключены; полного clipboard/full feature parity нет.

## Этап 98: selected-clip keyframe paste transaction и Ctrl+V

pasteCompositionKeyframeClipboard строит compatible visual/audio/mask property addresses selected clips, current keyed tracks либо constant fallback, затем использует planner stage96 на playhead. Один commitAutomationValues применяет all patches; после commit selectedKeyframes заменяется landed ticks. Unknown/incompatible groups report skipped; no target refuses. Timeline key lane/diamond Ctrl+V/Meta+V обрабатывает keyframe slot. Layer slot paste ещё не подключён.

Focused28 tests PASS: cross-layer copiedx/y paste в selected A at500ms сохраняет existingx100ms, добавляетx500ms и lifts y только1.9s; one Undo полностью восстанавливает project. No-target отказ и locked participant/negative local key в multi-target paste оставляют document/history unchanged. Typecheck0 errors/warnings, lint PASS до последней пары regression additions, diff check PASS. Negative local key отказ — retained gap относительно signed source, не заявляется parity. Physical clipboard, global command routing, layer copy/paste slot, mask-address semantics и signed native support остаются открытыми; полного переноса нет.

## Этап 99: shared copy/paste commands

Shortcut catalog содержит composition.copy/paste с Mod+C/V и labels/descriptions; App generic composition dispatch использует shared registry, поэтому focus на clip вместо key lane больше не мешает keyframe clipboard commands. Availability: copy требует key selection; paste требует keyframe slot, selected clip и unlocked participants. Registry dispatch делегирует stage97/98 APIs. Palette/settings получают команды из catalog. Layer copy/paste fallback ещё отсутствует.

Registry3 tests PASS, включая copy snapshot, paste at1s, landed selection, one history action и Undo восстановления. Full frontend622 tests/104 files PASS; typecheck0 errors/warnings, lint/diff check PASS. Local key-lane handler всё ещё принимает hardcoded C/V и не согласован с custom binding removal; этот gap открыт. Physical global clipboard/agent boundary, copied mask parameter addressing и signed native keyframe support ещё требуют реализации/qualification. Full parity не достигнута.

## Этап 100: configurable key-lane copy/paste routing

TimelineKeyframes больше не проверяет hardcoded Ctrl/Meta+C/V. Key lane/diamond использует тот же shortcutCommandForEvent и shared command registry, что App. Reassignment либо removal binding применяется на обеих поверхностях; shortcut capture исключает выполнение команды. Handled command consumes event, предотвращая повторное выполнение App path.

Focused29 sublane tests PASS: Mod+Shift+C/V заменяет default bindings, former Mod+C не создаёт clipboard, former Mod+V не меняет history, reassigned keys copy/paste одним history action. Existing clipboard/key gesture/menu tests PASS. Typecheck0 errors/warnings, lint/diff check PASS. Physical configurable clipboard, layer slot copy/paste, mask addressing и signed native keyframe support остаются открытыми. Full parity не достигнута.

## Этап 101: one copy slot с clip fallback

Сверен source App copySelected/pasteAtPlayhead: selected keys приоритетны, иначе copy primary layer reference; paste dispatch по kind последнего copy. Local copyCompositionSelection/pasteCompositionClipboard повторяют этот dispatch. Layer slot stores primary clip reference; paste клонирует exactly one referenced clip at playhead на его source lane через existing pasteClips, assigns fresh id и selects copy, one commit. Source reference может отражать позднейшие authored edits; keyframe slot остаётся независимым snapshot. Copy/paste catalog descriptions обновлены для обоих kinds.

Focused command+sublane33 tests PASS: key copy затем clip copy заменяет slot, clip paste сохраняет animation, lands at2s, fresh id/selection и one Undo полностью восстанавливает project. Lint/diff check PASS. Initial typecheck выявил inference union flatMap в test, explicit CompositionClip return type исправлен; final typecheck0 errors/warnings. Physical clip clipboard, source linked paste semantics, overlapping destination admission и stale/deleted source reference qualification ещё открыты; full parity не достигнута.

## Этап 102: automatic lane selection для clipboard clip paste

Сверен source main/state/actor paste_layer и mutations/duplicate: single clipboard paste detached clone, automatic reverse free-lane scan либо new lane; whole-link paste_layers — отдельная операция. Local clipboard paste теперь вычисляет exact snapped clone interval через existing pasteClips на immutable prospective new lane, затем reverse-scans unlocked same-kind lanes по interval overlap. Available lane получает clone; иначе новый lane и clone commit вместе, one Undo. Это устраняет overlap refusal stage101. Local lanes typed, source unified/reserved lane policy остаётся отличием. Source new-lane path имеет separate track+paste history commits, local deliberately retains one atomic action.

Focused registry6 tests PASS: overlap paste at0 создаёт lane иfresh clone, one Undo removes both; clipboard clip reference отражает later opacity0.4 edit; source deletion makes paste unavailable и executionfalse без document/history mutation. Earlier typecheck0 errors/warnings, lint PASS до финального live-reference regression, diff check PASS. Physical clipboard, locked-source copy semantics, effect/mask id cloning и reserved lane policy ещё требуют qualification/implementation. Full feature parity не достигнута.

## Этап 103: independent cloned style effects

Source cloneLayer deep-clones content; paste reassigns layer/effect identities. Local cloneClip уже копировал masks и их animatable tracks, но videoEffects array/items оставались shared references. Добавлено independent array/item cloning для videoEffects; existing clip identity changes остаются в duplicate command. Local style effect model не имеет effect id, mask id scoped по clip и сохраняется.

Focused composition41 tests PASS: duplicated effects equal по content, different array/item references; masks/keyframe values independent и per-key EaseIn metadata preserved, source document unchanged. Typecheck0 errors/warnings, lint/diff check PASS. При добавлении regression ошибочный final write временно заменил test file кодом commands; все original HEAD + recorded dirty edits восстановлены из session commands. Pre-turn40-line diff (39 additions/1 deletion) восстановлен до добавления нового test; new regression appended separately. Full frontend627 tests/104 files PASS после восстановления. Full effect identity/source capability parity и общий inventory ещё открыты.

## Этап 104: physical unified clipboard qualification

Chromium real MCP/editor suite PASS с actual Control+C/V. Focused lower key copy создаёт keyframe slot; physical clip click и focus переводят target на clipA, global paste atlocal1s добавляет opacity0.2 key, сохраняет existing250ms/1.5s и выделяет landed1s. History +1, MCP Undo exact project restoration. После clip copy slot переключается layer; paste на occupied source time добавляетfresh clone черезautomatic lane policy, animation equal original, clone selected, history+1 иUndo exact restoration.

Existing physical easing/menu/curves/gestures, MCP batch/history/analysis иdisconnect regressions also PASS. Script diffcheck PASS. Mac Meta+C/V и custom bindings physical qualification, locked-source semantics, signed native keys, advanced presets/tangents/extrapolation и остальной inventory открыты. Полного переноса всех функций нет.

## Этап 105: clipboard protected source read

Source single paste_layer reads requireLayer/cloneLayer без mutation source. Local clipboard clone теперь допускает readonly locked source через explicit allowLockedSource option; independent destination ensureUnlocked остаётся обязательным даже если destination/source lane совпадают. Automatic clipboard lane policy пропускает locked destinations и может создать unlocked lane; clone сохраняет authored clip locked flag. Registry layer-slot availability требует surviving source, а не source editability. Ordinary duplicate/set paste paths без option по-прежнему reject protected sources.

Focused composition/locks/registry56 tests PASS; дополнительный target-lock assertion и registry7 tests PASS. Locked clip на locked source lane копируется в new lane одним history action, original track JSON unchanged, Undo restores project. Explicit readonly-source option не позволяет вставку на locked target. Typecheck0 errors/warnings, lint/diff check PASS до последнего assertion. Physical protected-source clipboard, signed keyframes и полный inventory остаются открытыми. Full parity не достигнута.

## Этап 106: scale-link authoring invariant и операция связывания

Сверен source main/state/mutations/scaleLink.ts: linking заменяет весь Y свежей копией X, unlink сохраняет tracks, divergence clears flag на final result commit, не между entries batch. Добавлен optional scaleLinked для video/image/text, independent full-track copy с per-key interpolation, exact structural twin comparison (time base, track easing, ordered key tick/value/easing). normalizeComposition снимает stale/diverged true flag после validation на final document; старые документы без flag сохраняют прежнее поведение. Invalid flag type и audio flag rejected.

Inspector checkbox связывает/развязывает axes через один undoable commit. Mounted regression проверяет full-track copy, one-history action, exact Undo, subsequent divergent edit auto-unlink и Undo restoring link. Pure regressions проверяют deep-copy isolation, easing/time-base divergence и JSON restore normalization. Full frontend632 tests/105 files PASS; typecheck0 errors/warnings, lint PASS, diffcheck PASS.

Это перенос модели и операции linking, а не полная source Scale UX parity: composite linked Scale descriptor, fan-out inspector/keyframe gestures, gizmo resizing, legacy missing-flag twin inference и fresh per-track key identities ещё открыты. Backend получает authored X/Y tracks через существующий payload; scaleLinked — authoring metadata, native flag/command не добавлены. Full feature inventory остаётся незавершённым.

## Этап 107: composite linked Scale в keyframe inspector

Сверен source renderer/keyframe/fanOut.ts: composite descriptor writes both tracks as one batch, twin copy independent, linked invariant checked на final result. Visual keyframe inspector теперь скрывает scaleY при scaleLinked и показывает scaleX как единый Scale. Add at playhead, row time/value edit, per-row deletion, track interpolation и reset вызывают explicit fanOutScale edit. Twin values commit через commitAutomationValues одним history action; raw parameter APIs по умолчанию independent и могут снять flag при divergence. Empty/reset linked track материализует одинаковый X fallback constant в обе axes, чтобы implicit unequal static dimensions не разрушали связь.

Focused inspector/state/scaleLink67 tests PASS; дополнительная mounted reset/Undo regression, inspector25 tests PASS. Actual DOM controls проверяют отсутствие Y option, Scale label, add/value/interpolation/time/delete/reset twin equality и active link после каждого edit, one-commit add и exact multi-Undo restoration. Typecheck0 errors/warnings, lint/diffcheck PASS. Full frontend632 tests относится к этапу106, после этих изменений выполнены focused checks.

Shared selection move/scale/delete, timeline curves/clipboard, gizmo resize, physical browser Scale qualification и source reset/last-key conversion fidelity ещё открыты. Полная parity не достигнута.

## Этап 108: linked Scale timeline gestures и preview

Composite linked Scale теперь скрывает Y channel на timeline. Добавлен общий linkedScaleAutomationPatches для UI batch: twins получают independent copies, preview ticks и другие metadata сохранены; conflicting dual-axis values rejected before commit. Selection move/scale/easing/delete используют fan-out перед final admission, а pointer/easing preview publisher — тот же helper. Selection subjects остаются исходными, hidden Y не удваивает selected count. Raw parameter edits сохраняют independent semantics этапа106.

Mounted pointer regressions (move/Alt-scale/Escape) проверяют одну visible X channel, equal X/Y previews без document/history mutation, matching committed value, active link, one undo и exact restoration. Selection easing и delete-all сохраняют link и twins; delete-all материализует shared constant, Undo restores исходные tracks. Focused timeline+inspector57 tests PASS; typecheck0 errors/warnings, lint/diffcheck PASS. Initial regression неправильно ожидала сохранения selection после Undo moved keys; перед независимым easing test явно восстановлена selection исходных ticks.

Physical browser linked-scale gestures, clipboard composite fan-out, selection normalization при переключении link, gizmo resize, source static-reset/last-key conversion fidelity и прочий inventory остаются открытыми. Полный перенос функций не завершён.

## Этап 109: clipboard Scale fan-out и replaced-Y selection

Source clipboard использует expandScaleFanOut перед update_param_tracks_multi; source actor применяет entries последовательно, затем проверяет invariant один раз. Local keyframe paste теперь применяет тот же twin helper перед commit. Вставка X или Y в linked target сохраняет link, независимые равные tracks и one Undo. Landed selection canonicalized на visible X; одинаковые dual-axis records объединяются по target/property и deduplicated ticks. Linking снимает selection старой заменённой Y track и scale previews этого clip; другие targets/previews сохраняются.

Дополнительно исправлена stage108 conflict refusal: source последовательные dual-axis entries допускает, поэтому later entry теперь replaces оба twins. Regression раздельно проверяет X copy, Y copy, одинаковые X+Y и divergent X+Y с later-Y values9/10. Focused timeline/registry44 tests PASS; earlier timeline/inspector60 tests PASS. Exact project Undo, per-key easing, fresh destination local ticks и single visible selection checked.

Physical linked-scale browser qualification, selection identities/model differences, stale pasted selection при dual-axis разных times, source reset/last-key conversion fidelity, gizmo и прочие features остаются открытыми. Полная parity не достигнута.

Final stage109 checks: typecheck0 errors/warnings, lint PASS, diffcheck PASS.

## Этап 110: faithful static conversion после удаления/отключения animation

Source renderer/keyframe/edits removeKeyframe сохраняет value удалённого последнего key; source timeline/keyframeBatch removeKeys при удалении всех keys сохраняет value последнего authored key по времени, независимо от порядка selection. Source KeyframeField stopwatch использует collapseToStatic, sampling текущий local playhead. Local deleteKeyframe теперь возвращает constant removed value, missing/static deletion no-op; batch delete-all берёт value последнего authored key. clearCompositionAutomation фиксирует sampled playhead value в constant и делает no-op для implicit static field. Inspector действие переименовано в «Сделать статическим». Linked Scale получает тот же constant в X/Y через existing atomic fan-out.

Focused keyframe/timeline/inspector/state106 tests PASS; после value assertions и sampled linked regression focused68 tests PASS. Full frontend643 tests/105 files PASS. Последняя no-op guard для missing animation дополнительно проверена timeline38 tests PASS после полного прогона. Regression sampled EaseIn Scale1→3 на500ms даёт constant1.25 в обе axes, active link, one history entry и exact Undo; final-key/batch conversion сохраняет authored2. Typecheck0 errors/warnings и lint PASS до final guard; diffcheck PASS.

Это numeric tracks текущей модели. Color animation, advanced source splines/extrapolation/tangents, signed authored keys, physical linked Scale browser checks, gizmo и прочий inventory остаются открытыми. Полный перенос всех функций не завершён.

## Этап 111: inherited outgoing easing при insertion

Source renderer/keyframe/edits upsertKeyframe копирует preceding segment для нового ключа; без preceding начинает Linear; existing-key update сохраняет segment, explicit easing заменяет его. Local numeric upsert теперь inherits explicit preceding per-key interpolation. Если preceding использует track fallback, новый ключ сохраняет тот же fallback; перед первым ключом нелинейного legacy track добавляет explicit Linear. Existing metadata preserved; explicit optional interpolation теперь также задаёт outgoing override existing/new key, default lifting остаётся Linear.

Pure regressions проверяют middle/trailing insertion, leading Linear, existing update preservation, explicit override и source immutability. Linked Scale integration проверяет new500ms key между250ms EaseIn и1s Hold, equal independent twins, active link, one history action и exact Undo. Focused keyframe/per-key/timeline/inspector71 tests PASS до integration addition, затем timeline39 tests PASS. Typecheck0 errors/warnings, lint/diffcheck PASS.

Source tangent copying (`K.out=A.out`, `K.in=B.in`), advanced segment kinds, signed/fractional authored key identity и прочий inventory остаются открытыми. Новый этап не означает полного переноса функций.

## Этап 112: physical linked Scale qualification

Real Chromium + MCP editor suite PASS. Authored ScaleX fixture seeded через state API, дальнейшие изменения выполнены actual inspector checkbox, combobox/spinbutton и mouse gestures. Checkbox связывает full X/Y tracks, records one action, MCP Undo exact baseline. Inspector shows one Scale option без Y; spinbutton value edit updates twins, сохраняет flag, adds one history action и Undo restores exact document.

Physical pointer drag по visible X key publishes equal X/Y previews without document/history mutation. Escape+mouseup clears preview and leaves exact document; normal release commits same preview value in both axes, keeps active link, one Undo restores project. Hidden Y channel absent. Existing MCP/history/FFmpeg/selection/clipboard/curve/context regressions также PASS. Lint/diffcheck PASS.

Physical Alt-scale linked path пока покрыт mounted pointer tests stage108, browser здесь проверяет move/Escape. Physical linked clipboard, native signed keys/fractional identity, advanced source segments/tangents/extrapolation, gizmo resize и полный inventory всё ещё открыты. Полный перенос всех функций не завершён.

## Этап 113: signed authored key ticks

Native Keyframe.tick мигрирован u64→i64; timing clips/source/ramp остаётся unsigned. Native sampler принимает signed tick/seconds, не clamps negative sample clock; difference вычисляется через i128 чтобы MIN/MAX i64 span не overflow. FFmpeg adapter уже выводил signed numeric expressions, теперь принимает signed wire keys. Compiler/service positive duration high-water comparison использует max(last.tick,0), отрицательные ключи не превращаются в огромный unsigned extent. Existing typed test fixtures адаптированы; unrelated speed-ramp unsigned fixture сохранён.

Frontend authored key validation допускает signed safe integers и strict order с -Infinity initial sentinel. Pure upsert/update tick admission и sampler допускают negative keys/clock; clipboard сохраняет negative clip-local ticks вместо atomic rejection. Locked-target rejection сохраняется. Negative paste regression проверяет -1s X на target, стартующем в1s, one history entry и exact Undo.

Native6 keyframe tests PASS, включая signed JSON roundtrip, interpolation across0, negative sampling и extreme i64 span. Real FFmpeg signed expression frames at0/.5s gray100/150 совпадают с native sample. Compiler49 tests PASS; all-targets test compile PASS. Full frontend646 tests/105 files PASS, focused signed key/clipboard46 tests PASS, typecheck0 errors/warnings, lint/diffcheck PASS. Backend executable rebuild выполняется для последующей browser qualification.

Positive keys beyond clip duration, native/editor retime gesture bounds для negative selection, fractional key time/identity, source advanced segments/tangents/extrapolation и full inventory остаются открытыми. Полная parity не достигнута.

Stage113 backend executable rebuild PASS (dev profile); subsequent E2E will use the signed-key binary.

## Этап 114: authored keys после clip end

Source clipboard сохраняет authored keys независимо от layer span; native/source runtime sampling ограничивается реальным rendered interval слоя. Удалён local keyframe-duration refusal из frontend authoring validation, backend service plan admission и FFmpeg command compiler. Timing/value/count/order/time-base checks сохраняются. Clip timing остаётся отдельным от authored key extent.

Frontend paste regression сохраняет Y key3.4s у text clip duration2s, one commit и exact Undo. Focused composition/timeline81 tests PASS. Backend positive-outside image/source-audio/independent-audio regressions переведены на acceptance; invalid pan/gain ranges и active point budgets продолжают проверяться. Audio command expected duration остаётся5s при key4.000001s на4s clip. Backend composition106 tests PASS; executable rebuild PASS. Typecheck0 errors/warnings, lint/diffcheck PASS.

Первый прогон выявил ошибочную переименовку parameter в другом validator; исправлено точным scope edit. Старые audio duration-refusal assertions обновлены после broader native regression. Editor per-row min/max и retime gesture constraints ещё не перенесены на unrestricted authored times; native signed/positive-outside physical composition render, fractional identities, advanced curves и полный feature inventory остаются открытыми. Полная parity не достигнута.

## Этап 115: signed/outside authored time editing и recovery gestures

Сверен source renderer/keyframe/batchRetime: групповой translate ограничен tightest layer walls, negative/outside keys могут быть recovered внутрь; невозможная общая lo>hi даёт no-op. Scale сохраняет one-frame no-merge floor, даже когда outside selection задаёт меньший wall maximum. Local group validation теперь допускает signed/outside selected keys, impossible translate returns unchanged groups/delta0; scale допускает signed composition anchor. Walls/floor сохраняются по source.

Inspector row time input не имеет min0/maxclipDuration; updateCompositionAutomationKeyframe передаёт authored tick в safe-integer admission без clamp к clip duration. Mounted regression фактически меняет время на-0.25s и8s, проверяет authored ticks, two history actions и exact Undo. Pure regressions покрывают negative recovery→0, trailing recovery→clipEnd, impossible wide selection no-op, feasible centered scale и outside zero-anchor floor. Initial test ошибочно ожидал factor0.5 при zero anchor с negative key; source floor rule подтверждён отдельным assertion, centered recovery проверяется отдельно.

Focused timeline/inspector/selection72 tests PASS, дополнительный floor assertion и selection6 tests PASS. Typecheck0 errors/warnings, lint/diffcheck PASS. Physical signed/time-outside gestures, signed/outside full composition export render, row-retime collision normalization, fractional identities, advanced source curves и остальной inventory остаются открытыми. Полная parity не достигнута.

## Этап 116: real composition render signed/trailing keys

Добавлен production-pipeline integration test real_signed_and_trailing_keys_render_without_extending_the_clip. Actual FFmpeg генерирует white64×64/10fps source1s, затем FfmpegCompositionExportCompiler + run_compiled_ffmpeg экспортируют primary opacity track с authored keys-1s→0 и2s→1. Test требует доступных ffmpeg/ffprobe, не silently skips qualification.

Real render PASS: expected output1s и probe duration1s±120ms; actual RGB frame brightness at0/.4/.8s соответствует native signed track samples85/119/153 с tolerance7 для YUV/H264 quantization. Это проверяет весь composition graph, clipping output interval, signed expression и effect после конца clip без изменения длительности. Diffcheck PASS. Export fixture files isolated в tempfile и cleaned автоматически.

Этот тест квалифицирует numeric opacity на primary video. Audio/mask/overlay signed render, physical signed inspector/clipboard, fractional identities, row collision normalization, advanced curves/tangents/extrapolation, gizmo и остальной inventory остаются открытыми. Полная parity не достигнута.

## Этап 117: integer row retime collision normalization

Source renderer/keyframe/edits retime reorders keys; main/state/mutations/animated normalizeKeyframes stable-sorts и deduplicates exact integer times last-write-wins. Fractional time collisions source rejects separately. Local updateKeyframe больше не отказывает на exact occupied tick: mapped+stable-sorted entries folded через Map, last stable record retains full shape/value/easing. Это намеренно отличается от batch movement, где moved keys replace stationary destinations.

Pure regression проверяет earlier→later collision (later stationary value2/Hold wins), later→earlier collision (moved later value9/Hold wins), source immutability. Mounted inspector меняет время второго linked Scale key на0: equal X/Y one-key tracks сохраняют value2/Hold и active link; one history action, exact Undo restores both original keys. Focused keyframe/inspector/timeline75 tests PASS, typecheck0 errors/warnings, lint/diffcheck PASS.

Actor kind-specific write-grid snap, fractional key identity/collision rules, physical browser collision qualification, advanced segments/tangents/extrapolation и остальной inventory ещё открыты. Полная parity не достигнута.

## Этап 118: Continue extrapolation для текущих numeric curves

Сверен source native/eval extrapolate: Continue использует endpoint segment derivative; before/after independent, single-key/zero-period tracks hold. Добавлены optional extrapolation.before/after (Hold/Continue) в TS/native wire, default omission backward-compatible Hold. Samplers продолжают endpoint slopes текущих Linear/cubic easing modes, учитывая outgoing per-key override. FFmpeg adapter выводит affine продолжения вне диапазона; default Hold golden expression unchanged. Continue для local EaseIn p^3 соответствует source ease_in_cubic, а не source CSS ease_in preset: полный source Bezier/preset gallery остаётся отдельным gap.

Inspector позволяет отдельно выбрать удержание/продолжение до и после keys; linked Scale fan-out сохраняет twins в одном history action. Track cloning, upsert, payload mapping сохраняют extrapolation; scale twin equality учитывает обе стороны. Export compiler/service extrema включают samples на clip start/end, чтобы продолженные dimensions попадали в resource bounds.

Native7 keyframe tests PASS, включая serde roundtrip и real FFmpeg Continue frames10/10/30/90 против EaseIn samples. Native composition106 tests PASS и дополнительный continuation extent regression PASS (stored max2 → rendered max4 on4s clip). Focused frontend40 tests PASS: sampler derivatives, metadata retention, mounted linked Scale controls, exported wire и Undo. Full frontend653 tests/105 files PASS. Typecheck0 errors/warnings, lint/diffcheck PASS. Backend executable rebuild выполняется.

Loop/PingPong/Offset, Continue с source tangent/Bezier/Elastic/Bounce shapes, timeline tail drawing/range для Continue, physical browser/full composition Continue qualification и прочий inventory открыты. Полный перенос не завершён.

Stage118 backend executable rebuild PASS; subsequent runtime uses the new Continue wire and exporter.

## Этап 119: график Continue на таймлайне

Шкала numeric curve включает samples на clip start/end. Leading/trailing tails теперь используют общий sampler вместо постоянного endpoint value: Continue отображается диагональю, Hold сохраняет горизонталь. Tail interval ограничен границами клипа; signed/outside keys больше не создают обратные хвосты. Основные authored segments и diamonds сохраняются за пределами clip interval для редактирования.

Focused curve + mounted timeline suite: 45 tests PASS. Добавлены точные проверки обеих Continue tails, расширенной шкалы, signed/outside keys и single-key tail clipping. Полный перенос остаётся незавершённым: Loop/PingPong/Offset, source advanced easing/tangents и прочий inventory открыты.

## Этап 120: Loop/PingPong/Offset numeric evaluation и export

Сверен native/eval extrapolate pinned WeftCut: Euclidean floor cycles до первого ключа, first-inclusive periodic range, exact authored last endpoint сохраняется, PingPong mirrors odd cycles, Offset adds cycle*(last-first). Все три режима добавлены в native/TS samplers, wire enums, validation и inspector before/after choices; linked scale metadata fan-out сохраняется. Native folding uses i128 differences/rem_euclid без переполнения signed tick range. FFmpeg adapter повторно компилирует ordinary curve на folded time expression; outgoing easing overrides сохраняются. Single-key режимы держат endpoint.

Native8 keyframe tests PASS, including actual FFmpeg gray frames for each periodic mode at negative relative time, intermediate samples and boundaries with shifted clock. Native composition107 tests PASS. TS sampler10 tests PASS; mounted inspector31 tests PASS with three new mode cases, equal Scale twins, exported metadata, one action and exact Undo.

Открыты графики cyclic tails (текущий двухточечный tail подходит только Hold/Continue), строгие extrema Offset для resource bounds, full composition/browser cyclic qualification, native executable rebuild и весь remaining inventory. Этот этап не означает полного переноса.

## Этап 121: extrema Offset в native admission/compiler и шкале

Новый KeyframeTrack::value_bounds_seconds централизует прежние authored+clip endpoint bounds и добавляет повторённые Offset key extrema. Для каждого key вычисляются первый/последний cycle, попадающий в before/after interval; extrema cycle offset линейны, поэтому перебор всех периодов не нужен. Hold дополнительно включает left limit у следующего ключа. И service validation, и FFmpeg composition compiler используют общий метод; nonfinite bounds отвергаются. Authored-key bounds намеренно сохраняются даже вне clip interval, как в прежнем admission.

Regression 1→10→2 на1/1.5/2s, clip4s: max12 при endpoint4; до/после на -100..100s bounds[-100,108], проверено200001 native samples. Compiler integration max12 и count3 PASS. Native composition108 tests PASS. Frontend range зеркалит offset-extrema calculation; curve+timeline46 tests PASS. Native executable rebuild PASS (включает этап120 periodic modes). Cyclic tail plotting, full composition/browser periodic qualification, advanced curves и остальной inventory остаются открыты.

Stage121 verification: full frontend660 tests/105 files PASS; typecheck0 errors/warnings, lint and diffcheck PASS. No full parity claim.

## Этап 122: full composition render periodic/Continue qualification

Добавлен real_periodic_and_continued_opacity_matches_exported_frames в backend/tests/composition_render.rs. Реальный FFmpeg создаёт white64×64 source5s/10fps; composition compiler рендерит MP4 H264 отдельно для Loop, PingPong, Offset, Continue. Opacity keys1s→0.2,2s→0.4; before/after modes одинаковы. Проверены28 output RGB samples в0/.4/1.4/2/2.4/3.4/4.8s против заранее рассчитанного oracle (не вызова native sampler): before-first, ordinary segment, exact last key, later cycles. Pixel tolerance<7 учитывает encoding. Expected/probed duration5s для каждого mode; Done::Completed обязателен, FFmpeg/ffprobe отсутствие fails вместо skip.

Focused real integration test PASS, 4 MP4 renders и28 decoded frames,3.55s runtime. Diffcheck PASS. Это квалифицирует opacity/current Linear pipeline; arbitrary shapes, audio/masks/transforms, independent side combinations и physical browser mode selection не следуют из этого теста. Cyclic tail plotting, advanced curves и остальной inventory остаются открыты; полный перенос не завершён.

## Этап 123: cyclic tails на expanded timeline

keyframeTailPoints теперь строит повторённые authored segments для Loop/PingPong/Offset: mirrored odd Euclidean cycles, value offset per cycle, clip-boundary clipping, точные Hold vertical corners и cycle reset у Loop. Hold/Continue двухточечный путь сохранён. При subpixel period либо >131072 estimated points применяется bounded screen-resolution sampling (≤8193 points); столь плотные periods не получают exact per-cycle SVG representation. Samplers/export от этого rendering budget не меняются.

Pure tests проверяют все три periodic modes, final partial cycle, Loop reset, negative mirrored Hold corners; mounted timeline проверяет actual expanded lane vertical reset и отсутствие history/document mutation. Dense-period regression проверяет bounded finite output и immutable authored data. Physical screenshot/browser periodic graph qualification, advanced Bezier/tangent shapes и весь remaining inventory открыты. Полный перенос не завершён.

Stage123 verification: focused52 tests PASS, typecheck0 errors/warnings, lint/diffcheck PASS.

## Этап 124: real browser extrapolation controls и Undo

Physical Chromium + real MCP/desktop backend suite расширен выбором Continue/Loop/PingPong/Offset через actual inspector comboboxes. Для каждого mode отдельно after change и before change: X/Y keyed metadata twins, active scale link, two distinct single-action history increments; две Undo восстанавливают промежуточный и исходный документы точно. Actual expanded Scale X SVG tails проверяются finite, periodic before tail содержит >2 points. Сохранены и просмотрены четыре row screenshots /tmp/video-kadr-extrapolation-{continue,loop,ping_pong,offset}.png.

Первый запуск обнаружил ошибку selector в тесте: first curve belonged to another channel; исправлен explicit data-keyframe-curve scaleX selector. Повторный полный real MCP→Chromium suite PASS, вместе с прежними keyframe/clipboard/linked-scale pointer scenarios. Lint, script syntax и diffcheck PASS. Проверка graph geometry ограничена текущим authored cubic EaseIn track и screenshot row; advanced shapes и весь остальной WeftCut inventory открыты. Полный перенос не завершён.

## Этап 125: surviving selection после linked Scale dual-axis paste

Закрыт gap этапа112: если clipboard X и Y имеют разные relative times, ordered linked-scale fan-out оставляет last-axis track на обеих осях. Paste selection теперь intersect pasted ticks с actual committed visible property keyframes после canonical Y→X mapping; пустые subjects пропускаются, shared addresses deduplicate как прежде. Это убирает stale key addresses, из-за которых следующая copy выбрасывала Selected keyframe no longer exists.

Regression dual-axis copy X times0/.4 и Y .2/.6, target local playhead.25: landed visible selection .45/.85, subsequent copy yields rebased0/.4 values9/10; one paste history entry, copy не меняет документ/history, exact Undo restores original document. Timeline42 tests PASS, включая прежние same-time linked cases. Остальной WeftCut inventory остаётся открытым, полного переноса нет.

Stage125 typecheck0 errors/warnings, lint/diffcheck PASS.

## Этап 126: trim keyframes glued to content

Source main/state/mutations/trim.ts In edge calls shiftLayerKeyframes(-delta), Out edge only changes layer end; source split has separate retain/shift/collapse semantics. Local trimClip больше не вызывает boundary resampling: shiftClipAutomation сдвигает полные authored visual/audio/mask tracks, signed times сохраняются, key interpolation/extrapolation/clone independence удержаны. Fade bounds clamp сохранён. Split и grid-resnap пока используют прежний slice path — source split partition parity остаётся открытым gap.

Новый shiftAnimatableValue regression для Hold/Continue/Loop/PingPong/Offset проверяет signed shifted keys и equivalent samples по обе стороны endpoint range. Command integration audio gain/mask x теперь ожидает original values на times-2/2/6s после trim2..6s. Первый full run исполнял старые ожидаемые boundary samples и показал единственный контрактный failure; focused56 tests с обновлёнными expectations PASS. Полный перенос не завершён.

Stage126 final verification: full frontend672 tests/105 files PASS; typecheck0 errors/warnings, lint/diffcheck PASS. Physical trim curve qualification pending.

## Этап 127: ordinary split key partition

Source splitTrackHalf retains t<=cut on LEFT, t>cut shifted by-cut on RIGHT; an empty half collapses to authored first/last endpoint respectively. Added splitAnimatableValue and mapped visual/audio/mask helpers; splitClip now uses these, preserving easing/extrapolation metadata and independent cloned keys. Fade bounds remain clamped. Old boundary-sampling expectations updated for video x and audio pan (no artificial key at right-half zero).

Regression covers exact-cut ownership, negative authored keys, outgoing Hold metadata, extrapolation preservation, right rebasing, empty left/right collapse, source immutability. First full run found one legacy right-zero-key expectation, updated to source contract. Source time_map and Path progress exceptions plus effect parameter animation require separate unported model support; grid-resnap still uses existing slice path. Full WeftCut parity remains unproven.

Stage127 final verification: full frontend673 tests/105 files PASS, typecheck0 errors/warnings, lint/diffcheck PASS. Browser/render split qualification pending.

## Этап 128: move/raise grid-resnap preserves authored automation

Source mutations/move.ts delegates shift.ts; landed start/end rewritten without animated key rewrites. Local move-to-new-track and withClipGridStart no longer clip/resample animation when frame-grid resnap changes duration. Media/audio paths use zero-shift full cloned tracks plus existing fade clamping; text/image paths preserve animation. Removed unused sliceClipVisualData and slice imports from commands, preventing accidental boundary-key regeneration in these movement paths. Existing standalone slice helpers remain for compatibility/tests but command movement no longer calls them.

Regression asserts actual nontrivial media duration resnap with negative/trailing keys, per-key Hold/global EaseIn and Loop/Offset metadata unchanged; source clip immutable. Focused composition/raise/authoredExtent54 tests PASS, strengthened duration assertion composition42 tests PASS, typecheck0 errors/warnings, lint/diffcheck PASS. Physical resnap qualification, advanced curves and remaining WeftCut inventory pending; no full parity claim.

## Этап 129: real session trim/split key contracts

Real MCP→Chromium suite расширен authored linked Scale trim и split operations через apply_batch с expectedRevision. Trim actual layer start+.25/end-.25 проверяет полный key shift-.25; split at local.75 проверяет LEFT t<=.75, RIGHT t>.75 shifted-.75, equal X/Y twins в обеих halves. Каждая операция создаёт один history action, exact Undo восстанавливает весь документ. Полный browser suite PASS вместе с prior movement, clipboard, extrapolation и linked-pointer scenarios; script syntax/lint/diffcheck PASS.

Отдельной source slip mutation в pinned WeftCut не найдено; earlier предположение о slip-key shift не было доказано и не реализовано. Ordinary trim/split command parity квалифицирована на существующей linked text Scale fixture; source time_map/Path/effect special cases, media render split visual semantics и остальной inventory остаются открыты. Полный перенос не завершён.

## Этап 130: audio role assignment foundation

Сверен source state/model.ts AudioRole union и native/state/audio_role.rs: fixed order dialogue/music/sfx/voiceover, legacy default Dialogue. AudioClip local model получил optional role с validation; inspector accessible select позволяет назначить роль через общий immutable history update. Unknown role отвергается, non-audio target отказ, unchanged default choice no-op. Role stays in local document JSON/clones; render wire пока не содержит role — assignment ещё не меняет звук.

Mounted inspector34 tests PASS: three nondefault roles, document JSON retention, one history entry и exact Undo to omission. Typecheck0 errors/warnings, lint/diffcheck PASS. Это первый шаг audio role block, не role mixer parity.

Следующие требования из source: project-level four mix buses independent of timeline tracks; gain_db recorded edit, mute/solo unrecorded preferences across history; effective gain folds role gain into clip envelope, audible set filters role mute/solo; preview/export одинаковы; role meters. Persistence/API/native admission/render и UI общего микшера ещё требуют реализации и qualification. Nested compositions/Motifs/ASR/TTS/VLM/advanced curves и прочий inventory остаются открыты. Полный перенос не завершён.

## Этап 131: shared role audibility/gain projection

Composition получил optional audioRoles table (gainDb/muted/solo per fixed role), validated role names/finite gain/bools. Source native/eval role_audible: mute wins over solo, any solo across all roles; role_gain_linear=dB→linear. Added common audioRoleMix/audioRoleGain; preview AudioClip volume и export gain curve projection используют общий multiplier. Role table remains authoring metadata; native wire получает projected numeric gain, retaining timeBase/outgoing interpolation/extrapolation. Legacy absent table/role uses unity Dialogue.

New tests cover missing-role defaults, all-role solo, mute-over-solo, -6dB projected keys and curve metadata, immutable document. Export fixture corrected to include primary video required by existing builder, even for audio profile. Full run also found stage128 legacy timeGrid boundary-resampling expectation; updated to preserved authored tick33367/value1 despite shortened landing. Focused11 tests PASS.

Role assignment block не завершён: role gain/mute/solo UI/state/history preference commands и live meters pending; exported boosted gains still subject to existing native max16 admission, arbitrary finite extreme dB table can overflow projection and must be qualified/resolved before full mixer parity. Real audio amplitude/browser role playback qualification pending. Full WeftCut inventory remains open.

Stage131 final verification: full frontend679 tests/106 files PASS; typecheck0 errors/warnings, lint/diffcheck PASS. Remaining limits above unchanged.

## Этап 132: mixer role controls и preference history

Inspector содержит четыре role gainDb/mute/solo rows. setCompositionRoleGain записывает immutable document change через обычный commit; setCompositionRoleFlags patches only supplied booleans across current/past/future/checkpoint documents, preserving each snapshot's gainDb, undo cursor и redo branch. Flags bump agent revision, clear stale export result и schedule autosave, без новой undo action. Runtime guards reject unknown role/flags/nonfinite gain. Preferences intentionally independent of clip/track locks.

Mounted history regression: gain-6 one action, mute+solo no action; Undo gain→0 leaves flags, changing mute while redo future exists retains future; Redo gain→-6 carries latest mute=false/solo=true. Source main/state/history updateRoleFlags/native RoleFlagsPatch served as contract. Checkpoint rewrite implemented but dedicated restore qualification pending. Real role audio amplitude, native high-gain admission/overflow, browser mixer interaction/meters и остальной inventory открыты; full parity not reached.

Stage132 verification: focused37 tests PASS, full frontend680 tests/106 files PASS, typecheck0 errors/warnings, lint/diffcheck PASS.

## Этап 133: role preference checkpoint qualification

Runtime regression creates two real executeAgentTool checkpoints at different role gains (-3 and -9), changes mute+solo without undo entries (revision+2), verifies both checkpoints preserve their own gain while receiving latest flags. restore_checkpoint restores old gain/current flags, Undo restores newer role table including unrelated music gain-6. Project reset clears role table/checkpoints. Additional malformed/no-op checks prove unchanged flags don't revise, nonfinite gain/unknown role/nonbool flag throw without document/revision mutation.

Focused agent runtime+mounted inspector48 tests PASS. This closes dedicated local checkpoint behavior qualification; browser actual mixer/audio amplitude and project save/reopen qualification still pending, as are native high-gain admission, role meters and whole remaining inventory. Full parity not achieved.

## Этап 134: physical mixer controls + MCP checkpoint/Undo

Real Chromium scenario fills Music gain-6 then-12 through actual inspector, creates checkpoint at-6, toggles mute/solo checkboxes without undo entries, verifies document/history through real MCP. Undo→-6, Redo→-12 preserve flags and actual checkbox/input state; restore_checkpoint→-6 keeps latest flags; unmute leaves solo. Mixer screenshot /tmp/video-kadr-role-mixer-qa.png generated and viewed. Full MCP→browser suite PASS; script syntax/lint/diffcheck PASS.

Additional inspected gap: CompositionMediaLayer.svelte currently clamps HTMLMediaElement.volume to1 (line65), so positive total gain above unity cannot be faithfully previewed yet. A shared math helper alone is insufficient to qualify real audible boost. WebAudio gain graph/meters, native effective gain admission above16/overflow, actual RMS/render role tests and persistence reopen still pending. Remaining whole-product WeftCut inventory is open; full parity not achieved.

Stage134 screenshot inspection found expanded default form-grid rendering excessively tall. Mixer rows changed to compact four-column role/gain/M/S layout, preserving accessible control names; screenshot above is before compact correction. New compact browser screenshot qualification pending.

## Этап 135: WebAudio preview gain above unity

CompositionMediaLayer теперь routes HTMLMediaElement through shared AudioContext MediaElementSource→GainNode→destination. Element volume stays1, numeric envelope (clip×role×fades) is applied through GainNode without clamp to1. Graph resumes on playing, survives URL changes on same element, disposes source/gain and pauses old element on replacement/unmount. Without WebAudio fallback handles <=1 legacy volume; unsupported boost/error reports onmediaerror rather than being silently called equivalent. Nonfinite envelope mutes.

Controller regression gain4→0, context resume, idempotent cleanup and ignored post-dispose writes PASS. Mounted MediaLayer verifies actual graph construction with mocked WebAudio, gain4/element.volume1 and cleanup on unmount PASS. Preview15 tests PASS. Initial onDestroy import conflicted with Vitest SSR Svelte entry; replaced with rune effect cleanup and tests recovered. Real audible RMS/autoplay/CORS/browser lifecycle qualification pending; shared context remains app-level singleton. Native projected gain>16/overflow, meters and remaining inventory unresolved. Full parity not achieved.

Stage135 verification: full frontend683 tests/107 files PASS (run started before new mounted MediaLayer file), additional mounted MediaLayer1 test PASS; typecheck0 errors/warnings, lint/diffcheck PASS.

## Этап 136: real Chromium media/role gain RMS

Added test:e2e:audio-gain standalone Vite/Chromium script importing production createMediaGain/audioRoleGain. Real HTMLAudioElement plays PCM WAV blob (1kHz,0.02 amplitude); real MediaElementSource/GainNode feeds AnalyserNode/destination. RMS assertions verify unity, gain4, mute0, Music+6dB, solo exclusion by other role, mute-over-solo, replacing source URL with amplitude0.01 without rebuilding graph, silence after dispose, element.volume1. Controlled resources close/revoke/remove in finally. No autoplay override; trusted button click precedes production gain.resume, context must be running.

Measured unity~0.01413, boosted~0.05660, role+6dB~0.02820, replaced-source gain4~0.02831; mute/solo-block/muted-solo/disposed all0. This qualifies actual production helper graph amplitude and role multiplier, not full editor timeline clocks/export RMS or cross-origin source handling. Shared context lifecycle/meter/native high-gain admission/persistence reopen and remaining inventory still pending. Full parity not reached.

## Этап 137: native WAV role-projected amplitude qualification

Real composition integration renders independent AudioClip projected scalar gains1,+6dB (1.9952623),-6dB(.5011872),0 into four PCM WAV outputs. Primary embedded audio explicitly disabled to avoid double ownership; real source generated by FFmpeg. Each render requires Done::Completed, expected/probed1s duration, both stereo RMS channels sampled over.2..7s. Gain ratios must match multiplier within.002, muted output<1e-7. FFmpeg/ffprobe required, not skipped.

Actual RMS unity .06570894, +6 .13110665, -6 .03293245, mute0 on both channels. Test PASS (4 renders,1.66s runtime), diffcheck PASS. Combined with earlier frontend payload projection tests and real Chromium gain/role helper RMS, this qualifies constant gain math on separate pipeline stages; not one end-to-end UI→render request→browser-versus-WAV sample comparison. Animated envelopes, multi-role mixed signal, native gain>16/overflow, meters, persistence/reopen and remaining whole-product inventory still pending. Full parity not reached.


## Этап 138: диапазон регулятора громкости роли

Источник MixerPanel.tsx задаёт диапазон −30..+20 dB и шаг 0.5 dB. Inspector теперь использует эти границы и ограничивает введённые значения при подтверждении; дробное значение сохраняется, Undo возвращает предыдущие значения. Контракт программного setRoleGain в источнике не ограничен этим диапазоном, поэтому локальный программный setter и импорт не сужались.

Mounted Inspector: 36 tests PASS, включая дробный gain6.5, верхнюю/нижнюю границы и Undo. Typecheck/lint PASS. Это ограничивает обычный UI-ввод, но не закрывает native effective gain>16 и переполнение программно заданного gainDb. Для экспорта нужна отдельная передача множителя роли с сохранением ограничений авторского gain клипа. Полный перенос всех возможностей ещё не завершён.


## Этап 139: отдельный множитель роли в native export

WireAudioClip передаёт roleGain отдельно от авторского gain/ключей. Native AudioClip принимает camelCase roleGain с default1 для старых документов; domain и compiler отвергают отрицательные, nonfinite и переполняющие gain×16 множители. FFmpeg применяет второй volume после авторской анимации до pan/fade/mix. Исходный предел gain клипа16 сохранён; роль+20dB даёт множитель10 и больше не упирается в этот предел. Передача чрезмерного программного gainDb отвергается frontend payload с явной ошибкой numeric range.

Реальный WAV тест обновлён: тихий исходный сигнал, авторский gain16, роли1,+6dB,-6dB,0,+20dB. Все5 рендеров PASS; stereo RMS .01053609/.02102223/.00528067/0/.10536087, т.е. ×10 при эффективном gain160. Frontend полный набор686 tests/108 files PASS; typecheck/lint PASS. Native composition108 PASS перед добавлением отдельного regression теста. Совместимость default1, отклонение authored gain17 и невалидных roleGain проверяются новым native тестом; первоначальное ожидание формата числа исправлено с16 на16.000000. Проверка не утверждает полного UI→WAV сравнения, meters или завершения остальных возможностей.

Stage139 final verification: новый native regression PASS; cargo build PASS (имеющиеся предупреждения вне этой задачи). diffcheck PASS. Исполняемый backend обновлён под новый roleGain wire. Full browser editor suite после этой смены wire не запускался; реальный native WAV и полный frontend покрывают отдельные участки пути.


## Этап 140: измерители RMS по аудиоролям

Источник MixerPanel показывает роль RMS после gain/gating, со шкалой−60..0dBFS и −∞ для тишины. Preview теперь направляет AudioClip в общий AnalyserNode роли после индивидуального GainNode: несколько клипов суммируются как сигнал, а не арифметическая сумма RMS. Inspector показывает компактный meter и dBFS четырёх ролей, читает уровни при воспроизведении и останавливает RAF при паузе/unmount. Авторский документ и история не изменяются измерением.

Роль можно сменить без повторного MediaElementSource: GainNode перенаправляется, пустой старый bus освобождается, общий bus остаётся до последнего владельца. Unit проверяет общий bus,−6.0206dB при samples0.5, перенаправление, отсутствие повторного source и cleanup. Focused54 tests/4 files PASS; typecheck0 errors/warnings, lint/diffcheck PASS.

Real Chromium production helper проверяет показания Music/SFX против независимого выходного AnalyserNode (допуск0.1dB), суммарный сигнал двух media elements одной роли, смену роли, mute/solo и освобождение. PASS: +6dB signalRMS~.0282, roleRMS~−31dBFS; gated/released/disposed role meters silence. Нет master/stereo meter/peak hold UI, smoothing/bus gain audition и полной audioDSP parity. Embedded video audio пока не классифицировано по ролям. Остальной общий inventory остаётся открытым.

Stage140 full MCP→Chromium editor suite PASS against rebuilt stage139 backend. Fresh /tmp/video-kadr-role-mixer-qa.png viewed: four role controls and RMS meters visible, silence−∞; role controls/Undo/checkpoint scenario passed. This verifies UI presence/history and native wire compatibility exercised by suite, while audible role meter measurements are the separate real Chromium helper test above.

## Этап 141: общий измеритель выхода

Все WebAudio media gain graphs теперь проходят через один общий AnalyserNode контекста перед destination: роль bus и embedded video без роли суммируются до master tap. Inspector показывает общий RMS meter и числовой peak dBFS; остановка транспорта очищает показания. Master создаётся один раз на контекст и удаляется после последнего media controller; смена роли не пересоздаёт master/source. Старые mocked contexts без createAnalyser сохраняют прямой gain route.

Source AudioGraph.ts master meterSnapshot и roleMeterSnapshot используют readMeterTap с RMS/peak float time-domain buffer. Здесь повторена эта схема (mono analyser downmix, не независимые L/R каналы). Реальный Chromium helper PASS: master RMS−30.9978/peak−27.9839dBFS совпадают с role/output сигналом; teardown last source→−∞. Проверка shared bus расширена общей шиной и освобождением master. Остальные master mute/compressor/peak-hold/smoothing/UI audition ещё не реализованы; полная parity не достигнута.

Stage141 final checks: focused54 tests/4 files PASS; typecheck0 errors/warnings, lint PASS, diffcheck PASS. Full browser UI suite не повторялся после master meter: предыдущий stage140 suite и нынешний real Chromium graph/mounted Inspector покрывают разные участки. Сборка native не менялась.


## Этап 142: master mute предпросмотра

В AudioGraph.ts источник setMasterMute меняет input gain до master meter и compressor. Локальная общая шина получила input GainNode перед master tap, Inspector — checkbox «Отключить звук предпросмотра». Preference хранится отдельно в previewAudioState, не записывается в Composition/экспорт/Undo; включает embedded audio и все role buses. Role taps upstream остаются активными, master downstream читает тишину. Preference учитывается при создании первой/новой шины после отсутствия media.

Real Chromium production graph PASS: master mute outputRMS0, master meter−∞, role meter finite; unmute возвращает исходную амплитуду .02822765. Inspector37 tests PASS, включая неизменность документа/глубины истории после toggles; прежний focused39 PASS. Typecheck0 errors/warnings, lint/diffcheck PASS. Native export pipeline не изменён. Full editor browser suite после master controls не повторялся; compressor, peak hold, audition и прочая parity остаются открыты.

## Этап 143: защита предпросмотра от перегрузки

Перенесён master DynamicsCompressorNode AudioGraph.ts: threshold−1dB, ratio20, attack.001s, release.25s, knee0. Схема input→master analyser→compressor→destination сохраняет показания перегрузки до защиты. Последний media controller освобождает compressor вместе с master. Native export alimiter остаётся отдельным контрактом; WebAudio protection — source preview approximation, не sample-equivalence экспорту.

Unit проверяет параметры, порядок graph и idempotent cleanup; audio graph/media mounted5 tests PASS. Typecheck0 errors/warnings, lint/diffcheck PASS. Real Chromium PASS: input RMS9.027dBFS/peak12.050dBFS при gain400, output RMS.7309 (input RMS~2.827). Unity/boost4 ratios, roles, reroute, mixed2sources, master mute and cleanup remain qualified. WebAudio compressor gives small makeup amplification even below threshold: role preRMS.02823 versus output.03015. Harness now independently taps pre-compressor master and compares meters there, output tests remain post-compressor. Both media starts warm up600ms to avoid unequal analyser windows during startup; initial assertions failed on transient/meter-before-versus-after assumptions, fixed routing evidence rather than relaxing tolerances.

Remaining peak-hold/smoothing/audition and all other inventory still open. Полная parity не достигнута.

## Этап 144: максимум пика прохода и ручной сброс

Source masterMeterStore.peakHoldDb — бесконечный максимум прохода, сохраняемый при остановке транспорта, resetMasterPeakHold очищает число, clearMasterMeter при disposal preview его удаляет. Локально master samples увеличивают WeakMap hold контекста; hold не исчезает при временном отсутствии media/пересоздании отдельных bus. Preview unmount вызывает reset; Inspector показывает hold отдельной кнопкой с ручным сбросом, не изменяет документ/Undo/экспорт. После reset следующий реальный sample становится новым максимумом.

Unit проверяет .5→.1→0, сохранение−6.0206dB при silence, reset и новый максимум−20dB, сохранение через media disposal gap и explicit clear. Focused57 tests/3 files PASS; typecheck0 errors/warnings, lint/diffcheck PASS. Real Chromium PASS: overload hold12.0499428dBFS, silent sample оставляет12.0499428, explicit reset→−∞. Full editor screenshot/physical click нового peak reset после этого этапа не квалифицированы. Source useMasterMeterDisplay дополнительно имеет RMS300ms hold, peak1000ms marker hold и release envelopes — это остаётся открытым вместе с остальной parity.

## Этап 145: огибающие отображения master meter

Source useMasterMeterDisplay: RMS hold300ms/release24dB/s, signal level без hold/release36dB/s, marker hold1000ms/release18dB/s, silence cancels hold/release240dB/s and visual floor−60→silence−120. Добавлен чистый meterDisplay controller с этими правилами. Inspector показывает peak signal bar и удерживаемый marker, RMS доступен в tooltip; число peak pass maximum остаётся отдельным raw hold. После pause display продолжает спад, RAF прекращается при settled и отменяется при unmount. Raw analyser samples/export не затронуты.

Pure regressions cover hold boundaries/release/silent target/settled/marker reset. Inspector37 tests PASS. Initial integration read a newly written reactive masterLevel inside effect, causing repeat scheduling; paused sample changed to constant silence, repeated mounted suite recovered. Initial pure expected level−13.8 at300ms was corrected to−10.2 because lower target arrived at100ms: release begins with that new target. Source clock publication50ms отличается от local RAF refresh; dB/time formulas совпадают, независимого полного runtime envelope comparison ещё нет. Full parity remains open.

Stage145 final: pure3 tests PASS, mounted Inspector37 PASS, typecheck0 errors/warnings, lint/diffcheck PASS. Full MCP→Chromium editor suite PASS including physical preview master mute toggles/peak reset click and exact unchanged document. Fresh mixer screenshot viewed with master bar/marker/reset above four role meters. Browser silence UI/history qualified; dynamic audio/envelope visuals still supported by pure timed tests and earlier real analyser harness, not frame-by-frame browser envelope oracle.

## Этап 146: равномощная панорама предпросмотра

WeftCut panGraph.ts implements the StereoPannerNode 2×2 equal-power matrix with shared native pan_coeffs, channel-sensitive mono/stereo routing and 10ms coefficient interpolation. Preview MediaGain now routes gain→StereoPannerNode→role/master bus and updates pan without rebuilding MediaElementSource. CompositionMediaLayer accepts pan; CompositionPreview samples authored pan (constant/keyframed) for independent audio and embedded video. Unsupported nonzero pan reports media error instead of silently presenting matching behavior. Cleanup and role reroute include pan output.

Native video-kadr export still uses linear stereo balance, unlike this source law; CompositionSource/wire currently lack audio channel count even though richer StreamMetadata has it. Inspector accurately warns that export channel distribution differs. Native channel-aware pan law and 10ms coefficient interpolation remain required; no preview/export pan parity claim.

Focused Preview/MediaLayer/graph21 PASS before new pan unit; graph6 PASS including source→gain→pan routing, updates−1/.5, no source recreation, cleanup. Real Chromium production helper PASS: mono−1 RMS[.02828,0], +1[~0,.02831], .5[.01084,.02616], ratio tan(3π/8), combined power retained. Stereo different-frequency L/R: center[.02831,.02831], left[.04007,0], right[~0,.04007], demonstrates channel summation rather than dropping opposite input. Initial independent test taps were detached by role reroute; observer attached after reroute, production routing unchanged. Master compressor/mute/hold tests also PASS with pan active. Typecheck0 errors/warnings, lint/diffcheck PASS. Earlier typecheck/lint handles lost after interruption; re-run confirmed current checks. Whole-product inventory remains incomplete.

## Этап 147: число каналов для правильной export pan law

CompositionSource получила additive optional audioChannels/audio_channels. Legacy absent value остаётся None и не сериализуется; указанные значения1..64 допускаются только при hasAudio. Frontend source wire сохраняет channel metadata, validator отклоняет ноль/дробные/слишком большие значения. Native trusted source resolution берёт channels первого audio stream из реального ProbeResult, затем VerifiedCompositionRenderRequest заменяет active source metadata probe данными; выбор mono/stereo не должен доверять клиентскому полю.

Обновлены54 source constructors с legacy None, включая интеграционные fixtures. Native source regression проверяет legacy JSON, valid1/2/6/64, invalid0/65 и hasAudiofalse. Native composition110 tests PASS; frontend payload4 PASS; typecheck0 errors/warnings, lint/diffcheck PASS; all-target test compilation PASS (тесты интеграций только скомпилированы, не выполнены). Initial edit placed validator in stable-id macro; исправлено в scoped CompositionSource.validate и native suite восстановлен. Initial frontend assertion used wrong validator return shape; corrected to inspect issue list. Source validation was also narrowed for unknown input types after typecheck.

Это подготовка канально-зависимой панорамы экспорта, не завершение самой pan law. Compiler пока использует прежний linear balance; unknown channel count/mono downmix/10ms coefficient scheduling и полный общий inventory остаются открытыми.

## Этап 148: постоянная равномощная панорама native export

Compiler передаёт source.audio_channels в CompiledAudioAutomation для primary embedded и independent audio. Constant pan при известном числе каналов теперь применяет WeftCut pan_coeffs: mono [cos((p+1)π/4),sin((p+1)π/4)], stereo cross-feed matrix (p≤0: [1,cos((p+1)π/2),0,sin((p+1)π/2)]; p>0: [cos(pπ/2),0,sin(pπ/2),1]). Trig zeros snapped at1e-7. Mono сначала явно копируется в L/R до aformat, чтобы автоматическое−3dB FFmpeg upmix не применило law второй раз. Это также добавлено до speed-ramp segment aformat.

Raw old/unknown channel metadata None сохраняет legacy compiler path. Trusted export обычно получает real channel count в этапе147. Animated pan пока остаётся linear balance: source10ms coefficient interpolation ещё не перенесено. Multichannel downmix ещё не отдельно квалифицирован. Inspector warning narrowed to animated pan.

Реальный FFmpeg production media_audio_filter output test:8 renders mono/stereo ×[-1,0,.5,1], проверяет12000 stereo f32 frames и RMS каждого канала по независимым числовым ожиданиям с допуском1e-6. PASS: mono0 [.005,.005], mono.5 [.002706,.006533]; stereo0 [.007071,.007071], stereo.5 [.005,.008660], stereo extremes sum both input channels to one output. Native composition110 PASS before two new tests; actual waveform test PASS, cargo build PASS. Full UI→export audio comparison/animated scheduling и общий inventory остаются открыты.

## Этап 149: анимированная export pan и сетка коэффициентов10ms

При известном числе каналов animated pan теперь вычисляет pan в соседних точках floor(t×100)/100 и+10ms, преобразует каждое значение в равномощную матрицу и интерполирует коэффициенты, как WeftCut panCoeffsAt. FFmpeg aeval применяет matrix к обоим каналам; st/ld удерживает значения pan/долю внутри channel expression. Mono явно upmixed копированием до aformat также для animation. Unknown channel count пока сохраняет legacy balance. Native keyframe curves доступны через проверенный compiler; копии выражений сетки не увеличивают authored key budget.

Первый вариант пытался передать произвольное выражение как variable и был отвергнут adapter InvalidVariable. Вместо ослабления проверки добавлена token-boundary замена только отдельной t в валидированном выражении; lt/pow/etc не меняются. Composition113 tests PASS после исправления, включая старые golden/budget тесты,8 static FFmpeg renders и2 animated FFmpeg renders. Animated проверяет каждый из12000 stereo frames против10ms coefficient oracle с допуском1e-7, mono/stereo, переход−1→1 и held tail; key count остаётся2.

Compiler revision добавлена в CompositionPlan fingerprint отдельно от wire schema1, чтобы cached linear-pan export не переживал новую семантику. Новый regression сравнивает с предыдущим cache key и сохраняет schema1. Preview сейчас обновляет StereoPanner по frame-sampled pan, а не10ms coefficient curves, поэтому warning об animated preview/export отличии остаётся корректным. Source native f32 coefficient precision/extended curves/multichannel/resampling/whole UI→file pan comparison и общий inventory ещё открыты.

## Этап 150: live audition регулятора громкости роли

WeftCut MixerPanel audition меняет renderer-local override и draft readout, не project history; commitDrag записывает одно действие, Escape clear override плюс swallow release. Локальный previewAudioState теперь содержит временные roleGains, previewAudioRoleGain применяет их только в CompositionPreview при сохранении mute/solo gating. Payload использует прежний audioRoleGain и не читает override. Inspector добавляет range−30..20 step.5 рядом с role meter; readout отражает draft. Release clears override и commits role gain once. Escape/pointercancel clear draft и подавляют release; unmount удаляет временные overrides.

Mounted Inspector38 PASS: input gain6→preview multiplier10^(6/20), authored gain remains1, document/history unchanged, muted-role override still0; release creates one Undo, Undo exact document; Escape release ignored, unmount cleanup. Preview15 tests also PASS (combined53). Typecheck0 errors/warnings, lint/diffcheck PASS. Initial lint flagged mutable Set; changed to SvelteSet. Full physical editor fader test added and running. Real waveform role audition not yet directly measured through full editorUI; earlier production WebAudio role/gain tests cover gain controller independently. Animated pan preview coefficient scheduling and remaining global inventory are still open.

Stage150 final: full MCP→Chromium editor suite PASS. Physical Music fader mouse drag verified unchanged get_project during gesture and draft spinbox value; mouse release committed, one Undo restored exact document; Escape with mouse held then release left exact prior document. Fresh mixer screenshot viewed with four faders/readouts/meters. Master mute/reset and previous timeline/history scenarios also PASS against current native backend. diffcheck PASS.

## Этап 151: unity reset роли и общий commit audition

Source RoleResetButton calls gain.commitGain(NEUTRAL_GAIN_DB=0), sharing commit path with fader/readout. Inspector теперь имеет accessible reset каждой роли (↺/«Сбросить громкость роли …»). commitRoleMixerGain очищает temporary audition и cancelled guard, clamps UI range и записывает роль одним действием. Numeric readout теперь использует тот же commit, закрывая найденный дефект: typed gain после draft больше не оставляет старый override звучать поверх нового authored gain. Mute/solo не сбрасываются.

Inspector39 tests PASS: draft6→numeric−3 clears override, one history entry; mute+solo flags then reset→gain0 flags preserved, second history entry, Undo→−3 same flags. Legacy fader/Escape/undo tests remain PASS. diffcheck PASS; full browser suite после этого нового reset не повторялся, прежний stage150 physical fader suite и current mounted reset cover different scopes. Global inventory remains incomplete.

Stage151 final: typecheck0 errors/warnings, lint PASS. Initial test helper constraint accepted only input/select; reset button lookup changed to typed querySelector, typecheck recovered. No production behavior change from that test-only repair.

## Этап 152: причина тишины от Solo другой роли

Source MixerPanel silencedBySolo = !mix.muted && !roleAudible(...), displayed ImpliedMuteBadge; explicit Mute wins and does not claim implied mute. Добавлен audioRoleSilencedBySolo, общий predicate для audioRoleGain и label Inspector «Solo другой роли». Controls остаются доступными; label не меняет flags/document/history. Mute suppresses implied badge, own solo suppresses it, releasing last solo clears other role labels.

Mounted Inspector40 + audioRoles4=44 tests PASS: Music solo implies Dialogue/SFX, Music itself not implied; Dialogue explicit mute removes its badge, releasing Music solo clears SFX, flag actions keep history depth. Typecheck/lint/diffcheck requested; physical browser/visual snapshot новых labels отдельно не повторялись. Remaining preview10ms pan scheduling, broader audio semantics and global inventory still open.

## Этап 153: пересечение fade-in/fade-out

Source eval::fade_multiplier перемножает ramp factors, в том числе overlap. Локальный preview audioVolume использовал min(in,out), поэтому при обоих fades.8×duration в midpoint выдавал.625 вместо.390625. Добавлен audioFadeMultiplier и подключён к production CompositionPreview. Каждая рампа ограничена0..1, absent fade=1; signed handles вне faded span дают0. Crossfade и role/clip gain остаются отдельными множителями.

Pure fade2 + Preview16=18 tests PASS. Mounted actual HTMLMediaElement fallback volume at4s of8s clip with6.4s fades equals.390625; document/history unchanged. Native export уже использовал последовательные afade, т.е. product: real production compiler animated pan test расширен mono/stereo ×without/with overlapping.2s fades on.25s span. Four real FFmpeg renders compare every12000 stereo f32 frames against coefficient-grid×fade product oracle tolerance1e-7, PASS. Typecheck0 errors/warnings, lint/diffcheck PASS. Native production unchanged (test-only extension); full editor audible fade RMS/clock equivalence not claimed, broader inventory still open.

## Этап 154: ограничение панорамы на последней точке сетки

Source pan_coeffs ограничивает pan диапазоном −1…1 перед equal-power matrix. Continue extrapolation может оставаться допустимой на всём клипе, но выходить за диапазон на следующей точке сетки10мс за концом клипа. Native compiler теперь ограничивает обе точки перед вычислением и интерполяцией коэффициентов. Compiler revision повышена до weftcut-equal-power-pan-grid-2 для исключения старого render cache.

Real FFmpeg regression сначала FAIL на последнем интервале mono, затем PASS после исправления. Шесть renders: mono/stereo без fades, с overlapping fades и с Continue на clip233мс/keyframe233мс; boundary renders сравнивают все11184 stereo f32 frames с independently clamped coefficient oracle, tolerance1e-7. Native composition suite114 PASS. Animated preview scheduling и весь оставшийся inventory ещё не завершены.

## Этап 155: клавиатурное подтверждение и отмена gain readout

Source MixerPanel GainReadout buffers numeric edits, commits Enter/blur and discards Escape locally without abandoning a fader gesture. Local role numeric inputs now restore the current displayed gain on Escape and suppress its following change; Enter commits and suppresses the following change, preventing duplicate history. A fresh input clears the guard. Empty/nonfinite numeric input does not author a zero gain. Existing fader audition remains independent.

Mounted Inspector41 tests PASS: Escape9→0 keeps exact document/history; Enter−4.5 plus trailing change records one Undo; empty input preserves gain/history; Undo restores exact document. Typecheck0 errors/warnings. Physical Chromium keyboard qualification not run in this stage; wider feature inventory remains incomplete.

## Этап 156: физическая клавиатура readout и общий regression

Full real MCP→Chromium editor suite PASS against current native executable. Added physical role readout fill9/Escape: exact project unchanged, undo depth unchanged, displayed gain restored; fill−4.5/Enter: authored gain updated, one Undo only despite native blur/change, Undo restores exact project. Existing fader gesture/escape, master controls and timeline/history scenarios remain PASS. Full frontend703 tests across110 files PASS, node syntax/diffcheck PASS. Corrected top inventory to distinguish implemented role mixer/extrapolation from absent arbitrary Bezier/tangents/Elastic/Bounce and animated coefficient-grid preview/master export parity. Goal remains incomplete.

## Этап 157: процедурные Bounce In/Out/InOut

Перенесены точные четыре параболы Penner из pinned native/eval Bounce (G=7.5625, D=2.75), отражение In и piecewise InOut. Добавлены additive wire enum bounce_in/bounce_out/bounce_in_out в TS/Rust, validation, track-default и outgoing per-key interpolation. Общий список вариантов автоматически подключает Inspector и timeline context picker. Preview sampler и FFmpeg adapter реализуют одну семантику; Continue у Bounce имеет нулевой slope, как в source, даже когда математическая производная endpoint не нулевая. Старые режимы не менялись.

Pinned numeric preview17 tests PASS, native real FFmpeg qualification сравнивает1001 f64 samples каждого направления с Rust sampler (tolerance1e-12), wire outgoing override roundtrip и flat Continue tails PASS. Mounted linked Scale authoring проверяет все три options, атомарные X/Y, один Undo и exact document restoration; render payload сохраняет Bounce для обеих осей. Combined focused frontend100 tests PASS до добавления последнего wire assertion, повторный Inspector41 PASS после него. Native composition114 PASS, cargo build PASS, lint/diffcheck PASS. Full composition video pixel render и physical Chromium Bounce picker отдельно не квалифицированы; Elastic/tangents/Bezier и остальные inventory остаются открытыми.

## Этап 158: полный Bounce render и физическое меню

Added real_bounce_opacity_matches_full_composition_export integration test: actual compiler/runtime renders three H264 MP4s with white source and opacity0→1 using Bounce Out/In/InOut between1–2s. Independent pinned oracle checks21 RGB frames, including hold before/after and parabola interiors, tolerance7 for encoding. All renders Done::Completed, duration5s±120ms; test PASS without skip, isolated tempfile cleanup.

Physical MCP→Chromium suite extended to select every Bounce direction in timeline context, assert outgoing per-key wire field, one history entry and exact Undo; keyboard wrap/Home/End expected final menu item updated to Bounce InOut. Full MCP→Chromium suite PASS against current native executable, including all three physical Bounce choices and keyboard End→Bounce InOut with exact Undo. Full frontend704 tests/110 files PASS; syntax/diffcheck PASS. Elastic and arbitrary Bezier/tangents remain open.

## Этап 159: параметризованные native Elastic primitives

Добавлен domain/elastic.rs: направления In/Out/InOut, произвольные amplitude≥1/period>0 с finite validation, точные source Penner formulas и FFmpeg expressions. In реализован собственной формулой, не отражением Out; residue строго внутри segment сохраняется, exact-key boundary должен задавать segment owner. Nine real FFmpeg traces (three directions ×(1,.3)/(2,.17)/(1.5,.6)),1000 f64 samples each, match native to1e-12. Pinned source numeric samples проверяют default parameters и divergence при amplitude2; invalid parameters rejected. Initial divergence fixture picked a crossing where the two formulas accidentally coincide, changed to midpoint.

Это foundation, ещё НЕ пользовательская Elastic-функция: wire model, preview, parameter UI, analytic overshoot bounds (включая cyclic Offset) и интеграция full composition compiler остаются обязательны. Elastic не отмечен перенесённым в верхнем inventory.

## Этап 160: analytic Elastic overshoot bounds

Native Elastic::bounds вычисляет extrema на произвольном нормализованном интервале0..1. Для exp(k*t)*sin(theta) stationary roots решаются через atan2(−omega,k); первые/последние две roots достаточны, поскольку знаки чередуются, а amplitude envelope монотонна. InOut разбивается на две ветви; exact key endpoint values и Penner one-sided residues включены отдельно. Работа constant по числу колебаний, без scan или arbitrary cap.

Three native Elastic tests PASS, включая прежние nine real FFmpeg traces. Analytic bounds проверены containment tolerance1e-12 и tightness1e-4 против100001 samples для60 комбинаций direction/amplitude/period/partial interval; fast period1e-9 подтверждает отрицательный и положительный overshoot без enumeration. Initial tightness fixture не учитывал exact key endpoint; исправлен oracle. Затем10001-point grid недооценивал пик короткого периода более1e-4; sampling oracle повышен до100001, аналитический алгоритм не менялся. Wire/keyframe integration, cyclic Offset extrema, preview и UI всё ещё открыты; это подготовка общего evaluator, не завершённая пользовательская Elastic-функция.

## Этап 161: native parameterized Elastic wire и keyframe integration

Interpolation сохранил legacy string tokens и получил untagged ProceduralInterpolation object {kind:elastic,dir:in/out/in_out,amplitude,period}. Track defaults и outgoing overrides используют один тип. Native constructor проверяет amplitude/period через Elastic::new; endpoint Continue slope0. Sample_tick и FFmpeg adapter теперь вызывают параметризованный evaluator; exact left/right keys обходят Penner residue. Value bounds включают interior Elastic extrema, а Offset обрабатывает пересечение segment с first/last edge cycles и ближайшими full cycles, сохраняя constant cost по числу циклов.

Native wire roundtrip, unchanged linear string, invalid default parameters, exact boundaries PASS. Three real FFmpeg keyframe exports1001 f64 samples each match native1e-12. Offset containment checked over negative, ordinary and far positive intervals including1000.2–1001.7s. Existing keyframes17 and composition114 tests PASS. All-target compilation requested, diffcheck PASS. Frontend wire validator/sampler, parameter UI и полный Elastic video render ещё не подключены/не квалифицированы; полный перенос остаётся незавершённым.

## Этап 162: frontend Elastic model, sampler и параметры Inspector

TS CompositionInterpolation теперь additive union legacy string / {kind:elastic,dir,amplitude,period}. Validator accepts only exact supported object fields and finite amplitude≥1/period>0; normalizer rejects invalid objects. Preview evaluates exact source formulas and retains exact authored left/right keys. elasticBounds mirrors native stationary-root algorithm; animatableExtents now includes interior overshoots for canvas/visual range calculations. Inspector offers three Elastic directions and editable amplitude/period (defaults1/.3); switching directions preserves existing parameters. Linked Scale structural comparison now compares interpolation fields, correcting reference equality that initially unlinked copied X/Y object curves.

Focused Elastic/Inspector/ScaleLink46 tests PASS; linked inspector changes mode/amplitude/period, invalid period0 leaves exact document, three Undo restore prior document. Additional wire assertion preserves exact object and parameters through render payload, Inspector41 PASS. Typecheck0/lint/diffcheck PASS. Full frontend706 tests/111 files PASS. Timeline per-key context parameter UI, cyclic Offset frontend admission extrema, full Elastic video render and physical editor qualification remain open. Global feature parity not achieved.

## Этап 163: Elastic для outgoing selected keys в timeline context

Контекстное меню добавило Elastic In/Out/InOut и поля amplitude/period. Для нового или multi-key выбора поля задают draft до выбора направления; existing single Elastic показывает авторские параметры и checkmark. Hover/focus публикует linked previews без authored mutation; выбор направления атомарно записывает outgoing segments одним Undo. Parameter input previews existing Elastic, change/Enter commits without closing menu, identical parameters skip duplicate commit; Escape closes and clears previews, trailing change after close ignored. Tab from final Elastic item enters numeric parameters; ordinary menu Tab keeps existing dismissal behavior.

Timeline43 mounted tests PASS: draft amplitude2→hover leaves exact document, choose Out writes one Undo; reopen checkmark/values, period.17 preview unchanged authored doc, Escape+trailing change ignored; reopen commit period.17 adds one Undo, two Undo restore exact prior document. Typecheck0, lint and syntax/diffcheck PASS before last identical-value guard; repeated focused tests PASS after it. Browser script final-item expectations updated for new menu but physical Elastic scenario not run in this stage. Remaining full video render, physical qualification, mixed-selection param editing parity and wider inventory still open.

## Этап 164: full parameterized Elastic composition render

Native real_parameterized_elastic_opacity_matches_full_composition_export PASS: three actual H264 MP4 renders, directions In/Out/InOut, amplitude2/period.17, opacity.45→.55 on keys1–2s. Twenty-one sampled RGB frames match independent Python-math pinned values, tolerance7 for encoded YUV→RGB; key endpoints and outside Hold intervals included, duration5s±120ms, Done::Completed required. Bounds permit this authored range while retaining overshoot. Actual source/output files isolated in tempfile and cleaned. cargo build PASS.

Full MCP→Chromium scenario extended with all three physical Elastic choices, outgoing object wire, one Undo per choice, parameter audition Escape rollback, committed amplitude2/period.17 and three Undo exact restoration; PASS against newly built backend. Physical choices, parameter Escape, amplitude/period commit and exact Undo all confirmed by terminal success. Wider feature inventory and remaining Elastic admission/mixed-selection details remain open.

## Этап 165: arbitrary native timing Bézier foundation

Добавлен domain/bezier.rs TimingBezier: resolved p1/p2 с x∈0..1 и finite y, WeftCut UnitBezier Newton≤8 plus bisection fallback (EPS1e-7), exact endpoints, full-segment Y stationary extrema, Continue leaving/arrival tangent rules including zero-length opposite-handle/diagonal fallback and vertical slope0. Derivative coefficients normalized before quadratic discriminant to avoid overflow on finite large controls. No fixed preset reduction.

Two native tests PASS:1001 exact quadratic samples, pinned CSS ease midpoint, inverse residual1e-7 on degenerate/nonlinear X curves; overshoot containment over10001 parametric samples for three shapes; large y±1e200 extrema; tangent fallbacks and invalid parameter refusal. rustfmt/diffcheck PASS. Foundation only: wire/segment integration, FFmpeg inversion, TS preview and tangents/continuity authoring UI still required. Bézier not marked delivered in top inventory.

## Этап 166: arbitrary Bézier FFmpeg inversion primitive

TimingBezier::ffmpeg_expression теперь выполняет source-compatible Newton≤8 (EPS1e-7/derivative threshold1e-6) и fallback bisection в FFmpeg eval, без baking/fixed preset approximation. Bisection bounded64 steps covers f64 interval resolution; endpoints exact0/1. Registers3..9 reset per call, leaving pan grid0..2 untouched. Native Newton exhausted8 iterations always falls back even if last update crossed EPS; expression retains that detail.

Real FFmpeg aevalsrc test PASS: five curves (CSS ease, ease-in, flat/degenerate X, reversed-order controls and Y overshoot),1001 f64 samples each, native match tolerance1e-12 including exact endpoints. Three native Bézier tests pass as a suite; diffcheck PASS. This remains compiler primitive: wire/keyframe integration, partial/cyclic bounds and frontend/editor tangents are still required before claiming delivered Bézier capability.

## Этап 167: native Bézier keyframe wire, Continue и partial Offset bounds

ProceduralInterpolation added {kind:bezier,p1:[x,y],p2:[x,y]}, preserving legacy strings and Elastic. Shared procedural valid/sample/expression/bounds dispatch connects native keyframe default and outgoing overrides to TimingBezier. Continue uses resolved endpoint tangent slopes with fallback rules. Partial Bézier bounds invert interval endpoints with64-step bisection and widen normalized X by source solver EPS1e-7, including evaluation tolerance as well as interior Y roots; exact zero-length key boundaries retain authored value. Offset uses same intersected first/last segment cycles as Elastic.

Native Bézier roundtrip and invalid x refusal PASS; real FFmpeg keyframe expression1001 f64 samples matches native1e-12; Continue before/after pinned4.2/−3.6; dense Offset containment on negative/ordinary/far1000s intervals PASS. Four Bézier primitive tests PASS including partial interval containment for15 shape/interval cases; native composition114 PASS, diffcheck PASS. Frontend object handling, control/tangent UI and full video composition render are still missing for Bézier, so feature remains incomplete.

## Этап 168: frontend arbitrary Bézier и Inspector control points

CompositionInterpolation additive {kind:bezier,p1,p2}; validator accepts exactly finite coordinate pairs, X0..1, finite power coefficients. Normalizer handles both procedural families. Native-compatible TS Newton/bisection sampler, exact keys, interior derivative-root bounds и Continue endpoint fallback slopes подключены к shared animation evaluation. Scale twin comparison теперь различает control point coordinates, а не только Elastic fields. Existing Elastic UI explicitly narrows kind, preventing Bezier fields being treated as amplitude/dir.

Inspector added Bézier picker and four numeric coordinates, unrestricted finite Y overshoot, X0..1. Invalid values rejected before document writes. Focused86 tests PASS: quadratic/CSS, overshoot bounds, Continue values4.2/−3.6, exact keys, structural twin identity/divergence; mounted linked Scale defaults→p1.Y−2 updates both axes, wire exact, invalid p1.X−.1 preserves document, two Undo exact restore. Typecheck0/lint/diffcheck PASS (initial lint missing each keys fixed). Full frontend709 tests/112 files PASS. Per-key Bézier context editing, graphical tangents/continuity, full real video and physical UI qualification remain open; top inventory still not claimed complete.

## Этап 169: per-key Bézier context controls

Timeline context offers Bézier as outgoing interpolation, preserving existing authored control coordinates on reopen. Hover/focus publishes shared linked previews; selecting applies atomically through existing selected-key API. Existing single Bézier exposes four coordinates; input previews cloned draft pairs (no alias of authored arrays), change/Enter commits, same coordinates skip duplicate Enter/change history; Escape restores and trailing change ignored. Invalid X blocks commit without partial writes. Context menu now has viewport-dependent max-height and overflow scrolling so extra controls remain reachable; Bézier item inserted before Elastic preserving final keyboard menu item.

Mounted Timeline44 PASS: preview preserves exact document/history; choose Bézier adds one Undo; reopen checkmark, p1.Y−2 draft/Escape/trailing change leaves authored doc; Enter/change commits once, invalid p1.X−.1 leaves exact doc, two Undo restore initial document. Typecheck0/lint/diffcheck PASS; initial null narrowing compile errors fixed with optional discriminant guard. Graphical tangent drag/continuity, mixed-selection parameter editing and physical Bézier/video qualification still open. Full inventory incomplete.

## Этап 170: full arbitrary Bézier composition render

Native real_arbitrary_bezier_opacity_matches_full_composition_export PASS: three H264 MP4s using CSS ease controls, Y overshoot controls(.1,−2)/(.9,3), and degenerate X(0,0)/(0,1). Opacity.45→.55 between1–2s keeps complete curve inside legal range. Twenty-one RGB samples match independent Python Bernstein+100-step bisection oracle (baked values, no native sampler calls), tolerance7; Hold/key endpoints and duration5s±120ms included. FFmpeg while/Newton/bisection executes through full composition filter graph, Done::Completed required, files tempfile-cleaned. Initial generated numeric literals repaired before successful compilation.

Physical browser scenario added Bézier hover/Escape unchanged document, outgoing controls commit, p1.Y audition−2 Escape, commit.2 Enter+Tab one history entry, screenshot and two Undo exact restoration. Full real MCP→Chromium suite PASS; physical parameter audition/commit/Escape and exact Undo confirmed. Screenshot /tmp/video-kadr-bezier-menu-qa.png inspected: compact scrollable menu, selected Bézier item and four coordinate fields visible. Graphical tangent drag/continuity and remaining global inventory remain open.

## Этап 171: graphical Bézier control editor в Inspector

Added normalized timing curve with two focusable graphical handles and guide lines, fitting unrestricted Y controls while X is clamped0..1. Pointer drag freezes coordinate mapping at press; threshold3px prevents click-only commits. Draft publishes shared preview through linkedScaleAutomationPatches, updates Inspector curve and composition preview, authored document/history unchanged until release. Release uses existing track-default atomic interpolation commit, Escape/pointercancel/lost capture/unmount clears preview; document identity guard rejects concurrent document changes. Arrow keys step.01/Shift.1 and commit through same path. Locked clip/track handles disabled and removed from tab order.

Mounted Inspector41 PASS: real SVG event geometry mock moves p1(.25,−2)→(.5,−.5), two linked X/Y preview entries, unchanged doc/history; release one Undo with linked twins, clear previews, Undo exact. Escape+trailing release unchanged, click-only no-op, ArrowRight→.26 one Undo and restoration. Initial preview cleanup compared raw/proxied identities and accumulated4 instead of2; now retains actual published reactive entries, fixed. Initial helper typing optional values repaired. Typecheck0/lint/diffcheck PASS. Physical pointer/screenshot qualification not run yet; this edits track-default normalized controls, not full per-key on-timeline tangents/continuity modes. Those source requirements and remaining inventory stay open.

## Этап 172: стабильная геометрия drag и отмена при блокировке

Inspector Bézier now freezes the displayed Y range as well as pointer mapping for the entire gesture. Changing draft control extents no longer moves the handle away from the pointer. Locking the clip/track cancels an active gesture immediately, clears linked previews and releases pointer capture; trailing pointerup cannot author a curve. Mounted Inspector41 PASS includes handle cy=50 at pointer y=50 with changing Y extents, lock during draft, cleared previews, unchanged locked document/history after trailing release, and Undo restoration. Typecheck0/lint/diffcheck PASS. Physical graphical pointer qualification and full per-key tangent/continuity parity remain open.

## Этап 173: physical Bézier handle qualification

Added actual Chromium mouse drag/capture to the real MCP→browser scenario for linked Scale track-default Bézier controls. Both Escape+trailing pointerup and release tested: draft produces exactly two identical X/Y preview entries while get_project and history remain unchanged; cancellation clears both, release authors the captured draft to both axes in one history entry. Undo restores the exact timing document, then Undo of Bézier selection restores the preceding Scale document. Full test:e2e:agent PASS against existing native binary; node syntax and diff checks PASS. Screenshot /tmp/video-kadr-bezier-handles-qa.png inspected: two handles, guide lines and complete curve visible in compact Inspector graph.

Source requirement confirmed from shared/keyframe.ts and shared/tangents.ts: per-key in/out controls with Auto/Free and Smooth/Broken, monotone Auto solve and world-slope continuity across unequal adjacent spans are still absent locally. Current segment-control UI does not establish their parity. Other inventory remains incomplete.

## Этап 174: native write-time tangent solver

Added domain/tangents.rs adapted from pinned WeftCut MIT source: explicit Auto/Free sides, Smooth/Broken continuity, world-slope helpers, monotone Auto normalization, OUT WINS for two Free Smooth sides across unequal neighboring time/value spans. Non-Spline sides remain untouched, non-scalar Auto uses identity, zero-time/flat/vertical inverse cases preserve coordinates. Solver is pure and does not run during trim/split. Time differences use i128 to avoid signed i64 overflow.

Four focused native tests PASS: five upstream solve fixture cases (exact X/mode, solved Y tolerance1e-9 matching source golden harness), exact idempotence, 4004 dense Bézier samples without Auto overshoot, non-scalar identity and meaningless endpoint-side preservation, opposite-sign unequal-span world continuity, degenerate helpers, full signed time span and canonical positive zero. Initial exact-Y fixture assertion exposed source expected one-ULP differences; adjusted to the original golden harness tolerance, algorithm unchanged. diffcheck PASS. This is domain groundwork: solver not yet wired into project key records, authoring commands, preview/export or per-key handle UI. Full tangent feature and global inventory remain incomplete.

## Этап 175: native per-key tangent records and evaluation

Keyframe<T> now accepts optional tangents {incoming,outgoing,continuity}; omission serializes exactly as the previous key record. Tangent modes/continuity use snake_case, finite coordinates and X0..1 validated; resolved pair power coefficients also validated. Bézier segments use left outgoing/right incoming when present, with original p1/p2 defaults for omitted sides; Hold/Linear/procedural non-Bézier ignore tangent coordinates. Shared segment resolver feeds native sampling, Continue endpoint slopes, full/intersected Offset bounds and FFmpeg expressions. Explicit normalize_tangents authoring method resolves Auto/Smooth atomically while preserving omitted metadata and existing trim/split behavior. Existing Rust key literals receive tangents:None.

Native keyframes13 PASS, composition114 PASS; integration composition_render --no-run compiles. New test covers legacy omission, normalized endpoints/ramp, wire roundtrip, exact idempotence, finite bounds, invalid X rejection, and 2001 real FFmpeg f64 samples matching native within1e-12. diffcheck PASS. Production native supports resolved tangent records; frontend transport/validation/sampler and authoring commands/UI still need wiring. No claim of completed user-facing per-key tangent feature or all-feature parity.

## Этап 176: frontend resolved tangent transport and evaluation

Added optional typed key tangents matching native incoming/outgoing/continuity wire, strict finite coordinates/X0..1/mode/continuity and unknown-field checks. Project validation checks individual records and combined Bézier coefficient validity. Preview sampling, Continue endpoint slopes and curve extents use per-key sides for Bézier segments, original segment controls for omitted sides; other interpolation classes unchanged. Linked Scale twin comparison includes both coordinates, modes and continuity. cloneAnimatableValue clones tangent records and both sides independently; payload spread retains metadata.

Two new tests cover midpoint and flat Continue, interior bounds, independent clone, continuity divergence breaking twins, ignored controls on Hold, finite unrestricted Y and invalid fields/modes/X. Focused19 PASS; full frontend712 tests/113 files PASS, typecheck0/lint/diffcheck PASS. Auto/Smooth write normalization and per-key UI authoring still need wiring; frontend consumes already resolved numbers and does not evaluate Auto implicitly. Native binary not rebuilt this stage. All-feature parity remains incomplete.

## Этап 177: frontend Auto/Smooth write normalization

Added normalizeKeyframeTangents as write-time TS twin of native solver: monotone endpoint/extremum handling, segment-local Auto coordinates, OUT WINS world-slope Smooth Free continuity, untouched non-Bézier sides and omitted metadata, finite resolved-record refusal. Connected key upsert, individual update and delete to re-solve after sorted final keys; sampling and trim/split do not invoke it.

Focused21 tests PASS, including all five upstream solver goldens (same native tolerance), idempotence and real upsert mutation to a local peak producing incomingY1/outgoingY0 without mutating input. Typecheck0/lint/diffcheck PASS. Other batch writes, interpolation-class changes, native authoring normalization endpoints and per-key modes/handle UI still require wiring. Remaining all-feature inventory stays incomplete.

## Этап 178: tangent normalization for interpolation and group gestures

Track-default and selected-key interpolation changes now re-solve tangent sides against the final segment classes. Group move and time-scale helpers normalize the final sorted/collision-resolved keys. Added class-switch test confirming only meaningful Auto sides change, and group move test with 0/.5/2s key spacing resolving middle incomingY5/6 and outgoingY.5 without input mutation. Focused28 PASS before final additional case; final selection7 PASS, typecheck0/lint/diffcheck PASS. Initial test attempted assigning a readonly property, repaired to immutable fixture construction. Native authoring endpoints, clipboard/batch paths and per-key handle UI remain open; global parity incomplete.

## Этап 179: tangent authoring operation and shape-preserving materialization

Added resolvedKeyframeTangents and updateKeyframeTangents for explicit selected ticks. Missing records materialize arriving controls from the preceding segment p2 and leaving controls from the current segment p1, with Free/Broken defaults; existing records copied independently. Operation validates all subjects and coordinates before returning normalized final keys, preserving input on stale/invalid edits.

Tangent6 tests PASS: initial metadata materialization preserves 201 sampled values exactly across two different Bézier segments, correct incoming/ outgoing provenance, stale multi-subject refusal and invalid coordinate refusal. Typecheck0/lint/diffcheck PASS. Operation not yet exposed through state/history or UI; graphical per-key handle and mode editing remain required. Global all-feature inventory remains incomplete.

## Этап 180: tangent authoring state/history integration

Added setCompositionAutomationTangents for explicit target/property/ticks with validated patch and optional linked Scale fanout. Uses existing resolveAutomation/writeAutomationEdit path, so lock admission, atomic linked commits and normal history semantics apply. State regression checks linked X/Y equality and retained scaleLinked, exactly one past entry, exact Undo restoration, stale key refusal and locked clip refusal without document/history changes. Selection8 tests PASS, typecheck0/lint/diffcheck PASS. Per-key controls are not yet exposed in UI; native batch authoring and remaining inventory remain unfinished.

## Этап 181: per-key tangent mode UI

Inspector key table now exposes compact disclosure per Bézier/tangent-bearing key with incoming/outgoing Auto/Free and Smooth/Broken selects. Materialized values read owning segments; changes use state tangent API with linked Scale fanout. Mounted Inspector41 PASS includes outgoing Auto and Smooth commits to linked twins, one history entry each and two exact Undo restores. Single-key meaningless outgoing side keeps original coordinates while mode changes, matching solver contract. Typecheck0/lint/diffcheck PASS (each-key lint repaired). Physical browser qualification, graphical per-key handles and native authoring endpoint normalization remain open; full parity incomplete.

## Этап 182: tangent coordinates and incoming Smooth edits

Added editKeyframeTangent and state/history wrapper, plus per-key incoming/outgoing X/Y numeric controls. Authored coordinate edits switch that side to Free. Incoming Smooth edits re-aim the other Free side using world slope before OUT WINS normalization, across unequal time/value spans; only adjacent Bézier segments participate, degenerate inverses preserve existing sides. Coordinates use existing atomic state validation/linked fanout.

Tangent7 + Inspector41 =48 PASS. New unequal-span test preserves incomingY.4, computes outgoingY1, verifies matching world slopes and invalid X refusal. Typecheck0/lint/diffcheck PASS; readonly patch assignment repaired via immutable construction. Numeric controls have not yet received dedicated mounted/physical browser interaction qualification. Graphical per-key handles and native authoring endpoints remain open; global parity incomplete.

## Этап 183: numeric tangent UI qualification and arriving-side visibility

Inspector shows per-key controls when either owning segment is Bézier, fixing hidden arriving side on a key whose outgoing segment is Linear/Hold. Mounted numeric regression checks Auto→Free on outgoing Y−1.5, linked twin update, one history entry, invalid X−.1 refusal and exact three-Undo restoration. New two-key mixed-segment test confirms incomingY.8 provenance, incoming edit one history entry and exact Undo. Inspector42 PASS; typecheck0/lint/diffcheck PASS (typecheck/lint before final test-only addition). Physical pointer/numeric browser qualification and graphical per-key tangent UI remain open; full parity incomplete.

## Этап 184: on-timeline tangent geometry and guides

Added keyframeTangentPoints world projection using shared resolved segment controls, actual track timeBase, composition start, zoom and value axis. Incoming coordinates remain unmirrored in storage and are projected from the preceding segment; lines anchor them to their owner key. Expanded timeline Bézier curves now display guide lines and small control circles, pointer-events:none pending gesture integration. Geometry12 tests PASS: exact outgoing/incoming time/value projection with nonzero clip start and differing time bases; linear segments produce no handles. Typecheck0/lint/diffcheck PASS. Guides not physically/visually qualified yet; interactive per-key handle gesture and continuity context menu remain open. All-feature parity incomplete.

## Этап 185: inverse pointer-to-tangent geometry

Added keyframeTangentAtPoint with frozen value range/zoom/height, owning segment selection, actual time base/start mapping, X clamp0..1 and unrestricted finite Y. Flat value segment preserves stored Y because value scale is not invertible; missing/degenerate/non-Bézier segments and invalid pointer/axis refuse. Geometry13 PASS includes forward→inverse roundtrip for rising/falling/flat segments, incoming endpoint refusal, time clamp and nonfinite pointer. Initial inverse incorrectly applied extra padding already encoded in range; corrected to match keyframeValueY's actual full-height axis. Typecheck0/lint/diffcheck PASS. Actual pointer capture/preview/commit integration remains next; all-feature parity incomplete.

## Этап 186: timeline tangent pointer gesture integration

New TimelineTangentHandles connects on-timeline controls to pointer capture, 3px activation, frozen press channel/geometry mapping, editKeyframeTangent Smooth rules, linked shared previews and state/history commit. Draft does not mutate authored document; Escape/cancel/lost capture/unmount clears owned reactive preview entries. Release rejects changed document and uses existing lock admission, one atomic linked transaction. Typecheck0/lint/diffcheck PASS. Existing TimelineKeyframes44 PASS; dedicated tangent drag regression and physical browser qualification still required. Current handle pointer controls are not keyboard-focusable, ancestor SVG remains aria-hidden; keyboard editing available through Inspector, full on-timeline keyboard parity unfinished. Display axis is not yet frozen with mapping, which needs resolving before final gesture qualification. Global parity incomplete.

## Этап 187: tangent drag regression and frozen display axis

Timeline tangent gesture now freezes parent channel display range as well as inverse mapping; curve, diamonds and handles share frozen axis through draft updates, cleared on finish. SVG width read from actual width attribute permits deterministic mounted geometry without missing jsdom SVGAnimatedLength API.

Timeline46 PASS includes two dedicated real-component pointer event cases: draft publishes one preview with unchanged document/history, handle cx/cy match moved pointer, release one entry then exact Undo, Escape+trailing pointerup exact no-op and cleared preview. Typecheck0/lint/diffcheck PASS. Physical Chromium qualification, linked tangent drag, lock/concurrent document cancellation regressions and on-timeline keyboard/continuity menu remain open. Global all-feature parity incomplete.

## Этап 188: keyboard-accessible timeline tangent handles

Expanded curve SVG now exposes an accessible group and handle buttons. Unlocked handles enter Tab order, locked handles tabindex−1/aria-disabled. Arrow keys author segment-unit coordinates with .01 step, Shift .1, X clamp, normal state validation and linked fanout; consumed keys do not trigger timeline key nudge. Mounted Timeline47 PASS includes ArrowRight→x.26/Free, Shift+ArrowUp second entry, two exact Undo, locked keyboard no-op/history preservation. Typecheck0/lint/diffcheck PASS before final lock-test-only addition. Physical browser handle qualification, continuity context menu and native authoring paths remain open; full parity incomplete.

Lock regression initially exposed bubbling ArrowRight into timeline nudging another selected channel. Handle now consumes recognized arrow before lock refusal. Final Timeline47 PASS after fix; blocked handle cannot indirectly edit another selected key.

## Этап 189: linked tangent gesture and concurrent-change cancellation

Tangent gesture effect now cancels immediately on authored document identity change, in addition to lock cancellation, clearing previews and frozen axis before trailing release. Four mounted linked Scale cases verify identical X/Y preview with unchanged document/history, release authors exact captured preview to both axes with retained link and one Undo, Escape restoration, lock during draft, and concurrent canvas edit preservation without trailing tangent commit. Timeline51 PASS, typecheck0/lint/diffcheck PASS. Physical Chromium qualification, tangent continuity context menu and native normalization paths still open; full inventory not completed.

## Этап 190: timeline tangent continuity context menu

Handle context menu / ContextMenu / Shift+F10 opens Smooth/Broken choices with current continuity checkmark. Choice uses state tangent API and linked fanout, restores focus; Escape/outside pointer closes. Mounted Timeline51 PASS includes Shift+F10, Smooth commit one history entry and exact Undo. Typecheck0/lint/diffcheck PASS. Initial duplicate svelte:window compile error repaired by merging handlers into existing window element. Menu arrow/Home/End navigation and physical Chromium qualification still need completion; full inventory incomplete.

## Этап 191: tangent menu keyboard navigation and fresh runtime

Continuity menu supports wrapping arrows, Home/End, Enter/Space choice and Escape/Tab dismissal with focus restoration. Mounted Timeline51 PASS checks End, wrapping ArrowDown, Enter commit/exact Undo, Escape and Tab no document/history mutation. Full frontend729 tests/113 files PASS, typecheck0/lint/diffcheck PASS. cargo build PASS refreshes actual backend executable with per-key tangent records/evaluation, ready for MCP→Chromium qualification. Physical tangent gesture/menu scenario not yet executed; full inventory remains incomplete.

## Этап 192: physical timeline tangent qualification

Full real MCP→Chromium scenario now selects an outgoing Bézier segment, drags its timeline control with real mouse/capture, checks two identical linked Scale preview entries and unchanged get_project/history, Escape+trailing release, release authors exact captured draft to both axes with one history entry, exact Undo. Shift+F10 continuity menu followed by End/ArrowDown/Enter chooses Smooth, native get_project confirms metadata/twins and one entry; Undo restores exact source documents. Full test:e2e:agent PASS against freshly built native runtime. Initial wrong menu label selector timed out, corrected and rerun after terminal result. node syntax/diffcheck PASS. Screenshot /tmp/video-kadr-timeline-tangents-qa.png inspected: controls, guide lines, key diamonds and overlayed curves visible. Source per-key tangent UI materially qualified; native authoring normalization, Auto mixed-selection/clipboard and remaining global inventory still open.

## Этап 193: selected-key Auto/Free authoring

Added setKeyframeTangentMode and atomic multi-subject state operation with stale/duplicate subject refusal and linked fanout. Inspector selection actions now expose Auto/Free. Auto follows source set_auto: both sides Auto, continuity Smooth, both adjacent segments Bézier; neighbor records materialize before class changes to retain their coordinates. Free retains resolved coordinates and continuity. Tangent8 PASS, earlier combined Tangent8/Inspector42=50 PASS before neighbor materialization adjustment; final typecheck0/lint/diffcheck PASS. Source group behavior still needs dedicated state/mounted/physical qualification; native authoring paths and remaining feature inventory incomplete.

## Этап 194: Auto-to-Free grab fidelity and atomic multi-track Auto

Source set_tangent review confirmed either grabbed side converts both sides to Free. editKeyframeTangent now does this before Smooth reconciliation; incoming opposite-side patch also retains Free rather than reintroducing Auto. New source-contract test covers incoming Auto grab, both Free, retained Smooth and equal slopes. Multi-track state test confirms Auto changes two subjects in one history entry, exact Undo, and locked participant refusal preserves both tracks/history. Tangent9/selection9/Timeline51=69 PASS; typecheck0/lint/diffcheck PASS. Explicit edit on a non-Bézier owning side still needs segment conversion fidelity; group UI/physical qualification and native writes/global inventory remain incomplete.

## Этап 195: edited-side segment conversion fidelity

Manual tangent edit now converts only the owning segment to Bézier when it was Hold/Linear/procedural, materializing its two endpoint records before conversion. Original incoming/outgoing controls retain their own segment provenance. Incoming Smooth reconciliation checks opposite outgoing Bézier as source does, allowing an edited incoming non-Bézier segment to become Bézier. Meaningless endpoint sides remain stored without inventing a segment.

Tangent10/Timeline51/Inspector42=103 PASS, typecheck0/lint/diffcheck PASS. New mixed Hold→Bézier test verifies owning segment conversion, written incoming Free(.8,.4), opposite outgoing(.2,.3) retained, 101 unchanged samples on opposite segment and immutable input. Native authoring and remaining global feature inventory incomplete.

## Этап 196: clipboard tangent normalization and ownership

Clipboard snapshot deep-clones tangent records/sides; destination stationary and pasted keys are independent as well. Paste normalizes the sorted final merged track against destination neighbors after collision resolution, so Auto peak/continuity reflect new time/value context. Clipboard5/Timeline51=56 PASS, typecheck0/lint/diffcheck PASS. New test confirms no shared incoming object, copied input immutability, and destination peak incomingY1/outgoingY0.

Architecture correction: MCP apply_batch executes frontend agent/runtime.svelte.ts and its authoring commands; backend MCP bridges the session rather than owning a separate editor-state writer. Earlier generic 'native authoring endpoints remain' wording did not identify a real independent endpoint. Remaining MCP command normalization still requires audit; native persisted evaluation consumes explicit coordinates. Global feature inventory remains incomplete.

## Этап 197: MCP update_layer tangent normalization

MCP update_layer normalizes explicitly replaced visual/audio animation envelopes after initial schema admission and validates the result again. Untouched envelopes are preserved, trim/split remain outside this path. Shared normalizeAuthoredTangentAnimation resolves final track keys. Agent runtime14 PASS includes three-key Auto local-peak replacement, correct endpoints and middle controls, one patch history entry and exact Undo. Typecheck0/lint/diffcheck PASS; track-union mapper typing repaired with existing validated CompositionTrack cast pattern. add_layer/mask animation paths still require audit; broad parity incomplete.

## Этап 198: MCP add_layer and mask-envelope normalization

MCP add_layer normalizes admitted visual/audio tracks and video mask x/y/width/height/rotation envelopes before final batch validation. update_layer masks replacement normalizes only mask tracks; existing unrelated animation remains untouched. Agent runtime15 PASS includes admitted Auto layer creation, source request immutability, correct endpoint controls, one history entry and full Undo removing created track/layer. Typecheck0/lint/diffcheck PASS. Dedicated mask/audio normalization fixtures and physical MCP qualification still required; remaining all-feature inventory incomplete.

## Этап 199: MCP audio/mask Auto qualification

Agent runtime16 PASS includes video layer with Auto gain and mask x/rotation tracks, endpoint/peak resolved controls, mask-only replacement preserving audio envelope, equivalent raw Auto update resolves to exact same document and creates no history entry, one Undo restores initial document/source registry. Typecheck0/lint/diffcheck PASS before final Undo assertion-only addition; final runtime16 PASS. Initial fixture shape 'rect' corrected to supported 'rectangle'. Physical full MCP and source full-export tangent fixtures remain open; all-feature parity incomplete.

## Этап 200: full video export with per-key tangent controls

Expanded real_arbitrary_bezier_opacity_matches_full_composition_export to six real H264 MP4s: original track-default CSS/overshoot/degenerate-X cases plus same three curves supplied through first outgoing/last incoming Free/Broken key tangent records. Per-key cases deliberately use identity track defaults and unrelated first incoming/last outgoing Y±9; matching pinned independent oracle confirms effective segment controls and ignored meaningless endpoint sides. Each requires Completed, 21 RGB values within7, exact key/held samples, duration5±.12s, isolated cleanup. Real integration test PASS, diffcheck PASS. No product code changed here. Source all-feature inventory still incomplete; curve-specific export qualification does not establish nested compositions/Motifs/AI/color/motion-path/etc parity.

## Этап 201: canonical named Bézier preset family

Added 28 pinned source CSS/Sine/Quad/Cubic/Quart/Quint/Expo/Circ/Back Bézier entries with original arithmetic fractions and BACK_S/3. Exact coordinate reverse lookup preserves source naming without tolerance. Inspector track interpolation picker exposes preset ids through separate preset: tokens, avoiding legacy cubic ease string semantics. Presets persist resolved Bézier object and reuse existing evaluator/export. Catalog1 +Inspector42=43 PASS includes all entries valid/roundtrip lookup, exact quad sample and Penner back formula. Typecheck0/lint/diffcheck PASS. Timeline selected-key preset menu/MCP preset ids and baked tangent replacement for existing per-key records still require wiring; preset-family parity and global inventory incomplete.

## Этап 202: selected Bézier preset baking over authored handles

Selected-key Bézier interpolation authoring now writes left outgoing p1/right incoming p2 as Free when either endpoint carries tangent metadata, preserving outer controls and continuity before normal write solve. Metadata-free legacy segments retain their prior wire shape. Iteration reads current patched keys so adjacent selected segments retain already-baked incoming controls. Catalog2/Timeline51=53 PASS; final catalog2 PASS after iteration adjustment. Quad midpoint.25 regression includes conflicting old handles, unchanged external sides and immutable input. Typecheck0/lint/diffcheck PASS before final one-line iteration adjustment. Timeline preset catalog exposure, track-default baking and MCP ids remain unfinished; global parity incomplete.

## Этап 203: inherited track preset baking

Track-default Bézier changes now bake preset coordinates into inherited segments carrying tangent metadata, including consecutive segments. Explicit outgoing interpolation overrides remain unchanged; inherited keys retain omitted interpolation fields. Regression proves both Quad midpoints, preserved Hold override and outgoing handle, and input immutability. Catalog3 PASS, typecheck0 PASS, lint PASS after removing unused destructuring binding. Full feature parity remains incomplete; named timeline/MCP preset exposure and larger inventory are still open.

## Этап 204: timeline named preset authoring

Selected-key context menu exposes all28 canonical Bézier presets with exact reverse selection, shared hover/focus preview and atomic multi-lane commit. Generic Bézier radio is selected only for unnamed coordinates to avoid duplicate checked radios. Presets are placed before Elastic options, preserving existing End-key navigation. Mounted regression covers all names, two unchanged authored lanes during preview, one history entry for committed Quad, persisted interpolation on both lanes and exact Undo. Timeline52 +catalog3=55 PASS, typecheck0/lint/diffcheck PASS. Physical preset-menu qualification and MCP named preset ids remain open; all-feature parity remains incomplete.

## Этап 205: MCP named visual keyframe preset command

Added apply_batch/dry_run set_keyframe_preset {clipId,property,ticks,preset} for six visual scalar properties, with canonical28 ids advertised in native MCP operation schema. Pure evaluation uses the same selected-segment baking and validated layer-patch path; linked scale X/Y fan out together. Runtime17 PASS includes dry-run immutability, unknown-id refusal, one history entry, both linked curves and exact Undo. Native MCP5 PASS; typecheck0/lint/diffcheck PASS. Source uses edit_keyframe easing:{preset}; our batch command provides corresponding visual preset mutation but does not yet provide source-shaped readback preset_id, audio/mask targets, full edit-keyframe tangent API or physical MCP qualification. Global parity remains incomplete.

## Этап 206: MCP audio and mask preset targets

set_keyframe_preset accepts optional envelope visual/audio/mask (visual default), with maskId for mask tracks. Audio gain/pan supports source audio/video; mask x/y/width/height/rotation supports video masks. Property/envelope compatibility and existing tracks/keys are admitted before shared selected-segment bake and update_layer normalization. Native MCP schema advertises new fields and properties. Runtime18 PASS proves atomic two-envelope edit, failed second mask target leaves first edit unapplied, untouched gain/mask-y preservation, one history entry and exact Undo. Native MCP5/typecheck0/lint/diffcheck PASS. Physical roundtrip and source-shaped preset_id readback remain open; all-feature parity remains incomplete.

## Этап 207: real MCP named Bézier preset qualification

Expanded full native MCP→Chromium suite with all28 named Bézier ids, each dry-run preserving document/history, apply_batch writing exact controls to linked scale X/Y, exactly one history entry and Undo restoring exact prior project. Fresh native executable built; full browser suite PASS. First run exposed duplicate accessible ease_in names between legacy polynomial interpolation and canonical Bézier preset; preset buttons now have explicit Пресет <id> accessible names, keeping ids visible. Timeline52 PASS and lint/diffcheck PASS. Audio/mask physical preset roundtrip remains unqualified. Source audit additionally confirms Hold/Linear and six closed-form Elastic/Bounce canonical presets need inclusion in complete named MCP catalog; global feature parity remains incomplete.

## Этап 208: complete canonical easing class catalog

EASING_PRESETS now contains all36 pinned canonical names: Linear/Hold,28 Bézier, three Elastic defaults (amplitude1/period.3), three Bounce directions. MCP set_keyframe_preset resolves the full catalog and native schema advertises every id. Exact easingPresetId recognizes all classes, rejects perturbed Elastic defaults and legacy cubic ease_in string identity. Catalog4+runtime26=30 PASS includes each new closed-form name persisted through apply_batch in one history entry with exact Undo. Native MCP5/typecheck0/lint/diffcheck PASS. Existing Inspector/Timeline expose these classes through their previous mode controls; source-shaped readback preset_id and physical all36 qualification remain open. Global feature parity incomplete.

## Этап 209: MCP animation-track readback

Added discoverable get_keyframes {clipId,property,envelope?,maskId?}. Read-only projection returns constant/fallback or track timeBase/default/extrapolation plus keys with local native tick, local/timeline microseconds, resolved tangent records and effective leaving segment. Exact preset_id derives from actual per-key control overrides and full36 catalog; absent on last key or unnamed curve. Runtime27 PASS proves differing track-default/actual controls resolve Quad, non-microsecond clock and clip offset, last-key omission, static fallback, invalid property refusal and unchanged document/history. Native MCP5/typecheck0/lint/diffcheck PASS before description-only amendment. Physical readback, audio/mask read fixtures and broader parity remain open. Tool returns our scalar track representation rather than source typed/color/path animation records.

## Этап 210: MCP Auto/Free tangent authoring

Added set_keyframe_tangent_mode {clipId,property,ticks,mode,envelope?,maskId?}, advertised in native batch schemas. Shares preset command target admission and validated layer updates for visual/audio/mask, including linked scale fan-out. Auto delegates to solved-on-write key mode operation (adjacent spline conversion); Free retains coordinates. Runtime28 PASS proves a three-key local extremum resolves flat tangent slopes, Free retains every coordinate, one history entry per command, two exact Undos and locked-layer refusal without mutation. Native MCP5/typecheck0/lint/diffcheck PASS. Audio/mask dedicated mode fixtures and physical command qualification remain open; full edit-keyframe APIs and broader feature inventory remain incomplete.

## Этап 211: MCP authored tangent and continuity command

Added edit_keyframe_tangents {clipId,property,tick,incoming?,outgoing?,continuity?,envelope?,maskId?} with native schemas. Shared target routing covers visual/audio/mask and linked scale. Incoming then outgoing edits use the same manual Free/owning-segment conversion as UI; continuity applies last, Smooth derives incoming from outgoing. Empty edits refuse, X bounds/finite coordinates and existing-key validation use shared domain admission. Runtime29 PASS includes dry run, both adjacent segments becoming Bézier, resolved Smooth controls, one history entry, invalid-X/empty refusal without mutation and exact Undo. Smooth floating arithmetic expected incoming .4999999999999999; assertion uses14-digit proximity while persisted Undo remains exact. Native MCP5/typecheck0/lint/diffcheck PASS. Physical manual MCP/audio/mask qualification and source complete edit-keyframe move/easing operation remain open; all-feature inventory incomplete.

## Этап 212: exact MCP key retiming and full frontend regression

Added move_keyframe {clipId,property,tick,newTick,envelope?,maskId?} using native track ticks, same visual/audio/mask target routing and linked scale fan-out. Validates safe nonnegative destination, existing source and occupied destination refusal; sorts cloned keys and resolves Auto against final neighbours. Runtime30 PASS proves crossing neighbours, preserved Free controls, conflict leaves document unchanged, one history entry and exact Undo. Native MCP5/typecheck0/lint/diffcheck PASS. Full frontend initially caught stale pre-preset-baking Auto expectation; corrected separate tests for preset Free coordinates and ordinary value-edit Auto re-solving. A broad replacement briefly changed the Auto assertion too, corrected narrowly. Final full frontend756 tests/114files PASS. This does not prove physical new MCP operations or all-feature parity; broader inventory remains incomplete.

## Этап 213: MCP audio/mask key-operation regression qualification

Added dedicated gain/pan/mask-x/mask-rotation fixtures exercising get_keyframes, Auto solve, manual outgoing edit freeing both sides, Broken continuity and key retiming. All four cover native1000Hz track clock,3s clip offset, correct timeline readback, one history entry per edit, unrelated opposite envelope untouched and three exact Undos. Runtime34 PASS; lint/diffcheck PASS. Source-audio readback uses existing editor primary-video/hasAudio admission, as confirmed in requireAudioAutomationClip. Physical bridge qualification and remaining larger feature inventory still open; these scalar regressions do not establish full source typed-animation parity.

## Этап 214: MCP scalar key insertion/update/removal

Added upsert_keyframe {clipId,property,tick,value,envelope?,maskId?} and delete_keyframe {clipId,property,tick,envelope?,maskId?} to shared target routing/native schemas. Upsert creates missing envelope tracks, updates exact existing keys and uses existing interpolation inheritance/write normalization; linked scale fans out. Existing track ticks retain native timeBase; newly created tracks use1MHz. Delete refuses missing keys and last-key removal preserves removed value as constant. Runtime35 PASS covers absent x animation creation, exact-tick replacement, stale-delete atomic refusal, constant20 after last removal, one history entry each and three exact Undos. Typecheck0/lint/diffcheck PASS. Dedicated audio/mask insertion/removal and physical command qualification remain open; typed color/path/upstream full feature inventory remains incomplete.

## Этап 215: inserted key tangent inheritance and audio/mask lifecycle

Shared upsertKeyframe now copies preceding outgoing/next incoming controls onto a new key when surrounding authored metadata exists, with Broken continuity as pinned source newKey/upsert contract; append uses identity incoming and preceding outgoing. Metadata-free legacy wire shape remains unchanged. Catalog5 PASS tests middle/append controls and input immutability. Runtime35 PASS now includes insertion/deletion on gain/pan/mask-x/mask-rotation at native1000Hz, exact original document after removing inserted key, two history entries and exact Undos. Typecheck0/lint/diffcheck PASS. Optional explicit interpolation supplied during upsert with inherited tangents still needs baking audit; absent audio/mask envelope lifecycle and physical commands remain open. All-feature parity incomplete.

## Этап 216: explicit insertion easing baking

Shared upsertKeyframe applies explicit Bézier interpolation through selected-segment baking after insertion/update, ensuring inherited tangent metadata cannot override requested preset. Regression covers new interior key and exact existing-key update with conflicting old controls, both effective Quad midpoints. MCP upsert_keyframe accepts optional canonical preset for all target envelopes, validates ids and writes easing in the same transaction. Catalog6/keyframes/per-key25 +runtime36 PASS includes absent-track Quad creation/readback, unknown-id refusal and exact Undo. Typecheck0/lint/diffcheck PASS. Physical extended key APIs and remaining major feature inventory are still incomplete.

## Этап 217: unified MCP key edit transaction

Added edit_keyframe {clipId,property,tick,newTick?,value?,preset?,incoming?,outgoing?,continuity?,envelope?,maskId?}. Existing-key admission via retime, then value replacement, preset bake, manual incoming/outgoing and final continuity run within one pure batch evaluation/commit. Shared envelopes and linked scale routing retained. Runtime37 PASS proves combined move/value/Quad/custom-out/Broken edit on linked scale, custom controls overriding preset and unnamed readback, invalid final control rolling back preceding edits, one history entry and exact Undo. Native MCP5/typecheck0/lint/diffcheck PASS; initial scale fixture0 rejected correctly and changed to legal1→2. Our command uses native tick addresses and canonical preset field, not source id/t_us/raw-easing wire shape; full source-shaped API and all-feature parity remain incomplete.

## Этап 218: native distance-based motion-path geometry

Ported pinned WeftCut MIT eval/path.rs into native domain/motion_path.rs with attribution, equivalent std float sqrt/abs, serde node/point admission and bounded immutable CompiledMotionPath. Retains adaptive de Casteljau flattening depth9/tolerance.01, segment parameter table, cumulative-distance binary lookup, endpoint tangent extrapolation, degenerate/repeated point handling. Wrapper enforces1–128 nodes, finite authored controls and finite generated table/directions before runtime sampling. Native6 PASS includes four upstream geometric regressions (unequal lengths, coincident-endpoint loop, repeated points, ordered parameters) plus invalid/overflow admission and compiled lookup endpoints. Diffcheck PASS. This is geometry only: no persisted clip path/progress model, native rendering integration, WASM/preview or path UI yet; motion-path parity and complete feature inventory remain incomplete.

## Этап 219: authored native spatial path model and tangent solve

Added serializable MotionPath/PathNode with unique ids, absolute points, relative in/out handles, Line/Cubic segment and Corner/Smooth/Auto spatial modes. Local wire uses camelCase fields/lowercase enum values. Admission matches pinned coordinate limit10million,1–128 nodes and unique nonempty ids. Author-only solve ports shared/pathGeometry.ts unit-chord bisector Auto (neighbour-specific handle lengths; reversal stationary) and Smooth outgoing direction/incoming length. Playback compile consumes stored coordinates without implicit solve. Native8 PASS includes asymmetrical3/30 chord bisector, reversal, Auto exact idempotence/input preservation, Smooth lengths, serde roundtrip and duplicate/bounds refusal. Diffcheck PASS. Clip path/progress integration, spatial edit operations and native render/preview/UI remain unfinished; all-feature parity incomplete.

## Этап 220: native spatial node/mode edits

MotionPath native set_node_mode converts adjacent segments to Cubic for Smooth/Auto, initializes zero Smooth handles through Auto, and solves authored geometry. edit_node supports absolute point/relative incoming/outgoing; non-Corner manual handle becomes Smooth and aligns opposite handle preserving its length, then applies normal authored solve. Both are immutable, admitted before/after, stale indices/nonfinite/bounds refuse. Native10 PASS includes Auto→manual Smooth, incoming/outgoing symmetry, length preservation, Corner independence, adjacent class conversion and original-path preservation. Diffcheck PASS. Native path operations are not yet wired to persisted clip edits/UI/MCP or render; insertion/nearest lookup, integration and complete parity remain open.

## Этап 221: native exact path node insertion

MotionPath point_at evaluates line/cubic segment parameters; insert_node ports pinned de Casteljau split, copies segment class, freezes adjacent Auto nodes as Smooth, writes exact subdivided relative controls and admits result. Refuses endpoints/nonfinite parameter, invalid segment, duplicate/empty id and max-node overflow. Native12 PASS includes3003 positions across three asymmetric splits, both raw insertion and subsequent authored solve match original cubic within1e-10; line affine insertion/length exact and input preservation. Diffcheck PASS. These geometry operations still lack persisted clip integration/renderer/UI; source full-feature parity remains incomplete.

## Этап 222: native position/progress temporal evaluation

Added strict mutually-exclusive PositionAnimation XY/Path union and immutable compiled evaluator. Path uses existing AnimatableValue progress admission and KeyframeTrack sampling (all current easing/tangent/extrapolation semantics) with distance lookup and endpoint ray extension; XY uses same scalar evaluator. Compilation clones/sorts admitted keys without mutating original, retaining track options; nonfinite time/output refuses. AnimatableValue.validate visibility widened to crate for shared admission. Native14 PASS includes quadratic temporal progress driving unequal-length polyline at correct distance, exact endpoint, progress1.1 extension, staticXY, illegal mixed mode and nonfinite refusal. Diffcheck PASS. This standalone position domain is not yet included in clip serde/admission/render/preview/UI; all-feature parity remains incomplete.

## Этап 223: native transform position wire/admission

TransformSpec accepts optional position union with serde default/omitNone, preserving legacy documents byte-shape for absent field. Native transform admission compiles position, rejecting invalid scalar/path geometry. Compiler uses explicit XY tracks over legacy x/y; Path currently fails explicitly until distance-table FFmpeg generation is wired. Existing full literals updated None. Composition114 PASS; focused serde/admission1 PASS proves legacy omission/roundtrip, explicit XY roundtrip and invalid coordinate rejection. Diffcheck PASS. Model storage is native-only at this stage; frontend conversion, path export and UI still incomplete. Explicit export rejection is temporary unfinished work, not motion-path feature completion or all-feature parity.

## Этап 224: FFmpeg motion distance-table expressions

CompiledMotionPath now emits balanced binary-search piecewise coordinate expressions over the exact stored distance table, filtering zero-distance spans and retaining first/last endpoint-ray continuation. Register0 holds evaluated distance; existing Bézier/Elastic temporal evaluator uses3–9, allowing composition. Zero-length path constant; invalid axis/empty progress refuses. Native15 PASS includes real FFmpeg121 frames×2axes on unequal-length polyline from progress−.1 through1.1, matching native sampler within grayscale quantization1.01. Diffcheck PASS. Cubic-table/temporal-expression numeric qualification and connection to composition compiler remain pending; existing Path export rejection still present. All-feature parity incomplete.

## Этап 225: composition compiler consumes motion-path position

Removed temporary Path export rejection. Native compile_transform now compiles progress through shared animatable adapter/time offset, then X/Y through exact geometry table expressions; static progress folds to constant coordinates. Progress keyframes count once in existing admission budget. Conservative coordinate bounds include full table and endpoint-ray extrema; nonfinite extension refuses. Existing65KiB per-expression budget retained. Legacy/explicit XY path unchanged. Composition115 PASS; dedicated compiler1 PASS verifies unequal-length static coordinate10/40, animated X/Y exactly use shifted temporal expression and key count2. Diffcheck PASS. Real full MP4 path rendering, cubic-table numeric qualification, frontend serialization/preview/UI remain incomplete; compiler unit proof is not full source-feature parity.

## Этап 226: real MP4 path position and cubic FFmpeg qualification

Added real_motion_path_image_position_matches_distance_and_clip_clock: actualH264 MP4128²/20fps/2s, white image starts.25s and traverses unequal32/48px polyline over1s. Four independent analytic canvas coordinates are white>220; before/after clip pixels black<15, output Completed/duration2±.1, cleanup. Integration PASS after runtime factory corrected to local_default. Expanded real FFmpeg coordinate test to curved100px cubic as well as polyline,121frames×2axes×2paths=484quantized matches including endpoint continuation. Cubic adaptive table balanced lookup executes real FFmpeg successfully. Diffcheck PASS. Temporal nonlinear curves in full MP4, other layer kinds and frontend path transport/preview/UI remain incomplete; global feature parity incomplete.

## Этап 227: frontend path position schema and export transport

Added readonly frontend Point/PathNode/PositionAnimation XY/Path types on visual clips and WireTransform. Validation admits strict mode/node/point fields, unique1–128 ids, finite10million spatial coordinates and shared scalar track rules; audio position refuses. Active visual keyframe count includes position tracks. Strict payload copies path nodes/points and progress/default/extrapolation into native transform.position, including primary transport, rather than dropping it. Unsupported-authored-field gate recognizes validated position. Focused transport1 PASS covers JSON project roundtrip, export equality, detached spatial payload, duplicate ids and mixed-mode refusal; fixture repaired with required base video track. Typecheck0/lint/diffcheck PASS before final fixture-only addition. Preview currently still uses legacy x/y and primary video compiler has existing transform restrictions; position-aware UI/preview/native bridge and full product qualification remain incomplete. All-feature parity not achieved.

## Этап 228: native HTTP spatial authoring/preview bridge

Added POST /api/compositions/motion-path tagged solve/sample/edit/mode/insert request and typed frontend evaluateMotionPath transport with AbortSignal. Native bounded operations run in spawn_blocking, admit path, return authored path/length/points. Sample batch max4096finite progresses and finite output; no geometry math added to TS. Handler regression1 PASS exercises real async native sampler/endpoint continuation, insertion, oversized samples/stale node refusal and strict extra-field serde. Frontend typecheck0/lint/diffcheck PASS. Endpoint router-level/live-browser verification and consumer preview/UI wiring remain pending; no completed interactive path feature claim. Global feature inventory incomplete.

## Этап 229: native path points in composition preview

CompositionPreview consumes explicit XY position tracks and requests Path coordinates from the native motion-path API. Bounded nonreactive 256-point cache avoids repeating exact path/progress requests. Responses carry local request identity; effect teardown aborts outdated requests and late answers cannot replace the current point. Pending path layers stay hidden until their matching coordinate arrives; authored geometry is unchanged. Mounted preview regression verifies seek cancellation, stale response rejection, current native coordinate placement and document preservation. Focused frontend18 PASS (preview17, path transport1); typecheck0 errors/0 warnings, lint and diffcheck PASS. HTTP per-frame latency and continuous playback remain unqualified; primary video placement still ignores transforms and native compiler retains primary restrictions. Spatial editing UI and full source-feature parity remain incomplete.

## Этап 230: immutable native spatial authoring command

Added editCompositionMotionPath bridge for solve/node edit/tangent mode/exact insertion. Captures detached authored path and progress, refuses non-path/audio/locked track or clip before native request, passes AbortSignal, rejects cancelled completion and applies returned geometry through standard validated update_layer evaluation. Returns a new document; caller must commit only against captured document identity to avoid concurrent-edit overwrite. Focused path2 PASS includes native edit request/returned placement, progress preservation, original-document preservation, lock refusal without request and cancellation after native response. Typecheck0/lint/diffcheck PASS (lint before type-only narrowing correction). Not yet consumed by spatial UI or MCP, so interactive authoring and full-feature parity remain incomplete.

## Этап 231: inspector spatial path authoring

Added CompositionMotionPathEditor in inspector for text/image/secondary video. Creates a persisted two-node linear path at current static position with progress0→1 across actual clipDurationTicks; native bridge supports point coordinates, Corner/Smooth/Auto and midpoint insertion. Standard commitDocument preserves Undo/validation/locks. Busy input disabled, unmount aborts pending request, captured-document identity rejects concurrent-edit overwrite. Mounted regression verifies created geometry and concurrent canvas change preserved while late path edit refuses. Focused3 PASS; typecheck0/lint/diffcheck PASS after fixture switched to actual canvas field. Primary video editor deliberately unavailable under existing compiler restriction. Numeric handles, segment class/deletion, progress timeline/MCP and continuous preview qualification remain incomplete; this is partial spatial UI, not complete parity.

## Этап 232: spatial handle controls and stationary mode initialization

Inspector exposes relative incoming/outgoing handle X/Y and routes edits to native authored solver (Auto→Smooth and opposite handle rules remain Rust-owned). Mounted regression checks out_handle request retains other coordinate and stale response refusal. Re-read pinned ADR0060: corrected stage231's invented two-node animated initialization. Creation now yields one stationary node and constant progress0; separate explicit append-endpoint action invokes native solve and preserves progress verbatim, with abort/concurrent-document guard. Focused3 PASS; typecheck0/lint/diffcheck PASS. Animated XY→Path conversion, explicit XY static initialization, conversion previews, progress editing and node arrival remain incomplete; no claim of full position-mode parity.

## Этап 233: exact static XY/Path representation switches

Added pure switchStaticPositionMode and inspector single-node Return XY action. Static explicit XY takes precedence over legacy transform/animation axes when initializing stationary one-node Path; legacy animated X/Y and explicit animated XY refuse pending actual native conversion fitter. One-node Path returns exact constant XY even with animated progress, without baking keys. All writes use validated batch and normal inspector commit/Undo. Focused3 PASS adds explicit77/−12 preservation, immutable animated-conversion refusal and one-node animated-progress return, retaining mounted handle/stale-response regression. Typecheck0/lint/diffcheck PASS. Multi-node conversion, animation fitting and remaining feature inventory incomplete.

## Этап 234: authored path progress keys in inspector

Added setMotionPathProgressKey and inspector numeric progress plus start/end-at-playhead actions. Converts composition-local time to existing scalar timeBase, preserves geometry/other keys and inserts initial static value at zero when first key is later. Finite overshoot remains allowed; outside-clip and unsafe time refuse, corresponding UI controls disabled outside clip. Standard validated commit gives one Undo. Focused3 PASS covers later first key, initial value,1000Hz track on shifted2s clip, existing keys and overshoot1.1, outside-range refusal; mounted path/handle/concurrent-edit regression remains green. Typecheck0/lint/diffcheck PASS (lint before disabled-only markup addition). Node-distance arrival, general progress timeline editor/MCP targeting, conversion and full feature parity remain incomplete.

## Этап 235: native node distance arrival

Motion-path HTTP adds measure and nodeProgresses on responses, derived from integer node boundaries in exact compiled Rust cumulative arc-length table (zero-length0). Inspector exposes arrive-at-node-now, disabled outside clip, and immutable async command writes only full-precision progress through shared key helper. Captured-document guard/abort/normal commit preserve existing authoring state. Native handler1 PASS proves endpoint fractions0/1 and inserted unequal spans0/.25/1. Frontend3 PASS includes full-precision fraction.3141592653589793, initial key preservation and exact geometry equality. Typecheck0/lint/diffcheck PASS (lint before test-only addition). Live endpoint/browser and curved/repeated-node arrival qualification remain pending; complete conversion, advanced spatial UI and global parity remain incomplete.

## Этап 236: position curves survive split and trim

Split/trim automation now includes optional PositionAnimation. Retains detached complete spatial geometry and all scalar keys; left keeps source curve, right shifts complete temporal curve by negative cut, trim rebases by existing delta. Explicit XY axes use same preservation; keys outside edited interval intentionally remain, preserving spline/extrapolation rather than filtering endpoints. Focused19 PASS verifies path equality, linear cut/trim values and61 nonlinear Bézier right-half samples against original shifted time within1e−12; initial fixture interpolation spelling corrected. Typecheck0/lint/diffcheck PASS (lint before test-only Bézier addition). Live split/trim export and non-default composition timeBase qualification pending; global parity remains incomplete.

## Этап 237: detached path copies and render automation records

cloneClip now deep-copies optional position geometry/progress, including clipboard/duplicate paths. Shared cloneInterpolation detaches Bézier tuples and Elastic parameters; cloneKeyframe detaches interpolation alongside tangent handles; cloneAnimatableValue detaches default interpolation. Strict render copy detaches default/per-key curves and nested tangents, avoiding mutation of editor/history through wire payload. Regression mutates duplicate path point, exported Bézier control and key outgoing handle and verifies authored document unchanged. Full frontend769 PASS /116files; typecheck0 errors/0 warnings after readonly test cast corrected; lint/diffcheck PASS (lint before test-only additions). Global source-feature parity remains incomplete.

## Этап 238: native node removal and Line/Cubic switch

Read pinned PositionFields updateNodes/toggleCurve/remove behavior. Rust remove_node retains at least one node and runs authored solve; toggle_segment matches source Corner transition, Line↔Cubic, new curve outgoing(60,−60)/next incoming(−60,−60), followed by solve. Rejects stale indices including usize::MAX without overflow. Added strict HTTP remove/toggle_segment variants, typed frontend commands and inspector controls; standard immutable authoring pipeline preserves progress and commit/Undo. Native handler1 PASS checks curve length/handle, removal→stationary node, final-node refusal and overflow index refusal. Focused frontend3 PASS/typecheck0/lint/diffcheck PASS; rustfmt touched native path/handler only. Canvas gestures, conversion, live UI/export qualification and global source parity remain incomplete.

## Этап 239: live native motion-path HTTP qualification

Rebuilt backend binary and added scripts/check-motion-path-http.py reproducible live-route regression. Starts actual backend on ephemeral loopback port with temporary isolated storage, checks readiness/process liveness, sends JSON HTTP requests and always terminates process/cleans storage. Real HTTP PASS: unequal30/70 polyline length100/node fractions0/.3/1 and sampled midpoint30/20; cubic toggle fractions; Auto→manual Smooth handle edit; curved insertion/removal; coincident zero-length nodes/progress overshoot stay5/7 with fractions0; stale index400 and strict extra-field4xx. cargo build PASS with existing warnings; diffcheck PASS. This confirms real router/serde/native calculation, not browser rendering or continuous playback. Conversion, canvas editing, nested compositions and global feature parity remain incomplete.

## Этап 240: atomic agent position mode and progress authoring

Added switch_position_mode (positionMode/nodeId) and set_path_progress (composition playheadTicks/value) to AgentOperation, pure batch dispatch and native MCP operation schemas. Reuses exact static representation and scalar progress commands; recursive update_layer validation/locks and outer batch commit preserve atomicity/revision/Undo. Frontend40 PASS includes dry run unchanged, invalid later out-of-clip operation rolls back earlier mode/key edits, successful batch one history entry and exact Undo. Native MCP5 PASS; typecheck0/lint/diffcheck PASS. New operations not yet tested through actual MCP browser bridge; spatial native async commands/position keyframe easing targets remain incomplete, as does global feature parity.

## Этап 241: agent path progress easing/tangents and readback

Existing scalar key operations accept envelope path/property progress: preset, Auto/Free, manual tangents, move/upsert/delete/unified edit preserve spatial geometry and use standard validated atomic batch. get_keyframes reads path progress through existing resolved segment/clock/preset response. Native catalog advertises path envelope/progress. Runtime39 PASS includes Quad preset→Free→manual outgoing→insert key, unchanged geometry, readback0/.3/1 and one Undo. Typecheck0/lint/diffcheck PASS; native MCP5 PASS before final read-schema-only addition. Actual MCP browser path loop, inspector temporal curve UI and remaining all-feature parity incomplete.

## Этап 242: inspector progress easing and extrapolation

Added configureMotionPathProgress using shared updateInterpolation (including inherited Bézier tangent baking), validated extrapolation settings and standard document update/Undo. Inspector offers all36 named presets and before/after Hold/Continue/Loop/PingPong/Offset for keyed progress. Geometry and explicit per-key overrides retained. Focused3 PASS includes Quad midpoint.25, after-loop1.5s→.25, unchanged path and unknown-preset refusal; mounted controls regression retained. Typecheck0/lint/diffcheck PASS (lint before test-only additions). Full temporal timeline editor, custom curve control, conversion/native WASM preview and remaining global parity incomplete.

## Этап 243: custom progress Bézier control editor

configureMotionPathProgress accepts explicit admitted interpolation and shared inherited-segment/tangent baking. Inspector reuses BezierControlEditor for keyed Bézier progress; graph draft stays transient, release commits once through normal document update/Undo, Escape discards and captured-document identity rejects concurrent write. Geometry unchanged. Focused3 PASS includes custom overshooting control points, midpoint.5, unchanged path and invalid normalized X refusal; existing mounted path regression retained. Typecheck0/lint/diffcheck PASS. Actual path-specific pointer/keyboard gesture and canvas transient-preview qualification still pending; no new full-feature parity claim.

## Этап 244: progress curve gesture and locked-track regression

Path editor now derives disabled state from both clip and owning track lock, including Bézier keyboard controls/aria state. Mounted regression covers ArrowRight p1.25→.26 with one Undo/exact restoration; synthetic pointer drag holds document unchanged until release then p1.5/.5 one Undo; Escape discards draft with no history; track lock disables controls and ArrowUp leaves document exact. Component2 PASS/typecheck0/lint/diffcheck PASS (lint before synthetic pointer test addition). Pointer test uses mocked SVG rectangle and synthetic DOM events, not physical browser input; canvas preview is not updated during curve graph draft. Full feature parity remains incomplete.

## Этап 245: numeric Bézier and Elastic progress controls

Inspector exposes all four numeric Bézier coordinates (normalized X0..1, unrestricted finite Y) and Elastic amplitude/period, reusing validated configure command and shared scalar authoring semantics. Mounted regression edits p2Y1.4 preserving p2X.75, one history entry and exact Undo. Domain regression admits custom ElasticA1.5/P.45 while rejecting negative period, geometry retained. Focused4 PASS/typecheck0/lint/diffcheck PASS (lint before test-only additions). Graph gestures, numeric temporal defaults now available; selected-key/timeline integration, curve canvas draft preview, native conversion and global feature parity remain incomplete.

## Этап 246: inspector progress key authoring list

Inspector keyed progress shows editable layer-local seconds/value, outgoing segment preset and delete. Converts edited seconds through native track timeBase; shared agent operations enforce collision/admission/tangent rules and each UI action commits one Undo. Segment preset readout uses effective resolved left/right tangents rather than stale raw interpolation. Mounted regression covers value1→1.2,1000Hz time1s→.75s/tick750, key deletion and exact Undo, alongside curve gestures/locks. Focused41 PASS (component2/runtime39), typecheck0/lint/diffcheck PASS. Full timeline lanes/selection, per-key physical tangent UI, conversion and global parity remain incomplete.

## Этап 247: native whole-path translation

Added immutable MotionPath.translate, finite delta and complete before/after spatial admission; shifts points only and keeps relative handles/modes/segments intact without re-solving shape. Strict HTTP translate and typed authoring command wired to inspector offsetX/Y action; shared async pipeline preserves temporal progress, cancellation, concurrency guard and Undo. Native handler1 PASS samples121 translated cubic positions including overshoot, exact13/−9 shift within1e−10, same length/handle and overflow refusal. Focused frontend4 PASS/typecheck0/lint/diffcheck PASS. Explicit offset command is not canvas dragging; live translated preview/export and native conversion/global feature parity remain incomplete.

## Этап 248: per-key temporal tangent modes in path inspector

Progress key list exposes Auto/Free and Smooth/Broken through existing path-envelope key commands. Mounted test exercises Auto solved metadata and continuity change with exact Undo; fixture array expectation corrected to retain second key. Component2 PASS; focused41 PASS before test-only additions; typecheck0/lint/diffcheck PASS before final test-only assertion correction. Manual per-key handle widgets/temporal timeline integration and global feature parity remain incomplete.

## Этап 249: numeric per-key progress tangents

Inspector resolves each progress key's incoming/outgoing controls and exposes normalized X/finite Y coordinates. Manual changes use shared edit_keyframe_tangents path operation, convert edited side to Free, preserve opposite coordinate and continuity semantics, and commit once. Mounted regression edits outgoingY−.2 then restores exact document through Undo. Component2 PASS/typecheck0/lint/diffcheck PASS after Svelte const placement and test syntax corrected. Physical per-key tangent graph/timeline, native conversion and global feature parity remain incomplete.

## Этап 250: native nearest-curve hit testing and insertion command

Ported pinned nearestPathLocation to Rust:64 probes per span then28 ternary refinements, native point_at and distance. Single-node returnsNone; finite bounded query admission. Strict HTTP nearest returns optional segment/t/distance. insertMotionPathNodeNear uses native hit, distance tolerance and existing exact subdivision authoring command, retaining cancellation/locks/immutable document. Native handler1 PASS analytic line hitx37/y8→t.37/d8; frontend2 PASS hit→exact insert parameter and too-far refusal. Typecheck0/lint/diffcheck PASS before final test-only additions; serde derive corrected and native rerun after bounds patch PASS. This is hit-test foundation, not connected canvas double-click; loops/cusps live qualification and global parity remain incomplete.

## Этап 251: selected motion path canvas contour and insertion

Added SVG overlay for selected active secondary visual path: serializes authored Line/Cubic controls, centered canvas coordinates and nodes, no TS geometry evaluation. Stroke-only pointer target double-click maps CSS pixels to composition pixels, invokes native nearest+exact insertion within8-screen-pixel tolerance, guards concurrent document/cancellation and commits once. Preview integrates selected path overlay. Mounted overlay1 PASS validates scaled/offset coordinate mapping, request parameter.5, unchanged progress and one exact Undo; preview/editor19 PASS. Typecheck0/lint/diffcheck PASS. Synthetic DOM input/mock native transport is not physical browser/live qualification; node/handle dragging, native conversion and global parity incomplete.

## Этап 252: keyboard path node movement on canvas

Selected path overlay nodes become focusable controls with Arrow movement1px/Shift10px, explicit aria labels/disabled state for track and clip locks or pending request. Native edit_node handles authored solve; captured document/cancellation guard and one commit preserve Undo/progress. Mounted regression verifies ShiftRight native point request10/0, unchanged progress, one Undo and exact restoration. Preview/overlay18 PASS/typecheck0/lint/diffcheck PASS. Keyboard movement is implemented; pointer dragging/handle controls, physical live browser and conversion/global parity remain incomplete.

## Этап 253: canvas path node pointer gestures

Overlay node drag freezes SVG rectangle/document, uses3px threshold, shows transient point/contour without document writes and commits one native authored point edit on release. Escape/cancel/lost capture discard; lock/concurrent document cancels. Pointer capture released after clearing gesture. Mounted synthetic regression checks40CSSpx→20canvaspx mapping, draft-only document, Escape no native request, release one request/Undo and exact restoration. Overlay1 PASS/typecheck0/lint PASS before final capture-lifecycle-only addition; typecheck0/diffcheck after addition PASS. Draft contour translates authored point only; native Auto/Smooth recomputation happens on release, not transient drag. Physical browser/handle dragging and global feature parity incomplete.

## Этап 254: spatial canvas handles

Overlay renders incoming/outgoing relative-vector handles with connecting lines, focusable keyboard controls and shared pointer gesture pipeline. Draft updates targeted point/handle; native edit_node owns final Corner/Smooth/Auto solve and temporal progress retained. Arrow step1/Shift10 supported for handles. Mounted overlay1 PASS verifies outgoing ShiftUp request0/−10 and exact Undo with detached returned geometry (initial unchanged mock incorrectly produced no history; repaired). Typecheck0/lint/diffcheck PASS before final test-only correction. Handles are exposed even on Line spans where stored vectors do not affect playback; class switch remains inspector operation. Native-resolved drag preview, physical browser and global feature parity incomplete.

## Этап 255: refreshed live geometry route and full frontend qualification

Rebuilt current backend and expanded isolated real HTTP script to nearest analytic line hit (.5,4px), cubic nearest interior and whole-path translation13/−9 with same length and sampled43/11 coordinate. Full real-route PASS now covers new nearest/translate alongside mode/handle/insert/remove/zero-length/error cases. Full frontend773 PASS /117files after canvas/node/handle and progress UI additions; typecheck0/lint/diffcheck PASS; cargo build PASS with existing warnings. Physical browser canvas gestures/native-resolved transient preview, conversion, primary video transform and remaining global feature parity still incomplete.

## Этап 256: pinned native conversion core

Vendored MIT weftcut-conversion plus its exact weftcut-eval dependency from pinned d1db8020164c51ee49df9cf0143a5cbf250951a4 under backend/vendor; retained licenses/source provenance and copied golden fixtures with local include paths. Backend now links conversion via path dependency; algorithm owns XY→Path spatial least-squares fit, temporal fit/refinement, editable Path→XY, explicit bake, simplification and measured tolerance/capacity result. Upstream conversion14 PASS (circle order, joint error, jumps refused, fractional fps, plateaus, long sparse ranges etc.); eval63 PASS including golden frame/pan/time remapping/colour/path semantics. Backend cargo check PASS with existing warnings; diffcheck PASS. Standalone vendor tests resolve their own locked serde versions while backend uses existing lock. No product wire adapter, HTTP conversion, preview/apply UI or shared playback consolidation yet; these source algorithms available does not establish feature completion. Global parity remains incomplete.

## Этап 257: native position conversion preview API

Added POST /api/compositions/position-conversion taking pinned conversion Request and returning full Conversion metrics/result. One semaphore slot prevents concurrent native fitting; spawn_blocking keeps async reactor responsive, native validation/errors propagate4xx and preview never changes persisted project. Typed raw frontend transport includes native segments/tracks/nodes/options and AbortSignal. Handler1 PASS real XY linear/static-axis fit within.5px and invalid options refusal; typecheck0/lint/diffcheck PASS. HTTP abort does not interrupt already-running blocking fitter (disposable WASM worker/cancellation still pending). Product animatable wire adapter, transient preview/Apply UI and live-route qualification remain incomplete; global feature parity incomplete.

## Этап 258: authored position input packing for native conversion

Added TS transport adapter packing XY scalar axes or resolved Path geometry/progress into native conversion records. Converts timeBase ticks→microseconds, resolved segment tangents, Elastic/Bounce and extrapolation enums. Legacy cubic in/out uses exact normalized controls; piecewise in-out inserts exact midpoint segment boundary, no sampled fitting or approximation in TS. Detached nodes/options preserve authored records. Packing1 PASS compares707 grid samples across7 curves via independent reconstructed scalar records, preserves input bytes, checks custom Elastic/PingPong/Offset and XY tracks. Typecheck0/lint/diffcheck PASS (lint before test-only addition). End-to-end native packed conversion, result unpacking/Apply and UI incomplete; piecewise packing can exceed4096 source-key admission at dense capacity and needs qualification. Global parity remains incomplete.

## Этап 259: conversion result unpacking and validated apply command

Added native-track result transport to authored1MHz scalar tracks, retaining segments/extrapolation and explicit Free/Broken handles. Single-key axis collapses to constant. Path nodes receive caller IDs with detached relative geometry and Corner mode to preserve fitted controls without solver reshaping. applyPositionConversion delegates standard validated update_layer; missed/nonfinite tolerance results refuse and noncanonical fractional key time refuses. Focused4 PASS includes707 additional unpacked roundtrip samples, detached geometry, Free/Broken and tolerance/grid refusal. Typecheck0/lint/diffcheck PASS (lint before test-only additions). Actual native result→product Apply qualification/UI and discrete-time rounding error reassessment pending; global parity incomplete.

## Этап 260: conversion inspector with transient preview and Apply

Added debounced native XY/Path conversion inspector with pixel tolerance, editable/bake mode and frame interval. Candidate composition feeds canvas preview without document/history mutation; Cancel and component cleanup discard it. Apply requires tolerance success, current source identity and unlocked clip/track, then commits through validated command once. Failed-tolerance geometry can be previewed but cannot be applied. Mounted mocked-transport regression PASS verifies transient preview, Cancel with unchanged history/document, one Apply and exact Undo. Full frontend776 PASS/119files before final Cancel assertions; focused test rerun after those assertions PASS. Typecheck0 errors/0warnings and lint PASS before final test-only additions. Native core fitting remains Rust. Actual rebuilt conversion HTTP/browser integration, running-fit cancellation, primary video transforms, fractional-duration endpoint qualification and global feature parity remain incomplete.

## Этап 261: real conversion HTTP qualification and frame range authoring

Rebuilt backend and extended isolated actual-route script: XY linear/static axis→native fitted Path→native XY retains endpoints0/100 and constantY20; explicit five-frame bake yields seven keys; invalid tolerance returns400. Existing actual geometry-route checks also PASS. Inspector now exposes start/end frames, validates safe integers and clip bounds before request, uses full rounded clip endpoint by default as pinned PositionConversionFields does, and forces everyFrames1 in editable mode. Mounted regression PASS checks out-of-range21 on20-frame clip produces no HTTP request/preview, selected5..15 range reaches transport, Cancel and one Apply/Undo. Typecheck0/lint PASS before final test assertion-count correction; diffcheck PASS. Physical browser conversion, interruptible running fit and global feature parity remain incomplete.

## Этап 262: native animated color evaluator and actual HTTP goldens

Added product-domain native color sampler using pinned weftcut-eval generic Rgba8 evaluation: premultiplied-alpha OkLab, Hold/Linear/Spline/Elastic/Bounce and per-side Hold/Continue/Loop/PingPong/Offset semantics. Strict RGBA u8/time/key-order/tangent/Elastic/capacity admission; empty tracks return supplied default. POST /api/compositions/animated-color uses two bounded blocking slots rather than fitting on the async reactor. Typed frontend transport preserves colors, microseconds and AbortSignal. Copied unchanged MIT upstream externally anchored animatedColorGolden fixture with provenance. Native2 PASS; actual rebuilt HTTP10 cases/18 samples PASS with invalid-channel4xx, alongside complete geometry/conversion HTTP checks. Frontend API31 PASS, typecheck0/lint PASS before final test-only addition; build PASS with existing warnings; diffcheck PASS. This is evaluator/transport qualification, not completed color feature: authored document/wire ownership, inspector/timeline/MCP commands, canvas playback and export integration remain open. Running native sampling is not interrupted by HTTP abort. Global parity remains incomplete.

## Этап 263: authored color records and edit-clock preservation

TextClip now owns optional colorAnimation, constant hex RGBA or keyed color values with existing track timeBase, segment controls, tangent metadata and extrapolation. Color/type/clock/order/curve admission reuses temporal validation without scalar color projection; admits4096 color keys independently of existing32-key scalar cap. Immutable set/delete commands use layer-local playhead clocks, replace coincident key values, preserve initial color when starting later, enforce locks and collapse deleted last key to its color. Serialization keeps color data; split/trim retain entire curve with shifted signed times rather than resampling; duplicate deeply detaches controls/tangents. Full frontend779 PASS/120files before final color-capacity change; focused19 PASS after capacity change, color2 PASS after test typing fix; typecheck0/lint PASS (lint before final test-only typing fix); diffcheck PASS. Export explicitly refuses colorAnimation until color-channel integration, preventing silent loss. This stage is authoring/storage/commands, not native sampling adapter, inspector/timeline/MCP color command family, preview or export completion. Color-layer kind beyond text and global parity remain open.

## Этап 264: authored color packing and native seek preview

Added transport adapter from authored hex color tracks to native RGBA/time/curve/extrapolation records. Preserves fractional microsecond key times instead of forcing track clocks to rounded integer microseconds; sample times remain canonical integer project times. Shared scalar timing packer handles inherited/per-segment Bezier/Elastic/Bounce/Hold/Linear controls; color values never become scalar interpolation data. Native compatibility wrapper evaluates legacy piecewise cubic-in/out without quantizing a midpoint color, retains shared native OkLab blending and cyclic/Offset handling; canonical legacy controls ensure Continue endpoint slopes remain zero. Text preview consumes native color samples, bounded256-result cache, abort/identity protection and hides unresolved samples. Mounted regression verifies stale-after-seek refusal and current RGBA application without document mutation; authored packing test checks fractional negative time, opacity byte and custom controls. Native3 PASS including exact legacy fade/PingPong/Continue; frontend focused21 PASS; typecheck0/lint/build/diffcheck PASS. Rebuilt actual HTTP goldens10cases/18samples plus fractional-key legacy fade PASS; existing geometry/conversion routes PASS. Continuous playback under HTTP latency is not qualified and may hide pending frames; WASM/fast local native preview remains necessary. Inspector/timeline/MCP color commands, color-layer kind and export remain incomplete; export still refuses authored color. Global parity remains incomplete.

## Этап 265: reachable color key inspector

Text inspector now mounts CompositionColorEditor: hex RGBA draft, create/replace key at current playhead, keyed local-time/color list and delete. Controls derive actual current document clip and owning locks; layer-outside playhead disables creation. Added immutable updateTextColorKey preserving temporal metadata, signed key clocks and standard collision/admission rules. Each UI action commits once through normal Undo; invalid color/time collision leaves document unchanged and displays admission error. Mounted regression covers initial-color preservation, one-entry creation/exact Undo, value/time edits, collision refusal, deletion/Undo and locked-track refusal. Focused46 PASS including existing inspector39 and color-domain3; typecheck0warnings/0errors, lint and diffcheck PASS after initial derived typing/state-reference correction. Synthetic mounted controls are not physical-browser qualification. Segment/default curves, extrapolation/tangents in color inspector, timeline and MCP color command family, native local playback, color-layer kind and export remain incomplete. Global parity remains incomplete.

## Этап 266: color presets and per-side extrapolation

Color inspector now exposes all36 shared easing presets for track default and outgoing keyed segments plus Hold/Continue/Loop/PingPong/Offset on each side. configureTextColor bakes Bezier controls into Free/Broken temporal handles, retains RGBA values and explicit other-segment overrides when changing inherited defaults; color values never enter scalar slope solving. Pure command checks qualify packed exact quadratic coordinates, unchanged Hold override, Elastic override, Offset and input immutability. Mounted checks exercise default/per-key selectors and PingPong with exact Undo, retaining existing color/time/lock regressions. Focused5 PASS, typecheck0/lint/diffcheck PASS after keyed-each lint fixes and test array expectation repair. Custom numeric/graph curve controls, Auto/Free color-specific tangent solve, timeline/MCP, local native playback and export remain incomplete. Global parity remains incomplete.

## Этап 267: custom color curve parameters

configureTextColor now accepts explicit admitted interpolation separately from preset selection, preserving the same temporal baking and overrides. Reusable InterpolationNumbers exposes all four Bezier coordinates (X0..1, finite Y including overshoot), Elastic amplitude and period for color default and individual outgoing segments. Numeric release commits once, disabled fieldset retains owning locks and admission refusal leaves document exact. Mounted regression covers inherited p2Y1.4/native-bound incoming handle, segment Elastic amplitude1.5, invalid period0 and exact Undo; command test checks custom p1(0.2,−0.3)/p2(0.8,1.4) packing and invalid X refusal. Focused5 PASS; full frontend783 PASS/121files; typecheck0errors/0warnings, lint and diffcheck PASS. Graph gestures/curve draft canvas projection, Auto/Free color-specific solve, timeline/MCP, native local continuous playback, color-layer kind and export remain incomplete. Global parity remains incomplete.

## Этап 268: graphical color Bezier gestures

Color inspector reuses graphical BezierControlEditor for default color curve and collapsible individual segment graphs. ColorCurveEditor captures source document/clip/tick at gesture start, shows graph draft without history writes, commits one validated configure operation on release, cancels Escape and rejects concurrent document/target change. Keyboard arrows share immediate validated commit; owning locks disable controls. Mounted regression verifies unchanged document during drag, p1(.5,.5) release with one Undo, exact restoration, Escape no history and concurrent canvas edit retained without stale curve overwrite. Focused5 PASS, typecheck0/lint/diffcheck PASS. Pointer input uses synthetic events/mock rectangle; physical browser and transient canvas color projection remain unqualified/unimplemented. Auto/Free color-specific solve, timeline/MCP, native local continuous playback, color-layer kind and export remain incomplete. Global parity remains incomplete.

## Этап 269: transient color curve canvas preview

ColorCurveEditor now publishes a validated candidate document during Bezier gesture without committing history. Separate preview state stores source identity and owner token; Escape/release/unmount clears only the owning candidate. Preview consumes source-matching color candidate before position-conversion candidate, then existing scalar previews, and samples color through Rust transport. Concurrent document changes invalidate candidate display and release admission. Mounted gesture test verifies draft document contains changed curve while actual document remains exact, release/Cancel clear candidate; mounted canvas test verifies candidate controls reach native sampler, returned draft RGBA displays and clearing restores cached original color with no document mutation. Focused19 PASS; typecheck0/lint/diffcheck PASS (lint before final test-only additions). Native response is mocked in these mounted tests; real HTTP evaluator goldens qualified in264, but combined physical browser/native gesture and continuous-playback latency remain unqualified. Auto/Free color solve, timeline/MCP, native local playback, color-layer kind and export remain incomplete. Global parity remains incomplete.

## Этап 270: scalarless color Auto/Free authoring

Ported pinned keyframe_edits.rs scalar=None write rule to color authoring: Auto sides adjoining Spline resolve to identity (1/3,1/3) leaving and (2/3,2/3) arriving; Smooth does not couple two Free color sides through a scalar slope. Normalization runs after admitted update_layer colorAnimation writes. setTextColorTangentMode selects both sides, converts adjacent non-Spline spans for Auto, keeps other controls and freezes solved coordinates for Free. Inspector exposes Auto/Free per key; shared resolved color controls keep unused endpoint defaults canonical. Pure regression with unequal intervals/three different RGBA values checks Auto identities, opposite controls preserved, Free native packing identical and Smooth Free pair unchanged. Mounted selectors verify modes and exact Undo. Focused45 PASS (color5/editor1/runtime39), typecheck0/lint/diffcheck PASS. Manual continuity/individual-side widgets, timeline/MCP color family, native local continuous preview, color-layer kind and export remain incomplete. Global parity remains incomplete.

## Этап 271: manual color tangent sides and continuity

Added editTextColorTangents and compact inspector controls for each key's incoming/outgoing X/Y and Smooth/Broken. Manual edit freezes both sides Free, preserves the opposite coordinates, converts the touched non-Spline span to Bezier while retaining other endpoint controls, and relies on scalarless write normalization; Smooth on colors remains metadata without scalar coupling, matching pinned renderer/keyframe/edits.ts scalarOf=null route. Domain regression checks outgoing(.42,-.4), incoming(.9,-.2) retained, Broken leaves native packed curve unchanged, invalid X refuses. Mounted regression edits outgoingY-.2 through real control and continuity with exact Undo, alongside all existing gesture/value/time/lock scenarios. Focused45 PASS (color5/editor1/runtime39), typecheck0/lint/diffcheck PASS after a new-command syntax error was corrected. Timeline/MCP color family, native local continuous playback, color-layer kind and export remain incomplete. Physical browser color gesture remains unqualified; global parity incomplete.

## Этап 272: agent/MCP color key writes and readback

Added atomic set_color_key and delete_color_key batch operations using validated text color commands. RGBA requires a strict four-channel byte object; scalar values and extra fields refuse. Creation uses composition playhead ticks; deletion preserves exact authored track ticks, including negative keys retained by trim. get_color_track returns constant RGBA or detached keyed RGBA with native clocks, timeline/local microsecond times, resolved temporal tangents, outgoing segments and exact matching preset IDs. MCP catalog advertises the read tool and strict color write schemas. Frontend focused45 PASS (runtime40/color5), typecheck0/lint/diffcheck PASS; backend MCP5 PASS including final catalog assertions for RGBA schema bounds and signed delete ticks. Runtime regression qualifies dry-run without mutation, two keys in one Undo entry, exact Undo/Redo, delete/Undo, atomic rollback after an invalid channel and scalar-color refusal. These are runtime/catalog checks; physical browser-to-MCP color integration remains unqualified. Full color tangent/preset agent family, timeline color editing, native local continuous playback, color-layer kind and export remain incomplete. Source-shaped upstream APIs and global feature parity remain incomplete.

## Этап 273: agent color presets, extrapolation and tangent commands

Added set_color_preset (track default or exact signed key tick), set_color_extrapolation (both sides/five modes), set_color_tangent_mode and edit_color_tangents through existing validated color authoring commands. MCP batch catalog exposes these operations with preset enum, side/mode enums, signed authored ticks and bounded temporal X coordinates. Color Auto retains scalarless identity rules; manual points freeze Free and preserve opposite sides. Extended runtime regression applies four commands in one Undo entry, reads back PingPong and outgoing(.42,-.4)/Broken, proves invalid X rolls back the entire batch and exact Undo restores prior keys. Focused45 frontend tests PASS, typecheck0errors/0warnings, lint, MCP5 and diffcheck PASS. This qualifies runtime commands and MCP catalog, not physical browser-to-MCP color integration. Explicit custom interpolation and key move/value agent operations, timeline color editing, native local continuous preview, color-layer kind and export remain incomplete. Global parity remains incomplete.

## Этап 274: agent color key time/value edits

Added edit_color_key through validated updateTextColorKey, with optional authored newTick and strict RGBA byte value. Both existing and new ticks support signed retained track times. Empty edits refuse, collisions fail full document admission, key metadata remains attached to the moved key and batches retain one Undo transaction. MCP catalog exposes signed clocks and bounded four-channel values. Extended runtime regression moves the middle color key to -100000 while changing green alpha64, checks preserved outgoing(.42,-.4)/Broken, exact Undo, collision refusal and empty-edit refusal without mutation. Focused45 frontend tests PASS; typecheck0errors/0warnings, lint, backend MCP5 and diffcheck PASS. Physical browser/MCP integration remains unqualified. Explicit custom interpolation agent command, timeline color editing, native local continuous preview, color-layer kind and export remain incomplete; global feature parity remains incomplete.

## Этап 275: explicit color interpolation through agent/MCP

Added set_color_interpolation for inherited track default or exact authored signed key tick. Uses existing validated color configuration and temporal handle baking; RGBA values never enter scalar slope calculations. MCP schema exposes nine simple curves, bounded-X/unbounded-Y Bezier pairs and Elastic direction/amplitude>=1/period>0. Extended runtime checks read back custom Bezier outgoing(.2,-.3)/incoming(.8,1.4), exact Undo and Elastic period0 refusal without document mutation. Focused45 frontend tests PASS; typecheck0errors/0warnings, lint, MCP5 and diffcheck PASS. These checks qualify runtime/catalog; real browser-to-MCP color integration remains unqualified. Timeline color editing, native local continuous preview, color-layer kind, animated color export and global feature parity remain incomplete.

## Этап 276: consistent color Auto normalization on project load

normalizeComposition now applies scalarless color normalization to admitted text colorAnimation after document validation and visual scale normalization. Saved Auto sides adjoining Bezier resolve identity timing coordinates on load just as after editor writes; Free coordinates and Smooth metadata remain unchanged. Regression loads stale Auto coordinates, verifies packed identity incoming/outgoing, idempotent second normalization and untouched raw input; separately verifies hand-authored Free/Smooth data survives. Focused87 PASS and full frontend785 PASS/121files; typecheck0errors/0warnings, lint and diffcheck PASS. Native core behavior and physical browser integration are not newly qualified here. Timeline color editing, native local continuous preview, color-layer kind, animated color export and global parity remain incomplete.

## Этап 277: real browser/MCP color qualification

Extended isolated Chromium/MCP runner with real color batch before existing broad editor scenarios. Freshly built backend, real authenticated HTTP MCP session and mounted editor qualify dry-run without mutation, two RGBA keys, custom Bezier(.2,-.3)/(.8,1.4), PingPong readback, one Undo entry, rollback after invalid256 channel following deletion and exact Undo. VIDEO_KADR_E2E_COLOR_ONLY=1 runs this targeted scenario and still executes normal finally cleanup; default runner includes it before remaining scenarios. Actual targeted Chromium run PASS, backend build PASS with existing warnings, JS syntax/diffcheck PASS. This proves browser bridge commands/readback, not physical color inspector gestures, native color rendering or continuous playback; full broad MCP runner was not rerun here. Timeline color editing, fast local native preview, color-layer kind, animated color export and global parity remain incomplete.

## Этап 278: real Rust HTTP color seek preview in Chromium

Extended targeted actual MCP/browser qualification with seek to750000us between red/blue keys on custom Bezier color curve. Captures actual animated-color HTTP200 response, checks mixed red/blue RGBA, waits for visible authored text layer and compares computed CSS color to independently browser-normalized native response RGBA. No sampler mocking or JS color interpolation. Existing atomic/dry-run/rollback/exact Undo checks remain, playhead resets to0 before remaining broad scenarios. Targeted actual Chromium run PASS after final playhead cleanup, diffcheck PASS. This qualifies native-to-rendered color at one seek time, not continuous-playback latency, all native golden cases in browser or physical inspector curve gestures. Timeline color editing, local native preview, color-layer kind, animated color export and global parity remain incomplete.

## Этап 279: shared product color core compiled to WASM

Added independently buildable preview-core Rust cdylib using the exact backend animated_color.rs source by path, so native HTTP and WASM share admission, OkLab blending, fractional clocks and legacy compatibility logic. Bounded2MiB JSON upload ABI uses scalar bytes and resident output with no unsafe host pointers. Installed wasm32-unknown-unknown target and built release artifact243654bytes. Native shared-module3 tests PASS; actual Node WebAssembly instantiation passes unchanged external fixture10cases/18samples within one channel byte plus invalid-channel refusal. Reproducible check-color-wasm.mjs added; generated target files ignored, Cargo lock retained. JS/diff checks PASS. This is a validated executable core, not connected browser preview: JSON byte upload is not yet resident-track/per-frame optimized, browser loading/bundling and playback timing remain pending. Timeline color editing, color-layer kind, animated color export and global parity remain incomplete.

## Этап 280: resident color track ABI for per-frame native sampling

Added PreparedColorTrack to shared product Rust domain: existing HTTP admission validates once, native keys are prepared once, subsequent safe integer times call exact same color evaluator without JSON parse or repeated key validation/native vector construction. WASM color_prepare owns one resident track; color_at returns status and color_packed yields RGBA bytes in one u32, avoiding per-frame output JSON. Failed prepare clears previous resident track. Actual WASM checker compares resident results byte-exact to existing batch output for all10golden cases/18samples and refuses NaN/Infinity/fractional/unsafe times. Native golden regression compares prepared and batch paths too; shared native3 PASS, WASM release249927bytes/check PASS. Browser loader, multi-layer residency and synchronous preview integration remain pending, so no continuous-playback performance claim. Timeline color editing, color-layer kind, animated color export and global parity remain incomplete.

## Этап 281: browser WASM color loader and independent prepared instances

Added prepareColorPreview typed browser adapter loading Vite-url WASM asset with cached compiled module and retry after failed load. Each prepared track owns its own instance; bounded upload happens once and subsequent synchronous time→RGBA calls use native scalar ABI. Added reproducible locked build:preview-core script and generated249927byte module asset. Actual Chromium runner prepares authored custom color curve, compares750000us WASM RGBA byte-exact to actual HTTP Rust response, prepares a separate constant green instance and verifies neither track overwrites the other. Targeted Chromium PASS, typecheck0errors/0warnings, lint and diffcheck PASS. Loader/API exists but CompositionPreview still uses HTTP; synchronous render integration, instance lifecycle/memory budgets and playback performance qualification remain pending. Timeline color editing, color-layer kind, animated color export and global parity remain incomplete.

## Этап 282: synchronous WASM color in product preview

CompositionPreview now prepares authored text color tracks via local Rust WASM instead of per-seek HTTP calls. Preparation effect depends on preview document data, not playhead; prepared samplers evaluate local time synchronously during style rendering. Separate nonreactive promise/result maps prevent preparation subscribing to its own result writes; stale async preparation ignores changed document, removed/changed curves prune instances. Pending new curves hide until prepared; Free/native packed semantics unchanged. Mounted18 PASS checks one preparation across seek, current-time sample after delayed preparation, transient candidate controls/Cancel and synchronous subsequent seek. Initial reactive-loop failure was fixed before qualification. Full frontend785 PASS/121files, typecheck0errors/0warnings, lint/build/diffcheck PASS; production asset249.93KB/gzip94.42KB. Actual Chromium passes color rendering matching explicit HTTP Rust reference and asserts only that one deliberate reference request occurs, with no preview HTTP sampling. Continuous-playback throughput, large-project instance memory, physical curve gestures and overall bundle-budget gate remain unqualified. Timeline color editing, color-layer kind, animated color export and global parity remain incomplete.

## Этап 283: physical playback with per-frame native color checks

Actual Chromium scenario clicks product Play/Pause controls and observes requestAnimationFrame playback until750000us. For every observed frame, compares displayed text style to an independently prepared native WASM color sample at current product playhead, checks visibility and advancement, and verifies no preview/playback color HTTP requests (one explicit HTTP reference remains). Run PASS:91observedframes/758ms, every color correct/visible. Initial test-only bare Svelte import failed to resolve in page evaluation; replaced with microtask flush before final passing run. These observations qualify short one-layer continuous playback on this host, not guaranteed frame-rate throughput, CPU load or multi-layer/dense-track performance. Global parity remains incomplete: timeline color editing, large-project instance memory, color-layer kind, animated color export and other upstream inventory gaps remain open.

## Этап 284: dense color track measurement and prepared fast path

Added reproducible bench-color-wasm.mjs with4096linear keys, once-only upload,1000warmups and five10000sample runs/checksum. Found prepared sampling repeatedly scanned all keys for legacy compatibility. PreparedColorTrack now caches whether any legacy segment exists and directly invokes pinned eval_precise for ordinary tracks; compatibility track retains original exact branch. Same-host measured before34.675–36.915ms/10000samples, after13.511–14.678ms; identical checksum. Preparation12.35ms after vs13.03ms before is observation, not promised improvement. WASM linear memory3342336bytes for this dense instance; per-layer instances remain a scaling risk. Actual WASM goldens10cases/18samples PASS and native3 PASS including prepared-vs-batch legacy alpha/fractional-clock check. Updated shipped WASM250392bytes; lint/diffcheck PASS. Multi-track shared-instance residency/lifecycle, broad playback load, timeline color editing, color-layer kind, export and global parity remain incomplete.

## Этап 285: shared WASM residency and explicit track lifecycle

Added native multi-track ID prepare/sample/drop/count ABI retaining strict admission and independent prepared records in one BTreeMap. Failed replacement invalidates only its own ID; repeated drop is safe and removed sampling refuses. Actual WASM test512independent constant tracks PASS with memory1179648→1179648bytes, full golden10cases/18samples still PASS; this is sparse-track allocation, not512dense tracks. Browser loader now shares one WASM instance, allocates monotonic IDs and exposes idempotent dispose with FinalizationRegistry fallback. Product preview explicitly disposes curves on replacement/removal and all remaining tracks on component cleanup, including pending preparation completion; no reliance on GC for owned lifecycle. Mounted18 PASS includes exactly two releases on draft/cancel and third on unmount. Chromium shared-instance playback PASS90observedframes/750ms, all native colors visible/correct and no preview HTTP. Typecheck0/lint/diffcheck PASS before final test-only disposal assertions; targeted tests rerun after assertions PASS. New WASM271419bytes. Large dense multi-layer memory/load, timeline color editing, color-layer kind, animated color export and global parity remain incomplete.

## Этап 286: reachable native color keys on the timeline

MultiTrackTimeline now mounts typed TimelineColorKeys rows for keyed text colors. Diamonds carry authored RGBA/time, project through exact track timeBase→composition clocks, seek on click and support ArrowLeft/Right one rounded frame, Shift ten frames, Delete/Backspace. Existing validated color commands preserve curves and enforce locks/collisions; each action commits one Undo. Keyboard move restores focus to moved diamond. Mounted53 timeline tests PASS including new1000Hz color100→133 tick movement on30fps composition, unchanged hex alpha, click100000us, focus continuity, exact move/delete Undo and remaining other key. Typecheck0/lint/diffcheck PASS (lint before final focus/test addition). This adds display/keyboard actions; physical browser color timeline, drag/multi-selection/copy/paste and graphical color-value curve remain incomplete. Color-layer kind, animated color export and global parity remain incomplete.

## Этап 287: transactional color key drag and real browser mouse qualification

TimelineColorKeys now captures source document/clock/zoom/pointer on primary drag with3px threshold. Marker position and validated candidate canvas color update transiently; no history/document writes until release. One validated move commits on release; Escape, pointer cancel/lost capture, stale document and collisions refuse. Owner token isolates candidate cleanup from other curve gestures; drag click suppression avoids accidental seek after release. Mounted57timeline PASS includes commit150tick, Escape, collision200tick and concurrent canvas edit preservation with exact Undo/history checks. Actual Chromium mouse moves500000→750000us, checks get_project unchanged during held pointer, native readback750000 after release, one Undo and exact document restoration. Existing real color playback/bridge checks also PASS90frames/750ms. Typecheck0/lint/diffcheck PASS before final test-only/browser assertions. Multi-key color selection, scaling/copy/paste and value-curve graph remain incomplete; color-layer kind, animated color export and global parity remain incomplete.

## Этап 288: atomic multi-color-key clock edit command

Added moveTextColorKeys over explicit clipId/tick/newTick subjects, signed safe authored clocks and4096subject admission. Builds all resulting per-layer tracks together so swaps avoid transient collisions; preserved RGBA/segment/tangent metadata travels with keys, final sort/admission rejects collisions. Duplicate subjects, missing keys/layers and locked targets refuse; document remains immutable and all resulting patches form one batch. Agent/MCP move_color_keys advertises strict subject records. Pure regression qualifies signed two-key swap, collision/duplicate/missing-target rollback; agent runtime checks two-key swap readback and exactly one Undo with exact restoration. Focused46 frontend PASS, typecheck0/lint/MCP5/diffcheck PASS before final test-only runtime additions; focused46 rerun after additions PASS. Multi-selection UI wiring, cross-track selected gestures, scaling/copy/paste, color-layer kind, animated color export and global parity remain incomplete.

## Этап 289: selected color groups in timeline keyboard and drag

TimelineColorKeys adds local per-timeline-track Shift toggle selection with pressed/visual state. Arrow edits translate selected keys together using each authored timeBase; group Delete submits all removals in one batch. Pointer drag captures selected subjects/timeBases, previews all marker positions and validated native canvas candidate, then commits one atomic multi-key move; no-op release refuses extra history. Selection follows successful moved ticks; stale/deleted selection filters out. Mounted58timeline tests PASS including two-key Shift selection,133/233frame nudge,150/250group drag with immutable held document, group deletion, one-entry Undo and exact restoration. Typecheck0 before final test/no-op additions, lint/diffcheck PASS after; tests rerun after additions PASS. Selection is local to a timeline track, not unified scalar/color or cross-track selection; physical multi-color mouse gestures, scaling/copy/paste remain incomplete. Color-layer kind, animated color export and global parity remain incomplete.

## Этап 290: cross-track color selection and shared drag projection

Color selection moved into composition UI state with standard reset and repair of deleted/locked/nonkeyed subjects. Timeline rows derive selected subjects across all text layers, so group arrows, deletion and captured drag use each track's own timeBase across timeline tracks. Shared owner/source-guarded colorKeyDragPreview publishes transient authored destinations to every row; other-track diamonds now move during held drag, not only on commit. Mounted59timeline PASS covers1000Hz/48000Hz cross-track selection,133/6400one-frame movement,233/11200drag, correct other-row projected pixel position before release and two exact Undo restorations. Full frontend793 PASS/121files, typecheck0errors/0warnings, lint and diffcheck PASS. Physical cross-track color gestures, unified scalar/color selection, scaling/copy/paste remain incomplete. Color-layer kind, animated color export and global parity remain incomplete.

## Этап 291: Alt time scaling of selected color groups

Color drag captures Alt scaling mode, first selected composition-time pivot and anchor span. Positive factor scales selected absolute composition times and converts back to each layer-local authored clock, preserving RGBA/timing metadata; nonpositive/invalid factors refuse candidate and release. Pivot anchors and single-key/first-pivot gestures refuse with an explanatory UI message. Ordinary moves retain previous semantics. Mounted62timeline PASS includes successful100/200→100/300, Escape/no mutation, negative-factor refusal and cross-track1000Hz/48000Hz scale100/4800→100/52800 with one Undo and exact restoration. Actual Chromium physically Shift-selects500000/1000000, Alt-drags second to1250000, confirms held document unchanged, pivot500000 retained, one Undo and exact restoration. Existing actual playback/bridge/drag checks PASS90frames/750ms. Typecheck0/lint PASS before final test-only cross-clock/browser additions; focused62 rerun after additions PASS, diffcheck PASS. Color copy/paste, unified scalar/color gestures, color-layer kind, export and global parity remain incomplete.

## Этап 292: detached color-key clipboard domain commands

Added copyColorKeys snapshot grouped by source layer without source IDs in the stored groups. Copies RGBA/temporal metadata, materializes inherited segment interpolation, normalizes offsets against earliest selected composition time and deeply detaches curve arrays. pasteColorKeys maps explicit target layers, converts composition offsets to authored target clocks, merges/replaces coincident existing keys, rejects copied-key collisions after quantization and applies all resulting layer patches atomically. Returns pasted subjects for UI selection. Domain1 PASS qualifies1000Hz500/1000→composition0/500000→target750/1250, alpha/Bezier preservation, detached interpolation, duplicate-copy refusal, missing single/multiple-target rollback and source immutability. Typecheck0/lint/diffcheck PASS before final test-only detachment assertions; domain test rerun after additions PASS. Like existing scalar clipboard, copied times round to integer composition microseconds. Keyboard/editor clipboard slot, target mapping UI and actual browser copy/paste remain pending. Color-layer kind, animated color export and global parity remain incomplete.

## Этап 293: shared editor color copy/paste shortcuts

Extended the single CompositionClipboardSlot with color-keyframes and routes composition.copy/paste to detached color commands when color selection is active. Targets use selected color-layer order or selected clips; command availability enforces matching group count/text kinds/locks. Color diamonds honor configured shortcut bindings/capture mode and intercept successful copy/paste; text fields retain normal input shortcut behavior. Paste selects inserted subjects and commits once. Mounted shortcut regression verifies100/200copy has no history/document change, playhead500000µs paste lands500/600ticks and exact Undo. Focused76 PASS; full frontend798 PASS/122files, typecheck0/lint/diffcheck PASS after a temporary Set lint issue was corrected. Actual Chromium physically Ctrl+C/V copies two selected keys, pastes1500000/2000000µs with red alpha128/custom curve retained, one Undo and exact restoration; existing color playback/drag/scale checks PASS91frames/758ms. Multi-group destination-order UX, unified scalar/color selection and key marquee remain incomplete. Color-layer kind, animated color export and global parity remain incomplete.

## Этап 294: streaming native export color frame sequence

Added ColorExportFrames over shared PreparedColorTrack, streaming frame/time/RGBA without allocating a full render-length sample array. Frame count ceil and frame timestamps nearest microsecond use u128 rational arithmetic, support fractional FPS and validate safe duration/nonzero rate/count overflow. Native regression matches all20fps/30000:1001samples byte-exact to batch sampler with signed fractional key clocks, including expected last950000/967633µs, invalid rate refusal and empty duration. Shared-core4 and backend-domain4 PASS; diffcheck PASS. This is render-frame preparation, not export integration: text wire, command resource/graph and pixel qualification remain pending; authored color export remains explicitly refused. Live default /opt/homebrew/bin/ffmpeg reports Unknown filter 'drawtext', so text-render qualification needs a supported renderer/binary. This is a scoped qualification limitation, not a global task blocker. Color-layer kind and global feature parity remain incomplete.

## Этап 295: native drawtext command stream and actual renderer probe

ColorExportFrames writes bounded-memory sendcmd records with strict ASCII target admission and native RGBA samples. Command timestamps floor rational frame time while sample timestamps round nearest microsecond; regression checks 30000/1001 second command at0.033366,30records and injection refusal. Shared-core4 PASS. Installed keg-only Homebrew ffmpeg-full9.0.2 without changing default FFmpeg/PATH; actual binary supports drawtext fontcolor runtime commands. Actual two-frame30000/1001 transparent-RGBA probe changes red to blue on the correct second frame, but alpha128 renders pixel(0,0,127,63), versus expected straight RGBA(0,0,255,128); alpha255 red is exact. This pixel qualification FAILED and identifies transparent drawtext blending as unresolved before export integration. Writer is preparation only; export refusal retained. No production export correctness claim, global parity remains incomplete.

## Этап 296: separate foreground alpha commands and pixel regression

Native command writer now sends opaque RGB to drawtext@target and alpha gain to colorchannelmixer@target at the same floored rational timestamp. Its documented graph contract requires an isolated foreground, so background/stroke/shadow alpha must not receive this gain. Shared-core4 PASS before final assertion strengthening. Added scripts/check-text-color-render.py with explicit FFmpeg/font arguments: actual ffmpeg-full three-frame30000/1001 probe PASS for red255, blue128, green0 exact straight-RGBA interior values; checks every antialiased pixel alpha proportional within1byte and zero-alpha frame entirely transparent. Probe uses representative command syntax, not a product export job or direct Rust-generated command resource. Wire/resource/foreground-style separation and full product render qualification remain pending; authored color export refusal retained. Global feature parity remains incomplete.

## Этап 297: actual renderer consumes native generated color resource

Added backend/preview-core/tests/color_render.rs external-renderer integration test, explicitly ignored in portable default suite and invoked with VIDEO_KADR_COLOR_FFMPEG/VIDEO_KADR_COLOR_FONT. Uses the product ColorExportFrames writer to create a private0600 command file, cleans temporary directory on success/panic, launches actual FFmpeg and compares all30 rendered glyph-interior RGBA pixels byte-exact against shared native frame sampler. Signed fractional keys−0.5/1000000.5µs, alpha128→255 and fractional30000/1001 clocks PASS across every frame. This closes the manual-syntax-vs-generated-resource gap from stage296. Explicit invocation1 PASS, diffcheck PASS. Graph remains isolated foreground only; no product wire/resource integration or styled-background/stroke/shadow qualification yet. Export refusal retained and global parity remains incomplete.

## Этап 298: optional native text color wire and admission

Backend TextClip adds optional camelCase colorAnimation carrying strict native ColorTrack, omitted when absent for legacy wire compatibility. Color key/track/legacy-curve types now serialize and compare for document round-trip. Composition validation admits tracks through PreparedColorTrack, preserving shared4096key/finite-safe-clock/tangent/easing rules. Frontend WireTextClip and payload map authored color via existing packColorAnimation transport. Export availability gate still prevents frontend animated-color requests; compiler explicitly refuses direct backend animated-color export pending command resources, so accepted domain data cannot silently render as static color. Model regression preserves−0.5/1000000.5µs, alpha128, PingPong/Offset through round-trip; rejects256 channel at serde admission and reversed key clocks at composition admission; legacy static wire omits field. Domain21 PASS after final regression, typecheck0errors/0warnings, lint PASS. Constructor fixtures updated without global reformatting. Product resource generation, graph style separation, export job qualification and global parity remain incomplete.

## Этап 299: product temporary color command resource preparation

CompositionTextResource adds optional color_commands_file. PreparedTextResources creates native ColorExportFrames commands for each visible/enabled animated text layer using exact canvas FPS and duration converted from composition clock with u128 ceil. BufWriter generation runs in spawn_blocking; create_new UUID file receives0600 at creation on Unix. Worker owns a cleanup guard returned into resource owner, so errors/dropped worker result delete its file; existing resource Drop removes text while owned guards remove color resources. Lifecycle regression verifies static absence,30 native commands with exact alpha128 gain,0600 permission and deletion after owner Drop. Lifecycle1 PASS after test expansion; default text constructor resource updated. Compiler still explicitly refuses animated color until graph integration; no actual export job claim. Foreground/stroke/shadow/background separation, capability checks, resource-error/cancellation stress qualification and global parity remain incomplete.

## Этап 300: compiler foreground graph and per-clip command targets

Native color_command_target encodes every clip-ID byte into ASCII hex with color_ prefix, so distinct/arbitrary IDs produce collision-free sendcmd filter names. Product resource writer and graph compiler share this name; commands cannot broadcast across different animated text clips. Animated text compiler now requires prepared color resource, renders decorations with transparent glyph fill separately from opaque foreground RGB + native alpha gain, overlays foreground over decorations, then applies shared transform/opacity/timeline offset. Static clips reject unexpected color resources. Compiler regression checks missing-resource refusal, named drawtext/mixer, separate decorative vs foreground options, overlay and sendcmd. Focused backend text11 PASS includes updated resource lifecycle and legacy static golden. Actual ffmpeg-full representative64x64 decorated graph parses/renders16384RGBA bytes PASS; this is syntax/runtime feasibility, not styled pixel equivalence or a product-generated full export job. Existing drawtext antialias/decorative-alpha behavior is not independently qualified; frontend animated-color export gate remains until full product render/capability checks. Global parity remains incomplete.

## Этап 301: animated-color filter capabilities and failing product pixel reproduction

Active visible/enabled animated text requires drawtext/sendcmd/colorchannelmixer/overlay at backend capability validation; lifecycle regression rejects missing filters and admits complete set1 PASS. Added explicitly ignored actual_compiled_animated_text_graph_renders, requiring explicit external renderer/font variables. Calls product text_clip_filter with compiled visual transform/opacity and native-generated command file, renders30frames64x64 with opaque green background/stroke/shadow. FFmpeg successfully renders expected byte count but pixel assertion FAILED: maximum-red pixel is(128,0,0,64), expected red128 over opaque green(128,127,0,255). This invalidates styled export correctness despite earlier representative graph syntax pass. Separate raw overlay diagnostic preserves alpha127 over transparent for rgb/auto, so the defect is not established as generic overlay failure; investigate full named-command/decoration/transform chain. Test remains an explicit failing external qualification reproduction, portable suites omit it by #[ignore]. Frontend export gate retained. Product full-job render and global parity remain incomplete.

## Этап 302: corrected styled pixel oracle with every-frame checks

Investigation revises stage301 diagnosis: raw isolated foreground renders(255,0,0,128), decorated composite contains(128,0,0,255). Black stroke covers glyph interior beneath translucent red fill; expecting green beneath that region was an incorrect test oracle. max_by_key(red) returned a tied antialiased edge(128,0,0,64), not proof of interior alpha being squared. Standalone decorated30frame recreation confirms interior opacity preserved through overlay/scale/opacity. Product-generated graph regression now separately renders two fixtures: no stroke/shadow expects(128,127,0,255) over green, decorated expects(128,0,0,255) over black stroke; both require the appropriate interior pixel on EVERY30frames and untouched opaque green background presence. Explicit external test1 PASS(60renderedframes total), diffcheck PASS. No production graph change was needed for this diagnosis. Antialias edge equivalence, changing-color/cross-layer command isolation, translucent decoration qualification and complete export job still pending; frontend gate retained and global parity incomplete.

## Этап 303: changing color and alpha in actual compiled fractional-FPS graph

Strengthened actual_compiled_animated_text_graph_renders from constant track to redalpha128→bluealpha255 with signed fractional native keys−0.5/1000000.5µs and exact canvas30000/1001 FPS. Native sample and command streams use matching rational rate. Every rendered frame checks the SAME fully covered glyph pixel chosen from the first frame, against native sampled color composited over opaque green (plain) or opaque black stroke (decorated). Per-channel tolerance1 accounts for FFmpeg integer blend rounding; opaque untouched green background remains. Explicit external1 PASS,60renderedframes, diffcheck PASS. This qualifies changing RGB/alpha through product text graph/transform/opacity, not full composition job, all antialias pixels, cross-instance isolation or translucent style blending. Those checks, delivery integration and global parity remain incomplete; frontend export gate retained.

## Этап 304: full compiled MP4 delivery and frontend export admission

External renderer regression now synthesizes a64x64 fractional-FPS FFV1 primary source, builds the COMPLETE product composition command with native text resources, launches actual FFmpeg, verifies nonempty H.264 MP4, decodes resulting video and checks30frames plus red first-frame/blue last-frame at the same text interior pixel. Existing60raw compiled-graph frame checks retained. Initial fixture admission correctly rejected inconsistent fps_milli30000 vs30000/1001; fixed fixture to29970. Explicit external1 PASS including full delivery/decode. Removed frontend unconditional animated-color export refusal, exempts already-validated text colorAnimation from unknown-keyframes gate, preserves strict payload packing/native backend admission/filter capability checks. New frontend regression adds valid primary video and asserts export available plus exact packed RGBA/native track in wire;7color tests PASS and typecheck0errors/0warnings after correcting test fixture VisualTransform fields. Full frontend799 PASS/122files, final lint/diffcheck PASS. This enables product render requests; live HTTP job/actual deployed binary, cross-layer isolation, translucent decoration/antialias parity and global feature parity remain incomplete.

## Этап 305: rebuilt backend actual HTTP color export job

Rebuilt backend binary with current domain/resource/compiler/capability changes(cargo build PASS,5preexisting warnings). Added scripts/check-text-color-http.py requiring explicit drawtext-enabled FFmpeg path, isolated temporary storage/library and ephemeral loopback port; process-only PATH points to chosen renderer directory. Script synthesizes primary FFV1 source, POSTs strict native animated text composition to actual /api/compositions/render, polls specific /api/jobs/id until done, locates MP4, decodes30frames and checks same-pixel red→blue delivery; last pixel(41,1,231). HTTP/job/render/decode PASS and no composition-color temporary command files remain after job. Finally terminates backend and removes test storage. No user storage/default shell PATH changes. This qualifies actual HTTP resource preparation and worker/runner delivery on ffmpeg-full9.0.2; default installed FFmpeg still lacks drawtext and correctly cannot provide this render capability. Cross-layer isolation, translucent decorations/full antialias fidelity, live frontend export interaction and global feature parity remain incomplete.

## Этап 306: two text instances retain independent commands through HTTP delivery

HTTP export qualification adds two simultaneously visible text layers with separate IDs/positions/native command resources: left redalpha128→bluealpha255, right native keyed constant opaque green. Uses deep-detached fixture metadata. Actual rebuilt backend job done, MP4 decode30frames PASS; same left pixel red→blue(last55,32,227 after lossy encoding/chroma subsampling), same fully covered right pixel remains green>220/redblue<20 on EVERYframe. This checks named command isolation through actual product resource/graph/runner path, not merely target string uniqueness. No color temporary resources remain. diffcheck PASS. Broader overlapping-instance/timing/curve tests, translucent styles/full antialias equivalence, live frontend export interaction and global feature parity remain incomplete.

## Этап 307: native animated solid-color render stream

Re-inspected pinned upstream apps/desktop/src/main/state/commands.ts prodColorParams and native/src/state/layer.rs ColorParams: true Color kind carries Animated<Rgba> plus width/height, defaults black at composition size. Added native ColorExportFrames::write_solid_commands streaming four named colorchannelmixer gains(rr/gg/bb/aa) over opaque white RGBA; sendcmd precedes mixer so commands apply current frame, rational command time floors while native sample rounds. Explicit external renderer test now additionally renders2x2solid30frames30000/1001 with signed fractional red128→blue255 track and checks EVERYpixel RGBA byte-exact against native samples; text renderer checks retained, external1 PASS. Portable regression checks30records/floored0.033366/four channels/target injection refusal. Re-exported target-name helper in shared crate. This is native rendering preparation for Color kind, not a completed product Color layer: authored/wire model, inspector/timeline/preview/commands/MCP and delivery integration pending. Global parity remains incomplete.

## Этап 308: strict source-free native SolidColorClip model

Added SolidColorClip native domain primitive with stable ID, authored timeline span, explicit rectangle width/height, byte RGBA, optional shared native ColorTrack, full TransformSpec/AnimatableValue opacity and enabled flag. Strict camelCase serde denies unknown fields including sourceId, legacy constant serialization omits colorAnimation. validate admits1..3840x1..2160 dimensions including odd dimensions, positive safe tick span, shared transform/opacity and0..1opacity range, native track admission with base color as default. Dedicated regression1 PASS covers constant/keyed round-trip(signed fractional−0.5µs, PingPong/Offset,alpha128), omitted optional field,256alpha/unknown-source refusal, zero/oversize dimensions, zero-span,opacity1.1/NaNkey refusal. diffcheck PASS. Model is not yet a CompositionTrack variant: wire routing/compiler/resources/source-requirements and frontend kind/UI/commands remain pending. Color feature and global parity remain incomplete.

## Этап 309: native color track routing and composition invariants

Added strict CompositionTrack::Color with stable track ID/name/hidden/locked/solid clips and kind=color serde discriminator. Composition validation counts visible layers/clips, validates every solid clip, enforces global clip-ID uniqueness and output extent; duration_ticks includes color spans, track id accessor handles new kind. Source requirement collection explicitly treats Color as source-free. Domain regression confirms standalone color-track composition round-trip, kind=color,1.5Mduration, no media sources, ID accessor and duplicate rejection. Native compile initially identified two exhaustive matches(duration/source requirements), both implemented; model1 PASS after integration, diffcheck PASS. Active visible/enabled solid layers explicitly refuse current service/command planning until color compiler integration, preventing silent dropped output. Hidden/disabled remain persisted. Source-free primary rendering, product resource/compiler support and frontend kind/UI/MCP remain pending; global parity incomplete.

## Этап 310: native static solid overlay compiler integration

VisualClip adds Color with ID/span/transform/opacity, normal blend and no source ID. visual_slice includes visible enabled Color tracks, sorts intervals, preserves layer order and primary-video rule. Overlay compiler uses solid intrinsic width/height for transform, source-free dispatch, no media input pads or video masks/reverse/speed workloads. Service render validation admits static solids and validates visual bounds/nonoverlap/end within primary. solid_clip_filter generates opaque-white RGBA rectangle, byte color mixer gains, shared transform/opacity/timeline placement; animated solids explicitly refuse pending command resources. cargo check PASS. Explicit external actual_static_solid_shape_and_opacity_render1 PASS checks full composition command accepts solid and contains native63x31 source, then actual compiled solid graph renders EVERYpixel(12,34,56,64) with coloralpha128 and opacity0.5, including odd dimensions. diffcheck PASS. Animated solid resources, actual full static-solid HTTP delivery, source-free primary composition support and frontend/UI/MCP pending; global parity incomplete.

## Этап 311: animated solid command resources and actual HTTP delivery

CompositionExportCompileRequest adds separate color_commands map by stable clip ID; legacy helper signatures delegate with empty map to new layer-resource-aware compiler, integration fixture requests updated. PreparedTextResources additionally owns solid command map and guards, creates private0600 native solid stream in blocking worker for visible enabled keyed solids, default base RGBA/exact FPS/local duration, cleans on owner/worker-result Drop. Compiler requires exact active keyed-solid resource IDs/nonempty paths, routes sendcmd before uniquely named RGBA mixer, retains transform/opacity/timeline chain; removes service animated-solid refusal. Backend capabilities require sendcmd/colorchannelmixer/overlay for active animated solids. cargo build PASS. HTTP qualification --solid renders native keyed green32x24solid beside changing text through current backend job/MP4 decode;30frames green independent, textred→blue(last56,36,229), resources removed PASS. Service24 PASS. Existing composition_render integration target compiles with --no-run after updating two older text/color-resource fixtures; this does not claim all integration tests ran. diffcheck PASS. This is actual solid track/resource/worker delivery; tested solid track has one constant-valued key, changing-solid values already independently qualified by shared stream stage307. Frontend kind/UI/MCP and source-free primary composition rendering remain pending; global parity incomplete.

## Этап 312: frontend solid parameters, immutable edits and native wire transport

Added solidColor.ts SolidColorParams intrinsic rectangle model(factory defaults black at canvas dimensions), immutable edit and native transport adapter. Shape admission matches native1..3840/1..2160 integers, RGBA hex canonicalizes to byte-alpha form. Exposed existing shared validateColorAnimation helper, reuses full keyed admission/normalization and detached temporal metadata; clearing animation removes wire field.2tests PASS cover defaults,63x31alpha128,invalid shape/color, signed keys/customBezier/PingPong/Offset, detached input-curve mutation, duplicate-key refusal and clearing track. Typecheck initially caught readonly test mutation cast, corrected via deliberate unknown cast; lint PASS. This is frontend intrinsic data/edit/transport foundation, not yet main CompositionClip/Track union or reachable editor functionality. Main validation/commands/timeline/preview/inspector/MCP still pending; global parity incomplete.

## Этап 313: main frontend Color clip/track integration

Added real ColorClip/ColorTrack to main composition unions and strict native wire track. Main validation admits intrinsic bounded odd-sized RGBA rectangles and shared colorAnimation, normalizes persisted defaults, validates shared native transforms and currently limits solid blend to normal. Payload emits source-free solid fields and native color track; visual scale uses intrinsic rectangle dimensions. Generic track replacement/cloning, timeline time-grid/productivity, split/trim and detached duplication handle color; key tracks rebase across cuts. Source-dependent templates/audio/media tools explicitly exclude solids; timeline/inspector/search label them by color. Preview prepares text and solid native color tracks together and renders a transformed solid background using native sampled color. Added validated createSolidColorClip factory and main-model regression covering persistence, split/trim signed-key rebasing, detached duplication, primary-video render admission and exact native wire without sourceId.3solid tests PASS; full frontend802 PASS/123files; typecheck0errors/0warnings, lint and diffcheck PASS. Persistence test canonicalizes document before comparing normalized JSON defaults. No new mounted/live-browser solid preview qualification or Undo regression was performed. Reachable solid creation/parameter inspector, generalized color-key editor/MCP, common state transform controls and source-free primary composition remain pending; global feature parity incomplete.

## Этап 314: reachable solid creation and parameter inspector with history

Inspector now exposes + Цвет, creating a selected source-free black canvas-sized solid at playhead on an available color track or a new top track. Uses shared validated factory and a single commitDocument mutation. Added immutable updateCompositionColorParams through shared strict admission; inspector edits RGBA plus intrinsic width/height separately from visual transform. Common state opacity/rotation/rectangle transform controls now admit solids. Inspector visual placement/scale controls use solid intrinsic dimensions as their source size, preserving media source lookup for other layers and normal-only solid blending. Mounted UI regression clicks creation, checks kind/source-free metadata and playhead, edits RGBA, verifies Undo/Redo, edits63×31 intrinsic shape/position/opacity, and verifies invalid hex retains the exact document. Inspector43 PASS; full frontend803 PASS/123files; final typecheck0errors/0warnings after adding explicit CompositionClip generic to test flatMap; lint and diffcheck PASS. Test-only generic fix has no runtime change. Native animated-color editor/key commands/MCP still text-specific; physical-browser solid preview/export interaction and source-free primary rendering remain unqualified/pending. All-feature parity remains incomplete.

## Этап 315: solid color animation authoring and timeline/clipboard admission

Shared existing color commands now admit TextClip or ColorClip with appropriate base RGBA, preserving historical function names for callers. Solid layers use the same native track normalization, key edits/deletion, curves, extrapolation, tangent mode/handle edits and atomic multi-key moves. Clipboard copy/paste admits both kinds; operation updates normalize solid colorAnimation as text. Inspector exposes shared CompositionColorEditor for solids, timeline color-key lanes collect and label solid tracks, global color selection pruning retains unlocked solid key subjects. Agent get_color_track now reads solid fallback/keys; native evaluator unchanged. Added solid command regression for seeded black→redalpha128, Bezier/auto tangents, copy/paste, atomic move and detached unchanged originals. Extended mounted solid inspector test to create a color key and Undo/Redo its track.47focused PASS before final UI assertion, full804 PASS/123files; after selection-pruning correction inspector/timeline83 PASS, final typecheck0errors/0warnings, lint and diffcheck PASS. Initial typecheck caught timeline text-only labels and was repaired with kind-specific label. Physical-browser solid key gestures/preview/export and direct MCP runtime tests remain unqualified; nested compositions, Motifs and remaining upstream inventory still incomplete.

## Этап 316: mixed text/solid timeline gestures and command/agent qualification

Mounted timeline regression selects text and solid color keys together, moves both by ArrowRight frame in a single commit, verifies both selected DOM keys/subjects retain their updated times and one Undo restores the exact prior document. Found remaining commandRegistry paste admission restricted to text despite shared clipboard support; now admits both text/color tracks and updates rejection guidance. Registry regression verifies solid-key copy, paste availability/execution, landed selection and Undo removes only pasted key. Added direct agent runtime regression creating a63×31source-free solid via apply_batch, reading native black RGBA fallback, setting alpha128 red key, reading seeded/keyed native values and Undo restoring exact original. Timeline41 PASS, registry/runtime49 PASS, full frontend807 PASS/123files, typecheck0errors/0warnings, lint/diffcheck PASS. This proves mounted DOM keyboard/selection, command boundary and in-process agent tool behavior; physical browser pointer gestures, network MCP transport and live preview/export are distinct pending checks. Global inventory still incomplete.

## Этап 317: mounted solid preview geometry and alpha qualification

Added stable data-color-clip address on preview solid DOM layer and a mounted component regression creating a source-free63×31RGBA rectangle, setting shared transform126×62/x128/y72/rotation30/opacity0.5, then checking actual preview DOM background, opacity,60%center positions, canvas-relative dimensions and rotation with no missing-source message.19preview tests PASS, typecheck0errors/0warnings, lint/diffcheck PASS. First assertion assumed browser canonical rgba() serialization, but test DOM preserves valid8digit hex; corrected expectation to emitted#12345680. This qualifies mounted static preview geometry/style, not screenshot/pixel compositing or changing-color native-WASM/browser/export parity. Full frontend807tests last passed stage316; only targeted preview suite rerun for this isolated DOM/test addition. Remaining upstream feature inventory stays open.

## Этап 318: animated solid preview seek/curve lifecycle

Parameterized native-color preview lifecycle regression across text and real63×31solid layers. Uses mocked prepared native sampler boundary, confirms one preparation despite seeking while pending, local500000µs sampling yields RGBA128/0/128/128, temporary Bezier curve packs native handles and updates preview, cancellation restores original track/color, seek0 samples red, unchanged persistent document and all3samplers disposed on replacement/unmount. Production solid visualStyle now hides layer while keyed native sampler is pending, matching text behavior.20preview PASS; full frontend809 PASS/123files, typecheck0errors/0warnings, lint/diffcheck PASS. This is mounted lifecycle/transport proof with mocked sampler, not live WASM pixel/browser qualification. Backend/frontend source-free composition admission still requires primary video, independently verified by current services/validation; global inventory incomplete.

## Этап 319: source-free canvas compiler foundation

Compiler visual_slice now treats a bottom nonvideo visual layer or absent primary as canvas-backed composition, collecting all visible tracks as overlays; existing bottom-video path remains. Empty-primary command duration comes from composition duration, avoids indexing primary[0], and existing full-duration primary gap generates canvas background plus silence/concat base. Added source-free63×31solid fixture clearing all sources/video tracks, verifies compiled command has no -i, solid source, background gap, single base concat and anullsrc. cargo check PASS, focused1 PASS; full composition compiler unit suite PASS (external ignored tests remain omitted); diffcheck PASS. Service render admission and frontend export admission still require primary video, so source-free export is not yet reachable through product API/UI. Actual source-free graph render, zero-duration rejection and audio-only semantics require qualification before opening those gates. Global parity incomplete.

## Этап 320: actual source-free solid MP4 delivery

Added explicit external actual_source_free_solid_mp4_delivery regression requiring VIDEO_KADR_COLOR_FFMPEG. Clears all sources and media tracks, creates opaque-red63×31solid with1s timeline over64×64canvas, calls complete product command compiler with empty input map, executes chosen FFmpeg, verifies nonempty MP4, decodes RGB24 and checks red center on EVERYframe (at least20frames). Explicit ffmpeg-full test1 PASS including actual render/decode, diffcheck PASS. This proves compiler background+silence/overlay/encoding path can deliver without input files; does not qualify animated source-free colors or product HTTP/frontend admission, which remain gated on primary video. Global feature parity incomplete.

## Этап 321: service admission for canvas-backed compositions

Service renderability chooses bottom enabled video primary when present; otherwise uses full composition duration as canvas-backed base, validates all tracks as overlays and preserves source/transform/keyframe/audio budgets. Empty zero-duration document explicitly refused. Added source-free delayed63×31alpha128solid regression: valid layer accepted, empty document and opacity2 refused. Service25 PASS (prior24alsoPASS), diffcheck PASS. Compiler actual source-free MP4 qualification exists stage320. Frontend gate remains primary-video-specific; backend executable not rebuilt this stage and actual HTTP source-free job remains unverified. Global feature inventory remains incomplete.

## Этап 322: reachable canvas-backed HTTP export and frontend admission

Rebuilt backend and extended isolated HTTP qualification --canvas-only to remove all composition sources/video tracks while retaining independently animated text and green solid. First actual POST exposed handler empty-media refusal; removed redundant refusal after existing strict request/output/service validation. Second job exposed runner filesystem scope requiring input; admits empty inputs for filter_complex carrying generated primary_gap_0 canvas, retains filesystem output/path collection. Added scope admission/refusal regression1 PASS. Rebuilt binary actual HTTP job done, MP4/decode30frames, animated text red→blue and independent green solid plus temporary command cleanup PASS. Frontend export admission now validates canvas-backed visual/audio layers against authored document duration when bottom primary video absent; preserves per-layer admission/keyframe budgets, refuses zero-duration. Solid test asserts export available with empty sources; two older tests updated from obsolete bottom-image/text-only refusal expectations. Full809 PASS/123files, typecheck0errors/0warnings, lint/diffcheck PASS. Static source-free product compiler pixel proof stage320 retained. This qualifies actual HTTP source-free keyed-text/constant-keyed-solid export, not all animated-solid curves, live UI export clicks or complete upstream parity.

## Этап 323: canvas runner admission and zero-duration boundary

Tightened inputless runner scope admission to require a filter chain beginning color= and ending generated[primary_gap_0], rather than marker substring alone. Regression refuses referenced-input null chain, nullsrc marker and unrelated color output; actual canvas source admitted. Compiler explicitly refuses zero-duration canvas before graph generation; regression PASS. Full backend lib run451PASS/80FAIL/3ignored: new scope/duration, composition compiler and service tests pass; failures include legacy edit/args fixtures missing required fadeIn, so overall backend verification is RED and requires repair rather than claiming full pass. Rebuilt backend and repeated actual HTTP --solid --canvas-only job/render/decode30frames/independent green/temporary cleanup PASS under tighter scope; diffcheck PASS. Global parity remains incomplete, including full-suite legacy failures.

## Этап 324: legacy edit request defaults and full backend qualification

Diagnosed stage32380failures: optional neutral EditRequest float fields used custom finite deserializers without serde default, forcing fadeIn/fadeOut/pan/brightness/sharpen/grain presence; optional fps similarly forced field presence. Added default only for omission, preserving custom finite validation and explicit null/string/object refusal for mandatory numeric values. Extended defaults regression and invalid explicit values regression. Full library532 PASS/3ignored after repairs; complete cargo test including binaries/integration/property/API/actual renderer suites exit0 PASS, external explicitly ignored tests remain distinct. diffcheck PASS. This restores legacy minimal request compatibility, not a fixture-only workaround or admission weakening. Full upstream feature inventory still incomplete.

## Этап 325: changing solid color through source-free HTTP delivery

Extended isolated HTTP qualification --changing-solid (requires --solid) with native two-key green→yellow solid beside independently red→blue animated text, without video sources via --canvas-only. Decodes30MP4frames and checks solid center: green>220/blue<20 everyframe, red starts<20/ends>220, monotonic tolerance5 for lossy decode and more20distinct red values; existing text endpoint and temporary file cleanup retained. Actual HTTP job/render/decode PASS. First check picked a boundary pixel by scan and observed blue21 from chroma mixing beside text; corrected oracle to fixed solid interior(48,32), keeping original color thresholds. No production interpolation/encoder change. This qualifies changing solid RGB through native command resource/compiler/worker/inputless runner delivery; full curve/alpha/pixel equivalence and remaining upstream inventory stay open. diffcheck PASS.

## Этап 326: native nested composition dependency foundation

Re-inspected pinned upstream CompositionRefParams: child composition ID plus source in/out, layer timing, transform, animated opacity/blend; root cannot be referenced. Added strict serialized native CompositionGraph(root,references) with complete graph admission and deterministic children-first order, shared children emitted once. Rejects missing root, dangling child, root references, cycles including disconnected saved subgraphs; bounds1024nodes/4096edges/64nesting levels and IDs to prevent unbounded recursion/work.3native tests PASS covering shared DAG/serde roundtrip, invalid references/cycles and depth overflow; new file rustfmt and diffcheck PASS. This is dependency foundation only; CompositionRef layer storage, clocks, group authoring, flatten and nested preview/export remain pending. No claim of nested feature completion or full upstream parity.

## Этап 327: native CompositionRef source timing foundation

Added strict CompositionReferenceTiming with child ID, parent placement and source in/out microseconds, unit-rate timeline end, half-open active source-time mapping and detached split preserving exact source offsets/end clocks. Authored source overhang retained, matching inspected upstream tolerance; child duration sampling/clamping remains planner work. Safe wire clock bounds and checked additions refuse invalid/overflow timing. Native4graph/timing tests PASS including nonzero source offset, boundaries, split continuity, preserved parent/child clocks, invalid edge splits and overflow; rustfmt/diffcheck PASS. This is a timing primitive awaiting actual CompositionRef clip/project storage, visual parameters, flatten and render integration. No nested feature completion claim; global parity remains incomplete.

## Этап 328: persisted native nested project model

Added strict versioned NestedCompositionProject storing root ID and child document map; each document carries existing fully validated Composition plus CompositionInstance list with stable ID, reference timing, shared TransformSpec/AnimatableValue opacity and BlendMode. Dependency graph is derived from actual instances, avoiding separately persisted stale topology. Validates schema, microsecond clocks, instance count/identity uniqueness, source timing and shared visual/value validation before graph admission. Exposed TransformSpec validation only crate-wide for reuse.2native tests PASS cover project serde equality, derived child-before-parent order, retained visual/timing values, missing referenced document, duplicate instance and NaN anchor refusal; rustfmt/diffcheck PASS. Model not yet integrated into existing persistence endpoints/main clip union/UI; opacity render-specific range/budget, combined media/instance IDs/order, flatten and nested export remain pending. Global parity incomplete.

## Этап 329: native nested frame clock planning

NestedCompositionProject frame_plan validates complete dependency graph then resolves active half-open CompositionRef source times recursively. Emits composition/time/unique instance path plus ancestor instance records, preserving compositing group boundaries instead of distributing opacity/blend across children. Bounded4096active frame entries limits repeated DAG instance expansion. Native regression checks two nested source offsets: root800000→child400000→leaf900000, group/inner path, retained ancestor opacity and inactive parent boundaries.3nested project tests PASS; rustfmt/diffcheck PASS. Planner currently reports authored overhang source time; child-duration blank/clamp policy, concrete media draws, group surfaces, HTTP/persistence/UI integration and actual nested render remain pending. Global parity incomplete.

## Этап 330: native nested-frame API boundary and frontend transport

Added strict POST /api/compositions/nested-frame accepting nested project and timeUs, bounded2slot blocking-worker planning via native project graph/clock evaluator. Response serializes entries with sourceTimeUs/instancePath/ancestor visual records. Handler regression verifies actual async native response JSON and missing-root refusal; nested4tests PASS. Added typed frontend NativeNestedProject/instance/frame response and evaluateNestedFrame transport using existing wire composition/transform/opacity types, AbortSignal and requestJson. Typecheck0errors/0warnings, lint/diffcheck PASS. Direct handler test does not establish actual socket route invocation; server binary rebuild/live HTTP and frontend adapter request tests pending. Persistence, reachable nested editor, group surface rendering/flatten and global parity remain incomplete.

## Этап 331: actual nested-frame HTTP qualification

Rebuilt backend and extended isolated real HTTP script with nested-frame route qualification using strict serialized root/child/leaf project. Actual POST200 verifies clocks800000→400000→900000, outer/inner path and two ancestor records; half-open parent end1500000 yields root entry only. Adding disconnected/leaf→child cycle returns400 with cycle error. Existing motion/position/color live HTTP checks also PASS; script terminates backend/removes temporary storage. diffcheck PASS. This establishes socket route/extraction/native worker/serialized response and rejection on current binary; project storage/UI/native group surface rendering remain pending and full feature parity incomplete.

## Этап 332: SQLite nested project document storage admission

Extended existing composition-project storage document/source-index validation to recognize strict native nested projects, validate graph/instances, union all child media IDs deterministically, reject conflicting metadata for shared source IDs, and enforce existing source/index/document bounds. Uses existing atomic SQLite project/source transactions and revision infrastructure. Added database create/read equality for nested root document and empty source index; missing root rejects before write. DB7tests PASS including existing update/source atomicity/reopen regressions; diffcheck PASS. Initial compile caught missing BTreeMap import, corrected. HTTP save parser, archive import/export/remapping and frontend save/load still assume flat documents and remain pending; this is database admission, not complete nested persistence UI/API. Group authoring/render and global parity incomplete.

## Этап 333: nested project HTTP save parser admission

HTTP composition project body parser now recognizes nested documents and reuses exact database source-index/graph validator via crate-local export, retaining flat legacy parser/messages and name/size/mode/revision handling. Regression preserves nested document and baseRevision7, empty source index, rejects missing root. New parser1 PASS, priorHTTP2 PASS, API integration56 PASS, diffcheck PASS. Integration suite regression pass is not a newly authored nested save/read/socket test; direct nested HTTP persistence qualification, frontend save/load DTO union, archive nested-source handling, group editor/render still pending. Global parity incomplete.

## Этап 334: authenticated nested project router persistence and transport

Added actual-router integration through isolated SQLite/auth bearer account: POST nested root+child CompositionRef project, GET exactdocument, PUT group opacity with current revision, stale PUT409, dangling-child PUT400 and subsequent GET unchanged stored document.1new integration PASS. Added separate typed frontend nested project DTO/save request and create/update/get helpers against existing revision-protected endpoints, preserving fixed schemaVersion2/modecomposition envelope and encoded IDs/bearer auth. Typecheck0errors/0warnings, lint/diffcheck PASS. Router oneshot test is not socket persistence test; frontend functions have no new mock transport regression yet. Main editor selection/load/UI, archive remapping and nested group render remain pending; global parity incomplete.

## Этап 335: nested archive source discovery and relinking

Archive manifest admission recognizes nested projects, reuses strict document/graph/source-index validator and gathers deduplicated sorted borrowed source IDs across all child contents. Import relinking validates complete unique global mapping, recursively replaces each child content source keys/embeddedIDs/clip sourceId using its local mapping, revalidates nested result; final project source index derived by shared validator. New regression checks shared image source relink in root and child, unchanged root ID and incomplete mapping refusal. Archive unit8 PASS and existing archive HTTP integration3 PASS; diffcheck PASS. Existing integration is flat-document roundtrip, not newly qualified full nested archive roundtrip. Nested archive media/instance preservation end-to-end, editor/group rendering and full upstream parity still pending.

## Этап 336: complete nested archive HTTP roundtrip qualification

Extended existing real-router archive roundtrip harness with nested root+child native composition carrying actual WAV source/audio clip and group instance. Exports .veproj, imports to isolated team space using actual local probes, verifies new source ID in child registry and clip, unchanged full root instance timing/transform/opacity/blend, media bytes/library metadata/storage space and staging cleanup. Existing flat roundtrip retained. Archive integration4 PASS; exact nested test rerun with --nocapture1 PASS (no missing-tool skip), diffcheck PASS. First rerun filter typo selected0tests; corrected explicit filter and inspected actual1test result. This qualifies nested archive export/import/validation/relink/metadata path; reachable nested UI/native group render/flatten and remaining upstream inventory still pending.

## Этап 337: immutable native whole-document grouping

Added native group_document wrapping all parent media tracks/existing instances in a new child composition and replacing parent with same-canvas empty content plus identity/opacity1/normal CompositionRef spanning authored duration. Rejects existing child, missing parent, empty duration and invalid identity via complete result graph admission. Native4tests PASS including unchanged original, preserved child document, root/grouped/child800000/800000/400000clocks and outer/group paths. Added bounded blocking-worker POST nested-group endpoint returning edited project; cargo check PASS, rustfmt new files/diffcheck PASS. Operation groups the whole document; selected track/layer grouping, interleaved ordering, UI history integration, socket endpoint qualification, child surfaces/render and global parity remain pending.

## Этап 338: nested grouping live API and frontend transport

Added typed groupNestedComposition frontend transport carrying native project/parent/child/instanceId and AbortSignal without client-side clock rewriting. API32tests PASS including new exact request/response/signal regression; typecheck0errors/0warnings, lint/diffcheck PASS. Rebuilt backend, actual HTTP nested-group creates child matching authored parent, subsequent frame-plan yields800000/800000/400000/900000and new-group/outer/inner path; existing child ID returns400. Live script PASS with existing native path/color checks. Initial raw JSON equality differed only serialized canvas default backgroundMode/backgroundBlur; canonicalized input fixture to include those defaults, retaining full equality check. This qualifies whole-document group API; selected-layer grouping, editor history/navigation/group surfaces/render and global parity remain pending.

## Этап 339: explicit nested document duration

Native NestedCompositionDocument gains optional persisted durationUs with exact-wire bound; omitted duration derives media/nested-instance ends. Project durations_us exposes validated duration map. Whole-document grouping uses declared/derived parent duration, preserves pinned duration in moved child and sets parent reference source range accordingly. Frontend native project wire admits optional durationUs. Native6tests PASS including derived1.5M→pinned750000 override, preserved pin on grouping, correct sourceOut and overflow refusal; frontend typecheck0errors/0warnings, rustfmt/diffcheck PASS. Frame-plan overhang still reports authored child time rather than final blank/clamp draw policy; UI duration editing, sampled ancestor poses, concrete group surfaces/render and full parity pending.

## Этап 340: native sampled group poses with instance clocks

Nested frame entries add serialized ancestorPoses containing each group's localTimeUs/x/y/scale/rotation/opacity. Shared native keyframe sampler evaluates scalar properties; native compiled position evaluator handles XY/path position. Instance local clock is parent frame minus instance start, separate from child source offset; opacity samples clamp0..1 for draw usage. Retains authored ancestor transforms/anchors/blend alongside poses, preserving group boundaries. Native7tests PASS including animated x/opacity0.3 atlocal300000whilechildsource400000, multi-level local clocks and handler wire response; frontend response type/typecheck0errors/0warnings, rustfmt/diffcheck PASS. Concrete group surface rendering, full render-specific transform/key budgets, overhang draw policy, reachable editor and global parity remain pending.

## Этап 341: native nested canvas affine geometry

Added native GroupAffine mapping child pixel coordinates into parent canvas using canvas sizes, normalized anchor, sampled offset/scale/rotation; composable affine matrices retain nested transforms and reject nonfinite composition overflow. Nested frame plan derives serialized canvasToRoot from complete ancestor chain, alongside separate pose/opacity/blend records rather than prematurely distributing group compositing. Geometry regression verifies rotated/scaled center anchor, origin anchor, two-level matrix vs sequential mapping and invalid zero-sized canvas. Targeted group3PASS (includes existing cgroup test), nested7PASS, frontend typed6coefficient matrix/typecheck0errors/0warnings, rustfmt/diffcheck PASS. This is geometry planning, not group-surface raster/render qualification. Editor/group order/selected grouping/overhang and remaining upstream inventory still incomplete.

## Этап 342: native RGBA group surface primitive

Added bounded straight-RGBA8 GroupSurface with normal premultiplied source-over composition, inverse affine nearest-neighbor sampling and parent canvas clipping. Applies opacity once to a finished group surface; regression distinguishes this from independently fading overlapping children. Invalid dimensions/buffer/opacity/matrix/singular or overflowing inverse reject before mutation; nonfinite mapped samples are skipped. Two focused tests PASS covering group opacity, translucent color preservation, translated clipping and unchanged destination on invalid matrices; rustfmt/diffcheck PASS. Standalone CPU primitive only: not yet connected to nested frame media drawing, preview or export. Other blend modes, filtering/antialiasing, nested layer ordering, editor integration and global parity remain incomplete.

## Этап 343: all group surface blend modes

Inspected upstream native state/transform.rs BlendMode enum (Normal/Multiply/Screen/Overlay/Darken/Lighten/Add/Difference). Native GroupSurface now accepts corresponding existing composition BlendMode values, including Addition for upstream Add; composite_normal delegates to generic compositor. Uses alpha-aware source-over with blend applied only in overlapping coverage, preserving source color over transparent backdrop. Three focused tests PASS including all eight opaque pixel reference results and transparent backdrop at group opacity0.5, prior group boundary/clipping/refusal regressions; rustfmt/diffcheck PASS. This remains a standalone surface compositor; no nested media drawing/preview/export integration, high-quality filtering or linear-light color management qualification. Full upstream parity remains incomplete.

## Этап 344: recursive native nested surface assembly

NestedCompositionProject render_surfaces now connects per-document media-renderer callback to recursive GroupSurface assembly: local source clocks, per-child completed surface, sampled instance pose/opacity/blend and affine mapping. Current document model draws flat content first then instances in authored vector order; unified media/instance interleaving remains pending. Root/child times at or beyond declared/derived duration return transparent canvas without invoking media renderer (source-overhang blank policy); zero opacity/scale skip invisible instances, canvas mismatch rejects, active recursion bounded4096. Eight nested tests PASS including new pixel-level callback fixture root800000/child400000, group alpha128, child endpoint overhang and root endpoint blank. Initial1x1 fixture correctly rejected existing even>=2 canvas admission; corrected to2x2. rustfmt/diffcheck PASS. Callback fixture is not actual decoder/text/solid media rendering or reachable preview/export endpoint; those integrations and remaining upstream parity are incomplete.

## Этап 345: real rendered-movie RGBA decoder bridge

Added group_media decode_rendered_frame using explicit FFmpeg executable, integer-derived six-decimal local timestamp, exactly one raw RGBA frame and bounded stdout read (canvas bytes+1), dimension/time/file admission and child reap on excess/read failure. Empty successful EOF yields transparent canvas. render_project_frame connects composition-ID→pre-rendered local movie mapping to recursive group surface assembly at each native child source clock. Actual FFmpeg fixture generated2x2red rawvideo NUT and decoded at500000us, verified red/opaqueRGBA and blank after2sEOF; test1PASS with --nocapture, no missing-tool skip, rerun after bridge compile1PASS; rustfmt/diffcheck PASS. No service authorization/scheduling/cancellation/timeout or movie pre-render/cache integration yet; bridge consumes already-rendered files, not original media tracks. No full nested movie/pixel end-to-end test or reachable preview/export path yet. Alpha-capable source generation, arbitrary upstream layer interleaving and global parity remain incomplete.

## Этап 346: actual nested movie assembly pixel qualification

Added end-to-end native bridge fixture writing two real raw-RGBA NUT movies through FFmpeg stdin, with changing child green→half-alpha blue→opaque blue and opaque red parent. Constructs persisted native root/child project with start500000/sourceIn500000/groupopacity0.5, decodes actual movies through render_project_frame and recursively assembles surfaces. Pixel assertions prove root-only red before child, source offset selecting half-alpha blue (191/0/64/255) at root500000, opaque blue group (128/0/128/255) at root1s, root endpoint transparent and missing movie mapping refusal. Both real FFmpeg tests2PASS with --nocapture (no tool skip), rustfmt/diffcheck PASS. Qualifies raw alpha/source clocks/compositing bridge for fixture only; production pre-render lifecycle, arbitrary content render, process timeout/cancellation, UI/preview/export delivery and remaining full parity still incomplete.

## Этап 347: bounded cancellable frame decode process

Decoder now defaults to30s deadline; controlled entry accepts explicit timeout and shared AtomicBool cancellation. Bounded reader thread drains one frame while controller checks cancellation/deadline at10ms intervals, polls process exit and kills/reaps child on timeout/read/size/error; joins reader before returning. Pre-cancel/zero timeout reject before spawn. Unix regression uses real exec sleep5 fake decoder,50ms timeout and checks completion<2s, plus pre-cancel missing executable proves cancellation precedes spawning. Group media3tests PASS incl real movie/nested alpha qualification, rustfmt/diffcheck PASS. Midflight cancellation not separately qualified; caller project-level cancellation propagation, production scheduling/pre-render lifecycle and preview/export remain pending. Reader join assumes decoder itself owns stdout (normal FFmpeg); descendant-inherited stdout robustness not qualified. Full parity incomplete.

## Этап 348: shared nested-frame decode deadline and cancellation

render_project_frame now delegates to controlled project entry with single30s deadline. Explicit controlled entry forwards shared AtomicBool and remaining overall time to each child decoder, checks preflight/per-media/post-assembly cancellation and deadline, so child count cannot reset decode timeout. Extended actual Unix subprocess regression to flip cancellation after50ms while exec sleep5 decoder is running, verifies cancelled error and completion<2s; existing timeout/pre-cancel and real alpha movies retained. Group media3tests PASS with --nocapture, rustfmt/diffcheck PASS. Surface CPU loops themselves are not yet interrupted per row, so cancellation is observed around media callbacks/final return rather than throughout rasterization; UI/service caller wiring, production pre-render scheduling and global feature parity remain incomplete.

## Этап 349: raster-row cancellation through nested assembly

GroupSurface composite_checked accepts caller checkpoint, checks before mutation and each destination row; normal/generic legacy compositors retain no-op defaults. Native project render_surfaces_checked carries checkpoint through recursive children and base/group composition, while production movie bridge uses same shared deadline/AtomicBool for raster as decoder. Partial working surfaces on checkpoint failure are explicitly discard-only; project returns error and never exposes an incomplete frame. Regression cancels before second row and proves first composed/second untouched. Group-filter11PASS (includes one unrelated cgroup test), nested8PASS, rustfmt/diffcheck PASS, real movie fixtures retained. Graph validation, allocations and keyframe preparation still do not checkpoint internally; UI/service scheduling, original-content pre-render/cache, preview/export and full parity remain incomplete.

## Этап 350: authenticated rendered-output nested preview API

Added POST /api/compositions/nested-preview accepting native project/timeUs/composition-ID→rendered-output-UUID mapping. Reuses existing mandatory bearer session resolver, per-output access check and server output-file resolver; rejects unknown composition/invalid UUID/inaccessible output, never accepts client filesystem paths. Shared two-worker semaphore bounds blocking assembly,30s overall deadline and CancelOnDrop signal interrupt abandoned handler work. Returns raw RGBA bytes with content-type/width/height and no-store headers. Exposed existing publish auth/path helpers crate-wide without changing their behavior. cargo check PASS; real router unauthenticated preview regression1PASS (401), rustfmt new handler/diffcheck PASS. Authenticated output decode/pixel HTTP path and disconnect cancellation not yet qualified; frontend transport/UI, automatic original-media pre-render lifecycle and export remain pending. This endpoint consumes existing authorized outputs, whose alpha support depends on format; no claim of complete nested editor or upstream parity.

## Этап 351: authenticated nested preview pixels and binary client

Real-router test generates red/blue RGBA NUT movies with actual FFmpeg, grants owner access, POSTs native groupopacity0.5 preview, verifies2x2width/height/no-store headers and16RGBAbytes near128/0/128/255. Stranger and revoked-child requests return404. Backend preview2tests PASS --nocapture without toolskip. Added renderNestedPreview frontend binary transport with bearer/project/time/output mapping/AbortSignal, bounded dimension and exact RGBA byte-size admission, Uint8ClampedArray result and retained aborted-fetch error. FrontendAPI33PASS includes exact request/auth/signal/pixel response and truncated buffer rejection; initial empty fixture failed existing render-payload admission, corrected solid clip fixture. Typecheck0errors/0warnings before final test fixture addition, diffcheck PASS. Router test is not network socket/browser UI/disconnect qualification. Main editor wiring, automatic original-layer rendering/cache and nested export remain incomplete; full upstream parity open.

## Этап 352: nested RGBA canvas preview component

Added NestedCompositionPreview Svelte component accepting native project/local root time/rendered-output mapping/session, displaying binary native RGBA via canvas ImageData. Reactive changes abort prior request, suppress stale responses, clear outdated pixels, report loading/errors and dispose request on unmount; no animation math copied into JS. Full frontend typecheck0errors/0warnings, diffcheck PASS. Inspected current CompositionPreview/data/projects: main editor still owns flat document only. Component is not mounted by main editor yet; no browser canvas/abort race visual qualification. Existing project load union, nested navigation/history, automatic original-content rendering, preview delivery and export remain incomplete; full upstream parity open.

## Этап 353: mounted nested project preview workspace

Existing saved-project open detects native compositions/root document before flat normalization, opens separate reactive nested session preserving current flat draft identity/media. Main workspace mounts native canvas preview, root time input and per-composition selection from existing output library; flat inspector/timeline/tools unmounted for nested session. Save uses revision-protected nested API/name; return/new/flat-open close session, nested name and archive target reflect selected session; existing flat remote refresh paused during nested view. Native output mapping remains ephemeral and manually selected. Frontend typecheck0errors/0warnings, diffcheck PASS. Browser/runtime open/save/selection/canvas lifecycle and state regression not yet qualified; whole editor nested editing/navigation/undo, automatic child media rendering, metadata refresh/deletion support and nested export remain incomplete. Full parity open.

## Этап 354: nested open/save isolation and revision-conflict regression

Added state regression opening actual native-shaped nested DTO via existing saved-project load, preserving byte-identical flat draft and its identity, saving through nested API atrevision7 and checking revision8 adoption, no flat create/update calls, conflict retention/document/error/busy reset and close preservation. Successful nested save now applies returned canonical name/document and updates project list metadata. State40PASS. Combined workspace run exposed unavailable Node localStorage fallback returningtrue after user disabled import preference; fixed session fallback and test cleanup optional storage access. Final state+workspace51PASS2files; earlier typecheck0errors/0warnings, diffcheck PASS. Native canvas browser UI flow and nested editing/history/auto-pre-render/export still unqualified/incomplete; full parity open.

## Этап 355: mounted native whole-composition grouping with Undo/Redo

Nested workspace now invokes native group API for entire root composition using fresh child/instance UUIDs, exposes Undo/Redo and operation errors. Reactive history holds document+rendered-output mapping snapshots (max100past), clears stale output bindings on edit and restores them with Undo. Generation+document identity guard discards late responses after close/open/concurrent changes; rejected operation retains document/history. Open/close reset history, grouping/save buttons mutually disabled and save function refuses during native edit. History tests2PASS (transport/state fixtures, not native graph renderer), combined state42PASS, final typecheck0errors/0warnings, diffcheck PASS. Whole-document grouping only, no selected layers/interleaved order/browser interaction qualification; automatic media re-render, nested timeline/navigation/flatten/export and full upstream parity incomplete.

## Этап 356: lossless native nested instance split clocks

CompositionInstance gains optional persisted animationOffsetUs (default0 omitted), keeping authored animation origin through splits independently of child source clock. Native immutable split_instance splits timing at interior parent timestamp, retains left ID, adds adjacent right ID, accumulates animation offset, validates whole result including duplicate/empty IDs and exact-wire animation endpoint bounds. Pose sampling for scalar/XY/path uses offset local clock; existing unsplit wire unchanged. Regression double-splits animated group, compares source time/ancestor pose/affine before and after across six boundary/interior samples, preserves original, rejects endpoint/duplicate ID and verifies serde roundtrip. Nested9PASS, frontend optional field/typecheck0errors/0warnings, rustfmt/diffcheck PASS. Native domain operation only; split endpoint/UI/history integration and remaining nested editor/render/export/global parity incomplete.

## Этап 357: native split HTTP/history/workspace integration

Added bounded POST nested-split worker and typed splitNestedComposition transport; workspace exposes per-root-instance split at current cursor. Shared edit-history transaction serves grouping and split, retains generation/document stale guard and one Undo snapshot; split retains rendered movie mappings because flat child content is unchanged. Router1PASS verifies adjacent sourceOut/sourceIn400000, right animationOffset300000 and endpoint400. History3PASS verifies split transport args/one edit/retained mapping/UndoRedo along existing native group/stale failure regressions. cargo check PASS, frontend typecheck0errors/0warnings, rustfmt new handler/diffcheck PASS. No socket/browser button test or advanced nested timeline selection/navigation; selected-layer grouping/unified ordering, automatic content render, nested export and global upstream parity remain incomplete.

## Этап 358: child-composition navigation and focused native preview

Workspace active composition selector switches local cursor to0, previews selected document, routes grouping/split to selected parent. Native render_surfaces_from validates original project graph/root but renders selected existing document subtree, excluding external ancestors rather than rewriting graph root. Optional preview compositionId crosses component/client/API/movie bridge; omitted remains original root. Session open initializes focus, close clears, Undo to document without active child falls back root. Native focused fixture proves only child callback400000/opaque blue, unchanged project root, missing focus refusal; nested10PASS. Frontend API/history36PASS, typecheck0errors/0warnings, rustfmt/diffcheck PASS. Focused HTTP pixels/navigation browser flow not yet qualified; source clocks on entering child reset to0 (no instance-path seek yet), nested timeline/content editing, automatic rendering and export/full parity remain incomplete.

## Этап 359: focused HTTP pixel and canvas lifecycle qualification

Extended actual-router real FFmpeg preview fixture to focus child with only child output mapping: opaque blue pixels prove external parent0.5opacity excluded, missing focus400, existing root mix/access refusals retained. Preview HTTP2PASS no toolskip. Frontend API33PASS includes optional compositionId request propagation. New mounted Svelte DOM component2PASS checks native RGBA copied into ImageData/putImageData, child focus passed, AbortSignal triggered on disposal and late response after unmount never draws; canvas implementation mocked, not physical browser visual proof. Typecheck0errors/0warnings, diffcheck PASS. Navigation/Undo UI browser behavior, complete source-content automatic render, nested editing/flatten/export and full upstream parity still incomplete.

## Этап 360: automatic native generated-color content preview

Added flat_surface render_generated for native color-background/generated solid compositions, using shared PreparedColorTrack evaluator, instance pose sampler and affine/group RGBA compositor. Uses authored topmost-track reverse order, enabled half-open clips, scalar/XY/path transforms/opacity/alpha and row checkpoints; ignores audio/hidden media for visual drawing, returnsNone for visible image/video/text or non-color canvas backgrounds. Movie bridge tries native generation when composition has no output mapping; explicit movie mapping retains priority. Solid alpha/endpoint pixel regression1PASS, cargo check PASS, rustfmt/diffcheck PASS. This expands automatic source-free nested content without claiming all media renderer parity: image/video/text/effects/masks/filtering, full nested UI/export and other upstream features remain incomplete. Existing no-output empty-canvas movie fixture expectation updated from missing-file refusal to native black canvas; broader group media rerun separately recorded next.

Stage360 follow-up: group_media3PASS including both actual FFmpeg/movie assembly fixtures and decoder cancellation/timeout after automatic generated-content fallback.

## Этап 361: generated nested API end-to-end and broad regression pass

New real-router fixture disables advertised FFmpeg/tools and supplies no rendered movies: native red/blue child layers plus animated groupopacity return exact purple RGBA at root1s; POST split at800000 retains identical final frame; focused child opaque blue qualified via same HTTP renderer. Test1PASS no toolskip. Removed unused mut in wrapper. Broad backend lib558PASS3ignored and fullAPI61PASS; fullfrontend817PASS125files. First frontend run816PASS1FAIL due importPreferences persistence test directly accessing unavailable Node localStorage; replaced host-dependent storage with explicit isolated Map-backed fixture and restored original descriptor, reran all817PASS. diffcheck PASS. These suites do not prove browser UI, original video/image/text automatic renderer, nested audio/export, full upstream asset/feature coverage or global parity; goal remains incomplete.

## Этап 362: native image-layer surface and bounded PNG decoding

flat_surface render_generated_images accepts caller-resolved image decoder callback, renders native Image tracks in authored stack with half-open timing, shared scalar/XY/path transform/opacity pose and all supported blend modes. Existing generated-only entry continuesNone delegation for visible images without decoder. Added decode_image_surface using image reader PNG support with3840x2160dimension/33MBallocation limits, preserves intrinsic size and RGBA. Real PNG fixture alpha128 blue over red gives127/0/128/255, endpoint returns red and never calls decoder; flat surface2PASS, cargo check/diffcheck PASS. Test name's authorized callback means caller-supplied path only, not actual permission proof: HTTP/library/Space authorization/resolution not wired yet. JPEG/WebP/SVG/media effects and full source renderer/nested export/global parity incomplete.

## Этап 363: authorized automatic library PNG nested preview

Preview handler collects enabled visible image source IDs for documents without explicit output overrides, deduplicates and resolves existing library source paths after shared Space authorization/relocation; client still supplies no filesystem paths. Native movie bridge receives trusted image map and automatically runs generated+image flat surface callback with shared deadline/cancellation checks. Real-router fixture with disabled FFmpeg/tool flags writes PNG/library metadata, previews exactalpha127/0/128/255 with no rendered movies, saves native document to private Space, proves stranger403 and owner unchangedpixels after physical source isolation. Test1PASS, groupmedia3PASS actual movie/cancel regressions, cargo check/rustfmt/diffcheck PASS. Authorization checks all enabled visible document images even if subtree/time inactive; later admission/per-frame source pruning remains. PNG decode itself checkpoints around operation only; image cache/otherformats, originalvideo/text and nested export/full parity incomplete.

## Этап 364: active focused-frame image authorization pruning

Native frame_image_sources validates project/focus/time, walks actual source clocks/half-open active child refs with4096instance cap, clips to document duration, omits invisible group opacity/scale and disabled/hidden/inactive/zeroopacity/zeroscale image layers; output override skips local image requirements while retaining active descendants. Preview handler authorizes/resolves exact resulting image IDs rather than all authored images. Flat image renderer samples invisible pose before decoding, aligning with source admission. Native regression1PASS covers clockmapped picture atroot800k, beforestart/clipend/focus endpoint/output override/groupzeroopacity. Extended real privateSpace PNG HTTP1PASS: stranger403 foractive private image, root endpoint transparent200without source access, owner correctpixels retained. rustfmt/diffcheck PASS. Raster off-canvas visibility not pruned, output-map authorization still all supplied entries; originalvideo/text, otherformats/cache, nested editing/export and full parity incomplete.

## Этап 365: native raster import decoder formats

Inspected upstream native io/probe.rs image codec/extension classification (PNG/APNG/JPEG/GIF/WebP/BMP/TIFF/AVIF). Enabled image crate JPEG/WebP/GIF/BMP/TIFF alongside PNG, keeping bounded content-sniffed RGBA decoder and caller authorization; Cargo lock records added codec dependencies. Real encoded-file fixture exercises all six formats: dimensions/color (JPEG tolerance), PNG/WebP/TIFF alpha128 preservation, GIF/BMP opaque pixels, corrupt JPEG and width3841PNG refusal. Format1PASS, group_media4PASS incl actual FFmpeg/cancel fixtures, cargo check/rustfmt/diffcheck PASS. No claim of animated GIF/WebP/APNG playback, AVIF decode, orientation/color-profile normalization or all-format HTTP/privateSpace qualification. Sourcevideo/text automatic render/cache/nested export and full upstream parity incomplete.

## Этап 366: native original-video source frame clock

Added video_clock source_tick over existing ClipPlacement/PlaybackMode, deriving exact half-open authored extent, preserving sourceIn, forward/reverse/freeze and inverse Hold/Linear source-progress speed-ramp time using native validated segments/reciprocal-speed integral. Clamps presentation progress to selected last tick consistently with current frontend playback and rejects freeze outside interval/exact-wire overflow. Two regressions PASS: offsets/reverse/freeze/start/end refusal, linear speed1→2 inverse midpoint within2ticks. Initial ramp fixture used wrong sourceTick point key, corrected actual sourceProgressTick. rustfmt/diffcheck PASS. Source-clock primitive is not yet wired into automatic original-video layer decoding/effects/transitions/preview/export; media-frame selection semantics and real varying-content video qualification pending. Full parity incomplete.

## Этап 367: automatic original-library video layer rendering

flat_surface render_media_layers accepts original video frame callback, native source_tick maps active clips (forward/reverse/freeze/ramp), samples clip-local transform/opacity, composites in track order with blend/alpha. Native per-frame source collector now includes active video sources; existing library/Space path authorization supplies source map. Bridge decodes source frame at native clock with dimension bounds/shared timeout/cancellation; explicit rendered output still overrides local content. Advanced video effects/stabilization/opticalflow/transitions returnNone to existing complete-movie fallback rather than silently dropping. Actual-router fixture creates changing RGBA NUT library source, no rendered outputs, checks before-start red, sourceIn500000 half-alpha blue127/0/128/255 and source1s opaque blue. Test1PASS --nocapture no toolskip, cargo check/rustfmt/diffcheck PASS. Fixture samples exact presentation timestamps only: between-frame/VFR/current-frame vs first-at-or-after seek semantics remain unqualified and need correction; reverse/ramp/freeze actual decode/effects/transitions/text/audio/export and full parity incomplete.

## Этап 368: presented-frame hold for original video clocks

Original-video callback selects frame policy: forward/reverse/ramp hold latest PTS<=source time, Freeze retains documented first-at-or-after timestamp. Internal FFmpeg decoder preserves source timestamps/keyframe seek, filters by explicit lte(t,time), streams at most4096candidate frames retaining only last/two buffers with existing timeout/cancel/reap. Trim bounds decode lookahead1s; precise selection does not depend on rounded trim endpoint. First fixture failed exact0.5s because trim rounded end to stream timebase and excluded blue frame; inspected actual showinfo/bytes and corrected explicit PTS comparison. Real-router changing-alpha source now verifies exact+betweenPTS0.75+lastinterval1.499999, reverse boundary/inside interval and freeze0.75 selectingnext1s;1PASS --nocapture no skip. groupmedia4PASS, cargo check/diffcheck PASS. Existing pre-render movie decoder still first-at-or-after; VFR/nonzeroPTS/Bframes/longGOP/fps extremes and budget refusal not broadly qualified. Effects/transitions/text/audio/nested export/full parity remain incomplete.

## Этап 369: native video raster masks

Added Rust surface_masks for rectangle/ellipse/linear masks before layer affine, opacity and blend. Pixel-center normalized coordinates, clockwise rotation, inclusive hard edges, inward rectangle/ellipse feather and linear half-width feather match existing FFmpeg mask formulas. Shared clip-local keyframe sampler evaluates mask fields; incoming alpha multiplies masks in declaration order with per-row cancellation. Flat native video admission now accepts only these mask effects, retaining complete-movie fallback for other effects/reserved generic Linear masks. Two native regressions verify hard rectangle, linear feather, inverted ellipse, sequential alpha, RGB preservation, rotation and cancellation. Actual authorized original-video API fixture now also checks left-half blue/right-half red with no rendered output. Initial test fixtures used wrong AnimatableValue tag, corrected to mode:constant. Native mask2PASS, actual HTTP1PASS --nocapture, full backend library565PASS3ignored and diffcheckPASS. Animated mask sampler is wired but no dedicated animation/FFmpeg differential or browser visual qualification yet. Remaining effects/transitions/stabilization/text/nested audio/export and overall parity incomplete.

## Этап 370: WeftCut color-difference chroma matte in Rust

Inspected pinned upstream chromaKeySources.ts and shared visual catalog: upstream Vlahos matte is materially different from existing FFmpeg chromakey similarity/softness. Added MIT-attributed native chroma_surface CPU shader port with dominant-channel tie rules, screen balance, clip levels, nested four-tap shrink, five-tap feather, bilinear clamped premultiplied input sampling, screen subtraction, despill and neutral luma restore, matte visualization and straight-RGBA8 output conversion. Bounded GroupSurface/parameter validation and per-row cancellation; no per-pixel allocations. Added strict VideoEffect chroma_matte parameters and frontend wire type, composed in declared effect order with native masks before transform/opacity/blend. Legacy chroma_key stays unchanged; export service/compiler explicitly refuse new native matte until native export wiring exists. Initial fractional-tap test expected interpolation of alpha incorrectly: shader interpolates RGB before nonlinear matte; analytically corrected center0.9=230 byte. Three native tests PASS for exact key removal/foreground alpha, matte, signed shrink, integer/fractional feather, invalid params/cancellation. Actual authorized original-video API fixture blue screen removal returns red background without rendered output. Full backend568PASS3ignored; full API63PASS; frontend typecheck0errors0warnings; cargo check/diffcheckPASS. Dedicated inspector/animated chroma parameters/upstream import, native export and real GPU differential/f16 visual qualification remain incomplete; RGBA8 boundary differs from upstream floating render texture. Full feature parity not achieved.

## Этап 371: animated chroma matte channels on native clip clock

ChromaMatte now accepts optional strict animation overrides for all upstream channels keyR/keyG/keyB/balance/clipBlack/clipWhite/despill/feather/shrink/viewMatte, retaining static parameter baseline for omitted channels and compatibility for existing static documents. Shared AnimatableValue keyframe validation/sampling uses clip-local microseconds before native shader render; viewMatte follows upstream strict >0.5 threshold. Stored keyframe values and sampled output must satisfy host guidance ranges (unlike upstream permissive persisted params: out-of-guidance import and overshooting curves remain compatibility gap). Added frontend wire animation type. Native regression checks linear midpoint/end hold, view threshold, static baseline retention and invalid/NaN overrides. Actual authorized original-video API regression uses timelineStart500000, global1s local0.5s normal red background versus global1.25s local0.75s opaque black matte; no rendered outputs. Chroma4PASS; actual API fixture1PASS; full backend569PASS3ignored, full API63PASS; cargo check, frontend typecheck0errors0warnings, diffcheckPASS. Chroma inspector/editor, import mapping and export remain unfinished; no GPU differential or full browser visual qualification, full feature parity still incomplete.

## Этап 372: nested chroma matte inspector and validated history edits

Added NestedChromaInspector to nested composition workspace: add/remove matte effects, Russian labels for ten static channels, matte checkbox, clip-local keyframe insertion/replacement preserving existing channel interpolation, animation channel removal and keyframe tick/value readout. Baseline edits intentionally preserve animated overrides; key button writes current baseline at document cursor minus clip start. Added immutable editNestedVideoEffects using existing stale-session/identity guard and full native evaluateNestedFrame validation before history commit; locked track/missing clip errors retain original document. Changed effects clear rendered output overrides, Undo restores them. Output select now deletes empty mapping for automatic preview and disables concurrent binding edits during save/history mutation. DOM2PASS checks actual component input/click -> native validation call -> localtick250000, undo/redo and failed-validation preservation. Existing history/workspace14PASS; full frontend819PASS126files before final label/checkbox refinements, focusedDOM2PASS and typecheck0errors0warnings after refinements. Chromium headless1234 actual isolated-component screenshot inspected at /tmp/video-kadr-chroma-inspector-372.png: ten inputs, no page errors or horizontal overflow at900x700. Default Playwright1228 executables absent; used existing1234 binary, no download. Removed owned temporary HTML fixture and stopped owned Vite process. Browser screenshot uses fixture content; full application authenticated backend editing/save/preview workflow not yet browser-qualified. Individual key removal/tangent/curve inspector, eyedropper/import/export still incomplete; full parity open.

## Дополнительный источник: FilmCraft

2026-10-08 пользователь добавил https://github.com/storytold/filmcraft как второй источник функций. Реестр и первые native SDR RGB tone curves: docs/filmcraft-parity.md. Исходная цель всех WeftCut функций сохраняется; оба переноса незавершены.

## Этап 373: FilmCraft RGB curve inspector

Additional source FilmCraft now has native-sampled RGB graph + point/channel editor in nested workspace, using same native-validated history mutations as chroma. Details/evidence/limitations: docs/filmcraft-parity.md, second transfer. Full backend571PASS3ignored, API64PASS, frontend821PASS127files after isolated retry of load-sensitive waits; typecheck0errors0warnings/diffcheckPASS. Combined WeftCut/FilmCraft objective still incomplete.

## Этап 374: native nested video-only export engine

Native raster encoder now exports focused nested time range through existing Rust composition renderer, including newly ported masks/chroma/tone curves/group transforms/blends. Lossless FFV1/BGRA Matroska, policy-bound process execution, raw staging budget/cancel/deadline/no-replace atomic publication. Focused5PASS + full backend576PASS3ignored/API64PASS. Source authorization/HTTP job/library/UI wiring and audio export remain unfinished; details in docs/filmcraft-parity.md third transfer. Full parity remains open.


## Этап 375: authenticated durable native export API

POST /api/compositions/nested-export accepts strict schema1, explicit mode video_only, project/focused composition, half-open microsecond range and optional rendered output UUID bindings. NativeExport outbox kind uses existing Export resource admission, render permit, job lease/dispatcher, cancellation/progress/terminal/library lifecycle. Persisted work contains document, owner, IDs and SHA256 fingerprints, not client filesystem paths or bearer credentials. Admission requires authenticated actor and declared FFV1/Matroska capabilities. Worker re-resolves authorized source/output paths and fingerprints before rendering and before granting result ACL; source changes/access loss refuse result and remove this worker's published output. Dedupe includes owner and content fingerprints. Conservative range traversal respects nested source offsets, exclusive interval ends, constant-zero opacity/scales, hidden tracks and rendered overrides; animated invisible layers may still require authorization.

Focused API3PASS: unauthorized/missing mode/invalid range/unavailable capabilities, real FFmpeg source-free2x2@2fps FFV1 job→library/owner ACL→two exact decoded blue RGBA frames, same-owner dedupe/separate-owner job, private Space source403 and inaccessible override404. Extended native clock/visibility regression1PASS. Full backend576PASS3ignored/API67PASS, cargo check and diffcheckPASS. Existing unrelated compiler warnings remain. HTTP cancellation/restart/race/fingerprint-mutation integration not yet qualified. Hard crash between output hardlink and terminal registration may leave an orphan and prevent no-replace retry; no crash-safe reconciliation claim. Large-media pre/post hash time is not included in encoder deadline. No UI export control or audio mixing wired in this stage; full parity remains open.


## Этап 376: nested export workspace controls

NestedExportPanel wired into CompositionWorkspace nested mode: native focused document duration via bounded Rust POST /api/compositions/nested-durations, editable half-open microsecond range, explicit FFV1/MKV without audio, authenticated snapshot submission, job ID/progress, cancellation request, completion/library refresh and network observation retry using the same accepted job ID. Component disposal aborts HTTP observation/clears its timer while leaving the server job running. Existing getJob now accepts optional AbortSignal; legacy pollJob semantics unchanged. Three DOM regressions cover native range/bindings/token forwarding, completion/library refresh, retained accepted job after polling failure (no resubmit), cancel and disposal/late response suppression. Transport regression checks exact strict video-only body and auth/signal. Native durations API regression covers explicit bounds and missing root.

Chromium1234 inspected actual isolated panel at900x700 (/tmp/video-kadr-native-export-376.png): range/control/progress at42%, cancellation reaches cancelled, no page errors/horizontal overflow. Browser transport mocked; actual renderer/FFmpeg was qualified in stage375, full authenticated application save→export workflow remains unqualified. Owned temporary HTML fixture removed and Vite stopped. Backend576PASS3ignored/API68PASS. Final frontend825PASS128files, typecheck0errors0warnings, diffcheckPASS. New transport fixture initially failed for missing canvas/empty composition, fixed by using actual authored color composition; final full run passed. New export panel has no audio mode, codec presets, persisted UI job recovery, downloads, or selected-source grouping; combined full parity stays open.


## Этап 377: native nested audio placement and bounded PCM core

Inspected upstream apps/desktop/native/src/audio/mix.rs + envelope.rs and group-export-conformance spec: audio walks composition refs itself, intersects ancestor windows and retains original clip-local envelope clocks; visual opacity does not gate audio. Added host-native nested_audio::plan_audio_tracks for focused range, accumulated signed i128 root offsets, explicit document-duration clipping, repeated instance paths, track mute/solo, enabled and constant-zero gain/role pruning, speed-ramp Mute pruning. Full AudioClip retained for future voice/pitch/tone/duck/crossfade processing. Native sample resolves source clock through shared video_clock (forward/reverse/ramp), authored gain0..16, role gain, fades and pan on original local clock. Editing locks are absent from this host audio-track model; upstream role-table/locked-layer gating is not mapped here (host track/role gain contracts retained).

Added nested_pcm::mix_block over caller-supplied trusted/conformed mono/stereo samples: bounded65536frame/4096layer/8388608frame-layer budget, exact rational root sample→microsecond clock, per-frame/per-layer cancellation, finite PCM/mix checks, equal-power pan and ten-millisecond coefficient interpolation. Extracted existing equal_power_pan math into domain/audio_pan and reused it in legacy FFmpeg compiler and PCM core. Summation preserves bus headroom (no implicit normalization/clipping). Core refuses unsupported voice/pitch/tone/duck/crossfade/tempo-preserving processing; full clip descriptors are retained, not silently discarded.

Native regressions7: window/source/animated gain+fade+pan, focused child equivalence, zero visual opacity still audible, repeated instance/reverse, mute/solo/document tail, split-group boundary equivalence, mono fan-out/stereo crossfeed/bus summation, animated coefficient interpolation, hostile PCM/channel/cancel/overflow/DSP refusal. Focused nested27PASS. Full backend583PASS3ignored/API68PASS; cargo check and diffcheckPASS. Shared legacy FFmpeg equal-power pan regressions pass in the full backend run. This is an audio-plan/mixer core, not yet media decoding, cache/conform, full DSP, embedded-video audio, playback, muxing or UI audio export. Source-time microsecond quantization and high-depth multitrack performance remain unqualified; no sample-accurate whole-export or upstream PCM corpus parity claim. Combined all-feature goal remains open.


## Этап 378: policy-bound PCM conform and bounded disk-backed reads

Added tools/pcm_conform::conform_pcm for trusted local paths and caller-probed mono/stereo channels: FFmpeg resample48k, decoded PTS normalized to zero, sample-index trim, private owned float32LE staging file; strict source sample range/exact wire clock and <=8GiB caller budget. Existing ProcessRuntime render policy (network denied), stream progress execution, process-tree timeout/cancel/reap are reused. Output frame alignment/size/budget checked before returning reader. FFmpeg file-size option is an additional cap, not a hard filesystem quota. Short nonempty decoded streams return silence for requested selected samples after EOF; empty/no-audio/corrupt sources fail. No source-format autodetection/channel probing trust supplied by client.

ConformedPcm pages512frames (<=4096bytes) from disk, supports random/reverse reads, returns finite mono/stereo samples, uses nearest microsecond→sample rescale, and keeps reverse sourceOutUs-1 inside exclusive end. Reader closes handle before owned temp removal, preserving Windows cleanup ordering. Existing source files are retained; Drop removes only UUID-owned PCM staging. No persistent/shared conform cache or crash reconciliation yet.

Real FFmpeg fixtures: stereo PCM16 WAV1152samples -> selected100..1200 =>1052 decodedframes, exact channel amplitudes/page boundaries/reverse reads/short-tail silence/range refusal/cleanup; mono WAV1200samples -> native nested PCM mixer, equal-power channel output checked at1e-12 tolerance, exclusive reverse-end sample handling; corrupt source cleanup. Additional strict admission and Unix fake-decoder50ms timeout/midflight cancellation cleanup fixtures. Mono sin/cos can differ by floating-point roundoff; regression compares with tolerance rather than bit equality. Full backend587PASS3ignored/API68PASS, cargo check and diffcheckPASS. No long compressed/VFR/nonzero-PTS corpus, downmix/HDR-equivalent audio format matrix or server source authorization wiring qualified. Native export still video_only; file conform/mixer groundwork not a claim that audio export/playback is wired. Full WeftCut/FilmCraft parity remains open.


## Этап 379: focused nested audio-track mix staging

Added tools/native_audio_export::mix_project_audio_tracks connecting native nested audio placement, policy-bound source conforming and bounded PCM mixing to an owned raw stereo48k float32LE output. Caller provides trusted source paths/probed channel counts. Source conform ranges derive from actual clipped first/last source clocks (not whole clip/source duration), union repeated source IDs, and aggregate estimated conformed+mix bytes must fit caller budget<=8GiB before decode. Selected root start rounds upward to the48k sample grid; output frame count is caller supplied and must cover selected duration, with silence outside clipped audio windows (including padded video tail). Six-hour output-frame cap and wire/sample overflow checks. Shared deadline spans decode and mixing; cancellation bridge reaches per-sample/per-layer mixer checkpoints;2048-frame blocking batches retain small buffers/page caches. Progress30% source preparation then mix. Final finite float32 check preserves headroom and refuses overflow. File handle lives inside owned output guard, closes before cleanup on every error/Drop; no client/library publication in this layer.

Real FFmpeg stereo PCM16 source with three signed amplitude markers: focused root200..800ms, group at250ms with child sourceIn100ms, source clipIn100ms, gain0.5×role2, visual opacity0. Exact silence boundary samples and expected marker amplitudes checked; output33600frames includes padded silence. Group split at400ms produces identical complete PCM bytes; reverse selects opposite source markers. Huge finite role gain fails float32 conversion after a silence block was written, and owned mix/conform files are cleaned while source remains. Strict aggregate budget/missing binding/DSP processing fail before launching decoder. Focused2PASS. Full backend589PASS3ignored/API68PASS; cargo check and diffcheckPASS. No native video/audio mux, API audio mode/source authorization, UI audio controls, embedded-video audio, tempo/voice/crossfade/ducking DSP or persistent conform cache wired yet. Disjoint selected segments of one source use an enclosing conform range (not fragment cache), and microsecond/grid rounding/full long-media corpus remain unqualified. Full all-feature parity remains open.


## Этап 380: native lossless video + prepared PCM mux

Native RGBA engine now has encode_frames_with_audio and encode_project_with_audio; existing encode_frames/encode_project_video_only delegate with None and retain video-only behavior. Trusted RasterAudio carries stereo48k float32LE path/sample count, checked before raster work for exact byte size and ceil(videoFrames×fpsDenominator×48000/fpsNumerator) duration. Both raw RGBA and prepared PCM are admitted as readonly process inputs. Explicit FFmpeg stream mapping0:v:0/1:a:0, FFV1/BGRA + pcm_f32le Matroska, same process policy/cancel/deadline/output guard/no-replace atomic publication. Engine never deletes caller-owned PCM. Aggregate audio+video staging admission remains caller responsibility (audio stage and RGBA stage budgets separately enforced here).

Focused encoder8PASS: new rational30000/1001 coverage31frames→49650samples, duration/byte mismatch fails before any raster/process/temp output, actual2×2@2fps2RGBAframes with alpha +48000stereo samples (including >1.0 bus headroom) decoded from MKV are byte-exact; caller PCM retained and owned raster staging removed. Existing real video-only/group/tone/timeout/cancel regressions still pass. Extended actual source WAV→focused nested mix→native composition renderer→combined MKV fixture: selected200..800ms with rational10/7fps yields1frame700ms/33600audio samples, decoded audio equals entire prepared mix byte-for-byte and native video decodes one2×2frame. Initial fixture rounded fpsMilli incorrectly1430; native schema refused it, corrected to1429. Full backend592PASS3ignored/API68PASS, cargo check/diffcheckPASS.

Current HTTP job/request/UI still explicitly video_only. Prepared mix API is trusted internal boundary, not client filesystem input; source channel probing/authorization, embedded-video audio, full DSP and aggregate media-stage lifetime need wiring before enabling HTTP audio mode. Whole-project playback, audio export UI, crash-safe registration/recovery and full upstream audio/video conformance corpus remain incomplete. Full WeftCut/FilmCraft parity remains open.


## Этап 381: primary embedded audio + authenticated A/V export mode

Native nested audio plan now includes primary video source audio using the existing host contract (last nonempty visible Video/Image/Text visual track; overlay video sound must be explicit Audio tracks). Audio solo only gates independent audio tracks, retaining legacy primary-source behavior. Video enabled/sourceAudioEnabled/track muted/hidden/authored hasAudio gates; Freeze emits silence, Reverse maps shared source clock. Video audioGain/audioPan use original clip-local time through nested group/split clocks; visual opacity does not mute sound. Native focused regressions cover primary/source gain/pan/reverse/split boundary and enabled/mute/hidden/freeze/overlay/solo rules. Real Matroska video+PCM16 audio fixture generates the same complete mix bytes as equivalent independent audio; freeze produces silence with no audio binding/decoder.

Nested export strict mode now supports video_audio alongside video_only. Audio layer source IDs join existing owner/Space/output authorization and SHA256 admission/execution/publication rechecks. Unsupported native PCM DSP is rejected at request validation; FFV1/Matroska/pcm_f32le declared capabilities required. Worker probes actual first audio stream channels through cancellable ProcessRuntime ffprobe (authored channel metadata not trusted), prepares PCM mix, invokes native renderer+PCM mux, grants output ACL/registers library with withAudio/audioCodec/mode metadata. Shared audio-probe/mix/raster deadline and progress lanes0..30/30..100; raw RGBA reserved from aggregate<=8GiB source-conform+mix staging estimate. Both ffprobe and FFmpeg source conform commands apply existing OFFLINE_PROTOCOLS allowlist in addition to network-denied process policy. Prepared PCM owned guards remain alive through mux and are dropped before terminal result. Existing durable payload schema/backward video-only mode remains readable; no client paths/bearer tokens persisted.

Actual API regression: private WAV source is403 for stranger; owner job succeeds as video_audio, becomes library output with owner ACL. Client authoring deliberately declares mono while actual WAV is stereo: decoded result retains opposite L/R±0.25 amplitudes, then exact EOF silence, proving real channel probe. Two exact blue RGBA frames decoded too. Same-owner dedupe, unsupported Echo400, and no owned .pcm/.mix/.raster staging after completion checked. Focused nested API12PASS. Panel/API supports audio mode; compact checkbox “Добавить звук”, busy guards and existing progress/cancel/retry/library refresh retained. DOM/transport38PASS, typecheck0errors0warnings. Chromium1234 isolated actual panel900x700: checked audio emits mode video_audio, progress42%, cancellation succeeds, no page errors/overflow; screenshot /tmp/video-kadr-native-audio-export-381.png inspected. Browser transport mocked; real API/FFmpeg verified independently, full authenticated application workflow remains unqualified. Removed owned fixture and stopped Vite. Final full backend594PASS3ignored/API69PASS, frontend826PASS128files, typecheck0errors0warnings, cargo check/diffcheckPASS. Earlier simultaneous full test/build/browser load increased runtime but all checks completed without restarting live handles or raising timeouts.

Full upstream role-table mapping, nonzero audio/video stream-PTS sync/conformance, surround formats, voice/pitch/tempo/crossfade/duck DSP, master buses, playback/conform cache, crash-safe output registration and broad long-media/race/recovery checks remain open. Default panel mode stays video_only until checkbox selected. Thus ordinary mono/stereo independent+primary source audio is now usable in API/UI, not full audio/DSP or all-feature parity.

## Этап 382: preserve delayed source audio timestamps

Reproduced independent audio PTS normalization shifting sound 500ms early in an actual FFV1/PCM Matroska source. PCM conform now uses aresample async=1:first_pts=0 before sample trim, retaining audio delay relative to FFmpeg's normalized container origin. Regression checks silence and exact signal boundaries for media origins 0 and 5 seconds; failed before the fix, passes afterward. Semantics reference: https://www.ffmpeg.org/ffmpeg-resampler.html. Full backend595PASS3ignored/API69PASS; diffcheckPASS. Existing WAV, nested mixing, reverse, lossless mux and authenticated A/V tests pass. This qualifies two delayed-stream fixtures only; negative PTS, compressed encoder delay, discontinuities, VFR and broad long-media A/V synchronization remain unqualified. All-feature parity remains open.


## Этап 383: FilmCraft secondary color curves

Transferred five FilmCraft Lumetri secondary curves into native ToneCurves and the existing nested inspector, with original periodic interpolation/HSL processing and explicit SDR RGBA8 boundary. Details and remaining color/HDR/UI gaps in docs/filmcraft-parity.md twelfth transfer. Full backend597PASS3ignored/API69PASS, frontend827PASS128files, typecheck0errors0warnings, diffcheckPASS; real nested FFV1 render and isolated browser inspector checked. Full combined feature goal remains open.


## Этап 384: native FilmCraft creative looks and tonal wheels

Added native creative_grade effect and nested inspector with eight procedural looks/intensity, three tonal color wheels/lightness and native declaration-order rendering. Full details and limitations in docs/filmcraft-parity.md thirteenth transfer. Backend600PASS3ignored/API69PASS, frontend829PASS129files, typecheck0errors0warnings, cargo check/diffcheckPASS; actual nested FFV1 pixels and styled browser selector interaction verified. Static SDR only; HDR/LUT/animation/Color Match/full creative controls remain open. Combined all-feature goal remains active.


## Этап 385: native creative-grade animation

Ten animated FilmCraft creative channels now sample native clip-local clock and are authored through validated nested inspector/history. Actual FFV1 expected frame pixels preserve animated color across a group split. Full details and remaining gaps in docs/filmcraft-parity.md fourteenth transfer. Backend601PASS3ignored/API69PASS, cargo check/typecheck/diffcheckPASS; final sequential frontend830PASS129files (unchanged5s timeout), after two earlier full runs hit unrelated cold-import timeouts during concurrent compilation. Combined all-feature goal remains active.


## Этап 386: native imported LUT rendering

FilmCraft1D/3D/shaper CUBE and Autodesk3DL algorithms connected to native color_lut effect, bounded validation endpoint and nested import/intensity/history inspector. Actual FFV1 decoded pixels cover shaper+cube/translucent group. Backend608PASS3ignored/API70PASS, frontend832PASS130files, cargo check/typecheck/diffcheckPASS; remaining LUT size/library/HDR/builtin/animation limitations and final verification in docs/filmcraft-parity.md fifteenth transfer. Full combined feature goal remains open.


## Этап 387: FilmCraft runtime/source parity ledger

New actual compiled FilmCraft registry establishes675commands303effects/transitions1426parameters14export formats30presets, with tracked source/package/MCP/doc index. Concrete ledger/mapping/evidence guards in docs/filmcraft-catalog-audit.md and docs/filmcraft-catalog.json.21partial/2674unreviewed/0verified entries are review status, not a claim all unreviewed capabilities are absent. Four verifier regressions pass; no host application code changed. Both full WeftCut and FilmCraft transfer goals remain open.

## Этап 388: HSL Secondary — реализация и начальная проверка

Native HSL Secondary and nested inspector with twelve animated numeric channels added. Five focused native and one inspector regressions PASS; backend library 613 PASS / 3 ignored, typecheck zero errors/warnings, cargo check and diff check PASS. Upstream render equivalence, actual HSL media export, full frontend/API regression and HDR/preview-scale behavior remain unqualified; see seventeenth transfer in filmcraft-parity.md. Full combined feature goal remains open.


## Stage 389 — HSL Secondary actual media export and ledger refresh

Extended real rawvideo/NUT source → native nested composition → FFV1/Matroska → decoded RGBA fixture with HSL hue-shift animation at 0/500000us, nested instance split at250000us and White/Black mask with maximum denoise/blur. Exact expected pixels pass for opaque/translucent source, red child background and half-opacity group. Both unsplit and split animation preserve the original child clock. This qualifies those fixtures only, not upstream render equivalence or HDR. Existing curves/wheels/LUT portions also pass in the same export test.

Four color inspector files / nine DOM tests PASS; catalog verifier four tests PASS; diff check PASS. Added fourteen HSL parameter mappings as explicitly partial, refreshed and checked pinned source/runtime ledger:35partial /2660unreviewed /0verified. Source-render oracle, full frontend/API regression, preview-resolution scaling and unified Lumetri/HDR remain open. No full-feature completion claim.


## Stage 390 — independent FilmCraft HSL SDR render oracle

Added scripts/filmcraft-hsl-oracle.rs using actual pinned upstream render::effects::apply, with the other Lumetri sections disabled and Rec709 working space. Straight RGBA8 input is converted to premultiplied linear f32 before upstream processing and serialized back through upstream Image::to_rgba8. Eight deterministic 7x5 fixtures cover all four mask/correction modes, refinement off/on (63% median denoise and11% blur), mixed selected/nonselected colors and opaque/half/zero alpha, temperature/tint/saturation/hue correction. Native output matches within one RGB byte with exact alpha; fully transparent hidden RGB is excluded from comparison because upstream serialization clears it, while host preservation has a separate analytic test.

Saved fixture backend/tests/fixtures/filmcraft-hsl-secondary.json and SHA/clean-reference guarded generator scripts/generate-filmcraft-hsl-oracle.py. Fresh generator replay --check PASS using cargo+1.98.1; six native HSL tests PASS; diff check PASS. This is an independent renderer oracle for these SDR full-resolution cases, not all inputs, HDR, preview scale or unified Lumetri. Parameter ledger remains partial.

Reproduce: CARGO_TARGET_DIR=/tmp/video-kadr-filmcraft-catalog-audit/target python3 scripts/generate-filmcraft-hsl-oracle.py --reference /tmp/video-kadr-filmcraft-reference --check


## Stage 391 — HSL refinement resolution scaling

Added HslSecondary::apply_scaled for working/full-resolution pixel ratio in (0,1], with apply retaining full-resolution scale1. Median radius and blur sigma now follow FilmCraft FxCtx::px_scale. Invalid/nonfinite ratios are refused before mutation. Extended genuine upstream renderer oracle to24cases across1,0.5,0.25 scales; native output matches within one RGB byte with exact alpha. Fresh upstream generator --check PASS; seven focused native tests PASS including invalid-scale admission; catalog regeneration/comparison and diff check PASS. This qualifies the scaled SDR algorithm. Reduced-resolution preview routing has not yet been wired/qualified; full HDR and unified Lumetri remain incomplete. Existing export caller retains scale1.


## Stage 392 — FilmCraft Lumetri vignette native effect and inspector

Added native ColorVignette SDR effect with amount[-5,5], midpoint[0,100], roundness[-100,100], feather[0,100] and enabled switch, preserving the pinned source pixel-coordinate convention and negative-roundness aspect scaling (positive roundness is a source no-op). Per-row cancellation, finite/range validation and alpha/hidden RGB preservation retained. Four numeric channels support native clip-local animation; nested inspector edits validated undo history and invalidates output bindings. Native effect runs in declared stack order before transform; legacy FFmpeg path explicitly refuses unsupported native vignette.

Three focused native tests PASS (corners/center/alpha, invalid admission/cancellation, all four animation channels). One inspector DOM test PASS (four controls, editing, local key, reset/undo); final typecheck0errors0warnings; diff check PASS. Initial compile caught a missing Result return in validation, corrected before successful test build. Ledger refresh/check PASS:40partial/2655unreviewed/0verified. Genuine upstream renderer oracle, real vignette export, broad application tests and unified Lumetri/HDR remain unqualified. This is ongoing full feature transfer, not completion.


## Stage 393 — independent vignette renderer oracle and actual export

Added genuine upstream Lumetri renderer oracle with other sections disabled, Rec709 working space, premultiplied linear f32 input and upstream RGBA8 serialization. Forty-five deterministic 7x5 SDR cases combine amounts[-5,-1.7,0,2.3,5], roundness[-100,0,100], feathers[0,50,100], midpoint37, mixed colors and opaque/half/zero alpha. Native output matches within one RGB byte, alpha exact; hidden transparent RGB is independently preserved by host. Saved fixture and SHA/clean-reference guarded generator replay --check PASS. Reproduce with CARGO_TARGET_DIR=/tmp/video-kadr-filmcraft-catalog-audit/target python3 scripts/generate-filmcraft-vignette-oracle.py --reference /tmp/video-kadr-filmcraft-reference --check.

Extended real rawvideo/NUT→native nested→FFV1/Matroska→decoded RGBA fixture with amount animation0→-5, opaque/translucent source, child background and group opacity. Both unsplit and split-at250000us instances match exact expected neutral frame, dark corner and preserved center pixels. Four focused native vignette tests PASS; full backend library619PASS3ignored; five color inspector files/ten DOM tests PASS; ledger generation/check and diff check PASS. Ledger remains40partial/2655unreviewed/0verified. Full frontend/API integration, HDR, unified Lumetri order and exhaustive feature goal remain open.


## Stage 394 — authenticated color preview and broad application regression

Extended existing actual original-video API preview fixture with HSL Secondary and ColorVignette authored via wire JSON. The source clip starts at500000us; keys at local0/500000us verify effect clocks relative to clip placement (rather than global timeline). Exact decoded preview pixels pass for half-alpha blue over red background, hue-shifted opaque red, neutral vignette, dark corner and preserved center. Invalid static hueShift181 and feather-1 are rejected BAD_REQUEST even with animation present, confirming full authored admission rather than sampled-only acceptance.

Full updated API integration70PASS. Full frontend132files/834testsPASS with maxWorkers1. Previous stage393 backend library619PASS3ignored remains latest library evidence (this stage changed only API tests/evidence). Ledger refresh/check and diff check PASS; statuses remain40partial/2655unreviewed/0verified. These API fixtures use real authorized backend/FFmpeg, while component frontend tests mock transport; combined authenticated browser E2E, HDR and unified Lumetri order are still unqualified. Full feature goal remains active.


## Stage 395 — Lumetri Basic Correction SDR native effect and inspector

Added BasicGrade with enabled switch and temperature/tint/exposure/contrast/highlights/shadows/whites/blacks/saturation, native finite/range admission, clip-local animation of all nine numeric parameters, alpha/hidden RGB preservation and row cancellation. Port follows source ordering: sRGB→linear white balance/exposure→display endpoints→luma-weighted highlights/shadows→S-curve contrast→saturation. It runs as an explicit effect in the native declared stack; legacy export explicitly refuses it. Inspector authors validated edits/keys/reset through existing history and output invalidation.

Three focused native tests PASS (identity/alpha/disabled, zero saturation/admission, nine animation channels). One DOM inspector test PASS (nine controls/edit/local key/reset/undo); typecheck0errors0warnings; test target compiles; diff check PASS. Ledger regeneration/check PASS:50partial/2645unreviewed/0verified. Genuine upstream renderer oracle, actual BasicGrade media export/API, HDR, input LUT integration and full unified Lumetri ordering remain unqualified. Latest broad application evidence predates this new effect (stage394). Full original feature transfer remains open.


## Stage 396 — independent BasicGrade render oracle and actual export

Added genuine upstream Lumetri Basic Correction oracle with other sections disabled, Rec709 working space, premultiplied linear input and actual upstream RGBA8 output. Thirty-eight deterministic 7x5 scenarios cover neutral correction, four authored values for each of nine controls (including boundaries) and a combined nonneutral grade; mixed opaque/half/zero alpha retained. Host matches within one RGB byte with exact alpha. Saved fixture and SHA/clean-reference guarded generator replay --check PASS. Reproduce: CARGO_TARGET_DIR=/tmp/video-kadr-filmcraft-catalog-audit/target python3 scripts/generate-filmcraft-basic-oracle.py --reference /tmp/video-kadr-filmcraft-reference --check.

Extended actual source→native nested→FFV1/Matroska→RGBA fixture with saturation animation100→0 and instance split at250000us. Exact expected neutral and desaturated frame pixels pass with source alpha, red child background and half-opacity group; split retains child animation clock. Full backend library623PASS3ignored; six color inspector files/eleven DOM testsPASS; four focused BasicGrade testsPASS; ledger regeneration/check and diff checkPASS. Ledger remains50partial/2645unreviewed/0verified. Full frontend/API after BasicGrade, HDR, input LUT order and unified Lumetri behavior remain unqualified. Full feature goal stays open.


## Stage 397 — creative faded film, vibrance and split tone

Added explicit native CreativeAdjust SDR effect with enabled switch, fadedFilm[0,100], vibrance[-100,100], saturation[0,200], shadow/highlight RGB tint channels[0,1] (neutral0.5). Algorithm follows pinned source faded film→luma-weighted split tone→saturation/vibrance ordering. It runs in declared stack before transform; callers can place it before existing procedural looks/wheels. This does not claim full unified Lumetri ordering. Alpha and fully transparent hidden RGB are preserved; finite/range validation and per-row cancellation retained. Nine scalar channels use clip-local animation. Inspector supports validated edits/keys/reset/undo and output invalidation; source tint alpha is unused by the source algorithm and not exposed here.

Initial analytic native tests for faded black/alpha and neutral identity/range admission PASS; DOM inspector editing/local key/reset/undo PASS; typecheck0errors0warnings. Genuine upstream render oracle, actual export/API, HDR and full unified Lumetri integration remain unqualified; sharpen remains missing. Ledger refresh/check PASS:55partial/2640unreviewed/0verified. Full goal remains open.

Final stage397 native focused run:3PASS, including clip-local sampling of all nine channels and cancellation before mutation. Diff check PASS.


## Stage 398 — independent creative adjustment oracle and real export

Added actual upstream Lumetri creative renderer oracle with other sections disabled, Rec709 working space and premultiplied linear image input/output through upstream Image serialization. Twenty-five deterministic SDR scenarios cover neutral, faded film25/50/100, vibrance and saturation four values each, both endpoints of each six tint RGB components and a combined grade; mixed opaque/half/zero alpha retained. Host matches within one RGB byte, exact alpha. Saved fixture and SHA/clean-reference guarded generator replay --check PASS. Reproduce: CARGO_TARGET_DIR=/tmp/video-kadr-filmcraft-catalog-audit/target python3 scripts/generate-filmcraft-creative-adjust-oracle.py --reference /tmp/video-kadr-filmcraft-reference --check.

Extended actual original-video→native nested→FFV1/Matroska→decoded RGBA fixture with fadedFilm animation0→100 and instance split at250000us. Exact pixels pass for neutral/graded frames with source alpha, child background and group opacity; split preserves child effect clock. Four focused native testsPASS; full backend library627PASS3ignored; ledger regeneration/check and diff checkPASS. Ledger remains55partial/2640unreviewed/0verified. Full frontend/API after the new effect, HDR, sharpen and unified Lumetri ordering remain unqualified; full feature goal remains active.


## Stage 399 — Lumetri sharpen native SDR bridge and inspector

Added ColorSharpen amount[-100,100] with scalar clip-local animation. Converts each RGB plane to premultiplied linear f32, runs source three-pass box Gaussian approximation at radius1.2*max(pixelScale,0.35), applies original-minus-blurred delta times amount, unpremultiplies and encodes SDR RGBA8. Alpha remains unchanged and hidden RGB stays preserved. Full-resolution native effect uses scale1; apply_scaled admits finite(0,1] for future reduced-resolution routing. Reused native Gaussian-plane helper and shared sRGB conversions; cancellation checks during conversion/blur/output. Declared stack order and explicit legacy export refusal retained. Inspector supports one baseline, clip-local keys/reset/undo/output invalidation.

Two focused native testsPASS (constant fields/alpha/hidden RGB and finite admission/cancellation); inspector DOM testPASS; final typecheck0errors0warnings; diff checkPASS. Initial typecheck caught a copied enabled control unsupported by the scalar sharpen contract; removed before final checks. Ledger refresh/checkPASS:56partial/2639unreviewed/0verified. Genuine upstream oracle, actual media/API, reduced-preview routing, HDR and unified Lumetri order remain unqualified. Full original goal remains active.


## Stage 400 — actual FilmCraft sharpen oracle and FFV1 transport

Added genuine upstream Lumetri sharpen oracle with basic/curves/wheels/vignette/HSL sections disabled and creative section enabled. Premultiplied linear image input and upstream RGBA8 serialization qualify fifteen deterministic 7x5 scenarios: amount[-100,-35,0,48,100] × pixelScale[1,0.5,0.25], mixed colors/alpha. Native scaled bridge matches within one RGB byte with exact alpha; transparent hidden RGB host preservation checked separately. Source oracle generator replay --checkPASS, guarded by exact SHA and clean tracked reference. Reproduce: CARGO_TARGET_DIR=/tmp/video-kadr-filmcraft-catalog-audit/target python3 scripts/generate-filmcraft-sharpen-oracle.py --reference /tmp/video-kadr-filmcraft-reference --check.

Real FFV1/Matroska encoder test renders all15 oracle scenarios and decodes each frame back to7x5RGBA; comparison to independent upstream oracle passes, alpha exact, only published file remains after staging cleanup. This qualifies raw raster encode/decode transport, not nested original-video sharpening or full authenticated export UI. Three focused sharpen testsPASS; full backend library631PASS3ignored; three newest inspector DOM testsPASS; ledger regeneration/check and diff checkPASS. Full frontend/API after newest effects, reduced-preview routing, HDR and unified Lumetri order remain unqualified; full feature goal stays active.


## Stage401 — authenticated latest color effect preview and full regression

Extended actual original-video API preview with BasicGrade saturation100→0, CreativeAdjust fadedFilm0→100 and ColorSharpen amount0→100. Clip placement begins500000us; key times0/500000us verify local effect sampling. Exact pixels qualify half-alpha source/background and opaque second-frame results. Invalid authored exposure5.1, shadowRed1.1 and sharpen100.1 are rejected even with valid animated overrides. Sharpen API fixture is a spatially constant source: transport/admission coverage only; patterned sharpening math is separately covered by stage400 upstream oracle/actual FFV1 fixture.

Updated full API70PASS; full frontend135files/837testsPASS (maxWorkers1); ledger regeneration/check and diff checkPASS. Latest full library631PASS3ignored remains stage400 evidence, as this stage changed API tests/docs only. Ledger56partial/2639unreviewed/0verified. Combined authenticated browser E2E, unified float Lumetri/HDR and whole-feature parity remain unqualified. Source processing contract for the next unified integration is documented in filmcraft-parity.md; goal remains active.


## Stage402 — shared premultiplied linear color frame

Added bounded LinearColorFrame initialized from validated host GroupSurface, storing premultiplied linear f32 plus original RGBA template for exact unchanged alpha/transparent hidden RGB. SDR map_signal follows actual FilmCraft GradeSpace encode(nonnegative linear) / decode(unit-clamped signal), validates finite callback output and checks cancellation by row. Multiple signal operations retain fractions smaller than one RGBA8 byte until final serialization. Shared linear sharpen uses the source radius/box approximation and keeps values above display white internally until final SDR output. Refactored native ColorSharpen to use this shared frame; other color sections still cross standalone RGBA8 boundaries and are not claimed integrated.

Three new regressions PASS: all256 byte levels roundtrip with alpha/hidden RGB, three0.2-byte adjustments accumulate to one output byte, nonfinite signal/cancellation refusal. Actual sharpen source-oracle and FFV1 transport regressions remainPASS after refactor. Full backend library634PASS3ignored; ledger regeneration/check and diff checkPASS. Initial compile captured an old test range inferred asu8; fixed to an explicitu16 range/casts before final successful build. Float-frame allocation is bounded by existing3840x2160 raster limit; performance/aggregate memory not qualified. Unified color sections/HDR remain required; full goal stays active.


## Stage403 — consecutive basic/creative/sharpen use one linear frame

BasicGrade and CreativeAdjust now expose apply_linear on the shared premultiplied frame; their standalone apply converts once in/out and retains source-oracle behavior. Native media renderer accumulates consecutive BasicGrade/CreativeAdjust/ColorSharpen effects in one working frame, respecting authored order and clip-local animation, serializing once before a nonintegrated effect or final transform. Intermediate finite SDR signal encode/decode/clamping remains per standalone effect contract. Single integrated Lumetri's combined first-pass signal math is still a different contract and not claimed completed. Vignette/curves/looks/LUT/HSL are not yet integrated into this float chain.

Native regression proves three1% faded-film operations retain sub-byte changes (black→RGB1), while separate RGBA8 operations lose them (black→RGB0). Full backend library635PASS3ignored; actual BasicGrade/CreativeAdjust/Sharpen upstream oracle and native FFV1 fixtures remainPASS. Ledger regeneration/check and diff checkPASS. Full frontend/API after render-path change, combined Lumetri oracle/order/clamping, all remaining float sections and HDR remain unqualified. Full feature goal remains active.


## Stage404 — vignette shares linear color pipeline

ColorVignette now exposes apply_linear using shared SDR signal mapping and frame dimensions, preserving source positional formulas, enable/no-op behavior and static/animated validation. Standalone apply uses one float-frame conversion pair; native renderer retains one working frame across consecutive BasicGrade/CreativeAdjust/ColorVignette/ColorSharpen effects. Nonintegrated effects remain explicit serialization boundaries. This does not implement a single full Lumetri first-pass signal stage.

Updated native accumulation regression uses two0.9% faded-film corrections with a neutral vignette between: outputRGB1 confirms the neutral section does not force a byte boundary. Full backend library635PASS3ignored, including all45 upstream vignette cases, actual nested animated vignette/split media export and sharpen source-oracle/FFV1 transport. Ledger regeneration/check and diff checkPASS. Full frontend/API after render change, other float sections, unified stage ordering/clamping and HDR remain unqualified. Full feature goal remains open.


## Stage405 — LUT float chain integration and stable serialization

ColorLut now exposes apply_linear using cached parsed shaper/tetrahedral tables and authored intensity; standalone apply converts once in/out. Native consecutive BasicGrade/CreativeAdjust/ColorVignette/ColorLut/ColorSharpen effects share one frame. Fully transparent hidden RGB is now preserved rather than graded, matching shared image skip semantics. Added three0.2-byte LUT adjustments accumulation/alpha/hidden RGB regression; native accumulation test also places identity LUT alongside neutral vignette between small corrections.

First full run caught a real half-byte serialization drift (mapped0.5→127 instead of128 at alpha73) and an existing process-completion scheduling failure (try_recv Empty). Added1e-4-byte tolerance only at final RGBA8 rounding to stabilize exact half-byte numerical ties through f32 transfer/premultiplication. Final full backend library636PASS3ignored, including LUT intensity/cache/parser, float-chain regressions, all independent color source oracles, actual FFV1 transport/nested exports and the previously failed process test. Ledger regeneration/check and diff checkPASS. Full frontend/API after float pipeline changes, remaining curves/looks/wheels/HSL integration, unified single Lumetri signal stages and HDR remain unqualified. Full feature goal remains active.


## Stage406 — looks/wheels share linear frame; genuine upstream oracle

CreativeGrade now exposes apply_linear; native consecutive color pipeline includes procedural styles/intensity and all three tonal wheels without an RGBA8 boundary. Neutral sections keep the working frame; hidden transparent RGB follows shared skip semantics. Standalone apply retains public wire API and source look/wheel math.

Added actual pinned upstream Lumetri renderer oracle:28deterministic7x5SDR cases covering look0..8 at100%, each wheel x/y/lightness at both authored extremes, combined look/intensity/three nonneutral wheels, mixed opaque/half/zero alpha. Native matches within one RGB byte, exact alpha; saved SHA/clean-reference guarded generator replay --checkPASS. Reproduce: CARGO_TARGET_DIR=/tmp/video-kadr-filmcraft-catalog-audit/target python3 scripts/generate-filmcraft-looks-wheels-oracle.py --reference /tmp/video-kadr-filmcraft-reference --check.

Full backend library637PASS3ignored, including real animated wheels/instance-split nested export, all prior independent color oracles and FFV1 transport. Ledger regeneration/check and diff checkPASS;56partial/2639unreviewed/0verified. Curves/HSL float integration, unified single Lumetri processing stages, full frontend/API after float changes and HDR remain unqualified. Full goal remains active.


## Stage407 — RGB/HSL curves share linear frame; genuine upstream oracle

ToneCurves now exposes apply_linear, preserving master→RGB→five secondary HSL ordering and native1024-entry sampling. Standalone apply uses shared frame; consecutive native color chain retains all nine curve channels without RGBA8 boundaries. Transparent hidden RGB follows shared skip semantics. Native sub-byte accumulation regression includes a neutral curve stage to prevent accidental serialization.

Added actual pinned upstream Lumetri curves renderer oracle with unrelated sections disabled:20deterministic7x5SDR cases cover neutral, constant/nonsmooth multi-point curve for each of nine channels, combined master/blue/hue/saturation channels and mixed opaque/half/zero alpha. Native matches within one RGB byte with exact alpha; saved SHA/clean-reference guarded generator replay --checkPASS. Reproduce: CARGO_TARGET_DIR=/tmp/video-kadr-filmcraft-catalog-audit/target python3 scripts/generate-filmcraft-curves-oracle.py --reference /tmp/video-kadr-filmcraft-reference --check.

Full backend library638PASS3ignored, including prior color source oracles, actual native nested curves/HSL-curve FFV1 export and accumulated-color regression. Ledger regeneration/check and diff checkPASS. HSL Secondary float integration, unified single Lumetri stage order/clamping, full frontend/API after render changes and HDR remain unqualified. Full feature goal remains active.


## Stage408 — HSL Secondary completes standalone SDR float-chain integration

Added LinearColorFrame::signal_mask: scalar signal read without mutation/quantization, transparent samples zero, finite mask admission and row cancellation. HslSecondary now exposes apply_linear_scaled; key mask reads current fractional grading signal, median/box-Gaussian refinement retains source pixelScale, corrected/mask-view output maps onto the same frame. Standalone apply_scaled converts once in/out. Native consecutive color chain now includes all currently transferred standalone sections: BasicGrade/CreativeAdjust/ColorVignette/ColorLut/CreativeGrade/ToneCurves/HslSecondary/ColorSharpen. Noncolor effects remain explicit byte boundaries; authored stack order is preserved, not automatically reordered into a unified Lumetri effect.

New scalar-mask regression qualifies fractional signal read, transparent samples, unchanged frame and nonfinite refusal; cancellation test adjusted to reach refinement after new conversion/read checkpoints. Native accumulation test inserts disabled HSL between small corrections. Full backend library639PASS3ignored, including24actual FilmCraft HSL scale scenarios, actual animated HSL/mask/instance-split nested FFV1 exports and all prior independent color oracles. Ledger regeneration/check and diff checkPASS. Unified single Lumetri combined stages/order/clamping, section switches/LUT role handling, full frontend/API after shared-path changes and HDR remain unqualified. Full original feature goal remains active.


## Stage409 — authenticated shared float color pipeline regression

Extended real original-video authenticated API preview with two0.9% faded-film corrections separated by neutral vignette/looks/curves/LUT and disabled HSL. Exact opaque source pixelRGB[1,1,254] confirms sub-byte correction accumulation survives all these native section boundaries through API admission/authorized decoder/render/response; byte-rounding per section would lose the red/green correction. Existing single-effect/clip-local animation/range refusal/alpha preview fixtures retained.

Full API70PASS after shared frame changes; explicit updated preview regression rerunPASS (69other tests filtered), with fresh Cargo source-state check and no recompilation needed beyond1.89s. Latest full library639PASS3ignored remains stage408 evidence; frontend837PASS remains stage401 evidence because no frontend code changed in float stages. Ledger regeneration/check and diff checkPASS;56partial/2639unreviewed/0verified. Full authenticated browser E2E, unified single Lumetri combined-stage oracle/order/clamping, HDR and remaining feature catalog remain incomplete. Full goal stays active.


## Stage561 — standalone WeftCut chroma matte ordinary export route

Compared native effect eligibility against ordinary route selection: ChromaMatte was implemented in native flat_surface/chroma_surface (pinned WeftCut Vlahos shader CPU port) but omitted from requires_native_video. Added actual ordinary API fixture with only chroma_matte, no LUT/grading effect that could accidentally select native rendering; real blue FFV1 source, selected500000..1000000 H264 delivery. Before fix fixture FAILED with terminal native chroma matte export is not yet supported. Added standalone ChromaMatte route detection and bumped ordinary dedupe/render cachev5-native-matte so persisted v4 failed jobs do not block new requests after correction.

Final full API82PASS0ignored (23.47s), including actual standalone matte output all16x16pixels blackopaque within4levels, alongside prior ordinary audio/DSP/color/delivery/source authorization fixtures. git diff --check PASS. This fixes ordinary route availability; does not prove new source GPU/f16 differential, out-of-guidance imported parameter compatibility, full browser delivery workflow or original source clocks. Filmcraft Ultra Key is a distinct source contract, so no new Filmcraft parity promotion. Full two-source feature goal remains active.

## Shared local speech and word-caption path (2026-10-17)

Current shared WeftCut/Concat adoption now includes configured local Whisper, durable owner-scoped recognition/progress/cancel/history, managed shutdown/cold recovery with identity/permission refusal, and phrase/word segmentation. Word intervals come from actual Whisper -ml1 -sow, not guessed token durations; native source-clock alignment/Undo/preview/export retains current clip mapping. Actual public learned-model tests cover source2x country interval and a rendered your->country boundary. Latestfrontend1662/type/bothbuilds/API3/native5 qualified; detailed reproducible gates in docs/concat-parity.md and operating contract docs/local-speech.md. This does not close Motif/VLM/TTS/model manager/karaoke/diarization/full-language/platform or complete WeftCut parity. Full original three-source goal remains OPEN.


### Moving numbered-image sequence preview (2026-10-31)

The native program-preview adapter now supplies numbered image-sequence frames through the dynamic source-frame provider, using the root media interpretation frame rate and the existing source clocks, effects, masks and color pipeline. Authorized discovery and the same bounded decoder are shared with mask tracking. Background preview can discover sources using its persisted owner identity; this owner path still needs a dedicated sequence job integration test.

Sequence analysis keys include every discovered member's identity, logical name, storage location and local file revision. Reuse/checkpoint restore requires a complete attestable revision; missing local assets remain lazy and cannot seed unverified cached paths. Before publication, used sources, permissions, scope and revisions are checked again, including originally known revisions of members skipped by warm analysis. Ordinary video cache keys remain unchanged.

Validation: the existing native gate passed 50 tests (9 sequence, 15 tracking, 26 preview API), including video temporal effects and Full HD/4K stabilization. The expanded sequence target then passed all 11 tests. New HTTP raster checks compare moving sequence output with independently bound single images for normal sampling, a missing frame, stationary speed, negative-speed magnitude and explicit reverse clocks with the original one-tick boundary. Replacing the second image changes the rendered frame. A real stabilization regression verifies warm path reuse, invalidation after changing a nonfirst frame and exact equality to a separate cold cache. Images are generated in temporary directories; no tracked pixel fixture was added.

Logs: `backend/target/filmcraft-image-sequence-preview-rust.log` and `backend/target/filmcraft-image-sequence-preview-regression.log`. No browser sequence-preview qualification is claimed. Sequence export, dedicated background-owner/revocation coverage, proxies/offline slates, persistent mask tracking jobs and full cross-application feature parity remain open.


### Private image-sequence background ownership (2026-10-31)

Added a real HTTP/job integration regression for a six-frame private sequence with native warp stabilization. A Space editor submits the task, which runs without a bearer token using its persisted owner; all six sources migrate into the authorized Space namespace and the task reaches Done/ready. An unrelated user receives 403. A second task is held behind the analysis resource permits, the Space administrator removes the submitting editor through the public DB membership API, and releasing the permits produces Error with no result. The focused test passed; log: `backend/target/filmcraft-image-sequence-background.log`. This closes the dedicated owner/revocation test gap noted above, not sequence export or full parity. Durable submission currently fingerprints explicit root bindings; snapshotting discovered sibling membership/content at enqueue time is still open (preview cache revisions do cover siblings at execution time).


### Durable image-sequence queue snapshots (2026-10-31)

New stabilization tasks persist an optional sequence-snapshot map in addition to explicit root content fingerprints. The snapshot includes discovered member IDs, logical names, storage paths and local file revisions/presence. Discovery is metadata-only; submission does not decode or hydrate all frames. Worker execution rediscovers and checks each snapshot before analysis and again before success publication. A changed numbered sibling invalidates the queued task even when the bound first frame is unchanged. Job-access records include all discovered members. The new payload field defaults to empty for older persisted work; existing nonsequence jobs retain their content-fingerprint behavior.

Validation: all 12 image-sequence integration/domain tests passed, including the expanded private background test that holds analysis permits, queues a request, changes the second frame, releases the permits and observes a validation error with no result. It also checks persisted snapshot presence, bearer-token absence, successful authorized owner execution and queued membership revocation. The existing durable video-analysis cancellation/restart/owner regression passed separately (1 test). Logs: `backend/target/filmcraft-image-sequence-snapshot-all.log` and `backend/target/filmcraft-image-sequence-snapshot-regression.log`. The general preview key remains reusable only with complete local revisions; durable snapshots additionally represent unavailable members explicitly. Sequence export and full application parity remain open.


### Full-size PNG frame export (2026-10-31)

Factored native source-frame execution out of the JSON preview response. Added `POST /api/compositions/filmcraft/export-frame` with the same source document, root library bindings, exact timeTicks/timeFrames, effects and captions inputs. It produces an 8-bit RGBA PNG attachment at sequence scale 1, currently admitted up to 3840x2160, with no-store headers. Source authorization, numbered-image lookup/fallback, interpretation, masks and source-revision checks are shared with preview. Export uses its own analysis-cache namespace and a bounded full-size render context. Preview keeps the existing 480x270 fit and JSON response. No full-size pixel hex string is allocated for PNG output.

The source-project panel now offers “Экспортировать кадр PNG” at its selected frame position, preserving exact string identifiers, current source bindings, session authentication and request cancellation. Download object URLs use the existing cleanup lifecycle. This is a still-frame export feature and a reusable native-render foundation; sequence video encoding, audio/video muxing, long-running export jobs, additional formats, HDR export and full application parity remain open.

Validation: 39 native tests passed (13 sequence, 26 preview API). The new real HTTP test decodes a 960x720 PNG, compares all pixels to an independently bound selected still under the same effects/mask document, verifies moving source frames differ, verifies preview remains 360x270 for that sequence and enforces private-source 401/403/owner success after namespace migration. Existing temporal effects and Full HD/4K stabilization preview tests passed; these do not qualify 4K PNG performance. 40 mounted panel tests passed, including PNG Blob download with exact source IDs. Typecheck reported zero errors/warnings; the changed panel test passes ESLint. No new browser export qualification is claimed. Logs: `backend/target/filmcraft-export-frame-{native,regression,ui,typecheck,eslint}.log`. All new test images remain temporary; no tracked media fixture was added.


### Native source-project video encoding bridge (2026-10-31)

Added a trusted internal `filmcraft_video_export::encode_video` adapter from the full-size source-project renderer to the existing bounded FFV1/RGBA Matroska encoder. Sequence settings own dimensions and rational frame rate. Every sample is calculated directly in original 254016000000-Hz ticks with checked i128 arithmetic; encoder microseconds are not used as source clocks. Explicit roots receive content plus file-identity snapshots, and numbered-image catalogs receive full member revision snapshots before/after the complete render. A changed source refuses publication. Encoding stages to a unique temporary file, checks cancellation/deadline and source identities, then links the final output without replacing existing files. This internal bridge is video-only and accepts a trusted caller output path; it is not a new public filesystem-path API.

Validation: a real four-frame image sequence with a missing middle frame and timeline start 9007199254740993 produced FFV1 Matroska. ffprobe verifies one video stream, four frames and 24/1 rate. FFmpeg decodes every output RGBA pixel; the test compares all pixels with independently bound individual still-image renders (frames 8, 9, 9, 11). The same test verifies monotonic encoder progress, existing-file preservation, pre-cancellation, temporary-file cleanup, and refusal/no final output when the second image is changed after frame progress begins. The full-size PNG regression also passed after exposing the reusable render entry. Logs: `backend/target/filmcraft-video-bridge-native.log` and `backend/target/filmcraft-video-bridge-build.log`. No tracked media fixtures were added.

Still open: durable Filmcraft video-export job admission/resume/publication and UI, source audio mixing/muxing, other codecs/formats, HDR and complete cross-application parity. Internal encoder progress is not yet an artifact-readiness event; job integration must publish readiness only after final source checks/link publication succeed.


### Durable owner-scoped Filmcraft video job API (2026-10-31)

Added `POST /api/compositions/filmcraft/video-jobs`, returning 202/jobId with explicit `video_only`/Matroska mode. New durable JobKind `filmcraft_video` participates in dispatch/recovery and the Export resource class. Submission requires authentication and advertised FFV1/Matroska capability, validates the native spec and persists owner, server-generated output UUID, request and complete root/sequence snapshots without bearer/session headers. A dedicated access table gates status, cancellation, retry and job events; source access is rechecked for reads/retry. Workers use persisted owner authorization, including private numbered siblings, and compare enqueue snapshots before encoding and before grant/publication. Worker frame execution acquires its own render permit, avoiding nested acquisition deadlocks.

The worker caps encoder progress at 99 until common job completion publishes the result, grants output access to the owner and registers the output through JobService. Failed/cancelled or unsuccessfully published jobs remove their owned final file and access grant. A recovered open job can discard its own interrupted UUID destination before recomputation. Managed shutdown cancellation remains resumable work rather than a fabricated success/error. Frame checkpointing and SIGKILL/raw-staging cleanup are not qualified by this milestone.

Validation: all 15 image-sequence tests passed, including the new real HTTP job scenario: anonymous 401, unrelated private-source user 403, owner Done with FFV1/video-only metadata, owner download 200, unrelated status 403 and output access denied in DB; queued cancellation; changed second-frame snapshot rejection with no result; persisted payload with snapshot/no token; a queued job retained across managed shutdown, reloaded DB/library/AppState and automatic recovery to Done. Existing exact-pixel video bridge, PNG/full-size preview, source timing, tracking, private background analysis and cache invalidation tests passed together. Logs: `backend/target/filmcraft-video-jobs-native.log` and `backend/target/filmcraft-video-jobs-regression.log`. No tracked media fixture was added.

Still open: source-panel video job controls/history, audio mixing/muxing for source projects, running-worker interruption/crash qualification, additional video formats/HDR and full WeftCut/Filmcraft/Concat parity. This API is a working video-only job path; it does not imply complete audiovisual export parity.


### Source-panel background video export controls (2026-10-31)

Mounted a Filmcraft video-export panel alongside program preview. It submits the current preserved document, exact sequence/source IDs, native startFrames string and selected frame count. The initial count comes from a new exact Rust `durationFrames` manifest field (ceil of track end on the sequence frame grid), rather than floating-point seconds in JavaScript. VideoRequest accepts exactly one of legacy startTicks or startFrames; frame conversion, signed floor and overflow checks remain in Rust. The UI names the current mode explicitly: MKV/FFV1 without audio.

The panel polls authenticated job status, displays progress, supports cancellation and status recovery after polling failure, and downloads the completed owner-scoped artifact as a Blob through authenticated fetch. It validates output UUIDs before constructing the download path. Poll generations reject stale responses; lifecycle disposal aborts requests, clears timers and revokes download URLs. Errors in cancellation/download remain visible and retryable. Switching sequence disposes its monitor rather than cancelling independent server work. Persistent export history remains open.

Validation: 102 mounted UI/API tests passed across the source panel (40), dedicated video controls (6) and API suite (56). New checks cover exact string transport, authenticated submit/poll/cancel/download, encoded job paths, unsafe output rejection, invalid frame-count admission, no-login disable, progress/cancellation, poll recovery without duplicate submission, failed cancellation retry and unmount/late response/Blob cleanup. Typecheck reported zero errors/warnings; new component/tests pass ESLint. The expanded real native job test passed with startFrames=1: decoded first video frame equals an independently bound second still, source import reports durationFrames=120, invalid/overflowing frame positions return 400, and owner download/cancel/queued-revision rejection/managed recovery still pass. The legacy exact-tick video bridge also passed after the request change. Logs: `backend/target/filmcraft-video-controls-{build,native,ui,typecheck,eslint}.log`. No browser visual qualification is claimed.

Still open: source-project audio mixing/muxing, export history and running-worker crash qualification, broader formats/HDR and full multi-application parity. This completes source-panel controls for the implemented video-only job path, not complete audiovisual export.


### Source-project audiovisual export (2026-10-31)

Added optional `withAudio` to the durable Filmcraft video request. Old payloads still default to video-only. Audiovisual admission requires PCM float32 capability and counts both video raw storage and rendered stereo PCM against the staging allowance. The worker reports `video_audio`, `withAudio: true`, and `audioCodec: pcm_f32le`; source identities and owner permissions retain the same prequeue/before/after publication checks. The Matroska encoder receives exactly ceil(video duration * 48000) interleaved stereo float32 samples.

New native audio preparation keeps one SourceProjectAudioRuntime alive across bounded pages, preserving authored root/nested sample rates, clip/track/master DSP and source channel mapping. Final stereo resampling uses rational project-tick phase into 48 kHz and retains a page-edge sample for interpolation across root buffers. Small supported WAV files use the original native reader (128 MiB shared admission); other formats use policy-bound native-rate FFmpeg decoding with a bounded PCM window cache. File callbacks bridge the CPU graph to asynchronous decoding without restarting DSP state. Format conversion is followed by the same Rust linear source resampling as the WAV reader. An initial independent comparison exposed a large FLAC mismatch caused by FFmpeg rate conversion; decoding at the original rate and applying the source interpolation in Rust corrected it. Decoder and mixed-PCM staging files have scoped cleanup, cancellation and deadlines. Running-worker SIGKILL cleanup remains unqualified.

The common frame adapter now ignores audio-only bindings when preparing video pixels, allowing one complete audiovisual binding map. The source panel accepts library entries explicitly typed audio, including non-WAV assets. Its video-export control has an explicit “Добавить звук” option and labels the active mode as stereo PCM 48 kHz; default video-only behavior is retained.

Validation: all 16 native image-sequence/export tests passed. The new HTTP job regression uses private image/audio bindings and actual WAV, FLAC and Matroska-embedded PCM input. It combines original nested audio, speed changes, source channel maps, clip/track DSP and a 44.1 kHz root. Four 24-fps output frames beginning at frame 1 produce two MKV streams and exactly 8000 stereo samples at 48 kHz. Every decoded sample is compared (maximum allowed absolute error < 1e-5) to an independently rendered native root bus plus fractional-start linear conversion; nonzero signal is required. All three input formats pass the same reference. Temporary audio/decoder/raster files are absent after completion. Existing video-only/private job recovery, exact raster, PNG, tracking and cache tests pass together.

104 UI/API tests passed (source panel 41, video controls 7, API suite 56), including non-WAV selection into the production source panel, explicit audio mode transport, authenticated lifecycle and download. Typecheck reports zero errors/warnings; changed control/test files pass ESLint. No browser visual/audio playback qualification is claimed. Evidence remains ignored: `backend/target/filmcraft-av-{native,regression,ui,typecheck,eslint}.log`. All new media is temporary; no tracked PCM/pixel fixture was added.

Still open: full render queue/history UX, interruption/crash/staging qualification, broader codec/export settings, HDR/surround and complete original-application parity. This is a working stereo audiovisual job path, not proof that all features of WeftCut/Filmcraft/Concat have been transferred.


### Owned export staging and killed-worker recovery (2026-10-31)

Filmcraft job intermediates now live under `cache/filmcraft-exports/<server-output-uuid>/`, outside served output files. PCM mixes, decoded windows, raw RGBA and codec staging share that job-owned namespace. The worker validates its output UUID, rejects a non-directory/symlink staging target, resets the previous namespace on retry/recovery and publishes the completed render into outputs with a no-overwrite hard link after final source checks. Normal completion/failure removes the owned staging directory. Queue cancellation also removes owned interrupted staging, any unpublished UUID destination and its access grant without acquiring an Export slot; a task cancelled before its queue mark has the same cleanup path. Other jobs' directories are not scanned or removed.

Validation: all 18 regular image-sequence/export tests passed; one worker-only test is intentionally ignored in the parent suite and explicitly launched in disposable subprocesses by the crash test. The real worker is killed only after a partial raw RGBA file exists and before all 480 frames are written. The test verifies Unix signal 9, persisted Running state and absence of a success result, then seeds additional stale audio/render files in the owned directory. A new process reloads DB/library/AppState, reconciles the interrupted job and recomputes a complete 480-frame 240x180 FFV1 file under the same output UUID. ffprobe validates the complete stream. The owned directory is gone after child shutdown; a neighboring sentinel retains its exact bytes. This is recomputation after recovery, not resumption from a saved encoded-frame prefix.

A separate HTTP regression holds every Export permit, creates a queued job with simulated interrupted raw/final files and an access grant, cancels it and observes file/directory/grant removal before releasing any permit. Its neighboring sentinel is preserved. Existing private ownership/revocation, audiovisual WAV/FLAC/embedded-audio PCM, raster, tracking, queue recovery and stabilization-cache regressions pass together. Logs: `backend/target/filmcraft-export-{crash-native,queued-cleanup,staging-build,staging-regression}.log`; compact record: `backend/target/filmcraft-export-staging-evidence.json`. No tracked media fixture was added.

The process-kill qualification covers native image-sequence video rendering. Decoder/mux-process interruption, platform-specific filesystem behavior, encoded-prefix checkpoints, legacy globally named scratch ownership and full original-app feature parity remain open. Shared reusable analysis caches are retained separately from per-job raw staging.


### Persistent export history and panel reconnection (2026-10-31)

Added authenticated `GET /api/compositions/filmcraft/video-jobs` with owner-bound cursor pagination (1–50 rows, default 20), stable created-time/ID ordering and fresh persisted Job records. Foreign/unknown cursors return the same refusal. Source membership and catalog presence are checked without hydrating source files; revoked/deleted-source jobs are omitted. A filtered empty page retains its next cursor so clients can continue. Responses contain job status/result metadata rather than saved source documents or execution credentials.

The video-export panel loads history after mount/account changes, can open a completed file or resume monitoring/cancellation of an existing job, supports additional pages and manual refresh, and refreshes status after submission/completion. Selecting history never resubmits the original render. Session/request generations ignore stale list/poll responses; disposal aborts history loading. Permission loss clears any prior download state and allows the current editor to start another export. A cancellation response from a previously selected job cannot overwrite the newly selected task's state.

Validation: 111 UI/API tests passed (source panel 41, export controls 13, API 57). New cases cover panel remount, opening completed history without submission, reconnecting to active progress/cancel, empty-page pagination, list error recovery, late response disposal, permission-loss download clearing and authenticated encoded cursor transport. Two focused real native tests passed: the history test verifies anonymous refusal, owner-only three-page coverage, size limits, foreign cursors, source revocation, empty filtered pages, no storage-key mutation and DB/library/AppState reopen; the completed-job test verifies a real Done record and output ID in history alongside owner download and existing cancellation/recovery checks. Typecheck reports zero errors/warnings; changed controls/tests pass ESLint. Logs: `backend/target/filmcraft-video-history-{native,completed,ui,typecheck,eslint}.log`. No browser visual qualification is claimed.

Still open: richer render-queue editing/reordering/replay and export settings, decoder/mux-phase interruption and broader original-app parity. This adds persistent history/reconnection; it does not prove all application features transferred.


### Policy-controlled retry from export history (2026-10-31)

The source export panel now exposes retry for eligible error/interrupted jobs through the existing authenticated `/api/jobs/:id/retry` endpoint. It sends only the persisted job ID, so current editor document/range/bindings cannot replace the failed request. Retry uses the server's original request/snapshot and output UUID. Selection switching is locked during admission, stale account responses are ignored and policy refusal remains visible with the failed selection intact. Validation/security/internal failures are not offered as retryable; the server still owns retry counts and retained-payload admission. No backend retry policy or terminal payload-retention rule was broadened.

Validation: 115 UI/API tests passed (source panel 41, export controls 16, API 58), covering historical-job retry without a new export submission, authenticated escaped retry transport, policy-refusal feedback and validation-error exclusion, alongside existing history/session/download behavior. Typecheck reports zero errors/warnings; changed controls/tests pass ESLint. A focused native integration test deterministically reproduces the durable crash window after a Storage failure is committed and before automatic retry scheduling runs. Foreign retry returns 403 with zero operator actions; owner retry executes the retained private-source request to Done under the same job/output IDs and original dimensions/duration, with failed and later successful attempt records and one authorized operator action. The storage failure is fault-injected in durable state; no physical disk outage is claimed. Logs: `backend/target/filmcraft-export-retry-{native,ui,typecheck,eslint}.log`.

Still open: unrestricted replay of erased terminal recipes, richer queue editing/reordering, broader export codecs/settings and complete original-app parity. This action is the existing policy-controlled retry, not a promise to rerun every terminal failure.


### Multiple outstanding exports from the source panel (2026-10-31)

Separated export admission from monitoring state. After a 202 response the panel can accept another range while earlier work remains running/pending. Range/audio controls lock during submission/retry rather than for the entire render. Accepted work becomes the selected monitor; prior jobs continue independently and remain accessible through history. Submission generations prevent an older poll's completion from unlocking a newer in-flight POST. Poll generations reject an earlier job's late result after selection changes. A failed new submission retains the previous job/progress/cancellation and has a separate error state that ongoing polling cannot clear.

Validation: 118 UI/API tests passed (source panel 41, export controls 19, API 58). New cases queue a changed range while the first status request is unresolved, reject a stale completion after accepting another job, preserve the admission lock against an older finally block and retain progress/cancellation after queue-admission refusal. A real native HTTP integration queues two private-source requests while all Export slots are held, releases admission and verifies two distinct owned artifacts. Each decoded raster equals an independently bound still at its requested source clock; the outputs have different pixels and IDs. This proves independent queued requests/results, not a measured parallel-render throughput claim. Typecheck reports zero errors/warnings; changed controls/tests pass ESLint. Logs: `backend/target/filmcraft-export-multiqueue-{native,ui,typecheck,eslint}.log`.

Queue priority/reordering, richer batch/export settings and full original-app parity remain open. No browser visual qualification or tracked media fixture was added.
