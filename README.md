# Видеоредактор (MVP)

Онлайн-редактор видео: вставляешь ссылку (например, VK Видео), скачиваешь, режешь и
экспортируешь результат. Бэкенд на Rust (Axum) скачивает видео через `yt-dlp` и
обрабатывает через `ffmpeg`; фронтенд на Vue 3 + Vite.

## Что умеет

- Импорт видео по ссылке (всё, что поддерживает `yt-dlp`, включая `vkvideo.ru`).
- Импорт локального файла: перетащи в окно или выбери (`POST /api/upload`).
- Обрезка (trim) двойным слайдером: тянешь две ручки на одной полосе, точки входа/выхода
  можно ставить от позиции плеера или вводить время вручную.
- Вырезание куска из середины: оставшиеся части склеиваются (ffmpeg concat, MP4/WebM).
- Изменение размера (1080p / 720p / 480p / 360p, высота по пропорции).
- Кадрирование (crop) по прямоугольнику или по пресету пропорций (9:16, 1:1, 4:5, 4:3, 16:9).
- Удаление звука, регулировка громкости, нормализация громкости (loudnorm) и highpass-фильтр против гула.
- Изменение скорости (0.5×–2×, со звуком через `atempo`).
- Эффекты: поворот (90/180/270), отражение, реверс, fade in/out, частота кадров.
- Цвет: яркость/контраст/насыщенность, пресеты (ч/б, сепия, тёплый, холодный,
  teal-orange, выцветший, нуар, винтаж), 3D LUT `.cube` с интенсивностью и
  кривые Master/R/G/B (до 16 точек на канал), виньетка, шумодав, резкость и
  зерно. LUT и кривые точно применяются при экспорте; встроенный предпросмотр
  их не отображает и явно сообщает об этом.
- Замазать область прямоугольником (выбор рамкой на видео), поля под пропорции (letterbox 9:16, 1:1, и т.д.).
- Экспорт в MP4 (H.264/H.265 + AAC), WebM (VP9 + Opus), AV1, ProRes (.mov), GIF,
  стоп-кадр PNG/JPG или аудио MP3; выбор качества и пресеты под платформы
  (Telegram/Shorts/Reels/YouTube).
- Скачивание с осмысленным именем файла.
- Медиатека: импортированные источники и результаты сохраняются между перезапусками
  (`storage/library.json`), можно переоткрыть клип в редакторе, скачать или удалить.
- Проекты: правки автосохраняются и восстанавливаются при переоткрытии клипа (SQLite);
  задачи имеют event history/attempts/outbox, переживают crash и штатный перезапуск,
  а повторный одинаковый запрос получает прежний job ID и не запускает второй процесс.
- Светлая и тёмная тема (переключатель в шапке, выбор запоминается).
- Отмена/повтор изменений настроек (Cmd/Ctrl+Z, Cmd/Ctrl+Shift+Z или Ctrl+Y, плюс кнопки).
- Пресеты эффектов: сохрани набор (цвет, скорость, звук, формат) и применяй к другим клипам.
- Прогресс скачивания и обработки в процентах, кнопка отмены задачи.
- Очередь задач с ограничением параллелизма, таймауты, опциональная очистка старых файлов.
- Горячие клавиши: `Space` (плей/пауза), `I`/`O` (точки входа/выхода), `←`/`→` (перемотка,
  с `Shift` крупнее), `,`/`.` (по кадру), `Cmd/Ctrl+Z` / `Cmd/Ctrl+Shift+Z` (отмена/повтор).

## API

