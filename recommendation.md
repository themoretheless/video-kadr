# Рекомендации: что делать дальше (чеклист)

Приоритизированный, гранулярный план, синхронизированный с
[architecture.md](architecture.md) (целевой дизайн),
[docs/refactor-plan.md](docs/refactor-plan.md) (шаги рефакторинга) и
[docs/ideas/round-13.md](docs/ideas/round-13.md) (фичи).

Порядок: **корректность → дешёвая модульность → глубокий рефактор**; фичи
параллельно. Каждая задача идёт под зелёным `make check`. **S** = часы, **M** =
день-два, **L** = неделя+. У каждой задачи - файлы, шаги, критерий приёмки.

## Синхронизированный top-200: проблема → куда чинить

Номера совпадают с диагностическим списком в `architecture.md`; подробные
file:line и доказательства - в [docs/audit.md](docs/audit.md).

1. Cancel/finish race → P0-7 и P2-12.
2. Мусор после отмены/таймаута импорта → P0-7.
3. Cancel окна на cache-hit edit → P2-12.
4. Осиротевший child `ffmpeg` из `yt-dlp` → P2-9.
5. Shutdown бросает workers/processes → P2-9.
6. Queued job нельзя отменить сразу → P0-4 и P2-9.
7. `acquire_owned()` error оставляет job non-terminal → P0-4.
8. Progress drain пишет в terminal job → P2-9/P2-12.
9. Segments не работают для AV1/ProRes → P0-6.
10. Crop не валидируется по source dimensions → P0-8.
11. Overlay может выйти за кадр → P0-8.
12. Сегменты не сортируются → P0-6/P2-13.
13. Segment end не клампится к duration → P0-6/P2-13.
14. Negative segment start → P0-6/P2-13.
15. Overlapping segments → P0-6/P2-13.
16. `fps` без bounds → P0-8/P2-13.
17. `scale` без строгой validation → P0-8/P2-13.
18. Upload минует semaphore → P0 backlog/P2-9.
19. Нет resource limits ffmpeg/yt-dlp → security/perf track.
20. Нет no-progress watchdog → P2-9.
21. Нет filesize/duration import cap → security/perf track.
22. Partial upload после multipart error → P0 backlog.
23. Progress tick берёт global jobs mutex → P2-9/P2-12.
24. TTL удаляет referenced/active files → P0 backlog/P2-10.
25. Render cache не инвалидируется → P0-5/P2-10.
26. Нет DB migrations → P2-10.
27. Нет jobs/cache retention → P0-2/P2-10.
28. `library.json` и SQLite дрейфуют → P2-10.
29. Нет single-flight render cache → P2-10.
30. `cache_put` и `library.add` не атомарны → P2-10.
31. Persist errors глотаются → P2-12.
32. Нет DB indexes для retention/sort → P0-2/P2-10.
33. DNS SSRF bypass → P0-3.
34. Нет auth → before-exposure security track.
35. `ServeDir` отдаёт весь `storage` → before-exposure security track.
36. Permissive CORS → before-exposure security track.
37. Неполный special-use IP blocklist → P0-3.
38. Upload без magic-byte validation → security track.
39. Projects JSON без схемы/лимита → P2-10/P2-11.
40. Нет `deny_unknown_fields` → P2-11/contract work.
41. Сырой stderr наружу → P1-8/P2-11.
42. Разные формы API errors → P2-11.
43. DB errors без нормального body/log → P2-11.
44. Async jobs отвечают `200`, не `202` → P2-11.
45. Frontend маскирует real 500 → P1-5/frontend API cleanup.
46. God store + god `EditPanel.vue` → P1-5/P1-6.
47. Watchers как import side effects → P1-5.
48. Runtime validation отсутствует для JSON/presets → P1-5/P1-7.
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
76. Status code drift → AppError/API policy.
77. Cancel response ignored → frontend API cleanup.
78. 5xx/network mixed → frontend API error model.
79. No AbortController/timeouts → cancellable API client.
80. Polling no backoff/jitter → jobs UI/client refactor.
81. Polling no deadline → jobs client deadline.
82. No idempotency keys → future API hardening.
83. Upload sync-flow differs → upload-as-job or documented exception.
84. No jobs list endpoint → jobs history endpoint/UI.
85. No request correlation id → request-id middleware.
86. Error body shape not unified → AppError + typed ApiError.
87. Plain-text errors possible → convert all handlers to ApiError.
88. No pagination/search → library/projects API pagination.
89. No API versioning → `/api/v1` or schema version policy.
90. README API drift → docs/API contract check.
91. `EditRequest` triple role → DTO → `EditPlan` mapper.
92. Effects as scalars/bools → typed `Effect` enum.
93. Raw format/codec strings → enums/newtypes.
94. Missing `OutputSpec` → P2-13 step 1.
95. Missing `TimeRange` → domain validation refactor.
96. Missing `CropRect` → geometry validation refactor.
97. Missing `ScaleSpec` → geometry validation refactor.
98. Bounds scattered → central validation policy.
99. Numeric edge policy absent → validation corpus.
100. Duration tied to builder → `EditPlan` duration calculation.
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
128. Missing `AppError` → P2-11.
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
171. No threat model → security design note.
172. No auth → before-exposure auth middleware.
173. No CSRF posture → auth/session design.
174. No rate limiting → middleware/queue limits.
175. No per-user isolation → multi-user architecture later.
176. No process sandbox → container/seccomp/resource limits.
177. No URL domain policy → configurable allow/deny list.
178. No redirect policy → downloader adapter constraints.
179. No centralized redaction → redaction module.
180. Query tokens not unified → URL redaction/warning policy.
181. No redacted diagnostics bundle → diagnostics endpoint.
182. Upload magic bytes missing → media probe before publish.
183. No quarantine story → upload staging directory.
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

