# Аудит: топ-200 «сделано плохо/неправильно»

Получено двумя проходами мульти-агентного ревью: 10 линз ищут проблемы по
реальному коду → каждую находку отдельный «скептик» проверяет, читая код, и
пытается **опровергнуть** (при неуверенности - отклонить). 104 сырых находки →
**118 подтверждено** верификацией, **14 опровергнуто** (ложные срабатывания, см.
конец). Первые 56 пунктов ниже - верифицированные дефекты/риски с привязкой к
коду; пункты 57-200 добивают список до архитектурного debt register и
синхронизированы с `architecture.md`/`recommendation.md`. Это заменяет первый
однопроходный вариант: две прошлые 🔴 («пустой ключ render_cache» и «fan-out
playerTime») оказались ложными и убраны.

Легенда: 🔴 баг/функц. дефект/потеря данных/гонка, бьёт по локальному
пользователю · 🟠 реальный риск · 🟡 долг/нит. Метка **⚠выставление** = проявляется
прежде всего при выставлении сервиса наружу (сейчас это локальный однопользова-
тельский MVP, README уже предупреждает «не выставляй как есть»).

Что НЕ баг (подтверждено верификацией, не переоценивать риск): инъекция команд
(нет shell, argv-массивы), path-traversal через `video_id` (скан директории по
stem, без join), `library.json` пишется атомарно (tmp+rename), полная сериализация
ключа кэша уже включает формат/расширение.

## Самое критичное (чинить первым)
№1 (segments молча игнорируются для AV1/ProRes - функц. дефект), №2 (гонка
cancel↔finish: отменённая задача может стать Done), №3 (отмена/таймаут импорта
оставляет мусор на диске), №4 (crop не валидируется против размеров источника →
обрыв ffmpeg + утечка пути), №17 (upload минует семафор - DoS), №23 (TTL-чистка
удаляет файлы без проверки ссылок → сироты), №32 (SSRF обходится через DNS yt-dlp).

## A. Корректность задач и гонки

1. 🔴 `handlers/mod.rs:382-409` + `427-459` - гонка `cancel_handler`↔`finish_job`: две неатомарные секции под разными захватами лока, `finish_job` (в отличие от cancel) НЕ проверяет `is_terminal()` перед записью → отменённая пользователем задача может перезаписаться в `Done`. → гейтить терминальный переход на `!is_terminal()`/токен отмены в одной критической секции.
2. 🔴 `handlers/mod.rs:104-106, 439-454` - отмена/таймаут импорта не чистит частично скачанное (`<id>.*`, `.part`, `<id>.info.json`); edit-путь чистит (`:337`), import - нет; при `FILE_TTL_HOURS=0` мусор копится бесконечно. → подмести `sources/` по префиксу `vid` в ветках Cancelled и Err.
3. 🔴 `handlers/mod.rs:268-291` + `382-409` - кэш-хит edit отменяем в негейченном окне → мгновенно готовый рендер репортится как `Cancelled`. → проверять статус/токен перед записью Done на кэш-хите.
4. 🟠 `tools/mod.rs:303, 346-349` - внук `ffmpeg`, порождённый `yt-dlp`, осиротевает при отмене/таймауте (убит только прямой ребёнок, нет process-group). → `process_group(0)` + kill negative pgid.
5. 🟠 `main.rs:90-93` - graceful shutdown бросает in-flight воркеры и их дочерние процессы (HTTP закрыт, задачи нет). → `TaskTracker` + отмена воркеров.
6. 🟡 `handlers/mod.rs:64-79, 296-311` - ожидание permit не отменяемо: отменённая «queued» задача всё равно держит очередь. → `select!` permit против `token.cancelled()`.
7. 🟡 `handlers/mod.rs:66-69, 298-301` - ветка `acquire_owned() => Err` делает голый `return`: задача навсегда в non-terminal, течёт cancel-токен. → перевести в `Error`, `persist`, `clear_cancel`.
8. 🟡 `handlers/mod.rs:412-422` - progress-drain продолжает писать `progress` на уже отменённую/терминальную задачу. → игнорировать тики после терминала.

## B. Геометрия и сегменты (функциональная корректность правок)

