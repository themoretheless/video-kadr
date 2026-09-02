# Рекомендации: что делать дальше (чеклист)

Приоритизированный, гранулярный план, синхронизированный с
[architecture.md](architecture.md) (целевой дизайн),
[docs/refactor-plan.md](docs/refactor-plan.md) (шаги рефакторинга) и
[docs/ideas/round-13.md](docs/ideas/round-13.md) (фичи). Исследовательское
обоснование №784-883 - в [docs/research-100.md](docs/research-100.md),
следующих №884-983 - в
[docs/research-next-100.md](docs/research-next-100.md).

Порядок: **корректность → дешёвая модульность → глубокий рефактор**; фичи
параллельно. Каждая задача идёт под зелёным `make check`. **S** = часы, **M** =
день-два, **L** = неделя+. У каждой задачи - файлы, шаги, критерий приёмки.

Фазовый порядок исполнения (P0-A/B → P3) - в [plan.md](plan.md). Полный широкий
аудит (509 заземлённых на код проблем) - в [docs/audit-500.md](docs/audit-500.md);
проверенное ядро - в [docs/audit.md](docs/audit.md).

## Волны исполнения по 10 пунктов

Все research-backed карточки №784-883 разложены ровно на 10 волн без повторов.
Внутри волны сначала вводится контракт/fixture, затем реализация и общий verify.

| Волна | Статус | Пункты | Фокус |
|---:|---|---|---|
| 1 | ✅ 10/10 | 790, 824, 828, 829, 831, 844, 846, 847, 850, 854 | Runtime/contract/security/test guardrails |
| 2 | ✅ 10/10 | 784, 786, 787, 803, 814, 817, 820, 823, 825, 826 | Typed media/timeline domain и HTTP ports |
| 3 | ✅ 10/10 | 834, 835, 836, 837, 838, 839, 840, 842, 843, 870 | Durable jobs, outbox, replay и concurrency invariants |
| 4 | ✅ 10/10 | 789, 791, 792, 793, 795, 796, 798, 832, 871, 872 | Proxy/render artifacts и измеряемый media performance |
| 5 | ✅ 10/10 | 785, 804, 805, 806, 807, 808, 809, 810, 815, 818 | Player/canvas state machines и accessibility |
| 6 | ☑ | 794, 797, 799, 800, 801, 816, 819, 821, 822, 827 | Quality/codecs/design tokens и benchmarks |
| 7 | ☑ | 833, 855, 856, 857, 858, 859, 860, 861, 862, 863 | Deployment security, fuzzing и supply chain |
| 8 | ☑ | 812, 813, 830, 864, 865, 866, 867, 868, 869, 873 | Resource classes, SQL contract и observability |
| 9 | ☐ | 874, 875, 876, 877, 878, 879, 880, 881, 882, 883 | Versioned local-first ML artifacts |
| 10 | ☐ | 788, 802, 811, 841, 845, 848, 849, 851, 852, 853 | Compatibility, ingest и frontend completion |

Волны 1-4 проверяются `make check`: backend unit/API/upload/backpressure/render
suites, frontend lint/typecheck/Vitest/build/budget и 9 Playwright сценариев.
Волна 2 добавила pure-domain media/timeline primitives, общий Rust/TS geometry
corpus, command history и port-based system/project routers. Детали
upload-контролей - в [docs/threat-model-upload.md](docs/threat-model-upload.md).

Волна 3 добавила replayable job events, attempts/error taxonomy, failed registry,
dedupe/rate limit, lifecycle reconciliation и transactional outbox с пятью crash
failpoints. Attempt heartbeat, due-retry recovery, graceful-restart semantics и
legacy event backfill закрывают найденные review gaps; terminal payload стирается,
но metadata остаётся в rate-accounting окне.
`JobCell` проверяется Loom и публикует transition после durable write.
Backup проходит checksum/integrity/restore drill, поиск работает через
rebuildable SQLite FTS5 port, а отказ от RocksDB закреплён измеримым WAL gate.

Волна 4 добавила checksum/path-bound proxy, frame, chunk и package artifacts,
single-flight generation и downstream invalidation. Общий immutable `EditPlan`
имеет независимые preview/export profiles. cgroup-aware `EncodeBudget` применён
к production FFmpeg, hashing работает через bounded Rayon pool, perf corpus и
profile scripts versioned. Третья ревизия закрыла muxer suffix и безлимитный
proxy progress, serde invariant bypass, фактически bounded manifest reads,
ancestor-symlink escape при artifact verify и oversubscription через отдельный
render admission.
Следующий маленький integration slice - подключить
готовые proxy/chunk/package ports к пользовательским workflows; это не скрыто в
статусе contract-слоя.

Typed export slice от 20 июля 2026 закрыл backend-часть №736, 741, 745, 751,
758, 760, 762, 768, 772 и 781. Wire `EditRequest` преобразуется в source-aware
`EditPlan` v2 с отдельными `EditSpec`/`OutputSpec`; `ExportCommandCompiler`
изолирует application layer от FFmpeg. Source duration больше не передаётся
adapter вторым аргументом, а args/progress duration рождаются одним compile
result. Следующий приоритет этого направления: №742/101 (cache key по normalized
plan), №737 (generated Rust/TS contract) и №750 (Timeline -> EditPlan compiler).

Process-isolation P0 от 19 июля 2026 закрыл №934 и №943. Production spawn теперь
возможен только через typed `ProcessPolicy`/`PreparedCommand`; config/state
передают один runtime, environment очищается, output/lines bounded, downloader
имеет только pinned loopback proxy, FFmpeg/ffprobe offline protocol allowlist.
Unix rlimits закрывают CPU/FD/file size, Linux также address space. №936, 937 и
939 остаются частично открыты до mount/network namespaces, cgroup pids/memory и
per-tenant uid; точная матрица в `docs/process-isolation.md`.

## Синхронизированный top-200: проблема → куда чинить

**Раунд 2 (1 июля 2026).** P0-1…P0-8 ниже все ✅ «Сделано» - но перепроверка
текущего кода (11 агентов, adversarial-verified) нашла 2 новых high-пункта и
16 пунктов поменьше, часть из них - остаточные гэпы в самих P0-фиксах. Полный
ранжированный список - «Топ-50 актуальных проблем» в
[docs/audit.md](docs/audit.md); новые находки 201-218 - там же в разделе
«Раунд 2: новые находки». Ключевое: P0-3 (SSRF) закрыл DNS-резолвинг, но не
редирект `yt-dlp` (audit.md №201, новый P0); P0-фиксы cancellation (596327b)
закрыли терминальные переходы, но не переход в `Running` (audit.md №203); upload
остаётся без magic-byte проверки и это теперь подтверждённый PoC XSS, не гипотеза
(audit.md №202, новый P0). Это исторический снимок: №202/203 закрыты в раунде 5,
№201 - в раунде 6.

Номера совпадают с диагностическим списком в `architecture.md`; подробные
file:line и доказательства - в [docs/audit.md](docs/audit.md).

1. ✅ Cancel/finish race → P0-7 и P2-12.
2. ✅ Мусор после отмены/таймаута импорта → P0-7.
3. ✅ Cancel окна на cache-hit edit → P2-12.
4. ✅ Осиротевший child `ffmpeg` из `yt-dlp` → P2-9.
5. ✅ Shutdown владеет workers/processes, слушает SIGINT/SIGTERM и ограничивает HTTP/task drain → wave 1 №824.
6. ✅ Queued job нельзя отменить сразу → P0-4 и P2-9.
7. ✅ `acquire_owned()` error переводит job в terminal error → P0-4.
8. ✅ Progress drain обновляет только open job → P2-9/P2-12.
9. ✅ Segments не работают для AV1/ProRes → P0-6.
10. ✅ Crop не валидируется по source dimensions → P0-8.
11. Overlay может выйти за кадр → P0-8.
12. ✅ Сегменты не сортируются → P0-6/P2-13.
13. ✅ Segment end клампится к duration → P0-6/P2-13.
14. ✅ Negative segment start отклоняется → P0-6/P2-13.
15. ✅ Overlapping segments отклоняются → P0-6/P2-13.
16. ✅ `fps` ограничен 1..240 → P0-8/P2-13.
17. ✅ `scale` строго валидируется → P0-8/P2-13.
18. ✅ Upload минует concurrency gate → закрыто в раунде 7 отдельным upload pool.
19. Нет resource limits ffmpeg/yt-dlp → security/perf track.
20. Нет no-progress watchdog → P2-9.
21. Нет filesize/duration import cap → security/perf track.
22. ✅ Partial upload после multipart error → закрыто в раунде 7 единым cleanup boundary.
23. Progress tick берёт global jobs mutex → P2-9/P2-12.
24. ◐ TTL удаляет referenced/active files → P0 backlog/P2-10. *(file/cache/library-консистентность есть; active-job-awareness - нет, audit.md №24)*
25. ✅ Render cache не инвалидируется → P0-5/P2-10.
26. Нет DB migrations → P2-10.
27. ◐ Нет jobs/cache retention; recovery ограничен последними 200 → P0-2/P2-10.
28. `library.json` и SQLite дрейфуют → P2-10.
29. ✅ Нет single-flight render cache → P2-10. *(исправлено `a550a86`; побочный эффект - audit.md №205)*
30. `cache_put` и `library.add` не атомарны → P2-10.
31. Persist errors глотаются → P2-12.
32. ✅ Нет DB indexes для retention/sort → P0-2/P2-10.
33. ✅ DNS/redirect SSRF bypass → P0-3/P0-10. *(initial guard + per-request proxy; закрыто в раунде 6)*
34. ☐ Нет auth → before-exposure security track. *(⚠выставление, не тронуто)*
35. ✅ `ServeDir` отдаёт весь `storage` → before-exposure security track. *(исправлено `986b799`)*
36. ✅ Permissive CORS → before-exposure security track. *(исправлено `986b799`)*
37. Неполный special-use IP blocklist → P0-3.
38. Upload без magic-byte validation → security track.
39. Projects JSON без схемы/лимита → P2-10/P2-11.
40. ✅ Wire DTO strict/versioned; persisted project documents tolerant → wave 1 №829.
41. Сырой stderr наружу → P1-8/P2-11.
42. ✅ Единый `AppError` JSON envelope → P2-11.
43. ✅ DB errors получают безопасный body и redacted internal log → P2-11.
44. Async jobs отвечают `200`, не `202` → P2-11.
45. ✅ Frontend различает network outage и typed HTTP `ApiError` → P1-5/frontend API cleanup.
46. God store + god `EditPanel.vue` → P1-5/P1-6.
47. Watchers как import side effects → P1-5.
48. Runtime validation отсутствует для JSON/presets → P1-5/P1-7. *(переформулировано: реальная сегодняшняя проблема - слишком широкий `PRESET_KEYS`, audit.md №209)*
49. Silent catch/a11y/unsafe non-null → P1-6.
50. Test gaps по render orchestration/API/store → P0 + P2 acceptance tests.
51. `BIND_ADDR` silent fallback → P1-4 Config validation.
52. Numeric env fail-soft → P1-4 Config validation.
53. Env reads scattered → P1-4 single `Config`.
54. Нет testable config struct → P1-4 config tests.
55. Tool versions unpinned → Docker/CI reproducibility task.
56. Container root/no healthcheck/limits → Docker hardening task.
57. CI lacks `yt-dlp` import coverage → CI import smoke.
58. Node/Rust/toolchain drift → README/Docker/CI version alignment.
59. No MSRV/toolchain pin → add `rust-toolchain.toml` or documented MSRV.
60. No tracing docs → README ops/debug section.
61. No job lifecycle logs → observability task after JobService.
62. No metrics endpoint → later ops track.
63. Health mixes readiness/liveness → split `/healthz`/`/readyz`.
64. Health ignores storage writability → add storage/db probe.
65. CORS not config-driven → Config + deployment profile.
66. Served/internal storage mixed → split public files from internal state.
67. Cleanup no dry-run/report → cleanup worker refactor.
68. Compose no resource limits → docker-compose hardening.
69. No backup/restore docs → ops runbook.
70. No migration rollback → DB migration framework.
71. No OpenAPI/schema → contract generation task.
72. Manual TS DTOs → generated types/client.
73. `ProjectDto` in API client → move to generated/shared types.
74. Backend response DTOs missing → typed DTO structs.
75. Manual `json!` responses → HTTP DTO layer.
76. ◐ Error status/body drift закрыт AppError; async jobs 200→202 остаётся.
77. Cancel response ignored → frontend API cleanup.
78. ✅ 5xx/network mixed → закрыто typed `ApiError` в раунде 8.
79. No AbortController/timeouts → cancellable API client.
80. Polling no backoff/jitter → jobs UI/client refactor.
81. Polling no deadline → jobs client deadline.
82. No idempotency keys → future API hardening.
83. Upload sync-flow differs → upload-as-job or documented exception.
84. No jobs list endpoint → jobs history endpoint/UI.
85. No request correlation id → request-id middleware.
86. ✅ Error body shape unified → `{error, code}` через AppError.
87. ✅ Plain-text API errors устранены в handlers/extractors/fallbacks.
88. No pagination/search → library/projects API pagination.
89. No API versioning → `/api/v1` or schema version policy.
90. README API drift → docs/API contract check.
91. ✅ `EditRequest` ограничен DTO boundary; есть явный mapper в typed `EditPlan`.
92. Effects as scalars/bools → typed `Effect` enum.
93. ✅ Raw format/codec/look/geometry преобразуются в enums/smart types.
94. ✅ Отдельный validated `OutputSpec` реализован.
95. ✅ `TimeRange` и non-overlap validation реализованы.
96. ✅ `PixelRect` и source-aware geometry normalization реализованы.
97. ✅ `OutputScale` с `-1/-2` invariants реализован.
98. ✅ Bounds централизованы в plan/domain compile boundary.
99. Numeric edge policy absent → validation corpus.
100. ✅ Source duration принадлежит `EditPlan`; args/duration компилируются вместе.
101. Cache key from request JSON → normalized plan hash.
102. Cache ignores tool/pipeline version → cache key versioning.
103. Source provenance not typed → media domain model.
104. Raw IDs → newtypes.
105. Project stores video blob → media reference table/model.
106. Edit JSON no schema migration → versioned project schema.
107. Defaults unversioned → versioned defaults/migrations.
108. Autosave no rollback → project history/undo later.
109. No project audit trail → optional history table.
110. No domain error enum → AppError/domain errors.
111. Capabilities no contract → capabilities endpoint/schema.
112. Preview/backend mismatch → shared preview capability model.
113. Media kind not typed everywhere → media enum.
114. No edit recipe corpus → golden fixtures.
115. New edit field touches many files → generated/default diff work.
116. Platform presets in UI → export preset policy module.
117. Quality tier table duplicated → shared/generated policy.
118. No compatibility policy → project schema docs/tests.
119. Timeline IR only docs → P2-13 implementation.
120. Multi-track blocked → Timeline IR + range effects.
121. `tools/args.rs` too large → split compile/filter/output modules.
122. `handlers/mod.rs` owns jobs → JobRunner/JobService.
123. `db.rs` mixed concerns → repositories + migrations.
124. `library.rs` parallel JSON repo → move media to SQLite.
125. Router uses concretes → ports/services injected in state.
126. Missing repo traits → P2-10.
127. Missing in-memory repos → service tests.
128. ✅ `AppError` HTTP boundary введён в раунде 8; typed `job.error` остаётся.
129. Missing messages/i18n boundary → P1-8.
130. Process runner weak boundary → `ffmpeg/process.rs` + `download`.
131. No `JobService::spawn` → P2-9.
132. No job state machine → P2-12.
133. Cancel tokens outside jobs subsystem → jobs/cancellation.
134. Persist terminal invariant missing → JobService transitions.
135. No cleanup tests → TTL test suite.
136. No upload concurrency tests → API/concurrency test.
137. No property geometry tests → proptest/fixture corpus.
138. No API snapshots → contract/snapshot tests.
139. No DTO fuzz/negative corpus → serde validation tests.
140. No perf baseline → benches after split.
141. No component tests → Vitest component test setup.
142. No E2E smoke → Playwright smoke.
143. No route/lazy boundary → frontend app structure refactor.
144. No UI error boundary → error boundary component/composable.
145. Toasts no actions → toast action model.
146. Toasts no debug context → error context logging.
147. Shortcuts in `App.vue` → `useShortcuts`.
148. Shortcut a11y/IME gaps → keyboard handling audit.
149. No focus management → UX/accessibility task.
150. Upload preflight missing → client file validation.
151. URL warning not tied to backend policy → shared URL risk helper.
152. `pollJob` not abortable → abortable polling.
153. Poll timeout handle not cleaned → composable cleanup.
154. Import/export duplicate status → job view model.
155. No jobs history UI → jobs panel.
156. Library no search/pagination → media library UX.
157. Destructive actions no undo/confirm → delete flow redesign.
158. Result/library action duplication → media action model.
159. Preset quota UX missing → preset storage validation.
160. Theme no system mode → `system|dark|light` theme state.
161. No reduced-motion/contrast pass → accessibility pass.
162. No UI primitives → extract shared controls.
163. Options in component → `lib/editOptions`.
164. Export hints in component → export availability composable.
165. Autosave side effects not isolated → projects composable.
166. API responses not runtime-validated → lightweight schema guards.
167. Debug hook undocumented → dev docs/test helper note.
168. No design tokens docs → style/design token cleanup.
169. No visual regression → screenshot smoke later.
170. No localization boundary → frontend i18n/messages.
171. ◐ Upload threat model есть; общая deployment threat model ещё нужна.
172. No auth → before-exposure auth middleware.
173. No CSRF posture → auth/session design.
174. No rate limiting → middleware/queue limits.
175. No per-user isolation → multi-user architecture later.
176. No process sandbox → container/seccomp/resource limits.
177. No URL domain policy → configurable allow/deny list.
178. ✅ No redirect policy → downloader adapter constraints. *(закрыто в раунде 6 egress-proxy)*
179. ◐ Logs используют общий URL/query/path redaction; API/diagnostics policy ещё не едина.
180. ◐ Query tokens скрываются в логах; UI warning перед импортом ещё нужен.
181. No redacted diagnostics bundle → diagnostics endpoint.
182. ✅ Upload magic bytes missing → media probe before publish. *(закрыто в раунде 5)*
183. ◐ Private staging/probe/publish quarantine есть; malware scanner/sandbox policy ещё нет.
184. No safe content-disposition → file serving wrapper.
185. No audit log → mutating operations log.
186. No quota reporting → storage usage endpoint/tool.
187. No checksums → media metadata.
188. No orphan scanner → repair CLI/script.
189. No backup command → export/import storage task.
190. No DB maintenance → vacuum/checkpoint runbook.
191. No file+DB transaction boundary → unit of work pattern.
192. No media FK table → SQLite media schema.
193. Cleanup no ref counts → reference-aware cleanup.
194. Disk-full generic → storage error mapping.
195. Startup no free-space/permission check → readiness probe.
196. No filesystem lock → single-process storage guard.
197. No project retention → retention config.
198. No source/output retention split → retention policy.
199. No repair runbook → ops documentation.
200. No docs/code drift check → docs check in CI.

---

## SOLID/DRY модули по кусочкам (2 июля 2026)

