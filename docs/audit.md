# Аудит: топ-50 «сделано плохо/неправильно»

Дедуплицировано из 9-областного критического ревью (~108 сырых находок, каждая
проверена чтением кода). Легенда: 🔴 баг/уязвимость/потеря данных/гонка · 🟠
некорректность с риском · 🟡 долг/smell. Многие 🔴 подтверждены несколькими
независимыми аудиторами.

Связь с планом: критичные баги вынесены в P0 [recommendation.md](../recommendation.md);
архитектурные первопричины - в [architecture.md](../architecture.md) и
[refactor-plan.md](refactor-plan.md). Положительное (чтобы не переоценить риск):
инъекция в filtergraph закрыта (whitelist `filter_preset`/`sanitize_color`/
`parse_aspect` + числа), `Command` без shell, path-traversal через `video_id`
закрыт (скан директории по stem), upload реально стримится чанками, `tokio::select!`
в раннере `biased`, `MAX_UPLOAD_BYTES` применяется.

## Самое критичное (чинить первым)
№1 (persist при ошибке URL), №2 (зависание на закрытом семафоре), №5 (протухший
кэш), №14 (нет лимитов ffmpeg → OToM/DoS), №20 (нет миграций схемы), №27 (нет auth),
№28 (CORS permissive), №3 (внуки-процессы переживают kill).

## A. Корректность и гонки задач

1. 🔴 `handlers/mod.rs:53-61` - в `import_handler` ветка ошибки `validate_url` ставит `Error` только в памяти, **нет `persist_job`** (в отличие от всех других терминалов) → после рестарта джоба = `interrupted`, не `error`. → добавить `st.persist_job(&jid).await` перед `clear_cancel`.
2. 🔴 `handlers/mod.rs:66-69, 298-301` - при закрытом семафоре (`acquire_owned` → `Err`) воркер делает голый `return`: джоба навсегда `queued`/`pending`, токен отмены течёт, клиент крутит спиннер вечно. → перевести в `Error`, `persist`, `clear_cancel`.
3. 🔴 `tools/mod.rs` (`run_with_progress`) - `start_kill` убивает только родителя; внук `yt-dlp → ffmpeg` не в process-group и переживает отмену/таймаут. → убивать всю process-group (`process_group(0)` + kill negative pgid), ограничить reap-`wait()` таймаутом.
4. 🔴 `main.rs:90-93` - `with_graceful_shutdown` завершает только HTTP; воркеры и их ffmpeg/yt-dlp при SIGINT осиротевают, частичные файлы остаются. → `TaskTracker` + отмена воркеров на shutdown.
5. 🔴 `handlers/mod.rs` (`spawn_progress_drain` ↔ `finish_job`) - запоздавший progress-тик может перетереть терминальный статус/`progress` после финала (две задачи, две блокировки). → игнорировать тики после терминального статуса внутри `update_job`.
6. 🔴 ожидание permit не отменяемо (`handlers/mod.rs:64` `acquire_owned().await`) - отмена «queued»-джобы не прерывает ожидание; токен снят, воркер не видит отмены, пока не дойдёт очередь. → `select!` permit против `token.cancelled()`.
7. 🟠 `state.rs:92-109` (`recover_jobs`) - держит `jobs` Mutex и `await db.persist_job` в цикле → сериализация старта под локом. → собрать, персистить вне лока.
8. 🟠 `handlers/mod.rs:382-409` (`cancel_handler`) - TOCTOU: проверка статуса и перевод в `Cancelled` в двух разных захватах лока; гонка с `finish_job(Done)`. → одна критическая секция.
9. 🟠 `state.rs` (`set_job`/`persist_job`) - persist отдельным локом после `update_job` → persisted-строка может разойтись с памятью. → персистить именно то значение, что только что записал.
10. 🟠 `handlers/mod.rs:104-106` - отмена импорта **не** чистит частично скачанный source-файл (edit для output чистит). → чистить по `vid` при cancel.
11. 🟡 `state.rs:127` (`cancel`) - удаляет токен вместо только `token.cancel()` → возвращаемый bool ненадёжен. → удалять токен только в `clear_cancel`.
12. 🟡 `model.rs:38` (`from_token`) - неизвестный токен молча → `Pending`, без лога. → логировать неизвестный токен.

