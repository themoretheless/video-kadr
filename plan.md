# План исполнения

Порядок устранения проблем из аудита. Полный список (509 заземлённых на код
проблем) - в [docs/audit-500.md](docs/audit-500.md); проверенное ядро (118,
поштучно верифицировано) - в [docs/audit.md](docs/audit.md); гранулярные
P0-P3-шаги с критериями приёмки - в [recommendation.md](recommendation.md);
целевой дизайн - в [architecture.md](architecture.md).

Принцип: сначала то, что бьёт локального пользователя сейчас (функц. баги, потеря
данных), затем безопасность перед выставлением наружу, затем надёжность/контракт,
тесты, и в конце сборка/доки/наблюдаемость. Каждая правка под зелёным `make check`.

Ниже - очередь из 34 high-severity находок, сгруппированная по фазам (повторы вроде
SSRF/скорости сведены). Medium/low-хвост (475 шт.) разбирать внутри тех же фаз по
ходу касания файла.

## P0-A. Функциональные баги и потеря данных (часы-день каждый)

- [x] **Рассинхрон A/V при speed>2** - видео `setpts=1/speed` без границ, аудио `atempo` клампится 0.5..2.0. Клампить `speed` один раз в `normalize_edit_geometry` (или строить цепочку `atempo`). `tools/args.rs:138-173`
- [x] **NaN/Inf в числовых полях** - `speed/volume/brightness/.../fade` это `f64` без `is_finite()`; `{"speed":1e999}` ломает/вешает ffmpeg. Валидировать все `f64` + клампить в `normalize_edit_geometry`; `Trim` без finite-guard. `model.rs:111-167`, `tools/args.rs:524-529`
- [x] **Сегменты/trim не клампятся к длительности** - `end>duration` уходит в trim за EOF → пустой/битый concat. Клампить каждый сегмент к `[0, probe.duration]`, дропать нулевые. `handlers/mod.rs:476-497`
- [ ] **Пустой crop-инпут → NaN → 422** - `v-model.number` ставит NaN при очистке поля, бэкенд `u32` отвергает всё тело. Коэрсить на вводе до finite int. `components/EditPanel.vue:515-518`
- [ ] **Гонка cancel↔finish + кэш-хит перетирает cancel** - `finish_job` гейтит `update_job_if_open`, но кэш-хит использует `update_job` и токен чистится до `cancel()`. Атомарно: решение об отмене под тем же локом, кэш-хит тоже через `update_job_if_open`. `handlers/mod.rs:275-292, 381-408`
- [ ] **Параллельные идентичные edit → сирота-output** - оба мимо кэша, оба рендерят, второй `cache_put` перетирает первый, его файл осиротевает. Single-flight по `cache_key` (in-flight карта). `handlers/mod.rs:268-359`
- [x] **`library.save()` глотает ошибки записи** - `save()->io::Result`, логирует `tracing::error!`; `add()` коммитит в память только после успешной записи. `library.rs:80-97`
- [x] **`library.remove()` удаляет файл до `save()`** - теперь сначала persist, файл удаляется только после успешного `save()`; при сбое запись сохраняется и `remove` возвращает false. `library.rs:127-146`
- [x] **`from_result` пустой id схлопывает медиа** - `add()` отбраковывает записи с пустым id/filename (guard перед dedup), два пустых id больше не сливаются. `library.rs:90-96`
- [ ] **Autosave гонится с restore, затирает проект дефолтом** - `openFromLibrary` ставит дефолтный edit и заряжает 1000ms autosave; async `restoreProject` может прийти позже → сохранится дефолт. Флаг `restoring`, гасить таймер до завершения restore. `store.ts:345-368, 569-609`
- [ ] **`redo()` не флашит pending debounce** - правка в окне 350ms перед redo теряется (в отличие от `undo()`). Добавить `if (historyTimer) recordChange()` в начало `redo()`. `store.ts:424-438`
- [ ] **`run_with_progress` висит/осиротевает процессы** - `wait` ждёт EOF обоих пайпов (висит, если один открыт); `start_kill` бьёт только прямого ребёнка, внук-ffmpeg от yt-dlp выживает. Гонять `child.wait()` безусловно + бить process-group; ограничить post-kill wait таймаутом. `tools/mod.rs:314-351`
- [x] **Отмена/таймаут импорта оставляет мусор; TTL чистит без проверки ссылок** (из [docs/audit.md](docs/audit.md)) - чистить `sources/` по префиксу vid; TTL удалять только нессылаемые/неактивные. `handlers/mod.rs`, `main.rs:103-135`