```
POST /api/import            { url, start?, end? }        -> { jobId }
POST /api/edit              { videoId, trim?, crop?, ... } -> { jobId }
POST /api/upload            multipart file               -> VideoInfo
POST /api/luts              multipart 3D .cube            -> LutAsset
GET  /api/luts                                             -> [ LutAsset ]
GET  /api/luts/:id                                         -> LutAsset | 404
GET  /api/jobs/:id          -> { status, progress?, stage?, result?, error? }
POST /api/jobs/:id/cancel   -> 200 cancelled | 404 | 409
GET  /api/jobs/failed       -> { jobs: [ FailedJob ] }
GET  /api/jobs/registry     -> { counts: { queued, started, deferred, failed, finished } }
POST /api/jobs/:id/retry    -> 200 pending | 404 | 409
POST /api/jobs/:id/discard  -> 200 discarded | 404
GET  /api/library           -> [ MediaEntry ]  (sources + outputs, newest first)
GET  /api/library/search?q= -> [ SearchHit ]  (SQLite FTS5, prefix/ranking)
DELETE /api/library/:id     -> 204 | 404  (also deletes the file)
POST /api/projects          { videoId, video, edit, name? } -> Project  (autosave/upsert by clip)
GET  /api/projects          -> [ Project ]  (newest first)
GET  /api/projects/by-video/:videoId -> Project | 404
GET  /api/projects/:id      -> Project | 404
DELETE /api/projects/:id    -> 204 | 404
GET  /api/health            -> { status, ffmpeg, ytdlp, ffmpegVersion, ytdlpVersion }
GET  /api/capabilities      -> { schemaVersion, toolFingerprint, formats, codecs, filters, hardware }
GET  /files/sources/...     -> исходники (с поддержкой Range)
GET  /files/outputs/...     -> результаты (с поддержкой Range)
```

Все ошибки приложения в `/api` имеют один JSON-контракт:
`{"error":"Понятное сообщение","code":"machine_readable_code"}`. Frontend
сохраняет `status`/`code` в `ApiError`; сообщение о недоступном backend
используется только при сетевой ошибке, а не для настоящего HTTP `5xx`.
Wire DTO строги к неизвестным полям и принимают опциональный `schemaVersion: 1`;
вложенные JSON-документы сохранённых проектов остаются migration-tolerant.

LUT API принимает только 3D `.cube` размером до 16 МиБ с `LUT_3D_SIZE` от 2 до
65; одномерные LUT отклоняются. `/api/edit` ссылается на сохранённый LUT по
`lut: { id, intensity }`, где интенсивность лежит в диапазоне 0–1, и принимает
`curves` с каналами `master`, `red`, `green`, `blue`; координаты точек
нормализованы в диапазон 0–1. Приватное хранилище ограничено 256 LUT и 256 МиБ
суммарно; повторная загрузка того же содержимого переиспользует существующий
ресурс.

Переменные окружения: `PORT` (8080), `BIND_ADDR` (127.0.0.1), `STORAGE_DIR` (storage),
`MAX_HEIGHT` (720), `MAX_CONCURRENT_JOBS` (2; размер независимых job/upload
пулов), `JOB_TIMEOUT_SECS` (1800),
`JOB_DEDUPE_TTL_SECS` (300), `JOB_RATE_WINDOW_SECS` (60), `JOB_RATE_LIMIT` (60),
`FILE_TTL_HOURS` (0 = выключено), `MAX_UPLOAD_BYTES` (2 ГиБ),
`RECOVER_JOBS_LIMIT` (200), `CORS_ALLOW_ORIGINS` (локальные dev-origin'ы через
запятую), `RUST_LOG` (`info,tower_http=info`). External processes дополнительно
управляются через `ISOLATION_TIER` (`local` по умолчанию) и
`PROCESS_MAX_{CPU_SECONDS,MEMORY_MIB,CHILDREN,OPEN_FILES,FILE_BYTES,CAPTURE_BYTES,LINE_BYTES}`.
Non-loopback bind требует явного `lan`; `public` fail-closed. Точная матрица
enforcement находится в [docs/process-isolation.md](docs/process-isolation.md).

## Требования

- Rust (cargo)
- Node.js 20+ (`.nvmrc` зафиксирован на 20)
- `ffmpeg` и `yt-dlp` в `PATH`
  ```
  brew install ffmpeg yt-dlp
  ```

