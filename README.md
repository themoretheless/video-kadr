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
GET  /files/...             -> исходники и результаты (с поддержкой Range)
```

Переменные окружения: `PORT` (8080), `BIND_ADDR` (127.0.0.1), `STORAGE_DIR` (storage),
`MAX_HEIGHT` (720), `MAX_CONCURRENT_JOBS` (2), `JOB_TIMEOUT_SECS` (1800),
`FILE_TTL_HOURS` (0 = выключено), `MAX_UPLOAD_BYTES` (2 ГиБ).

## Требования

- Rust (cargo)
- Node.js 18+
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

Нужны оба процесса. Если фронтенд показывает «Сервер недоступен» или раньше отдавал
`/api/import -> HTTP 500`, значит не поднят бэкенд: dev-прокси Vite не может достучаться
до `:8080` и отвечает 500. Запусти `cargo run` в `backend`.

## Разработка и тесты

Бэкенд покрыт юнит-тестами (сборка ffmpeg-аргументов, валидация URL, медиатека,
модель) и HTTP-интеграционными тестами (роутер гоняется через
`tower::ServiceExt::oneshot`, без сокета). Есть один реальный ffmpeg-тест рендера
(`backend/tests/render.rs`), который сам пропускается, если `ffmpeg`/`ffprobe` нет
в `PATH`. Фронтенд — ESLint + Vitest на логику стора (`buildEditPayload`,
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

CI (GitHub Actions, `.github/workflows/ci.yml`) на push/PR в `main` ставит ffmpeg,
гоняет для бэкенда `cargo fmt --check`, `clippy -D warnings`, `cargo test`, а для
фронтенда — lint, typecheck, тесты и сборку.

## Архитектура

Что делать дальше (приоритизированный план действий) - в
[recommendation.md](recommendation.md). Подробный модульный дизайн (целевой «как с
нуля» + текущее состояние и правила зависимостей) - в [architecture.md](architecture.md)
и совместимом alias-файле [arhitecture.md](arhitecture.md).
Пошаговый рефакторинг на слабую зацепленность - в [docs/refactor-plan.md](docs/refactor-plan.md).
Аудит «топ-200 проблем» (баги/безопасность/перф/тесты) - в [docs/audit.md](docs/audit.md).
Синхронизированный top-200 теперь продублирован в `architecture.md` как диагноз и
в `recommendation.md` как карта исправлений; источником правды остаётся
верифицированный `docs/audit.md`.

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
