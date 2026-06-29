# Рекомендации: что делать дальше (чеклист)

Приоритизированный, гранулярный план, синхронизированный с
[architecture.md](architecture.md) (целевой дизайн),
[docs/refactor-plan.md](docs/refactor-plan.md) (шаги рефакторинга) и
[docs/ideas/round-13.md](docs/ideas/round-13.md) (фичи).

Порядок: **корректность → дешёвая модульность → глубокий рефактор**; фичи
параллельно. Каждая задача идёт под зелёным `make check`. **S** = часы, **M** =
день-два, **L** = неделя+. У каждой задачи - файлы, шаги, критерий приёмки.

---

## P0 - Корректность (чинить первым)

> Полный аудит (топ-50 проблем по серьёзности) - в [docs/audit.md](docs/audit.md).
> Ниже - первоочередные баги из него.

### ☐ P0-1. `persist_job` при ошибке URL в импорте · S
- **Файлы:** `backend/src/handlers/mod.rs` (`import_handler`, ветка `validate_url`, ~53-59).
- **Шаги:**
  - [ ] В ветке `if let Err(e) = tools::validate_url(...)` добавить `st.persist_job(&jid).await;` перед `st.clear_cancel(&jid).await;`.
  - [ ] Тест в `backend/tests/api.rs`: импорт с плохим URL → poll до `error`; затем новый `AppState` поверх той же БД + `recover_jobs()` → джоба остаётся `error`, **не** `interrupted`.
- **Критерий:** новый тест зелёный; `make check` зелёный.

### ☐ P0-2. Ретеншн `recover_jobs` · S
- **Файлы:** `backend/src/state.rs` (`recover_jobs`), `backend/src/db.rs` (`load_jobs`).
- **Шаги:**
  - [ ] В `db.rs` добавить `load_recent_jobs(limit)` (ORDER BY updated_at DESC LIMIT) или `prune_jobs_older_than(ts)`.
  - [ ] `recover_jobs` грузит только последние N (env `RECOVER_JOBS_LIMIT`, дефолт 200) и/или чистит терминальные старше TTL.
  - [ ] db-тест: при >N сохранённых джобах загрузка возвращает ≤N.
- **Критерий:** db-тест зелёный; recover не падает на пустой/большой БД; `make check`.

### ☐ P0-3. DNS-rebinding в `validate_url` · S (пометка) / M (резолвинг)
- **Файлы:** `backend/src/tools/net.rs`; (документация уже в architecture.md/README).
- **Шаги:**
  - [ ] Минимум (S): явный комментарий-ограничение в `net.rs` + строка в «Ограничениях» README.
  - [ ] Полноценно (M): резолвить host (`(host, 0).to_socket_addrs()`), прогнать каждый IP через `is_blocked_ip`; отклонять, если хоть один приватный.
- **Критерий:** для S - комментарий + тест текущего поведения; для M - тест, что хост, резолвящийся в loopback/RFC1918, отклоняется (за фичей/мокабельным резолвером).

### ☐ P0-4. Зависание джобы на закрытом семафоре · S
- **Файлы:** `backend/src/handlers/mod.rs` (ветки `acquire_owned() => Err`, ~66-69, 298-301).
- **Шаги:**
  - [ ] В ветке `Err(_)` перевести джобу в `Error`, `persist_job`, `clear_cancel` (вместо голого `return`).
  - [ ] (опц.) `select!` ожидания permit против `token.cancelled()`, чтобы отмена «queued» прерывала очередь.
- **Критерий:** тест с `max_concurrent=1` и забитым слотом: вторая джоба не висит вечно.

### ☐ P0-5. Целостность render_cache · S
- **Файлы:** `backend/src/handlers/mod.rs` (cache_get ~273), `backend/src/db.rs`, `backend/src/library.rs` (`remove`).
- **Шаги:**
  - [ ] При промахе файла (`metadata` fail) удалять запись кэша (`cache_delete(key)`) и падать в обычный рендер.
  - [ ] (вместе с P2-10) инвалидировать кэш при `library.remove` и при TTL-чистке.
- **Критерий:** тест «cache_put без файла → джоба идёт в рендер, не Done мгновенно».
- **Прим.:** изначальный пункт «пустой ключ при ошибке сериализации» снят - верификация показала, что `serde_json` пишет `null` для не-finite f64 (не ошибка), ключ не пустеет (audit.md, раздел опровергнутого).

### ☐ P0-6. Вырезание сегмента молча не работает для AV1/ProRes · S
- **Файлы:** `backend/src/tools/args.rs` (~234, 491-496), `frontend/src/store.ts` (~213-226).
- **Шаги:**
  - [ ] Строить concat-ветку для всех видеоформатов (или явно запрещать сегменты в UI для AV1/ProRes с предупреждением).
  - [ ] Тест: edit с `segments` + format=av1/prores даёт concat-аргументы (или явную ошибку), а не неразрезанный экспорт.
- **Критерий:** тест зелёный; пользовательский вырез не пропадает молча.

### ☐ P0-7. Гонка cancel↔finish и очистка при отмене импорта · S
- **Файлы:** `backend/src/handlers/mod.rs` (`finish_job` ~427-459, `cancel_handler` ~382-409, import cancel ~104-106).
- **Шаги:**
  - [ ] В `finish_job` гейтить запись статуса на `!j.status.is_terminal()` (как уже делает `cancel_handler`), чтобы отменённая задача не перезаписалась в `Done`.
  - [ ] В ветках Cancelled/Err импорта подмести `sources/` по префиксу `vid` (+ `.info.json`, `.part`).
  - [ ] Тест: late-cancel завершённой задачи не превращает её в Done; отменённый импорт не оставляет файлов.
- **Критерий:** тесты зелёные; `make check`.

### ☐ P0-8. crop/geometry валидируется против размеров источника · S
- **Файлы:** `backend/src/tools/args.rs` (crop ~75-79), `backend/src/handlers/mod.rs` (probe ~331), `frontend/src/components/RectOverlay.vue` (~87-96).
- **Шаги:**
  - [ ] Клампить crop/censor к `width/height` из probe; на бэкенде - перед `build_ffmpeg_args`.
  - [ ] На фронте при эмите overlay: `x = min(x, W-w)`, чтобы `x+w ≤ W`.
- **Критерий:** crop за кадром не роняет ffmpeg (и не утекает stderr); тест на клампинг.

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