9. 🔴 `tools/args.rs:234, 491-496` + `store.ts:213-226` - вырезание куска (segments) молча НЕ работает для AV1 и ProRes: concat-ветка не строится, экспортируется неразрезанное. → строить concat для всех видеоформатов или явно запрещать в UI.
10. 🔴 `handlers/mod.rs:331-333` + `tools/args.rs:75-79` - crop не валидируется против размеров источника (probe берётся только для duration); выход за кадр → ffmpeg падает → сырой stderr с путём уходит юзеру. → клампить crop к `width/height` из probe.
11. 🟠 `RectOverlay.vue:87-96` + `args.rs:75` - независимое округление `x` и `w` даёт `x+w > width` даже из штатного overlay. → клампить `x = min(x, W-w)` при эмите.
12. 🟠 `tools/args.rs:491-496, 420-442` - сегменты не сортируются: переставленные сегменты перемешивают таймлайн. → сортировать по `start`.
13. 🟠 `tools/args.rs:407-431` - концы сегментов не клампятся к длительности (`end > duration` идёт сырым в `trim/atrim`). → клампить к `source_duration`.
14. 🟠 `tools/args.rs:493, 420-430` - отрицательный `start` принимается и эмитится как `trim=start=<negative>`. → отбраковывать/клампить `>= 0`.
15. 🟠 `tools/args.rs:420-442` - перекрывающиеся сегменты дублируют кадры без детекции. → мержить/отклонять пересечения.
16. 🟠 `tools/args.rs:208-215, 280-293` - `fps` без верхней границы идёт прямо в `-r` (а в gif - в `palettegen/paletteuse`) → взрыв числа кадров, CPU/mem DoS; крошечный `0.0001` обходит `>0.0` и даёт `-r 0.000`. → клампить fps в разумный диапазон.
17. 🟡 `tools/args.rs:96-98` + `model.rs:208-212` - `Scale` (i32) эмитится дословно: значимы только `-1/-2`, прочие отрицательные/нечётные ширины ломают энкод. → валидировать scale.

## C. Ресурсы и DoS

18. 🔴 `handlers/mod.rs:142-229` - `upload_handler` полностью минует `jobs_semaphore`: неограниченная запись на диск + `ffprobe` на каждый запрос (без `spawn_blocking`, без cap). → провести upload через семафор, probe в `spawn_blocking`.
19. 🟠 `tools/mod.rs:83-103, 256-290` - нет лимита CPU/RAM/threads/filesize на ffmpeg/yt-dlp; только счётный семафор → насыщение/OOM. → `-threads`, `nice`, лимиты в compose.
20. 🟠 `tools/mod.rs:295-352` - нет no-progress watchdog: зависший-но-живой процесс держит слот весь `JOB_TIMEOUT` (1800с). → watchdog по бездействию.
21. 🟠 `tools/mod.rs:73-103` - нет cap на размер/длительность импорта (`MAX_HEIGHT` ограничивает только высоту). → `--max-filesize`, лимит длительности.
22. 🟠 `handlers/mod.rs:164-198` - частичный upload остаётся на диске при обрыве по лимиту тела / ошибке multipart. → удалять файл в ветках ошибки.
23. 🟡 `handlers/mod.rs:411-422` + `state.rs:73-77` - `update_job` на каждый progress-тик берёт глобальный `jobs` Mutex, общий со всеми чтениями. → throttle прогресса / отдельный канал.

## D. Данные и целостность

24. 🔴 `main.rs:103-135` - TTL-чистка удаляет файлы по mtime без проверки ссылок → сироты projects/jobs/cache; может удалить source под идущим/очередным рендером. → удалять только нессылаемые, не трогать активные.
25. 🟠 `db.rs:40-46` + `main.rs:103-134` + `library.rs` - `render_cache` не инвалидируется при удалении выходного файла → кэш-хит на удалённый файл = 404. → `cache_delete` при промахе файла и при `library.remove`.
26. 🟠 `db.rs:18-46` - нет миграций: `schema_version` пишется, но не читается; изменение схемы сломает существующую БД. → `PRAGMA user_version` + пошаговые миграции.
27. 🟠 `state.rs:92-109` + `db.rs:206-213` - нет ретеншна jobs/render_cache (растут вечно); `recover_jobs` грузит ВСЕ строки в память при старте. → TTL-очистка + лимит загрузки.
28. 🟠 `library.rs:90-137` + `db.rs:40-46` - два параллельных хранилища (`library.json` vs SQLite) без FK/общего id, дрейфуют независимо. → унифицировать в SQLite.
29. 🟠 `handlers/mod.rs:262-368` - нет single-flight: два идентичных параллельных edit оба мимо кэша, оба рендерят, гонка на `cache_put`/`library.add`, сирота-output. → in-flight карта по `cache_key`.
30. 🟡 `handlers/mod.rs:357-364` + `427-459` - `cache_put` и `library.add` не атомарны: краш между ними → кэш без библиотеки (или наоборот). → писать кэш после успешной регистрации.
31. 🟡 `state.rs:60-65` - `set_job` персистит синхронно, но **глотает ошибку**: задача живёт в памяти без durable-записи и пропадёт после рестарта. → на критический сбой persist возвращать ошибку.
32. 🟡 `db.rs:18-46, 135-143` - нет индексов на `jobs.updated_at`/`render_cache.created_at` (есть только `idx_projects_video_id`), хотя по ним идёт сортировка/ретеншн. → добавить индексы.