## P0-B. Безопасность (обязательно перед любым выставлением наружу)

- [ ] **Dockerfile `BIND_ADDR=0.0.0.0` + нет auth** - образ слушает все интерфейсы при полностью открытом API. Дефолт `127.0.0.1`, расширять только за аутентифицированным прокси. `Dockerfile:16-17`
- [ ] **Нет auth/ownership на projects** - любой клиент читает/удаляет любой проект (URL, имена, метаданные). Сессия/owner-ключ или явный single-tenant. `handlers/projects.rs:47-92`
- [ ] **`ServeDir` отдаёт `app.db` + WAL/SHM** - `/files/app.db-wal` скачивается, утечка всей БД. Вынести БД из обслуживаемого дерева; монтировать только `sources/` и `outputs/`. `lib.rs:54`
- [ ] **CORS `permissive()`** - любой сайт читает тела ответов (`/api/library`, `/api/projects`) из браузера жертвы. Явный allowlist origin, за конфигом. `lib.rs:56`
- [ ] **SSRF обходится** - `validate_url` теперь резолвит DNS и блокирует private/special IP edge cases; осталось закрыть редиректы `yt-dlp` на приватные адреса. `tools/net.rs:10-47`, `tools/mod.rs:63-115`

## P1. Надёжность, ресурсы, контракт ошибок

- [ ] **project JSON без size cap** - до ~2MB на любой videoId, БД растёт без предела. Кап `video_json/edit_json` (напр. 64KB), 413/400. `handlers/projects.rs:21-42`
- [ ] **upload без MIME/magic/quota** - доверяет расширению, отдаёт обратно. Allowlist расширений + проверка `codec_type` + квота. `handlers/mod.rs:144-231`
- [ ] **ffmpeg без CPU/RAM/threads/filesize-лимитов**; нет no-progress watchdog (из audit-500 разделов «Ресурсы»).
- [ ] **Логировать падение задачи** - `finish_job` Err не пишет `tracing::error!`, диагностики ноль. `handlers/mod.rs:576-583`
- [ ] **RectOverlay: координаты по letterbox, не по контенту видео** - при разнице пропорций crop/censor попадает мимо. Считать реальный content-box. `components/RectOverlay.vue:37-77`
- [ ] **`applyPreset` перетирает format/codec/quality** - `PRESET_KEYS` включает их, «look»-пресет меняет контейнер. Разделить look/export или убрать из `PRESET_KEYS`. `store.ts:458-484`
- [ ] Единый `ApiError`/`IntoResponse`, `202 Accepted` на async-задачи, единая форма ошибок (из audit-500 раздела «Ошибки и API»).

## P2. Тесты (всё ниже сейчас без покрытия)

- [ ] Happy-path импорта/рендера через `edit_handler` (skip-если-нет-ffmpeg/yt-dlp). `tests/api.rs`
- [ ] Отмена Running-задачи (kill процесса) → `Done::Cancelled` + очистка частичного файла.
- [ ] Async-экшены стора (`doImport/doExport/doUpload/library/restore`) с `vi.mock('./api')`.
- [ ] Undo/redo/история (debounce-флаш, 100-cap). `store.ts:385-447`

## P3. Сборка, наблюдаемость, документация

- [ ] **yt-dlp через curl без `-f`/pin/checksum** - на не-200 в бинарь пишется тело ошибки; `latest` невоспроизводим. `curl -fSL` + пин-тег + sha256. `Dockerfile:11-12`
- [ ] Логи жизненного цикла задач (`tracing::info!` на create/start/done/cancel, `warn/error` на fail).
- [ ] README-дрейф (env/Node/API/фичи), `RUST_LOG`, MSRV, healthcheck/non-root в Docker/compose.

---

Полный перечень с `file:line` по каждому пункту (включая весь medium/low-хвост) -
[docs/audit-500.md](docs/audit-500.md). Глубже всего проверено ядро в
[docs/audit.md](docs/audit.md) (там же 14 опровергнутых ложных срабатываний).