### ☐ P0-3. DNS-rebinding в `validate_url` · S (пометка) / M (резолвинг)
- **Файлы:** `backend/src/tools/net.rs`; (документация уже в architecture.md/README).
- **Шаги:**
  - [ ] Минимум (S): явный комментарий-ограничение в `net.rs` + строка в «Ограничениях» README.
  - [ ] Полноценно (M): резолвить host (`(host, 0).to_socket_addrs()`), прогнать каждый IP через `is_blocked_ip`; отклонять, если хоть один приватный.
- **Критерий:** для S - комментарий + тест текущего поведения; для M - тест, что хост, резолвящийся в loopback/RFC1918, отклоняется (за фичей/мокабельным резолвером).

### ☑ P0-4. Зависание джобы на закрытом семафоре · S
- **Файлы:** `backend/src/handlers/mod.rs` (ветки `acquire_owned() => Err`, ~66-69, 298-301).
- **Шаги:**
  - [x] В ветке `Err(_)` перевести джобу в `Error`, `persist_job`, `clear_cancel` (вместо голого `return`).
  - [x] `select!` ожидания permit против `token.cancelled()`, чтобы отмена «queued» прерывала очередь.
- **Критерий:** тест с закрытым semaphore зелёный; queued cancel теперь не ждёт permit. **Сделано.**

### ◐ P0-5. Целостность render_cache · S
- **Файлы:** `backend/src/handlers/mod.rs` (cache_get ~273), `backend/src/db.rs`, `backend/src/library.rs` (`remove`).
- **Шаги:**
  - [x] При промахе файла (`metadata` fail) удалять запись кэша (`cache_delete(key)`) и падать в обычный рендер.
  - [x] Инвалидировать кэш при `library.remove` для output-файлов по `filename`.
  - [ ] (вместе с P2-10) инвалидировать кэш при TTL-чистке.
- **Критерий:** тест «cache_put без файла → джоба идёт в рендер, не Done мгновенно» зелёный; delete-инвалидация покрыта HTTP-тестом; TTL ещё впереди.
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