## Запуск

Два процесса в двух терминалах.

**Бэкенд** (порт 8080):
```
cd backend
cargo run
```

**Фронтенд** (порт 5173, проксирует `/api` и `/files` на бэкенд):
```
cd frontend
npm install
npm run dev
```

Открой http://localhost:5173

Нужны оба процесса. Если frontend показывает «Сервер недоступен» или ошибку
dev-прокси, значит не поднят backend: Vite не может достучаться до `:8080`.
Запусти `cargo run` в `backend`.

## Разработка и тесты

Бэкенд покрыт юнит-тестами (сборка ffmpeg-аргументов, валидация URL, медиатека,
модель) и HTTP-интеграционными тестами (роутер гоняется через
`tower::ServiceExt::oneshot`, без сокета). Есть один реальный ffmpeg-тест рендера
(`backend/tests/render.rs`), который сам пропускается, если `ffmpeg`/`ffprobe` нет
в `PATH`, и реальный `yt-dlp`-тест редиректа в приватную сеть (также skip без
`yt-dlp`). Фронтенд — ESLint + Vitest на логику стора (`buildEditPayload`,
`parseTime`, пресеты, polling с fake timers). Playwright проверяет offline boot,
mocked import/edit/export и отсутствие overflow на 390 px в Chromium, Firefox и
WebKit. Upload security и HTTP backpressure вынесены в отдельные backend suites.
Linux CI запускает все три движка; локально на macOS Firefox можно включить
через `PLAYWRIGHT_FIREFOX=1 npm run test:e2e` (по умолчанию остаются Chromium и
WebKit из-за зависания teardown текущей bundled Firefox-сборки). E2E поднимает
собственный strict dev-server на порту, детерминированном от пути worktree; его
можно заменить через `PLAYWRIGHT_PORT`, чужой сервер не переиспользуется.

```
make check   # всё как в CI: backend + frontend + bundle budget + Playwright
make test    # только тесты (cargo test + vitest)
make lint    # clippy -D warnings + eslint
make fmt     # cargo fmt
```

Точечно:
```
cd backend  && cargo test
cd backend  && cargo fmt --check
cd backend  && cargo clippy --all-targets -- -D warnings
cd backend  && cargo bench --bench persistence
cd frontend && npm run lint
cd frontend && npm run typecheck
cd frontend && npm run test
cd frontend && npx playwright install chromium firefox webkit # один раз локально
cd frontend && npm run test:e2e
```

CI (GitHub Actions, `.github/workflows/ci.yml`) на push/PR в `main` ставит ffmpeg
и закреплённый `yt-dlp`, гоняет для бэкенда `cargo fmt --check`,
`clippy -D warnings`, `cargo test`, а для фронтенда — lint, typecheck, тесты и
сборку.

## GitHub Pages

Официальный Pages workflow-шаблон находится в
`.github/workflows/pages.yml`. На каждый push в `main` (или вручную через
`workflow_dispatch`) он собирает `frontend/dist` с базовым путём, который
возвращает `actions/configure-pages`, загружает Pages artifact и публикует его
через `actions/deploy-pages`.

Перед первым запуском в GitHub открой **Settings → Pages → Build and
deployment** и выбери **Source: GitHub Actions**. Путь репозитория и custom
domain учитываются автоматически, поэтому отдельная правка `vite.config.ts` не
нужна.

GitHub Pages размещает только статический Vue-интерфейс. Rust/FFmpeg backend,
загрузка файлов, LUT и экспорт требуют отдельно запущенный backend; сам Pages
их выполнить не может.

## Backup и restore

Snapshot включает консистентный `app.db`, `library.json`, `sources/`, `outputs/`
и приватные LUT из `luts/`; `staging/` исключён. LUT-файлы хранятся отдельно от
JSON проектов, а проекты и edit-запросы содержат только ID LUT и интенсивность.
Имя каталога - SHA-256 содержимого,
а manifest хранит размер/checksum каждого файла и версию backend. Restore
публикуется только в пустой target после полной проверки checksums, безопасных
путей и `PRAGMA integrity_check`.