## B. Безопасность

13. 🔴 `lib.rs` - **нет аутентификации** ни на одном эндпоинте: `/api/import|edit|upload|projects`, `/files/*` открыты любому, кто достучался до порта. → middleware с токеном.
14. 🔴 `lib.rs:56` - `CorsLayer::permissive()` (`Allow-Origin: *`) на мутирующих POST/DELETE → любой сайт в браузере жертвы дёргает API. → ограничить origin/методы.
15. 🔴 `tools/net.rs` + `tools/mod.rs` - **SSRF: DNS-rebinding и redirect не закрыты**: домен резолвится не нами, а yt-dlp, и ходит по редиректам на `169.254.169.254`/`127.0.0.1`. → резолвить host и проверять все A/AAAA, запретить приватные редиректы (или сетевая песочница).
16. 🟠 `tools/net.rs` - неполный список диапазонов: `100.64.0.0/10` (CGNAT) и IPv4-mapped IPv6 (`::ffff:127.0.0.1`) проходят. → полный special-use allowlist.
17. 🟠 `lib.rs:54` - `ServeDir` отдаёт весь `storage` (sources+outputs, рядом `app.db`) без auth; чужие исходники/рендеры по предсказуемым путям. → вынести БД из `storage`, отдавать за авторизацией.
18. 🟠 `model.rs` + `tools/mod.rs` - нет лимита размера/длительности входа при импорте (только `MAX_HEIGHT`) → заполнение диска. → `--max-filesize`, лимит длительности.
19. 🟠 `handlers/mod.rs` (upload) - нет проверки размера до записи и MIME/magic-байт, нет квоты. → ранний отказ по размеру в цикле чанков, проверка контейнера.
20. 🟡 `args.rs` - числовые поля (`scale`/`fps`/`brightness`/`crop`/`fade`) не клампятся (в отличие от `sharpen`/`grain`/`speed`) → абсурдные значения = тяжёлый/падающий ffmpeg. → клампить все в одном месте.
21. 🟡 `model.rs` - нет `#[serde(deny_unknown_fields)]` на `EditRequest`/`ImportRequest`; неизвестный `filter` молчит. → строгая десериализация.
22. 🟡 `projects.rs` - `project_upsert` принимает произвольный `Json<Value>` (video/edit) без лимита размера/глубины. → типизированный DTO + лимит.

## C. Ресурсы / DoS-лимиты

23. 🔴 раннер ffmpeg - нет лимита CPU/RAM/threads (`-threads`/`nice`/cgroup); только семафор по числу задач → N×all-core энкодов насыщают/OOM хост. → `-threads`, лимиты в compose.
24. 🟠 `handlers/mod.rs` - progress-канал `unbounded` + per-tick `update_job` под локом → всплеск тиков растит память и бьёт по Mutex. → bounded/throttle прогресс.
25. 🟠 `tools/mod.rs` - нет no-progress watchdog: зависший-но-живой процесс держит слот весь `JOB_TIMEOUT_SECS` (1800с). → watchdog по бездействию.
26. 🟠 `db.rs` - pool `max_connections=5` + `busy_timeout=5s`; persist под нагрузкой может терять (warn проглочен) для терминальных статусов. → больше соединений / отдельный writer, не глушить ошибку терминала.

## D. Данные и целостность