### ◐ P0-8. crop/geometry валидируется против размеров источника · S
- **Файлы:** `backend/src/tools/args.rs` (crop ~75-79), `backend/src/handlers/mod.rs` (probe ~331), `frontend/src/components/RectOverlay.vue` (~87-96).
- **Шаги:**
  - [x] Клампить crop/censor к `width/height` из probe; на бэкенде - перед `build_ffmpeg_args`.
  - [x] На бэкенде ограничить `fps` и валидировать `scale`, чтобы API не принимал заведомо невозможные значения.
  - [ ] На фронте при эмите overlay: `x = min(x, W-w)`, чтобы `x+w ≤ W`.
- **Критерий:** backend-тест на клампинг/валидацию зелёный; frontend overlay clamp ещё впереди.

> Прочие верифицированные 🔴/🟠 (upload минует семафор `handlers/mod.rs:142-229`;
> TTL-чистка без проверки ссылок `main.rs:103-135`; клампинг сегментов/fps в
> `args.rs`) - в [docs/audit.md](docs/audit.md), разделы A-D.
> Безопасность (auth, CORS, SSRF-резолвинг, ресурсные лимиты ffmpeg) - отдельный
> трек, см. [docs/audit.md](docs/audit.md) §C/§E; обязателен перед выставлением наружу.

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
  - [ ] Чистые `parseTime`/`tierToCrf`/`defaultEdit`/`buildEditPayload` → lib/core, реэкспорт из `store.ts`.
  - [ ] `theme`/`presets`/`history` (+ их module-level `let`/watch) → свои файлы, реэкспорт.
  - [ ] Развязать `resetHistory`/`restoreProject` от `doImport`/`openFromLibrary` через событие смены `video` в core-модели.
- **Критерий:** `store.test.ts` без изменений зелёный; `typecheck`/`build`; ручная проверка undo/тема/пресеты в превью.

### ☐ P1-6. `EditPanel.vue` → секции · S→M
- **Файлы:** новый `frontend/src/lib/editOptions.ts`; `components/{AudioControls,PresetBar,ColorControls,TimingControls,FrameControls,ExportControls}.vue`; `EditPanel.vue` - тонкий контейнер.
- **Шаги:**
  - [ ] 10 каталогов опций (`speeds`/`aspects`/`filters`/`formats`/…) → `lib/editOptions.ts`.
  - [ ] Секции по одной (начать с `AudioControls`/`PresetBar` - они без скрытых связей).
  - [ ] Вынести `applyPlatform`/`setAspect` в store/lib (развязка Export→Frame).
- **Критерий:** `typecheck`/`build`; ручная проверка каждой секции в превью (как в прошлых фичах через `window.__store`).

### ☐ P1-7. Единый источник дефолтов контракта · S
- **Файлы:** `frontend/src/store.ts` (или `lib/defaults.ts`), `store.test.ts`.
- **Шаги:**
  - [ ] `EDIT_DEFAULTS` (его же отдаёт `defaultEdit()`).
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

### ☐ P2-11. `AppError` + `IntoResponse` · L
- **Файлы:** новый `error.rs`; хендлеры; домен (строки → варианты).
- [ ] enum `AppError{BadRequest(Reason)/NotFound/Conflict/Internal}` + `IntoResponse`; `ErrorKind` для `job.error`; убрать россыпь `(StatusCode, String)`.
- **Критерий:** статус-коды в `tests/api.rs` не меняются.

### ☐ P2-12. `JobService` (инвариант завершения) · M
- [ ] `transition(id, f)` сам персистит на терминальном статусе + `clear_cancel`; убрать ручные `persist_job` из 5 мест.
- **Критерий:** тест «терминал → токен очищен + статус в БД».

### ☐ P2-13. Timeline IR · L (разблокирует мультитрек)
- [ ] **Шаг 1 (S):** вынести `OutputSpec` (enum) из плоских format/codec/quality.
- [ ] **Шаг 2 (M):** `EditRequest -> EditPlan` адаптером; билдер на `&EditPlan`; ключ кэша по хешу плана.
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