565 находок раунда 3 (полные формулировки, file и обоснование - в
[architecture.md](architecture.md#модульная-карта-soliddry-декомпозиция-2-июля-2026)),
здесь - чеклист для работы: бери один модуль, иди сверху вниз (🔴 high первыми),
отмечай сделанное. Модули почти не пересекаются по файлам - можно закрывать
по одному, не боясь конфликтов с остальными.

**Как брать по маленьким кускам.**

1. Сначала бери один модуль и один severity-слой: например, только 🔴 из
   `Frontend компоненты`, не весь frontend.
2. Для рефакторинга сначала делай safe-extract без изменения поведения, потом
   отдельным коммитом меняй поведение.
3. Для UI/UX сначала исправляй состояния/иерархию/доступность, потом эстетику:
   так дизайнерские правки меньше ломают сценарии.
4. После каждого куска запускай `make check`; для frontend-визуала дополнительно
   открывай dev-сборку и проверяй desktop/mobile.

**Исходный первый набор кусочков:** P0-9 upload XSS, P0-10 SSRF redirect,
P0-11 cancel→Running race, `upload_handler` через semaphore, `AppError`,
`Config`, `JobRunner`, `EditPanel` split, `store.ts` split, RectOverlay/TrimSlider
cleanup, design empty states, shared format/time utils. Раунд 5 закрыл P0-9,
P0-11, cleanup и первые безопасные frontend-срезы; раунд 6 закрыл P0-10,
раунд 7 - upload gate, раунд 8 - `AppError`.
Актуальный остаток - ниже.

**Сверка 18 июля 2026.** Этот файл синхронизирован с `architecture.md`: здесь
765 номерных чекбоксов - исходные 565 SOLID/DRY-пунктов и два research-backed
слоя №784-883 и №884-983; в архитектуре - те же номера с пояснениями.
`docs/audit-500.md` остаётся широким источником на 509 проблем/улучшений. Новый
PR лучше делать не по всему списку, а по одному маленькому вертикальному срезу:
один модуль, один severity-слой, один критерий приёмки. Источники второго слоя
находятся в `docs/research-next-100.md`.

**Раунд 5, 11 июля 2026.** После трёх итераций закрыты P0-9 (upload stored
XSS), P0-11 (cancel→queued/running race), 209 (look-пресет менял экспорт),
517 (чистый payload compiler), 553/554 (listener cleanup), а также no-op export
warning и mobile overflow. `store.ts`/`EditPanel.vue` уменьшены безопасными
срезами, но их полная декомпозиция 500/550 остаётся открытой. Проверка: backend
тесты, frontend lint/typecheck/Vitest/build и живой desktop/mobile-прогон.

**Раунд 6, 11 июля 2026.** P0-10 и связанные 291/410/411/414/415/663 закрыты
per-job egress-proxy: единая host/IP/port policy применяется к каждому HTTP/
CONNECT target, DNS-ответ пинится в фактический `SocketAddr`, `NO_PROXY` и
пользовательский config `yt-dlp` не могут обойти transport. Adversarial review
добавил deny-status 472 + отдельный atomic policy marker (настоящий upstream
403/472 не даёт ложную SSRF-ошибку), общий bounded DNS/connect budget, лимит 64
proxy-connections и bounded shutdown. Format selector разрешает только HTTP(S)
media URL и не даёт выбрать direct RTMP/FTP/WebSocket downloader. Реальные
`yt-dlp` regressions проходят public host → 302 → private sink и synthetic
HTTPS+RTMP/RTMP-only metadata: private TCP connect не создаётся, non-HTTP-only
media отклоняется.

**Раунд 7, 11 июля 2026.** Закрыты 18/22/223/225/278/288/444: job semaphore
инкапсулирован, upload использует независимый fail-fast pool и отвечает `429`
при насыщении. Multipart receive имеет 30-минутный timeout с обязательным
cleanup `.upload`; `ffprobe` bounded 30 секундами, startup tool checks - 5
секундами, timeout делает kill+wait с `kill_on_drop` fallback. Regression-тесты
проверяют pools, saturation/recovery, timeout latency и staging cleanup. Frontend
уже показывает серверный текст `429` inline и toast.

**Раунд 8, 11 июля 2026.** Закрыты 455/457/460/469/472/488: новый `error.rs`
возвращает единый `{error, code}` для handlers, JSON/multipart extractors и
`404/405`; internal source логируется и не утекает клиенту. Теперь
`http/mod.rs` разделяет project parsing и `ProjectPort` persistence. Frontend использует один
parser и typed `ApiError(status, code)`, поэтому реальный HTTP 500 больше не
маскируется под network failure. `job.error`, 202 и cancel outcome остаются
отдельными задачами.

**Следующие маленькие PR по приоритету.**

1. Собрать `Config` один раз на старте и убрать scattered env reads.
2. Вынести `JobRunner`: create, acquire, progress, finish, cancel, panic handling.
3. Продолжить `EditPanel.vue`: `AudioControls`, `PresetBar`, затем timing/frame.
4. Продолжить store: history/presets/theme, сохраняя совместимый фасад.
5. Вынести общий `useDragHandle`, теперь поверх уже безопасного unmount cleanup.
6. ◐ Redaction helper для логов готов; добавить URL query-token warning в UI.
7. ✅ Добавлен first-class offline state без backend.
8. Вынести shared formatDuration/formatSize и каталоги edit options.
9. Добавить typed API/OpenAPI слой между Rust и TS.
10. ✅ Добавлен cross-browser smoke: frontend открывается без backend и показывает понятное состояние.
11. Собрать diagnostic bundle с redaction, чтобы приватные URL/token query не попадали в архив.
12. Закрепить Rust toolchain/MSRV и воспроизводимые Docker image digests.

### HTTP-хендлеры и роутинг (51)

- [ ] 🔴 **219.** (проблема) import_handler смешивает валидацию URL, оркестрацию очереди, скачивание, probing и persistence в одной async-функции -> Вынести оркестрацию job (create/permit/progress/finish) в общий раннер, а скачивание+probing в отдельную доменную функцию, которую import_handler только вызывает. `backend/src/handlers/mod.rs`
- [ ] 🔴 **220.** (проблема) import_handler и edit_handler дублируют весь жизненный цикл job целиком -> Выделить общую функцию run_job(state, job_id, token, work: impl Future) -> Json<Value>, инкапсулирующую permit/progress/finish, и передавать в неё только специфичную работу. `backend/src/handlers/mod.rs`
- [ ] 🟠 **221.** (проблема) edit_handler дополнительно совмещает cache-логику рендера с оркестрацией job -> Вынести проверку и запись в render cache в отдельный RenderCache-хелпер с методами try_serve/store, вызываемый из edit_handler декларативно. `backend/src/handlers/mod.rs`
- [x] 🟠 **223.** (проблема) upload_handler не проходил через concurrency gate -> Закрыто в раунде 7: отдельный upload pool ограничивает multipart/probe без starvation job queue; saturation возвращает `429`. `backend/src/handlers/upload.rs`, `backend/src/state.rs`
- [x] 🟠 **225.** (баг) upload_handler не ограничивал время probe_video -> Закрыто в раунде 7: общий `probe_video` bounded 30 секундами с kill+wait; upload удаляет staging и возвращает отдельный `504`. `backend/src/tools/mod.rs`, `backend/src/handlers/upload.rs`
- [x] ✅ **226.** Media normalization удалена из HTTP: source-aware compile живёт в `services/render.rs`, invariants — в `domain/edit.rs`/`domain/output.rs`.
- [ ] 🟠 **228.** (проблема) import_handler и edit_handler игнорируют JoinHandle из tokio::spawn — паника воркера не отражается в job -> Сохранять JoinHandle и в отдельной задаче через .await с обработкой Err(JoinError) переводить job в JobStatus::Error с сообщением о панике. `backend/src/handlers/mod.rs`
- [ ] 🟠 **239.** (баг) library_delete_handler читает entry и удаляет его двумя раздельными операциями без атомарности -> Изменить Library::remove, чтобы он возвращал Option<MediaEntry> удалённой записи одним атомарным вызовом, и убрать отдельный get. `backend/src/handlers/library.rs`
- [ ] 🟠 **244.** (проблема) build_router не имеет fallback-хендлера для несуществующих маршрутов -> Добавить .fallback(|| async { (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))) }) для единообразного JSON-ответа на неизвестные маршруты. `backend/src/lib.rs`
- [ ] 🟠 **245.** (проблема) build_router не задаёт лимит тела запроса для JSON-эндпоинтов кроме /api/upload -> Явно задать .layer(DefaultBodyLimit::max(N)) на уровне общего Router для всех /api/* JSON-маршрутов, чтобы лимит был виден и настраиваем в одном месте. `backend/src/lib.rs`
- [ ] 🟠 **249.** (проблема) handlers/mod.rs объединяет публичные HTTP-хендлеры и приватные доменные хелперы в одном файле на 964 строки -> Разбить mod.rs на handlers/jobs.rs (import/edit/upload/status/cancel), handlers/render_cache.rs и validation/edit.rs по аналогии с уже выделенными health.rs/library.rs/projects.rs. `backend/src/handlers/mod.rs`
- [ ] 🟠 **250.** (проблема) import_handler передаёт req.start/req.end без нормализации, в отличие от edit_handler -> Добавить в import_handler проверку start < end и start/end >= 0 перед вызовом download_video, отклоняя запрос с понятной ошибкой вместо передачи в yt-dlp как есть. `backend/src/handlers/mod.rs`
- [ ] 🟠 **254.** (проблема) edit_handler не проверяет существование video_id в library до создания job и занятия permit -> Проверить наличие source-файла синхронно в HTTP-хендлере до создания job и возвращать 404, если video_id неизвестен. `backend/src/handlers/mod.rs`
- [ ] 🟠 **255.** (баг) Ключ кэша рендера считается до нормализации EditRequest, из-за чего эквивалентные запросы не совпадают -> Переместить нормализацию перед вычислением render_cache_key, синхронно выполнив probe+normalize в HTTP-хендлере до spawn, либо считать ключ по уже нормализованному EditRequest внутри воркера. `backend/src/handlers/mod.rs`
- [ ] 🟠 **256.** (баг) Попадание в render cache не регистрирует результат в медиатеке -> Вызвать state.library.add(MediaEntry::from_result("output", &output)) внутри finish_from_render_cache при успешном попадании в кэш. `backend/src/handlers/mod.rs`
- [ ] 🟡 **222.** (идея) upload_handler вручную повторяет сборку VideoInfo-JSON, уже собираемую в import_handler -> Ввести конструктор VideoInfo::new(id,url,filename,probe,title,size) -> Value и использовать его в обоих хендлерах. `backend/src/handlers/mod.rs`
- [ ] 🟡 **251.** (идея) Нет эндпоинта для получения списка активных/очереди jobs -> Добавить GET /api/jobs, возвращающий список нетерминальных jobs из AppState, чтобы фронтенд мог восстановить состояние после перезагрузки. `backend/src/handlers/mod.rs`
- [ ] 🟡 **224.** (дизайн) upload_handler возвращает результат по первому подходящему полю формы и молча игнорирует остальные части multipart -> Явно проверять, что multipart содержит ровно одно валидное поле файла, и возвращать 400 при лишних/нераспознанных полях. `backend/src/handlers/mod.rs`
- [ ] 🟡 **227.** (улучшение) Файловые и криптографические утилиты живут в handlers/mod.rs вперемешку с HTTP-хендлерами -> Вынести их в backend/src/tools/fs_utils.rs или backend/src/tools/cache.rs, оставив в handlers только вызовы. `backend/src/handlers/mod.rs`
- [ ] 🟡 **230.** (дизайн) Прогресс сохраняется только при шаге >=1%, без гарантии сохранения финального значения перед 100 -> Если это осознанный компромисс — оставить как есть; иначе сохранять хотя бы последнее полученное значение перед закрытием канала независимо от порога. `backend/src/handlers/mod.rs`
- [ ] 🟡 **235.** (улучшение) project_upsert_handler не проверяет, что video["id"] совпадает с videoId тела запроса -> После разбора video проверить video["id"].as_str() == Some(video_id) и вернуть 400 при расхождении. `backend/src/http/mod.rs`
- [ ] 🟡 **236.** (дизайн) Ошибки в projects.rs смешивают русский текст с сырыми деталями SQLx в INTERNAL_SERVER_ERROR -> Логировать e через tracing::error! на сервере и возвращать клиенту фиксированное русскоязычное сообщение 'внутренняя ошибка' без деталей SQLx. `backend/src/http/mod.rs`
- [ ] 🟡 **242.** (улучшение) health_handler жёстко завязан на ровно два внешних инструмента -> Хранить инструменты как Vec<(name, ToolStatus)> в ToolInfo и сериализовать их в цикле, чтобы health_handler не менялся при добавлении нового инструмента. `backend/src/http/mod.rs`
- [ ] 🟡 **246.** (дизайн) CORS allow_methods не включает PUT/PATCH/OPTIONS, ограничивая будущие эндпоинты -> Добавить Method::PUT и Method::PATCH заранее либо держать список методов в одной константе, синхронизированной с build_router. `backend/src/lib.rs`
- [ ] 🟡 **248.** (улучшение) std::env::var для CORS и JOB_TIMEOUT читается при каждом старте роутера/job, а не один раз в main -> Централизовать чтение env-конфигурации (CORS origins, JOB_TIMEOUT_SECS, RECOVER_JOBS_LIMIT) в одном Config, собираемом в main и передаваемом через AppState. `backend/src/lib.rs`
- [ ] 🟡 **253.** (улучшение) finish_job принимает kind: &str как непроверяемый компилятором строковый тег -> Заменить kind: &str на enum MediaKind { Source, Output } с реализацией as_str(), используемый и в finish_job, и в MediaEntry::from_result. `backend/src/handlers/mod.rs`
- [ ] 🟡 **262.** (улучшение) acquire_render_lock_or_cancelled и mark_cancelled создают риск двойной записи отмены при отмене на этапе ожидания render_lock -> Консолидировать перевод job в Cancelled в единственном методе AppState (например state.mark_job_cancelled), вызываемом и из cancel_handler, и из mod.rs-хелперов вместо двух параллельных реализаций. `backend/src/handlers/mod.rs`
- [ ] 🟡 **229.** (проблема) job_timeout() читает переменную окружения JOB_TIMEOUT_SECS при каждом вызове вместо однократного чтения -> Прочитать JOB_TIMEOUT_SECS один раз в main/build_router через std::sync::OnceLock и переиспользовать закэшированное значение. `backend/src/handlers/mod.rs`
- [x] ✅ **231.** Handler передаёт wire DTO в `EditPlan::compile` и дальше использует immutable plan через export port. `backend/src/services/render.rs`, `backend/src/ports/media_export.rs`
- [ ] 🟡 **232.** (баг) cleanup_files_with_prefix сопоставляет файлы по префиксу имени, что может задеть чужой файл при совпадении подстроки -> Сравнивать Path::file_stem() файла целиком с prefix вместо strip_prefix, чтобы исключить любую теоретическую двусмысленность. `backend/src/handlers/mod.rs`
- [ ] 🟡 **233.** (проблема) project_upsert_handler вручную парсит сырой serde_json::Value вместо типизированного тела запроса -> Ввести #[derive(Deserialize)] struct ProjectUpsertRequest { video_id: String, video: Value, edit: Value, name: Option<String> } и десериализовать через Json<ProjectUpsertRequest>. `backend/src/http/mod.rs`
- [ ] 🟡 **234.** (проблема) ensure_project_json_size сериализует video/edit отдельно от последующей записи в БД, дублируя сериализацию -> Сериализовать video/edit один раз в project_upsert_handler, проверить длину байт и передать уже сериализованные строки в upsert_project, чтобы БД не сериализовала повторно. `backend/src/http/mod.rs`
- [ ] 🟡 **237.** (проблема) project_get_handler и project_by_video_handler возвращают голый StatusCode без текста ошибки, в отличие от upsert/list -> Привести все четыре project-хендлера к единому Result<_, (StatusCode, String)> с логированием причины на сервере. `backend/src/http/mod.rs`
- [ ] 🟡 **238.** (проблема) Паттерн 'Ok(Some)/Ok(None)/Err -> StatusCode' повторяется идентично в нескольких хендлерах -> Добавить extension-trait для Result<Option<T>, sqlx::Error> с методом .into_response_or_404(), переиспользуемый во всех подобных хендлерах. `backend/src/http/mod.rs`
- [ ] 🟡 **240.** (проблема) library_delete_handler чистит db cache только для kind == 'output', не проверяя cache-записи, ссылающиеся на source-файлы -> При удалении source дополнительно проверять и очищать cache-записи, у которых закешированный результат ссылается на этот video_id, либо явно задокументировать, что это не требуется, так как cache хранит только output. `backend/src/handlers/library.rs`
- [ ] 🟡 **241.** (проблема) health_handler не проверяет живость хранилища/БД, только статически захваченные при старте флаги ffmpeg/ytdlp -> Добавить лёгкую проверку доступности storage-директорий и SQLite-пула (например, SELECT 1) прямо в health_handler с коротким таймаутом. `backend/src/http/mod.rs`
- [ ] 🟡 **243.** (проблема) health_handler не различает, какой из статусов 'ok'/'degraded' к чему относится при отсутствии обоих инструментов -> Добавить массив missingTools в ответ, перечисляющий конкретные недоступные инструменты, чтобы не заставлять клиента сверять два булевых поля. `backend/src/http/mod.rs`
- [ ] 🟡 **247.** (проблема) parse_cors_origin разрешает схему http для не-localhost хостов наравне с https -> Логировать warn (не только на invalid) при приёме http-origin с хостом, отличным от localhost/127.0.0.1, чтобы явно предупредить о небезопасной конфигурации. `backend/src/lib.rs`
- [ ] 🟡 **252.** (проблема) acquire_render_lock_or_cancelled и acquire_job_permit_or_cancelled дублируют одинаковую tokio::select!-обёртку 'ресурс или cancel' -> Обобщить через дженерик acquire_or_cancelled<F: Future<Output=T>>(fut: F, st, jid, token) -> Option<T>, вызываемый с разными future из render_lock.lock_owned() и semaphore.acquire_owned(). `backend/src/handlers/mod.rs`
- [ ] 🟡 **257.** (проблема) import_handler не проверяет заранее пустой url и порядок start/end до запуска job -> Выполнить лёгкую синхронную проверку url (не пустой, парсится как URL) в самом import_handler до set_job/register_cancel и вернуть 400 без создания job. `backend/src/handlers/mod.rs`
- [ ] 🟡 **258.** (проблема) upload_handler и import_handler по-разному вычисляют title файла -> Свести оба пути к общей функции resolve_title(source: TitleSource) с явными вариантами Uploaded{original_name} и Downloaded{sources,id}. `backend/src/handlers/mod.rs`
- [ ] 🟡 **259.** (баг) upload_handler не ограничивает длину title, беря его напрямую из оригинального имени файла -> Обрезать title до разумной длины (например 200 символов) и/или санитизировать управляющие символы перед сохранением в library. `backend/src/handlers/mod.rs`
- [ ] 🟡 **260.** (проблема) project_upsert_handler инлайн реализует fallback-цепочку для имени проекта вместо доменной функции -> Вынести резолюцию имени в отдельную чистую функцию resolve_project_name(body, video) -> String, тестируемую независимо от HTTP-слоя. `backend/src/http/mod.rs`
- [ ] 🟡 **261.** (проблема) project_upsert_handler не ограничивает длину поля name -> Добавить проверку name.len() <= разумного предела (например 200 байт) и обрезать/отклонить при превышении, аналогично ensure_project_json_size для video/edit. `backend/src/http/mod.rs`
- [ ] 🟡 **263.** (проблема) job_status_handler не различает 404 для несуществующего job и job, уже вычищенного лимитом recover_jobs -> Добавить отдельный код/сообщение (например {"error":"job_expired"}) для случая, когда id валиден по формату UUID, но отсутствует и в памяти, и в БД. `backend/src/handlers/mod.rs`
- [x] 🟡 **264.** (баг) sanitize_ext не проверял конфликт очищенного расширения с зарезервированными именами -> Закрыто в раунде 5: `sanitize_ext` удалён, server suffix выбирает `safe_upload_extension`, staging использует `.upload`. `backend/src/handlers/upload.rs`
- [ ] 🟡 **265.** (проблема) build_router перечисляет пары REST-методов на одном пути отдельными .route() вызовами без общего паттерна регистрации ресурса -> При росте числа ресурсов ввести небольшой builder resource(path, handlers) для типового набора GET/POST/DELETE, снижающий повторение синтаксиса регистрации. `backend/src/lib.rs`
- [ ] 🟡 **266.** (проблема) spawn_progress_drain не имеет верхней границы по времени и переживает job, если воркер зависает до закрытия tx -> Обернуть rx.recv() в tokio::select! с сигналом отмены/таймаутом job_timeout(), чтобы drain гарантированно завершался вместе с воркером. `backend/src/handlers/mod.rs`
- [ ] 🟡 **267.** (проблема) upload_handler комбинирует чтение multipart-полей, запись на диск и пробинг в одном цикле без ранней валидации типа поля -> Сначала пройтись по всем полям и выбрать/провалидировать единственное поле файла, затем отдельным шагом выполнить запись+probe, разделяя разбор входа и работу с диском. `backend/src/handlers/mod.rs`
- [ ] 🟡 **268.** (проблема) Хендлеры projects.rs возвращают внутреннюю структуру Project из db.rs напрямую как HTTP DTO -> Ввести отдельный ProjectResponse в handlers/projects.rs с явным From<Project>, чтобы изменения схемы БД не автоматически меняли HTTP-ответ. `backend/src/http/mod.rs`
- [ ] 🟡 **269.** (баг) cors_origins_from_env использует запятую как единственный разделитель без экранирования -> Задокументировать в комментарии, что CORS_ALLOW_ORIGINS ожидает только plain origin без query/fragment, разделённых запятой, без изменения кода при текущих ограничениях parse_cors_origin. `backend/src/lib.rs`

### Jobs, конкурентность, process lifecycle (46)

- [ ] 🔴 **271.** (баг) Bare update_job после получения permit/render-lock может воскресить job, отменённый в узком окне между проверкой token.is_cancelled() и вызовом. -> Заменить оба вызова на update_job_if_open и прекращать работу воркера, если обновление не применилось (job уже terminal). `backend/src/handlers/mod.rs`
- [ ] 🟠 **270.** (проблема) render_locks не имеет верхней границы и никогда не очищается — растёт на каждый уникальный edit-запрос. -> Добавить эвакуацию записи после завершения рендера (например, удалять запись, если strong_count Arc опустился до 1 под локом карты) или заменить на LRU/TTL-кэш с ограничением размера. `backend/src/state.rs`
- [ ] 🟠 **273.** (баг) set_job пишет в БД до вставки job в память — окно, где GET /api/jobs/:id отдаёт 404, хотя job уже есть в БД. -> Сначала вставить job в jobs-map под локом, затем персистить в БД (или делать это параллельно), чтобы память была источником истины раньше или одновременно с БД. `backend/src/state.rs`
- [ ] 🟠 **274.** (баг) Окно между снятием job.status=Cancelled и удалением cancel-токена в cancel_open_job, где воркер видит job уже Cancelled, но токен ещё не сигнален. -> Объединить обновление статуса job и token.cancel() в одну критическую секцию (например, держать оба лока последовательно без промежуточного await между ними) либо cancel'ить токен первым. `backend/src/state.rs`
- [ ] 🟠 **281.** (проблема) jobs HashMap в AppState не имеет TTL или эвакуации — завершённые задачи (Done/Error/Cancelled/Interrupted) копятся в памяти навсегда до перезапуска процесса. -> Добавить фоновую задачу или обёртку в persist_job/update_job_if_open, которая удаляет из jobs записи старше N часов после достижения terminal-статуса, оставляя историю только в SQLite. `backend/src/state.rs`
- [ ] 🟠 **289.** (проблема) find_by_id делает линейный скан всей директории sources на каждый find_source/find_by_id вызов вместо прямого поиска по известным расширениям. -> Хранить связку video_id -> filename в БД (или Library) при создании файла и искать по точному пути вместо сканирования директории при каждом обращении. `backend/src/tools/mod.rs`
- [ ] 🟠 **292.** (баг) acquire_render_lock_or_cancelled удерживает render_lock guard, пока не будет получен jobs_semaphore permit — конкурентные edit-запросы с одинаковым cache_key упираются в permit-голод, удерживая лок долго. -> Получать permit до захвата render_lock (сначала дождаться слота в очереди, затем сериализовать по cache_key) либо разделить 'ожидание очереди' и 'сериализация одинаковых рендеров' на независимые этапы, не блокирующие друг друга. `backend/src/handlers/mod.rs`
- [ ] 🟠 **295.** (проблема) finish_job добавляет в library только если updated==true — если job был отменён ровно в момент завершения рендера, готовый файл остаётся на диске, но не попадает ни в library, ни в render-кэш. -> При outcome=Ok(Some(..)) но updated==false (job уже terminal не по этой ветке) всё равно выполнять cache_put для валидного файла или удалять его, чтобы не оставлять несогласованное состояние диск/БД. `backend/src/handlers/mod.rs`
- [ ] 🟠 **296.** (проблема) Аналогичная orphan-файл гонка в import_handler: если скачивание успешно завершилось, но job уже был отменён к моменту finish_job, скачанный source остаётся на диске без cleanup и без записи в library. -> В finish_job, если updated==false, но outcome содержал успешный результат, удалять только что созданные файлы (source/output) так же, как это уже делается в ветке Done::Cancelled. `backend/src/handlers/mod.rs`
- [ ] 🟠 **308.** (баг) persist_job считывает job из памяти уже после того, как отпустил лок в момент чтения — конкурентный update_job_if_open между чтением и записью в БД может привести к записи устаревшего снимка job. -> Это фундаментально неизбежно при чтение-затем-запись без сериализации, но можно уменьшить окно, добавив монотонный version-counter в Job и в БД делать UPDATE ... WHERE version <= ?, отбрасывая устаревшие записи вместо слепой перезаписи. `backend/src/state.rs`
- [ ] 🟡 **275.** (улучшение) recover_jobs держит единственный jobs-lock guard на весь цикл persist_job для каждого interrupted job, блокируя все остальные операции с jobs на время старта. -> Сначала собрать все job в Vec под локом, отпустить лок, затем персистить interrupted записи в БД без удержания jobs guard, и только на короткое время повторно взять лок для финальной вставки в map. `backend/src/state.rs`
- [ ] 🟡 **279.** (дизайн) update_job и update_job_if_open дублируют идентичную структуру блокировки, различаясь только одной проверкой is_terminal. -> Реализовать update_job через update_job_if_open с параметром force: bool или сделать update_job приватным алиасом, вызывающим общий internal-helper с флагом пропуска terminal-проверки. `backend/src/state.rs`
- [ ] 🟡 **290.** (дизайн) map_ytdlp_error жёстко зашивает распознавание типов ошибок по подстрокам в stderr — новый тип ошибки требует правки самой функции. -> Вынести таблицу (маркер, сообщение) в статический список пар и итерировать её, чтобы новые случаи добавлялись как данные, а не код. `backend/src/tools/mod.rs`
- [ ] 🟡 **293.** (улучшение) mark_cancelled и mark_queue_closed дублируют одинаковый паттерн update_job_if_open + persist_job + clear_cancel, отличаясь только устанавливаемыми полями job. -> Вынести общий helper fn finalize_job_if_open(st, jid, f: impl FnOnce(&mut Job)) -> bool, инкапсулирующий update_job_if_open+persist_job+clear_cancel, и вызывать его с разными замыканиями из обоих мест. `backend/src/handlers/mod.rs`
- [ ] 🟡 **294.** (дизайн) Цепочка вложенных tokio::select!-хелперов для edit_handler требует держать в голове 4 независимые точки отмены, что легко пропустить при будущих правках. -> Свести все точки отмены к единому паттерну (например, обернуть каждый шаг в общий cancellable_step helper, который всегда возвращает Result<T, Cancelled> через select!), убрав отдельную ручную проверку is_cancelled(). `backend/src/handlers/mod.rs`
- [ ] 🟡 **299.** (дизайн) Job.stage — Option<String> без enum; комментарий перечисляет 3 значения, но код использует минимум 4 разных строковых литерала в разных местах без единого источника истины. -> Заменить Option<String> на Option<JobStage> enum с Serialize в те же строковые токены (serde rename_all snake_case), чтобы все места установки stage проходили проверку компилятором. `backend/src/model.rs`
- [ ] 🟡 **302.** (улучшение) probe_video не использует -select_streams и берёт первый видео-поток по порядку в контейнере, а не гарантированно основной. -> Передавать ffprobe -select_streams v:0 или дополнительно фильтровать по disposition.attached_pic == 0, чтобы гарантированно выбрать основной видеопоток, а не обложку. `backend/src/tools/mod.rs`
- [ ] 🟡 **314.** (улучшение) check_tool для ffmpeg и yt-dlp вызывается последовательно при старте, а не параллельно, увеличивая время запуска сервера на сумму двух health-check таймаутов. -> Добавить в tools/mod.rs обёртку pub async fn check_all_tools() -> ToolInfo, которая внутри делает tokio::join!(check_tool("ffmpeg",...), check_tool("yt-dlp",...)), инкапсулируя параллельность в одном месте. `backend/src/tools/mod.rs`
- [ ] 🟡 **272.** (проблема) update_job без суффикса _if_open в state.rs — небезопасный по умолчанию API, который легко перепутать с update_job_if_open. -> Оставить только update_job_if_open как основной публичный метод, а update_job сделать приватным helper'ом или переименовать в update_job_unchecked, чтобы название явно предупреждало о риске. `backend/src/state.rs`
- [ ] 🟡 **276.** (проблема) AppState совмещает пять разных ответственностей: jobs-реестр, cancel-токены, render-lock-кэш, семафор конкурентности и доступ к Db/Library/storage. -> Выделить JobsRegistry (jobs+cancels+recover) и RenderLockRegistry в отдельные структуры-поля с собственными методами, оставив AppState тонкой агрегирующей оболочкой. `backend/src/state.rs`
- [ ] 🟡 **277.** (проблема) AppState хранит конкретные типы Db и Library вместо трейтов, что не позволяет подменить их в тестах без реальной SQLite/FS. -> Ввести трейты JobStore/MediaLibrary с реализациями поверх Db/Library и хранить в AppState Arc<dyn Trait>, чтобы юнит-тесты могли подставлять fake-реализации. `backend/src/state.rs`
- [x] 🟡 **278.** (проблема) jobs_semaphore был публичным полем AppState -> Закрыто в раунде 7: оба gates приватны, наружу выходят только узкие acquire/close методы. `backend/src/state.rs`
- [ ] 🟡 **280.** (проблема) Job::pending не хранит created_at/updated_at, поэтому recover_jobs и любая будущая очистка jobs не имеют временной метки для принятия решений. -> Добавить поле created_at: DateTime<Utc> (и опционально updated_at) в Job и заполнять его в Job::pending, используя далее для TTL-эвакуации и сортировки recover_jobs. `backend/src/model.rs`
- [ ] 🟡 **282.** (проблема) Парсинг числовых переменных окружения (RECOVER_JOBS_LIMIT, JOB_TIMEOUT_SECS, MAX_HEIGHT) дублирует один и тот же паттерн 'parse + filter + unwrap_or' в разных модулях без общего helper'а. -> Вынести общую функцию env_usize(name, default) / env_positive(name, default) в отдельный модуль config.rs и переиспользовать её во всех трёх местах. `backend/src/state.rs`
- [ ] 🟡 **283.** (проблема) 3-секундный SIGTERM grace period в terminate_child_tree захардкожен и не настраивается через конфиг. -> Вынести grace-period в переменную окружения (например GRACEFUL_KILL_SECS) с разумным дефолтом 3, аналогично MAX_HEIGHT/JOB_TIMEOUT_SECS. `backend/src/tools/mod.rs`
- [ ] 🟡 **284.** (проблема) MAX_HEIGHT читается из env на каждый вызов download_video вместо однократного парсинга при старте процесса. -> Считать MAX_HEIGHT один раз в main.rs при построении AppState/ToolInfo и передавать значение в download_video параметром вместо чтения env на каждый вызов. `backend/src/tools/mod.rs`
- [ ] 🟡 **285.** (проблема) run_ffmpeg и download_video дублируют одинаковый паттерн match status { Ok/Cancelled/TimedOut/Failed } с разными текстами ошибок. -> Вынести общий helper fn map_proc_status(status, stderr, timeout_msg, fail_fmt: impl Fn(&str) -> String) -> Result<Done>, параметризованный только форматтером ошибки. `backend/src/tools/mod.rs`
- [ ] 🟡 **286.** (баг) parse_ytdlp_progress не отличает overall percent от процента отдельного фрагмента при раздельной загрузке video+audio форматов. -> Учитывать, что yt-dlp пишет отдельный '[download] Destination:' перед каждым фрагментом, и сбрасывать/усреднять базовую точку прогресса при смене фрагмента, либо делить на количество ожидаемых форматов. `backend/src/tools/mod.rs`
- [ ] 🟡 **287.** (проблема) tail() пересчитывает весь Vec<&str> из накопленного err_buf при каждом вызове без ограничения на общий размер накопленного буфера. -> Ограничить err_buf кольцевым буфером фиксированного размера (например, хранить только последние ~64 КБ) при накоплении в run_with_progress, а не постфактум резать в tail(). `backend/src/tools/mod.rs`
- [x] 🟡 **288.** (баг) check_tool не имел таймаута -> Закрыто в раунде 7: общий bounded output runner даёт tool checks 5 секунд, timeout kill+wait и `kill_on_drop` fallback. `backend/src/tools/mod.rs`
- [x] 🟡 **291.** (баг) validate_url вызывался один раз перед стартом импорта, оставляя TOCTOU на редиректах. -> Закрыто в раунде 6: все запросы `yt-dlp` принудительно проходят per-target policy egress-proxy. `backend/src/tools/egress_proxy.rs`
- [ ] 🟡 **297.** (проблема) JobStatus::from_token не различает 'неизвестный статус в БД' от 'pending', что маскирует порчу данных при восстановлении. -> Вернуть Result<JobStatus, String> из from_token и залогировать warn в recover_jobs при получении ошибки вместо тихого приведения к Pending. `backend/src/model.rs`
- [x] ✅ **298.** `EditRequest` остаётся wire DTO; `EditPlan::compile` проверяет rotate/quality/censor/numeric ranges до adapter. `backend/src/model.rs`, `backend/src/services/render.rs`
- [ ] 🟡 **300.** (проблема) Job.result: Option<serde_json::Value> — неограниченная по размеру и структуре полезная нагрузка хранится и в памяти, и в БД без какой-либо схемы. -> Определить конкретный enum/struct JobResult { Import { .. }, Edit { .. } } с ограниченным набором полей вместо serde_json::Value, либо задокументировать и enforced-ограничить максимальный размер сериализованного результата. `backend/src/model.rs`
- [ ] 🟡 **301.** (баг) cancel_open_job зануляет job.progress перед persist, теряя последнее известное значение прогресса отменённой задачи в истории/БД. -> Не сбрасывать progress при отмене (оставить последнее известное значение) — обнулять только stage, так как прогресс сам по себе полезен для UI/истории даже у отменённой задачи. `backend/src/state.rs`
- [ ] 🟡 **303.** (проблема) map_ytdlp_error (tools/mod.rs) и validate_url (tools/net.rs) используют разные, несвязанные словари для распознавания ошибок/проблем одного и того же внешнего инструмента (yt-dlp / скачиваемый URL). -> Ввести единый enum ImportError (InvalidUrl, GeoBlocked, Private, NotFound, Timeout, Other(String)) и конвертировать в него как из validate_url, так и из map_ytdlp_error, чтобы вызывающий код (import_handler) обрабатывал один тип. `backend/src/tools/mod.rs`
- [ ] 🟡 **304.** (проблема) ProcStatus и Done — два похожих, но несовместимых enum для представления исхода одного и того же процесса, требующие ручного match-преобразования в каждом вызывающем месте. -> Убрать промежуточный ProcStatus и возвращать из run_with_progress сразу Result<Done> с текстом ошибки, вычисленным внутри run_with_progress через обобщённый форматтер (см. также DRY-находку про дублирование match). `backend/src/tools/mod.rs`
- [ ] 🟡 **305.** (баг) find_by_id не защищён от TOCTOU между поиском файла источника и последующим его открытием в probe_video/build_ffmpeg_args. -> Ловить эту ситуацию явно: если ffprobe/ffmpeg падает с ошибкой отсутствия файла, возвращать пользователю понятное 'источник был удалён во время обработки' вместо общей 'ffprobe failed'. `backend/src/tools/mod.rs`
- [ ] 🟡 **306.** (проблема) recover_jobs_limit() и MAX_HEIGHT/JOB_TIMEOUT_SECS парсинг по-разному ведут себя при value=0 — recover_jobs_limit явно фильтрует v>0, но MAX_HEIGHT/JOB_TIMEOUT_SECS не отклоняют 0 или отрицательные значения (для u32 отрицательные и так невозможны, но 0 проходит). -> Добавить единообразную проверку 'v > 0' (или явный отдельный минимум) для MAX_HEIGHT так же, как это уже сделано для RECOVER_JOBS_LIMIT. `backend/src/tools/mod.rs`
- [ ] 🟡 **307.** (баг) download_video не отклоняет end <= start при формировании --download-sections, передавая yt-dlp заведомо невалидный диапазон. -> Проверить end.is_some_and(|e| e <= s) до формирования секции и вернуть содержательную ошибку 'end must be greater than start' до запуска yt-dlp. `backend/src/tools/mod.rs`
- [ ] 🟡 **309.** (проблема) recover_jobs выполняет чтение из БД, мутацию статуса в памяти и повторную запись в БД в одном неразделённом цикле. -> Разбить на явные шаги: собрать список job, отфильтровать non-terminal, замьютировать их в Vec, батчем персистить в БД, и только затем одним проходом вставить всё в jobs map. `backend/src/state.rs`
- [ ] 🟡 **310.** (проблема) run_with_progress биасит cancel/timeout выше завершения дочернего процесса даже при успешном near-instant exit из-за biased select!. -> Либо документировать это как намеренное поведение ('отмена побеждает гонку с завершением'), либо перед выходом по cancel/timeout проверять child.try_wait() на предмет уже готового результата и предпочитать его. `backend/src/tools/mod.rs`
- [ ] 🟡 **311.** (баг) Cache key вычисляется до `EditPlan::compile`; эквивалентные после canonicalization запросы не разделяют render cache. -> Перенести lookup/single-flight на `plan_fingerprint` после probe/compile. `backend/src/handlers/mod.rs`
- [ ] 🟡 **312.** (проблема) clamp_rect_to_source не проверяет переполнение source_width - min_w при source_width=1, что может привести к панике/underflow на u32. -> Использовать saturating_sub вместо прямого вычитания при вычислении верхней границы координат в clamp_rect_to_source. `backend/src/handlers/mod.rs`
- [ ] 🟡 **313.** (баг) run_with_progress теряет неотправленные прогресс-обновления, если progress-канал (unbounded) уже закрыт получателем (например, drain-таск завершился раньше из-за паники), так как progress.send(...) молча игнорирует ошибку. -> Логировать через tracing::debug!, когда progress.send возвращает Err, чтобы такие потери были видны в логах, а не полностью незаметны. `backend/src/tools/mod.rs`
- [ ] 🟡 **315.** (проблема) tail() режет уже собранный список строк без учёта того, что многобайтовые UTF-8 символы, разорванные на границе строки при разбиении BufReader::lines(), могут дать некорректный вывод при join. -> Если stderr процесса может содержать не-UTF8 байты (пути с нестандартной кодировкой в именах файлов), заменить BufReader::lines() на построчное чтение с явной lossy-конвертацией (String::from_utf8_lossy) вместо AsyncBufReadExt::lines, чтобы не терять хвост лога при первой невалидной последовательности. `backend/src/tools/mod.rs`

### FFmpeg domain compiler (args.rs) (53)

- [x] ✅ **319.** `OutputSpec` валидирует CRF по codec-specific диапазону до FFmpeg adapter. `backend/src/domain/output.rs`
- [ ] 🔴 **353.** (баг) render_cache_key всё ещё считается по wire `EditRequest`, а не по normalized `EditPlan.plan_fingerprint`. -> Перестроить cache lookup/single-flight после source probe. `backend/src/handlers/mod.rs`
- [ ] 🟠 **347.** (улучшение) build_ffmpeg_args совмещает выбор режима (single vs concat), диспетчеризацию формата и финальную сборку -y/-i/output в одной функции -> Разбить на build_single_args/build_input_trim/dispatch_by_format, оставив build_ffmpeg_args тонким координатором. `backend/src/tools/args.rs`
- [ ] 🟠 **316.** (проблема) Добавление нового формата экспорта требует правки в build_ffmpeg_args, push_video_codec и build_concat_args одновременно -> Свести формат к одной таблице/enum с методами video_codec()/audio_codec()/container(), используемой во всех трёх местах. `backend/src/tools/args.rs`
- [x] ✅ **320.** `OutputFormat`/`VideoCodec` whitelist и compatibility проверяются при compile. `backend/src/domain/output.rs`, `backend/src/services/render.rs`
- [ ] 🟠 **326.** (баг) pad-фильтр не форсирует чётность итогового кадра, если crop/scale после него в цепочке отсутствуют -> Добавить unit-тест, который явно проверяет чётность выходного w/h для pad без последующего crop/scale, чтобы застраховать формулу от регрессий. `backend/src/tools/args.rs`
- [x] ✅ **327.** Even rounding не создаёт `crop=0`; regression test фиксирует one-pixel source. `backend/src/tools/args.rs`
- [ ] 🟠 **334.** (проблема) expected_output_secs не учитывает fade_in/fade_out и не совпадает по сортировке сегментов с build_concat_args -> Задокументировать явно, что expected_output_secs — оценка длительности контента без учёта fade, либо убрать fade из описания функции в комментарии, если он и не должен туда входить. `backend/src/tools/args.rs`
- [ ] 🟠 **339.** (проблема) censor и crop оба типизированы как model::Crop, хотя семантически это разные концепции (censor box vs crop rect) -> Ввести отдельный тип-алиас или newtype CensorBox(Crop), чтобы компилятор различал назначение полей. `backend/src/model.rs`
- [ ] 🟠 **346.** (проблема) censor применяется в исходных координатах до crop, но нет теста на порядок censor относительно rotate/flip -> Добавить unit-тест на комбинацию censor+rotate=90, документирующий, что censor координаты — всегда в исходной (до поворота) системе координат. `backend/src/tools/args.rs`
- [x] ✅ **348.** Codec-specific CRF defaults/ranges принадлежат typed `OutputSpec`. `backend/src/domain/output.rs`
- [ ] 🟠 **352.** (проблема) build_concat_args не поддерживает mp3/png/jpg/gif форматы из-за жёсткого matches! в build_ffmpeg_args, хотя multi-segment gif был бы осмысленным сценарием -> Либо расширить build_concat_args для аудио/still-форматов, либо явно возвращать ошибку в handler, если segments заданы вместе с неподдерживающим форматом. `backend/src/tools/args.rs`
- [ ] 🟠 **354.** (баг) scale с h=-1 может дать нечётную высоту и сломать кодирование в yuv420p -> Либо запретить -1 в validate_scale и разрешить только -2 (гарантированно чётный), либо документировать пользователю разницу и не считать -1 безопасным по умолчанию. `backend/src/tools/args.rs`
- [ ] 🟠 **366.** (баг) push_video_codec не имеет audio-ветки для 'mp3'/'png'/'jpg'/'gif' и не используется для них — эти форматы вообще не проходят через общую точку сборки видео-кодека -> Добавить doc-комментарий на push_video_codec, поясняющий, что она вызывается только для форматов из matches! в build_ffmpeg_args, чтобы связь между двумя списками была явной, а не подразумеваемой. `backend/src/tools/args.rs`
- [ ] 🟠 **367.** (баг) edit.quality игнорируется в prores-ветке, хотя поле принимается и документируется как общий CRF-параметр -> Либо использовать edit.quality как -profile:v (0-5 у prores_ks) с валидацией диапазона, либо задокументировать в model.rs, что quality не применяется к формату prores. `backend/src/tools/args.rs`
- [x] ✅ **341.** Явный codec разрешён только для MP4; WebM/H.265 отклоняется regression test. `backend/src/services/render.rs`
- [ ] 🟡 **342.** (идея) Нет доменной валидации, что pad и crop, применённые вместе, дают осмысленный результат -> Добавить интеграционный тест на комбинацию crop+pad с фиксацией ожидаемых размеров, либо предупреждение в API-ответе. `backend/src/tools/args.rs`
- [ ] 🟡 **322.** (дизайн) Магические числа CRF по умолчанию (32, 23, 28) разбросаны по коду без единого источника истины -> Вынести именованные константы DEFAULT_CRF_WEBM/DEFAULT_CRF_AV1/DEFAULT_CRF_H264/DEFAULT_CRF_H265 в один модуль. `backend/src/tools/args.rs`
- [ ] 🟡 **323.** (улучшение) mp3/png/jpg/gif обрабатываются как особые случаи в общем match вместо единой точки расширения формата -> Выделить каждую ветку в отдельную fn build_<format>_args(...) -> Vec<String>, а match оставить диспетчером. `backend/src/tools/args.rs`
- [ ] 🟡 **330.** (дизайн) video_filters напрямую зависит от конкретных строковых констант ffmpeg вместо промежуточного домена фильтров -> Ввести промежуточный enum VideoFilter { Crop{...}, Rotate(u32), ... } с отдельным to_ffmpeg_string(), чтобы порядок и построение синтаксиса были раздельными задачами. `backend/src/tools/args.rs`
- [ ] 🟡 **333.** (улучшение) Эпсилон 1e-6 для сравнения f64 захардкожен в шести разных местах без общей константы -> Ввести const F64_EPS: f64 = 1e-6; в начале файла и использовать её везде вместо литерала. `backend/src/tools/args.rs`
- [ ] 🟡 **338.** (дизайн) eq_changed сравнивает brightness/contrast/saturation с дефолтами 0.0/1.0/1.0, но эти дефолты не именованы как доменные константы -> Явно задокументировать или связать константы дефолтов между model.rs (default_one) и args.rs (eq_changed), например через общий модуль domain-констант. `backend/src/tools/args.rs`
- [ ] 🟡 **344.** (дизайн) Backend использует единый `LookPreset` enum, но frontend options ещё не генерируются из контракта. -> Включить enum в generated Rust/TS API/options manifest. `backend/src/domain/edit.rs`, `frontend/src/domain/edit.ts`
- [ ] 🟡 **358.** (дизайн) filter_preset и sanitize_color — закрытые для расширения справочники без единого источника допустимых значений для API-документации -> Добавить эндпоинт GET /api/capabilities (или аналог), отдающий списки допустимых filter/censor_color/codec/format значений, построенные на тех же константах, что использует args.rs. `backend/src/tools/args.rs`
- [ ] 🟡 **361.** (улучшение) drawbox (censor) форсирует чётность неявно только через clamp_rect_to_source в handlers, а crop форсирует явно через '& !1' в args.rs — асимметричные механизмы для похожей задачи -> Либо убрать дублирующий '& !1' из crop (раз чётность уже гарантирована upstream), либо добавить симметричную защиту для censor, чтобы обе ветки не полагались на разные уровни защиты. `backend/src/tools/args.rs`
- [ ] 🟡 **317.** (проблема) Выбор аудиокодека по формату продублирован между push_audio-вызовами в build_ffmpeg_args и match в build_concat_args -> Вынести fn audio_codec_for(format: &str) -> &'static str и использовать её в обоих местах. `backend/src/tools/args.rs`
- [ ] 🟡 **318.** (проблема) video_filters и audio_filters дублируют шаблон 'построить цепочку -> push через -vf/-af' в шести ветках формата -> Добавить хелпер push_vf/push_af(args, chain), принимающий уже построенную цепочку и делающий push только если она не пуста. `backend/src/tools/args.rs`
- [x] ✅ **321.** `LookPreset::parse` отклоняет неизвестные имена; adapter принимает enum. `backend/src/domain/edit.rs`
- [ ] 🟡 **324.** (проблема) Аудио-кодек и битрейт 128k хардкожены в push_audio и продублированы отдельной строкой в build_concat_args -> Вынести общий хелпер push_audio_codec_and_bitrate(args, format) и использовать его в обоих путях. `backend/src/tools/args.rs`
- [ ] 🟡 **325.** (проблема) parse_aspect допускает деформирующие пропорции (например 1:100) без предупреждения пользователю -> Добавить проверку разумного диапазона соотношения (например 1:5..5:1) с явной ошибкой при выходе за пределы. `backend/src/tools/args.rs`
- [ ] 🟡 **328.** (проблема) speed для видео (setpts) не ограничен диапазоном на уровне args.rs, хотя atempo для аудио жёстко клампится к 0.5..2.0 -> Либо клампить speed внутри video_filters тем же диапазоном, что и audio_filters, либо явно задокументировать инвариант 'вызывающий обязан клампить' в doc-комментарии функции. `backend/src/tools/args.rs`
- [x] ✅ **329.** Комментарий у `atempo` ссылается на invariant `EditPlan`, не frontend clamp. `backend/src/tools/args.rs`
- [x] ✅ **331.** `Rotation` содержит четыре состояния; полные обороты канонизируются, прочие углы отклоняются. `backend/src/domain/edit.rs`
- [ ] 🟡 **332.** (проблема) Проверка '(speed - 1.0).abs() > 1e-6 && speed > 0.0' дословно продублирована в video_filters и audio_filters -> Вынести fn speed_changed(speed: f64) -> bool и использовать в обеих функциях. `backend/src/tools/args.rs`
- [ ] 🟡 **335.** (проблема) format_secs — однострочная функция-обёртка над format!, не добавляющая логики, вопреки комментарию 'trimming trailing noise' -> Либо убрать вводящий в заблуждение комментарий, либо реально удалить незначащие нули, если это когда-то было целью. `backend/src/tools/args.rs`
- [ ] 🟡 **336.** (проблема) gif-путь строит filter_complex-подобный граф вручную внутри строки формата, смешивая video_filters с ручным split/palettegen синтаксисом -> Вынести построение gif-графа в отдельную fn build_gif_filter_graph(parts: &[String], fps: f64) -> String с явным комментарием о смешении синтаксисов. `backend/src/tools/args.rs`
- [ ] 🟡 **337.** (проблема) push_fps вызывается вручную в каждой ветке push_video_codec и build_ffmpeg_args вместо единой точки в сборке видео-аргументов -> Переместить единственный вызов push_fps в конец push_video_codec и убрать дублирующиеся вызовы из build_ffmpeg_args (av1/prores путь и так проходит через push_video_codec в build_concat_args, но не в build_ffmpeg_args — унифицировать эти два пути). `backend/src/tools/args.rs`
- [x] ✅ **340.** Wire `Crop` преобразуется в source-clamped `PixelRect`; adapter не принимает сырой DTO. `backend/src/domain/edit.rs`, `backend/src/services/render.rs`
- [ ] 🟡 **343.** (проблема) sanitize_color whitelist жёстко ограничен 4 цветами (black/white/gray/red) без возможности расширения без правки кода -> Вынести список допустимых цветов в статический массив const ALLOWED_CENSOR_COLORS, переиспользуемый и для валидации, и потенциально для описания в OpenAPI/типах фронтенда. `backend/src/tools/args.rs`
- [ ] 🟡 **345.** (проблема) parse_aspect полагается только на u32::parse без учёта возможных пробелов внутри числа (не только по краям) -> Добавить unit-тест на parse_aspect с граничными строками ('09:16', ' 9 : 16 ', '9:-16'), чтобы зафиксировать текущее поведение как контракт. `backend/src/tools/args.rs`
- [x] ✅ **349.** `CensorColor` enum whitelist отклоняет неизвестные значения вместо silent fallback. `backend/src/domain/edit.rs`
- [ ] 🟡 **350.** (проблема) filter: Option<String> в model.rs документирует только 4 из 8 реальных пресетов -> Обновить doc-комментарий в model.rs, перечислив все 8 текущих пресетов, либо сослаться на filter_preset как источник истины. `backend/src/model.rs`
- [ ] 🟡 **351.** (проблема) push_fps форматирует fps с фиксированными 3 знаками после запятой даже для целых значений -> Не критично технически; при желании можно использовать более компактное форматирование только для читаемости логов. `backend/src/tools/args.rs`
- [ ] 🟡 **355.** (проблема) fps для gif применяется отдельным путём с собственным дефолтом 12.0, минуя push_fps и его защиту диапазона -> Переиспользовать общую функцию resolve_fps(edit) -> f64 с единым дефолтом и диапазоном для всех форматов, включая gif. `backend/src/tools/args.rs`
- [ ] 🟡 **356.** (проблема) clamp_rect_to_source в handlers занижает минимальный размер прямоугольника до 1px для источников шириной/высотой 1 -> Для источников шириной/высотой 1 либо отклонять crop/censor полностью, либо документировать, что вырожденные источники не поддерживают crop/censor. `backend/src/handlers/mod.rs`
- [ ] 🟡 **357.** (проблема) push_audio дублирует финальный шаг push_video_codec (кодек+битрейт) как отдельную самостоятельную функцию вместо общей точки сборки аудио+видео пары -> Объединить в один вызов, принимающий format один раз и решающий и видео-, и аудио-кодек вместе, снижая риск рассинхрона между веткам. `backend/src/tools/args.rs`
- [ ] 🟡 **359.** (проблема) expected_output_secs игнорирует reverse и представляет длительность одинаково для reverse и не-reverse клипов -> Добавить короткий doc-комментарий, поясняющий, что reverse намеренно не входит в расчёт длительности, чтобы не создавать ложное ощущение недосмотра при код-ревью. `backend/src/tools/args.rs`
- [x] ✅ **360.** Handler получает extension через typed `OutputFormat`; plan compiler строго валидирует output semantics до spawn. `backend/src/domain/output.rs`, `backend/src/handlers/mod.rs`
- [ ] 🟡 **362.** (проблема) video_filters и build_concat_args по-разному вычисляют момент начала fade_out относительно out_dur, но сам out_dur получают из одного источника expected_output_secs -> Добавить unit-тест, сравнивающий out_dur, переданный в video_filters, для эквивалентных single-trim и single-segment сценариев, чтобы зафиксировать инвариант согласованности. `backend/src/tools/args.rs`
- [ ] 🟡 **363.** (баг) gif-путь не проходит через push_fps и не наследует его защиту, дублируя логику дефолта fps отдельно от остальных форматов -> Централизовать резолюцию fps (см. resolve_fps) для всех форматов включая gif, независимо от внешней нормализации. `backend/src/tools/args.rs`
- [ ] 🟡 **364.** (проблема) video_filters принимает весь &EditRequest вместо среза полей, скрывая реальные зависимости фильтра от 20+ полей структуры -> Либо разбить EditRequest на под-структуры (GeometryEdit, ColorEdit, TemporalEdit) и передавать их отдельно, либо явно перечислить используемые поля в doc-комментарии функции. `backend/src/tools/args.rs`
- [ ] 🟡 **365.** (проблема) format_secs используется только для input-side -ss/-t, а сегменты и fade используют format! напрямую с той же точностью .3, но без общей функции -> Заменить прямые format!("{:.3}", ...) на format_secs(...) везде, где форматируется временная величина в секундах, для единообразия. `backend/src/tools/args.rs`
- [ ] 🟡 **368.** (проблема) validate_scale допускает как -1, так и -2 для одного и того же поля без объяснения разницы в поведении ffmpeg -> Задокументировать разницу в doc-комментарии validate_scale или сузить whitelist до -2, если чётность обязательна для всех текущих кодеков (все они требуют yuv420p/yuv422p10le). `backend/src/handlers/mod.rs`

### Persistence layer (DIP) (41)

- [ ] 🔴 **369.** (проблема) Хендлеры и AppState жёстко зависят от конкретных типов Db и Library, общего порта-трейта для хранилища нет. -> Выделить трейты ProjectStore/JobStore/RenderCacheStore и MediaStore и параметризовать AppState ими вместо конкретных Db/Library. `backend/src/state.rs`
- [ ] 🟠 **403.** (улучшение) Маппинг kind -> подкаталог продублирован в main.rs отдельным трейтом вместо переиспользования Library::file_path. -> Сделать Library::file_path (или его логику маппинга kind->subdir) публичным методом MediaEntry/Library и переиспользовать в main.rs вместо отдельного трейта. `backend/src/main.rs`
- [ ] 🟠 **370.** (проблема) Удаление медиафайла и очистка render_cache не атомарны между Library и Db. -> Обернуть удаление файла из Library и очистку render_cache в одну единицу работы с откатом или сначала чистить кэш, потом файл. `backend/src/main.rs`
- [ ] 🟠 **371.** (проблема) Схема БД объявлена одной константой без системы миграций. -> Ввести пронумерованные миграции (например через sqlx::migrate!) вместо одной статической DDL-строки. `backend/src/db.rs`
- [ ] 🟠 **372.** (проблема) Таблицы jobs и render_cache не имеют retention-политики и растут неограниченно. -> Добавить периодическую очистку старых записей jobs/render_cache по updated_at/created_at, аналогично файловому TTL в main.rs. `backend/src/db.rs`
- [ ] 🟠 **375.** (проблема) Db и Library не тестируемы через in-memory реализацию, тесты гоняют реальную SQLite и файловую систему. -> Добавить трейт-порт хранилища с in-memory реализацией для быстрых юнит-тестов бизнес-логики поверх Db/Library. `backend/src/db.rs`
- [ ] 🟠 **376.** (проблема) upsert_project делает три последовательных запроса без транзакции, гонка при параллельных автосейвах. -> Обернуть SELECT+UPSERT в одну SQL-транзакцию или использовать INSERT ... ON CONFLICT(video_id) DO UPDATE вместо ручного SELECT-then-branch. `backend/src/db.rs`
- [ ] 🟠 **377.** (проблема) video_id не имеет UNIQUE-ограничения на уровне схемы, хотя upsert_project трактует его как уникальный ключ. -> Добавить UNIQUE(video_id) в определение таблицы projects, чтобы гонка приводила к контролируемой ошибке конфликта, а не к тихому дублированию. `backend/src/db.rs`
- [ ] 🟠 **378.** (проблема) Library.save() пишет tmp-файл с фиксированным именем, конкурентные вызовы add/remove гоняют друг друга по одному .tmp пути. -> Использовать уникальное имя tmp-файла (например с суффиксом Uuid или PID) для каждого вызова save(). `backend/src/library.rs`
- [ ] 🟠 **384.** (баг) persist_job перезаписывает created_at текущим временем при каждом апдейте вместо сохранения исходной даты создания. -> Передавать и сохранять оригинальный created_at из Job вместо пересчёта now при каждом INSERT. `backend/src/db.rs`
- [ ] 🟠 **387.** (проблема) Library::load молча проглатывает ошибку чтения файла без логирования и без различения 'файла нет' от 'файл повреждён'. -> Логировать через tracing::warn любую ошибку чтения, кроме ErrorKind::NotFound, и по-прежнему падать обратно на пустой список. `backend/src/library.rs`
- [ ] 🟠 **388.** (проблема) serde_json::from_slice в Library::load проглатывает ошибку парсинга без логирования. -> Логировать через tracing::error причину сбоя парсинга перед откатом на пустой список, чтобы потеря данных была видна в логах. `backend/src/library.rs`
- [ ] 🟠 **390.** (проблема) Library::list делает fs::metadata на каждый файл последовательно при каждом вызове. -> Запускать проверки metadata параллельно через futures::future::join_all вместо последовательного цикла. `backend/src/library.rs`
- [ ] 🟠 **395.** (баг) Ошибки cache_get маскируются под cache miss в вызывающем коде без логирования. -> Логировать через tracing::warn ветку Err(e) отдельно от Ok(None) перед возвратом false. `backend/src/handlers/mod.rs`
- [ ] 🟠 **404.** (проблема) cache-hit путь finish_from_render_cache не продлевает жизнь записи в render_cache при повторном использовании. -> При успешном cache-hit обновлять created_at (или отдельный last_used_at) записи render_cache, чтобы TTL считался от последнего использования. `backend/src/db.rs`
- [ ] 🟡 **373.** (улучшение) db.rs смешивает DDL-схему, SQL-запросы и DTO-маппинг в одном файле без разделения по агрегатам. -> Разбить db.rs на модули db/projects.rs, db/jobs.rs, db/render_cache.rs с общим db/mod.rs для пула соединений. `backend/src/db.rs`
- [ ] 🟡 **382.** (улучшение) row_to_job и row_to_project - свободные функции вне impl Db, разрывающие связность маппинга со схемой. -> Реализовать TryFrom<SqliteRow> для Job и Project вместо свободных функций. `backend/src/db.rs`
- [ ] 🟡 **393.** (улучшение) upsert_project делает финальный get_project(&id) вместо построения Project из уже известных данных. -> Возвращать Project напрямую из уже известных полей, делая отдельный SELECT только в ветке обновления для восстановления created_at. `backend/src/db.rs`
- [ ] 🟡 **401.** (улучшение) GET /api/projects возвращает полные video/edit JSON-блобы для каждого проекта в списке. -> Добавить облегчённый list-запрос без video/edit колонок для отображения списка проектов, подгружая полный JSON только по get_project. `backend/src/http/mod.rs`
- [ ] 🟡 **409.** (улучшение) JobStatus::from_token сворачивает любую нераспознанную строку в Pending без сигнала об ошибке. -> Добавить вариант JobStatus::Unknown(String) или логировать через tracing::warn при попадании в catch-all, прежде чем возвращать Pending. `backend/src/model.rs`
- [ ] 🟡 **374.** (баг) load_recent_jobs(0) неотличим от 'без лимита' в вызывающем коде. -> Явно задокументировать в сигнатуре load_recent_jobs, что limit<=0 означает пустой результат, а не 'без лимита', либо ввести Option<i64> для явности. `backend/src/db.rs`
- [ ] 🟡 **379.** (проблема) file_path() в Library жёстко зашивает подкаталоги 'outputs'/'sources' по kind без валидации значения. -> Заменить kind: String на enum MediaKind { Source, Output } с сериализацией, чтобы неверное значение отклонялось на десериализации. `backend/src/library.rs`
- [ ] 🟡 **380.** (проблема) MediaEntry::from_result молча подставляет пустые id/filename/url при отсутствии полей в JSON. -> Вернуть Result<MediaEntry, String> из from_result и явно проверять обязательные поля (id, filename, url) на этапе создания, а не полагаться на проверку в add(). `backend/src/library.rs`
- [ ] 🟡 **381.** (проблема) cache_delete_filename ищет по точному совпадению filename, но столбец не индексирован. -> Добавить CREATE INDEX ON render_cache(filename), раз это основной путь очистки при удалении файла. `backend/src/db.rs`
- [ ] 🟡 **383.** (проблема) Job::result хранится как TEXT-блоб JSON без структуры и индекса. -> Вынести часто используемые поля результата (url, filename) в отдельные колонки, оставив result_json для остального. `backend/src/db.rs`
- [ ] 🟡 **385.** (проблема) list_projects и load_jobs не имеют пагинации и грузят все записи одним запросом. -> Добавить необязательный лимит/offset к list_projects и load_jobs, аналогично load_recent_jobs. `backend/src/db.rs`
- [ ] 🟡 **386.** (проблема) Db::open жёстко фиксирует max_connections(5) без возможности конфигурации. -> Прокинуть max_connections через env-переменную по аналогии с MAX_CONCURRENT_JOBS в main.rs. `backend/src/db.rs`
- [ ] 🟡 **389.** (проблема) MediaEntry (доменная DTO) и Library (хранилище) находятся в одном файле без разделения на модель и репозиторий. -> Вынести MediaEntry в отдельный model.rs или library/model.rs, оставив library.rs только за хранилищем. `backend/src/library.rs`
- [ ] 🟡 **391.** (проблема) Нет общего понятия 'storage root' между Db и Library - два разных API для одного корня хранилища. -> Ввести общий StorageRoot/AppStorage тип, который оба конструктора принимают, вместо двух разных сигнатур пути. `backend/src/db.rs`
- [ ] 🟡 **392.** (проблема) get_project_by_video при гипотетическом дубле video_id молча возвращает только последний обновлённый. -> После добавления UNIQUE(video_id) эта проблема исчезнет сама; до этого - хотя бы логировать случай, когда SELECT COUNT(*) по video_id > 1. `backend/src/db.rs`
- [ ] 🟡 **394.** (проблема) Тесты db.rs и library.rs не проверяют поведение при повреждённых/некорректных данных на диске. -> Добавить тесты, которые пишут заведомо некорректные данные (например, произвольную строку status) в БД/файл и проверяют, что row_to_job/Library::load не паникуют и логируют проблему. `backend/src/db.rs`
- [ ] 🟡 **396.** (проблема) Все вызовы db.rs для render_cache в вызывающем коде молча игнорируют ошибки без логирования, в отличие от Library. -> Заменить let _ = ... на явное логирование ошибки cache_put/cache_delete по аналогии с Library::save. `backend/src/handlers/mod.rs`
- [ ] 🟡 **397.** (проблема) db.rs импортирует now_secs из library.rs ради тривиальной утилиты времени. -> Вынести now_secs в отдельный util-модуль (например crate::time), не привязанный ни к db, ни к library. `backend/src/db.rs`
- [ ] 🟡 **398.** (проблема) Колонка schema_version объявлена в схеме projects, но нигде не читается и не задаётся явно приложением. -> Либо использовать schema_version для версионирования формата video/edit JSON, либо убрать колонку, пока не появится нужда. `backend/src/db.rs`
- [ ] 🟡 **399.** (проблема) idx_render_cache_created_at - мёртвый индекс, ни один запрос не сортирует и не фильтрует по created_at. -> Либо удалить неиспользуемый индекс, либо добавить retention-запрос (см. находку про отсутствие очистки), который реально будет использовать сортировку по created_at. `backend/src/db.rs`
- [ ] 🟡 **400.** (проблема) Таблица projects не имеет индекса по updated_at, хотя list_projects и get_project_by_video сортируют именно по нему. -> Добавить CREATE INDEX ON projects(updated_at DESC). `backend/src/db.rs`
- [ ] 🟡 **402.** (проблема) Db::open неявно требует, чтобы каталог storage уже существовал, но создаёт его только main.rs. -> Создавать storage-каталог внутри Db::open через tokio::fs::create_dir_all перед подключением, чтобы модуль не зависел от вызывающего кода. `backend/src/db.rs`
- [ ] 🟡 **405.** (проблема) Project (только Serialize) заставляет хендлеры парсить входящий JSON вручную через serde_json::Value. -> Ввести отдельный ProjectUpsertRequest со строгими полями и Deserialize, заменив ручной разбор serde_json::Value в хендлере. `backend/src/http/mod.rs`
- [ ] 🟡 **406.** (проблема) Db::open не задаёт synchronous pragma, WAL с дефолтным synchronous=FULL избыточно тормозит запись для локального инструмента. -> Добавить .synchronous(SqliteSynchronous::Normal) к SqliteConnectOptions в Db::open. `backend/src/db.rs`
- [ ] 🟡 **407.** (проблема) upsert_project не ограничивает длину name, хотя video/edit ограничены MAX_PROJECT_JSON_BYTES на уровне хендлера. -> Добавить проверку длины name (например, до нескольких сотен символов) либо в хендлере, либо как защиту внутри upsert_project. `backend/src/db.rs`
- [ ] 🟡 **408.** (баг) cancel_open_job персистит job через persist_job (INSERT ON CONFLICT), что перегенерирует created_at=now даже для уже существующей записи при отмене. -> Передавать в db.persist_job исходный job.created_at (после добавления этого поля в Job/DB, см. отдельную находку про created_at) вместо всегда использования now при INSERT. `backend/src/state.rs`

### Security и сеть (45)

- [x] 🔴 **410.** (проблема) SSRF-проверка и фактический connect были разделены (TOCTOU/DNS rebinding) -> Закрыто в раунде 6: proxy валидирует всю DNS-выборку и соединяется с тем же pinned `SocketAddr`. `backend/src/tools/egress_proxy.rs`
- [x] 🔴 **411.** (проблема) validate_url не проверял редиректы `yt-dlp` -> Закрыто в раунде 6: каждый HTTP/CONNECT target проходит единую policy. `backend/src/tools/egress_proxy.rs`
- [ ] 🔴 **421.** (проблема) Ни один API-эндпоинт не требует аутентификации -> Добавить хотя бы простую проверку статического API-ключа или basic-auth middleware перед /api и /files роутами, конфигурируемую через env. `backend/src/lib.rs`
- [x] 🔴 **424.** (баг) sanitize_ext допускал опасные клиентские расширения -> Закрыто в раунде 5: клиентское имя display-only, server suffix выводится из `ffprobe format_name` по allow-list. `backend/src/handlers/upload.rs`
- [x] 🔴 **445.** (баг) Файл с расширением .html/.svg, прошедший sanitize_ext, отдаётся ServeDir с соответствующим Content-Type, создавая stored XSS -> Закрыто: расширение выводится из `ffprobe format_name`, `/files` получает `nosniff` и sandbox CSP. `backend/src/handlers/upload.rs`, `backend/src/lib.rs`
- [ ] 🟠 **412.** (баг) is_blocked_ip не перечисляет явно IPv4 broadcast и все зарезервированные диапазоны, полагаясь на широкий octets[0] >= 240 -> Добавить явную проверку v4.is_broadcast() (255.255.255.255) и тест на неё, не полагаясь молча на побочный эффект диапазона 240+. `backend/src/tools/net.rs`
- [x] 🟠 **414.** (проблема) validate_url разрешал произвольный порт на публичном хосте -> Закрыто в раунде 6: initial guard и proxy-hop разрешают только 80/443. `backend/src/tools/net.rs`
- [ ] 🟠 **416.** (проблема) CORS default origins в lib.rs жёстко зашиты под dev-порты 5173/8088 без явного требования CORS_ALLOW_ORIGINS в проде -> При старте в non-dev окружении (например, когда BIND_ADDR не 127.0.0.1) логировать warn, если CORS_ALLOW_ORIGINS не задан, а дефолты все еще localhost. `backend/src/lib.rs`
- [ ] 🟠 **418.** (проблема) ServeDir для /files/sources и /files/outputs не ограничивает конкурентные Range-запросы к одному большому видео -> Добавить rate-limiting/concurrency-cap middleware (например tower::limit::ConcurrencyLimitLayer) перед ServeDir или ограничить на уровне nginx. `backend/src/lib.rs`
- [ ] 🟠 **422.** (проблема) upload_handler пишет файл на диск до какой-либо валидации содержимого, позволяя гигабайтные не-видео файлы полностью записываться прежде чем быть отклонёнными -> Проверять Content-Type и/или делать раннюю частичную проверку (например, ffprobe по первым N МБ через pipe) прежде чем стримить весь файл на диск. `backend/src/handlers/mod.rs`
- [x] 🟠 **425.** (проблема) probe_video не сверял реальный контейнер с клиентским расширением -> Закрыто в раунде 5: клиентский suffix игнорируется, server suffix выбирается по контейнеру/codec combination. `backend/src/handlers/upload.rs`
- [ ] 🟠 **431.** (проблема) video_id в EditRequest не валидируется как UUID и используется в find_source/find_by_id, который линейно сравнивает file_stem по всей директории -> Валидировать video_id как UUID на входе edit_handler/import_handler и возвращать 400 до похода в файловую систему. `backend/src/handlers/mod.rs`
- [ ] 🟠 **433.** (проблема) Backend Docker-образ запускается от root: нет директивы USER для непривилегированного пользователя -> Добавить непривилегированного пользователя (RUN useradd -m appuser) и переключиться на него директивой USER appuser перед CMD. `backend/Dockerfile`
- [ ] 🟠 **434.** (проблема) yt-dlp устанавливается через curl без проверки контрольной суммы или подписи -> Скачивать также файл контрольных сумм релиза и проверять sha256sum скачанного бинарника перед chmod +x. `backend/Dockerfile`
- [ ] 🟠 **440.** (проблема) volumes.storage не имеет ограничения размера, при MAX_UPLOAD_BYTES=2GiB на файл и FILE_TTL_HOURS=0 по умолчанию диск может расти неограниченно -> Либо задать FILE_TTL_HOURS в environment backend-сервиса по умолчанию, либо ограничить объём volume через driver_opts, либо явно задокументировать это в README как эксплуатационное требование. `docker-compose.yml`
- [ ] 🟡 **419.** (идея) Нет теста, подтверждающего что SQLite-файл не отдаётся через ServeDir -> Добавить интеграционный тест в backend/tests, который убеждается, что запрос к БД-файлу через /files/sources или /files/outputs возвращает 404, а не 200. `backend/src/lib.rs`
- [ ] 🟡 **413.** (улучшение) looks_like_noncanonical_ip дублирует парсинг IP-подобных меток вместо переиспользования std IpAddr parse -> Вынести общую логику разбора octal/hex/decimal представлений IP в одну функцию, используемую и для канонического, и для неканонического случая. `backend/src/tools/net.rs`
- [x] 🟡 **415.** (дизайн) Комментарий 'basic SSRF guard' занижал фактический объём защиты -> Закрыто в раунде 6: module docs разделяют URL-policy и enforced transport с DNS pinning. `backend/src/tools/net.rs`, `backend/src/tools/egress_proxy.rs`
- [ ] 🟡 **417.** (улучшение) cors_origins_from_env читает CORS_ALLOW_ORIGINS один раз при старте роутера без отдельного этапа валидации конфигурации -> Валидировать CORS_ALLOW_ORIGINS в main.rs на старте и завершать процесс с понятной ошибкой, если ни одна валидная origin не была распознана при непустой переменной. `backend/src/lib.rs`
- [ ] 🟡 **423.** (улучшение) upload_handler совмещает парсинг multipart, запись на диск, санитизацию расширения, пробирование видео и обновление библиотеки в одной функции -> Выделить сохранение файла, санитизацию имени и построение JSON-ответа в отдельные вспомогательные функции с независимыми unit-тестами. `backend/src/handlers/mod.rs`
- [ ] 🟡 **427.** (улучшение) Формирование JSON-ответа VideoInfo дублируется почти дословно в import_handler и upload_handler -> Вынести построение этого JSON в общую функцию, принимающую id, filename, ProbeInfo, title и sizeBytes. `backend/src/handlers/mod.rs`
- [ ] 🟡 **429.** (дизайн) finish_job принимает kind как строковый тег вместо enum, не защищённый компилятором от опечаток -> Заменить &str на enum MediaKind { Source, Output } с явным match при сериализации в MediaEntry. `backend/src/handlers/mod.rs`
- [ ] 🟡 **441.** (улучшение) docker-compose.yml не пробрасывает MAX_CONCURRENT_JOBS, MAX_UPLOAD_BYTES, JOB_TIMEOUT_SECS, FILE_TTL_HOURS, CORS_ALLOW_ORIGINS явно, полагаясь целиком на дефолты в main.rs -> Добавить хотя бы закомментированные примеры этих переменных в environment: секцию docker-compose.yml, чтобы конфигурация была обнаруживаемой без чтения исходников main.rs. `docker-compose.yml`
- [ ] 🟡 **442.** (дизайн) BIND_ADDR=127.0.0.1 в Dockerfile и docker-compose.yml=0.0.0.0 расходятся, что критично при прямом docker run без compose -> Уточнить комментарий в Dockerfile, что при самостоятельном docker run с -p потребуется явно передать -e BIND_ADDR=0.0.0.0. `backend/Dockerfile`
- [ ] 🟡 **448.** (улучшение) upload_handler не проверяет Content-Type multipart-поля, полагаясь только на имя файла и позднюю проверку ffprobe -> Проверять field.content_type() на префикс video/ как раннюю (не единственную) эвристику до начала записи файла. `backend/src/handlers/mod.rs`
- [ ] 🟡 **420.** (проблема) CORS allow_headers ограничен только CONTENT_TYPE, что заблокирует будущий Authorization без правки CORS-слоя -> Зафиксировать в комментарии рядом с cors_layer, что список allow_headers нужно расширить при добавлении аутентификации. `backend/src/lib.rs`
- [ ] 🟡 **426.** (проблема) upload_handler читает только первое подходящее multipart-поле и молча возвращает Ok, игнорируя остальные части запроса -> Либо явно документировать и валидировать, что запрос должен содержать ровно одно файловое поле, отклоняя лишние поля с 400, либо поддержать множественную загрузку осознанно. `backend/src/handlers/mod.rs`
- [ ] 🟡 **428.** (проблема) job_timeout читает переменную окружения JOB_TIMEOUT_SECS при каждом вызове import_handler/edit_handler вместо однократного чтения при старте -> Прочитать JOB_TIMEOUT_SECS один раз в main.rs и передавать значение через AppState, как сделано для MAX_CONCURRENT_JOBS. `backend/src/handlers/mod.rs`
- [ ] 🟡 **430.** (проблема) render_cache_key схлопывает все несериализуемые EditRequest в один и тот же пустой ключ -> Заменить unwrap_or_default() на явную ошибку/bail при сбое сериализации вместо тихого schlopping в один ключ. `backend/src/handlers/mod.rs`
- [ ] 🟡 **432.** (проблема) find_by_id делает полный линейный перебор директории sources при каждом /api/edit запросе -> Хранить сопоставление video_id -> путь к файлу в индексе (например, в SQLite или in-memory HashMap), заполняемом при импорте/загрузке, вместо перебора директории. `backend/src/tools/mod.rs`
- [ ] 🟡 **435.** (проблема) Build-стадия делает COPY . . перед cargo build, копируя весь build-контекст backend/ в промежуточный слой -> Добавить в backend/.dockerignore явные записи для *.md, tests/ (если не нужны в runtime-образе) и любых потенциальных .env файлов, чтобы гарантия не зависела молча от расположения build-контекста. `backend/Dockerfile`
- [ ] 🟡 **436.** (проблема) Версии базовых образов rust:1-bookworm и debian:bookworm-slim не зафиксированы по digest -> Закрепить оба образа по sha256-digest (FROM debian:bookworm-slim@sha256:...) для воспроизводимых сборок. `backend/Dockerfile`
- [ ] 🟡 **437.** (проблема) apt-get install ставит curl и python3 в финальный рантайм-образ, хотя они нужны только на этапе установки yt-dlp -> Удалить curl (apt-get purge -y curl) в конце той же RUN-инструкции после скачивания yt-dlp, оставив только действительно нужные для рантайма пакеты. `backend/Dockerfile`
- [ ] 🟡 **438.** (проблема) Нет HEALTHCHECK в Dockerfile и healthcheck в docker-compose, несмотря на существующий /api/health эндпоинт -> Добавить HEALTHCHECK CMD curl -f http://localhost:8080/api/health в Dockerfile или healthcheck: секцию в docker-compose.yml, ссылающуюся на этот эндпоинт. `backend/Dockerfile`
- [ ] 🟡 **439.** (проблема) backend не публикует порт наружу, но и не изолирован explicit internal-сетью — полагается только на отсутствие ports: -> Объявить явную internal-сеть для backend и подключить к ней frontend отдельным вторым интерфейсом, либо задокументировать, что изоляция опирается только на отсутствие ports: и это осознанный компромисс MVP. `docker-compose.yml`
- [ ] 🟡 **443.** (проблема) ffmpeg-аргументы логируются целиком через tracing::info! без редактирования путей -> Понизить подробность до DEBUG или логировать только идентификаторы video_id/out_id вместо полных ffmpeg-аргументов на уровне INFO. `backend/src/handlers/mod.rs`
- [x] 🟡 **444.** (проблема) upload_handler не ограничивал число одновременно принимаемых upload-запросов -> Закрыто в раунде 7 независимым fail-fast upload pool. `backend/src/handlers/upload.rs`, `backend/src/state.rs`
- [ ] 🟡 **446.** (проблема) Ни backend, ни nginx не выставляют X-Content-Type-Options: nosniff или Content-Security-Policy -> Добавить add_header X-Content-Type-Options nosniff; и базовый Content-Security-Policy в блок server nginx.conf. `frontend/nginx.conf`
- [ ] 🟡 **447.** (проблема) /api/import не ограничивает размер скачиваемого видео, только высоту через MAX_HEIGHT и общий таймаут -> Добавить флаг yt-dlp --max-filesize с лимитом, согласованным с MAX_UPLOAD_BYTES, чтобы download не мог превысить тот же порог, что и ручная загрузка. `backend/src/tools/mod.rs`
- [ ] 🟡 **449.** (проблема) Заголовок yt-dlp title сохраняется в библиотеку без ограничения длины и без санитизации -> Ограничить длину title (например, до 500 символов) и отфильтровать управляющие символы перед сохранением в MediaEntry. `backend/src/tools/mod.rs`
- [ ] 🟡 **450.** (баг) BIND_ADDR с некорректным значением молча откатывается на 127.0.0.1 вместо явной ошибки старта -> Заменить unwrap_or_else на явный panic!/expect с сообщением о некорректном BIND_ADDR, чтобы ошибка конфигурации была видна сразу при старте контейнера. `backend/src/main.rs`
- [ ] 🟡 **451.** (проблема) /api/health отдаёт точные версии ffmpeg и yt-dlp без аутентификации -> Либо скрыть версии инструментов за отдельным защищённым эндпоинтом, либо осознанно принять риск для MVP и задокументировать это в комментарии к health_handler. `backend/src/handlers/mod.rs`
- [ ] 🟡 **452.** (проблема) job.error, включающий хвост stderr ffmpeg/yt-dlp, персистится в SQLite и отдаётся клиенту через GET /api/jobs/:id без редактирования путей -> Санитизировать job.error перед сохранением/выдачей клиенту, отфильтровывая абсолютные пути файловой системы сервера. `backend/src/handlers/mod.rs`
- [ ] 🟡 **453.** (проблема) clamp_rect_to_source не проверяет переполнение u32 при вычитании source_width/source_height -> Заменить прямое вычитание на checked_sub с fallback на 0, чтобы корректность не зависела от точного порядка условий выше по функции. `backend/src/handlers/mod.rs`
- [ ] 🟡 **454.** (проблема) render_cache_key не проверяет, что video_id в EditRequest всё ещё принадлежит существующему файлу на момент создания кэш-ключа -> Явно задокументировать, что кэш-ключ полагается на уникальность UUID video_id на весь срок жизни файла, либо включить в ключ хэш содержимого/mtime исходника. `backend/src/handlers/mod.rs`

### API contract и обработка ошибок (45)

- [x] 🔴 **455.** (проблема) Не было общего типа ошибки/IntoResponse -> Закрыто в раунде 8: `error.rs` централизует AppError/AppResult, безопасный internal mapping и JSON `{error, code}` для handlers/extractors/fallbacks. `backend/src/error.rs`
- [ ] 🔴 **456.** (баг) Ошибка `EditPlan::compile` приходит только как terminal job error, а не в тело `POST /api/edit`. -> Выполнять source-independent DTO→domain validation до enqueue, source-aware compile оставить worker-у. `backend/src/handlers/mod.rs`
- [x] 🟠 **457.** (баг) cancel_handler имел отдельную форму ошибки -> Закрыто в раунде 8: not-found/conflict используют AppError. `backend/src/handlers/mod.rs`
- [ ] 🟠 **458.** (проблема) import_handler и upload_handler дублируют ручную сборку VideoInfo-JSON с разными допущениями о title -> Вынести общий builder fn video_info_json(id, path, title, size) -> Value и передавать title как параметр из каждого источника. `backend/src/handlers/mod.rs`
- [ ] 🟠 **459.** (проблема) Ручной json!() вместо typed DTO во всех async job-хендлерах -> Добавить в model.rs структуры VideoInfo и EditResult с Serialize и заменить json!() на них в обоих хендлерах. `backend/src/handlers/mod.rs`
- [x] 🟠 **460.** (проблема) frontend угадывал форму ошибки по HTTP-статусу -> Закрыто в раунде 8: единый parser читает `{error, code}`, сохраняет text fallback и создаёт typed ApiError. `frontend/src/api.ts`
- [ ] 🟠 **461.** (проблема) Job.result типизирован как serde_json::Value / TS unknown — нет единой формы результата job -> Ввести серверный enum JobResult { Video(VideoInfo), Output(EditResult) } с serde(untagged) и зеркальный union-тип в types.ts вместо unknown. `backend/src/model.rs`
- [ ] 🟠 **465.** (проблема) EditRequest — одна плоская структура на 27+ полей без группировки по фиче -> Разбить EditRequest на вложенные группы с #[serde(flatten)] (TimingOptions, ColorOptions, ExportOptions), сохранив совместимость сериализации. `backend/src/model.rs`
- [ ] 🟠 **466.** (проблема) GET /api/library и GET /api/projects отдают весь список без пагинации -> Добавить query-параметры limit/offset (или курсор) в оба хендлера и соответствующие типы в api.ts. `backend/src/handlers/library.rs`
- [ ] 🟠 **467.** (баг) MediaEntry.kind в Rust — произвольная String, в TS — union 'source'|'output' без валидации на границе -> Заменить String на enum MediaKind { Source, Output } с #[serde(rename_all="lowercase")] в library.rs, что сделает несоответствие невозможным по построению. `backend/src/library.rs`
- [ ] 🟠 **468.** (проблема) POST /api/edit не возвращает 404, если video_id не существует — ошибка видна только после факта в error job -> Либо проверить существование source синхронно до spawn и вернуть 404, либо задокументировать асинхронную семантику как контракт и не пытаться её "чинить" частично. `backend/src/handlers/mod.rs`
- [ ] 🟠 **471.** (проблема) `services/render.rs::normalize_request` ещё совмещает reject-policy и source-dependent canonicalization. -> Разделить wire validation и source normalization на два шага с typed error enum.
- [ ] 🟠 **474.** (проблема) ProjectDto.edit — Partial<EditState> во фронте, но бэкенд хранит edit как serde_json::Value без проверки формы -> Переиспользовать EditRequest (или его подмножество) как typed Deserialize для поля edit в project_upsert_handler вместо произвольного Value. `backend/src/db.rs`
- [ ] 🟠 **482.** (проблема) Job — одна структура на все статусы; result/error/progress/stage валидны только в подмножестве состояний -> Смоделировать как enum JobState { Pending, Running{progress,stage}, Done{result}, Error{message}, Cancelled, Interrupted } с serde(tag="status") вместо плоской структуры с опциональными полями. `backend/src/model.rs`
- [ ] 🟠 **483.** (проблема) POST /api/projects принимает произвольный serde_json::Value без typed DTO для тела запроса -> Ввести struct ProjectUpsertRequest { video_id: String, name: Option<String>, video: Value, edit: Value } с Deserialize и убрать ручное индексирование Value. `backend/src/http/mod.rs`
- [ ] 🟠 **487.** (баг) upload_handler возвращает 400 BAD_REQUEST на ошибку чтения multipart-поля, даже если она вызвана обрывом соединения клиента -> Различать multipart::Error по типу (обрыв потока -> просто прервать без ответа/499-подобная семантика, реальная ошибка формата -> 400). `backend/src/handlers/mod.rs`
- [x] 🟠 **488.** (проблема) project_upsert_handler смешивал parsing/name/persistence -> Закрыто в раунде 8: `parse_project_body` и `resolve_project_name` возвращают ParsedProject, handler вызывает только repository. `backend/src/http/mod.rs`
- [ ] 🟠 **492.** (баг) cancelJob проглатывает даже успешный не-2xx ответ (404/409 CancelJobOutcome), не давая вызывающему коду отличить исходы -> Вернуть из cancelJob Promise<'cancelled'|'not_found'|'already_finished'|'network_error'> вместо void, разобрав тело/статус ответа. `frontend/src/api.ts`
- [ ] 🟠 **495.** (баг) project_get_handler (GET /api/projects/:id) и project_list_handler/getProjects/deleteProject объявлены и экспортированы, но не используются нигде на фронтенде -> Либо удалить неиспользуемые эндпоинты/функции, либо подключить их к UI (например список сохранённых проектов), если такая фича планируется. `backend/src/http/mod.rs`
- [ ] 🟡 **462.** (проблема) GET /api/health всегда отвечает 200, даже когда status: "degraded" -> Возвращать (StatusCode::SERVICE_UNAVAILABLE, Json(...)) при status == "degraded", чтобы код ответа и тело были согласованы. `backend/src/http/mod.rs`
- [ ] 🟡 **463.** (проблема) health-эндпоинт не используется фронтендом вовсе -> Добавить getHealth() в api.ts и показывать статус в UI (например баннер при status !== "ok"). `frontend/src/api.ts`
- [ ] 🟡 **464.** (проблема) Нет версионирования API — все пути живут под /api без /v1 -> Либо задокументировать as-is для локального MVP как осознанное решение, либо ввести /api/v1 префикс до появления второго клиента контракта. `backend/src/lib.rs`
- [x] 🟡 **469.** (баг) upload_handler выдавал сырой io::Error -> Закрыто в раунде 8: source логируется как internal cause, клиент получает безопасный `internal_error`. `backend/src/handlers/upload.rs`, `backend/src/error.rs`
- [ ] 🟡 **470.** (проблема) ImportRequest.start/end не валидируются на уровне модели -> Добавить #[serde(deny_unknown_fields)] и явную проверку 0 <= start < end в отдельной validate()-функции ImportRequest, вызываемой синхронно в import_handler до spawn. `backend/src/model.rs`
- [x] 🟡 **472.** (проблема) job_status_handler возвращал пустой 404 -> Закрыто в раунде 8 общим `not_found` envelope. `backend/src/handlers/mod.rs`
- [ ] 🟡 **473.** (проблема) getProjectByVideo — единственное место во фронте, где 404 трактуется как валидный null-результат -> Ввести общий helper fetchOrNull/fetchOkOr404, явно кодирующий семантику "404 = ожидаемое отсутствие" одним способом для всех трёх мест. `frontend/src/api.ts`
- [ ] 🟡 **475.** (проблема) Project.video — тоже нетипизированный Value, дублирующий VideoInfo без проверки полей -> Десериализовать video как typed VideoInfo DTO (после его введения по пункту дублирования json!()) вместо серого Value. `backend/src/db.rs`
- [ ] 🟡 **476.** (проблема) MAX_PROJECT_JSON_BYTES = 64KB — magic number без сообщения клиенту о лимите заранее -> Экспортировать лимит в GET /api/health или отдельный /api/config эндпоинт, чтобы фронтенд мог предупреждать до отправки большого edit-состояния (например при работе с очень длинным списком segments). `backend/src/http/mod.rs`
- [ ] 🟡 **477.** (проблема) Формирование URL /files/sources/... и /files/outputs/... строковой интерполяцией в нескольких местах бэкенда -> Ввести helper fn source_url(filename) / output_url(filename) в handlers/mod.rs и переиспользовать во всех трёх местах. `backend/src/handlers/mod.rs`
- [ ] 🟡 **478.** (проблема) pollJob сравнивает статус со строковыми литералами вручную вместо exhaustive switch по JobStatus -> Переписать на switch (job.status) с exhaustive проверкой через never в default, что заставит компилятор упасть при добавлении нового статуса без обработки. `frontend/src/api.ts`
- [ ] 🟡 **479.** (проблема) Захардкоженный интервал поллинга 500мс в pollJob не настраивается и не учитывает backoff -> Ввести нарастающий интервал (например 500мс -> 2с после первых 10 тиков) или вынести константу с комментарием, почему выбрано именно 500мс. `frontend/src/api.ts`
- [ ] 🟡 **480.** (проблема) BACKEND_DOWN — единственное клиентское сообщение об ошибке, зашитое на русском в api.ts, тогда как остальные тексты приходят с сервера -> Задокументировать BACKEND_DOWN как осознанное клиентское исключение (единственный случай, когда сервер физически недостижим и не может прислать текст), не смешивая с остальными серверными сообщениями. `frontend/src/api.ts`
- [ ] 🟡 **481.** (проблема) ResultInfo в types.ts не имеет соответствующего Rust DTO — форма выведена только из ручного json!() в edit_handler -> После введения typed EditResult DTO (см. отдельный пункт про json!()) держать ResultInfo как его прямое зеркало и проверять расхождение в CI-тесте контракта, если такой существует. `frontend/src/types.ts`
- [ ] 🟡 **484.** (проблема) CORS allow_methods не включает PATCH/PUT, ограничивая эволюцию контракта на уровне инфраструктуры -> Либо оставить as-is как осознанный минимализм MVP, либо добавить Method::PATCH заранее, если планируется частичное обновление проектов. `backend/src/lib.rs`
- [ ] 🟡 **485.** (баг) Ошибка «source video not found» из tools::find_source долетает до job.error на английском -> Обернуть ошибку find_source в edit_handler через .map_err в русское сообщение ("источник не найден: {video_id}") перед пробросом в outcome. `backend/src/handlers/mod.rs`
- [ ] 🟡 **486.** (проблема) import_handler и edit_handler дублируют последовательность finish_job/drain/tx, но edit_handler дополнительно пишет в render cache только внутри себя -> Вынести общий хвост в helper fn finalize_job(st, jid, tx, drain, outcome, kind) -> bool, а cache_put оставить отдельным вызовом только в edit-пути после helper'а. `backend/src/handlers/mod.rs`
- [ ] 🟡 **489.** (баг) ensure_project_json_size сериализует JSON дважды на каждый upsert без необходимости в успешном пути -> Считать размер по одной комбинированной сериализации {video, edit} или переиспользовать уже посчитанные байты для последующей записи в БД вместо повторной сериализации. `backend/src/http/mod.rs`
- [ ] 🟡 **490.** (проблема) Формат Job.error — plain String — не различает пользовательскую ошибку валидации от внутренней ошибки ffmpeg/IO -> Добавить в Job поле error_kind: Option<ErrorKind> (Validation | Internal) либо разделить сообщение на user-facing и internal (логируемое отдельно через tracing) в finish_job. `backend/src/model.rs`
- [ ] 🟡 **491.** (проблема) getJob и pollJob не переиспользуют список терминальных статусов, уже определённый на бэкенде через JobStatus::is_terminal -> Добавить в types.ts функцию isTerminalStatus(status: JobStatus): boolean и использовать её и в pollJob, и в любом другом месте фронта, проверяющем завершённость job. `frontend/src/api.ts`
- [ ] 🟡 **493.** (проблема) `services/render.rs::normalize_request` жёстко перечисляет numeric policies и сообщения. -> Вводить declarative field policy вместе с generated API/options contract, не отдельным макросом.
- [ ] 🟡 **494.** (проблема) ProjectDto на фронте требует video: VideoInfo целиком, хотя store.ts передаёт в saveProject произвольный state.video без структурной проверки -> Типизировать параметр saveProject как { videoId: string; name?: string; video: VideoInfo; edit: Partial<EditState> } вместо Record<string, unknown>, чтобы TS проверял вызывающий код, а не только ответ. `frontend/src/api.ts`
- [ ] 🟡 **496.** (проблема) finish_from_render_cache совмещает чтение кэша, валидацию имени файла, проверку существования файла на диске и обновление статуса job в одной функции -> Вынести валидацию имени + существования файла в отдельную fn cached_output_is_usable(st, filename) -> bool и оставить в finish_from_render_cache только оркестрацию. `backend/src/handlers/mod.rs`
- [ ] 🟡 **497.** (проблема) stage: Option<String> в Job — произвольная строка без enum -> Ввести enum JobStage { Queued, Downloading, Processing } с #[serde(rename_all="lowercase")] и использовать его вместо строковых литералов во всех трёх местах присвоения. `backend/src/model.rs`
- [ ] 🟡 **498.** (проблема) MAX_PROJECT_JSON_BYTES проверяется отдельно для video и edit по 64KB каждый, но нет предела на итоговый размер строки, которую пишет upsert_project -> Добавить дополнительную проверку суммарного размера (video.len() + edit.len() <= MAX_PROJECT_TOTAL_BYTES) в project_upsert_handler. `backend/src/http/mod.rs`
- [ ] 🟡 **499.** (баг) deleteLibraryItem и deleteProject трактуют 404 как success молча, cancelJob не проверяет статус вовсе — три DELETE/POST-подобные мутации обрабатывают отсутствие ресурса по-разному -> Вынести общий helper типа async function deleteOrNotFound(path): Promise<void> и использовать его в обоих DELETE-вызовах; для cancelJob явно решить, какой из трёх паттернов уместен, и привести к нему же. `frontend/src/api.ts`

### Frontend state и store (50)

- [ ] 🟠 **500.** (проблема) store.ts на 675 строк совмещает импорт, экспорт, библиотеку, историю, пресеты, тему, плеер и автосейв проектов в одном модуле -> Разбить на отдельные модули (import/export, library, history, presets, theme, project-autosave) с явными публичными API. `frontend/src/store.ts`
- [ ] 🟠 **501.** (проблема) doImport и doUpload дублируют блок сброса state.edit из VideoInfo -> Вынести общий хелпер applyFreshEditFor(v: VideoInfo) и использовать его в doImport/doUpload/openFromLibrary. `frontend/src/store.ts`
- [ ] 🟠 **502.** (проблема) doImport, doUpload и doExport дублируют skeleton try/catch/finally для флагов importing/exporting -> Обобщить в helper вида runJob({flagKey, statusKey, ...}, fn) или единый композабл useAsyncJob. `frontend/src/store.ts`
- [ ] 🟠 **503.** (баг) doUpload не поддерживает отмену загрузки, хотя использует тот же UI-паттерн importing, что и doImport -> Либо поддержать AbortController в api.uploadFile и прокинуть отмену, либо явно задокументировать невозможность отмены в UI. `frontend/src/store.ts`
- [ ] 🟠 **506.** (проблема) Автосейв проекта построен на трёх module-level mutable let-переменных с неявными состояниями гонки -> Свести три переменные в один объект/enum состояния autosaveState = 'idle'|'restoring'|'restored' с явными переходами. `frontend/src/store.ts`
- [x] ✅ **509.** Snapshot replacement устранён вместе с №814: undo/redo применяет field-level commands без `JSON.parse` полного `EditState`; вложенные значения клонируются внутри command boundary. `frontend/src/domain/history.ts`, `frontend/src/store.ts`
- [ ] 🟠 **513.** (проблема) Компоненты импортируют широкий срез store вместо только нужных им полей -> Выделить под каждый домен (import, library, edit) отдельный reactive-срез или composable с точечным API вместо общего state. `frontend/src/components/MediaLibrary.vue, frontend/src/components/EditPanel.vue, frontend/src/components/UrlImport.vue`
- [ ] 🟠 **518.** (проблема) seekTo не защищён от NaN/Infinity во входном значении -> Добавить явную проверку Number.isFinite(t) в начале функции и игнорировать вызов при невалидном значении. `frontend/src/store.ts`
- [ ] 🟠 **521.** (баг) applyPreset делает Object.assign без валидации значений из localStorage -> Валидировать каждое поле p.edit по известной схеме EditState перед Object.assign, отбрасывая недопустимые значения. `frontend/src/store.ts`
- [ ] 🟠 **525.** (проблема) restoreProject молча проглатывает ошибку сети без уведомления пользователя -> Добавить toast('error', 'Не удалось восстановить сохранённые настройки') в catch restoreProject. `frontend/src/store.ts`
- [ ] 🟠 **526.** (проблема) persistProject не даёт пользователю знать, что автосейв не удался -> Показать неинтрузивный индикатор статуса автосейва (например, точку 'не сохранено' рядом с именем клипа) и/или toast при повторных неудачах подряд. `frontend/src/store.ts`
- [ ] 🟠 **528.** (баг) buildEditPayload не защищён от инвертированного trim (trimStart > trimEnd) -> В начале buildEditPayload явно клампить trimEnd = Math.max(e.trimStart, e.trimEnd) перед дальнейшими расчётами cut/trim. `frontend/src/store.ts`
- [ ] 🟠 **531.** (баг) deleteFromLibrary оставляет state.edit/state.result/history в противоречивом состоянии при удалении текущего клипа -> При сбросе state.video также вызывать resetHistory() и обнулять state.edit/state.result до дефолтных значений. `frontend/src/store.ts`
- [ ] 🟠 **532.** (баг) cancelImport/cancelExport не защищены от повторного клика во время отмены -> Добавить локальный флаг cancelling и обернуть api.cancelJob в try/catch с toast('error', ...) при неудаче. `frontend/src/store.ts`
- [ ] 🟠 **540.** (баг) setTrimStartFromPlayer/setTrimEndFromPlayer не обновляют cut.start/cut.end при сужении диапазона trim -> После изменения trim пересчитывать/клампить state.edit.cut в те же функции, аналогично тому, как это уже частично делает watcher в EditPanel.vue при cutEnabled. `frontend/src/store.ts`
- [ ] 🟠 **542.** (баг) doImport при ошибке валидации диапазона выходит раньше установки importing, разрешая спам одинаковых тостов -> Либо дебаунсить повторные идентичные ошибки валидации, либо кратковременно блокировать повторный вызов (например через локальный флаг validating). `frontend/src/store.ts`
- [ ] 🟡 **529.** (идея) Нет способа переименовать сохранённый пресет -> Добавить renamePreset(oldName, newName), обновляющую entry.name на месте без создания нового элемента списка. `frontend/src/store.ts`
- [ ] 🟡 **511.** (дизайн) Toast всегда автозакрывается через фиксированные 4 секунды независимо от длины текста и вида -> Добавить необязательный параметр durationMs (или вычислять из text.length) и увеличить дефолт для kind='error'. `frontend/src/toasts.ts`
- [x] 🟡 **517.** (улучшение) buildEditPayload — 60-строчная функция с плотной бизнес-логикой внутри store.ts -> Закрыто: `defaultEdit`, `parseTime`, `tierToCrf`, `sanitizeRect`, payload compiler и no-op detection вынесены в чистый `domain/edit.ts`; `store.ts` оставляет фасад. `frontend/src/domain/edit.ts`, `frontend/src/store.ts`
- [ ] 🟡 **537.** (улучшение) openFromLibrary смешивает синхронный сброс дефолтного edit с последующим асинхронным restoreProject -> Показать состояние загрузки (skeleton/disabled edit panel) на время restoreProject вместо промежуточного показа дефолтного edit. `frontend/src/store.ts`
- [ ] 🟡 **543.** (улучшение) PRESET_KEYS — захардкоженный список полей EditState, требующий ручной синхронизации -> Либо генерировать PRESET_KEYS из схемы EditState с явным исключением геометрических полей, либо добавить тест, проверяющий покрытие всех полей EditState (кроме геометрии) в PRESET_KEYS. `frontend/src/store.ts`
- [ ] 🟡 **545.** (улучшение) initTheme и loadPresets повторяют один и тот же паттерн безопасного чтения из localStorage с разной обработкой ошибок -> Вынести общий helper safeReadLocalStorage<T>(key, validate, fallback): T, используемый в обеих функциях и в persistPresets/applyTheme. `frontend/src/store.ts`
- [ ] 🟡 **504.** (проблема) Модуль регистрирует watch() на верхнем уровне при импорте, а не при инициализации приложения -> Обернуть регистрацию watch в экспортируемую функцию initStore(), вызываемую один раз из main.ts/App.vue. `frontend/src/store.ts`
- [ ] 🟡 **505.** (проблема) История и автосейв-watch не имеют cleanup API, таймеры остаются висеть при потере компонента -> Вернуть handle от watch() и таймеров в объекте, который можно явно dispose() из тестов или при смене видео. `frontend/src/store.ts`
- [ ] 🟡 **507.** (баг) savePreset использует name как ключ идентичности без нормализации регистра -> Нормализовать ключ сравнения через toLowerCase() при поиске совпадения, сохраняя оригинальный регистр для отображения. `frontend/src/store.ts`
- [ ] 🟡 **508.** (баг) loadLibrary молча глотает ошибку без уведомления пользователя -> Добавить toast('error', ...) в catch loadLibrary для единообразия с остальными сетевыми операциями. `frontend/src/store.ts`
- [ ] 🟡 **510.** (проблема) toast() вызывается с разнородными форматами сообщений без единой точки форматирования ошибок -> Ввести helper errorText(e: unknown): string и использовать его во всех catch-блоках вместо повторяющегося тернарника. `frontend/src/store.ts`
- [ ] 🟡 **512.** (проблема) dismissToast использует findIndex+splice вместо filter, а nextId — незащищённая module-level переменная -> Заменить на toasts.splice(0, toasts.length, ...toasts.filter(t => t.id !== id)) или просто оставить filter-присваивание и добавить экспортируемый resetToasts() для тестов. `frontend/src/toasts.ts`
- [ ] 🟡 **514.** (баг) resetHistory и history.past хранят до 100 полных JSON-снапшотов EditState без сжатия -> Либо уведомлять пользователя при достижении лимита истории, либо хранить дифф вместо полной копии state.edit на каждый шаг. `frontend/src/store.ts`
- [ ] 🟡 **515.** (проблема) store.ts напрямую импортирует toast из конкретного модуля и обращается к document/localStorage напрямую -> Ввести интерфейсы NotificationPort и StoragePort, инжектируемые в store, чтобы логика была тестируема без реального DOM/localStorage. `frontend/src/store.ts`
- [ ] 🟡 **516.** (баг) loadPresets и initTheme не различают 'нет данных' и 'испорченные данные' в localStorage -> В catch логировать/чистить повреждённый ключ localStorage.removeItem, чтобы не пытаться парсить его повторно на каждой загрузке. `frontend/src/store.ts`
- [ ] 🟡 **519.** (проблема) setTrimStartFromPlayer/setTrimEndFromPlayer используют фиксированный зазор 0.1 секунды не связанный с fps видео -> Вычислять минимальный зазор как 1/fps (если fps видео известен), с fallback на текущую константу 0.1. `frontend/src/store.ts`
- [ ] 🟡 **520.** (проблема) onImportTick и onExportTick — идентичные по структуре функции с разными префиксами полей state -> Обобщить через фабрику makeTickHandler(prefix) или общий helper, принимающий ref-пару progress/stage. `frontend/src/store.ts`
- [ ] 🟡 **522.** (проблема) Модуль экспортирует данные и функции плоским набором из ~35 именованных экспортов без фасада -> Сгруппировать экспорты в именованные объекты-фасады (editorActions, libraryActions, presetActions) либо разнести по отдельным файлам per предыдущий пункт SRP. `frontend/src/store.ts`
- [ ] 🟡 **523.** (проблема) toasts.ts не ограничивает количество одновременно показанных тостов -> Ограничить очередь (например, максимум 5 одновременно, старые вытеснять) в самой toast(). `frontend/src/toasts.ts`
- [ ] 🟡 **524.** (проблема) toast() не даёт возможности отменить свой setTimeout при ручном dismissToast -> Сохранять handle таймера и очищать его в dismissToast через clearTimeout при ручном закрытии. `frontend/src/toasts.ts`
- [ ] 🟡 **527.** (проблема) tierToCrf хранит таблицу CRF-значений как захардкоженный литерал внутри функции -> Вынести table в модульную константу верхнего уровня (или отдельный конфиг-файл), экспортируемую отдельно от функции поиска. `frontend/src/store.ts`
- [ ] 🟡 **530.** (проблема) Все toast-сообщения на русском захардкожены прямо в бизнес-логике store.ts -> Вынести строки в отдельный messages.ts/i18n-словарь и ссылаться на ключи вместо инлайновых литералов. `frontend/src/store.ts`
- [ ] 🟡 **533.** (баг) doUpload не проверяет пустой/некорректный файл и не защищён от двойного клика во время uploading -> Добавить проверку file && file.size > 0 с toast('error', ...) при пустом/битом файле перед вызовом api.uploadFile. `frontend/src/store.ts`
- [ ] 🟡 **534.** (баг) seekTo при отсутствующем видео не ограничивает t снизу после последующей загрузки -> Явно возвращать без действия (или клампить к 0), если state.video === null, вместо использования t как собственного потолка. `frontend/src/store.ts`
- [ ] 🟡 **535.** (проблема) Дублирование форматирования длительности/размера файла между компонентами дублирует проблему из store.ts -> Добавить formatDuration/formatSize рядом с parseTime в store.ts (или отдельном utils.ts) и переиспользовать в обоих компонентах. `frontend/src/components/VideoPreview.vue, frontend/src/components/MediaLibrary.vue`
- [ ] 🟡 **536.** (баг) toasts.ts не защищает nextId от коллизий между HMR-перезагрузками модуля в dev-режиме -> Использовать более устойчивый генератор id (например crypto.randomUUID()) вместо инкрементного module-level счётчика. `frontend/src/toasts.ts`
- [ ] 🟡 **538.** (проблема) pollJob использует единый жёстко закодированный интервал опроса без backoff, вне контроля store.ts -> Дать api.pollJob принимать опциональный интервал/backoff-стратегию и передавать её из store.ts в зависимости от типа операции (импорт vs экспорт). `frontend/src/store.ts, frontend/src/api.ts`
- [ ] 🟡 **539.** (проблема) buildEditPayload использует магический порог 0.05 секунды в четырёх местах без именованной константы -> Вынести в именованную константу EPSILON_SECONDS = 0.05 с комментарием об источнике значения. `frontend/src/store.ts`
- [ ] 🟡 **541.** (проблема) toast() не ограничивает длину text, длинные сообщения об ошибке рендерятся без обрезки -> Обрезать текст в toast() до разумной длины (например 200 символов) с многоточием, либо добавить CSS line-clamp в Toasts.vue. `frontend/src/toasts.ts, frontend/src/store.ts`
- [ ] 🟡 **544.** (баг) savePreset/deletePreset не синхронизируют presets.list между вкладками браузера -> Добавить storage-event listener, перечитывающий presets.list при изменении ключа ve_presets в другой вкладке. `frontend/src/store.ts`
- [ ] 🟡 **546.** (баг) restoreProject: promise более раннего вызова продолжает выполняться впустую при быстрой смене клипов -> Использовать AbortController на каждый вызов restoreProject и отменять предыдущий запрос при смене videoId. `frontend/src/store.ts`
- [ ] 🟡 **547.** (проблема) applyTheme одновременно мутирует реактивное состояние, DOM и localStorage в одной функции -> Разбить на setThemeState(t), applyThemeToDom(t) и persistTheme(t), вызываемые последовательно из toggleTheme/initTheme. `frontend/src/store.ts`
- [ ] 🟡 **548.** (проблема) EditPanel.vue дублирует ту же формулу клампа cut.start/cut.end, что и buildEditPayload в store.ts -> Вынести clampToRange(value, trimStart, trimEnd) в store.ts (или общий utils) и переиспользовать в EditPanel.vue и buildEditPayload. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **549.** (проблема) Нет экспортируемой функции сброса всего state/presets/ui для тестов -> Добавить экспортируемую resetStateForTests() в store.ts, переустанавливающую все поля state/presets/ui/history к дефолтам, и использовать её в beforeEach. `frontend/src/store.ts, frontend/src/store.test.ts`

### Frontend компоненты и интерактивность (45)

- [ ] 🔴 **550.** (проблема) EditPanel.vue — компонент на 725 строк объединяет пресеты, тайминг, кадр, цвет, звук и экспорт -> Разбить на подкомпоненты по секциям (TimingSection, FrameSection, ColorSection, AudioSection, ExportSection), EditPanel оставить оркестратором. `frontend/src/components/EditPanel.vue`
- [x] 🔴 **553.** (баг) RectOverlay не снимает window pointermove/pointerup при размонтировании во время активного драга -> Закрыто: `stopDrag` вызывается из `onUnmounted`, root/zero-size guarded. `frontend/src/components/RectOverlay.vue`
- [x] 🔴 **554.** (баг) TrimSlider не снимает window pointermove/pointerup при размонтировании во время активного драга -> Закрыто: единый `stopDrag` вызывается из `pointerup` и `onUnmounted`. `frontend/src/components/TrimSlider.vue`
- [ ] 🟠 **562.** (дизайн) Нет визуальной индикации, какой из двух RectOverlay (crop или censor) активен, если оба включены одновременно -> Добавить подпись рядом с каждым прямоугольником ('Кадрирование' / 'Замазка') или временно скрывать неактивный, пока не наведена мышь. `frontend/src/components/VideoPreview.vue`
- [ ] 🟠 **576.** (дизайн) MediaLibrary: кнопка удаления '✕' не запрашивает подтверждение -> Добавить confirm-диалог или двухшаговое подтверждение (например, требовать повторный клик в течение 3с). `frontend/src/components/MediaLibrary.vue`
- [ ] 🟠 **584.** (дизайн) VideoPreview использует нативные controls браузера, не синхронизированные визуально с TrimSlider -> Скрыть нативный seek или кастомизировать progress-бар плеера, синхронизировав его с trim-диапазоном визуально. `frontend/src/components/VideoPreview.vue`
- [ ] 🟠 **551.** (проблема) Массивы опций (formats, filters, aspects, speeds, rotations, censorColors, padAspects, widthPresets, fpsPresets, qualityTiers) захардкожены в EditPanel.vue -> Вынести в отдельный модуль (например edit-options.ts) с типами и экспортировать в EditPanel. `frontend/src/components/EditPanel.vue`
- [ ] 🟠 **552.** (проблема) Pointer-драг дублируется в RectOverlay и TrimSlider с разными правилами клампинга -> Вынести общий composable useDragHandle(window pointermove/up + cleanup) и переиспользовать в обоих компонентах. `frontend/src/components/RectOverlay.vue`
- [ ] 🟠 **556.** (проблема) Все компоненты редактора напрямую импортируют глобальный singleton state/actions из store.ts -> Ввести props/emit или provide/inject границу между презентационными компонентами и store, либо явно принять глобальный стор как осознанный архитектурный выбор для MVP. `frontend/src/components/EditPanel.vue`
- [ ] 🟠 **557.** (проблема) fmt()/fmtDuration() форматирование времени продублировано тремя разными реализациями -> Вынести единый formatTime(seconds, {withTenths?}) в общий utils-модуль и использовать во всех трёх компонентах. `frontend/src/components/EditPanel.vue`
- [ ] 🟠 **567.** (баг) widthPresets 'active' подсветка сравнивает только scale.w, игнорируя h -> Сравнивать также h или хранить id применённого preset отдельно от вычисляемых scale.w/h. `frontend/src/components/EditPanel.vue`
- [ ] 🟠 **577.** (проблема) ProgressBar.stage типизирован как голый string, а не union известных значений -> Типизировать stage как union 'queued'|'downloading'|'uploading'|'processing' и синхронизировать с типами в store.ts/types.ts. `frontend/src/components/ProgressBar.vue`
- [ ] 🟠 **585.** (баг) VideoPreview.onTimeUpdate зацикливает только по trimStart/trimEnd, не пропуская cut-диапазон -> При cutEnabled проверять попадание currentTime в cut.start..cut.end и перескакивать на cut.end, как это будет в финальном рендере. `frontend/src/components/VideoPreview.vue`
- [ ] 🟡 **555.** (дизайн) RectOverlay в режиме censor стартует с нулевым прямоугольником до срабатывания watcher в EditPanel -> Инициализировать censor сразу валидным прямоугольником в самом sеттере cropEnabled/censorEnabled, а не через отдельный watcher. `frontend/src/components/RectOverlay.vue`
- [ ] 🟡 **561.** (улучшение) RectOverlay.dims()/normalizeRect() пересчитываются на каждый computed и каждый onMove без мемоизации -> Кэшировать { W, H } в computed(() => ({W: state.video?.width||1, H: state.video?.height||1})) и переиспользовать. `frontend/src/components/RectOverlay.vue`
- [ ] 🟡 **564.** (дизайн) Инпуты времени startStr/endStr не валидируют вживую, только по blur/enter -> Показывать текстовую подсказку рядом с полем при startInvalid/endInvalid вместо одной только CSS-подсветки. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **568.** (дизайн) TrimSlider визуально неотличим для секций 'Обрезка' (границы клипа) и 'Вырезать кусок из середины' (cut) -> Добавить differentiating-стиль (например другой accent-цвет заливки) для cut-слайдера через prop variant. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **569.** (улучшение) watch(cutEnabled) и watch(censorEnabled) реализуют одинаковый паттерн 'seed default rect on enable' независимо друг от друга -> Вынести generic watchAndSeed(source, isValid, seedFn) helper и переиспользовать для обоих случаев. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **570.** (дизайн) UrlImport: текстовые поля importStart/importEnd не валидируются на лету -> Добавить локальную валидацию через parseTime() с подсветкой поля при вводе некорректного формата, аналогично startInvalid в EditPanel. `frontend/src/components/UrlImport.vue`
- [ ] 🟡 **571.** (дизайн) UrlImport: URL-инпут и dropzone-кнопка визуально равнозначны без индикатора выбранного источника -> Добавить радио-подобное переключение источника или блокировать URL-инпут после выбора файла и наоборот. `frontend/src/components/UrlImport.vue`
- [ ] 🟡 **580.** (дизайн) Toasts не имеют явной кнопки закрытия, только клик по всему телу -> Добавить явную кнопку закрытия и увеличить таймаут (или сделать его бесконечным) для kind='error'. `frontend/src/components/Toasts.vue`
- [ ] 🟡 **582.** (дизайн) Хоткеи Space/I/O/стрелки/,/. нигде в UI не задокументированы, кроме мелкой подписи в футере -> Добавить всплывающую справку по хоткеям (например по ? или иконке помощи) с полным списком. `frontend/src/App.vue`
- [ ] 🟡 **583.** (улучшение) App.vue.onKey — монолитная функция смешивает undo/redo, playback control и trim-seeking в одном switch -> Разбить на handleHistoryKeys/handlePlaybackKeys/handleSeekKeys и вызывать по очереди с early-return. `frontend/src/App.vue`
- [ ] 🟡 **586.** (дизайн) CSS-фильтры videoStyle — грубое приближение серверного ffmpeg-рендера без явной пометки 'превью примерное' -> Добавить отдельную пометку 'цветокоррекция в превью приблизительная' рядом с секцией эффектов в EditPanel. `frontend/src/components/VideoPreview.vue`
- [ ] 🟡 **589.** (улучшение) MediaLibrary.ext() не защищён от файлов без точки в имени -> Проверять filename.includes('.') перед split и возвращать '' при отсутствии расширения. `frontend/src/components/MediaLibrary.vue`
- [ ] 🟡 **590.** (дизайн) MediaLibrary не показывает, какой source-файл сейчас открыт в редакторе -> Добавить class active при e.id === state.video?.id и подсветить строку в списке. `frontend/src/components/MediaLibrary.vue`
- [ ] 🟡 **594.** (дизайн) ProgressBar.cancellable в UrlImport скрывает кнопку без объяснения, почему отмена недоступна -> Показывать disabled-кнопку с title-объяснением вместо полного скрытия элемента управления. `frontend/src/components/UrlImport.vue`
- [ ] 🟡 **558.** (проблема) fmtSize (форматирование байтов) дословно продублирован в VideoPreview.vue и MediaLibrary.vue -> Вынести fmtSize(bytes) в общий utils-модуль и импортировать в обоих компонентах. `frontend/src/components/VideoPreview.vue`
- [ ] 🟡 **559.** (баг) TrimSlider.onKey использует фиксированный шаг Shift=1с независимо от диапазона слайдера -> Сделать big = e.shiftKey ? (props.max - props.min) * 0.05 (или отдельный proп bigStep) вместо жёсткой единицы. `frontend/src/components/TrimSlider.vue`
- [ ] 🟡 **560.** (баг) TrimSlider.onTrackDown может выбрать не ту ручку при клике между близко расположенными хэndlами -> Учитывать реальную ширину .trim-handle в пикселях при выборе ближайшей ручки, а не только числовое значение. `frontend/src/components/TrimSlider.vue`
- [ ] 🟡 **563.** (баг) Минимальный зазор между trimStart/trimEnd в EditPanel.commitStart/commitEnd (0.1с) не совпадает с GAP в TrimSlider -> Вынести GAP в общий модуль-константу и импортировать в обоих местах вместо дублирования литерала 0.1. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **565.** (проблема) applyPlatform() смешивает доменную логику пресетов платформ с UI-компонентом EditPanel -> Вынести applyPlatform в store.ts или отдельный platform-presets.ts модуль, EditPanel вызывает готовую функцию. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **566.** (проблема) setAspect()/aspectActive() жёстко привязаны к массиву aspects, добавление нового соотношения требует правки функции и вызывающего кода -> Описать aspects как конфигурацию с fn-обработчиком внутри самого объекта, а не как отдельные параметры, разбираемые в компоненте. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **572.** (баг) UrlImport.onDrop не сбрасывает state.url при drop файла -> Очищать state.url в onDrop перед вызовом doUpload или показывать явное предупреждение о том, что URL игнорируется. `frontend/src/components/UrlImport.vue`
- [ ] 🟡 **573.** (баг) UrlImport dragover-подсветка мигает при пересечении дочерних элементов dropzone -> Считать глубину вложенности через dragenter/dragleave-counter или использовать pointer-events:none на детях во время dragover. `frontend/src/components/UrlImport.vue`
- [ ] 🟡 **574.** (баг) ResultPanel.downloadName не убирает управляющие символы и переносы строк из video.title -> Расширить regex до /[\\/:*?"<>|\x00-\x1f]+/g или использовать общий sanitizeFilename-хелпер. `frontend/src/components/ResultPanel.vue`
- [ ] 🟡 **575.** (проблема) downloadName в ResultPanel дублирует regex-санитайзинг имени файла, не вынесенный в общий модуль -> Вынести sanitizeFilename(name, ext) в utils-модуль. `frontend/src/components/ResultPanel.vue`
- [ ] 🟡 **578.** (баг) ProgressBar.progress-fill всегда показывает минимум 2% ширины даже при progress=0 -> Показывать 0% как реальный 0, а минимальную видимую полоску применять только к indeterminate-состоянию. `frontend/src/components/ProgressBar.vue`
- [ ] 🟡 **579.** (проблема) ProgressBar.progress-fill inline-style дублирует условие 'known' вместо единого computed -> Вынести fillWidth в computed(() => known.value ? Math.max(2, props.progress!) + '%' : undefined). `frontend/src/components/ProgressBar.vue`
- [ ] 🟡 **581.** (баг) App.vue.onKey не проверяет state.exporting/state.importing перед обработкой хоткеев -> Добавить ранний return при state.exporting (или ограничить только undo/redo/seek, не трогающие edit во время экспорта). `frontend/src/App.vue`
- [ ] 🟡 **587.** (баг) denoise/sharpen/grain/vignette эффекты никак не отражены в живом превью -> Добавить хотя бы грубые CSS-аналоги (box-shadow inset для vignette, SVG turbulence overlay для grain) или явно пометить эти контролы как 'без предпросмотра'. `frontend/src/components/VideoPreview.vue`
- [ ] 🟡 **588.** (проблема) RectOverlay жёстко использует var(--accent)/var(--danger) как дефолтные цвета через props default -> Требовать явную передачу color от родителя без дефолта, завязанного на тему, либо вынести цвета в отдельный theme-config, импортируемый в местах использования. `frontend/src/components/RectOverlay.vue`
- [ ] 🟡 **591.** (баг) EditPanel.onSavePreset не блокирует Enter в пустом поле имени пресета -> Добавить ту же проверку presetName.trim() перед вызовом onSavePreset по Enter, как у кнопки. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **592.** (баг) applyPlatform для shorts/reels может оставить crop несогласованным с уже включённым censor -> После смены aspect в applyPlatform дополнительно кламповать censor через sanitizeRect с новыми границами. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **593.** (баг) parseTime не ограничивает число сегментов, разделённых двоеточием -> Ограничить parts.length <= 3 (hh:mm:ss) и возвращать null при превышении. `frontend/src/store.ts`

### Визуальный дизайн и UX (взгляд дизайнера) (55)

- [ ] 🟠 **595.** (дизайн) Пустое состояние без загруженного видео обрывается пустым фоном без сообщения -> Добавить `v-else` блок с иллюстрацией/текстом-подсказкой 'Импортируй видео, чтобы начать'. `frontend/src/App.vue`
- [ ] 🟠 **602.** (дизайн) Акцентный цвет применяется только к кнопке импорта и активным чипам, остальной интерфейс монотонно тёмно-серый -> Добавить лёгкий акцентный левый бордер или фоновую подсветку для активных/заполненных секций EditPanel. `frontend/src/style.css`
- [ ] 🟠 **614.** (дизайн) На узких вьюпортах правая колонка EditPanel остаётся очень длинной без якорей/табов -> На мобильной ширине превратить `.group-title` в кликабельные аккордеон-заголовки с обычным `<details>`/toggle. `frontend/src/style.css`
- [ ] 🟠 **616.** (дизайн) Кнопка 'Отмена' в ProgressBar визуально идентична обычным ghost-кнопкам действий -> Добавить модификатор вроде `.btn.ghost.sm.warn` с цветом `--danger` для кнопки отмены прогресса. `frontend/src/components/ProgressBar.vue`
- [ ] 🟠 **622.** (дизайн) Кнопка 'Экспортировать' не имеет визуальной иерархии относительно 7 секций формы выше -> Добавить видимый разделитель (padding-top + border-top акцентного цвета) или sticky-футер для кнопки экспорта. `frontend/src/components/EditPanel.vue`
- [ ] 🟠 **626.** (дизайн) Медиатека не показывает пустое состояние при первом визите -> Показывать `.card.library` всегда с сообщением-заглушкой, когда `state.library.length === 0`. `frontend/src/components/MediaLibrary.vue`
- [ ] 🟠 **597.** (баг) Колонки редактора .left/.right не имеют CSS-правил, порядок держится только на DOM -> Добавить явные grid-column/flex правила для `.left`/`.right` или удалить неиспользуемые классы, положившись на порядок в `.editor`. `frontend/src/style.css`
- [ ] 🟠 **606.** (проблема) Форматирование длительности и размера дублируется в MediaLibrary.vue, VideoPreview.vue и EditPanel.vue -> Вынести fmtDuration/fmtSize/fmt в общий `frontend/src/format.ts` и импортировать во всех трёх компонентах. `frontend/src/components/MediaLibrary.vue`
- [ ] 🟠 **608.** (проблема) Chip-группы визуально идентичны несмотря на разную семантику выбора -> Добавить отдельный визуальный стиль (например, с иконкой-стрелкой) для action-чипов вроде 'Под платформу', отличный от persistent-toggle чипов. `frontend/src/components/EditPanel.vue`
- [ ] 🟠 **624.** (проблема) Числовые поля кропа X/Y/Ширина/Высота не имеют верхней границы max относительно размеров видео -> Добавить `:max="state.video?.width"`/`:max="state.video?.height"` на соответствующие поля кропа. `frontend/src/components/EditPanel.vue`
- [ ] 🟠 **632.** (баг) У censor-прямоугольника нет числовых полей X/Y/W/H в отличие от кропа -> Добавить аналогичный `.grid2` с X/Y/Ширина/Высота, привязанный к `state.edit.censor`, как у кропа. `frontend/src/components/EditPanel.vue`
- [ ] 🟠 **634.** (проблема) Ни одна из chip-групп не имеет role=radiogroup и aria-pressed/aria-checked на активном чипе -> Добавить `:aria-pressed="activeCondition"` на каждую `.chip`-кнопку и `role="group"` с `aria-label` на обёртку `.chips`. `frontend/src/components/EditPanel.vue`
- [ ] 🟠 **639.** (баг) crop.w/crop.h можно обнулить через number input, normalizeCrop вызывается только по blur -> Добавить debounce-валидацию на `@input`, а не только на `@blur`, либо clamp значение сразу в RectOverlay через normalizeRect (частично уже есть, но crop.w=0 может дать деление на 0 в rectStyle до нормализации). `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **596.** (дизайн) Карточка импорта не сворачивается после загрузки клипа -> Сворачивать `.card.import` в компактный collapsed-заголовок, когда `state.video` уже установлен. `frontend/src/components/UrlImport.vue`
- [ ] 🟡 **598.** (дизайн) Двухколоночный редактор не sticky, левая колонка обрывается при скролле длинной формы -> Добавить `position: sticky; top: 16px` для `.left` в пределах `.editor`. `frontend/src/style.css`
- [ ] 🟡 **599.** (дизайн) Нативные чекбоксы визуально не согласованы с pill-кнопками для однотипных toggle-паттернов -> Свести все boolean-переключатели к единому компоненту (кастомный switch или chip), убрав нативные чекбоксы из `.toggle`. `frontend/src/style.css`
- [ ] 🟡 **600.** (дизайн) Плоская типографическая шкала между заголовком, лейблами и подсказками -> Добавить `font-weight: 600` для `.card h2` и `.group-title`, оставив 400 для обычного текста полей. `frontend/src/style.css`
- [ ] 🟡 **601.** (дизайн) Подзаголовок и футер почти неразличимы по контрасту относительно заголовка -> Дать `.foot` более приглушённый `--faint` вместо `--muted`, чтобы визуально развести вступление и подвал. `frontend/src/style.css`
- [ ] 🟡 **603.** (дизайн) Disabled-состояние текстовых полей визуально не обозначено -> Добавить `.url-input:disabled, .time-input:disabled { opacity: 0.55; cursor: not-allowed; }`. `frontend/src/style.css`
- [ ] 🟡 **605.** (дизайн) Индикатор невалидности time-input не имеет aria-атрибутов, только цвет рамки -> Добавить `:aria-invalid="startInvalid"` на оба time-input в EditPanel.vue. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **607.** (дизайн) Плотность полей EditPanel одинаковая для простых toggle и составных контролов -> Дать составным полям (с вложенными chips+grid2) увеличенный нижний отступ или разделитель, отличный от простых toggle-полей. `frontend/src/style.css`
- [ ] 🟡 **609.** (дизайн) Значок медиатеки .lib-badge для источника не имеет цветового отличия от результата -> Добавить `.lib-badge.source { color: var(--accent); border-color: var(--accent); }` для симметрии с output. `frontend/src/style.css`
- [ ] 🟡 **610.** (дизайн) Группировка 'Появление/Затухание' и цветовых слайдеров в grid2 не показывает единицы визуально -> Обернуть числовое значение в `<span class="value">` с tabular-nums и чуть большим весом шрифта. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **611.** (дизайн) Заголовок с эмодзи не имеет aria-hidden, попадает в озвучку скринридера -> Обернуть эмодзи в `<span aria-hidden="true">🎬</span>` отдельно от текста заголовка. `frontend/src/App.vue`
- [ ] 🟡 **612.** (дизайн) Кнопка темы использует эмодзи как визуальный индикатор состояния, дублирующий текст -> Оставить один канал: либо только текст, либо иконку без дублирующего слова, но не оба одновременно. `frontend/src/App.vue`
- [ ] 🟡 **613.** (дизайн) Ручки crop/censor overlay имеют фиксированный радиус 14px независимо от размера видео-превью -> Задать размер ручек в `clamp()` относительно ширины `.player-wrap`, а не фиксированным px. `frontend/src/style.css`
- [ ] 🟡 **615.** (дизайн) progress-fill.indeterminate использует !important для ширины -> Убрать `!important`, так как ProgressBar.vue уже не выставляет `:style` в indeterminate-режиме (строка 40). `frontend/src/style.css`
- [ ] 🟡 **617.** (дизайн) Тосты позиционируются fixed поверх контента без учёта видео-превью снизу справа -> Сдвинуть `.toasts` в левый нижний угол или под видео-плеер, подальше от основных кнопок действия. `frontend/src/style.css`
- [ ] 🟡 **618.** (дизайн) grid2 в блоке кадрирования не выравнивает подписи по базовой линии с range-полями цвета -> Задать единую высоту строки лейбла (`line-height`/`min-height`) в `.grid2 label`, независимо от типа вложенного input. `frontend/src/style.css`
- [ ] 🟡 **619.** (дизайн) Кнопки 'Сброс кадрирования' и 'Сбросить цвет' стилизованы по-разному -> Привести оба reset-действия к единому компоненту (например, всегда `.btn.ghost.sm`), расположенному по единому правилу (например, справа от заголовка группы). `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **620.** (дизайн) Подсказка про реверс — обычный .hint, а не предупреждение, хотя описывает риск памяти -> Использовать `--warn` цвет или отдельный класс `.hint.warn` для этой строки вместо обычного `.hint`. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **621.** (дизайн) Подсказка для GIF рекомендует указать размер и отрезок, но рядом нет перехода к этим контролам -> Добавить ссылку-кнопку в hint, которая скроллит/фокусирует на поля обрезки времени и изменения размера. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **623.** (дизайн) Активный чип 'Поворот 0°' неотличим от невыбранной чипы других групп при беглом сканировании -> Не подсвечивать чип, соответствующий дефолтному/неизменённому значению, как активный, либо выделять изменённые от дефолта группы отдельно. `frontend/src/style.css`
- [ ] 🟡 **625.** (дизайн) Единственная шкала border-radius не документирует, какой компонент какой уровень использует -> Добавить CSS-комментарий у объявления переменных с правилом: card=lg, поля/кнопки=radius, мелкие бейджи/пилюли=sm или 999px. `frontend/src/style.css`
- [ ] 🟡 **627.** (дизайн) Drag-over состояние применяется ко всей карточке импорта, включая уже заполненные поля -> Ограничить визуальную реакцию на dragover только зоной `.dropzone`, не подсвечивая всю карточку целиком. `frontend/src/style.css`
- [ ] 🟡 **628.** (дизайн) Кнопка 'Скачать' в медиатеке — это <a>, а не <button>, визуально неотличима от btn ghost sm кнопок рядом -> Ничего не менять визуально, но задокументировать в компоненте, что это осознанный выбор `<a download>`, либо унифицировать через общий `.btn` wrapper-компонент, скрывающий разницу тега. `frontend/src/components/MediaLibrary.vue`
- [ ] 🟡 **629.** (дизайн) meta-chip и lib-badge используют одинаковый pill-паттерн для разных по смыслу данных -> Дать `.lib-badge` прямоугольную форму (`--radius-sm`) вместо pill, чтобы визуально отделить категорию от метаданных. `frontend/src/style.css`
- [ ] 🟡 **630.** (дизайн) Indeterminate-прогресс не сообщает реальный этап явно, если progress долго равен null -> После N секунд в indeterminate-режиме показывать дополнительный текст вроде 'это может занять некоторое время' в progress-label. `frontend/src/components/ProgressBar.vue`
- [ ] 🟡 **631.** (дизайн) topbar-row размещает h1 и переключатель темы в одну строку без обработки переноса на узких вьюпортах -> Добавить `flex-wrap: wrap` для `.topbar-row` на мобильной ширине. `frontend/src/style.css`
- [ ] 🟡 **633.** (дизайн) Чипы 'Под платформу' визуально идентичны persistent-toggle чипам, хотя это одноразовые действия -> Стилизовать action-чипы как `.btn.ghost.sm` вместо `.chip`, чтобы визуально отличать 'применить пресет' от 'выбрать значение'. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **636.** (дизайн) В style.css задан ровно один font-weight на весь файл -> См. фикс про типографическую шкалу выше — добавить 2-3 уровня font-weight (400/500/600) для разных ролей текста. `frontend/src/style.css`
- [ ] 🟡 **637.** (дизайн) Кнопка 'Сбросить цвет' занимает 4-ю ячейку grid2 рядом с тремя слайдерами -> Вынести кнопку сброса цвета из `.grid2` в отдельную строку над или под сеткой слайдеров. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **638.** (дизайн) Подписи 'Под платформу' и 'Поля под пропорции (letterbox)' разной длины нарушают вертикальный ритм списка полей -> Ограничить длину лейблов одной строкой везде или вынести пояснение '(letterbox)' в отдельный `.hint` под чипами. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **640.** (дизайн) meta-chip растянут без ограничения ширины — длинные codec-строки переполняют ряд -> Добавить `max-width` и `text-overflow: ellipsis` с `title`-атрибутом на `.meta-chip` для длинных значений codec. `frontend/src/components/VideoPreview.vue`
- [ ] 🟡 **641.** (дизайн) Кнопки истории и primary-кнопка экспорта используют одинаковый .btn паттерн без учёта частоты использования -> Оставить как есть по сути, но явно закрепить hotkey-подсказку (уже есть title у истории) и рассмотреть увеличение hit-area кнопок истории, раз они используются чаще. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **643.** (дизайн) trim-times использует flex-wrap с justify-content space-between, но pos-btns центрируется, ломая порядок на средних ширинах -> Задать явный `order` или обернуть `.pos-btns` в отдельный ряд с `flex-basis: 100%` при переносе. `frontend/src/style.css`
- [ ] 🟡 **644.** (дизайн) lib-badge и .chip.active оба красят через border-color/background два разных паттерна 'активного' состояния -> Привести `.lib-badge.output` к той же заливке, что `.chip.active`, либо явно задокументировать разницу как 'статус' vs 'выбор'. `frontend/src/style.css`
- [ ] 🟡 **646.** (дизайн) Чипы поворота и FPS используют одинаковую ширину при разной длине текста, что даёт неровный ритм внутри группы -> Задать `.chips .chip { min-width: 56px }` для числовых/коротких групп, чтобы выровнять ширину визуально близких по смыслу чипов. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **647.** (дизайн) h1 использует letter-spacing, а card h2 и group-title — нет, несогласованный трекинг заголовков -> Свести к осознанной шкале: например, отрицательный tracking только для крупных заголовков (h1/h2), положительный — только для uppercase-лейблов (group-title), без промежуточных исключений. `frontend/src/style.css`
- [ ] 🟡 **649.** (дизайн) Поле имени пресета использует class time-input, смешивая семантику 'ввод времени' и 'произвольный текст' -> Ввести отдельный класс `.text-input` для полей произвольного текста и использовать его для имени пресета, оставив `.time-input` только для времени. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **604.** (баг) Невалидные time-input поля не имеют focus-состояния, отличного от error-состояния -> Добавить `.time-input.invalid:focus { border-color: var(--danger); box-shadow: 0 0 0 3px rgba(255,107,107,0.25); }`. `frontend/src/style.css`
- [ ] 🟡 **635.** (баг) Нет :disabled стилей для .url-input и .time-input во время импорта -> См. фикс выше: добавить общее правило `:disabled { opacity: 0.55; cursor: not-allowed; }` для обоих классов полей. `frontend/src/style.css`
- [ ] 🟡 **642.** (проблема) watch на cutEnabled и censorEnabled дублируют паттерн 'seed default rect on enable' без общей абстракции -> Вынести общую функцию `seedRectIfEmpty(enabledRef, rectRef, seedFn)` в store и переиспользовать для cut/censor. `frontend/src/components/EditPanel.vue`
- [ ] 🟡 **645.** (баг) Минимальная ширина 2% индикатора прогресса визуально неотличима от близких к нулю значений на треке высотой 8px -> Поднять минимальный порог до 4-5% либо добавить текстовый процент рядом, который уже показывается через progress-label, чтобы не полагаться только на визуальную полоску. `frontend/src/components/ProgressBar.vue`
- [ ] 🟡 **648.** (проблема) aspectActive использует epsilon-сравнение 0.02, которое может ложно подсвечивать чип 4:3 при похожем кастомном кропе -> Сузить epsilon до значения, дающего false positive только при реально идентичном соотношении (например, 0.005), или сравнивать по нормализованному отношению с учётом округления до чётных пикселей. `frontend/src/components/EditPanel.vue`

### Тесты и quality gates (40)

- [ ] 🔴 **650.** (проблема) Ни один тест не бьёт по гонке двух одновременных идентичных edit-запросов -> Добавить тест, который параллельно (tokio::join!) отправляет два идентичных /api/edit и проверяет единственный результат рендера и общий cache-хит. `backend/tests/api.rs`
- [ ] 🔴 **652.** (проблема) pollJob и весь api.ts не покрыты ни одним тестом -> Добавить frontend/src/api.test.ts с fetch-моками (msw или vi.stubGlobal('fetch', ...)), покрывающий pollJob (включая обработку ошибок и терминальных статусов) и остальные экспортируемые функции. `frontend/src/api.ts`
- [ ] 🟠 **651.** (проблема) cancel_queued_edit_does_not_wait_for_permit не проверяет отсутствие орфанного ffmpeg-процесса -> Добавить тест с реальным ffmpeg (аналогично render.rs), который отменяет job после старта процесса и проверяет через ps/pgrep, что процесс действительно завершён. `backend/tests/api.rs`
- [ ] 🟠 **654.** (проблема) normalizeCrop и sanitizeRect не тестируются с video=null или width/height=0 -> Добавить тест normalizeCrop() при state.video = null и при width/height = 0, проверяющий, что функция не падает и не производит NaN/Infinity в state.edit.crop. `frontend/src/store.test.ts`
- [ ] 🟠 **655.** (проблема) Нет теста на doImport/doUpload/doExport целиком, только их составные части -> Добавить тесты doImport/doUpload/doExport с замоканными api.importUrl/uploadFile/edit/pollJob, проверяющие обновление state.video, state.jobs и обработку ошибок. `frontend/src/store.ts`
- [ ] 🟠 **656.** (проблема) restoreProject не тестируется на смену videoId во время in-flight запроса -> Добавить тест: вызвать openFromLibrary для видео A, затем до resolve промиса переключиться на видео B, и проверить, что применённый результат A не перезаписывает состояние B. `frontend/src/store.ts`
- [ ] 🟠 **657.** (проблема) Нет regression-корпуса реальных ffmpeg-запусков за пределами трёх сценариев -> Добавить минимум один тест на build_concat_args с segments и один на одновременные crop+censor, оба через реальный ffmpeg. `backend/tests/render.rs`
- [ ] 🟠 **658.** (проблема) Реальные ffmpeg-тесты не проверяют ошибочный путь -> Добавить тест, который подсовывает run_ffmpeg заведомо некорректные args (или повреждённый source) и проверяет Done::Failed вместо паники или зависания. `backend/tests/render.rs`
- [ ] 🟠 **659.** (проблема) JOB_TIMEOUT_SECS нигде не тестируется на реальное срабатывание -> Добавить render.rs-тест, который выставляет JOB_TIMEOUT_SECS в малое значение через env и запускает заведомо долгий ffmpeg-рендер, проверяя переход job в error/timeout. `backend/src/handlers/mod.rs`
- [ ] 🟠 **661.** (проблема) Нет теста на гонку cancel_open_job и finish_job в момент завершения worker'а -> Добавить тест с tokio::join! на cancel_open_job и finish_job над одной job, запущенный многократно (loom или stress-repeat), чтобы отловить неатомарность. `backend/src/handlers/mod.rs`
- [ ] 🟠 **671.** (проблема) Frontend CI не запускает никаких компонентных тестов — их просто нет -> Добавить @vue/test-utils и минимум smoke-тест на монтирование App.vue, ловящий явные runtime-ошибки рендера. `frontend/src/store.test.ts`
- [ ] 🟠 **675.** (проблема) files_route_serves_only_media_subdirectories не проверяет path traversal через ../ -> Добавить тест GET /files/sources/../app.db (и URL-encoded вариант) и проверить, что ответ не 200 и не содержит содержимого БД. `backend/tests/api.rs`
- [ ] 🟠 **678.** (проблема) Нет теста buildEditPayload на censor с уменьшенным crop -> Добавить тест: включить cropEnabled с ненулевым x/y, задать censor и проверить, что buildEditPayload().censor либо пересчитан относительно crop, либо документированно передаётся в абсолютных координатах и backend это ожидает. `frontend/src/store.test.ts`
- [ ] 🟡 **679.** (улучшение) poll_terminal использует busy-poll с sleep(10ms)×400 итераций -> Заменить busy-poll на watch-канал/уведомление о завершении job в AppState, которое тест может ожидать напрямую без фиксированного интервала опроса. `backend/tests/api.rs`
- [ ] 🟡 **653.** (баг) Тест 'flushes a pending edit before redo' не проверяет ветку flushPendingHistory без pending-таймера -> Добавить кейс, где redo()/undo() вызывается сразу после предыдущего commit без новых изменений, и проверить, что history не меняется лишний раз. `frontend/src/store.test.ts`
- [ ] 🟡 **660.** (проблема) Нет теста на восстановление после падения посреди рендера с недописанным output-файлом -> Добавить в jobs_survive_restart создание недописанного *.part или .mp4 файла в outputs/ перед recover_jobs и проверить, что он не отдаётся как валидный результат. `backend/tests/api.rs`
- [ ] 🟡 **662.** (проблема) Для `EditPlan::compile`/source geometry есть regression tests, но нет property/fuzz-корпуса. -> Добавить proptest для duration/rect/scale и проверять containment, finite identity и panic freedom. `backend/src/services/render.rs`
- [x] 🟡 **663.** (проблема) SSRF-тесты не покрывали DNS rebinding между URL validation и connect -> Закрыто в раунде 6: injected resolver меняет public answer на private, тест доказывает block до TCP connect. `backend/src/tools/egress_proxy.rs`
- [ ] 🟡 **664.** (проблема) make_state дублируется почти дословно между api.rs и handlers/mod.rs::tests::state() -> Вынести общую фабрику AppState для тестов в отдельный test-util модуль (например backend/src/test_support.rs с #[cfg(test)]) и переиспользовать из обоих мест. `backend/tests/api.rs`
- [ ] 🟡 **665.** (проблема) Нет теста на конкурентный upload двух файлов с коллизией video_id -> Добавить тест, отправляющий два параллельных multipart upload и проверяющий, что оба файла сохраняются под разными id без порчи данных. `backend/tests/api.rs`
- [ ] 🟡 **666.** (проблема) upload-тесты не проверяют путь ошибки probe_video на реальном файле -> Добавить тест с реальным ffmpeg (в render.rs или отдельном ffmpeg-gated тесте), который заливает файл с мусорным содержимым и проверяет 400/ошибку от probe_video, а не панику. `backend/tests/api.rs`
- [x] 🟡 **667.** (проблема) client extension policy не была покрыта прямым или HTTP-level тестом -> Закрыто в раунде 5: `safe_upload_extension` unit-тестирован, HTTP regression проверяет MP4 с именем `payload.html`, MIME и protective headers. `backend/src/handlers/upload.rs`, `backend/tests/api.rs`
- [ ] 🟡 **668.** (проблема) cache_key коллизии между разными videoId с одинаковым edit не тестируются с реальным разделением файлов -> Добавить тест: закэшировать результат для videoId='vidX', затем отправить идентичный edit с videoId='vidY' и проверить, что кэш не срабатывает (реальный рендер или явная ошибка отсутствия источника). `backend/tests/api.rs`
- [ ] 🟡 **669.** (проблема) CI не запускает Makefile check, дублируя его шаги вручную вместо переиспользования -> Заменить пошаговые run-команды в ci.yml на вызов `make check` (или отдельных `make lint`/`make test`), чтобы Makefile оставался единственным источником истины. `.github/workflows/ci.yml`
- [ ] 🟡 **670.** (проблема) CI не публикует и не проверяет покрытие тестами ни для backend, ни для frontend -> Добавить cargo-llvm-cov для backend и vitest --coverage для frontend с минимальным порогом, публикуемым как job summary или артефакт. `.github/workflows/ci.yml`
- [ ] 🟡 **672.** (проблема) Нет теста project_upsert_rejects_oversized_json_fields для поля edit -> Добавить тест с huge-строкой внутри edit вместо video и проверить тот же PAYLOAD_TOO_LARGE с текстом 'edit' в сообщении об ошибке. `backend/tests/api.rs`
- [ ] 🟡 **673.** (проблема) Нет теста на некорректный EditRequest на HTTP-уровне через /api/edit -> Добавить тест POST /api/edit с телом невалидного JSON и с полем неверного типа, проверяющий явный 400 Bad Request, а не паническую 500. `backend/tests/api.rs`
- [ ] 🟡 **674.** (проблема) health_reflects_tool_availability не проверяет сериализацию ffmpegVersion/ytdlpVersion -> Добавить сценарий make_state с заполненными ffmpeg_version/ytdlp_version и проверить, что они появляются в JSON-ответе под ожидаемыми ключами. `backend/tests/api.rs`
- [ ] 🟡 **676.** (проблема) Нет теста двойной параллельной отмены одной и той же задачи -> Добавить тест, вызывающий cancel_open_job дважды параллельно (tokio::join!) на одной job и проверяющий, что ровно один вызов вернул Cancelled, а другой — AlreadyFinished. `backend/src/state.rs`
- [ ] 🟡 **677.** (проблема) edit_cache_hit_returns_existing_output не проверяет отсутствие повторного добавления в library при cache-хите -> Добавить в тест проверку library.list().len() до и после запроса — должно остаться 1 запись с id='cached', а не появиться вторая. `backend/tests/api.rs`
- [ ] 🟡 **680.** (проблема) Нет теста library_delete на симметричный случай удаления source-записи -> Добавить тест: создать source-запись и кэш-запись с ссылкой на неё, удалить source через DELETE /api/library/{id} и проверить, зависит ли инвалидация кэша от source, а не только от output. `backend/tests/api.rs`
- [ ] 🟡 **681.** (проблема) Нет теста store.ts на seekTo/seekRelative/togglePlay/setTrimStartFromPlayer/setTrimEndFromPlayer -> Добавить тесты с моком HTMLVideoElement (или state.playerEl-эквивалентом), проверяющие корректность seekTo/seekRelative границ и обновление trimStart/trimEnd от текущей позиции плеера. `frontend/src/store.ts`
- [ ] 🟡 **682.** (проблема) render_locks в AppState растёт без ограничений и не покрыт тестом на очистку -> Добавить периодическую очистку render_locks для ключей без активных Mutex-владельцев (Arc::strong_count == 1) в spawn_cleanup и тест, подтверждающий, что размер карты не растёт бесконечно. `backend/src/state.rs`
- [ ] 🟡 **683.** (проблема) spawn_cleanup (TTL-очистка sources/outputs) не покрыт ни одним тестом -> Вынести тело spawn_cleanup в тестируемую async-функцию, принимающую TTL и текущее время как параметры, и добавить тест с искусственно состаренными файлами. `backend/src/main.rs`
- [ ] 🟡 **684.** (проблема) PRESET_KEYS в store.ts не синхронизирован с EditState и не покрыт regression-тестом -> Добавить тест, который через Object.keys(defaultEdit()) сверяет, что каждое поле EditState либо явно есть в PRESET_KEYS, либо явно в списке геометрических/clip-specific исключений — чтобы забытое поле проваливало тест, а не тихо терялось. `frontend/src/store.ts`
- [ ] 🟡 **685.** (проблема) Контракт округления crop между frontend sanitizeRect и backend clamp_rect_to_source не тестируется совместно -> Добавить cross-стековый integration-тест (или задокументированный контрактный тест), который прогоняет одно и то же значение crop через buildEditPayload(), затем через clamp_rect_to_source и проверяет идемпотентность. `frontend/src/store.test.ts`
- [ ] 🟡 **686.** (проблема) pollJob не имеет верхнего предела попыток/времени, и это не тестируется -> Добавить frontend/src/api.test.ts тест, который мокает getJob всегда возвращающим running, и проверяет либо явный таймаут/лимит попыток в pollJob, либо документирует его отсутствие как осознанное поведение. `frontend/src/api.ts`
- [ ] 🟡 **687.** (проблема) Нет теста на повторный finish_job/library.add при restart с уже существующей library-записью -> Добавить в jobs_survive_restart шаг, вызывающий finish_job второй раз для уже терминальной 'done1' job после recover_jobs, и проверить отсутствие дублирующей library-записи. `backend/tests/api.rs`
- [ ] 🟡 **688.** (проблема) cleanup_files_with_prefix не тестируется на префикс, совпадающий с UUID-подстрокой другого видео -> Добавить тест с парой реалистичных UUID-подобных id, где один является строгим префиксом другого, и проверить, что cleanup для короткого id не удаляет файлы длинного. `backend/src/handlers/mod.rs`
- [ ] 🟡 **689.** (проблема) make_state в api.rs строит ToolInfo вручную вместо переиспользования ToolInfo::default -> Заменить литерал в make_state на ToolInfo { ffmpeg, ytdlp, ..Default::default() }, чтобы новые поля ToolInfo подхватывались автоматически. `backend/tests/api.rs`

### Config, build, Docker, CI, observability (46)

- [ ] 🔴 **691.** (проблема) Оба Docker-образа запускают процесс от root без USER-директивы -> Добавить RUN useradd + USER в обоих Dockerfile перед CMD/финальным слоем. `backend/Dockerfile`
- [ ] 🟠 **690.** (проблема) Конфигурация читается россыпью из env в 5+ файлах вместо единой точки загрузки -> Ввести единую структуру Config::from_env(), читаемую один раз в main.rs и передаваемую по цепочке вместо точечных std::env::var в модулях. `backend/src/main.rs`
- [ ] 🟠 **692.** (проблема) Ни один Dockerfile не объявляет HEALTHCHECK -> Добавить HEALTHCHECK CMD curl -f http://localhost:8080/api/health в backend/Dockerfile и аналогичный для nginx в frontend/Dockerfile. `backend/Dockerfile`
- [ ] 🟠 **693.** (проблема) Базовые образы и Rust-тулчейн не запиннены — версии плавают независимо друг от друга -> Запиннить базовые образы по sha256-digest и зафиксировать конкретную версию Rust (например rust:1.80-bookworm) одинаково в Dockerfile и CI. `backend/Dockerfile`
- [ ] 🟠 **700.** (баг) BIND_ADDR парсится с молчаливым фоллбэком на 127.0.0.1 при невалидном значении -> Заменить unwrap_or_else на явный panic!/expect с сообщением о некорректном BIND_ADDR при ошибке парсинга. `backend/src/main.rs`
- [ ] 🟠 **702.** (проблема) MAX_CONCURRENT_JOBS, FILE_TTL_HOURS, MAX_UPLOAD_BYTES читаются через .ok().and_then(parse) без проверки на 0 или неразумные значения -> Добавить валидацию с понятным log::error!/panic при значениях вне разумного диапазона (например MAX_CONCURRENT_JOBS < 1). `backend/src/main.rs`
- [ ] 🟠 **706.** (проблема) /api/health не проверяет реальное состояние SQLite или диска, только статичные флаги ffmpeg/ytdlp -> Добавить в health_handler лёгкий SELECT 1 к Db и проверку доступности STORAGE_DIR на запись, включив их в ответ. `backend/src/http/mod.rs`
- [ ] 🟠 **707.** (баг) Доступность ffmpeg/yt-dlp проверяется один раз при старте и никогда не переоценивается -> Переоценивать доступность инструментов периодически (например раз в 5 минут) или прямо в health_handler с троттлингом. `backend/src/main.rs`
- [ ] 🟠 **708.** (проблема) docker-compose.yml не задаёт ресурсные лимиты (memory/cpu) ни для backend, ни для frontend -> Добавить mem_limit/cpus (или deploy.resources.limits в compose v3) для обоих сервисов. `docker-compose.yml`
- [ ] 🟠 **709.** (проблема) docker-compose не пробрасывает MAX_CONCURRENT_JOBS/JOB_TIMEOUT_SECS/MAX_UPLOAD_BYTES/CORS_ALLOW_ORIGINS — все лимиты жёстко на дефолтах контейнера -> Добавить эти переменные в environment (или через env_file) с возможностью переопределения через .env при деплое. `docker-compose.yml`
- [ ] 🟠 **710.** (проблема) CORS_ALLOW_ORIGINS не задан в docker-compose — дефолтные origin'ы для localhost не подходят за реальным доменом -> Задать CORS_ALLOW_ORIGINS в docker-compose.yml (или через .env) под реальный домен деплоя. `docker-compose.yml`
- [ ] 🟠 **724.** (проблема) Путь скачивания через yt-dlp не покрыт ни одним тестом, и CI даже не устанавливает yt-dlp -> Либо установить yt-dlp в CI и добавить интеграционный тест с фиктивным/локальным медиа-источником, либо явно задокументировать это ограничение покрытия. `.github/workflows/ci.yml`
- [ ] 🟡 **697.** (идея) В CI нет сканирования зависимостей на известные уязвимости -> Добавить шаг cargo audit в backend job и npm audit --audit-level=high в frontend job. `.github/workflows/ci.yml`
- [ ] 🟡 **718.** (идея) CI не публикует и не кэширует собранные Docker-образы для последующего деплоя -> Добавить отдельный job с docker/build-push-action, публикующий образ в GHCR при пуше в main. `.github/workflows/ci.yml`
- [ ] 🟡 **703.** (улучшение) Логи пишутся в текстовом формате без структурированного JSON-вывода -> Добавить опциональный JSON-форматтер (tracing_subscriber::fmt().json()), включаемый через переменную окружения LOG_FORMAT=json. `backend/src/main.rs`
- [ ] 🟡 **713.** (улучшение) nginx.conf слушает только IPv4 (listen 80) без явного IPv6-листенера -> Добавить listen [::]:80; рядом с существующим listen 80;. `frontend/nginx.conf`
- [ ] 🟡 **728.** (улучшение) tokio подключён с features = ["full"] вместо реально используемого подмножества -> Заменить "full" на явный список используемых фич (rt-multi-thread, macros, fs, process, signal, net, time, sync). `backend/Cargo.toml`
- [ ] 🟡 **733.** (улучшение) nginx.conf не включает gzip для текстовых ассетов SPA -> Добавить gzip on; gzip_types text/css application/javascript application/json; в server-блок. `frontend/nginx.conf`
- [ ] 🟡 **694.** (проблема) apt-get install ffmpeg без версии — рантайм ffmpeg не зафиксирован -> Указать ffmpeg=<version> явно или зафиксировать образ по digest, чтобы версия ffmpeg была воспроизводимой. `backend/Dockerfile`
- [ ] 🟡 **695.** (проблема) Docker build не кэширует cargo-зависимости — любое изменение исходников пересобирает все крейты заново -> Сначала COPY Cargo.toml/Cargo.lock и собрать зависимости отдельным слоем (cargo build с пустым src/main.rs-заглушкой), затем COPY остальных исходников. `backend/Dockerfile`
- [ ] 🟡 **696.** (проблема) CI не собирает и не проверяет сами Docker-образы -> Добавить job docker в ci.yml с docker build ./backend и docker build ./frontend, либо docker compose build. `.github/workflows/ci.yml`
- [x] 🟡 **698.** (проблема) README требует Node.js 18+, но CI и Dockerfile используют Node 20 — версии не синхронизированы -> README поднят до Node.js 20+, добавлен `.nvmrc` со значением `20`. `README.md`
- [x] 🟡 **699.** (проблема) README не документирует CORS_ALLOW_ORIGINS и RECOVER_JOBS_LIMIT, хотя это реальные переменные окружения -> README теперь перечисляет `CORS_ALLOW_ORIGINS`, `RECOVER_JOBS_LIMIT` и дефолты. `README.md`
- [ ] 🟡 **701.** (проблема) main.rs напрямую создаёт пути sources/outputs вместо делегирования Library -> Вынести создание storage-подкаталогов в Library::load или AppState::new, чтобы main.rs не знал о конкретных именах поддиректорий. `backend/src/main.rs`
- [ ] 🟡 **704.** (проблема) Нет trace-идентификатора на уровне job — лог-строки одной задачи не сопоставить друг с другом -> Обернуть обработку каждой задачи в tracing::info_span!("job", job_id = %id) при её запуске. `backend/src/handlers/mod.rs`
- [ ] 🟡 **705.** (проблема) Нет метрик Prometheus/OpenMetrics для очереди задач и рендеров -> Подключить metrics-exporter-prometheus и отдавать /metrics с счётчиками активных/завершённых/упавших задач. `backend/src/main.rs`
- [ ] 🟡 **711.** (проблема) nginx.conf не устанавливает заголовки безопасности (X-Content-Type-Options, X-Frame-Options, CSP) -> Добавить add_header X-Content-Type-Options nosniff, X-Frame-Options DENY и базовый Content-Security-Policy в server-блок. `frontend/nginx.conf`
- [ ] 🟡 **712.** (проблема) nginx проксирует /files/ на backend без кэширующих заголовков для статики -> Добавить expires/Cache-Control с длинным TTL для проксируемых /files/ ответов, раз содержимое по id не меняется. `frontend/nginx.conf`
- [ ] 🟡 **714.** (проблема) restart: unless-stopped без ограничения количества попыток — crash loop будет длиться бесконечно -> Рассмотреть restart: on-failure:5 либо добавить внешний мониторинг перезапусков через HEALTHCHECK. `docker-compose.yml`
- [ ] 🟡 **715.** (проблема) Frontend Dockerfile не поддерживает передачу VITE_*-переменных на этапе сборки -> Добавить ARG VITE_API_BASE (и подобные) с ENV перед RUN npm run build, документировать в README. `frontend/Dockerfile`
- [ ] 🟡 **716.** (проблема) Makefile дублирует cd backend && / cd frontend && в каждой строке вместо -C -> Использовать $(MAKE) -C backend <target> / $(MAKE) -C frontend <target> либо объединить команды одного каталога в одну строку с &&. `Makefile`
- [ ] 🟡 **717.** (проблема) Makefile-таргеты check и lint дублируют одинаковые clippy/eslint-команды вместо переиспользования lint -> Сделать check: lint test build своими зависимостями в Makefile вместо копирования команд. `Makefile`
- [ ] 🟡 **719.** (проблема) CI не кэширует сборку Vite, только node_modules через cache: npm -> Добавить actions/cache для node_modules/.vite с ключом по хэшу lock-файла и исходников. `.github/workflows/ci.yml`
- [ ] 🟡 **720.** (проблема) Cargo.toml не задаёт профиль release — нет тюнинга LTO/codegen-units для рантайм-образа -> Добавить [profile.release] с lto = true и codegen-units = 1 для более быстрого и компактного бинарника. `backend/Cargo.toml`
- [ ] 🟡 **721.** (проблема) README не описывает поведение приложения при недоступности ffmpeg/yt-dlp после старта -> Добавить абзац в README о поведении при рантайм-сбоях внешних инструментов (задачи упадут с ошибкой, /api/health не обновится до перезапуска). `README.md`
- [ ] 🟡 **722.** (проблема) Makefile-таргет dev (scripts/dev.sh) не имеет соответствующего CI-джоба -> Добавить shellcheck scripts/dev.sh как отдельный шаг в CI либо в make lint. `Makefile`
- [ ] 🟡 **723.** (баг) FILE_TTL_HOURS из env умножается на 3600 без проверки переполнения u64 -> Использовать ttl_hours.saturating_mul(3600) или checked_mul с логированием ошибки вместо прямого умножения. `backend/src/main.rs`
- [ ] 🟡 **725.** (проблема) MAX_HEIGHT читается из env внутри download_video при каждом вызове вместо однократного чтения при старте -> Перенести чтение MAX_HEIGHT в main.rs/Config и передавать значение параметром в download_video. `backend/src/tools/mod.rs`
- [ ] 🟡 **726.** (проблема) MAX_HEIGHT не покрыт unit-тестом, в отличие от аналогичного RECOVER_JOBS_LIMIT -> Вынести чтение MAX_HEIGHT в отдельную функцию с unit-тестом по образцу recover_jobs_limit(). `backend/src/tools/mod.rs`
- [x] 🟡 **727.** (проблема) RUST_LOG не задокументирован в README, хотя реально читается через EnvFilter -> README теперь перечисляет `RUST_LOG` и фактический дефолт `info,tower_http=info`. `README.md`
- [ ] 🟡 **729.** (проблема) tower объявлен дважды в Cargo.toml — в [dependencies] и [dev-dependencies] с разными наборами фич -> Объединить в одну запись tower в [dependencies] с нужными фичами, либо явно прокомментировать, почему нужно раздельное объявление. `backend/Cargo.toml`
- [ ] 🟡 **730.** (проблема) docker-compose.yml не используется и не проверяется нигде в CI -> Добавить шаг docker compose config -q (валидация синтаксиса) или полноценный docker compose build в CI. `docker-compose.yml`
- [ ] 🟡 **731.** (проблема) depends_on в docker-compose без condition: service_healthy — frontend стартует раньше готовности backend -> Добавить HEALTHCHECK в backend/Dockerfile и заменить depends_on на форму condition: service_healthy. `docker-compose.yml`
- [ ] 🟡 **732.** (проблема) Порт backend жёстко продублирован в 3 местах без единого источника истины -> Параметризовать порт через build-arg/env в nginx.conf (envsubst в entrypoint) и явно задавать PORT в docker-compose.yml backend-сервиса. `frontend/nginx.conf`
- [ ] 🟡 **734.** (проблема) CI не имеет concurrency-группы — параллельные пуши не отменяют устаревшие прогоны -> Добавить concurrency: { group: ci-${{ github.ref }}, cancel-in-progress: true } на верхнем уровне ci.yml. `.github/workflows/ci.yml`
- [ ] 🟡 **735.** (проблема) curl остаётся в финальном рантайм-образе backend после однократного использования для скачивания yt-dlp -> Скачать yt-dlp в отдельном build-стейдже с curl и скопировать только бинарник в финальный образ через COPY --from, не устанавливая curl в рантайм-слой. `backend/Dockerfile`

### Доменная модель и архитектура целиком (cross-cutting SOLID и DRY) (48)

- [x] ✅ **736.** `EditRequest` ограничен wire boundary; `EditPlan::compile` преобразует его в `EditSpec` + `OutputSpec`, FFmpeg зависит от immutable plan через port. `backend/src/domain/edit.rs`, `backend/src/domain/output.rs`, `backend/src/services/render.rs`, `backend/src/ports/media_export.rs`
- [ ] 🔴 **737.** (проблема) Контракт полей правки продублирован в шести местах без единого источника истины -> Сгенерировать TS-тип из Rust (например, ts-rs/specta) и вывести PRESET_KEYS/дефолты из единственного описания полей. `backend/src/model.rs`
- [ ] 🔴 **738.** (проблема) Новый эффект требует правок в 4+ несвязанных файлах backend -> Ввести таблицу/реестр эффектов (имя, тип параметра, диапазон, генератор фильтра) как единый источник для валидации, сборки фильтров и UI. `backend/src/tools/args.rs`
- [ ] 🔴 **742.** (баг) render_cache_key считается до `EditPlan::compile`, поэтому не использует canonical plan identity. -> Перестроить lookup/single-flight после source probe вокруг `plan_fingerprint`. `backend/src/handlers/mod.rs`
- [ ] 🟠 **740.** (дизайн) На бэкенде нет доменного типа Project/EditState — только строка edit_json + Value на границе -> Ввести ProjectUpsertRequest { video_id, name, video: VideoInfo, edit: EditRequest } и десериализовать тело запроса в него целиком. `backend/src/db.rs`
- [ ] 🟠 **739.** (проблема) edit_json в проектах хранится как непроверенный serde_json::Value -> Десериализовать edit в EditState/EditRequest перед записью в БД и отклонять невалидные значения на project_upsert_handler. `backend/src/db.rs`
- [x] ✅ **741.** Source-aware normalization удалена из HTTP handler и выполняется на compile boundary; domain constructors и serde отдельно проверяют инварианты. `backend/src/services/render.rs`, `backend/src/domain/edit.rs`
- [ ] 🟠 **743.** (проблема) Форматно-специфичные наборы кодеков дублируются между push_video_codec и build_concat_args -> Вынести единую функцию audio_codec_for_format(format) -> &str и переиспользовать её в push_audio-вызовах и build_concat_args. `backend/src/tools/args.rs`
- [x] ✅ **745.** Неизвестные look preset и censor color отклоняются typed parser до создания плана. `backend/src/domain/edit.rs`, `backend/src/services/render.rs`
- [ ] 🟠 **748.** (проблема) EditPanel.vue читает и пишет весь глобальный state.edit напрямую, а не через props/emit -> Ввести props для конкретных секций (trim, crop, effects) и emit('update:...') вместо прямого чтения/записи в общий reactive state. `frontend/src/components/EditPanel.vue`
- [ ] 🟠 **749.** (проблема) store.ts напрямую знает о localStorage и HTTP/SQLite-эндпоинтах вместо абстракции хранилища -> Выделить интерфейс PresetStore/ProjectStore с реализациями поверх localStorage и HTTP, инжектируемыми в store.ts. `frontend/src/store.ts`
- [x] ✅ **758.** Wire geometry/timing преобразуются в `TimeRange`/`PixelRect`/`OutputScale`; custom serde не позволяет обойти invariants. `backend/src/domain/edit.rs`
- [x] ✅ **760.** `VideoCodec` валидируется один раз в `OutputSpec`; FFmpeg adapter не читает raw codec string. `backend/src/domain/output.rs`, `backend/src/tools/args.rs`
- [ ] 🟠 **766.** (баг) video_filters строит drawbox/crop по исходным координатам, но segments-путь применяет их уже после конкатенации нескольких кусков -> Задокументировать явно, что censor/crop-координаты валидны только пока все сегменты берутся из одного source с постоянным разрешением. `backend/src/tools/args.rs`
- [ ] 🟠 **767.** (баг) reverse при segments переворачивает уже склеенный ролик целиком. -> Зафиксировать семантику в `EditPlan::compile` либо запретить неоднозначную комбинацию. `backend/src/tools/args.rs`
- [ ] 🟠 **773.** (проблема) project_upsert_handler валидирует форму project JSON вручную через Value-индексацию вместо десериализации в типизированный DTO -> Определить `#[derive(Deserialize)] struct ProjectUpsertRequest { video_id: String, video: Value, edit: Value, name: Option<String> }` и заменить Json<Value> на Json<ProjectUpsertRequest>. `backend/src/http/mod.rs`
- [ ] 🟠 **775.** (проблема) Db::upsert_project делает SELECT id, затем UPDATE/INSERT, затем ещё раз SELECT * без единой транзакции -> Обернуть три запроса в одну транзакцию (pool.begin()) или использовать один INSERT ... ON CONFLICT(video_id) DO UPDATE ... RETURNING *. `backend/src/db.rs`
- [ ] 🟠 **783.** (баг) render_locks в AppState растёт неограниченно — записи никогда не удаляются после завершения рендера -> После освобождения _render_guard в edit_handler удалять запись из render_locks (например, через weak-reference или periodic sweep неиспользуемых Arc с strong_count == 1). `backend/src/state.rs`
- [ ] 🟡 **750.** (идея) Нет Timeline IR — EditRequest моделирует одну операцию на весь клип, а не последовательность операций -> Спроектировать Timeline { clips: Vec<ClipRef>, ops: Vec<Operation> } как отдельный IR поверх текущего EditRequest для одноклипового MVP. `backend/src/model.rs`
- [x] ✅ **751.** `OutputSpec` независимо владеет format/codec/audio/CRF/fps/dimensions и codec-specific defaults. `backend/src/domain/output.rs`
- [ ] 🟡 **759.** (идея) Нет endpoint/типа для получения списка допустимых значений effect enum'ов клиентом -> Добавить /api/edit-options, отдающий JSON с допустимыми пресетами/цветами/форматами, и генерировать UI-опции из него. `backend/src/tools/args.rs`
- [ ] 🟡 **764.** (идея) Нет versioning/schema-migration стратегии для EditState, персистентно хранимого в localStorage и SQLite -> Либо реализовать фактическую миграцию по schema_version при чтении старых edit_json, либо убрать неиспользуемую колонку из схемы, чтобы не создавать ложное ощущение защиты. `backend/src/db.rs`
- [ ] 🟡 **744.** (улучшение) filter_preset и qualityTier/tierToCrf — строковые enum без типовой защиты на границе backend/frontend -> Определить общий enum LookPreset с сериализацией serde(rename_all) на бэке и сгенерировать соответствующий TS union. `backend/src/tools/args.rs`
- [ ] 🟡 **753.** (улучшение) capturePreset использует небезопасный `as unknown as Record<string, unknown>` вместо типобезопасного маппинга -> Заменить на `(Object.keys(state.edit) as (keyof EditState)[])` без unknown-каста, либо явный switch по ключам с корректной типизацией. `frontend/src/store.ts`
- [ ] 🟡 **761.** (дизайн) qualityTier на фронте не имеет прямого отражения в EditRequest — переводится в quality: CRF асимметрично формату -> Либо хранить qualityTier прямо в EditRequest как typed enum и переводить в CRF только на бэкенде, либо восстанавливать tier обратным поиском по CRF при загрузке проекта. `frontend/src/types.ts`
- [ ] 🟡 **780.** (улучшение) video_filters добавляет каждый новый эффект через последовательный if-блок, порядок эффектов задан императивно и не выражен декларативно -> Вынести список эффектов как Vec<(EffectKind, impl Fn(&EditRequest) -> Option<String>)> в фиксированном порядке, чтобы порядок был декларативным и виден без чтения всего тела функции. `backend/src/tools/args.rs`
- [ ] 🟡 **782.** (улучшение) state.rs напрямую использует std::collections::HashMap с ручной блокировкой Mutex вместо инкапсуляции job-хранилища за отдельным типом -> Вынести JobStore { jobs: Mutex<HashMap<...>>, cancels: Mutex<HashMap<...>> } в отдельный тип со своим API, инжектируемый в AppState. `backend/src/state.rs`
- [ ] 🟡 **746.** (проблема) buildEditPayload дублирует логику default-значений, уже описанную в defaultEdit -> Сравнивать state.edit с результатом defaultEdit() программно (diff по ключам) вместо ручного перечисления условий. `frontend/src/store.ts`
- [ ] 🟡 **747.** (проблема) tierToCrf дублирует пороги качества, независимо заданные как unwrap_or в push_video_codec -> Переносить дефолтный CRF только на бэкенд и убрать qualityTier/tierToCrf с фронта, либо наоборот сделать таблицу серверной и отдавать её через /api. `frontend/src/store.ts`
- [ ] 🟡 **752.** (проблема) Нет domain events / audit trail для отредактированных клипов -> При желании версионирования — добавить таблицу project_history с append-only записями вместо UPDATE по video_id. `backend/src/db.rs`
- [ ] 🟡 **754.** (проблема) PRESET_KEYS не включает поля geometry (crop/censor/scale/trim/cut), но критерий 'reusable look' не проверяется тестами -> Либо исключить censorColor из PRESET_KEYS вместе с остальной geometry, либо добавить тест, фиксирующий намеренный список полей пресета. `frontend/src/store.ts`
- [ ] 🟡 **755.** (проблема) JobStatus::from_token имеет неявный fallback на Pending, скрывающий реальные ошибки БД -> Вернуть Result<JobStatus, String> или залогировать tracing::warn при непойманном значении вместо тихого fallback. `backend/src/model.rs`
- [ ] 🟡 **756.** (проблема) Job.stage — свободная строка без enum, допустимые значения перечислены только в комментарии -> Заменить на enum JobStage { Queued, Downloading, Processing } с serde(rename_all = "lowercase"). `backend/src/model.rs`
- [ ] 🟡 **757.** (проблема) censorColor whitelist (sanitize_color) не синхронизирован с UI-опциями цвета на фронте -> Экспортировать список допустимых цветов с бэкенда (например, через /api/health или отдельный constants-эндпоинт) и генерировать из него select-опции на фронте. `backend/src/tools/args.rs`
- [x] ✅ **762.** Codec-specific CRF валидируется в `OutputSpec`: H.264/H.265 0..51, VP9/AV1 0..63. `backend/src/domain/output.rs`
- [ ] 🟡 **763.** (проблема) format_secs — тривиальная однострочная обёртка, дублирующая format! напрямую использованный в другом месте -> Использовать format_secs везде, где форматируется время в секундах внутри этого файла, либо удалить обёртку и оставить прямой format!. `backend/src/tools/args.rs`
- [x] ✅ **765.** `EditPlan` v2 immutable/source-aware; identity tests покрывают source metadata, typed edit/output и tampered serde. `backend/src/services/render.rs`
- [x] ✅ **768.** `FfmpegExportCompiler` возвращает args и expected duration одним `CompiledExportCommand` из одного compile pass. `backend/src/ports/media_export.rs`, `backend/src/tools/args.rs`
- [ ] 🟡 **769.** (проблема) MediaEntry::from_result парсит JSON вручную теми же ключами, что json!({...}) в handlers/mod.rs, без общего типа-источника -> Ввести общий struct ResultInfo/SourceInfo с Serialize и строить его напрямую вместо json!({...}), передавая typed-значение в MediaEntry::from_result. `backend/src/library.rs`
- [ ] 🟡 **770.** (проблема) Source clamp и encoder even-rounding разделены между plan compiler и FFmpeg adapter; one-pixel zero bug закрыт, но ownership policy двойной. -> Ввести pixel-format capability в output compile policy. `backend/src/services/render.rs`, `backend/src/tools/args.rs`
- [ ] 🟡 **771.** (проблема) Job.stage строковые литералы 'queued'/'downloading'/'processing' захардкожены в handlers/mod.rs без общего источника -> Ввести JobStage enum (см. отдельный пункт про Job.stage) и заменить строковые литералы на его варианты. `backend/src/handlers/mod.rs`
- [x] ✅ **772.** `OutputFormat::extension` и validated `OutputSpec.video_codec` являются единым typed source для filename и encoder. `backend/src/domain/output.rs`
- [ ] 🟡 **774.** (проблема) ensure_project_json_size сериализует video/edit ещё раз только для проверки размера, а upsert_project сериализует их снова -> Сериализовать video/edit один раз в handler, передать готовые строки в Db::upsert_project (изменив сигнатуру на &str) и проверять их len() напрямую. `backend/src/http/mod.rs`
- [ ] 🟡 **776.** (проблема) row_to_job и row_to_project — почти идентичный шаблон ручного маппинга SqliteRow -> struct, повторённый для каждой таблицы -> Вынести sqlx::FromRow (derive или ручной impl) для Job/Project вместо отдельных функций row_to_*, либо helper `fn json_column<T>(row, name) -> Result<T>`. `backend/src/db.rs`
- [ ] 🟡 **777.** (проблема) EditRequest сериализуется в render_cache_key со всеми полями включая video_id, что делает кэш непереносимым между источниками с идентичным edit -> Если нужен переносимый кэш, хэшировать video_id отдельно от содержимого edit и учитывать это явно в схеме ключа; иначе явно задокументировать, что кэш всегда per-source. `backend/src/handlers/mod.rs`
- [ ] 🟡 **778.** (проблема) watch(() => [state.video, state.edit]) в автосейве триггерится на любое изменение video, включая переключение на null при deleteFromLibrary -> Разделить на два отдельных watch: один для смены клипа (сброс/восстановление проекта), другой только для state.edit (debounced autosave). `frontend/src/store.ts`
- [ ] 🟡 **779.** (проблема) MediaEntry (frontend types.ts) не отражает поля vcodec/acodec/fps, которые есть в VideoInfo, из-за чего openFromLibrary теряет эти данные при реоткрытии клипа -> Добавить vcodec/acodec/fps в MediaEntry и в MediaEntry::from_result (library.rs:33-46), либо запрашивать полный VideoInfo отдельным эндпоинтом при открытии из библиотеки. `frontend/src/types.ts`
- [x] ✅ **781.** Filename и adapter используют `OutputFormat`; AV1 типизированно отображается в MP4 extension/container. `backend/src/domain/output.rs`, `backend/src/handlers/mod.rs`, `backend/src/tools/args.rs`

## Исследовательский чеклист: 100 репозиториев (14 июля 2026)

Это ещё 100 неповторяющихся задач №784-883 поверх исторического набора из 565
SOLID/DRY-пунктов. Отбор, точные рейтинги GitHub, papers, стандарты и связь
«репозиторий → решение» находятся в
[docs/research-100.md](docs/research-100.md); полные архитектурные обоснования -
в [architecture.md](architecture.md#исследовательский-слой-100-репозиториев-14-июля-2026).
Брать по одной строке: сначала контракт/fixture/benchmark, затем адаптер.

### A. NLE и media pipeline

- [x] ✅ **784. FFmpeg:** `FilterGraph` DAG типизирует audio/video pads, валидирует topology/required inputs/single-input и даёт стабильные JSON/DOT; текущие ffmpeg audio/video chains компилируются через него. `backend/src/domain/filter_graph.rs`, `backend/src/tools/args.rs`
- [x] ✅ **785. GStreamer:** Preview оформлен как `Idle/Ready/Paused/Playing/Draining/Failed` state machine с monotonic clock и fake-time race tests. `frontend/src/lib/features/player/previewSession.ts`
- [x] ✅ **786. MLT:** Независимые `Producer/Filter/Transition/Consumer` ports, role-scoped registry и стабильный manifest не импортируют process/CLI/runtime types. `backend/src/domain/media_pipeline.rs`
- [x] ✅ **787. Olive:** Стабильные `ClipId`/`OperationId`, immutable timeline operations и invertible move command сохраняют ссылки после reorder/undo/serde round-trip. `backend/src/domain/timeline.rs`
- [ ] 🟡 **788. OpenShot:** Создать golden corpus версий проекта, migration-to-latest и policy test для неизвестных операций. `backend/tests/fixtures/projects/` (target)
- [x] ✅ **789. Kdenlive:** Content-addressed proxy service проверяет source checksum, single-flight background generation, relink и удаление; staging имеет media suffix, progress без consumer не буферизуется, `MediaIntent::Export` всегда возвращает original. `backend/src/analysis/proxy.rs`, `backend/src/tools/proxy.rs`
- [x] ✅ **790. Shotcut:** Runtime manifest encoders/muxers/filters/hardware с fingerprint и disabled-reason подключён к UI. `backend/src/capabilities.rs`, `frontend/src/components/`
- [x] ✅ **791. Blender:** Typed artifact DAG проверяет dependency existence, fingerprint identity и точечную downstream invalidation; serde не обходит key/dependency/cycle invariants. `backend/src/domain/artifact_graph.rs`
- [x] ✅ **792. SRP/DIP/OBS:** Source-aware `EditPlan` v2 содержит validated `EditSpec` + `OutputSpec`; preview/export profiles независимы, serde пересчитывает identity, export использует `ExportCommandCompiler` port. `backend/src/domain/{edit,output}.rs`, `backend/src/services/{preview,render}.rs`, `backend/src/ports/media_export.rs`
- [x] ✅ **793. Remotion:** Frame key зависит от frame/plan/source/output fingerprints; single-flight publication, checksum-bound path, retry и resolve проверены adversarial tests. `backend/src/render/frame_renderer.rs`

### B. Кодеки, качество и packaging

- [x] ✅ **794. VMAF:** Opt-in advisory contract фиксирует versioned VMAF model/viewing condition, VMAF/PSNR/SSIM aggregate и per-scene метрики с fail-closed validation. `backend/src/analysis/quality.rs`
- [x] ✅ **795. Av1an:** Scene-aware contiguous chunk plan, atomic manifest, identity-bound checksums, missing/corrupt reconcile и compatible verified stitch повторно валидируют identity перед операцией. `backend/src/render/chunks.rs`
- [x] ✅ **796. rav1e:** cgroup-aware `EncodeBudget` валидирует threads/tiles/speed/memory; render admission и production FFmpeg дают bounded filter/encoder threads и AV1 preset/tiles. `backend/src/config/encode_budget.rs`, `backend/src/state.rs`, `backend/src/tools/args.rs`
- [x] ✅ **797. Opus:** `AudioOutputSpec` типизирует bitrate/channel layout/sample rate/loudness и проверяет container/codec compatibility; стабильный `OutputSpec` расширяет policy без wire-breaking поля. `backend/src/domain/audio_output.rs`, `backend/src/domain/output.rs`
- [x] ✅ **798. Shaka Packager:** Checksummed `OutputBundle` и отдельные typed HLS/DASH Shaka adapters находятся после encode; обычный file export модуль не импортирует. `backend/src/packaging/mod.rs`
- [x] ✅ **799. libavif:** Still-export round-trip contract отдельно доказывает preservation/loss для color primaries, ICC, alpha, orientation и metadata blocks. `backend/tests/media/still_metadata.rs`
- [x] ✅ **800. libjxl:** `StillImageEncoder` port владеет capability descriptor, pre-spawn validation и encode result; `EditPlan` не изменён. `backend/src/ports/still_encoder.rs`
- [x] ✅ **801. libheif:** Container/brand/item role/codec типизированы; duplicate/missing primary, sequence-brand и alpha auxiliary mismatch отклоняются до adapter spawn. `backend/src/domain/still_container.rs`
- [ ] 🟡 **802. SRT:** Оформить remote ingest как adapter с reconnect/latency/clock budgets, выдающий immutable source artifact. `backend/src/ingest/srt.rs` (target)
- [x] ✅ **803. PyAV:** ffprobe adapter нормализует container/streams/time-base/frame-rate/color/rotation/disposition в typed `ProbeResult`; raw fixture corpus проверяет контракт. `backend/src/domain/media_probe.rs`, `fixtures/media-probe/`

### C. Playback и streaming

- [x] ✅ **804. Video.js:** `PlayerAdapter` отделяет preview/store contracts от `HTMLMediaElement`; browser adapter изолирован. `frontend/src/lib/ports/player.ts`, `frontend/src/lib/adapters/player/mediaElement.ts`
- [x] ✅ **805. hls.js:** Playback errors разделены на network/media/config/unsupported и fatal/recoverable; recovery имеет bounded attempts/backoff. `frontend/src/lib/features/player/errors.ts`
- [x] ✅ **806. Shaka Player:** Unsupported capability возвращается typed result с причиной и progressive fallback. `frontend/src/lib/features/player/capabilities.ts`
- [x] ✅ **807. dash.js:** Preview representation policy выбирает seek-friendly вариант и типово не меняет export spec. `frontend/src/lib/features/player/representationPolicy.ts`
- [x] ✅ **808. Plyr:** Keyboard/label/`aria-valuetext` contract подключён к preview controls и покрыт component/headless tests. `frontend/src/lib/ui/media-controls/`, `frontend/src/lib/components/VideoPreview.svelte`
- [x] ✅ **809. MediaElement:** Local/progressive/HLS/DASH sources и события нормализованы в один player model и suite. `frontend/src/lib/adapters/player/sources.ts`
- [x] ✅ **810. Media Chrome:** Custom controls разбиты на headless commands/labels без чтения global store; Svelte boundary только применяет commands. `frontend/src/lib/ui/media-controls/model.ts`
- [ ] 🟡 **811. MediaMTX:** Держать live ingest gateway отдельным сервисом, отдающим редактору только immutable recording artifact. `services/ingest-gateway` (future)
- [x] ✅ **812. SRS:** Ingest/analysis/export используют независимые admission pools с отдельными env quotas; 100-sample regression держит export admission p95 доступным при полностью занятом ingest. `backend/src/config/resource_classes.rs`, `backend/src/state.rs`
- [x] ✅ **813. Jellyfin:** FTS5 adapter получил durable `(created_at,id)` cursor, incremental startup sync, изоляцию malformed entries, deferred transient errors и полный rebuild derived state. `backend/src/services/media_indexer.rs`

### D. Editor interactions и canvas

- [x] ✅ **814. Excalidraw:** Full-state JSON snapshots заменены field-level `PatchCommand.apply/invert/merge`; crop/censor pointer lifecycle открывает и закрывает одну drag transaction. `frontend/src/domain/history.ts`, `frontend/src/store.ts`
- [x] ✅ **815. tldraw:** Tool state machine владеет единым идемпотентным pointer-capture/cancel lifecycle и подключён к crop/censor overlay. `frontend/src/lib/features/canvas/toolMachine.ts`, `frontend/src/lib/components/RectOverlay.svelte`
- [x] ✅ **816. Penpot:** Semantic token contract versioned в одном CSS-файле; dark/light contrast tests и `check:tokens` запрещают raw palette в feature code. `frontend/src/lib/ui/tokens.css`, `frontend/scripts/check-design-tokens.mjs`
- [x] ✅ **817. Fabric.js:** Branded source/preview/export/normalized spaces и immutable `Transform2D` подключены к overlay mapping; matrix round-trip/rotation/scale проверяются corpus/property tests. `frontend/src/domain/geometry.ts`, `frontend/src/components/RectOverlay.vue`
- [x] ✅ **818. Konva:** Media/guides/overlays/handles разделены на scene layers; hit testing ограничен interactive overlay/handle layers. `frontend/src/lib/features/canvas/scene.ts`
- [x] ✅ **819. PixiJS:** Reproducible Chromium workload измеряет 2 500 timeline items × 60 frames: DOM p50/p95 5.7/8.8 ms, Canvas2D 0.1/0.2 ms, WebGL 0.0/0.1 ms; GPU остаётся future threshold decision и не включён в product path. `frontend/bench/canvas/`
- [x] ✅ **820. Paper.js:** Rust и TS geometry kernels читают один fixture corpus; clamp containment, inverse transforms, singular/NaN rejection и fuzz-like inputs проверяются с обеих сторон. `backend/src/domain/geometry.rs`, `frontend/src/domain/geometry.ts`, `fixtures/geometry/`
- [x] ✅ **821. TUI Image Editor:** Declarative tool descriptors имеют typed ID/shortcut/cursor/capture policy; startup и tests fail-fast на ID/shortcut collision. `frontend/src/lib/features/tools/registry.ts`
- [x] ✅ **822. xyflow:** Render DAG visualizer грузится только под `import.meta.env.DEV`; production bundle gate ищет marker и падает при leakage. `frontend/src/lib/dev/renderGraph/`, `frontend/scripts/check-bundle-budget.mjs`
- [x] ✅ **823. Motionity:** `KeyframeTrack<T>` валидирует time base/ticks/finite values, детерминированно семплирует hold/linear/cubic и имеет отдельные preview/ffmpeg adapters с golden expression. `backend/src/domain/keyframes.rs`

### E. Rust backend

- [x] ✅ **824. Tokio:** Root `CancellationToken` + `TaskTracker` владеют workers; SIGINT/SIGTERM запускают bounded HTTP/task drain, process runner эскалирует process groups. `backend/src/runtime.rs`, `backend/src/main.rs`
- [x] ✅ **825. Axum:** System/project handlers вынесены в port-based routers; contract tests используют runtime port и in-memory fake без `AppState`, SQLite и filesystem, production подключает те же DTO/status. `backend/src/http/mod.rs`, `backend/src/http/ports.rs`
- [x] ✅ **826. Tower:** Route catalog фиксирует единый порядок request ID/body/auth/rate/timeout/tracing и способ enforcement; outer middleware собирается одной функцией. Auth честно отмечен как local-deployment boundary, а не как готовая public auth. `backend/src/http/policy.rs`
- [x] ✅ **827. Actix Web:** In-process Axum baseline фиксирует throughput/p50/p95/p99/peak RSS; локальный release run: 1.61M req/s, 0.58/0.67/0.75 µs, 2.23 MB. Framework rewrite требует ADR и сопоставимого profile. `backend/benches/http_baseline.rs`
- [x] ✅ **828. Hyper:** Реальные TCP-тесты доказывают incremental upload, cleanup после disconnect и отзывчивость API при slow Range reader. `backend/tests/http_backpressure.rs`
- [x] ✅ **829. Serde:** Wire DTO strict, поддерживают `schemaVersion: 1`; project envelope strict, вложенные persisted documents tolerant. `backend/src/model.rs`, `backend/src/http/mod.rs`, `backend/tests/api.rs`
- [x] ✅ **830. SQLx:** Schema-bound cursor query использует checked macro, migration создаёт compile database, `.sqlx` metadata проходит offline build и CI выполняет pinned `cargo sqlx prepare --check`. `backend/.sqlx/`, `backend/migrations/`, `.github/workflows/ci.yml`
- [x] ✅ **831. tracing:** Span tree `request -> job -> process`, CORS-visible request ID, path-only HTTP fields и URL/query/path canary-redaction реализованы. `backend/src/telemetry.rs`, `backend/src/privacy.rs`
- [x] ✅ **832. Rayon:** Named bounded Rayon pool имеет fail-fast queue admission, cooperative cancellation, panic isolation и saturation/completion metrics; hashing использует этот port. `backend/src/runtime/cpu_pool.rs`, `backend/src/artifacts.rs`
- [x] ✅ **833. rustls:** Deployment ADR разрешает plaintext только на loopback, требует TLS termination для LAN/public, не доверяет forwarded headers без allowlisted proxy hop и запрещает public plaintext profile; image теперь non-root и pinned-download verified. `docs/deployment-security.md`, `backend/Dockerfile`

### F. Jobs и persistence

- [x] ✅ **834. Temporal:** Append-only event history, idempotency keys, reducer/replay, legacy backfill и crash-boundary rollback tests реализованы. `backend/src/jobs/event_log.rs`, `backend/src/jobs/store.rs`
- [x] ✅ **835. Airflow:** Job/attempt разделены; bounded retry идёт по `ErrorKind`, validation/security не retry. `backend/src/jobs/attempt.rs`
- [x] ✅ **836. Celery:** Failed registry и audited operator retry/discard с cap доступны через API. `backend/src/jobs/failed_registry.rs`, `backend/src/handlers/mod.rs`
- [x] ✅ **837. BullMQ:** Stable dedupe + TTL/rate window возвращают прежний job ID и не запускают duplicate process. `backend/src/jobs/dedupe.rs`
- [x] ✅ **838. RQ:** Lifecycle registries и startup reconciliation requeue abandoned attempts по policy. `backend/src/jobs/registry.rs`
- [x] ✅ **839. River:** Job/request/event/dedupe/outbox enqueue атомарен; claim защищён attempt heartbeat, due retry и shutdown recovery, terminal payload очищается, пять failpoints не оставляют partial state; orchestration вынесен из media handlers. `backend/src/jobs/outbox.rs`, `backend/src/jobs/store.rs`, `backend/src/handlers/jobs.rs`
- [x] ✅ **840. Restic:** Content-addressed backup, manifest/checksums/integrity verify и restore drill реализованы; staging исключён. `backend/src/backup.rs`, `backend/src/bin/backup.rs`
- [ ] 🟡 **841. Borg:** Измерить chunk dedup на media corpus и определить prune policy до добавления зависимости. `bench/backup-dedup.md` (target)
- [x] ✅ **842. RocksDB:** SQLite WAL benchmark и migration thresholds зафиксированы; локальные p95-прогоны 0.241-0.632 ms ниже gate 50 ms. `backend/benches/persistence.rs`, `backend/src/jobs/persistence_profile.rs`
- [x] ✅ **843. Meilisearch:** `MediaSearch` port, SQLite FTS5 default и rebuild from source of truth реализованы. `backend/src/ports/media_search.rs`

### G. Vue, frontend и testing

- [x] ✅ **844. Vue:** ESLint запрещает domain→transport/UI, component→API и transport→store/component imports; доступ идёт через store/facade. `frontend/eslint.config.js`
- [ ] 🟠 **845. Pinia:** Выделить pilot `project`/`ui` stores с facade и command-only cross-store interaction. `frontend/src/stores/` (target)
- [x] ✅ **846. Vite:** Initial JS/CSS/total gzip budgets измеряются после build и падают в Make/CI; текущая сборка укладывается в total 60 KiB. `frontend/scripts/check-bundle-budget.mjs`, CI
- [x] ✅ **847. Vitest:** Polling terminal/cancel cases используют fake timers/table cases; autosave/history suites также без real-time sleep. `frontend/src/api.test.ts`, `frontend/src/store.test.ts`
- [ ] 🟡 **848. VueUse:** Централизовать global listeners/resize/online в lifecycle-safe composables и запретить обход lint-аудитом. `frontend/src/composables/` (target)
- [ ] 🟠 **849. Storybook:** Каталогизировать empty/loading/error/long/localized/mobile/reduced-motion states с a11y/screenshots. `frontend/src/**/*.stories.ts` (target)
- [x] ✅ **850. Playwright:** 9 browser smoke cases покрывают backendless offline shell, mocked import/edit/export и 390px no-overflow в трёх движках; harness владеет strict isolated server. `frontend/e2e/smoke.spec.ts`, `frontend/playwright.config.ts`
- [ ] 🟡 **851. Cypress:** Сравнить один fault scenario и оформить ADR выбора ровно одного E2E runner. `docs/adr/e2e-runner.md` (target)
- [ ] 🟠 **852. TanStack Query:** Вынести library/projects/jobs server cache/invalidation/polling из mutable UI stores. `frontend/src/data/` (target)
- [ ] 🟠 **853. Floating UI:** Создать один tooltip/menu/popover primitive с collision/focus return/Escape/outside-click tests. `frontend/src/ui/overlay/` (target)

### H. Security и supply chain

- [x] ✅ **854. OWASP:** Extension/MIME/probe/generated name/private staging/storage headers/body+concurrency limits связаны с 6 focused regressions и residual risks. `docs/threat-model-upload.md`, `backend/tests/upload_security.rs`
- [x] ✅ **855. OSS-Fuzz:** Hermetic OSS-Fuzz project builds five libFuzzer targets under ASan/UBSan, packages seed corpora и фиксирует security/non-security triage SLA. `fuzz/oss-fuzz/`, `backend/fuzz/README.md`
- [x] ✅ **856. cargo-fuzz:** Edit normalization, multipart/archive path, library JSON, DNS-free URL policy и cache-key targets собраны sanitizer build; crash policy требует сначала regression test, затем corpus seed. `backend/fuzz/`
- [x] ✅ **857. RustSec:** Audit ignores по умолчанию пусты; CI требует exact companion exception record с owner/rationale/tracking/expiry и падает на просрочке. Проверка выявила и обновила vulnerable `anyhow` 1.0.102. `.cargo/audit.toml`, `security/advisory-exceptions.toml`
- [x] ✅ **858. cargo-deny:** License/source/yanked/wildcard/duplicate policy включена; private workspace отмечен непубликуемым, текущие advisories/licenses/sources проходят. `deny.toml`, `backend/Cargo.toml`
- [x] ✅ **859. Trivy:** Security workflow сканирует filesystem, Compose/Dockerfile IaC и собранный image, публикует раздельный SARIF; companion exception schema требует expiry. `.github/workflows/security.yml`, `security/trivy-exceptions.toml`
- [x] ✅ **860. OSV-Scanner:** OSV проверяет Rust и npm lockfile в том же workflow, а native RustSec/cargo-deny дают независимую сверку dependency path/fixed version. `.github/workflows/security.yml`
- [x] ✅ **861. Cosign:** Tag release строит immutable digest, создаёт SPDX SBOM и SLSA provenance, keyless-signs digest и блокирует deploy-stage до identity/provenance verification. `.github/workflows/release.yml`
- [x] ✅ **862. Gitleaks:** PR+full-history scan использует default rules, custom query/Bearer token rules и пустой reviewed baseline. `.gitleaks.toml`, `.gitleaksignore`
- [x] ✅ **863. SOPS:** Repo разрешает только SOPS `*.enc.yaml`, external age/KMS keys и quarterly rotation drill; CI отклоняет plaintext-shaped deploy secret files. `ops/secrets/`, `.sops.yaml`, `scripts/check-encrypted-secrets.py`

### I. Observability и performance

- [x] ✅ **864. Prometheus:** `/metrics` отдаёт OpenMetrics queue wait/resource saturation/cache/bytes contract; labels закрыты enum-наборами, тест запрещает ID/URL/filename dimensions. `backend/src/telemetry/metrics.rs`
- [x] ✅ **865. Loki:** Promotion policy разрешает только service/env/level/error_kind с bounded values; request/job/trace IDs остаются fields, URL и filename labels запрещены. `backend/src/telemetry/log_policy.rs`, `ops/observability/vector-policy.yaml`
- [x] ✅ **866. Jaeger:** Deterministic 10% sampling link сохраняется в versioned durable work payload и восстанавливается в job span после SQLite queue boundary. `backend/src/telemetry/context.rs`, `backend/src/handlers/`
- [x] ✅ **867. OpenTelemetry:** Exporter-neutral `TelemetryPort` задаёт semantic events и имеет noop/test/Prometheus adapters без vendor API в domain/services. `backend/src/ports/telemetry.rs`
- [x] ✅ **868. Vector:** Единый allowlist `RedactionTransform` обслуживает log/diagnostic bundle paths; canary fixture доказывает удаление credentials, query secret, absolute path и unknown sensitive fields. `backend/src/privacy/redaction.rs`
- [x] ✅ **869. Parca:** Staging-only policy pins Parca 0.28.0, symbolized profiling profile, 7-day retention и обязательные same-corpus before/after 15-minute captures для perf/concurrency PR. `ops/profiling/`
- [x] ✅ **870. Loom:** `JobCell` и terminal/permit/cancel single-claim invariants model-check'ятся Loom. `backend/src/jobs/job_cell.rs`
- [x] ✅ **871. Hyperfine:** Versioned cold/warm corpus измеряет probe/list/cache-hit/plan compile, пишет median/p95, checksums и environment metadata; optional Hyperfine wrapper готов. `bench/perf/`
- [x] ✅ **872. Flamegraph:** Profile-before-optimize policy и reproducible script сохраняют deterministic SVG + folded stacks для каждого corpus workload. `bench/perf/profile.sh`, `docs/performance.md`
- [x] ✅ **873. tokio-console:** Optional feature запускается только при explicit staging+loopback config; production/local fail closed, runbook требует authenticated tunnel и проверку возврата task count к baseline. `backend/src/telemetry/console.rs`, `ops/profiling/README.md`

### J. ML-assisted media

- [ ] 🟡 **874. Whisper:** Transcript как versioned derived artifact с source checksum/model/language/confidence/invalidation. `backend/src/analysis/transcript.rs` (target)
- [ ] 🟡 **875. whisper.cpp:** Local/offline ASR adapter с capability/resource estimate и понятным CPU fallback. `backend/src/adapters/asr/local.rs` (target)
- [ ] 🟡 **876. faster-whisper:** Benchmark real-time factor/RAM/VRAM/accuracy proxy по model/quantization на языковом corpus. `bench/asr/` (target)
- [ ] 🟠 **877. WhisperX:** Word alignment + confidence для transcript range draft; low-confidence boundary требует preview/confirm. `frontend/src/features/transcript/` (target)
- [ ] 🟡 **878. pyannote:** Speaker track с labels/segments/model/confidence/delete/privacy warning; embeddings не хранить по умолчанию. `backend/src/analysis/speakers.rs` (target)
- [ ] 🟠 **879. PySceneDetect:** Versioned detector/threshold/metrics artifact как snap/chapter suggestions с manual override. `backend/src/analysis/scenes.rs` (target)
- [ ] 🟡 **880. OpenCV:** `VisualAnalysis` port возвращает DTO artifacts и не пропускает OpenCV `Mat` в core. `backend/src/ports/visual_analysis.rs` (target)
- [ ] 🟡 **881. librosa:** Chunked waveform/loudness/beat/onset cache по audio fingerprint/version с progressive UI. `backend/src/analysis/audio_features.rs` (target)
- [ ] 🟡 **882. PaddleOCR:** OCR track с time/bbox/language/confidence/model и local-first privacy profile. `backend/src/analysis/ocr.rs` (target)
- [ ] 🟠 **883. Ultralytics:** Object tracks создают только human-approved follow-crop/censor suggestions; фиксировать model/license/confidence. `backend/src/analysis/object_tracks.rs` (target)

## Следующий research-чеклист: 100 идей №884-983 (18 июля 2026)

Все пункты ниже открыты. Полные source links, обоснования и критерии приёмки -
в [docs/research-next-100.md](docs/research-next-100.md), архитектурная карта -
в [architecture.md](architecture.md#второй-исследовательский-слой-ещё-100-идей-18-июля-2026).
Порядок P0: 934 → 937 → 939 → 943; затем domain contracts 884/888/894/904/906,
после них UI/testing gates 954/955/963/964/965.

### K. Container metadata и provenance

- [ ] 🟠 **884. ProbeEnvelope:** normalized metadata + bounded raw diagnostics; optional malformed tag не отменяет валидный import.
- [ ] 🟠 **885. Stable stream identity:** track ID/kind/language/disposition вместо array index.
- [ ] 🟠 **886. Metadata privacy:** allowlist export/diagnostics и default strip location/device/author/query-like tags.
- [ ] 🟡 **887. Post-mux conformance:** independent probe до atomic output publish.
- [ ] 🟠 **888. Rational media time:** checked ticks/time base внутри domain, float только в UI.
- [ ] 🟠 **889. Display transform:** rotation/SAR/display matrix едины для preview/proxy/export.
- [ ] 🟡 **890. Attachment policy:** MIME/count/bytes allowlist для fonts/covers/Matroska attachments.
- [ ] 🟡 **891. Chapters/timecode:** stable marker tracks, сохраняющие source ticks.
- [ ] 🟠 **892. Source provenance:** checksum, sanitized origin и tool/adapter versions.
- [ ] 🟡 **893. Capability negotiation:** typed container/codec adapter selection до spawn.

### L. Color, HDR и image pipeline

- [ ] 🟠 **894. ColorDescriptor:** primaries/transfer/matrix/range/chroma location с explicit unspecified.
- [ ] 🟠 **895. Color round-trip:** сравнивать descriptor/mastering metadata до и после encode.
- [ ] 🟡 **896. OCIO identity:** config checksum/version входит в artifact fingerprint.
- [ ] 🟡 **897. Scene-linear policy:** ACES working space opt-in и видим в graph.
- [ ] 🟡 **898. HDR frame port:** OpenEXR/float16 adapter без library dependency в domain.
- [ ] 🟠 **899. Display-aware preview:** deterministic tone-map не меняет HDR export.
- [ ] 🟠 **900. Conversion corpus:** CPU/GPU pixel-tolerance references.
- [ ] 🟠 **901. HDR metadata validation:** finite/range/consistency для MaxCLL/MaxFALL/mastering.
- [ ] 🟡 **902. Gamut/clipping UX:** scopes, warning и explicit conversion preset.
- [ ] 🟡 **903. Color QA:** per-scene clipping/histogram/delta отдельно от codec score.

### M. Audio graph, sync и loudness

- [ ] 🟠 **904. AudioTime:** sample ticks и checked conversion к video time.
- [ ] 🟠 **905. Latency compensation:** effect-declared latency и automatic path alignment.
- [ ] 🟠 **906. ChannelLayout:** labels/layout и explicit downmix matrix.
- [ ] 🟠 **907. Two-pass loudness:** versioned EBU R128 measurement artifact.
- [ ] 🟠 **908. True-peak guard:** post-codec ceiling отдельно от LUFS target.
- [ ] 🟡 **909. Waveform pyramid:** content-addressed multi-resolution min/max/RMS tiles.
- [ ] 🟡 **910. Stretch profiles:** realtime/offline adapters одного port.
- [ ] 🟠 **911. Audio graph:** независим от video effects при общем timeline.
- [ ] 🟠 **912. Non-finite sanitizer:** NaN/Inf/denormal/silence stage error.
- [ ] 🟠 **913. A/V drift corpus:** VFR/rates/speed/cut/concat tolerance в frames/samples.

### N. Captions, localization и accessibility

- [ ] 🟠 **914. CaptionTrack/Cue:** stable ID, rational range, language/speaker/region.
- [ ] 🟠 **915. Strict WebVTT:** bounded UTF-8/timestamp/settings/cue parser.
- [ ] 🟡 **916. Overlap semantics:** policy зависит от caption/chapter track kind.
- [ ] 🟠 **917. Safe-region preview:** guides рассчитываются от output aspect.
- [ ] 🟠 **918. libass golden render:** fonts/bidi/outline/positioning corpus.
- [ ] 🟠 **919. Font artifact:** checksum/license/size/fallback chain.
- [ ] 🟡 **920. Readability linter:** CPS/line length/count/gap findings.
- [ ] 🟡 **921. Cue editing transaction:** drag/split/merge + keyboard = один undo.
- [ ] 🟡 **922. Linked translations:** cue ID/alignment вместо shared index.
- [ ] 🟠 **923. Accessible-media audit:** captions/descriptions/chapters/languages + manual review.

### O. Local-first collaboration, storage и upload

- [ ] 🟠 **924. Project operation log:** stable append-only operations + deterministic snapshot.
- [ ] 🟠 **925. CRDT scope:** timeline metadata sync, media blobs out-of-band.
- [ ] 🟡 **926. State-vector sync:** duplicate/reordered updates идемпотентны.
- [ ] 🟠 **927. Selective undo:** transaction origin не откатывает remote edits.
- [ ] 🟠 **928. Safe compaction:** durable snapshot + acknowledgement horizon.
- [ ] 🟠 **929. SQLite replication:** measured RPO/RTO и обязательный restore drill.
- [ ] 🟠 **930. Resumable upload:** durable ID/offset/checksum/expiry.
- [ ] 🟡 **931. Blob dedupe:** checksum storage отдельно от ownership/reference count.
- [ ] 🟡 **932. Project lease:** read-only conflict вместо silent last-write-wins.
- [ ] 🟠 **933. Sharing privacy:** explicit scope/recipients/encryption/revoke, off by default.

### P. Process isolation и plugin security

- [x] ✅ **934. ProcessPolicy:** Только typed `PreparedCommand` может spawn; role, FS/network/env, kernel и bounded output/line budgets обязательны; timeout и detached pipe holders завершают process group через TERM→KILL. `backend/src/process_control/{policy,runtime,limits,execution}.rs`
- [ ] 🔴 **935. NsJail adapter:** namespaces/cgroup/seccomp/dropped privileges в public Linux profile.
- [ ] 🟠 **936. Filesystem allowlist:** Typed read-only/read-write scope и scrubbed `HOME` готовы; mount enforcement и hidden siblings/home ждут sandbox adapter.
- [ ] 🟠 **937. Network deny:** FFmpeg/ffprobe URL rejection + protocol allowlist и pinned downloader proxy готовы; kernel network namespace ещё открыт.
- [ ] 🟠 **938. Versioned seccomp:** tool fingerprint + codec regression corpus.
- [ ] 🟠 **939. Kernel limits:** Unix CPU/FD/file-size rlimits, Linux address-space и bounded pipes готовы; cgroup RSS/PID tree/per-tenant uid ещё открыты.
- [ ] 🟠 **940. Per-tenant identity:** отдельные uid/gid/work directories.
- [ ] 🟡 **941. Wasm capabilities:** host calls, memory/fuel/epoch limits.
- [ ] 🟠 **942. Custom logic quarantine:** signed descriptor + sandbox tier.
- [x] ✅ **943. Isolation tiers ADR:** Local требует loopback, LAN explicit, public и фиктивный sandbox adapter fail-closed. `docs/process-isolation.md`

### Q. Reliability, tail latency и operability

- [ ] 🟠 **944. Artifact failpoints:** hash/rename/manifest/fsync crash matrix.
- [ ] 🟠 **945. Deterministic lifecycle simulation:** replayable seeds для cancel/finish/retry/lease/shutdown.
- [ ] 🟠 **946. Network fault matrix:** latency/reset/partial/slow-close/redirect import tests.
- [ ] 🟠 **947. Tail histogram:** corrected p50/p95/p99 queue/probe/preview/render.
- [ ] 🟡 **948. Phase spans:** safe ingest→publish critical-path trace.
- [ ] 🟡 **949. User-facing SLO:** API, first preview frame и export completion отдельно.
- [ ] 🟠 **950. Class-aware shedding:** preview/upload/analysis/export gates и retry-after.
- [ ] 🟠 **951. Retry budget:** per-source/tool circuit breaker и observable half-open.
- [ ] 🟠 **952. Crash-only reconcile:** ownership/age/manifest-aware startup cleanup.
- [ ] 🟡 **953. Baseline comparator:** matching environment/schema + три regression runs.

### R. Timeline UI/UX и accessibility

- [ ] 🟠 **954. Semantic tokens:** theme/contrast contract, raw palette запрещена во features.
- [ ] 🟠 **955. Toolbar contract:** roving tabindex/arrows/labels/disabled reason.
- [ ] 🟡 **956. Command registry:** toolbar/menu/shortcut/palette используют одну command.
- [ ] 🟠 **957. Virtual timeline:** visible clips/tracks/markers + stable dimensions/anchor.
- [ ] 🟠 **958. Keyboard spatial editing:** nudge/resize/slip = pointer domain transaction.
- [ ] 🟠 **959. Timeline list alternative:** synchronized semantic representation для AT.
- [ ] 🟠 **960. Input parity:** mouse/touch/pen/keyboard gestures/cancel/capture cleanup.
- [ ] 🟠 **961. Overlay focus primitive:** trap/Escape/outside/focus return.
- [ ] 🟡 **962. Reduced motion:** state не передаётся только анимацией.
- [ ] 🟠 **963. Stateful a11y gate:** axe dialogs/menus/errors + keyboard/screen-reader matrix.

### S. Property, mutation и formal verification

- [ ] 🟠 **964. EditRequest properties:** normalize idempotence и plan invariants.
- [ ] 🟠 **965. Artifact properties:** generated paths/manifests/symlinks, identity/no escape.
- [ ] 🟠 **966. Job model:** generated commands сравнивают state machine и SQLite adapter.
- [ ] 🟠 **967. Kani arithmetic:** frame/sample/tick/chunk overflow/gap proofs.
- [ ] 🟡 **968. Mutation gate:** critical validators/retry/redaction/artifact checks.
- [ ] 🟡 **969. Nextest profiles:** unit/integration/media/slow/flaky policy.
- [ ] 🟡 **970. Disposable dependencies:** version-pinned host-sensitive integration tests.
- [ ] 🟠 **971. Golden media corpus:** VFR/HDR/rotation/channels/subtitles/corruption.
- [ ] 🟠 **972. Metamorphic tests:** split/merge, undo, proxy/original, chunk/stitch.
- [ ] 🟠 **973. Differential FFmpeg tests:** actual ffprobe/reference semantics.

### T. Local ML, privacy и model governance

- [ ] 🟠 **974. InferenceProvider:** local/remote typed port; remote отсутствует по умолчанию.
- [ ] 🟡 **975. Candle experiment:** CPU/GPU/RAM/binary-size/license matrix.
- [ ] 🟡 **976. tract experiment:** ONNX CPU adapter против subprocess за тем же port.
- [ ] 🟠 **977. Model descriptor:** checksum/source/license/version/task/languages/limitations.
- [ ] 🟠 **978. Explicit scope:** source/range/inputs/output показаны до inference.
- [ ] 🔴 **979. PII boundary:** local redaction или consent до remote transport.
- [ ] 🟠 **980. Ephemeral ML staging:** TTL/no-training/no-telemetry/audited deletion.
- [ ] 🟠 **981. Human confirmation:** confidence/provenance draft до `EditPlan` command.
- [ ] 🟡 **982. Evaluation corpus:** accuracy/latency/privacy drift gate model upgrades.
- [ ] 🟠 **983. Disposable ML artifacts:** source/model/params fingerprint, safe delete/recompute.

## P0 - Корректность (чинить первым)

> Полный аудит (топ-200 проблем по серьёзности) - в [docs/audit.md](docs/audit.md).
> Ниже - первоочередные баги из него.

### ☑ P0-1. `persist_job` при ошибке URL в импорте · S
- **Файлы:** `backend/src/handlers/mod.rs` (`import_handler`, ветка `validate_url`, ~53-59).
- **Шаги:**
  - [x] В ветке `if let Err(e) = tools::validate_url(...)` добавить `st.persist_job(&jid).await;` перед `st.clear_cancel(&jid).await;`.
  - [x] Тест в `backend/tests/api.rs`: импорт с плохим URL → poll до `error`; затем новый `AppState` поверх той же БД + `recover_jobs()` → джоба остаётся `error`, **не** `interrupted`.
- **Критерий:** новый тест зелёный; `make check` зелёный. **Сделано.**

### ☑ P0-2. Ретеншн `recover_jobs` · S
- **Файлы:** `backend/src/state.rs` (`recover_jobs`), `backend/src/db.rs` (`load_jobs`).
- **Шаги:**
  - [x] В `db.rs` добавить `load_recent_jobs(limit)` (ORDER BY updated_at DESC LIMIT) или `prune_jobs_older_than(ts)`.
  - [x] `recover_jobs` грузит только последние N (env `RECOVER_JOBS_LIMIT`, дефолт 200) и/или чистит терминальные старше TTL.
  - [x] db-тест: при >N сохранённых джобах загрузка возвращает ≤N.
- **Критерий:** db-тест зелёный; recover не падает на пустой/большой БД; `make check`. **Сделано.**

### ☑ P0-3. DNS-rebinding в `validate_url` · S/M
- **Файлы:** `backend/src/tools/net.rs`; (документация уже в architecture.md/README).
- **Шаги:**
  - [x] Резолвить host, прогнать каждый IP через `is_blocked_ip`; отклонять, если хоть один приватный.
  - [x] Закрыть edge cases: credentials в URL, decimal/octal/hex-like IP, CGNAT `100.64/10`, IPv4-mapped IPv6, `.local`/`.localhost`.
  - [x] Тесты через мокабельный resolver без зависимости от внешнего DNS.
- **Критерий:** DNS/private-host тесты зелёные; initial URL guard усилен. **Сделано.** Redirect-to-private позже закрыт отдельным P0-10.

### ☑ P0-4. Зависание джобы на закрытом семафоре · S
- **Файлы:** `backend/src/handlers/mod.rs` (ветки `acquire_owned() => Err`, ~66-69, 298-301).
- **Шаги:**
  - [x] В ветке `Err(_)` перевести джобу в `Error`, `persist_job`, `clear_cancel` (вместо голого `return`).
  - [x] `select!` ожидания permit против `token.cancelled()`, чтобы отмена «queued» прерывала очередь.
- **Критерий:** тест с закрытым semaphore зелёный; queued cancel теперь не ждёт permit. **Сделано.**

### ☑ P0-5. Целостность render_cache · S
- **Файлы:** `backend/src/handlers/mod.rs` (cache_get ~273), `backend/src/db.rs`, `backend/src/library.rs` (`remove`).
- **Шаги:**
  - [x] При промахе файла (`metadata` fail) удалять запись кэша (`cache_delete(key)`) и падать в обычный рендер.
  - [x] Инвалидировать кэш при `library.remove` для output-файлов по `filename`.
  - [x] Инвалидировать кэш при TTL-чистке output-файлов и удалять library entry через `Library::remove`.
- **Критерий:** тест «cache_put без файла → джоба идёт в рендер, не Done мгновенно» зелёный; delete-инвалидация покрыта HTTP-тестом; TTL теперь чистит файл/library/cache согласованно. **Сделано.**
- **Прим.:** изначальный пункт «пустой ключ при ошибке сериализации» снят - верификация показала, что `serde_json` пишет `null` для не-finite f64 (не ошибка), ключ не пустеет (audit.md, раздел опровергнутого).

### ☑ P0-6. Вырезание сегмента молча не работает для AV1/ProRes · S
- **Файлы:** `backend/src/tools/args.rs` (~234, 491-496), `frontend/src/store.ts` (~213-226).
- **Шаги:**
  - [x] Строить concat-ветку для всех видеоформатов (или явно запрещать сегменты в UI для AV1/ProRes с предупреждением).
  - [x] Тест: edit с `segments` + format=av1/prores даёт concat-аргументы, а не неразрезанный экспорт.
- **Критерий:** тест зелёный; пользовательский вырез не пропадает молча. **Сделано.**

### ☑ P0-7. Гонка cancel↔finish и очистка при отмене импорта · S
- **Файлы:** `backend/src/handlers/mod.rs` (`finish_job` ~427-459, `cancel_handler` ~382-409, import cancel ~104-106).
- **Шаги:**
  - [x] В `finish_job` гейтить запись статуса на `!j.status.is_terminal()` (как уже делает `cancel_handler`), чтобы отменённая задача не перезаписалась в `Done`.
  - [x] В ветках Cancelled/Err импорта подмести `sources/` по префиксу `vid` (+ `.info.json`, `.part`).
  - [x] Тест: late-cancel завершённой задачи не превращает её в Done; отменённый импорт не оставляет файлов.
- **Критерий:** тесты зелёные; `make check`. **Сделано.**

### ☑ P0-8. crop/geometry валидируется против размеров источника · S
- **Файлы:** `backend/src/tools/args.rs` (crop ~75-79), `backend/src/handlers/mod.rs` (probe ~331), `frontend/src/components/RectOverlay.vue` (~87-96).
- **Шаги:**
  - [x] Клампить crop/censor к `width/height` из probe; на бэкенде - перед `build_ffmpeg_args`.
  - [x] На бэкенде ограничить `fps` и валидировать `scale`, чтобы API не принимал заведомо невозможные значения.
  - [x] На фронте при эмите overlay: `x = min(x, W-w)`, чтобы `x+w ≤ W`.
- **Критерий:** backend-тест на клампинг/валидацию зелёный; frontend overlay clamp включён. **Сделано.**

> Прочие верифицированные 🔴/🟠 (upload минует семафор `handlers/mod.rs:142-229`;
> TTL-чистка без проверки ссылок `main.rs:103-135`; клампинг сегментов/fps в
> `args.rs`) - в [docs/audit.md](docs/audit.md), разделы A-D.
> Безопасность (auth, CORS, controlled egress, ресурсные лимиты ffmpeg) - отдельный
> трек, см. [docs/audit.md](docs/audit.md) §C/§E; обязателен перед выставлением наружу.

### ☑ P0-9. Upload принимает polyglot-файл, отдаёт как `text/html` - подтверждённый stored XSS · S/M
- **Найдено:** раунд 2 аудита, 1 июля 2026 (audit.md №202), подтверждено рабочим PoC (GIF-заголовок + `<script>`, имя `poc.html`, проходит `ffprobe`/`sanitize_ext`, отдаётся `/files/sources/<uuid>.html` как `text/html`).
- **Файлы:** `backend/src/handlers/upload.rs`, `backend/src/tools/mod.rs`, `backend/src/lib.rs`, `backend/tests/api.rs`.
- **Шаги:**
  - [x] Определять расширение/тип по фактическому содержимому (`ffprobe format_name` через allow-list), не по имени файла от клиента.
  - [x] Добавить `X-Content-Type-Options: nosniff` и sandbox CSP; MIME выводится только из server-selected расширения.
- **Критерий:** regression-тест загружает MP4 под именем `payload.html`, получает `.mp4`/`video/mp4` и оба защитных заголовка. **Сделано.**

### ☑ P0-10. SSRF-редирект `yt-dlp` на приватный адрес после успешной `validate_url` · M
- **Найдено:** раунд 2 аудита, 1 июля 2026 (audit.md №201); P0-3 закрыл DNS-резолвинг исходного URL, но не поведение `yt-dlp` (следование редиректам/собственный DNS-resolve).
- **Файлы:** `backend/src/tools/egress_proxy.rs`, `backend/src/tools/mod.rs`, `backend/src/tools/net.rs`, `.github/workflows/ci.yml`.
- **Шаги:**
  - [x] Запускать отдельный loopback HTTP/CONNECT proxy на import-job; каждый target заново резолвится, вся DNS-выборка валидируется, connect идёт к pinned `SocketAddr`.
  - [x] Принудить `yt-dlp` и дочерние downloader'ы к proxy через `--ignore-config`, `--proxy`, upper/lower-case proxy env и удаление `NO_PROXY`.
  - [x] Ограничить format selector только media URL `http://`/`https://`, чтобы RTMP/FTP/WebSocket не выбирали downloader вне HTTP proxy.
  - [x] Ограничить egress портами 80/443, DNS/connect budget и 64 соединениями; дать policy-deny транспортный 472 и отдельный atomic marker, не смешивая его с upstream 403/472.
  - [x] Закрепить `yt-dlp` в CI, чтобы реальный regression не превращался в silent skip.