## E. Безопасность (в основном ⚠выставление)

33. 🟠 `tools/net.rs:10-47` + `tools/mod.rs:103` - SSRF обходится: host резолвит yt-dlp, а `validate_url` проверяет только литеральные IP (DNS-rebinding/редиректы на `169.254.169.254`/loopback). → резолвить host и проверять все A/AAAA, запрет приватных редиректов.
34. 🔴⚠выставление `lib.rs:28-58` - нет аутентификации ни на одном эндпоинте. → middleware с токеном перед выставлением.
35. 🟠⚠выставление `lib.rs:54` - `ServeDir` отдаёт весь `storage`, включая `app.db` и `library.json`. → вынести БД из `storage`, отдавать за авторизацией.
36. 🟠⚠выставление `lib.rs:56` - `CorsLayer::permissive()` на мутирующих POST/DELETE. → ограничить origin/методы.
37. 🟡 `tools/net.rs:29-47` - список заблокированных диапазонов без CGNAT (`100.64/10`) и IPv4-mapped IPv6 (`::ffff:127.0.0.1`). → полный special-use allowlist.
38. 🟡 `handlers/mod.rs:149-229` - upload не проверяет тип/magic-байты до того, как файл доверен и отдаётся через `/files`. → проверка контейнера/magic.
39. 🟡 `handlers/projects.rs:14-44` - `project_upsert` хранит произвольный JSON-блоб дословно (нет схемы/лимита). → типизированный DTO + лимит размера.
40. 🟡 `model.rs:83-212` - нет `#[serde(deny_unknown_fields)]` ни на одном request-DTO; неизвестные поля молча игнорируются. → строгая десериализация.

## F. Ошибки и API-контракт

41. 🟠 `tools/mod.rs:131-146, 288` - сырой stderr ffmpeg/yt-dlp (с путями) уходит в `job.error` и показывается пользователю. → типизированные ошибки, безопасный текст наружу, техника в лог.
42. 🟡 `handlers/projects.rs:17,49,62,74,86` - три разные формы ошибки в одном API (`(StatusCode,String)` / plain-text / голый код). → общий `ApiError: IntoResponse`.
43. 🟡 `handlers/projects.rs:63-91` - `Err(_) => 500` глотает причину БД без тела и без лога. → единый маппинг + `tracing::error!`.
44. 🟡 `handlers/mod.rs:136, 262-265` - создание async-задачи отвечает `200`, а не `202 Accepted`. → `StatusCode::ACCEPTED`.
45. 🟡 `api.ts:24-28, 45-58` - фронт трактует любой `>=500` как «бэкенд недоступен», маскируя реальные 500 с телом; `cancelJob` (`:117-124`) игнорирует ответ. → различать сетевой сбой и HTTP 5xx.

## G. Frontend (структура/качество)

46. 🟠 `store.ts` (609 строк) + `EditPanel.vue:315-724` - god-модуль (один reactive, прямые мутации из компонентов) + god-компонент (8 доменов в одном файле). → разбить на core + features composables / секции-компоненты (см. recommendation.md P1-5/6).
47. 🟡 `store.ts:440-447, 598-609` - два `watch()` регистрируются как сайд-эффект импорта модуля (нельзя выключить/замокать; текут между тестами); они же дублируют deep-обход `state.edit`. → `initStore()`/composable, один объединённый watch.
48. 🟡 `store.ts:453-456, 514-531` - `applyPreset` делает `Object.assign` без фильтра по `PRESET_KEYS`, плюс `loadPresets`/`applySnapshot`/`getProjectByVideo`/`job.result` приводят недоверенный JSON через `as` без runtime-валидации → устаревший/чужой пресет затирает геометрию, битый payload портит редактор. → применять только `PRESET_KEYS`, валидировать форму ответов.
49. 🟡 `store.ts:336-342` + `EditPanel.vue:442-454` + `RectOverlay.vue:55-59` - проглоченные `catch {}` без лога; нет a11y (`fieldset/legend`, `role=radio`/`aria-pressed`/`aria-invalid`); `root.value!` в drag-математике. → логировать, добавить семантику/ARIA, guard вместо `!`.

## H. Тесты

50. 🟠 `tests/api.rs` + `store.test.ts` - не покрыто: happy-path рендера через `edit_handler` (оркестрация семафор/drain/library/cache), баг persist-on-URL-error, undo/redo/history, `run_with_progress`(kill), `map_ytdlp_error`/`sanitize_ext`, контракт-снэпшот JSON-ответов, async-экшены стора (нет api-мока). → добавить интеграционный (skip-если-нет-ffmpeg) + юнит-тесты.

## I. Конфиг / сборка / Docker / CI

