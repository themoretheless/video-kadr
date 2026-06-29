# Архитектура

Документ описывает: (1) **текущую** структуру, (2) **целевой модульный дизайн**
«как если бы строили с нуля» с упором на слабую зацепленность, (3) **правила
зависимостей**. Синтез 10-критического ревью; пошаговый путь миграции -
в [docs/refactor-plan.md](docs/refactor-plan.md).

## Принципы

1. **Слои с однонаправленными зависимостями.** HTTP → сервисы/домен →
   инфраструктура. Внутренний слой не знает о внешнем. Никаких циклов между модулями.
2. **Чистое ядро.** Доменная логика (сборка ffmpeg-аргументов, валидация,
   модель правок) - чистые функции без I/O, юнит-тестируемые без процессов и БД.
3. **Зацепление через узкие контракты.** Хендлер/сервис зависит от трейта-порта,
   а не от конкретного типа (`Db`, `Library`). Контракт виден в сигнатуре.
4. **Один источник правды на сущность.** Конфиг - один `Config`; контракт правок -
   одно описание; инвариант задачи - в одном месте.
5. **Каждый модуль меняется по одной причине** (single responsibility).

## Обзор системы

```
Браузер (Vue 3 + TS, без UI-библиотек)
   │  fetch /api/*  (dev: Vite proxy → :8080)
   ▼
Backend (Rust + Axum + Tokio)
   ├─ HTTP-слой        тонкие хендлеры: запрос ⇄ ответ
   ├─ Задачи           асинхронные джобы (импорт/рендер): очередь, прогресс, отмена
   ├─ Домен            модель правок → компилятор в ffmpeg filter_complex
   ├─ Инфраструктура   ffmpeg/ffprobe/yt-dlp (шелл), SQLite (sqlx), файлы на диске
   └─ Хранилище        storage/{sources,outputs}/  + storage/app.db
```

Импорт и экспорт идут фоновыми задачами; фронт опрашивает статус. Проекты, задачи
и кэш рендеров персистятся в SQLite и переживают рестарт (см. README).

## Текущая структура

**Backend** (`backend/src/`):
- `lib.rs` - `build_router()` (сборка маршрутов), реэкспорт модулей.
- `main.rs` - bootstrap: env, probe инструментов, открыть БД, recover задач, `serve`.
- `handlers/` - HTTP. `mod.rs` (import/edit/jobs + общая job-машинерия:
  `finish_job`/`spawn_progress_drain`/`job_timeout`), `projects.rs`, `library.rs`,
  `health.rs` (вынесены как независимые группы).
- `tools/` - внешние инструменты. `args.rs` (чистая сборка ffmpeg-аргументов +
  тесты), `net.rs` (SSRF-валидация URL), `mod.rs` (process/download/probe I/O,
  реэкспорт публичного API).
- `state.rs` - `AppState`: jobs (in-memory) + cancels + semaphore + tools + library
  + db + storage.
- `db.rs` - sqlx/SQLite: projects, jobs, render_cache.
- `library.rs` - медиатека (JSON-файл, параллельно БД).
- `model.rs` - `Job`/`JobStatus`, `EditRequest` (плоский DTO+домен), `Trim`/`Crop`/`Scale`.

**Frontend** (`frontend/src/`):
- `store.ts` - единый reactive `state` + все экшены (импорт/экспорт/библиотека/
  проекты/история/пресеты/тема/плеер) + чистые `parseTime`/`tierToCrf`/`buildEditPayload`.
- `components/` - `EditPanel.vue` (god-компонент со всеми контролами), `VideoPreview`,
  `RectOverlay`, `TrimSlider`, `MediaLibrary`, `UrlImport`, `ResultPanel`, `Toasts`,
  `ProgressBar`.
- `api.ts` - HTTP-клиент. `types.ts` - `EditState` (зеркало `EditRequest`).

Уже выровнено по целевому дизайну: `tools/{args,net,mod}` и
`handlers/{projects,library,health}` (чистое перемещение, см. refactor-plan).

## Top-200: что сделано плохо или неправильно

Источник правды - [docs/audit.md](docs/audit.md). Ниже та же первая
приоритизированная двухсотка, синхронизированная с `recommendation.md`.
`docs/audit.md` хранит расширенный список, file:line и опровергнутые находки.

### Correctness и гонки задач