- **Критерий:** реальный `yt-dlp` проходит разрешённый первый hop, получает 302 на loopback и завершается с policy 472; private sink не принимает соединение. Resolver-тест меняет public DNS answer на private между validation/connect и блокируется до TCP; synthetic info JSON выбирает HTTPS вместо RTMP и отклоняет RTMP-only. **Сделано.**

### ☑ P0-11. Отмена перед переходом в `Running` в import/edit-воркерах перетирается · S
- **Найдено:** раунд 2 аудита, 1 июля 2026 (audit.md №203); соседняя гонка, не закрытая `596327b`.
- **Файлы:** `backend/src/handlers/mod.rs` (~74-84, 300-310).
- **Шаги:**
  - [x] Заменить `queued`/`Running` и validation-error переходы на `update_job_if_open`; при `false` - ранний `return` без запуска процесса.
- **Критерий:** тест отменённой до старта job подтверждает, что `queued`/`Running` не возвращаются. **Сделано.**

---

## P1 - Безопасная модульность (поведение не меняется, под тестами)

### ☐ P1-4. `Config`-struct (env в одном месте) · M
- **Файлы:** новый `backend/src/config.rs`; `main.rs`; `state.rs` (поле `config`); `handlers/mod.rs` (убрать `job_timeout()`); `tools/mod.rs` (`download_video` берёт `max_height` параметром).
- **Шаги:**
  - [ ] `Config` + `from_env()` со всеми 8 переменными (PORT/BIND_ADDR/STORAGE_DIR/MAX_HEIGHT/MAX_CONCURRENT_JOBS/JOB_TIMEOUT_SECS/FILE_TTL_HOURS/MAX_UPLOAD_BYTES); `file_ttl: Option<Duration>` вместо магического 0; fail-fast на кривом BIND_ADDR/PORT.
  - [ ] `AppState` получает `Arc<Config>`; `main` собирает один раз.
  - [ ] Удалить `job_timeout()`, брать из `config`; `download_video(..., max_height)`.
