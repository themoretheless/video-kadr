# План исполнения

Порядок устранения проблем из аудита. Полный список (509 заземлённых на код
проблем) - в [docs/audit-500.md](docs/audit-500.md); проверенное ядро (118,
поштучно верифицировано) - в [docs/audit.md](docs/audit.md); гранулярные
P0-P3-шаги с критериями приёмки - в [recommendation.md](recommendation.md);
целевой дизайн - в [architecture.md](architecture.md); исследовательский слой
из 100 репозиториев/papers/стандартов - в
[docs/research-100.md](docs/research-100.md).

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
- [x] **Пустой crop-инпут → NaN → 422** - `v-model.number` ставит NaN при очистке поля, бэкенд `u32` отвергает всё тело. Коэрсить на вводе до finite int. `components/EditPanel.vue:515-518`
- [x] **Гонка cancel↔finish + кэш-хит перетирает cancel** - cancel теперь atomically переводит только non-terminal job через `AppState::cancel_open_job`, а cache-hit/finish используют open-only update и не перетирают terminal-состояния. `handlers/mod.rs:275-292, 381-408`
- [x] **Параллельные идентичные edit → сирота-output** - одинаковые render-запросы сериализуются lock-ом по `cache_key`; follower после ожидания повторно проверяет кэш и не запускает второй ffmpeg. Кэш-хит валидирует plain filename и наличие output-файла. `handlers/mod.rs:268-359`
- [x] **`library.save()` глотает ошибки записи** - `save()->io::Result`, логирует `tracing::error!`; `add()` коммитит в память только после успешной записи. `library.rs:80-97`
- [x] **`library.remove()` удаляет файл до `save()`** - теперь сначала persist, файл удаляется только после успешного `save()`; при сбое запись сохраняется и `remove` возвращает false. `library.rs:127-146`
- [x] **`from_result` пустой id схлопывает медиа** - `add()` отбраковывает записи с пустым id/filename (guard перед dedup), два пустых id больше не сливаются. `library.rs:90-96`
- [x] **Autosave гонится с restore, затирает проект дефолтом** - `openFromLibrary` ставит дефолтный edit и заряжает 1000ms autosave; async `restoreProject` может прийти позже → сохранится дефолт. Флаг `restoring`, гасить таймер до завершения restore. `store.ts:345-368, 569-609`
- [x] **`redo()` не флашит pending debounce** - правка в окне 350ms перед redo теряется (в отличие от `undo()`). Добавить `if (historyTimer) recordChange()` в начало `redo()`. `store.ts:424-438`
- [x] **`run_with_progress` висит/осиротевает процессы** - `child.wait()` теперь участвует в select всегда, процессы запускаются в отдельной process group, cancel/timeout гасят группу SIGTERM→SIGKILL с bounded wait; покрыто регрессиями на pipe-holder/background child. `tools/mod.rs:314-351`
- [x] **Отмена/таймаут импорта оставляет мусор; TTL чистит без проверки ссылок** (из [docs/audit.md](docs/audit.md)) - чистить `sources/` по префиксу vid; TTL удалять только нессылаемые/неактивные. `handlers/mod.rs`, `main.rs:103-135`

## P0-B. Безопасность (обязательно перед любым выставлением наружу)

- [x] **Dockerfile `BIND_ADDR=0.0.0.0`** - образ по умолчанию снова слушает `127.0.0.1`; `docker-compose` оставляет явный `0.0.0.0` только для внутреннего nginx proxy. `backend/Dockerfile`, `docker-compose.yml`
- [ ] **Нет глобального auth при внешней публикации API** - bind теперь безопаснее по дефолту, но при прямом expose наружу нужен auth/reverse-proxy guard.
- [ ] **Нет auth/ownership на projects** - любой клиент читает/удаляет любой проект (URL, имена, метаданные). Сессия/owner-ключ или явный single-tenant. `handlers/projects.rs:47-92`
- [x] **`ServeDir` отдаёт `app.db` + WAL/SHM** - `/files` теперь монтирует только `sources/` и `outputs/`; корень storage и SQLite-файлы не публикуются. `lib.rs:54`
- [x] **CORS `permissive()`** - заменён на явный allowlist (`CORS_ALLOW_ORIGINS`, defaults для local dev); wildcard/не-origin значения отбрасываются. `lib.rs:56`
- [x] **SSRF через redirect/DNS rebinding/protocol bypass** - HTTP(S) `yt-dlp` идёт через loopback egress-proxy: повторная DNS/IP-проверка, pinned `SocketAddr`, только 80/443, запрет `NO_PROXY`; format selector отклоняет RTMP/FTP/WebSocket media. Реальные тесты подтверждают блок до connect/downloader. `tools/net.rs`, `tools/egress_proxy.rs`, `tools/mod.rs`

## P1. Надёжность, ресурсы, контракт ошибок