1. `cancel_handler` и `finish_job` гоняются: отменённая задача может стать `Done`.
2. Отмена или таймаут импорта оставляет частично скачанные файлы в `sources/`.
3. Кэш-хит edit может быть отменён в окне между cancel и записью `Done`.
4. `ffmpeg`, порождённый через `yt-dlp`, может осиротеть при cancel/timeout.
5. Graceful shutdown закрывает HTTP, но бросает in-flight workers и child processes.
6. Ожидание permit в очереди не отменяемо.
7. Ошибка `acquire_owned()` оставляет job в non-terminal статусе.
8. Progress drain продолжает писать progress в терминальную job.

### Геометрия и сегменты

9. Вырезание сегмента молча не работает для AV1/ProRes.
10. Crop не валидируется против размеров источника.
11. Overlay может округлить `x+w` за пределы ширины кадра.
12. Сегменты не сортируются, пользовательский порядок ломает timeline.
13. Концы сегментов не клампятся к duration.
14. Отрицательный start сегмента может уйти в ffmpeg.
15. Перекрывающиеся сегменты не отклоняются и дублируют кадры.
16. `fps` не ограничен сверху/снизу и может устроить CPU/memory blow-up.
17. `scale` принимает небезопасные отрицательные и нечётные размеры.

### Ресурсы и DoS

18. `upload_handler` минует общий `jobs_semaphore`.
19. Для ffmpeg/yt-dlp нет CPU/RAM/threads/filesize лимитов.
20. Нет watchdog-а по отсутствию progress.
21. Нет cap на размер и длительность импорта.
22. Partial upload может остаться на диске после ошибки multipart/body limit.
23. Каждый progress tick берёт глобальный jobs mutex.

### Данные и целостность

24. TTL-чистка удаляет файлы по mtime без проверки ссылок и активных jobs.
25. `render_cache` не инвалидируется при удалении/пропаже output-файла.
26. SQLite schema version пишется, но миграций нет.
27. Jobs/render_cache растут без retention, `recover_jobs` грузит всё в память.
28. `library.json` и SQLite живут параллельно и могут дрейфовать.
29. Нет single-flight для одинаковых параллельных renders.
30. `cache_put` и `library.add` не атомарны.
31. Ошибки persist job глотаются.
32. Нет индексов для сортировки/retention jobs и render_cache.

### Security

33. SSRF-валидация не резолвит DNS, `yt-dlp` может уйти в private IP.
34. Нет authentication на мутирующих endpoints.
35. `ServeDir` отдаёт весь `storage`, включая потенциально чувствительные файлы.
36. `CorsLayer::permissive()` открыт для mutating requests.
37. Blocklist приватных диапазонов неполный.
38. Upload доверяет расширению/контейнеру до проверки magic bytes.
39. Projects API хранит произвольный JSON без схемы и лимита.
40. Request DTO не используют `deny_unknown_fields`.

### API, frontend и тесты

41. Сырой stderr ffmpeg/yt-dlp может попасть в `job.error`.
42. Project endpoints возвращают разные формы ошибок.
43. DB errors местами превращаются в голый 500 без тела и лога.
44. Async job creation отвечает `200`, а не `202 Accepted`.
45. Frontend маскирует реальные HTTP 500 как «backend down».
46. Frontend `store.ts` и `EditPanel.vue` остаются god-module/god-component.
47. Store watchers регистрируются как side effect импорта модуля.
48. Presets/project/job JSON приводятся через `as` без runtime validation.
49. Есть проглоченные `catch {}`, a11y debt и unsafe `root.value!`.
50. Тесты не покрывают orchestration render path, async store actions и API
    contract snapshots.

### Конфиг, сборка и эксплуатация

51. `BIND_ADDR` при ошибке парсинга тихо откатывается на localhost.
52. Числовые env-переменные fail-soft: опечатка молча превращается в дефолт.
53. Env читается в нескольких местах вместо одного `Config`.
54. Нет тестируемой `Config`-структуры с явными defaults и validation.
55. Docker тянет `yt-dlp`/ffmpeg без строгого pinning и checksum.
56. Backend container не имеет non-root user, healthcheck и resource limits.
57. CI не ставит `yt-dlp`, поэтому import path покрыт хуже render path.
58. README, Docker и CI могут разойтись по версии Node/Rust/toolchain.
59. Не зафиксирован MSRV/Rust toolchain для backend.
60. `RUST_LOG`/tracing policy не документированы для диагностики.
61. Нет structured job lifecycle logs как отдельного observability contract.
62. Нет metrics endpoint для длительности jobs, ошибок, очереди и cache hit rate.
63. Health endpoint смешивает readiness/liveness и не различает degradation классы.
64. Health не проверяет, что storage и SQLite реально writable.
65. CORS/security policy не управляется конфигом по окружениям.
66. Served media и internal runtime files живут в одном `storage` namespace.
67. Cleanup не имеет dry-run/report mode и явного журнала удалений.
68. Docker Compose не задаёт CPU/memory/pids limits для тяжёлых media процессов.
69. Нет backup/restore инструкции для `storage/app.db` и media файлов.
70. Нет rollback strategy для будущих DB/schema migrations.