27. 🔴 `handlers/mod.rs:273` + `db.rs` + `library.rs` (`remove`) - `render_cache` не инвалидируется при удалении файла/источника → cache-hit на битый файл, 404 при скачивании, повтор всегда из кэша. → `cache_delete` при промахе файла и при `library.remove`.
28. 🔴 `db.rs` (SCHEMA) - нет миграций (только `CREATE IF NOT EXISTS`; `schema_version` не читается) → ALTER на существующей БД молча не применится. → `PRAGMA user_version` + пошаговые миграции.
29. 🔴 `handlers/mod.rs:255` - `render_cache_key = to_string(req).unwrap_or_default()`: при ошибке сериализации → пустой ключ, коллизия, чужой результат рендера. → не кэшировать при ошибке сериализации.
30. 🟠 два хранилища: `library.json` vs SQLite - без FK/связи, рассинхрон (усиливает №27). → унифицировать в SQLite.
31. 🟠 `handlers/mod.rs:358` - `cache_put` и `library.add` не атомарны: краш между ними → кэш-без-библиотеки или файл-сирота. → записывать кэш после успешной регистрации.
32. 🟠 jobs/render_cache - нет ретеншна, растут вечно; `recover_jobs` грузит все строки в память. → TTL-очистка терминальных + лимит загрузки.
33. 🟠 `library.rs:80-97` - `library.json` перезаписывается целиком под Mutex на каждый add/remove (+`clone` всего вектора), O(N). → SQLite/JSONL/дебаунс.
34. 🟠 `db.rs` - нет индексов на `jobs(updated_at)`/`projects(updated_at)`/`render_cache(created_at)` (есть только `idx_projects_video_id`). → добавить индексы.
35. 🟡 `db.rs` - `now_secs` импортируется из `library` (перепутанные границы) + `unwrap_or(0)` при сбое часов → `created_at=0` ломает сортировку. → общий `util`, не глушить нулём.
36. 🟡 `state.rs:60` - `set_job` игнорирует ошибку persist: клиент получает `jobId`, но после рестарта джоб потерян. → на критический сбой persist возвращать ошибку.
37. 🟡 нет single-flight по cache_key: два идентичных параллельных edit мимо кэша оба рендерят + дублируют library/cache, осиротевший output. → in-flight карта по ключу.

## E. Ошибки и API-контракт

38. 🔴 `finish_job` + `tools/mod.rs` + `api.ts:143` - сырая `anyhow`-строка (`ffmpeg failed: <лог>`, `source video <uuid> not found`) пишется в `job.error` и показывается пользователю. → `enum JobError{код, безопасный текст}`, технику в логи.
39. 🔴 `handlers/mod.rs` - создание async-задачи отвечает `200`, а не `202 Accepted` (неотличимо от завершения). → `StatusCode::ACCEPTED`.
40. 🟠 `projects.rs`/`library.rs`/`mod.rs` - три разные формы ошибки в одном API (`(StatusCode,String)` / голый `StatusCode` / `json!{error}`) → фронт не парсит единообразно. → общий `ApiError: IntoResponse`, единый JSON-конверт.
41. 🟠 `projects.rs:64-91` - `Err(_) => INTERNAL_SERVER_ERROR` глотает причину БД без тела и без лога; та же ошибка в других местах отдаётся с текстом. → единый маппинг + `tracing::error!`.
42. 🟠 `handlers/mod.rs` - проглоченные `.ok()`/`let _ =` на `cache_get`/`cache_put`/`metadata` без логирования → отладка вслепую. → логировать ветки ошибок.
43. 🟡 `api.ts:25` - фронт трактует любой `status >= 500` как «бэкенд недоступен», маскируя реальные 500. → различать сетевой сбой и HTTP 5xx с телом.
44. 🟡 валидация импорта асинхронная: явно пустой/битый URL принимается (200+jobId), падает только в воркере. → синхронная проверка до spawn, `400`.

## F. Производительность

45. 🔴 `tools/mod.rs` (`probe_video`) - ffprobe запускается на **каждый** import/edit, метаданные не кэшируются (хотя лежат в `library.json`/`projects.video_json`). → кэш ProbeInfo по `video_id`.
46. 🔴 `library.rs:103-113` - `list()` делает последовательный `metadata` для каждого элемента на **каждый** `GET /api/library` (O(N) stat, await в цикле). → кэш существования / `join_all`.
47. 🟠 `tools/mod.rs:77` + `handlers` - `MAX_HEIGHT`/`JOB_TIMEOUT_SECS` парсятся из env на каждый запрос (hot-path), а не один раз. → `Config`/`OnceLock` при старте.
48. 🟠 нет smart-render (`-c copy`, когда пиксели не трогаются) и нет GPU/hwaccel → всегда перекодирование, многократно медленнее на длинных видео. → copy-ветка + опц. hwaccel.
49. 🟠 `store.ts:440,598` + `VideoPreview.vue:9` - два deep-watch на пересекающихся `state.edit` + запись `playerTime` на каждый `timeupdate` (~4-66/с) → двойной обход дерева, лишние реактивные апдейты. → объединить watch, throttle `playerTime`.
50. 🟡 поллинг статуса каждые 500мс на всё время задачи (минуты) → лишние round-trip’ы. → backoff или SSE.

