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
- Цвет: яркость/контраст/насыщенность, пресеты (ч/б, сепия, тёплый, холодный, teal-orange, выцветший, нуар, винтаж), виньетка, шумодав, резкость и зерно.
- Замазать область прямоугольником (выбор рамкой на видео), поля под пропорции (letterbox 9:16, 1:1, и т.д.).
- Экспорт в MP4 (H.264/H.265 + AAC), WebM (VP9 + Opus), AV1, ProRes (.mov), GIF,
  стоп-кадр PNG/JPG или аудио MP3; выбор качества и пресеты под платформы
  (Telegram/Shorts/Reels/YouTube).
- Скачивание с осмысленным именем файла.
- Медиатека: импортированные источники и результаты сохраняются между перезапусками
  (`storage/library.json`), можно переоткрыть клип в редакторе, скачать или удалить.
- Проекты: правки автосохраняются и восстанавливаются при переоткрытии клипа (SQLite);
  задачи переживают перезапуск, повторный одинаковый экспорт берётся из кэша.
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
GET  /api/jobs/:id          -> { status, progress?, stage?, result?, error? }
POST /api/jobs/:id/cancel   -> 200 cancelled | 404 | 409
GET  /api/library           -> [ MediaEntry ]  (sources + outputs, newest first)
DELETE /api/library/:id     -> 204 | 404  (also deletes the file)
POST /api/projects          { videoId, video, edit, name? } -> Project  (autosave/upsert by clip)
GET  /api/projects          -> [ Project ]  (newest first)
GET  /api/projects/by-video/:videoId -> Project | 404
GET  /api/projects/:id      -> Project | 404
DELETE /api/projects/:id    -> 204 | 404
GET  /api/health            -> { status, ffmpeg, ytdlp, ffmpegVersion, ytdlpVersion }
GET  /files/sources/...     -> исходники (с поддержкой Range)
GET  /files/outputs/...     -> результаты (с поддержкой Range)
```

Все ошибки приложения в `/api` имеют один JSON-контракт:
`{"error":"Понятное сообщение","code":"machine_readable_code"}`. Frontend
сохраняет `status`/`code` в `ApiError`; сообщение о недоступном backend
используется только при сетевой ошибке, а не для настоящего HTTP `5xx`.

Переменные окружения: `PORT` (8080), `BIND_ADDR` (127.0.0.1), `STORAGE_DIR` (storage),
`MAX_HEIGHT` (720), `MAX_CONCURRENT_JOBS` (2; размер независимых job/upload
пулов), `JOB_TIMEOUT_SECS` (1800),
`FILE_TTL_HOURS` (0 = выключено), `MAX_UPLOAD_BYTES` (2 ГиБ),
`RECOVER_JOBS_LIMIT` (200), `CORS_ALLOW_ORIGINS` (локальные dev-origin'ы через
запятую), `RUST_LOG` (`info,tower_http=info`).

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
`parseTime`, пресеты).

```
make check   # всё как в CI: fmt + clippy + cargo test + lint + typecheck + vitest + build
make test    # только тесты (cargo test + vitest)
make lint    # clippy -D warnings + eslint
make fmt     # cargo fmt
```

Точечно:
```
cd backend  && cargo test
cd backend  && cargo fmt --check
cd backend  && cargo clippy --all-targets -- -D warnings
cd frontend && npm run lint
cd frontend && npm run typecheck
cd frontend && npm run test
```

CI (GitHub Actions, `.github/workflows/ci.yml`) на push/PR в `main` ставит ffmpeg
и закреплённый `yt-dlp`, гоняет для бэкенда `cargo fmt --check`,
`clippy -D warnings`, `cargo test`, а для фронтенда — lint, typecheck, тесты и
сборку.

## Архитектура

Что делать дальше (приоритизированный план действий) - в
[recommendation.md](recommendation.md). Подробный модульный дизайн (целевой «как с
нуля» + текущее состояние и правила зависимостей) - в [architecture.md](architecture.md)
и совместимом alias-файле [arhitecture.md](arhitecture.md).
Пошаговый рефакторинг на слабую зацепленность - в [docs/refactor-plan.md](docs/refactor-plan.md).
Порядок исполнения по фазам - в [plan.md](plan.md).
Исследовательский benchmark 100 сильных media/editor/backend/security-проектов,
papers и стандартов - в [docs/research-100.md](docs/research-100.md).
Аудиты проблем: проверенное ядро (118 находок, поштучно верифицировано, 14
опровергнутых) - в [docs/audit.md](docs/audit.md); расширенный широкий охват
(509 заземлённых на код проблем) - в [docs/audit-500.md](docs/audit-500.md);
единый бэклог с идеями/фичами - в [docs/ideas/top-200-backlog.md](docs/ideas/top-200-backlog.md).
Синхронизированный top-200 продублирован в `architecture.md` как диагноз и в
`recommendation.md` как карта исправлений; источник правды по багам - `docs/audit.md`,
порядок работ - `plan.md`. Если нужен именно большой список «500 предложений,
улучшений, проблем и ошибок», читай так: `docs/audit-500.md` = 509 широких
находок по коду; `architecture.md` и `recommendation.md` = 665 активных
рекомендаций: 565 SOLID/DRY-пунктов в 12 маленьких модулях плюс 100
research-backed решений в 10 тематических группах. Их можно брать по одному
кусочку.

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
envelope. `projects.rs` дополнительно разделён на parsing/name resolution и
persistence. Frontend использует один parser и typed `ApiError`, сохраняя
plain-text fallback для старого proxy. Regression-тесты покрывают malformed
JSON, routing errors, body limit, скрытие internal source и HTTP 500 vs network.

**Исследовательский раунд (14 июля 2026): ещё 100 решений.** Изучены 100
активных высокорейтинговых репозиториев в 10 группах: NLE/media pipeline,
кодеки/качество, playback, editor interactions, Rust backend, jobs/persistence,
Vue/testing, security/supply chain, observability/performance и ML-assisted
media. Выводы сверены с первичными papers и спецификациями FFmpeg, GStreamer,
MLT, Tokio, OWASP, W3C, OpenTelemetry и SLSA. Каждый источник дал отдельную
проверяемую карточку №784-883; популярность проекта не трактуется как команда
добавить зависимость. Общий исполняемый набор теперь 665: исходные 565 + 100
исследовательских. Каталог со снимком звёзд и трассировкой решений - в
[docs/research-100.md](docs/research-100.md), формулировки - в
[architecture.md](architecture.md#исследовательский-слой-100-репозиториев-14-июля-2026),
чеклист - в
[recommendation.md](recommendation.md#исследовательский-чеклист-100-репозиториев-14-июля-2026).

```
frontend (Vue 3 + Vite)
  └── POST /api/import { url }        -> { jobId }      (yt-dlp скачивает)
  └── GET  /api/jobs/:id              -> { status, result|error }
  └── POST /api/edit { videoId, ... } -> { jobId }      (ffmpeg обрабатывает)
  └── GET  /files/sources/<id>.<ext>  -> исходное видео (с поддержкой Range)
  └── GET  /files/outputs/<id>.mp4    -> результат

backend (Rust + Axum + Tokio)
  storage/sources/  скачанные оригиналы
  storage/outputs/  отрендеренные результаты
  storage/app.db    SQLite: проекты, задачи, кэш рендеров
```

Импорт и экспорт идут асинхронно, фронтенд опрашивает статус. Проекты и задачи
персистятся в SQLite (`storage/app.db`): правки автосохраняются и восстанавливаются
при переоткрытии клипа, а задача, прерванная остановкой сервера, после перезапуска
помечается `interrupted` (вместо вечного спиннера). Одинаковый экспорт (тот же
источник и те же настройки) берётся из кэша рендеров без повторного запуска ffmpeg.

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