### API и контракт

71. Нет OpenAPI/schema source of truth в текущем `main`.
72. TypeScript DTO поддерживаются вручную и не генерируются из backend contract.
73. `ProjectDto` объявлен в `api.ts`, а не в едином `types.ts`/generated module.
74. Response DTO не типизированы на backend: много ручного `json!`.
75. Handlers вручную собирают JSON вместо typed response structs.
76. Status codes не нормализованы: async jobs отвечают 200, delete допускает 404 как success.
77. `cancelJob` на frontend игнорирует HTTP статус и тело ответа.
78. `safeFetch` смешивает network failure и backend 5xx.
79. Frontend requests не имеют `AbortController`/timeout на уровне API client.
80. Polling interval фиксирован, без backoff/jitter.
81. Polling не имеет max attempts/deadline на зависшие jobs.
82. Нет idempotency keys для import/edit/upload.
83. Upload sync-flow отличается от job-based import/edit.
84. Нет `GET /api/jobs` для списка jobs и диагностики очереди.
85. Нет request-id/correlation-id в API responses и logs.
86. Error body shape не единый между resources.
87. Plain-text errors всё ещё возможны в projects/upload paths.
88. Library/projects endpoints без pagination/filter/search contract.
89. API не версионирован (`/api/v1` или schema version отсутствуют).
90. README API блок отстаёт от фактических ошибок, cache и jobs details.

### Доменная модель edit/render

91. `EditRequest` совмещает wire DTO, domain input и cache-key normalization.
92. Эффекты представлены россыпью bool/scalar полей вместо typed effect enum.
93. `format`, `codec`, `filter`, `pad`, colors остаются raw strings.
94. Нет `OutputSpec` как отдельной политики формата/кодека/качества.
95. Нет `TimeRange` type с invariant `start < end`.
96. Нет `CropRect` type с invariant внутри source dimensions и even dimensions.
97. Нет `ScaleSpec` type с допустимыми sentinel values `-1/-2`.
98. Speed/volume/fade/fps bounds не централизованы в domain policy.
99. Non-finite/edge numeric inputs не описаны как отдельная validation policy.
100. Расчёт output duration зависит от builder logic, а не от domain plan.
101. Render cache key считается от serialized request, а не от normalized `EditPlan`.
102. Cache key не учитывает ffmpeg version/capabilities/pipeline version.
103. Cache policy не различает source provenance: URL import vs upload.
104. Нет typed `MediaId`/`ProjectId`/`CacheKey`.
105. Project хранит полный video JSON blob вместо нормализованной связи на media.
106. Project edit JSON не имеет versioned schema/migration path.
107. Defaults UI/edit не имеют явной версии для старых проектов.
108. Autosave project не имеет rollback/history.
109. Нет доменного audit trail для изменений проекта.
110. Нет typed domain errors для edit validation.
111. Backend capabilities не оформлены как contract для frontend controls.
112. Preview fidelity не связана с backend compile policy.
113. Source/output media kind не представлен отдельной domain enum везде.
114. Нет corpus-а canonical edit recipes для regression testing.
115. Новое поле edit всё ещё требует правок в нескольких местах.
116. Platform presets живут как UI logic, а не как domain/export presets.
117. Quality tiers не представлены как общая таблица backend/frontend.
118. Нет formal compatibility policy для старых сохранённых проектов.
119. Timeline IR описан в архитектуре, но ещё не является кодовой границей.
120. Domain model не готова к multi-track/range effects без большого переписывания.

### Backend модульность и тестируемость