- **Критерий:** дефолты и поведение те же; нет `std::env::var` вне `config.rs` (grep); тест `Config::from_env` без мутации процессного env; `make check`.

### ☐ P1-5. `store.ts` → модули · S→M
- **Файлы:** новые `frontend/src/lib/{time,quality,defaults}.ts`, `core/payload.ts`; `features/{presets,history}.ts`, `ui/useTheme.ts`; `store.ts` оставляет реэкспорты.
- **Шаги:**
  - [x] Чистые `parseTime`/`tierToCrf`/`defaultEdit`/`buildEditPayload` → `domain/edit.ts`, реэкспорт из `store.ts`.
  - [ ] `theme`/`presets`/`history` (+ их module-level `let`/watch) → свои файлы, реэкспорт.
  - [ ] Развязать `resetHistory`/`restoreProject` от `doImport`/`openFromLibrary` через событие смены `video` в core-модели.
- **Критерий:** публичные импорты сохранены; `store.test.ts`, `typecheck`/`build` зелёные; ручная проверка выполнена. Остальные два шага ещё открыты.

### ☐ P1-6. `EditPanel.vue` → секции · S→M
- **Файлы:** новый `frontend/src/lib/editOptions.ts`; `components/{AudioControls,PresetBar,ColorControls,TimingControls,FrameControls,ExportControls}.vue`; `EditPanel.vue` - тонкий контейнер.
- **Шаги:**
  - [ ] 10 каталогов опций (`speeds`/`aspects`/`filters`/`formats`/…) → `lib/editOptions.ts`.
  - [x] Вынести export-секцию и no-op confirmation в `components/edit/ExportControls.vue`.
  - [ ] Секции по одной (начать с `AudioControls`/`PresetBar` - они без скрытых связей).
  - [ ] Вынести `applyPlatform`/`setAspect` в store/lib (развязка Export→Frame).
