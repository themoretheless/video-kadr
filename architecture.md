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

## Top-50: что сделано плохо или неправильно

Источник правды - [docs/audit.md](docs/audit.md). Ниже та же первая
приоритизированная пятидесятка, синхронизированная с `recommendation.md`.
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