121. `backend/src/tools/args.rs` остаётся слишком крупным compile god-file.
122. `backend/src/handlers/mod.rs` всё ещё хранит общую job-машинерию.
123. `backend/src/db.rs` смешивает schema, SQL, DTO mapping и tests.
124. `backend/src/library.rs` остаётся JSON repository рядом с SQLite repository.
125. `lib.rs` строит router с конкретными implementations вместо ports.
126. Нет traits для `ProjectRepo`, `JobRepo`, `MediaRepo`, `RenderCache`.
127. Нет in-memory repositories для быстрых service tests.
128. Нет `AppError`/`IntoResponse` как единого error boundary.
129. Нет `messages.rs`/i18n boundary для пользовательских строк.
130. Process runner и parser progress всё ещё слабо отделены от tool facade.
131. Нет `JobService::spawn` как единственного скелета queued/running/finish.
132. Нет explicit state machine для Job transitions.
133. Cancel tokens живут рядом с app state, а не внутри jobs subsystem.
134. Persist-on-terminal invariant не закреплён типом/service API.
135. Нет тестов на TTL cleanup и active-file protection.
136. Нет тестов на upload concurrency/backpressure.
137. Нет property tests для geometry/timeline normalization.
138. Нет API snapshot tests для JSON response shapes.
139. Нет fuzz/negative corpus для request DTO.
140. Нет benchmark/perf baseline для ffmpeg args builder и large library/projects.

### Frontend архитектура и UX

141. Нет component tests для editor panels.
142. Нет browser/E2E smoke на import/open/edit/export UI flow.
143. Нет route/lazy boundary для проектов, медиатеки и будущего audit/ideas surface.
144. Нет error boundary для неожиданных runtime ошибок UI.
145. Toasts не имеют action/retry и не связаны с API error codes.
146. Toast history/debug context не сохраняется.
147. Global keyboard handler живёт в `App.vue`, а не в `useShortcuts`.
148. Shortcut handling не учитывает все a11y/IME/composition случаи.
149. После upload/open/delete нет системного focus management.
150. Drag/drop upload не имеет client-side size/type preflight.
151. URL import warning не связан с backend URL policy.
152. `pollJob` нельзя отменить при unmount/смене клипа.
153. `setTimeout` polling не хранит handle для cleanup.
154. Import/export status fields дублируют друг друга вместо job view model.
155. Нет UI для persisted jobs history.
156. Media library без search/filter/pagination.
157. Delete media/project без undo/confirm для destructive flow.
158. Result panel и media library не имеют unified download/open actions model.
159. LocalStorage presets без size limit/quota handling UX.
160. Theme хранится локально, но нет first-class system preference mode.
161. Нет reduced-motion/high-contrast accessibility pass.
162. Controls не вынесены в reusable primitives (`Field`, `SegmentedControl`, `Slider`).
163. Options arrays живут в `EditPanel.vue`, а не в `lib/editOptions`.
164. Export hints смешаны с component computed logic.
165. Autosave/project restore side effects не изолированы в composable.
166. Frontend API client не валидирует runtime shape ответов.
167. `window.__store` DEV hook полезен, но не описан как debug-only contract.
168. CSS/design tokens не описаны как система.
169. Нет visual regression/screenshots для responsive editor.
170. Frontend не готов к локализации строк.

### Security и privacy beyond local MVP

171. Нет threat model для локального vs exposed deployment.
172. Нет authentication middleware и token story.
173. Нет CSRF posture для future cookie/session mode.
174. Нет rate limiting на import/upload/edit.
175. Нет per-user isolation для проектов, jobs и files.
176. Нет sandbox profile для ffmpeg/yt-dlp.
177. Нет allowlist/denylist policy для supported URL domains.
178. Нет redirect policy для yt-dlp после URL validation.
179. Нет centralized redaction для logs/errors/API.
180. Query tokens в импортируемых URL не редактируются как единая policy.
181. Нет diagnostics bundle с гарантированной redaction.
182. Upload не проверяет magic bytes до публикации файла через `/files`.
183. Нет malware/quarantine story для uploaded media.
184. File serving не ставит explicit safe `Content-Disposition`.
185. Нет audit log для mutating operations.

### Storage, data lifecycle и operations

186. Нет quota/usage reporting по `sources`, `outputs`, SQLite и cache.
187. Нет checksum/integrity metadata для source/output files.
188. Нет orphan scanner/repair tool для storage/library/db/cache.
189. Нет backup/export/import command для проектов и медиатеки.
190. Нет DB maintenance story: vacuum/analyze/checkpoint.
191. Нет transaction boundary для file write + DB/cache/library update.
192. Нет media table с FK на projects/jobs/render_cache.
193. Cleanup не считает reference counts для files.
194. Disk-full ошибки не превращаются в понятный user-facing state.
195. Startup не проверяет permissions/free space заранее.
196. Нет filesystem lock для защиты от двух backend процессов на одном storage.
197. Нет retention policy для проектов.
198. Нет retention policy для старых source files отдельно от outputs.
199. Нет runbook для ручного восстановления после broken DB/storage drift.
200. Нет регулярного docs/code drift check, который гарантирует актуальность этой двухсотки.