- **Критерий:** `typecheck`/`build`; ручная проверка каждой секции в превью (как в прошлых фичах через `window.__store`).

### ☐ P1-7. Единый источник дефолтов контракта · S
- **Файлы:** `frontend/src/store.ts` (или `lib/defaults.ts`), `store.test.ts`.
- **Шаги:**
  - [x] `EDIT_DEFAULTS` (его же отдаёт `defaultEdit()`) в `domain/edit.ts`.
  - [ ] Плоскую часть `buildEditPayload` (строки «if e.x !== default») заменить на `diffFromDefault`.
  - [ ] `PRESET_KEYS` вывести из списка скалярных полей (не вручную).
- **Критерий:** `store.test.ts` зелёный (payload идентичен); добавить тест «`defaultEdit()` == `EDIT_DEFAULTS`».

### ☐ P1-8. `messages.rs` (i18n-каталог) · S→M
- **Файлы:** новый `backend/src/messages.rs`; `tools/{mod,net}.rs`, `handlers/*` (use `messages::`).
- **Шаги:**
  - [ ] Каталог ключей (Timeout/PrivateVideo/GeoBlocked/NotFound/BadUrl/…) + русские тексты в одном месте.
  - [ ] Заменить захардкоженные строки в домене на `messages::*`.
- **Критерий:** тексты не меняются (тесты, что ждут конкретные строки, напр. `"Недопустимый URL"`, зелёные); `make check`.

