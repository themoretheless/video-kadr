# Следующие 100 research-backed идей

Дата сверки: 18 июля 2026. Это новый слой №884-983 поверх исходных 565
SOLID/DRY-пунктов и первого исследования №784-883. Здесь не предлагается слепо
копировать чужую архитектуру или добавлять 40 зависимостей. Каждый источник
даёт проверяемый контракт, эксперимент или критерий приёмки для текущего
проекта.

## Как читать

- Ровно 100 карточек разбиты на 10 независимых пакетов по 10.
- Сначала вводится чистый тип/порт и fixture, затем adapter, затем UI.
- Производные данные всегда удаляемы и воспроизводимы из оригинала и проекта.
- Новая dependency принимается только после license, binary-size и benchmark gate.
- Статус и порядок исполнения дублируются в `recommendation.md`; архитектурный
  диагноз - в `architecture.md`.

## Первичные источники

**Media/container:** [MediaInfo](https://github.com/MediaArea/MediaInfo),
[Bento4](https://github.com/axiomatic-systems/Bento4),
[GPAC](https://github.com/gpac/gpac),
[libmatroska](https://github.com/Matroska-Org/libmatroska) и
[ExifTool](https://github.com/exiftool/exiftool). Они отделяют технический probe,
container structure, tracks/items и metadata policy; из этого следует typed
ingest envelope вместо дальнейшего расширения одного ffprobe JSON.

**Color/HDR:** [OpenColorIO](https://github.com/AcademySoftwareFoundation/OpenColorIO),
[OpenEXR](https://github.com/AcademySoftwareFoundation/openexr),
[ACES core](https://github.com/aces-aswf/aces-core),
[libplacebo](https://github.com/haasn/libplacebo) и
[zimg](https://github.com/sekrit-twc/zimg). Их общий урок: primaries, transfer,
matrix, range, chroma siting, display transform и config version являются
данными pipeline, а не неявными флагами последнего encoder.

**Audio:** [Ardour](https://github.com/Ardour/ardour),
[Audacity](https://github.com/audacity/audacity),
[PipeWire](https://github.com/PipeWire/pipewire),
[Rubber Band](https://github.com/breakfastquay/rubberband) и
[libebur128](https://github.com/jiixyj/libebur128). Они дают отдельный audio
graph, latency/sample contracts, offline/realtime time-stretch profiles и
измеримое loudness вместо одного `volume` в video DTO.

**Timed text/accessibility:** [WebVTT](https://www.w3.org/TR/webvtt/),
[MAUR](https://www.w3.org/TR/media-accessibility-reqs/),
[libass](https://github.com/libass/libass),
[Aegisub](https://github.com/Aegisub/Aegisub) и
[Subtitle Edit](https://github.com/SubtitleEdit/subtitleedit). Cue overlap,
regions, chapters, descriptions, fonts, reading speed и device-independent
controls должны быть частью модели и QA.

**Local-first/storage:** [Automerge](https://github.com/automerge/automerge),
[Yjs](https://github.com/yjs/yjs),
[Local-first software](https://www.inkandswitch.com/local-first/),
[Litestream](https://github.com/benbjohnson/litestream) и
[tusd](https://github.com/tus/tusd). Синхронизируются маленькие операции и
state vectors; большие media blobs остаются content-addressed объектами.

**Isolation:** [NsJail](https://github.com/google/nsjail),
[Bubblewrap](https://github.com/containers/bubblewrap),
[Wasmtime](https://github.com/bytecodealliance/wasmtime) и
[Firecracker](https://github.com/firecracker-microvm/firecracker). Sandbox - это
версионированная policy из filesystem/network/syscall/resource controls, а не
факт наличия одного executable.

**Reliability:** [MadSim](https://github.com/madsim-rs/madsim),
[fail](https://docs.rs/fail/latest/fail/),
[Toxiproxy](https://github.com/Shopify/toxiproxy) и
[HdrHistogram Rust](https://github.com/HdrHistogram/HdrHistogram_rust). Нужны
replayable schedules, fault points, network faults и tail latency без
coordinated omission.

**Frontend/UX:** [React Spectrum](https://github.com/adobe/react-spectrum),
[Radix Primitives](https://github.com/radix-ui/primitives),
[TanStack Virtual](https://github.com/TanStack/virtual),
[axe-core](https://github.com/dequelabs/axe-core) и
[WAI-ARIA APG](https://www.w3.org/WAI/ARIA/apg/). Качество editor UI означает
одинаковый semantic contract для мыши, touch, клавиатуры и assistive technology,
включая скрытые до взаимодействия состояния.

**Verification:** [Proptest](https://github.com/proptest-rs/proptest),
[Kani](https://github.com/model-checking/kani),
[cargo-mutants](https://github.com/sourcefrog/cargo-mutants),
[Nextest](https://github.com/nextest-rs/nextest) и
[Testcontainers for Rust](https://github.com/testcontainers/testcontainers-rs).
Example tests дополняются properties, model checking, mutation score и
реальными disposable dependencies.

**Local ML/privacy:** [Candle](https://github.com/huggingface/candle),
[tract](https://github.com/sonos/tract),
[Presidio](https://github.com/microsoft/presidio),
[NIST AI RMF](https://www.nist.gov/itl/ai-risk-management-framework),
[Model Cards](https://arxiv.org/abs/1810.03993) и
[Datasheets for Datasets](https://arxiv.org/abs/1803.09010). Любой inference
остаётся versioned, scoped, privacy-aware suggestion, а не скрытым источником
истины.

## K. Container metadata и provenance (884-893)

884. 🟠 **ProbeEnvelope:** разделить normalized metadata и bounded raw diagnostics; один malformed optional tag не отменяет импорт валидных streams. Приёмка: golden corpus MediaInfo/ffprobe даёт одинаковое domain-представление.
885. 🟠 **Stable stream identity:** ссылаться на track ID + media kind + language/disposition, не на индекс массива. Приёмка: reorder streams не ломает выбранные audio/caption tracks.
886. 🟠 **Metadata privacy policy:** сохранять allowlist title/language/chapter, а location/device/author/query-like tags удалять по умолчанию. Приёмка: canary PII отсутствует в export и diagnostic bundle.
887. 🟡 **Post-mux conformance:** после export проверять container structure, duration и обязательные tracks через независимый probe. Приёмка: truncated MP4 и неверный manifest не публикуются.
888. 🟠 **Rational media time:** time base, duration и timestamps хранить как checked rational/ticks, float оставлять только UI-adapter. Приёмка: длинный 30000/1001 материал не накапливает frame drift.
889. 🟠 **Display transform:** rotation, sample aspect ratio и display matrix компилировать в один typed transform. Приёмка: portrait/anamorphic fixtures совпадают в preview, proxy и export.
890. 🟡 **Attachment policy:** fonts/covers/Matroska attachments импортировать только по MIME/size/count allowlist. Приёмка: attachment bomb и executable payload отклоняются до публикации.
891. 🟡 **Chapters/timecode:** импортировать chapters и source timecode как недеструктивные marker tracks со stable IDs. Приёмка: trim/reorder сохраняет связь с source ticks.
892. 🟠 **Source provenance:** рядом с source хранить checksum, probe/tool versions, ingest method и sanitized origin. Приёмка: report объясняет, каким adapter создан artifact, не раскрывая URL secrets.
893. 🟡 **Capability negotiation:** container/codec/metadata adapter выбирается по typed runtime capabilities, а не по extension/string fallback. Приёмка: unsupported combination возвращает причину до spawn.

## L. Color, HDR и image pipeline (894-903)

894. 🟠 **ColorDescriptor:** primaries/transfer/matrix/range/chroma location становятся обязательным typed входом render plan с explicit `Unspecified`. Приёмка: ни один conversion не угадывает HDR по filename.
895. 🟠 **Color round-trip gate:** probe до и после filter/encode сравнивает descriptor и mastering metadata. Приёмка: metadata loss становится ошибкой или явным approved conversion.
896. 🟡 **OCIO config identity:** path не является идентичностью; config checksum/version входит в artifact fingerprint. Приёмка: смена config invalidates только color-dependent derivatives.
897. 🟡 **Scene-linear working space:** compositing/effects могут opt-in в ACES scene-linear, но простые cuts не получают лишний conversion. Приёмка: graph явно показывает каждый transform.
898. 🟡 **HDR frame artifact:** промежуточный high-precision frame port поддерживает OpenEXR/float16 без привязки domain к библиотеке. Приёмка: clipping test сохраняет значения выше SDR white.
899. 🟠 **Display-aware preview:** preview profile задаёт display peak/transfer и deterministic tone-map; export descriptor остаётся независимым. Приёмка: SDR display preview не переписывает HDR output spec.
900. 🟠 **Reference conversion corpus:** zimg/libplacebo/FFmpeg conversions сверяются на primaries/range/chroma fixtures с pixel tolerance. Приёмка: CPU и GPU paths имеют зафиксированную допустимую дельту.
901. 🟠 **HDR metadata validation:** MaxCLL/MaxFALL/mastering values проверяются на finite/range/consistency до encode. Приёмка: malformed metadata не проходит как нулевое значение.
902. 🟡 **Gamut/clipping UX:** scopes показывают clipped highlights и out-of-gamut pixels, а conversion требует осознанного preset. Приёмка: warning имеет понятный fix и не блокирует lossless passthrough.
903. 🟡 **Color QA report:** per-scene histogram, clipping ratio и perceptual color delta хранятся отдельно от codec quality. Приёмка: отчёт сравнивает только одинаковые display assumptions.

## M. Audio graph, sync и loudness (904-913)

904. 🟠 **AudioTime:** audio ranges хранятся в sample ticks/rate, video ranges - в frame ticks; conversion checked на boundary. Приёмка: repeated trims не теряют и не дублируют sample.
905. 🟠 **Latency compensation:** каждый audio effect объявляет latency, graph автоматически выравнивает parallel paths. Приёмка: impulse fixture после mix остаётся sample-aligned.
906. 🟠 **ChannelLayout:** mono/stereo/5.1 и channel labels типизированы; количество channels не заменяет layout. Приёмка: downmix требует явной matrix policy.
907. 🟠 **Two-pass loudness:** measurement artifact хранит integrated LUFS/LRA/true peak; normalize использует его вторым проходом. Приёмка: EBU fixture попадает в target tolerance без clipping.
908. 🟠 **True-peak guard:** limiter policy отделена от loudness target и измеряется после encode. Приёмка: lossy codec overshoot выше ceiling отклоняет публикацию.
909. 🟡 **Waveform pyramid:** content-addressed min/max/RMS tiles нескольких уровней, а не один огромный PCM/JSON. Приёмка: zoom читает только видимые tiles и удаление cache безопасно.
910. 🟡 **Stretch profiles:** realtime preview и high-quality offline time-stretch - разные adapters общего port. Приёмка: preview setting не меняет выбранный export algorithm.
911. 🟠 **Audio render graph:** video DTO больше не владеет audio effects; общий timeline синхронизирует независимые audio/video plans. Приёмка: audio-only export не компилирует video filters.
912. 🟠 **Non-finite sanitizer:** NaN/Inf/denormal и неожиданный silence детектируются между plugins/effects. Приёмка: job завершается typed error с безопасным sample range, не битым файлом.
913. 🟠 **A/V drift corpus:** VFR, 44.1/48 kHz, speed, cuts и concat проверяются по start/end sync tolerance. Приёмка: p95 drift задан в samples/frames, не визуальным впечатлением.

## N. Captions, localization и media accessibility (914-923)

914. 🟠 **CaptionTrack/Cue:** stable cue ID, rational range, payload, language, speaker и region отделены от UI rows. Приёмка: reorder/translation не меняет identity.
915. 🟠 **Strict WebVTT boundary:** parser валидирует UTF-8, timestamps, settings и bounded cue count/size; storage сохраняет unknown future metadata отдельно. Приёмка: round-trip corpus соответствует spec.
916. 🟡 **Overlap semantics:** overlap разрешён для captions/metadata, но chapters валидируются отдельно. Приёмка: policy зависит от track kind, а не от одного глобального запрета.
917. 🟠 **Safe-region preview:** cue region/title-safe guides используют output aspect и не кодируются в сам текст. Приёмка: portrait/landscape screenshots не обрезают caption.
918. 🟠 **Reference subtitle render:** ASS/SSA golden images сверяются с libass на fonts, bidi, outlines и positioning. Приёмка: unsupported tag даёт warning, а не тихо иной export.
919. 🟠 **Font attachment boundary:** font checksum/license/size и fallback chain входят в caption artifact. Приёмка: отсутствующий font воспроизводимо отмечен до export.
920. 🟡 **Readability linter:** configurable characters-per-second, line length, line count и minimum gap дают advisory findings. Приёмка: пользователь может перейти к каждому cue и исправить его.
921. 🟡 **Waveform cue editing:** drag start/end/move и split/merge работают одной transaction с keyboard equivalents. Приёмка: один gesture создаёт один undo и сохраняет cue duration invariants.
922. 🟡 **Linked translations:** original/translation связываются cue ID и alignment relation, не общим индексом массива. Приёмка: insert/delete явно показывает unmatched cue.
923. 🟠 **Accessible-media audit:** export report проверяет captions, descriptions, chapters, language labels и keyboard-operable controls. Приёмка: автоматические findings отделены от manual review.

## O. Local-first collaboration, storage и upload (924-933)

924. 🟠 **Append-only project operations:** persisted project хранит stable operation IDs и snapshots, а не только последний mutable JSON. Приёмка: replay строит тот же fingerprint.
925. 🟠 **CRDT scope boundary:** CRDT применяется к timeline metadata/comments, большие media/proxy blobs передаются content-addressed отдельно. Приёмка: project sync не сериализует media bytes.
926. 🟡 **State-vector sync:** клиент обменивает только missing updates и принимает повтор в любом порядке. Приёмка: duplicate/reordered delivery идемпотентна.
927. 🟠 **Selective undo origin:** undo откатывает операции текущего пользователя/transaction origin, не чужие concurrent edits. Приёмка: двухклиентский test сохраняет remote move.
928. 🟠 **Snapshot/compaction policy:** operation log compacted только после durable snapshot и acknowledgement horizon. Приёмка: offline client старой версии может безопасно resync.
929. 🟠 **Continuous SQLite recovery:** optional Litestream-like adapter реплицирует WAL, но restore drill остаётся обязательным. Приёмка: RPO/RTO измерены, corrupted replica не заменяет local source silently.
930. 🟠 **Resumable upload protocol:** upload ID, offset, checksum и expiry персистятся; retry не начинает гигабайтный файл заново. Приёмка: disconnect/restart продолжает с подтверждённого offset.
931. 🟡 **Blob dedupe:** sources адресуются checksum внутри storage, project ownership хранится отдельно. Приёмка: удаление одной ссылки не удаляет blob с другим owner/reference.
932. 🟡 **Single-writer fallback:** до полноценной collaboration project lease/advisory lock предотвращает silent last-write-wins. Приёмка: второй editor получает read-only/conflict state.
933. 🟠 **Sharing privacy:** sync выключен по умолчанию; sharing показывает media scope, recipients, encryption и revoke semantics. Приёмка: diagnostic/logging не содержит operation payload.

## P. Process isolation и plugin security (934-943)

934. 🔴 **ProcessPolicy port:** каждый external tool объявляет filesystem, network, env, CPU, memory, PID, FD и output limits. Приёмка: spawn невозможен без policy profile.
935. 🔴 **Linux NsJail adapter:** server profile запускает untrusted media tools в namespaces/cgroup/seccomp с dropped privileges. Приёмка: fixture не читает host home и не создаёт extra process.
936. 🔴 **Filesystem allowlist:** Bubblewrap policy монтирует source read-only, staging writable, остальное скрыто. Приёмка: symlink/path escape test не достигает storage siblings.
937. 🔴 **Network deny by default:** ffprobe/ffmpeg локального source не имеют network namespace; только download adapter получает pinned egress. Приёмка: crafted playlist не делает outbound request.
938. 🟠 **Versioned seccomp policy:** syscall profile хранит версию/tool fingerprint и regression suite реальных codecs. Приёмка: upgrade FFmpeg либо проходит corpus, либо требует review policy diff.
939. 🔴 **Kernel resource limits:** cgroup/rlimit ограничивают CPU time, RSS, PIDs, open files и file size независимо от cooperative app cancellation. Приёмка: adversarial media не истощает host.
940. 🟠 **Per-tenant identity:** multi-user deployment использует отдельные uid/gid/work directories и ownership checks. Приёмка: job одного tenant не открывает path другого.
941. 🟡 **Wasm plugin capability model:** будущие user effects получают explicit host calls, memory/fuel/epoch limits. Приёмка: plugin без media-read capability не видит source.
942. 🟠 **Quarantine custom logic:** custom shaders/scripts/filters не входят в trusted default path и требуют signed descriptor + sandbox tier. Приёмка: project import не исполняет embedded code.
943. 🔴 **Isolation tiers ADR:** local single-user, LAN и public multi-tenant profiles имеют разные обязательные controls и startup refusal. Приёмка: public bind без required tier не стартует.

## Q. Reliability, tail latency и operability (944-953)

944. 🟠 **Artifact failpoints:** inject errors before hash, rename, manifest write и directory sync. Приёмка: каждый crash point оставляет старую версию или безопасный retry, не mixed state.
945. 🟠 **Deterministic lifecycle simulation:** cancel/finish/retry/lease/shutdown schedules воспроизводятся seed. Приёмка: найденный seed сохраняется regression test.
946. 🟠 **Network fault matrix:** URL import проходит latency, reset, partial body, slow close и redirect interruption. Приёмка: retry taxonomy и cleanup проверяются реальным proxy.
947. 🟠 **Tail histogram:** queue wait, first preview frame, probe и render phases пишутся в HDR histogram с corrected sampling. Приёмка: p50/p95/p99 не вычисляются из averages.
948. 🟡 **Phase spans:** ingest/probe/hash/proxy/plan/encode/package/publish имеют stable low-cardinality fields и artifact IDs. Приёмка: один job trace объясняет critical path без private URL.
949. 🟡 **User-facing SLO:** отдельно задать availability API, preview first-frame latency и successful export completion. Приёмка: SLO имеет окно, threshold и источник измерения.
950. 🟠 **Class-aware load shedding:** preview, upload, analysis и export имеют независимые gates/priority; admission возвращает retry-after. Приёмка: saturated export не блокирует navigation/preview.
951. 🟠 **Retry budget/circuit breaker:** repeated tool/network failures ограничиваются по source/tool, а не создают retry storm. Приёмка: half-open probe и manual retry наблюдаемы.
952. 🟠 **Crash-only reconciliation:** startup сканирует staging/manifests/leases по ownership и age, не удаляет всё подряд. Приёмка: valid resumable chunk сохраняется, orphan безопасно очищается.
953. 🟡 **Baseline comparator:** tool сравнивает matching schema/environment, median/p95 и требует три повторения threshold. Приёмка: noisy single run не красит CI.

## R. Timeline UI/UX и accessibility (954-963)

954. 🟠 **Semantic design tokens:** surface/text/border/accent/danger/focus tokens отделены от raw palette и проверяются light/dark/high-contrast. Приёмка: feature CSS не импортирует hex colors напрямую.
955. 🟠 **Toolbar interaction contract:** roving tabindex, arrow navigation, labels/tooltips и disabled reason едины. Приёмка: полный toolbar доступен без pointer.
956. 🟡 **Command registry/palette:** action ID, icon, label, availability и execute живут в одном registry. Приёмка: menu/shortcut/palette вызывают одну command, не три handlers.
957. 🟠 **Virtualized timeline:** только видимые clips/tracks/markers рендерятся, размеры tracks стабильны. Приёмка: 10k clips сохраняют interaction p95 и scroll anchor.
958. 🟠 **Keyboard spatial editing:** nudge, resize, slip и coarse/fine increments имеют visible focus and announcements. Приёмка: pointer и keyboard компилируют одинаковую domain transaction.
959. 🟠 **Timeline list alternative:** screen reader получает упорядоченный список tracks/clips/ranges/actions вместо попытки озвучить canvas. Приёмка: selection синхронизирован в обе стороны.
960. 🟠 **Input parity matrix:** mouse/touch/pen/keyboard имеют явные gestures, cancellation и capture cleanup. Приёмка: mobile resize не зависит от hover/right-click.
961. 🟠 **Overlay focus lifecycle:** dialog/menu/popover trap, Escape, outside action и focus return реализованы одним primitive. Приёмка: nested overlay не теряет trigger focus.
962. 🟡 **Reduced motion/state persistence:** motion отключается по preference, прогресс/selection не передаются только анимацией. Приёмка: reduced-motion visual suite остаётся понятным.
963. 🟠 **Stateful a11y gate:** axe запускается после открытия dialogs/menus/errors и дополняется keyboard + screen-reader checklist. Приёмка: hidden interactive states входят в CI screenshots/tests.

## S. Property, mutation и formal verification (964-973)

964. 🟠 **EditRequest properties:** arbitrary finite/non-finite ranges проверяют normalize idempotence и plan invariants. Приёмка: minimal failing cases сохраняются в corpus.
965. 🟠 **Artifact properties:** arbitrary path/manifest trees доказывают no escape, identity binding и reconcile idempotence. Приёмка: symlink/Unicode/path separator cases входят в seeds.
966. 🟠 **Job state-machine properties:** generated command sequences никогда не создают второй terminal transition или lost permit. Приёмка: model и SQLite adapter сравниваются после каждой команды.
967. 🟠 **Kani arithmetic proofs:** frame/sample/tick conversion и chunk boundaries не overflow и не создают gaps. Приёмка: bounded proof harness запускается nightly.
968. 🟡 **Mutation gate:** validators, retry taxonomy, redaction и artifact verification проходят targeted cargo-mutants. Приёмка: survived mutants triaged, critical score не падает.
969. 🟡 **Nextest profiles:** unit/integration/media/slow/flaky разделены timeout/retry policy и JUnit output. Приёмка: обычный PR не скрывает flaky retry как pass без отчёта.
970. 🟡 **Disposable dependency tests:** SQLite/filesystem/egress/packager adapters тестируются в reproducible containers там, где host variance существенна. Приёмка: version pin записан в report.
971. 🟠 **Golden media corpus:** tiny licensed fixtures покрывают VFR/HDR/rotation/channels/subtitles/corruption. Приёмка: checksums и expected probe/export metadata versioned.
972. 🟠 **Metamorphic editor tests:** split+merge, apply+undo, proxy+original и chunk+stitch сохраняют согласованные properties. Приёмка: тест не зависит от одного hard-coded output.
973. 🟠 **Differential adapter tests:** generated FFmpeg invocation проверяется фактическим ffprobe output/reference renderer. Приёмка: compiler test доказывает семантику, не только строку args.

## T. Local ML, privacy и model governance (974-983)

974. 🟠 **InferenceProvider port:** local/remote providers имеют общий typed request/result, remote отсутствует по умолчанию. Приёмка: domain не импортирует SDK/model runtime.
975. 🟡 **Candle local adapter experiment:** feature-gated Rust inference измеряется на representative CPU/GPU и binary size. Приёмка: dependency принимается только после matrix.
976. 🟡 **tract ONNX adapter experiment:** CPU/offline backend сравнивается по compatibility, memory и latency с subprocess вариантом. Приёмка: backend заменяем через тот же port.
977. 🟠 **Model artifact descriptor:** checksum, source/license, version, task, languages, quantization и limitations обязательны. Приёмка: unknown model не запускается.
978. 🟠 **Explicit inference scope:** пользователь видит source, time range, derived inputs и ожидаемый output до запуска. Приёмка: full-library scan никогда не начинается из локального clip action.
979. 🔴 **PII boundary before remote:** transcript/OCR/frames проходят configurable detection/redaction или explicit consent. Приёмка: canary PII не покидает local transport в default profile.
980. 🟠 **Ephemeral ML staging:** inputs/results имеют TTL, no-training/no-telemetry policy и audited deletion. Приёмка: cancel/shutdown cleanup не оставляет remote payload cache.
981. 🟠 **Human-confirmed suggestions:** transcript cuts/object tracks/redaction masks сначала draft с confidence/provenance. Приёмка: model result не изменяет `EditPlan` без command confirmation.
982. 🟡 **Versioned evaluation corpus:** accuracy, latency, memory и subgroup/language failures сравниваются при смене model. Приёмка: drift report блокирует silent default upgrade.
983. 🟠 **Disposable ML artifacts:** transcript/OCR/scenes/tracks зависят от source+model+params fingerprints и могут быть удалены/recomputed. Приёмка: project и originals открываются без ML cache.

## Исполнение пакетами

| Пакет | IDs | Первый вертикальный срез |
|---|---|---|
| K | 884-893 | `ProbeEnvelope` + metadata privacy fixture |
| L | 894-903 | `ColorDescriptor` + round-trip corpus |
| M | 904-913 | `AudioTime`/`ChannelLayout` + drift fixture |
| N | 914-923 | `CaptionTrack` + strict WebVTT boundary |
| O | 924-933 | operation log + resumable upload persistence |
| P | 934-943 | `ProcessPolicy` + public-profile startup refusal |
| Q | 944-953 | artifact failpoints + tail histogram |
| R | 954-963 | semantic tokens + keyboard/a11y state matrix |
| S | 964-973 | artifact/edit properties + targeted mutation run |
| T | 974-983 | `InferenceProvider` + model descriptor/privacy gate |

Приоритет следующего исполнения: сначала P (934, 937, 939, 943), затем K/L/M
domain contracts (884, 888, 894, 904, 906), затем R/S quality gates (954, 955,
963, 964, 965). Collaboration и ML не должны опережать ownership, process
isolation и Timeline-IR.