Активные durable import-задачи требуют исходный URL для возобновления, поэтому
их query credentials могут находиться в `app.db` и backup. После окончательного
success/cancel/non-retryable failure payload удаляется. Каталог backup всё равно
считается приватным и должен храниться с теми же правами, что и `storage/`.

```bash
cd backend
cargo run --bin backup -- create ../storage ../backups
cargo run --bin backup -- verify ../backups/<root-hash>
cargo run --bin backup -- restore ../backups/<root-hash> ../restored-storage
```

## Архитектура

Что делать дальше (приоритизированный план действий) - в
[recommendation.md](recommendation.md). Подробный модульный дизайн (целевой «как с
нуля» + текущее состояние и правила зависимостей) - в [architecture.md](architecture.md)
и совместимом alias-файле [arhitecture.md](arhitecture.md).
Пошаговый рефакторинг на слабую зацепленность - в [docs/refactor-plan.md](docs/refactor-plan.md).
Порядок исполнения по фазам - в [plan.md](plan.md).
Исследовательский benchmark 100 сильных media/editor/backend/security-проектов,
papers и стандартов - в [docs/research-100.md](docs/research-100.md); следующий
слой из 100 проверяемых идей №884-983 - в
[docs/research-next-100.md](docs/research-next-100.md).
ADR по subprocess security и local/LAN/public tiers - в
[docs/process-isolation.md](docs/process-isolation.md).
Аудиты проблем: проверенное ядро (118 находок, поштучно верифицировано, 14
опровергнутых) - в [docs/audit.md](docs/audit.md); расширенный широкий охват
(509 заземлённых на код проблем) - в [docs/audit-500.md](docs/audit-500.md);
единый бэклог с идеями/фичами - в [docs/ideas/top-200-backlog.md](docs/ideas/top-200-backlog.md).
Синхронизированный top-200 продублирован в `architecture.md` как диагноз и в
`recommendation.md` как карта исправлений; источник правды по багам - `docs/audit.md`,
порядок работ - `plan.md`. Если нужен именно большой список «500 предложений,
улучшений, проблем и ошибок», читай так: `docs/audit-500.md` = 509 широких
находок по коду; `architecture.md` и `recommendation.md` = 765 активных
рекомендаций: 565 SOLID/DRY-пунктов в 12 маленьких модулях плюс два слоя по 100
research-backed решений. Их можно брать по одному кусочку.