---

## P2 - Глубокий рефактор (меняет контракты/контрол-флоу, нужны новые тесты)

### ☐ P2-9. JobRunner + трейт `Task` · L  ← закрывает P0-1 и убирает дублирование
- **Файлы:** новый `backend/src/jobs/{mod,runner,task}.rs`; `handlers/mod.rs` (`import`/`edit` переписать); перенести `spawn_progress_drain`/`finish_job`.
- **Шаги:**
  - [ ] `trait Task { async fn run(&self, ctx) -> Result<Option<Value>> }` + `JobContext{progress,cancel}`.
  - [ ] `JobService::spawn(id, spec, task)` - единственное место со скелетом (queued→permit→cancel→running→drain→finish→persist).
  - [ ] `import`/`edit` строят `Task` и зовут `spawn`; `validate_url` как `Err` внутри `work` (фикс P0-1).
  - [ ] Юнит-тесты раннера: отмена в `queued`, отсутствие зависания (`drop(tx)`), три исхода `Ok(Some)/Ok(None)/Err`.
- **Критерий:** `tests/api.rs` без изменений зелёный; новые runner-тесты; `make check`.

### ☐ P2-10. Репозитории-трейты · M→L
- [ ] **Шаг A (M):** `ProjectRepo`/`JobRepo`/`MediaRepo`/`RenderCache` поверх существующего `Db`; `AppState` на `Arc<dyn ...>`; in-memory реализации + переписать 1-2 теста хендлеров на них.
- [ ] **Шаг B (L):** `Library` (JSON) → таблица `media` в SQLite + `FileStore` (file-IO отдельно); одноразовая миграция `library.json` + тест миграции.
- **Критерий:** существующие тесты зелёные; новый in-memory тест хендлера без sqlite.