## G. Frontend (структура/качество) — сверх №43/49/50

- 🔴 `store.ts:440,598` - два `watch` регистрируются как **сайд-эффект импорта модуля** (нельзя выключить/замокать; текут между тестами). → `initStore()`/composable.
- 🔴 god-модуль `store.ts` (один reactive, прямые мутации из компонентов) + god-компонент `EditPanel.vue` (724 строки). → разбить (см. recommendation.md P1-5/6).
- 🟠 `store.ts:259` - `buildEditPayload` молча дропает `codec` для не-mp4 и `quality` для prores/gif/png/jpg/mp3 → потеря намерения юзера без предупреждения. → типобезопасный маппер по формату.
- 🟠 `store.ts:514` - `applyPreset` делает `Object.assign` без фильтра `PRESET_KEYS` → устаревший localStorage может затереть `crop`/`censor`/`trimEnd`. → применять только `PRESET_KEYS`.
- 🟠 проглоченные `catch {}` без лога по всему фронту (`api.ts`/`store.ts`). → хотя бы `console.warn`.
- 🟠 a11y: нет `fieldset/legend`, `role=radio`/`aria-pressed` на chip, `aria-invalid` на time-input. → семантика + ARIA.
- 🟡 `root.value!` и `as`-касты без runtime-валидации ответов сервера. → ранний guard + проверка формы.

## H. Тесты

- 🔴 нет теста на баг №1 (persist); `import_rejects_unsafe_url_via_job_error` проверяет только память. → restart-style тест.
- 🔴 нет интеграционного happy-path рендера через `edit_handler` (семафор/drain/library/cache не покрыты). → тест под skip-если-нет-ffmpeg.
- 🟠 `poll_terminal` флейкоопасен: все `#[tokio::test]` current-thread, happy-path зависит от 400×`sleep(10ms)`. → `multi_thread` или событийное ожидание.
- 🟠 undo/redo/history (`store.ts:381-447`), `map_ytdlp_error`, `sanitize_ext`, `run_with_progress`(kill), recover(`running→interrupted`) - не покрыты. → юнит-тесты.
- 🟡 нет golden/snapshot на форму JSON-ответов (контракт с фронтом); нет e2e/Playwright; нет теста стабильности `render_cache_key`.

## I. Сборка / конфиг / Docker / CI

- 🔴 `main.rs:84` - `BIND_ADDR` fail-soft → тихий `127.0.0.1` (контейнер недостижим, лог не покажет ошибку). → строгий парсинг, `bail!`.
- 🔴 `Dockerfile`/`compose` - бэкенд от root, нет `USER`/`HEALTHCHECK`/resource-limits. → непривилегированный user, healthcheck, лимиты.
- 🔴 `Dockerfile:11` - `yt-dlp` без пиннинга версии (latest) + ffmpeg из apt без версии → невоспроизводимо, парсинг прогресса может сломаться. → пиннинг + sha256.
- 🟠 `ci.yml` - ставит ffmpeg, не ставит yt-dlp (import-путь не покрыт в CI). → добавить yt-dlp.
- 🟠 `main.rs` - все числовые env fail-soft (`.parse().ok().unwrap_or`): `MAX_CONCURRENT_JOBS=0`/опечатка молча → дефолт. → ошибка старта.
- 🟠 env россыпью из ~9 мест, нет `Config` (см. recommendation.md P1-4).
- 🟡 README «Node 18+» vs CI/Docker Node 20 + vite6/eslint9; README не упоминает `RUST_LOG`; Dockerfile без слоистого кэша deps; нет MSRV (`rust-version`).

---

Итог: ~18 🔴 (баги/уязвимости/гонки), остальное - 🟠/🟡. Самые повторяемые
(подтверждены 3+ аудиторами): №1 persist, №2 семафор-зависание, №27 протухший
кэш, №32 рост recover_jobs. Порядок устранения - в
[recommendation.md](../recommendation.md) (P0 → P1 → P2).