## Целевой модульный дизайн (backend)

Дерево по слоям, стрелка = «зависит от» (только вниз):

```
http/                 тонкий HTTP-слой
  mod.rs              routes(): Router из под-роутеров
  import.rs edit.rs jobs.rs library.rs projects.rs health.rs
  dto.rs              типизированные тела ответов (Serialize), конец ручным json!{}
  error.rs            AppError + IntoResponse  (маппинг кодов в одном месте)
        │
        ▼
services/             прикладные сервисы (оркестрация, инварианты)
  job_service.rs      владеет «память + write-through»; transition() сам персистит на терминале
  render_service.rs   кэш → файл на диске → рендер → cache+media; знает JobService + ffmpeg
  project_service.rs  фасад над ProjectRepo
        │
        ├──────────────┬───────────────┬──────────────┐
        ▼              ▼               ▼              ▼
domain/            jobs/            persistence/    storage/
  edit_plan.rs       runner.rs        mod.rs (traits) file_store.rs
  effect.rs (enum)   progress.rs      sqlite/*        (sources/outputs,
  output_spec.rs     task.rs (trait)  memory/*         удаление/проверка)
  (Timeline-IR)
        │              │               │
        ▼              ▼               ▼
ffmpeg/            (Task: работа)   sqlx / files
  compile.rs (IR→filter_complex, чистый, тестируемый)
  process.rs (запуск/прогресс/отмена/таймаут)
net.rs (SSRF, чистый)     config.rs (env один раз)    messages.rs (i18n)
```

### Ключевые абстракции (сигнатуры-эскизы)

```rust
// Персистентность: узкие трейты-порты, доменные типы и ошибки (не sqlx::Error).
trait ProjectRepo { upsert/get/get_by_video/list/delete -> RepoResult<...> }
trait JobRepo     { save(&Job) / load_all() / get(id) -> RepoResult<...> }
trait MediaRepo   { add/list/get/remove -> RepoResult<...> }       // бывшая Library
trait RenderCache { get(key)->Option<CachedRender> / put(key, &CachedRender) }

// Задача: единица фоновой работы. Some=результат, None=отменено, Err=ошибка.
trait Task { async fn run(&self, ctx: &JobContext) -> Result<Option<Value>> }
// JobContext = { progress: Sender<f64>, cancel: CancellationToken }

// Раннер: единственное место со скелетом задачи (queued→permit→cancel→running→
// drain→finish→persist). Хендлеры только строят Task и сдают раннеру.
impl JobService { async fn spawn(&self, id: JobId, spec: JobSpec, task: impl Task); }

// Ошибки: один enum, один маппинг в HTTP.
enum AppError { BadRequest(Reason), NotFound, Conflict(&str), Internal(anyhow::Error) }
impl IntoResponse for AppError { ... }
```

### Доменная модель (Timeline-IR)

`EditRequest` сейчас тащит тройную роль (wire-DTO + домен + вход билдера). Цель -
разделить:

```
wire DTO (serde, camelCase)
   │  to_plan()  — валидация, нормализация (trim/segments → Timeline), tier→CRF
   ▼
EditPlan { timeline: Timeline, output: OutputSpec }
  Timeline { segments: Vec<Segment> }            // «весь клип» = один сегмент
  Segment  { range, effects: Vec<ScopedEffect> }
  ScopedEffect { scope: Whole | Range(t0,t1), effect: Effect }   // задел под диапазоны
  Effect  enum { Crop/Rotate/Eq/LookPreset/Speed/Fade/... }      // эффект = 1 ветка компилятора
  OutputSpec enum { Mp4{codec,crf} | Webm{crf} | Av1{crf} | ProRes | Gif{fps} | Still | Audio }
   │  compile(&EditPlan, &SourceProbe)  — чистая функция, без I/O
   ▼
Vec<String>  (ffmpeg args / filter_complex)
```

Выгода: новый эффект = один вариант enum + одна ветка `compile`; мультитрек =
несколько `Segment`/дорожек; диапазонные эффекты = `Scope::Range`. Ключ
render-кэша считается от `EditPlan` (хеш домена), а не от wire-JSON.