51. 🟠 `main.rs:84` - `BIND_ADDR` при ошибке парсинга тихо откатывается в `127.0.0.1` (контейнер недостижим, в логе ничего). → строгий парсинг, `bail!`.
52. 🟠 `main.rs:51-83` - все числовые env fail-soft (`.parse().ok().unwrap_or`): `MAX_CONCURRENT_JOBS=0`/опечатка молча → дефолт. → ошибка старта на невалидном значении; единый `Config` (env россыпью в ~7 местах).
53. 🟠 `Dockerfile:9-13` - `yt-dlp` без пиннинга (`releases/latest`) + ffmpeg из apt без версии → невоспроизводимо, парсинг прогресса может сломаться. → пиннинг + sha256.
54. 🟡⚠выставление `Dockerfile:8-20` - бэкенд от root, нет `USER`/`HEALTHCHECK`/лимитов (`compose`). → непривилегированный user, healthcheck, лимиты.
55. 🟡 `ci.yml:17-31` - CI ставит ffmpeg, не ставит yt-dlp → import-путь не покрыт в CI; README «Node 18+» vs CI/Docker Node 20; README не упоминает `RUST_LOG`; нет MSRV. → добавить yt-dlp, выровнять доки/MSRV.

## J. Доменная модель

56. 🟠 `model.rs:95-181` + `types.ts:22-60` + `store.ts` - контракт правок продублирован в ~6 несинхронизированных местах без компиляторной проверки (тихий дроп поля); `EditRequest` тащит тройную роль (wire DTO + домен + вход билдера); нет валидации enum формата/кодека (неизвестное молча → mp4/H.264). → один источник истины (генерация TS из Rust), типобезопасный маппер.

## K. Дополнительный архитектурный долг и риски масштабирования

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

---

## Проверено и опровергнуто (14 ложных срабатываний)

Верификация отсекла находки, которые при чтении кода не подтвердились - полезно,
чтобы не тратить время:

- **Инъекция команд** (`tools/mod.rs:83-103`) - SAFE: `Command` без shell, всё через argv; `sanitize_color`→`black`, `filter_preset` - фикс-таблица, числа форматируются.
- **Path-traversal через `video_id`** (`tools/mod.rs:163-186`) - SAFE: скан директории по равенству stem, без `Path::join` от ввода.
- **`library.json` переписывается целиком** - НЕ баг: запись атомарна (tmp+rename), осознанный trade-off для маленькой однопользовательской библиотеки.
- **Пустой ключ `render_cache` при ошибке сериализации** - НЕ баг: `serde_json` пишет `null` для не-finite f64 (не ошибка), а NaN вообще не десериализуется из тела.
- **Ключ кэша игнорирует env/расширение** - НЕ баг: формат входит в сериализованный `EditRequest`, расширение определяется форматом, на хите есть проверка существования файла.
- **`set_job` создаёт окно 404 после ответа** - НЕ баг: и persist, и in-memory вставка завершаются до ответа; остаётся лишь мелочь - проглоченная ошибка persist (см. №31).
- **`VideoPreview` `playerTime` фанит через два deep-watch** - НЕ баг: `playerTime` - top-level поле, вне `state.edit`/`state.video`, watch не срабатывает.
- **Гонка автосейва с переоткрытием клипа** - НЕ баг: `persistProject` читает текущий `state.video.id`+`state.edit`, watch перепланирует таймер при смене клипа, `restoreProject` гейтит по id.
- **Quality-tier молча теряется для prores/gif/png/jpg/mp3** - НЕ баг: чипы качества не показываются для этих форматов (`showQuality`), CRF для них бессмыслен.
- **Quality-tier av1 mismatch** - НЕ баг: то же, гейтится `showQuality`; `tierToCrf` намеренно `null` вне списка.
- **`poll_terminal` флейки на current-thread runtime** - НЕ баг: кооперативное планирование Tokio, тестируемые пути не блокируют.
- **Тест `recover_jobs running→interrupted`** - НЕ баг: транзишн реально проверяется через `recover_jobs` + HTTP-роут (формулировка находки неточна).
- **Concat дублирует всю сборку фильтров/кодеков** - переоценено: тяжёлая логика уже вынесена в общие `video_filters`/`audio_filters`/`push_video_codec`; остаётся тривиальный дубль.
- **Безлимитный progress-канал = DoS** - переоценено: ffmpeg шлёт прогресс ~1/с, drain без `await` под локом; в худшем случае - стилистический нит (`watch::channel` был бы идиоматичнее).

---

Порядок устранения - в [recommendation.md](../recommendation.md) (P0 → P1 → P2).
Полный список подтверждённых (118) и их `file:line` - в результатах прогона;
здесь сведён топ-200 по влиянию: 56 verified-first + 144 архитектурных долга.