**Раунд 2 (1 июля 2026).** После 5 P0-фиксов (single-flight рендер-кэша,
process-group kill, атомарная отмена задач, CORS/ServeDir lockdown, лимит на
project-payload) код перепроверен заново 11 агентами: 23 находки подтверждены
adversarial-верификацией (плюс 15 доп. низкоприоритетных, плюс 26 явных
подтверждений «фикс корректен»), 2 старые находки опровергнуты. Большинство
исходных P0 закрыто; из нового - 2 подтверждённых high: обход SSRF-защиты
импорта через редирект `yt-dlp` и воспроизведённый (рабочий PoC) stored XSS
через загрузку файла. Актуальный ранжированный **топ-50** - в начале
[docs/audit.md](docs/audit.md#топ-50-актуальных-проблем-1-июля-2026). Оба пункта
позже закрыты: upload XSS в раунде 5, SSRF redirect/DNS rebinding в раунде 6.

**Раунд 3 (2 июля 2026): 565 находок, разбитые на 12 SOLID/DRY-модулей.**
Отдельный проход не по глубине (как раунд 2), а по ширине: весь код заново
прочитан 12 независимыми проходами (по одному на модуль - HTTP-хендлеры, Jobs/
конкурентность, ffmpeg-компилятор, persistence, security, API-контракт,
frontend-store, frontend-компоненты, **визуальный дизайн/UX** (реальный проход
по запущенному приложению со скриншотами, не только по коду), тесты, ops/config,
доменная модель), каждый - 3 итерации (широкий проход → углубление → дедуп и
добивка), каждый явно ищет нарушения SOLID/DRY в своей зоне. Смесь: 342
проблемы/бага, 43 улучшения, 12 идей новых фич, 75 дизайн-находок; 23 high,
151 medium, 391 low. Модули почти не пересекаются по файлам - можно закрывать
один модуль за раз, не боясь конфликтов с другими. Полный список по модулям -
в [architecture.md](architecture.md#модульная-карта-soliddry-декомпозиция-2-июля-2026);
чеклист с чекбоксами для работы - в
[recommendation.md](recommendation.md#soliddry-модули-по-кусочкам-2-июля-2026).
Самый удобный порядок чтения: сначала таблица модулей в `architecture.md`, затем
один соответствующий чеклист-модуль в `recommendation.md`, затем при споре -
подтверждение/доказательство в `docs/audit.md` или широкий источник в
`docs/audit-500.md`.

**Сверка 9 июля 2026.** Списки не раздваивались: `docs/audit-500.md` остаётся
широким top-500+ источником на 509 пунктов, а `architecture.md` и
`recommendation.md` - рабочей SOLID/DRY-картой на 565 пунктов. Для маленьких PR:
бери один модуль, один severity-слой, сначала safe-extract без смены поведения,
после каждого куска запускай `make check`. UI-правки проходи дизайнерской
линзой: состояние, иерархия, доступность, responsive, затем эстетика.

**Раунд 5 (11 июля 2026): первый исполнимый SOLID/DRY-срез.** Три итерации
прошли от security/correctness к модульности и затем к adversarial review.
Upload вынесен в `backend/src/handlers/upload.rs`: имя клиента больше не задаёт
расширение, контейнер определяется через `ffprobe`, а `/files` получает
`nosniff` и sandbox CSP. Переходы job в `queued`/`running` теперь атомарны
относительно отмены. Во frontend чистые defaults/time/quality/payload вынесены
в `frontend/src/domain/edit.ts`, экспорт - в отдельный `ExportControls.vue`;
добавлено подтверждение экспорта без изменений, старые look-пресеты больше не
меняют формат/качество, drag-listeners очищаются при unmount. Живой UI-прогон
закрыл горизонтальный overflow на 390 px. Полные 509/565 списки сохранены как
backlog; закрытые пункты отмечены в `recommendation.md`.

**Раунд 6 (11 июля 2026): защищённый сетевой транспорт импорта.** P0-10 закрыт
отдельным loopback egress-proxy на каждый запуск `yt-dlp`. Proxy повторно
резолвит и валидирует каждый HTTP/CONNECT target, отклоняет всю DNS-выборку при
наличии private/special IP и соединяется с уже проверенным `SocketAddr`, поэтому
редиректы и DNS rebinding не создают окно между проверкой и connect. Разрешены
только порты 80/443, а format selector допускает только media URL с `http://`/
`https://`, исключая прямой RTMP/FTP/WebSocket downloader bypass. DNS/connect
имеют общий bounded budget, proxy ограничен 64 соединениями на job. `yt-dlp`
запускается с `--ignore-config`, явным `--proxy`, proxy-env для дочерних
downloader'ов и без `NO_PROXY`. Реальные regression-тесты проверяют 302 на
loopback, смену DNS-ответа public→private и отказ RTMP-only metadata без
соединения с приватной целью.

**Раунд 7 (11 июля 2026): изоляция upload-ресурсов.** `AppState` больше не
публикует сырой job semaphore: очередь доступна через узкие методы, а upload
получил отдельный пул того же размера и fail-fast `429`, поэтому медленный
multipart не занимает render/download slots. Приём тела ограничен 30 минутами
и удаляет staging-файл при timeout; `ffprobe` ограничен 30 секундами, startup
tool checks - 5 секундами. Timeout явно делает kill+wait, а `kill_on_drop`
остаётся страховкой; probe-timeout возвращает отдельный `504`. Существующий
frontend показывает понятный текст ошибки inline и toast без отдельной UI-ветки.

**Раунд 8 (11 июля 2026): единая граница ошибок API.** Новый `error.rs`
инкапсулирует статус, машинный `code`, пользовательский текст и internal cause;
внутренние детали логируются, но не выдаются клиенту. Upload, projects, jobs,
library, JSON/multipart extractors и `404/405` fallbacks отвечают одним JSON
envelope. Project HTTP parsing и persistence дополнительно разделены
`ProjectPort`; production использует SQLite adapter, contract tests - in-memory
fake. Frontend использует один parser и typed `ApiError`, сохраняя
plain-text fallback для старого proxy. Regression-тесты покрывают malformed
JSON, routing errors, body limit, скрытие internal source и HTTP 500 vs network.

**Исследовательский раунд (14 июля 2026): ещё 100 решений.** Изучены 100
активных высокорейтинговых репозиториев в 10 группах: NLE/media pipeline,
кодеки/качество, playback, editor interactions, Rust backend, jobs/persistence,
Vue/testing, security/supply chain, observability/performance и ML-assisted
media. Выводы сверены с первичными papers и спецификациями FFmpeg, GStreamer,
MLT, Tokio, OWASP, W3C, OpenTelemetry и SLSA. Каждый источник дал отдельную
проверяемую карточку №784-883; популярность проекта не трактуется как команда
добавить зависимость. На этом этапе набор вырос до 665: исходные 565 + 100
исследовательских. Каталог со снимком звёзд и трассировкой решений - в
[docs/research-100.md](docs/research-100.md), формулировки - в
[architecture.md](architecture.md#исследовательский-слой-100-репозиториев-14-июля-2026),
чеклист - в
[recommendation.md](recommendation.md#исследовательский-чеклист-100-репозиториев-14-июля-2026).

**Волна 1/10 (14 июля 2026): первые 10 research-backed пунктов реализованы.**
Закрыты №790, 824, 828, 829, 831, 844, 846, 847, 850 и 854: runtime manifest
encoder/muxer/filter возможностей ffmpeg, `TaskSupervisor` для bounded structured
shutdown, transport-level
backpressure/disconnect tests, versioned strict wire DTO при tolerant project
documents, request/job/process spans с canary-redaction, ESLint dependency
boundaries, gzip bundle budget, fake-time polling, backendless Playwright matrix
и формальная upload threat matrix. Полное разбиение всех 100 задач на 10 волн -
в [recommendation.md](recommendation.md#волны-исполнения-по-10-пунктов); модель
upload-рисков - в [docs/threat-model-upload.md](docs/threat-model-upload.md).

**Волна 2/10 (14 июля 2026): media/editor domain и HTTP ports реализованы.**
Закрыты №784, 786, 787, 803, 814, 817, 820, 823, 825 и 826. Backend получил
чистые модули `FilterGraph`, media-service registry, stable timeline IDs,
normalized `ProbeResult`, geometry и keyframes; ffprobe и ffmpeg compiler уже
используют новые границы. Frontend больше не сериализует весь `EditState` для
undo: field-level commands и explicit pointer transactions дают один undo на
crop/censor drag. Branded coordinate spaces и общий Rust/TS fixture corpus
закрепляют transform/clamp/NaN invariants. System/projects маршруты работают
через ports и in-memory contract tests, route policy централизована. В policy
auth обозначен как local-only deployment boundary; публичные auth/ownership и
process resource limits остаются отдельными P0.

**Волна 3/10 (18 июля 2026): durable jobs и persistence реализованы.**
Закрыты №834-840, 842, 843 и 870. `backend/src/jobs/` теперь разделяет
append-only events/reducer, `JobAttempt` и retry taxonomy, failed registry,
dedupe/rate limits, lifecycle reconciliation, transactional outbox и per-job
`JobCell`. Import/edit атомарно сохраняют request + job + event + outbox;
повтор возвращает существующий ID, crash-boundary tests исключают частичные
enqueue, а dispatcher возобновляет abandoned lease. Attempt-scoped heartbeat
не даёт долгому ожиданию в очереди породить второй worker; crash-gap между
failure и retry восстанавливается по `next_retry_at`, graceful shutdown не
превращается в пользовательский cancel. Legacy snapshots мигрируют в event log,
terminal request payload удаляется. Validation/security не retry; временные
ошибки получают bounded backoff и максимум три attempts.
Operator retry/discard пишутся в audit. Добавлены content-addressed backup с
verify/restore drill, rebuildable `MediaSearch` port на SQLite FTS5 и измеримый
порог смены SQLite; локальные benchmark-прогоны 1000 WAL enqueue дали p95
0.241-0.632 ms при
пороге 50 ms. Loom перебирает terminal/cancel/permit interleavings.

**Волна 4/10 (18 июля 2026): производные media artifacts и performance
contracts реализованы.** Закрыты №789, 791, 792, 793, 795, 796, 798, 832, 871
и 872. Появились content-addressed proxy с verified relink и обязательным
full-resolution export, dependency graph с downstream invalidation, immutable
`EditPlan` и независимые preview/export profiles, deterministic frame/chunk
manifests с checksum, resumable scene chunks и verified stitch. Encoding теперь
получает cgroup-aware `EncodeBudget`; hashing/analysis вынесены в bounded Rayon
pool, а отдельный render admission удерживает общий encoder thread budget даже
при большом `MAX_CONCURRENT_JOBS`. Manifest reads действительно bounded,
artifact verification отклоняет ancestor-symlink escape, serde пересчитывает
graph/plan/chunk identities; proxy staging сохраняет muxer suffix и не копит
неиспользуемый progress. HLS/DASH packaging изолирован от обычного file export.
Версионированный cold/warm perf corpus пишет median/p95 и environment metadata; profile workflow
сохраняет SVG и folded stacks. Это завершённый contract/adapter слой: включение
proxy/chunk/package flows в пользовательский UI остаётся отдельной интеграцией.

**Typed export compiler (20 июля 2026).** `EditRequest` теперь заканчивается на
transport boundary. `EditPlan::compile` один раз нормализует запрос относительно
probe metadata и создаёт versioned `EditPlan` из `SourceMediaSpec`, сгруппированного
`EditSpec` и независимого `OutputSpec`. Smart types фиксируют time ranges,
geometry, rotation, presets, format/codec, CRF, fps и mute/audio semantics;
custom serde повторно проверяет инварианты и fingerprint. `RenderExecution`
добавляет только resource policy, а `ExportCommandCompiler` отделяет application
слой от `FfmpegExportCompiler`. Аргументы процесса и ожидаемая длительность
компилируются одним результатом, без второго source-duration параметра.
Открытыми намеренно остаются `Timeline -> EditPlan` compiler, generated Rust/TS
contract и переход render-cache key с raw request на normalized plan hash.

**Следующий исследовательский слой (18 июля 2026): ещё 100 идей.** Карточки
№884-983 разбиты на 10 пакетов: container provenance, color/HDR, audio, timed
text/accessibility, local-first storage, process isolation, reliability,
timeline UX, formal verification и local ML/privacy. Каждая содержит критерий
приёмки; source inventory и порядок маленьких PR находятся в
[docs/research-next-100.md](docs/research-next-100.md). Общий backlog теперь 765
пунктов, а ближайший P0 - process isolation №934, 937, 939 и 943.

**Process policy P0 (19 июля 2026).** Закрыты contract №934 и fail-closed tier
№943. Каждый production subprocess теперь требует `ProcessPolicy` и только
`PreparedCommand` может вызвать spawn. Environment очищается до allowlist,
FFmpeg/ffprobe получают offline protocol allowlist, downloader - только pinned
loopback proxy. Captured output и отдельная строка bounded; Unix применяет
CPU/FD/file-size rlimits, Linux также `RLIMIT_AS`; timeout и оставшиеся потомки
проходят ограниченный TERM grace period и затем принудительный KILL всей process
group. №936, 937 и 939 остаются
частично открыты до mount/network namespaces, cgroup pids/memory и per-tenant
uid; NsJail/Bubblewrap нельзя выбрать, пока adapter реально не реализован.

```
frontend (Vue 3 + Vite)
  └── POST /api/import { url }        -> { jobId }      (yt-dlp скачивает)
  └── GET  /api/jobs/:id              -> { status, result|error }
  └── POST /api/edit { videoId, ... } -> { jobId }      (ffmpeg обрабатывает)
  └── GET  /files/sources/<id>.<ext>  -> исходное видео (с поддержкой Range)
  └── GET  /files/outputs/<id>.mp4    -> результат

backend (Rust + Axum + Tokio)
  domain/          typed edit/output/source, filter graph, timeline, media primitives
  services/render  EditRequest + probe metadata -> immutable EditPlan v2
  jobs/            events, attempts, outbox, retry, registries, JobCell
  handlers/jobs.rs durable dispatch, lease heartbeat и operator HTTP API
  http/            wire DTO, routers и route/middleware policy
  ports/           export compiler и replaceable application boundaries
  tools/           FFmpeg compiler, ffprobe/yt-dlp/process adapters
  storage/staging/  приватный карантин незавершённых upload
  storage/sources/  скачанные оригиналы
  storage/outputs/  отрендеренные результаты
  storage/app.db    SQLite: проекты, job log/outbox, FTS, кэш рендеров
```

Импорт и экспорт идут асинхронно, фронтенд опрашивает статус. Проекты и задачи
персистятся в SQLite (`storage/app.db`): правки автосохраняются и восстанавливаются
при переоткрытии клипа. Задача, прерванная остановкой сервера, получает событие
`interrupted` и либо bounded retry через durable outbox, либо объяснимое
terminal-состояние после исчерпания policy. Одинаковый экспорт (тот же источник
и те же настройки) берётся из кэша рендеров без повторного запуска ffmpeg.

## Ограничения и предупреждения

- Скачивание чужого видео из VK и подобных сервисов — серая зона по их условиям
  использования и вопрос авторских прав. Используй для своего контента / в личных
  целях.
- Приватные и закрытые видео скачать нельзя — только публично доступные.
- Кадрирование можно задавать интерактивной рамкой прямо на видео (тянешь углы),
  а не только числами.
- Живое превью в браузере: цвет (яркость/контраст/насыщенность/пресеты), отражение,
  скорость и громкость видны сразу; обрезка зациклена внутри отрезка. Финальные
  поворот и ресайз видны после экспорта.
- Наложение текста (drawtext) и вшивание субтитров (subtitles) требуют сборки ffmpeg
  с `libfreetype`/`libass`; в стандартной brew-сборке их может не быть (в Docker-образе
  ffmpeg полный). Поэтому в редакторе пока цензура-прямоугольник, виньетка и letterbox.
- Это MVP: нет аутентификации. Проекты, задачи и медиатека персистятся (SQLite +
  файлы на диске) и переживают перезапуск. Не выставляй наружу как есть.
- Stored XSS через подменённое расширение локального upload закрыт: публикация
  происходит только после `ffprobe`, расширение выбирается из allow-list
  фактического контейнера, а статические ответы запрещают MIME-sniffing и
  активный document-контекст.
- SSRF через redirect-hop и DNS rebinding закрыт egress-proxy; импорт намеренно
  принимает только HTTP(S) на стандартных портах 80/443. Это не заменяет auth,
  rate limits и process sandbox, которые всё ещё обязательны перед публикацией.