### ◐ P2-11. `AppError` + `IntoResponse` · L
- **Файлы:** новый `error.rs`; хендлеры; домен (строки → варианты).
- [x] `AppError` + `IntoResponse`; убрать россыпь `(StatusCode, String)` и унифицировать extractor/routing errors.
- [ ] Ввести `ErrorKind` для асинхронного `job.error`, не смешивая его с HTTP boundary.
- **Критерий:** статус-коды в `tests/api.rs` не меняются.

### ☐ P2-12. `JobService` (инвариант завершения) · M
- [ ] `transition(id, f)` сам персистит на терминальном статусе + `clear_cancel`; убрать ручные `persist_job` из 5 мест.
- **Критерий:** тест «терминал → токен очищен + статус в БД».

### ☐ P2-13. Timeline IR · L (разблокирует мультитрек)
- [x] **Шаг 1 (S):** вынести validated `OutputSpec` из плоских format/codec/quality.
- [x] **Шаг 2A (M):** `EditRequest -> EditPlan` mapper; FFmpeg adapter принимает только `&EditPlan` через port.
- [ ] **Шаг 2B (M):** перевести render cache lookup/single-flight с wire JSON на `plan_fingerprint`.
- [ ] **Шаг 3 (L):** `Scope`/диапазоны + мультитрек в компиляторе.
- **Критерий:** golden-тест «старый `build_ffmpeg_args` == новый» на корпусе запросов.