## Целевой модульный дизайн (frontend)

```
core/
  editModel.ts     video + edit (единственный общий стейт) + loadVideo() + событие смены клипа
  payload.ts       buildEditPayload: (edit, video) → DTO (чистая)
lib/               чистые функции/константы (без reactive): time, quality, defaults, editOptions
features/          composables по доменам, состояние локально, наружу — узкий контракт
  useImport useExport useLibrary useHistory useProjects usePresets
ui/                useTheme, usePlayer; примитивы ChipGroup/RangeField/ToggleRow (props/emit, без store)
components/        EditPanel = тонкий контейнер + секции:
                   TimingControls FrameControls ColorControls AudioControls ExportControls PresetBar
```

Развязка: `useImport`/`openFromLibrary` зовут `editModel.loadVideo()`, а
`useHistory`/`useProjects` **подписываются** на смену клипа (а не вызываются
напрямую). Watch-и истории/автосейва живут внутри своих composables, не как
сайд-эффект импорта модуля. Pinia не нужен на текущем масштабе.

## Контракт правок: один источник правды

Сейчас новое скалярное поле трогает ~6 мест (`EditRequest`, `EditState`,
`defaultEdit`, `buildEditPayload`, `PRESET_KEYS`, `EditPanel`) - и ни одно не
ловится компилятором (тихий дроп поля). Цель: Rust как источник истины,
генерация TS-типов (`ts-rs`/`schemars`); `EDIT_DEFAULTS` + `diffFromDefault` для
плоской части (`buildEditPayload`/`PRESET_KEYS` выводятся), ручная только доменная
часть (сегменты, tier→CRF, геометрия). `EditState ≠ EditRequest` (UI-флаги
`*Enabled` остаются на фронте), слепая генерация одного из другого недопустима.

## Правила зависимостей

- `http` → `services` → (`domain`, `jobs`, `persistence` traits, `storage`).
  HTTP не знает про sqlx/ffmpeg-флаги; `persistence/sqlite` - единственное место с sqlx.
- `domain`/`ffmpeg::compile`/`net`/`config` - **чистые** (без I/O), тестируются изолированно.
- Пользовательские строки - только в `messages` (i18n), не в доменном коде.
- Внешние процессы (ffmpeg/yt-dlp) - только за `ffmpeg`/`download`; всё через
  один process-раннер с отменой/таймаутом.
- Конфиг читается один раз (`Config::from_env`) и прокидывается явно.

## Текущее → целевое (карта миграции)

| Область | Сейчас | Цель | Статус |
|---|---|---|---|
| ffmpeg-аргументы | `tools/args.rs` (чистый) | `ffmpeg/compile.rs` от IR | ✅ выделено, IR - позже |
| SSRF | `tools/net.rs` | `net.rs` | ✅ |
| HTTP god-file | `handlers/mod.rs` + 3 группы | `http/*` по ресурсам | ◐ частично |
| Оркестрация задач | копипаста в import/edit | `jobs::JobService::spawn` + `Task` | ☐ |
| Ошибки | `(StatusCode,String)` россыпью | `AppError` + `IntoResponse` | ☐ |
| Персистентность | конкретный `Db` + `Library` JSON | трейты-репозитории + SQLite | ☐ |
| Модель правок | плоский `EditRequest` | `EditPlan`/Timeline-IR | ☐ |
| Конфиг | env в ~9 местах | `Config` один раз | ☐ |
| Frontend стор | `store.ts` god-модуль | core + features composables | ☐ |
| Frontend панель | `EditPanel.vue` god-компонент | секции-компоненты | ☐ |

Приоритизированный план «что делать первым» - в [recommendation.md](recommendation.md);
порядок и трудоёмкость каждого шага рефакторинга - в [docs/refactor-plan.md](docs/refactor-plan.md).
Известные дефекты (не модульность), верифицированный топ - в
[docs/audit.md](docs/audit.md) (118 подтверждено двумя проходами adversarial-
верификации, 14 ложных срабатываний отсеяно). Самое острое: вырезание сегмента
молча не работает для AV1/ProRes; гонка cancel↔finish (отменённая задача может
стать Done); crop не валидируется против размеров источника; TTL-чистка удаляет
файлы без проверки ссылок; `upload_handler` минует семафор; SSRF обходится через
DNS-резолвинг yt-dlp; пропущенный `persist_job` при ошибке URL; рост `recover_jobs`.