- [x] **project JSON без size cap** - `video` и `edit` ограничены 64KiB каждый; oversized autosave получает `413 Payload Too Large` до записи в SQLite. `handlers/projects.rs:21-42`
- [x] **upload без MIME/magic/quota/concurrency cap** - body-size quota уже есть; клиентское расширение игнорируется, контейнер проходит bounded `ffprobe`/allow-list перед publish, статика получает `nosniff` + sandbox CSP; отдельный upload-pool отвечает `429` при насыщении. `handlers/upload.rs`, `state.rs`, `lib.rs`
- [ ] **ffmpeg без CPU/RAM/threads/filesize-лимитов**; нет no-progress watchdog (из audit-500 разделов «Ресурсы»).
- [x] **Логировать падение задачи** - `finish_job` пишет `tracing::error!` с job ID, internal detail остаётся в серверном логе. `handlers/mod.rs`
- [x] **RectOverlay: координаты по letterbox, не по контенту видео** - маппинг переведён на `domain/contentBox.ts`: прямоугольник позиционируется и тянется относительно реального content-box, а не элемента плеера. `components/RectOverlay.vue`, `domain/contentBox.ts`
- [x] **`applyPreset` перетирает format/codec/quality** - look-presets отделены от export-настроек; старый preset больше не меняет контейнер/codec/quality. `domain/edit.ts`, `store.ts`
- [x] Единый `AppError`/`IntoResponse` и JSON envelope введены в раунде 8.
- [ ] Перевести создание async-задач с `200 OK` на `202 Accepted` и закрепить contract-тестами.

## P2. Тесты (всё ниже сейчас без покрытия)

- [ ] Happy-path импорта/рендера через `edit_handler` (skip-если-нет-ffmpeg/yt-dlp). `tests/api.rs`
- [ ] Отмена Running-задачи (kill процесса) → `Done::Cancelled` + очистка частичного файла.
- [ ] Async-экшены стора (`doImport/doExport/doUpload/library/restore`) с `vi.mock('./api')`.
- [ ] Undo/redo/история (debounce-флаш, 100-cap). `store.ts:385-447`

## P3. Сборка, наблюдаемость, документация

- [ ] **yt-dlp через curl без `-f`/pin/checksum** - на не-200 в бинарь пишется тело ошибки; `latest` невоспроизводим. `curl -fSL` + пин-тег + sha256. `Dockerfile:11-12`
- [ ] Логи жизненного цикла задач (`tracing::info!` на create/start/done/cancel, `warn/error` на fail).
- [x] Сверка 9 июля 2026: `README.md`, `architecture.md` и `recommendation.md` синхронизированы вокруг 509 широких и 565 SOLID/DRY пунктов; порядок маленьких PR обновлён.
- [x] Раунд 5 (11 июля 2026): закрыты upload XSS и cancel→running, вынесены backend upload/frontend edit domain/export controls, исправлены no-op export, preset drift, drag cleanup и mobile overflow; три итерации проверены тестами и живым UI.
- [x] Раунд 6 (11 июля 2026): закрыты SSRF redirect/DNS rebinding и custom-port egress; добавлены per-job proxy, bounded DNS/connect, реальные `yt-dlp` regression-тесты и pinned `yt-dlp` в CI.
- [x] Раунд 7 (11 июля 2026): закрыты upload concurrency, partial-upload cleanup, probe timeout и публичный jobs_semaphore; multipart bounded с cleanup, tool probes имеют timeout с kill+wait, добавлены state/API/tool regressions.
- [x] Раунд 8 (11 июля 2026): введены `AppError`/`AppResult`, единый JSON envelope и typed frontend `ApiError`; projects parsing отделён от persistence, extractor/404/405/body-limit ошибки покрыты regression-тестами.
- [x] Исследовательский раунд (14 июля 2026): изучены 100 активных высокорейтинговых репозиториев и первичные papers/specs; добавлены и синхронизированы карточки №784-883. Рабочий набор теперь 665 пунктов (565 SOLID/DRY + 100 research-backed).
- [x] Research wave 1/10 (14 июля 2026): закрыты №790/824/828/829/831/844/846/847/850/854; добавлены runtime/shutdown/contract/privacy/perf/UI/security gates и отдельные regression suites.
- [ ] Research wave 2/10: №784/786/787/803/814/817/820/823/825/826 - typed media/timeline domain и HTTP ports/policy.
- [x] Волна паритета (август 2026, 13 агентов + интеграция): мультиклиповый таймлайн с переходами, наложения и хромакей, титры и субтитры, микс звука с дакингом и динамикой, Ken Burns и рампы скорости, 360-перекадрирование и стабилизация, расширенный цвет, `/api/assets`. Проверено реальными рендерами в `backend/tests/parity_render.rs` (15 тестов), контракт фронт-бэк закреплён в `backend/tests/wire_contract.rs`. Не проверено локально: титры/субтитры (нет `drawtext`/`subtitles` в ffmpeg этой машины) и точная стабилизация (нет `libvidstab`).
- [ ] README-дрейф: env/Node/API/`RUST_LOG` обновлены; остаются MSRV, healthcheck/non-root в Docker/compose и дальнейшая docs/code drift-проверка.

---

Полный перечень с `file:line` по каждому пункту (включая весь medium/low-хвост) -
[docs/audit-500.md](docs/audit-500.md). Глубже всего проверено ядро в
[docs/audit.md](docs/audit.md) (там же 14 опровергнутых ложных срабатываний).