---

## P3 - Фичи (параллельно, из round-13)

### ☐ Тир-1 (ложатся на текущий `build_ffmpeg_args`, S-M каждая)
- [ ] Хромакей (зелёный экран) + despill · M
- [ ] LUT-импорт `.cube` + интенсивность · M
- [ ] Стабилизация `vidstab` (двухпроходная) · M
- [ ] Scopes: гистограмма/waveform/vectorscope (бэк рендерит PNG по кадру) · M
- [ ] Режим «до/после» слайдером · M
- [ ] Авто-обрезка чёрных полос (`cropdetect`) · S
- [ ] Boomerang-экспорт · S

### ☐ Тир-0 (разблокировщик) - только после P2-13
- [ ] Мультитрек-таймлайн · L `[нужен Timeline IR]`

---

## Рекомендуемая последовательность

```
P0 (часы)  →  P1 #4-#8 (модульность, безопасно)  →  P2 #9 JobRunner
           →  #10 репозитории  →  #11 AppError  →  #12 JobService  →  #13 Timeline IR
Тир-1 фичи (P3) — в любой момент;  мультитрек — после #13.
```

Самый высокий ROI прямо сейчас: **P0 целиком** + **P1 #4/#5/#6**. Самый ценный
крупный шаг: **#9 JobRunner** (дедуп + фикс бага), затем **#13 Timeline IR**
(мультитрек + половина фич-бэклога).
