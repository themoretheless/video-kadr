# Модульный рефакторинг: план из 10-критического ревью

Свод 10 независимых code-grounded ревью (каждый критик читал реальный код) по оси
**модульность / разбиение / слабая зацепленность**. Ранжировано по
польза÷риск. Принцип безопасности: «чистое перемещение, поведение не меняется,
ловится компилятором + тестами (44 unit + 16 integration + 3 real-ffmpeg render)».

## Уже сделано

- **`tools.rs` (~1280 строк) → `tools/{mod,args,net}`** (`8cd508b`). Чистая сборка
  ffmpeg-аргументов (`args.rs`, без I/O, все 22+ arg-теста), SSRF-валидация
  (`net.rs`), и process/download/probe I/O (`mod.rs`, реэкспортит публичный API).
- **`handlers.rs` → `handlers/{mod,projects,library,health}`** (`349ed0d`).
  Независимые от job-машинерии группы вынесены; `mod.rs` реэкспортит, держит
  import/edit/jobs + общий finish_job/spawn_progress_drain/job_timeout.

## Дальше - безопасно сейчас (S, прикрыто тестами, без смены поведения)

- **Frontend `store.ts` (~600 строк) → модули.** Вынести чистые функции
  (`parseTime`/`tierToCrf`/`defaultEdit`/`buildEditPayload`) в `lib/*`, затем
  `theme`/`presets`/`history` в свои composable-файлы, оставив реэкспорты из
  `store.ts` (потребители и vitest не меняются). Связь импорт↔история развязать
  событием смены `video` в core-модели, а не прямыми вызовами `resetHistory()`. **S→M.**
- **Frontend `EditPanel.vue` (~720 строк) → под-компоненты.** Каталоги опций →
  `lib/editOptions.ts` (S); секции `AudioControls`/`PresetBar`/`ColorControls`
  (S) → `Timing`/`Frame`/`Export` (M). Развязать скрытую связь Export→Frame:
  вынести `applyPlatform`/`setAspect` в store/lib. **S→M.**
- **Централизовать контракт правок (change-coupling).** Новое скалярное поле
  сейчас трогает ~6 мест (EditRequest, EditState, defaultEdit, buildEditPayload,
  PRESET_KEYS, EditPanel) - и ни одно не ловится компилятором (тихий дроп поля).
  Первый шаг (S): один `EDIT_DEFAULTS` + `diffFromDefault`, из него же вывести
  `PRESET_KEYS` и плоскую часть `buildEditPayload`. Снижает до ~3 мест.
- **`Config`-struct (env в одном месте).** Сейчас env читается из ~9 мест на лету
  (`MAX_HEIGHT` в `tools`, `JOB_TIMEOUT_SECS` в `handlers` - на каждый запрос).
  Собрать `Config::from_env()` один раз в `main`, положить в `AppState`, прокинуть
  явно (`max_height` аргументом в `download_video`); fail-fast на кривом
  `BIND_ADDR`/`PORT`. **M.**
- **`messages.rs` (i18n-каталог).** Русские строки-ошибки зашиты в `tools`/`handlers`.
  Вынести в один модуль по ключам (готовит почву под i18n). **S→M.**

## Дальше - глубже (M/L, меняет сигнатуры/контрол-флоу, нужны новые тесты)

- **JobRunner: убрать дублирование оркестрации import/edit.** Оба хендлера
  посимвольно повторяют `set_job→spawn→queued→permit→cancel-check→running→drain→
  finish→persist`. Вынести в `jobs::spawn_job(id, spec, work)`, где `work` -
  async-замыкание `(JobContext)->Result<Option<Value>>`; хендлеры ужимаются до
  ~25 строк. Заодно чинит найденный баг: cancel в ветке `validate_url` не делает
  `persist`. Нужны unit-тесты на runner (отмена в queued, no-hang `drop(tx)`). **L.**
- **Типизированный `AppError` + `IntoResponse`.** Сейчас HTTP-маппинг руками в
  каждом хендлере (`(StatusCode, String)` россыпью), фоновые ошибки - голая строка
  в `job.error`. Ввести enum + единый `into_response`, `ErrorKind` для job. **L.**
- **Репозитории-трейты (ProjectRepo/JobRepo/MediaRepo/RenderCache).** Хендлеры
  жёстко зависят от конкретного `Db` (sqlx); `Library` - второй параллельный
  механизм (JSON). Шаг A (S/M): трейты поверх существующего `Db`, `AppState` на
  `Arc<dyn ...>` - даёт in-memory тесты без sqlite. Шаг B (L): мигрировать
  `Library` в SQLite + вынести `FileStore` (file-IO отдельно от записи). **M→L.**
- **Сервисный слой (`JobService`/`RenderService`).** Инвариант «терминал →
  persist + clear_cancel» сейчас держится дисциплиной вызовов в 5 местах.
  Инкапсулировать в `JobService::transition`. **M.**
- **Timeline IR.** `EditRequest` тащит тройную роль: wire-DTO + доменная модель +
  вход арг-билдера. С нуля: `wire DTO → to_plan() → EditPlan/Timeline-IR →
  compile() → ffmpeg`, где `Effect` - enum (эффект = одна единица + одна ветка
  компилятора), `OutputSpec` - тегированный union, `Scope` - задел под
  диапазоны/мультитрек. Первый шаг (S): вынести `OutputSpec` из плоских
  format/codec/quality. Полный IR - **L**, и только он разблокирует мультитрек. **L.**

## Сквозные находки (вне разбиения, но критики отметили)

- `validate_url` проверяет только хост-литерал, не резолвит DNS → домен,
  резолвящийся в 127.0.0.1/RFC1918, проходит SSRF-guard (DNS-rebinding).
- Баг: в `import_handler` отмена на пути `validate_url` делает `clear_cancel`, но
  не `persist_job` (статус Error не дописывается в БД). Чинится в рамках JobRunner.
- `recover_jobs` грузит все исторические джобы в память (рост от рестарта к
  рестарту) - нужен ретеншн.

### Порядок, который рекомендую
Frontend `store`/`EditPanel` split + `Config` + контракт-дефолты (всё S/M,
безопасно) → затем JobRunner (L, высокая ценность) → репозитории-трейты (M) →
`AppError` (L) → Timeline IR (L, разблокирует мультитрек).
