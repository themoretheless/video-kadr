# Архитектура

Документ описывает: (1) **текущую** структуру, (2) **целевой модульный дизайн**
«как если бы строили с нуля» с упором на слабую зацепленность, (3) **правила
зависимостей**. Синтез 10-критического ревью; пошаговый путь миграции -
в [docs/refactor-plan.md](docs/refactor-plan.md). Текущий исполняемый набор -
665 рекомендаций: 565 SOLID/DRY-находок плюс 100 решений из исследования
[100 сильных репозиториев и первичных источников](docs/research-100.md).

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
- `error.rs` - `AppError`/`AppResult`, JSON error envelope и адаптеры extractors.
- `main.rs` - bootstrap: env, probe инструментов, открыть БД, recover задач, `serve`.
- `handlers/` - HTTP. `mod.rs` (import/edit/jobs + общая job-машинерия:
  `finish_job`/`spawn_progress_drain`/`job_timeout`), `upload.rs`, `projects.rs`,
  `library.rs`, `health.rs` (вынесены как независимые группы).
- `tools/` - внешние инструменты. `args.rs` (чистая сборка ffmpeg-аргументов +
  тесты), `net.rs` (единая URL/host/IP/port-policy), `egress_proxy.rs`
  (контролируемый transport с DNS pinning на каждый request), `mod.rs`
  (process/download/probe orchestration и реэкспорт публичного API).
- `state.rs` - `AppState`: горячий jobs-registry + cancels + инкапсулированные
  независимые job/upload gates + tools + library + SQLite persistence + storage.
- `db.rs` - sqlx/SQLite: projects, jobs, render_cache.
- `library.rs` - медиатека (JSON-файл, параллельно БД).
- `model.rs` - `Job`/`JobStatus`, `EditRequest` (плоский DTO+домен), `Trim`/`Crop`/`Scale`.

**Frontend** (`frontend/src/`):
- `store.ts` - единый reactive `state` + orchestration-экшены (импорт/экспорт/
  библиотека/проекты/история/пресеты/тема/плеер); чистые edit-defaults,
  валидация и payload compiler уже вынесены в `domain/edit.ts`.
- `components/` - `EditPanel.vue` (ещё крупный контейнер), отдельный
  `edit/ExportControls.vue`, `VideoPreview`, `RectOverlay`, `TrimSlider`,
  `MediaLibrary`, `UrlImport`, `ResultPanel`, `Toasts`, `ProgressBar`.
- `api.ts` - HTTP-клиент. `types.ts` - `EditState` (зеркало `EditRequest`).

Уже выровнено по целевому дизайну: `tools/{args,net,egress_proxy,mod}`,
`handlers/{upload,projects,library,health}`, frontend `domain/edit.ts` и
`components/edit/ExportControls.vue` (см. refactor-plan).

## Top-200: что сделано плохо или неправильно

Источник правды - [docs/audit.md](docs/audit.md). Ниже та же первая
приоритизированная двухсотка, синхронизированная с `recommendation.md`.
`docs/audit.md` хранит проверенное ядро (118 находок, file:line и опровергнутые).
Расширенный широкий охват (509 заземлённых на код проблем, 28 линз) - в
[docs/audit-500.md](docs/audit-500.md); порядок исполнения по фазам - в
[plan.md](plan.md).

**Раунд 2 (1 июля 2026).** После 5 P0-фиксов (single-flight рендер-кэша,
process-group kill, атомарная отмена, CORS/ServeDir, лимит project-payload)
номера ниже помечены ✅ (закрыто и перепроверено) / ◐ (частично) там, где статус
изменился; нумерация 1-200 не переиспользуется. 18 новых находок (201-218,
включая два новых high: обход SSRF через редирект `yt-dlp` и подтверждённый
stored XSS через upload) и полный ранжированный **топ-50 актуальных проблем** -
в [docs/audit.md](docs/audit.md#топ-50-актуальных-проблем-1-июля-2026)
(исторический источник доказательств). Оба high позже закрыты: №202 в раунде 5,
№201 в раунде 6.

### Correctness и гонки задач

1. ✅ `cancel_handler` и `finish_job` гоняются: отменённая задача может стать `Done`. *(исправлено `596327b`; соседняя гонка на переходе в `Running` не закрыта - см. audit.md №203)*
2. ✅ Отмена или таймаут импорта оставляет частично скачанные файлы в `sources/`.
3. ✅ Кэш-хит edit может быть отменён в окне между cancel и записью `Done`. *(исправлено `596327b`)*
4. ✅ `ffmpeg`, порождённый через `yt-dlp`, может осиротеть при cancel/timeout. *(исправлено `174e1a7`; остаточный гэп в не-cancel/timeout ветке - audit.md №206)*
5. Graceful shutdown закрывает HTTP, но бросает in-flight workers и child processes.
6. ✅ Ожидание permit в очереди не отменяемо.
7. Ошибка `acquire_owned()` оставляет job в non-terminal статусе.
8. Progress drain продолжает писать progress в терминальную job.

### Геометрия и сегменты

9. ✅ Вырезание сегмента молча не работает для AV1/ProRes.
10. ✅ Crop не валидируется против размеров источника.
11. Overlay может округлить `x+w` за пределы ширины кадра.
12. ✅ Сегменты не сортируются, пользовательский порядок ломает timeline.
13. Концы сегментов не клампятся к duration.
14. Отрицательный start сегмента может уйти в ffmpeg.
15. Перекрывающиеся сегменты не отклоняются и дублируют кадры.
16. `fps` не ограничен сверху/снизу и может устроить CPU/memory blow-up.
17. `scale` принимает небезопасные отрицательные и нечётные размеры.

### Ресурсы и DoS

18. ✅ `upload_handler` минует concurrency gate. *(закрыто в раунде 7 отдельным upload pool; job/upload slots независимы)*
19. Для ffmpeg/yt-dlp нет CPU/RAM/threads/filesize лимитов.
20. Нет watchdog-а по отсутствию progress.
21. Нет cap на размер и длительность импорта.
22. ✅ Partial upload может остаться на диске после ошибки multipart/body limit. *(закрыто в раунде 7: единый receive boundary удаляет staging при parse/I/O/timeout)*
23. Каждый progress tick берёт глобальный jobs mutex.

### Данные и целостность

24. ◐ TTL-чистка удаляет файлы по mtime без проверки ссылок и активных jobs. *(file/cache/library-консистентность исправлена; active-job-awareness - нет, см. audit.md №24)*
25. ✅ `render_cache` не инвалидируется при удалении/пропаже output-файла.
26. SQLite schema version пишется, но миграций нет.
27. Jobs/render_cache растут без retention, `recover_jobs` грузит всё в память.
28. `library.json` и SQLite живут параллельно и могут дрейфовать.
29. ✅ Нет single-flight для одинаковых параллельных renders. *(исправлено `a550a86`; побочный эффект - неограниченный рост карты локов, audit.md №205)*
30. `cache_put` и `library.add` не атомарны.
31. Ошибки persist job глотаются.
32. ✅ Нет индексов для сортировки/retention jobs и render_cache. *(индексы есть; сам retention всё ещё не реализован, audit.md №27)*

### Security

33. ✅ SSRF-валидация не резолвит DNS, `yt-dlp` может уйти в private IP. *(закрыто полностью в раунде 6: initial guard + per-request egress-proxy, redirect/DNS rebinding test)*
34. ☐ Нет authentication на мутирующих endpoints. *(⚠выставление, задокументированный tradeoff для локального MVP, не тронуто)*
35. ✅ `ServeDir` отдаёт весь `storage`, включая потенциально чувствительные файлы. *(исправлено `986b799`)*
36. ✅ `CorsLayer::permissive()` открыт для mutating requests. *(исправлено `986b799`)*
37. Blocklist приватных диапазонов неполный.
38. ✅ Upload доверяет расширению/контейнеру до проверки magic bytes. *(закрыто в раунде 5: `ffprobe format_name` + server allow-list до publish)*
39. ◐ Projects API хранит произвольный JSON без схемы и лимита. *(лимит на `video`/`edit` исправлен `3b655c8`; поле `name` осталось без лимита, audit.md №207)*
40. Request DTO не используют `deny_unknown_fields`.

### API, frontend и тесты

41. Сырой stderr ffmpeg/yt-dlp может попасть в `job.error`.
42. Project endpoints возвращают разные формы ошибок.
43. DB errors местами превращаются в голый 500 без тела и лога.
44. Async job creation отвечает `200`, а не `202 Accepted`.
45. Frontend маскирует реальные HTTP 500 как «backend down». *(латентно, не активно: `import`/`edit`-хендлеры сейчас не возвращают 5xx физически, см. audit.md №45)*
46. Frontend `store.ts` и `EditPanel.vue` остаются god-module/god-component.
47. Store watchers регистрируются как side effect импорта модуля.
48. Presets/project/job JSON приводятся через `as` без runtime validation. *(переформулировано 1 июля 2026: `job.result` - это typed-ответ собственного бэкенда, не «чужой JSON» - опровергнуто; реальная сегодняшняя проблема того же семейства - `PRESET_KEYS` слишком широкий, audit.md №209)*
49. Есть проглоченные `catch {}`, a11y debt и unsafe `root.value!`.
50. Тесты не покрывают orchestration render path, async store actions и API
    contract snapshots.

### Конфиг, сборка и эксплуатация

51. `BIND_ADDR` при ошибке парсинга тихо откатывается на localhost.
52. Числовые env-переменные fail-soft: опечатка молча превращается в дефолт.
53. Env читается в нескольких местах вместо одного `Config`.
54. Нет тестируемой `Config`-структуры с явными defaults и validation.
55. Docker тянет `yt-dlp`/ffmpeg без строгого pinning и checksum.
56. Backend container не имеет non-root user, healthcheck и resource limits.
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

### API и контракт

71. Нет OpenAPI/schema source of truth в текущем `main`.
72. TypeScript DTO поддерживаются вручную и не генерируются из backend contract.
73. `ProjectDto` объявлен в `api.ts`, а не в едином `types.ts`/generated module.
74. Response DTO не типизированы на backend: много ручного `json!`.
75. Handlers вручную собирают JSON вместо typed response structs.
76. ◐ Error status/body нормализованы через `AppError`; async jobs всё ещё отвечают 200 вместо 202.
77. `cancelJob` на frontend игнорирует HTTP статус и тело ответа.
78. ✅ `safeFetch` смешивал network failure и backend 5xx. *(закрыто typed `ApiError` в раунде 8)*
79. Frontend requests не имеют `AbortController`/timeout на уровне API client.
80. Polling interval фиксирован, без backoff/jitter.
81. Polling не имеет max attempts/deadline на зависшие jobs.
82. Нет idempotency keys для import/edit/upload.
83. Upload sync-flow отличается от job-based import/edit.
84. Нет `GET /api/jobs` для списка jobs и диагностики очереди.
85. Нет request-id/correlation-id в API responses и logs.
86. ✅ Error body shape унифицирован как `{error, code}` в раунде 8.
87. ✅ Plain-text API errors в projects/upload/extractors устранены; frontend хранит text fallback только для старого proxy.
88. Library/projects endpoints без pagination/filter/search contract.
89. API не версионирован (`/api/v1` или schema version отсутствуют).
90. README API блок отстаёт от фактических ошибок, cache и jobs details.

### Доменная модель edit/render

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

### Backend модульность и тестируемость

121. `backend/src/tools/args.rs` остаётся слишком крупным compile god-file.
122. `backend/src/handlers/mod.rs` всё ещё хранит общую job-машинерию.
123. `backend/src/db.rs` смешивает schema, SQL, DTO mapping и tests.
124. `backend/src/library.rs` остаётся JSON repository рядом с SQLite repository.
125. `lib.rs` строит router с конкретными implementations вместо ports.
126. Нет traits для `ProjectRepo`, `JobRepo`, `MediaRepo`, `RenderCache`.
127. Нет in-memory repositories для быстрых service tests.
128. ✅ `AppError`/`IntoResponse` введён как единый error boundary в раунде 8.
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

### Frontend архитектура и UX

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
152. `pollJob` нельзя отменить при unmount/смене клипа. *(механизм подтверждён, но не воспроизводится - store синглтон, панели не размонтируются во время job, debt не активный баг)*
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

### Security и privacy beyond local MVP

171. Нет threat model для локального vs exposed deployment.
172. Нет authentication middleware и token story.
173. Нет CSRF posture для future cookie/session mode.
174. Нет rate limiting на import/upload/edit.
175. Нет per-user isolation для проектов, jobs и files.
176. Нет sandbox profile для ffmpeg/yt-dlp.
177. Нет allowlist/denylist policy для supported URL domains.
178. ✅ Нет redirect policy для yt-dlp после URL validation. *(закрыто в раунде 6: каждый redirect открывает новый проверяемый proxy target)*
179. Нет centralized redaction для logs/errors/API.
180. Query tokens в импортируемых URL не редактируются как единая policy.
181. Нет diagnostics bundle с гарантированной redaction.
182. ✅ Upload не проверяет magic bytes до публикации файла через `/files`. *(закрыто в раунде 5: staging + `ffprobe` + server-selected extension)*
183. Нет malware/quarantine story для uploaded media.
184. File serving не ставит explicit safe `Content-Disposition`.
185. Нет audit log для mutating operations.

### Storage, data lifecycle и operations

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

### Раунд 2 (1 июля 2026): новые находки 201-218

Полные описания, `file:line` и adversarial-верификация - в
[docs/audit.md](docs/audit.md#раунд-2-новые-находки-201-218-1-июля-2026).

201. ✅ Редирект `yt-dlp` на приватный адрес после успешной `validate_url` обходит SSRF-защиту импорта (новый high). **Закрыто в раунде 6:** loopback egress-proxy валидирует каждый HTTP/CONNECT target и соединяется с pinned `SocketAddr`; реальный `yt-dlp`-тест подтверждает отсутствие connect к private sink.
202. 🟠 Upload принимает полиглот-файл и отдаёт его как `text/html` - подтверждённый PoC stored XSS (новый high). **Закрыто в раунде 5:** расширение теперь выводится из `ffprobe format_name`, ответы получают `nosniff` + sandbox CSP.
203. Отмена между `is_cancelled()` и записью `Running` в import/edit-воркерах по-прежнему перетирается - `596327b` не закрыл этот путь. **Закрыто в раунде 5:** `queued`/`running`/validation-error переходы используют `update_job_if_open`.
204. `upsert_project` не атомарен - конкурентный автосейв из двух вкладок даёт дубликаты project-строк.
205. `render_locks` в `AppState` растёт неограниченно, записи никогда не удаляются (побочный эффект `a550a86`).
206. В `174e1a7` путь «оба потока дали EOF раньше child» шлёт только SIGTERM без эскалации до SIGKILL.
207. Лимит `3b655c8` покрывает `video`/`edit`, но не `name` - можно записать ~2МБ в `projects.name`.
208. `fade_in`/`fade_out` клэмпятся к длительности исходника, а не результата - фейд может молча выпасть из графа фильтров.
209. `PRESET_KEYS` включает `format`/`codec`/`qualityTier` наравне с цветом - цветовой пресет тихо меняет экспорт (переформулировка №48). **Закрыто в раунде 5**, включая фильтрацию старых пресетов из `localStorage`.
210. `RectOverlay` не снимает `window` pointermove/pointerup при размонтировании во время drag → `root.value!` бросает `TypeError`. **Закрыто в раунде 5:** общий `stopDrag` вызывается из `onUnmounted`, доступ к root guarded.
211. Провал задачи (`finish_job` `Err`-ветка) не пишет `tracing::error!` - нет диагностики в логах на упавший job. **Закрыто в раунде 5.**
212. CI не ставит `yt-dlp` - путь импорта не выполняется в CI ни разу.
213. `cache_put` может тихо не сработать после успешного рендера A → воркер B перерендеривает вместо переиспользования готового файла.
214. Геометрический клэмп crop/censor может дать нулевую/нечётную ширину на источнике 1px.
215. `list_projects`/`load_jobs`/`load_recent_jobs` роняют весь список 500-й, если у одной строки испорчен JSON.
216. `TrimSlider` - тот же listener-leak паттерн, что №210. **Закрыто в раунде 5.**
217. `normalizeRect` маскирует `NaN` как `0`/минимум вместо явной валидации.
218. Нет `rust-toolchain.toml`/MSRV, сборка зависит от плавающего `stable`.

## Модульная карта: SOLID/DRY-декомпозиция (2 июля 2026)

Раунд 3 аудита: тот же код прочитан заново отдельно по 12 модулям, 3 итерации на
модуль (широкий проход -> углубление -> дедуп/добивка), каждый модуль явно ищет
SOLID/DRY-нарушения в своей зоне. Итог - 565 находок (`bug`/`problem` = дефект,
`improvement` = качество существующего кода, `suggestion` = новая возможность,
`design` = визуальное/UX-качество), пронумерованных 219+ (нумерация 1-218 из
прошлых раундов не переиспользуется). Лёгкий cross-module дедуп убрал 5 явных
дублей; в остальном разные модули иногда описывают один и тот же корень с разных
сторон (например «X читает env на каждый вызов» встречается у нескольких вызывающих) -
это оставлено осознанно, не токенный шум.

**Как разбирать по кусочкам:** бери один модуль (не весь список) и работай только
в его файлах - модули почти не пересекаются по файлам, так что явных мердж-конфликтов
между модулями не будет. Actionable-чеклист по каждому модулю (с чекбоксами) -
в [recommendation.md](recommendation.md#soliddry-модули-по-кусочкам-2-июля-2026).

| Модуль | Находок | Что это |
|---|---|---|
| [HTTP-хендлеры и роутинг](#module-http) | 51 | Хендлеры делают больше одной вещи; import/edit копируют один и тот же job-lifecycle. |
| [Jobs, конкурентность, process lifecycle](#module-jobs) | 46 | Гонки, семафоры, отмена, spawn/kill/timeout; AppState как god-object. |
| [FFmpeg domain compiler (args.rs)](#module-ffmpeg) | 53 | Чистое ядро сборки ffmpeg-графа: OCP при новых эффектах, DRY между путями. |
| [Persistence layer (DIP)](#module-persistence) | 41 | Два хранилища без общего трейта-порта; миграции, транзакции, retention. |
| [Security и сеть](#module-security) | 45 | CORS/ServeDir, upload, controlled egress, auth и Docker hardening. |
| [API contract и обработка ошибок](#module-api-contract) | 45 | Типизация wire-контракта, единый error-boundary, версионирование. |
| [Frontend state и store](#module-frontend-store) | 50 | God-module store.ts: SRP по доменам, DRY между async-экшенами. |
| [Frontend компоненты и интерактивность](#module-frontend-components) | 45 | God-component EditPanel.vue; дублирование drag-логики между компонентами. |
| [Визуальный дизайн и UX (взгляд дизайнера)](#module-design) | 55 | Иерархия, spacing/type scale, цвет, состояния контролов, responsive. |
| [Тесты и quality gates](#module-testing) | 40 | Непокрытые пути и тесты с ложным чувством защищённости. |
| [Config, build, Docker, CI, observability](#module-ops) | 46 | Env разбросан по местам; логи/метрики; Docker hardening; toolchain drift. |
| [Доменная модель и архитектура целиком (cross-cutting SOLID и DRY)](#module-domain) | 48 | Тройная роль EditRequest; контракт правки в 6 местах; нет Timeline-IR. |

### Как проходить список маленькими кусками

1. Выбери один модуль из таблицы, не весь документ.
2. Внутри модуля сначала закрывай 🔴, потом 🟠, потом 🟡.
3. Если пункт требует много файлов, сначала сделай только safe-extract
   (перемещение/переименование без смены поведения), затем отдельным коммитом -
   изменение поведения.
4. После каждого маленького куска - `make check`.

### Три итерации раунда 3

- **Итерация 1: широкий проход.** Для каждого из 12 модулей собраны все заметные
  нарушения SRP/OCP/DIP/DRY, UI/UX-долг, тестовые дыры и эксплуатационные риски.
- **Итерация 2: углубление.** High/medium пункты перечитаны по файлам и привязаны
  к конкретным зонам кода, чтобы рекомендация была исполнимой, а не общей.
- **Итерация 3: ревью и дедуп.** Явные повторы сняты, спорные низкоуверенные
  пункты понижены, а визуальный дизайн выделен отдельным модулем с дизайнерской
  линзой: иерархия, плотность, типографика, состояния, mobile/responsive.

### Сверка раунда 4 (9 июля 2026)

Цель этой сверки - не создать второй расходящийся список, а закрепить один
понятный маршрут работы. Канонические источники сейчас такие:
`docs/audit-500.md` - широкий top-500+ аудит на 509 пунктов; этот документ -
565 SOLID/DRY-пунктов, разложенных по модулям; `recommendation.md` - тот же
набор как исполнимый чеклист.

- **Итерация 1: синхронизация.** Пересчитаны источники: 509 широких находок и
  565 модульных пунктов совпадают между архитектурой и чеклистом. Старые top-200
  и top-50 оставлены как исторический приоритетный слой, а не как отдельный
  конкурирующий backlog.
- **Итерация 2: приоритизация.** Маршрут начинался с security/correctness high
  (upload XSS, SSRF redirect, cancel-to-running race); раунды 5-6 закрыли все
  три. Текущий порядок: upload concurrency, `AppError`, `Config`, `JobRunner`,
  затем frontend-декомпозиция (`store.ts`, `EditPanel`, общие форматтеры).
- **Итерация 3: review и дизайн.** Документы проверены на drift между
  `README.md`, `architecture.md` и `recommendation.md`; UI/UX-слой трактуется
  как инженерный долг, а не как косметика. Для каждого визуального PR сначала
  фиксируются состояния, фокус, пустые/ошибочные экраны и responsive, потом
  цвет, плотность и polish.

Если бы проект начинался с нуля, базовая форма была бы такой: маленькие HTTP
адаптеры, сервис `JobRunner`, чистый `Timeline`/`EditPlan` домен, порты
`MediaRepository`/`JobRepository`/`Renderer`, typed API contract, frontend-store
по доменам `media/project/timeline/export/ui`, а дизайн-система с токенами,
состояниями и accessibility-checklist появилась бы до роста компонентов.

### Раунд 5: три итерации исполнения (11 июля 2026)

Полные списки не пересоздавались: `docs/audit-500.md` по-прежнему содержит 509
широких пунктов, а модульная карта ниже - 565. Этот раунд взял только один
вертикальный срез, чтобы diff оставался читаемым и проверяемым.

- **Итерация 1, backend security/correctness.** `upload_handler` вынесен из
  `handlers/mod.rs` в собственный модуль; имя клиента стало display-only,
  расширение определяется по `ffprobe format_name`, статика получает
  `X-Content-Type-Options: nosniff` и sandbox CSP. Переходы в `queued` и
  `running` больше не перетирают терминальную отмену. Закрыты 202/203/445.
- **Итерация 2, frontend SOLID/DRY + дизайн.** Чистые defaults, time parsing,
  quality mapping, rectangle sanitization и payload compiler вынесены в
  `domain/edit.ts`; экспорт отделён в `components/edit/ExportControls.vue`.
  Добавлены accessible segmented states, предупреждение перед no-op export,
  спокойная геометрия контролов и отсутствие horizontal overflow на 390 px.
  Закрыты 209/517; 500/550 выполнены частично одним безопасным срезом.
- **Итерация 3, adversarial review.** Исправлена очистка оборванного `.upload`,
  атомарность стадии `queued` и validation-error, логирование job/cache errors,
  cleanup window-listeners в `RectOverlay`/`TrimSlider`; добавлены regression-
  тесты и живой desktop/mobile-прогон. Закрыты 210/211/216/553/554.

### Раунд 6: SSRF transport и adversarial review (11 июля 2026)

- **Итерация 1, policy.** `tools/net.rs` стал единым источником правил host/IP/
  port: только HTTP(S) 80/443, mixed DNS answer отклоняется целиком, известные
  private/special IPv4/IPv6 cases покрыты одной policy. Форматный селектор
  допускает только `http://`/`https://` media URL, не позволяя `yt-dlp` выбрать
  прямой RTMP/FTP/WebSocket downloader вне proxy.
- **Итерация 2, enforced transport.** Новый per-job `tools/egress_proxy.rs`
  обслуживает HTTP и HTTPS CONNECT, повторно резолвит каждый target и соединяется
  непосредственно с проверенным `SocketAddr`. `yt-dlp` получает явный proxy в
  CLI/env, `--ignore-config`, а `NO_PROXY` удаляется и для дочерних downloader'ов.
- **Итерация 3, adversarial/perf review.** Policy-deny получает транспортный
  код 472 и отдельный per-job atomic marker, поэтому настоящий upstream 403/472
  не маскируется под SSRF; DNS и connect bounded, активные proxy-клиенты
  ограничены 64 на job, shutdown прерывает оставшиеся tasks. Добавлены реальный
  302→private-sink тест через `yt-dlp`, resolver-тест public→private и synthetic
  HTTPS+RTMP/RTMP-only format tests. Они доказывают отсутствие private TCP
  connect и отказ non-HTTP media до downloader. CI ставит закреплённый `yt-dlp`.

### Раунд 7: upload resource isolation (11 июля 2026)

- `jobs_semaphore` скрыт за `acquire_job_slot`/`close_job_queue`; upload использует
  отдельный fail-fast pool, поэтому медленный HTTP client не блокирует очередь
  render/download. Saturation возвращает пользовательский `429` без скрытого
  ожидания соединения.
- Multipart receive вынесен в отдельную pre-publish стадию с 30-минутным timeout;
  любой parse/I/O/timeout path удаляет `.upload`. `ffprobe` ограничен 30 секундами,
  startup `--version` checks - 5 секундами; timeout явно делает kill+wait, а
  `kill_on_drop` остаётся fallback.
- State/API/tool tests проверяют независимость pools, saturation/recovery,
  timeout latency и cleanup staging-файла. Закрыты 18/22/223/225/278/288/444;
  строгий multipart contract и ранняя content-validation остаются открытыми.

### Раунд 8: единый API error boundary (11 июля 2026)

- Новый `error.rs` владеет `AppError`/`AppResult`, HTTP status, стабильным
  machine code и JSON envelope `{error, code}`. Internal source остаётся в
  структурированном логе, клиент получает безопасное общее сообщение.
- Upload, projects, jobs и library переведены на boundary; `ApiJson`/
  `ApiMultipart`, неизвестный route и method-not-allowed также возвращают JSON.
  `projects.rs` разделяет parse/name resolution и persistence.
- Frontend больше не угадывает ошибку по status: единый parser создаёт typed
  `ApiError(status, code)`, а backend-down относится только к network exception.
  Закрыты 455/457/460/469/472/488; `job.error` и cancel response остаются отдельно.

Следующий независимый срез по приоритету: единый `Config`, затем общий
`JobRunner`. Frontend продолжать секциями: `AudioControls`,
`PresetBar`, history/presets/theme stores; фасад `store.ts` сохранять до конца
миграции.

<a id="module-http"></a>
### Модуль: HTTP-хендлеры и роутинг (51)

HTTP-хендлеры и роутинг (`backend/src/handlers/`, `lib.rs`). SRP/DRY на границе транспорта: где хендлер делает больше одной вещи, где два хендлера копируют один и тот же жизненный цикл job.

219. 🔴 [проблема/SRP] import_handler смешивает валидацию URL, оркестрацию очереди, скачивание, probing и persistence в одной async-функции - `backend/src/handlers/mod.rs` -> Вынести оркестрацию job (create/permit/progress/finish) в общий раннер, а скачивание+probing в отдельную доменную функцию, которую import_handler только вызывает.
220. 🔴 [проблема/DRY] import_handler и edit_handler дублируют весь жизненный цикл job целиком - `backend/src/handlers/mod.rs` -> Выделить общую функцию run_job(state, job_id, token, work: impl Future) -> Json<Value>, инкапсулирующую permit/progress/finish, и передавать в неё только специфичную работу.
221. 🟠 [проблема/SRP] edit_handler дополнительно совмещает cache-логику рендера с оркестрацией job - `backend/src/handlers/mod.rs` -> Вынести проверку и запись в render cache в отдельный RenderCache-хелпер с методами try_serve/store, вызываемый из edit_handler декларативно.
222. 🟡 [идея/DRY] upload_handler вручную повторяет сборку VideoInfo-JSON, уже собираемую в import_handler - `backend/src/handlers/mod.rs` -> Ввести конструктор VideoInfo::new(id,url,filename,probe,title,size) -> Value и использовать его в обоих хендлерах.
223. ✅ [проблема] upload_handler не проходил через concurrency gate - **закрыто в раунде 7:** отдельный upload pool ограничивает multipart/probe, не занимая job slots; saturation даёт `429`. `backend/src/handlers/upload.rs`, `backend/src/state.rs`
224. 🟡 [дизайн] upload_handler возвращает результат по первому подходящему полю формы и молча игнорирует остальные части multipart - `backend/src/handlers/mod.rs` -> Явно проверять, что multipart содержит ровно одно валидное поле файла, и возвращать 400 при лишних/нераспознанных полях.
225. ✅ [баг] upload_handler не ограничивал время probe_video - **закрыто в раунде 7:** общий `probe_video` имеет 30-секундный timeout с kill+wait; upload удаляет staging и возвращает отдельный `504`. `backend/src/tools/mod.rs`, `backend/src/handlers/upload.rs`
226. 🟠 [проблема/SRP] normalize_edit_request и валидаторы диапазонов — доменная логика в HTTP-слое - `backend/src/handlers/mod.rs` -> Перенести эти функции в model.rs или отдельный модуль domain/edit_validation.rs, откуда их будет импортировать edit_handler.
227. 🟡 [улучшение/SRP] Файловые и криптографические утилиты живут в handlers/mod.rs вперемешку с HTTP-хендлерами - `backend/src/handlers/mod.rs` -> Вынести их в backend/src/tools/fs_utils.rs или backend/src/tools/cache.rs, оставив в handlers только вызовы.
228. 🟠 [проблема] import_handler и edit_handler игнорируют JoinHandle из tokio::spawn — паника воркера не отражается в job - `backend/src/handlers/mod.rs` -> Сохранять JoinHandle и в отдельной задаче через .await с обработкой Err(JoinError) переводить job в JobStatus::Error с сообщением о панике.
229. 🟡 [проблема] job_timeout() читает переменную окружения JOB_TIMEOUT_SECS при каждом вызове вместо однократного чтения - `backend/src/handlers/mod.rs` -> Прочитать JOB_TIMEOUT_SECS один раз в main/build_router через std::sync::OnceLock и переиспользовать закэшированное значение.
230. 🟡 [дизайн] Прогресс сохраняется только при шаге >=1%, без гарантии сохранения финального значения перед 100 - `backend/src/handlers/mod.rs` -> Если это осознанный компромисс — оставить как есть; иначе сохранять хотя бы последнее полученное значение перед закрытием канала независимо от порога.
231. 🟡 [проблема/DIP] edit_handler мутирует входящий EditRequest прямо в воркере вместо получения провалидированного типа - `backend/src/handlers/mod.rs` -> Изменить normalize_edit_request на fn(EditRequest, ...) -> anyhow::Result<NormalizedEditRequest>, чтобы невалидированный EditRequest не мог случайно использоваться дальше по ошибке.
232. 🟡 [баг] cleanup_files_with_prefix сопоставляет файлы по префиксу имени, что может задеть чужой файл при совпадении подстроки - `backend/src/handlers/mod.rs` -> Сравнивать Path::file_stem() файла целиком с prefix вместо strip_prefix, чтобы исключить любую теоретическую двусмысленность.
233. 🟡 [проблема/SRP] project_upsert_handler вручную парсит сырой serde_json::Value вместо типизированного тела запроса - `backend/src/handlers/projects.rs` -> Ввести #[derive(Deserialize)] struct ProjectUpsertRequest { video_id: String, video: Value, edit: Value, name: Option<String> } и десериализовать через Json<ProjectUpsertRequest>.
234. 🟡 [проблема/DRY] ensure_project_json_size сериализует video/edit отдельно от последующей записи в БД, дублируя сериализацию - `backend/src/handlers/projects.rs` -> Сериализовать video/edit один раз в project_upsert_handler, проверить длину байт и передать уже сериализованные строки в upsert_project, чтобы БД не сериализовала повторно.
235. 🟡 [улучшение] project_upsert_handler не проверяет, что video["id"] совпадает с videoId тела запроса - `backend/src/handlers/projects.rs` -> После разбора video проверить video["id"].as_str() == Some(video_id) и вернуть 400 при расхождении.
236. 🟡 [дизайн] Ошибки в projects.rs смешивают русский текст с сырыми деталями SQLx в INTERNAL_SERVER_ERROR - `backend/src/handlers/projects.rs` -> Логировать e через tracing::error! на сервере и возвращать клиенту фиксированное русскоязычное сообщение 'внутренняя ошибка' без деталей SQLx.
237. 🟡 [проблема] project_get_handler и project_by_video_handler возвращают голый StatusCode без текста ошибки, в отличие от upsert/list - `backend/src/handlers/projects.rs` -> Привести все четыре project-хендлера к единому Result<_, (StatusCode, String)> с логированием причины на сервере.
238. 🟡 [проблема/DRY] Паттерн 'Ok(Some)/Ok(None)/Err -> StatusCode' повторяется идентично в нескольких хендлерах - `backend/src/handlers/projects.rs` -> Добавить extension-trait для Result<Option<T>, sqlx::Error> с методом .into_response_or_404(), переиспользуемый во всех подобных хендлерах.
239. 🟠 [баг] library_delete_handler читает entry и удаляет его двумя раздельными операциями без атомарности - `backend/src/handlers/library.rs` -> Изменить Library::remove, чтобы он возвращал Option<MediaEntry> удалённой записи одним атомарным вызовом, и убрать отдельный get.
240. 🟡 [проблема] library_delete_handler чистит db cache только для kind == 'output', не проверяя cache-записи, ссылающиеся на source-файлы - `backend/src/handlers/library.rs` -> При удалении source дополнительно проверять и очищать cache-записи, у которых закешированный результат ссылается на этот video_id, либо явно задокументировать, что это не требуется, так как cache хранит только output.
241. 🟡 [проблема] health_handler не проверяет живость хранилища/БД, только статически захваченные при старте флаги ffmpeg/ytdlp - `backend/src/handlers/health.rs` -> Добавить лёгкую проверку доступности storage-директорий и SQLite-пула (например, SELECT 1) прямо в health_handler с коротким таймаутом.
242. 🟡 [улучшение/OCP] health_handler жёстко завязан на ровно два внешних инструмента - `backend/src/handlers/health.rs` -> Хранить инструменты как Vec<(name, ToolStatus)> в ToolInfo и сериализовать их в цикле, чтобы health_handler не менялся при добавлении нового инструмента.
243. 🟡 [проблема/ISP] health_handler не различает, какой из статусов 'ok'/'degraded' к чему относится при отсутствии обоих инструментов - `backend/src/handlers/health.rs` -> Добавить массив missingTools в ответ, перечисляющий конкретные недоступные инструменты, чтобы не заставлять клиента сверять два булевых поля.
244. 🟠 [проблема] build_router не имеет fallback-хендлера для несуществующих маршрутов - `backend/src/lib.rs` -> Добавить .fallback(|| async { (StatusCode::NOT_FOUND, Json(json!({"error":"not found"}))) }) для единообразного JSON-ответа на неизвестные маршруты.
245. 🟠 [проблема] build_router не задаёт лимит тела запроса для JSON-эндпоинтов кроме /api/upload - `backend/src/lib.rs` -> Явно задать .layer(DefaultBodyLimit::max(N)) на уровне общего Router для всех /api/* JSON-маршрутов, чтобы лимит был виден и настраиваем в одном месте.
246. 🟡 [дизайн] CORS allow_methods не включает PUT/PATCH/OPTIONS, ограничивая будущие эндпоинты - `backend/src/lib.rs` -> Добавить Method::PUT и Method::PATCH заранее либо держать список методов в одной константе, синхронизированной с build_router.
247. 🟡 [проблема] parse_cors_origin разрешает схему http для не-localhost хостов наравне с https - `backend/src/lib.rs` -> Логировать warn (не только на invalid) при приёме http-origin с хостом, отличным от localhost/127.0.0.1, чтобы явно предупредить о небезопасной конфигурации.
248. 🟡 [улучшение/KISS] std::env::var для CORS и JOB_TIMEOUT читается при каждом старте роутера/job, а не один раз в main - `backend/src/lib.rs` -> Централизовать чтение env-конфигурации (CORS origins, JOB_TIMEOUT_SECS, RECOVER_JOBS_LIMIT) в одном Config, собираемом в main и передаваемом через AppState.
249. 🟠 [проблема/ISP] handlers/mod.rs объединяет публичные HTTP-хендлеры и приватные доменные хелперы в одном файле на 964 строки - `backend/src/handlers/mod.rs` -> Разбить mod.rs на handlers/jobs.rs (import/edit/upload/status/cancel), handlers/render_cache.rs и validation/edit.rs по аналогии с уже выделенными health.rs/library.rs/projects.rs.
250. 🟠 [проблема] import_handler передаёт req.start/req.end без нормализации, в отличие от edit_handler - `backend/src/handlers/mod.rs` -> Добавить в import_handler проверку start < end и start/end >= 0 перед вызовом download_video, отклоняя запрос с понятной ошибкой вместо передачи в yt-dlp как есть.
251. 🟡 [идея] Нет эндпоинта для получения списка активных/очереди jobs - `backend/src/handlers/mod.rs` -> Добавить GET /api/jobs, возвращающий список нетерминальных jobs из AppState, чтобы фронтенд мог восстановить состояние после перезагрузки.
252. 🟡 [проблема/DRY] acquire_render_lock_or_cancelled и acquire_job_permit_or_cancelled дублируют одинаковую tokio::select!-обёртку 'ресурс или cancel' - `backend/src/handlers/mod.rs` -> Обобщить через дженерик acquire_or_cancelled<F: Future<Output=T>>(fut: F, st, jid, token) -> Option<T>, вызываемый с разными future из render_lock.lock_owned() и semaphore.acquire_owned().
253. 🟡 [улучшение] finish_job принимает kind: &str как непроверяемый компилятором строковый тег - `backend/src/handlers/mod.rs` -> Заменить kind: &str на enum MediaKind { Source, Output } с реализацией as_str(), используемый и в finish_job, и в MediaEntry::from_result.
254. 🟠 [проблема] edit_handler не проверяет существование video_id в library до создания job и занятия permit - `backend/src/handlers/mod.rs` -> Проверить наличие source-файла синхронно в HTTP-хендлере до создания job и возвращать 404, если video_id неизвестен.
255. 🟠 [баг] Ключ кэша рендера считается до нормализации EditRequest, из-за чего эквивалентные запросы не совпадают - `backend/src/handlers/mod.rs` -> Переместить нормализацию перед вычислением render_cache_key, синхронно выполнив probe+normalize в HTTP-хендлере до spawn, либо считать ключ по уже нормализованному EditRequest внутри воркера.
256. 🟠 [баг] Попадание в render cache не регистрирует результат в медиатеке - `backend/src/handlers/mod.rs` -> Вызвать state.library.add(MediaEntry::from_result("output", &output)) внутри finish_from_render_cache при успешном попадании в кэш.
257. 🟡 [проблема] import_handler не проверяет заранее пустой url и порядок start/end до запуска job - `backend/src/handlers/mod.rs` -> Выполнить лёгкую синхронную проверку url (не пустой, парсится как URL) в самом import_handler до set_job/register_cancel и вернуть 400 без создания job.
258. 🟡 [проблема/DRY] upload_handler и import_handler по-разному вычисляют title файла - `backend/src/handlers/mod.rs` -> Свести оба пути к общей функции resolve_title(source: TitleSource) с явными вариантами Uploaded{original_name} и Downloaded{sources,id}.
259. 🟡 [баг] upload_handler не ограничивает длину title, беря его напрямую из оригинального имени файла - `backend/src/handlers/mod.rs` -> Обрезать title до разумной длины (например 200 символов) и/или санитизировать управляющие символы перед сохранением в library.
260. 🟡 [проблема/SRP] project_upsert_handler инлайн реализует fallback-цепочку для имени проекта вместо доменной функции - `backend/src/handlers/projects.rs` -> Вынести резолюцию имени в отдельную чистую функцию resolve_project_name(body, video) -> String, тестируемую независимо от HTTP-слоя.
261. 🟡 [проблема] project_upsert_handler не ограничивает длину поля name - `backend/src/handlers/projects.rs` -> Добавить проверку name.len() <= разумного предела (например 200 байт) и обрезать/отклонить при превышении, аналогично ensure_project_json_size для video/edit.
262. 🟡 [улучшение/DRY] acquire_render_lock_or_cancelled и mark_cancelled создают риск двойной записи отмены при отмене на этапе ожидания render_lock - `backend/src/handlers/mod.rs` -> Консолидировать перевод job в Cancelled в единственном методе AppState (например state.mark_job_cancelled), вызываемом и из cancel_handler, и из mod.rs-хелперов вместо двух параллельных реализаций.
263. 🟡 [проблема/SRP] job_status_handler не различает 404 для несуществующего job и job, уже вычищенного лимитом recover_jobs - `backend/src/handlers/mod.rs` -> Добавить отдельный код/сообщение (например {"error":"job_expired"}) для случая, когда id валиден по формату UUID, но отсутствует и в памяти, и в БД.
264. ✅ [баг] sanitize_ext не проверяет конфликт очищенного расширения с зарезервированными именами - **закрыто в раунде 5:** `sanitize_ext` удалён, итоговый suffix выбирается только `safe_upload_extension` из фиксированного allow-list; staging использует непубликуемый `.upload`. `backend/src/handlers/upload.rs`
265. 🟡 [проблема/DRY] build_router перечисляет пары REST-методов на одном пути отдельными .route() вызовами без общего паттерна регистрации ресурса - `backend/src/lib.rs` -> При росте числа ресурсов ввести небольшой builder resource(path, handlers) для типового набора GET/POST/DELETE, снижающий повторение синтаксиса регистрации.
266. 🟡 [проблема] spawn_progress_drain не имеет верхней границы по времени и переживает job, если воркер зависает до закрытия tx - `backend/src/handlers/mod.rs` -> Обернуть rx.recv() в tokio::select! с сигналом отмены/таймаутом job_timeout(), чтобы drain гарантированно завершался вместе с воркером.
267. 🟡 [проблема/SRP] upload_handler комбинирует чтение multipart-полей, запись на диск и пробинг в одном цикле без ранней валидации типа поля - `backend/src/handlers/mod.rs` -> Сначала пройтись по всем полям и выбрать/провалидировать единственное поле файла, затем отдельным шагом выполнить запись+probe, разделяя разбор входа и работу с диском.
268. 🟡 [проблема/DIP] Хендлеры projects.rs возвращают внутреннюю структуру Project из db.rs напрямую как HTTP DTO - `backend/src/handlers/projects.rs` -> Ввести отдельный ProjectResponse в handlers/projects.rs с явным From<Project>, чтобы изменения схемы БД не автоматически меняли HTTP-ответ.
269. 🟡 [баг] cors_origins_from_env использует запятую как единственный разделитель без экранирования - `backend/src/lib.rs` -> Задокументировать в комментарии, что CORS_ALLOW_ORIGINS ожидает только plain origin без query/fragment, разделённых запятой, без изменения кода при текущих ограничениях parse_cors_origin.

<a id="module-jobs"></a>
### Модуль: Jobs, конкурентность, process lifecycle (46)

Jobs, конкурентность, процессы (`state.rs`, процессная часть `tools/mod.rs`, `model.rs`). Гонки, семафоры, отмена, spawn/kill/timeout, AppState как god-object.

270. 🟠 [проблема] render_locks не имеет верхней границы и никогда не очищается — растёт на каждый уникальный edit-запрос. - `backend/src/state.rs` -> Добавить эвакуацию записи после завершения рендера (например, удалять запись, если strong_count Arc опустился до 1 под локом карты) или заменить на LRU/TTL-кэш с ограничением размера.
271. 🔴 [баг] Bare update_job после получения permit/render-lock может воскресить job, отменённый в узком окне между проверкой token.is_cancelled() и вызовом. - `backend/src/handlers/mod.rs` -> Заменить оба вызова на update_job_if_open и прекращать работу воркера, если обновление не применилось (job уже terminal).
272. 🟡 [проблема/ISP] update_job без суффикса _if_open в state.rs — небезопасный по умолчанию API, который легко перепутать с update_job_if_open. - `backend/src/state.rs` -> Оставить только update_job_if_open как основной публичный метод, а update_job сделать приватным helper'ом или переименовать в update_job_unchecked, чтобы название явно предупреждало о риске.
273. 🟠 [баг] set_job пишет в БД до вставки job в память — окно, где GET /api/jobs/:id отдаёт 404, хотя job уже есть в БД. - `backend/src/state.rs` -> Сначала вставить job в jobs-map под локом, затем персистить в БД (или делать это параллельно), чтобы память была источником истины раньше или одновременно с БД.
274. 🟠 [баг] Окно между снятием job.status=Cancelled и удалением cancel-токена в cancel_open_job, где воркер видит job уже Cancelled, но токен ещё не сигнален. - `backend/src/state.rs` -> Объединить обновление статуса job и token.cancel() в одну критическую секцию (например, держать оба лока последовательно без промежуточного await между ними) либо cancel'ить токен первым.
275. 🟡 [улучшение] recover_jobs держит единственный jobs-lock guard на весь цикл persist_job для каждого interrupted job, блокируя все остальные операции с jobs на время старта. - `backend/src/state.rs` -> Сначала собрать все job в Vec под локом, отпустить лок, затем персистить interrupted записи в БД без удержания jobs guard, и только на короткое время повторно взять лок для финальной вставки в map.
276. 🟡 [проблема/SRP] AppState совмещает пять разных ответственностей: jobs-реестр, cancel-токены, render-lock-кэш, семафор конкурентности и доступ к Db/Library/storage. - `backend/src/state.rs` -> Выделить JobsRegistry (jobs+cancels+recover) и RenderLockRegistry в отдельные структуры-поля с собственными методами, оставив AppState тонкой агрегирующей оболочкой.
277. 🟡 [проблема/DIP] AppState хранит конкретные типы Db и Library вместо трейтов, что не позволяет подменить их в тестах без реальной SQLite/FS. - `backend/src/state.rs` -> Ввести трейты JobStore/MediaLibrary с реализациями поверх Db/Library и хранить в AppState Arc<dyn Trait>, чтобы юнит-тесты могли подставлять fake-реализации.
278. ✅ [проблема/ISP] jobs_semaphore был публичным полем AppState - **закрыто в раунде 7:** оба semaphore приватны, наружу выходят только `acquire_job_slot`, `close_job_queue` и `try_acquire_upload_slot`. `backend/src/state.rs`
279. 🟡 [дизайн/KISS] update_job и update_job_if_open дублируют идентичную структуру блокировки, различаясь только одной проверкой is_terminal. - `backend/src/state.rs` -> Реализовать update_job через update_job_if_open с параметром force: bool или сделать update_job приватным алиасом, вызывающим общий internal-helper с флагом пропуска terminal-проверки.
280. 🟡 [проблема] Job::pending не хранит created_at/updated_at, поэтому recover_jobs и любая будущая очистка jobs не имеют временной метки для принятия решений. - `backend/src/model.rs` -> Добавить поле created_at: DateTime<Utc> (и опционально updated_at) в Job и заполнять его в Job::pending, используя далее для TTL-эвакуации и сортировки recover_jobs.
281. 🟠 [проблема] jobs HashMap в AppState не имеет TTL или эвакуации — завершённые задачи (Done/Error/Cancelled/Interrupted) копятся в памяти навсегда до перезапуска процесса. - `backend/src/state.rs` -> Добавить фоновую задачу или обёртку в persist_job/update_job_if_open, которая удаляет из jobs записи старше N часов после достижения terminal-статуса, оставляя историю только в SQLite.
282. 🟡 [проблема/DRY] Парсинг числовых переменных окружения (RECOVER_JOBS_LIMIT, JOB_TIMEOUT_SECS, MAX_HEIGHT) дублирует один и тот же паттерн 'parse + filter + unwrap_or' в разных модулях без общего helper'а. - `backend/src/state.rs` -> Вынести общую функцию env_usize(name, default) / env_positive(name, default) в отдельный модуль config.rs и переиспользовать её во всех трёх местах.
283. 🟡 [проблема] 3-секундный SIGTERM grace period в terminate_child_tree захардкожен и не настраивается через конфиг. - `backend/src/tools/mod.rs` -> Вынести grace-period в переменную окружения (например GRACEFUL_KILL_SECS) с разумным дефолтом 3, аналогично MAX_HEIGHT/JOB_TIMEOUT_SECS.
284. 🟡 [проблема] MAX_HEIGHT читается из env на каждый вызов download_video вместо однократного парсинга при старте процесса. - `backend/src/tools/mod.rs` -> Считать MAX_HEIGHT один раз в main.rs при построении AppState/ToolInfo и передавать значение в download_video параметром вместо чтения env на каждый вызов.
285. 🟡 [проблема/DRY] run_ffmpeg и download_video дублируют одинаковый паттерн match status { Ok/Cancelled/TimedOut/Failed } с разными текстами ошибок. - `backend/src/tools/mod.rs` -> Вынести общий helper fn map_proc_status(status, stderr, timeout_msg, fail_fmt: impl Fn(&str) -> String) -> Result<Done>, параметризованный только форматтером ошибки.
286. 🟡 [баг] parse_ytdlp_progress не отличает overall percent от процента отдельного фрагмента при раздельной загрузке video+audio форматов. - `backend/src/tools/mod.rs` -> Учитывать, что yt-dlp пишет отдельный '[download] Destination:' перед каждым фрагментом, и сбрасывать/усреднять базовую точку прогресса при смене фрагмента, либо делить на количество ожидаемых форматов.
287. 🟡 [проблема] tail() пересчитывает весь Vec<&str> из накопленного err_buf при каждом вызове без ограничения на общий размер накопленного буфера. - `backend/src/tools/mod.rs` -> Ограничить err_buf кольцевым буфером фиксированного размера (например, хранить только последние ~64 КБ) при накоплении в run_with_progress, а не постфактум резать в tail().
288. ✅ [баг] check_tool не имел таймаута - **закрыто в раунде 7:** короткие tool checks проходят общий 5-секундный bounded output runner с `kill_on_drop`. `backend/src/tools/mod.rs`
289. 🟠 [проблема] find_by_id делает линейный скан всей директории sources на каждый find_source/find_by_id вызов вместо прямого поиска по известным расширениям. - `backend/src/tools/mod.rs` -> Хранить связку video_id -> filename в БД (или Library) при создании файла и искать по точному пути вместо сканирования директории при каждом обращении.
290. 🟡 [дизайн/OCP] map_ytdlp_error жёстко зашивает распознавание типов ошибок по подстрокам в stderr — новый тип ошибки требует правки самой функции. - `backend/src/tools/mod.rs` -> Вынести таблицу (маркер, сообщение) в статический список пар и итерировать её, чтобы новые случаи добавлялись как данные, а не код.
291. ✅ [баг] validate_url вызывался один раз перед стартом импорта, оставляя TOCTOU на HTTP-редиректах - **закрыто в раунде 6:** `yt-dlp` принудительно идёт через egress-proxy, который повторяет policy на каждом новом target. `backend/src/tools/egress_proxy.rs`
292. 🟠 [баг] acquire_render_lock_or_cancelled удерживает render_lock guard, пока не будет получен jobs_semaphore permit — конкурентные edit-запросы с одинаковым cache_key упираются в permit-голод, удерживая лок долго. - `backend/src/handlers/mod.rs` -> Получать permit до захвата render_lock (сначала дождаться слота в очереди, затем сериализовать по cache_key) либо разделить 'ожидание очереди' и 'сериализация одинаковых рендеров' на независимые этапы, не блокирующие друг друга.
293. 🟡 [улучшение/DRY] mark_cancelled и mark_queue_closed дублируют одинаковый паттерн update_job_if_open + persist_job + clear_cancel, отличаясь только устанавливаемыми полями job. - `backend/src/handlers/mod.rs` -> Вынести общий helper fn finalize_job_if_open(st, jid, f: impl FnOnce(&mut Job)) -> bool, инкапсулирующий update_job_if_open+persist_job+clear_cancel, и вызывать его с разными замыканиями из обоих мест.
294. 🟡 [дизайн/KISS] Цепочка вложенных tokio::select!-хелперов для edit_handler требует держать в голове 4 независимые точки отмены, что легко пропустить при будущих правках. - `backend/src/handlers/mod.rs` -> Свести все точки отмены к единому паттерну (например, обернуть каждый шаг в общий cancellable_step helper, который всегда возвращает Result<T, Cancelled> через select!), убрав отдельную ручную проверку is_cancelled().
295. 🟠 [проблема] finish_job добавляет в library только если updated==true — если job был отменён ровно в момент завершения рендера, готовый файл остаётся на диске, но не попадает ни в library, ни в render-кэш. - `backend/src/handlers/mod.rs` -> При outcome=Ok(Some(..)) но updated==false (job уже terminal не по этой ветке) всё равно выполнять cache_put для валидного файла или удалять его, чтобы не оставлять несогласованное состояние диск/БД.
296. 🟠 [проблема] Аналогичная orphan-файл гонка в import_handler: если скачивание успешно завершилось, но job уже был отменён к моменту finish_job, скачанный source остаётся на диске без cleanup и без записи в library. - `backend/src/handlers/mod.rs` -> В finish_job, если updated==false, но outcome содержал успешный результат, удалять только что созданные файлы (source/output) так же, как это уже делается в ветке Done::Cancelled.
297. 🟡 [проблема] JobStatus::from_token не различает 'неизвестный статус в БД' от 'pending', что маскирует порчу данных при восстановлении. - `backend/src/model.rs` -> Вернуть Result<JobStatus, String> из from_token и залогировать warn в recover_jobs при получении ошибки вместо тихого приведения к Pending.
298. 🟡 [проблема] EditRequest не валидирует диапазоны большинства числовых полей на уровне модели — rotate, quality, censor и другие принимают любые значения из JSON без ограничений. - `backend/src/model.rs` -> Либо добавить #[serde(deserialize_with = ...)] валидаторы на самые опасные поля (rotate, speed, quality), либо явно задокументировать в модели, что EditRequest — сырой DTO и вся валидация намеренно вынесена в normalize_edit_request.
299. 🟡 [дизайн] Job.stage — Option<String> без enum; комментарий перечисляет 3 значения, но код использует минимум 4 разных строковых литерала в разных местах без единого источника истины. - `backend/src/model.rs` -> Заменить Option<String> на Option<JobStage> enum с Serialize в те же строковые токены (serde rename_all snake_case), чтобы все места установки stage проходили проверку компилятором.
300. 🟡 [проблема] Job.result: Option<serde_json::Value> — неограниченная по размеру и структуре полезная нагрузка хранится и в памяти, и в БД без какой-либо схемы. - `backend/src/model.rs` -> Определить конкретный enum/struct JobResult { Import { .. }, Edit { .. } } с ограниченным набором полей вместо serde_json::Value, либо задокументировать и enforced-ограничить максимальный размер сериализованного результата.
301. 🟡 [баг] cancel_open_job зануляет job.progress перед persist, теряя последнее известное значение прогресса отменённой задачи в истории/БД. - `backend/src/state.rs` -> Не сбрасывать progress при отмене (оставить последнее известное значение) — обнулять только stage, так как прогресс сам по себе полезен для UI/истории даже у отменённой задачи.
302. 🟡 [улучшение] probe_video не использует -select_streams и берёт первый видео-поток по порядку в контейнере, а не гарантированно основной. - `backend/src/tools/mod.rs` -> Передавать ffprobe -select_streams v:0 или дополнительно фильтровать по disposition.attached_pic == 0, чтобы гарантированно выбрать основной видеопоток, а не обложку.
303. 🟡 [проблема/DIP] map_ytdlp_error (tools/mod.rs) и validate_url (tools/net.rs) используют разные, несвязанные словари для распознавания ошибок/проблем одного и того же внешнего инструмента (yt-dlp / скачиваемый URL). - `backend/src/tools/mod.rs` -> Ввести единый enum ImportError (InvalidUrl, GeoBlocked, Private, NotFound, Timeout, Other(String)) и конвертировать в него как из validate_url, так и из map_ytdlp_error, чтобы вызывающий код (import_handler) обрабатывал один тип.
304. 🟡 [проблема/OCP] ProcStatus и Done — два похожих, но несовместимых enum для представления исхода одного и того же процесса, требующие ручного match-преобразования в каждом вызывающем месте. - `backend/src/tools/mod.rs` -> Убрать промежуточный ProcStatus и возвращать из run_with_progress сразу Result<Done> с текстом ошибки, вычисленным внутри run_with_progress через обобщённый форматтер (см. также DRY-находку про дублирование match).
305. 🟡 [баг] find_by_id не защищён от TOCTOU между поиском файла источника и последующим его открытием в probe_video/build_ffmpeg_args. - `backend/src/tools/mod.rs` -> Ловить эту ситуацию явно: если ffprobe/ffmpeg падает с ошибкой отсутствия файла, возвращать пользователю понятное 'источник был удалён во время обработки' вместо общей 'ffprobe failed'.
306. 🟡 [проблема] recover_jobs_limit() и MAX_HEIGHT/JOB_TIMEOUT_SECS парсинг по-разному ведут себя при value=0 — recover_jobs_limit явно фильтрует v>0, но MAX_HEIGHT/JOB_TIMEOUT_SECS не отклоняют 0 или отрицательные значения (для u32 отрицательные и так невозможны, но 0 проходит). - `backend/src/tools/mod.rs` -> Добавить единообразную проверку 'v > 0' (или явный отдельный минимум) для MAX_HEIGHT так же, как это уже сделано для RECOVER_JOBS_LIMIT.
307. 🟡 [баг] download_video не отклоняет end <= start при формировании --download-sections, передавая yt-dlp заведомо невалидный диапазон. - `backend/src/tools/mod.rs` -> Проверить end.is_some_and(|e| e <= s) до формирования секции и вернуть содержательную ошибку 'end must be greater than start' до запуска yt-dlp.
308. 🟠 [баг] persist_job считывает job из памяти уже после того, как отпустил лок в момент чтения — конкурентный update_job_if_open между чтением и записью в БД может привести к записи устаревшего снимка job. - `backend/src/state.rs` -> Это фундаментально неизбежно при чтение-затем-запись без сериализации, но можно уменьшить окно, добавив монотонный version-counter в Job и в БД делать UPDATE ... WHERE version <= ?, отбрасывая устаревшие записи вместо слепой перезаписи.
309. 🟡 [проблема/SRP] recover_jobs выполняет чтение из БД, мутацию статуса в памяти и повторную запись в БД в одном неразделённом цикле. - `backend/src/state.rs` -> Разбить на явные шаги: собрать список job, отфильтровать non-terminal, замьютировать их в Vec, батчем персистить в БД, и только затем одним проходом вставить всё в jobs map.
310. 🟡 [проблема] run_with_progress биасит cancel/timeout выше завершения дочернего процесса даже при успешном near-instant exit из-за biased select!. - `backend/src/tools/mod.rs` -> Либо документировать это как намеренное поведение ('отмена побеждает гонку с завершением'), либо перед выходом по cancel/timeout проверять child.try_wait() на предмет уже готового результата и предпочитать его.
311. 🟡 [баг] cache_key в edit_handler вычисляется до normalize_edit_request, поэтому эквивалентные после клэмпинга запросы (например rotate=450 и rotate=90) не разделяют один и тот же render-кэш. - `backend/src/handlers/mod.rs` -> Вызывать normalize_edit_request до вычисления render_cache_key (например, вынести нормализацию в отдельный шаг перед постановкой в очередь) либо сериализовать в ключ только уже нормализованные поля.
312. 🟡 [проблема/SRP] clamp_rect_to_source не проверяет переполнение source_width - min_w при source_width=1, что может привести к панике/underflow на u32. - `backend/src/handlers/mod.rs` -> Использовать saturating_sub вместо прямого вычитания при вычислении верхней границы координат в clamp_rect_to_source.
313. 🟡 [баг] run_with_progress теряет неотправленные прогресс-обновления, если progress-канал (unbounded) уже закрыт получателем (например, drain-таск завершился раньше из-за паники), так как progress.send(...) молча игнорирует ошибку. - `backend/src/tools/mod.rs` -> Логировать через tracing::debug!, когда progress.send возвращает Err, чтобы такие потери были видны в логах, а не полностью незаметны.
314. 🟡 [улучшение/DRY] check_tool для ffmpeg и yt-dlp вызывается последовательно при старте, а не параллельно, увеличивая время запуска сервера на сумму двух health-check таймаутов. - `backend/src/tools/mod.rs` -> Добавить в tools/mod.rs обёртку pub async fn check_all_tools() -> ToolInfo, которая внутри делает tokio::join!(check_tool("ffmpeg",...), check_tool("yt-dlp",...)), инкапсулируя параллельность в одном месте.
315. 🟡 [проблема] tail() режет уже собранный список строк без учёта того, что многобайтовые UTF-8 символы, разорванные на границе строки при разбиении BufReader::lines(), могут дать некорректный вывод при join. - `backend/src/tools/mod.rs` -> Если stderr процесса может содержать не-UTF8 байты (пути с нестандартной кодировкой в именах файлов), заменить BufReader::lines() на построчное чтение с явной lossy-конвертацией (String::from_utf8_lossy) вместо AsyncBufReadExt::lines, чтобы не терять хвост лога при первой невалидной последовательности.

<a id="module-ffmpeg"></a>
### Модуль: FFmpeg domain compiler (args.rs) (53)

FFmpeg domain compiler (`tools/args.rs`, геометрия в `model.rs`). Чистое ядро сборки filter_complex: OCP при добавлении эффектов, DRY между single/concat-путями, типобезопасность.

316. 🟠 [проблема/OCP] Добавление нового формата экспорта требует правки в build_ffmpeg_args, push_video_codec и build_concat_args одновременно - `backend/src/tools/args.rs` -> Свести формат к одной таблице/enum с методами video_codec()/audio_codec()/container(), используемой во всех трёх местах.
317. 🟡 [проблема/DRY] Выбор аудиокодека по формату продублирован между push_audio-вызовами в build_ffmpeg_args и match в build_concat_args - `backend/src/tools/args.rs` -> Вынести fn audio_codec_for(format: &str) -> &'static str и использовать её в обоих местах.
318. 🟡 [проблема/DRY] video_filters и audio_filters дублируют шаблон 'построить цепочку -> push через -vf/-af' в шести ветках формата - `backend/src/tools/args.rs` -> Добавить хелпер push_vf/push_af(args, chain), принимающий уже построенную цепочку и делающий push только если она не пуста.
319. 🔴 [баг] edit.quality (CRF) нигде не валидируется и не клампится перед подстановкой в аргументы ffmpeg - `backend/src/model.rs` -> Добавить в normalize_edit_request кламп quality к диапазону, зависящему от кодека/формата (например 0..=63 для vp9/av1, 0..=51 для x264/x265).
320. 🟠 [проблема] edit.codec и edit.format — произвольные строки без whitelisting на границе домена - `backend/src/model.rs` -> Валидировать format/codec в normalize_edit_request через whitelist и возвращать 400 при неизвестном значении, либо перейти на serde enum с serde(other).
321. 🟡 [проблема] filter_preset принимает произвольную строку в edit.filter, неизвестное имя тихо игнорируется без ошибки - `backend/src/tools/args.rs` -> Либо валидировать filter в normalize_edit_request по тому же списку имён, либо возвращать ошибку из filter_preset для явно нераспознанных значений.
322. 🟡 [дизайн] Магические числа CRF по умолчанию (32, 23, 28) разбросаны по коду без единого источника истины - `backend/src/tools/args.rs` -> Вынести именованные константы DEFAULT_CRF_WEBM/DEFAULT_CRF_AV1/DEFAULT_CRF_H264/DEFAULT_CRF_H265 в один модуль.
323. 🟡 [улучшение/OCP] mp3/png/jpg/gif обрабатываются как особые случаи в общем match вместо единой точки расширения формата - `backend/src/tools/args.rs` -> Выделить каждую ветку в отдельную fn build_<format>_args(...) -> Vec<String>, а match оставить диспетчером.
324. 🟡 [проблема/DRY] Аудио-кодек и битрейт 128k хардкожены в push_audio и продублированы отдельной строкой в build_concat_args - `backend/src/tools/args.rs` -> Вынести общий хелпер push_audio_codec_and_bitrate(args, format) и использовать его в обоих путях.
325. 🟡 [проблема] parse_aspect допускает деформирующие пропорции (например 1:100) без предупреждения пользователю - `backend/src/tools/args.rs` -> Добавить проверку разумного диапазона соотношения (например 1:5..5:1) с явной ошибкой при выходе за пределы.
326. 🟠 [баг] pad-фильтр не форсирует чётность итогового кадра, если crop/scale после него в цепочке отсутствуют - `backend/src/tools/args.rs` -> Добавить unit-тест, который явно проверяет чётность выходного w/h для pad без последующего crop/scale, чтобы застраховать формулу от регрессий.
327. 🟠 [баг] crop с чётностью через 'w & !1' может дать w=0 при вырожденном прямоугольнике шириной 1 в args.rs, если вызвано в обход normalize_edit_request - `backend/src/tools/args.rs` -> Добавить .max(2) прямо в video_filters рядом с '& !1', не полагаясь только на вызывающий код в handlers.
328. 🟡 [проблема] speed для видео (setpts) не ограничен диапазоном на уровне args.rs, хотя atempo для аудио жёстко клампится к 0.5..2.0 - `backend/src/tools/args.rs` -> Либо клампить speed внутри video_filters тем же диапазоном, что и audio_filters, либо явно задокументировать инвариант 'вызывающий обязан клампить' в doc-комментарии функции.
329. 🟡 [проблема] Комментарий 'the frontend clamps speed to that range' в audio_filters недостоверен — на самом деле клампит backend (normalize_edit_request), а не фронтенд - `backend/src/tools/args.rs` -> Поправить комментарий на 'normalize_edit_request on the backend clamps speed to that range before this runs'.
330. 🟡 [дизайн/DIP] video_filters напрямую зависит от конкретных строковых констант ffmpeg вместо промежуточного домена фильтров - `backend/src/tools/args.rs` -> Ввести промежуточный enum VideoFilter { Crop{...}, Rotate(u32), ... } с отдельным to_ffmpeg_string(), чтобы порядок и построение синтаксиса были раздельными задачами.
331. 🟡 [проблема] rotate: i32 в модели принимает произвольные значения, эффективно используются только четыре через rem_euclid(360) - `backend/src/model.rs` -> Валидировать rotate в normalize_edit_request через whitelist [0,90,180,270] с явной ошибкой на прочих значениях.
332. 🟡 [проблема/DRY] Проверка '(speed - 1.0).abs() > 1e-6 && speed > 0.0' дословно продублирована в video_filters и audio_filters - `backend/src/tools/args.rs` -> Вынести fn speed_changed(speed: f64) -> bool и использовать в обеих функциях.
333. 🟡 [улучшение] Эпсилон 1e-6 для сравнения f64 захардкожен в шести разных местах без общей константы - `backend/src/tools/args.rs` -> Ввести const F64_EPS: f64 = 1e-6; в начале файла и использовать её везде вместо литерала.
334. 🟠 [проблема] expected_output_secs не учитывает fade_in/fade_out и не совпадает по сортировке сегментов с build_concat_args - `backend/src/tools/args.rs` -> Задокументировать явно, что expected_output_secs — оценка длительности контента без учёта fade, либо убрать fade из описания функции в комментарии, если он и не должен туда входить.
335. 🟡 [проблема/KISS] format_secs — однострочная функция-обёртка над format!, не добавляющая логики, вопреки комментарию 'trimming trailing noise' - `backend/src/tools/args.rs` -> Либо убрать вводящий в заблуждение комментарий, либо реально удалить незначащие нули, если это когда-то было целью.
336. 🟡 [проблема] gif-путь строит filter_complex-подобный граф вручную внутри строки формата, смешивая video_filters с ручным split/palettegen синтаксисом - `backend/src/tools/args.rs` -> Вынести построение gif-графа в отдельную fn build_gif_filter_graph(parts: &[String], fps: f64) -> String с явным комментарием о смешении синтаксисов.
337. 🟡 [проблема/OCP] push_fps вызывается вручную в каждой ветке push_video_codec и build_ffmpeg_args вместо единой точки в сборке видео-аргументов - `backend/src/tools/args.rs` -> Переместить единственный вызов push_fps в конец push_video_codec и убрать дублирующиеся вызовы из build_ffmpeg_args (av1/prores путь и так проходит через push_video_codec в build_concat_args, но не в build_ffmpeg_args — унифицировать эти два пути).
338. 🟡 [дизайн] eq_changed сравнивает brightness/contrast/saturation с дефолтами 0.0/1.0/1.0, но эти дефолты не именованы как доменные константы - `backend/src/tools/args.rs` -> Явно задокументировать или связать константы дефолтов между model.rs (default_one) и args.rs (eq_changed), например через общий модуль domain-констант.
339. 🟠 [проблема] censor и crop оба типизированы как model::Crop, хотя семантически это разные концепции (censor box vs crop rect) - `backend/src/model.rs` -> Ввести отдельный тип-алиас или newtype CensorBox(Crop), чтобы компилятор различал назначение полей.
340. 🟡 [проблема] Crop.x/y/w/h — u32 без верхней доменной границы, полагается только на clamp_rect_to_source в handlers - `backend/src/model.rs` -> Задокументировать в model.rs, что Crop валиден только после normalize_edit_request, либо добавить конструктор/valid()-метод, чтобы инвариант был явным в типе.
341. 🟡 [идея] Нет доменной проверки совместимости codec с format (например h265 запрошен вместе с format=webm) - `backend/src/tools/args.rs` -> Возвращать явную ошибку или warning в normalize_edit_request, если codec задан вместе с форматом, где он не имеет смысла.
342. 🟡 [идея] Нет доменной валидации, что pad и crop, применённые вместе, дают осмысленный результат - `backend/src/tools/args.rs` -> Добавить интеграционный тест на комбинацию crop+pad с фиксацией ожидаемых размеров, либо предупреждение в API-ответе.
343. 🟡 [проблема] sanitize_color whitelist жёстко ограничен 4 цветами (black/white/gray/red) без возможности расширения без правки кода - `backend/src/tools/args.rs` -> Вынести список допустимых цветов в статический массив const ALLOWED_CENSOR_COLORS, переиспользуемый и для валидации, и потенциально для описания в OpenAPI/типах фронтенда.
344. 🟡 [дизайн/OCP] Добавление нового look-пресета требует знания имени в filter_preset, но нет единого списка допустимых имён, используемого и для валидации, и для UI - `backend/src/tools/args.rs` -> Экспортировать список имён пресетов через отдельную fn filter_preset_names() -> &'static [&'static str] и переиспользовать её при валидации в normalize_edit_request.
345. 🟡 [проблема] parse_aspect полагается только на u32::parse без учёта возможных пробелов внутри числа (не только по краям) - `backend/src/tools/args.rs` -> Добавить unit-тест на parse_aspect с граничными строками ('09:16', ' 9 : 16 ', '9:-16'), чтобы зафиксировать текущее поведение как контракт.
346. 🟠 [проблема] censor применяется в исходных координатах до crop, но нет теста на порядок censor относительно rotate/flip - `backend/src/tools/args.rs` -> Добавить unit-тест на комбинацию censor+rotate=90, документирующий, что censor координаты — всегда в исходной (до поворота) системе координат.
347. 🟠 [улучшение/SRP] build_ffmpeg_args совмещает выбор режима (single vs concat), диспетчеризацию формата и финальную сборку -y/-i/output в одной функции - `backend/src/tools/args.rs` -> Разбить на build_single_args/build_input_trim/dispatch_by_format, оставив build_ffmpeg_args тонким координатором.
348. 🟠 [проблема] quality: Option<u32> используется как CRF для принципиально разных кодеков (x264 0-51, vp9/av1 0-63) без указания в типе, что диапазоны отличаются - `backend/src/model.rs` -> Документировать диапазон по кодеку в doc-комментарии и/или клампить per-codec в normalize_edit_request, как указано в отдельном пункте про отсутствие валидации quality.
349. 🟡 [проблема] censor_color: Option<String> — строка вместо enum, хотя допустимых значений всего четыре - `backend/src/model.rs` -> Либо валидировать значение в normalize_edit_request по тому же whitelist, что и sanitize_color, либо перейти на serde enum с #[serde(rename_all)].
350. 🟡 [проблема] filter: Option<String> в model.rs документирует только 4 из 8 реальных пресетов - `backend/src/model.rs` -> Обновить doc-комментарий в model.rs, перечислив все 8 текущих пресетов, либо сослаться на filter_preset как источник истины.
351. 🟡 [проблема] push_fps форматирует fps с фиксированными 3 знаками после запятой даже для целых значений - `backend/src/tools/args.rs` -> Не критично технически; при желании можно использовать более компактное форматирование только для читаемости логов.
352. 🟠 [проблема] build_concat_args не поддерживает mp3/png/jpg/gif форматы из-за жёсткого matches! в build_ffmpeg_args, хотя multi-segment gif был бы осмысленным сценарием - `backend/src/tools/args.rs` -> Либо расширить build_concat_args для аудио/still-форматов, либо явно возвращать ошибку в handler, если segments заданы вместе с неподдерживающим форматом.
353. 🔴 [баг] render_cache_key считается по EditRequest до normalize_edit_request, а не после нормализации - `backend/src/handlers/mod.rs` -> Переместить вычисление cache_key после normalize_edit_request, либо клонировать/нормализовать req перед первым вызовом render_cache_key.
354. 🟠 [баг] scale с h=-1 может дать нечётную высоту и сломать кодирование в yuv420p - `backend/src/tools/args.rs` -> Либо запретить -1 в validate_scale и разрешить только -2 (гарантированно чётный), либо документировать пользователю разницу и не считать -1 безопасным по умолчанию.
355. 🟡 [проблема/DRY] fps для gif применяется отдельным путём с собственным дефолтом 12.0, минуя push_fps и его защиту диапазона - `backend/src/tools/args.rs` -> Переиспользовать общую функцию resolve_fps(edit) -> f64 с единым дефолтом и диапазоном для всех форматов, включая gif.
356. 🟡 [проблема/SRP] clamp_rect_to_source в handlers занижает минимальный размер прямоугольника до 1px для источников шириной/высотой 1 - `backend/src/handlers/mod.rs` -> Для источников шириной/высотой 1 либо отклонять crop/censor полностью, либо документировать, что вырожденные источники не поддерживают crop/censor.
357. 🟡 [проблема/DRY] push_audio дублирует финальный шаг push_video_codec (кодек+битрейт) как отдельную самостоятельную функцию вместо общей точки сборки аудио+видео пары - `backend/src/tools/args.rs` -> Объединить в один вызов, принимающий format один раз и решающий и видео-, и аудио-кодек вместе, снижая риск рассинхрона между веткам.
358. 🟡 [дизайн/OCP] filter_preset и sanitize_color — закрытые для расширения справочники без единого источника допустимых значений для API-документации - `backend/src/tools/args.rs` -> Добавить эндпоинт GET /api/capabilities (или аналог), отдающий списки допустимых filter/censor_color/codec/format значений, построенные на тех же константах, что использует args.rs.
359. 🟡 [проблема/DIP] expected_output_secs игнорирует reverse и представляет длительность одинаково для reverse и не-reverse клипов - `backend/src/tools/args.rs` -> Добавить короткий doc-комментарий, поясняющий, что reverse намеренно не входит в расчёт длительности, чтобы не создавать ложное ощущение недосмотра при код-ревью.
360. 🟡 [проблема/SRP] edit_handler читает req.format для output_ext ещё до normalize_edit_request, хотя format вообще не валидируется normalize_edit_request - `backend/src/handlers/mod.rs` -> Явно задокументировать в normalize_edit_request или рядом с ней, какие поля EditRequest она не трогает (format/codec/quality/filter/censor_color), чтобы вызывающий код не предполагал обратное.
361. 🟡 [улучшение/KISS] drawbox (censor) форсирует чётность неявно только через clamp_rect_to_source в handlers, а crop форсирует явно через '& !1' в args.rs — асимметричные механизмы для похожей задачи - `backend/src/tools/args.rs` -> Либо убрать дублирующий '& !1' из crop (раз чётность уже гарантирована upstream), либо добавить симметричную защиту для censor, чтобы обе ветки не полагались на разные уровни защиты.
362. 🟡 [проблема/DRY] video_filters и build_concat_args по-разному вычисляют момент начала fade_out относительно out_dur, но сам out_dur получают из одного источника expected_output_secs - `backend/src/tools/args.rs` -> Добавить unit-тест, сравнивающий out_dur, переданный в video_filters, для эквивалентных single-trim и single-segment сценариев, чтобы зафиксировать инвариант согласованности.
363. 🟡 [баг] gif-путь не проходит через push_fps и не наследует его защиту, дублируя логику дефолта fps отдельно от остальных форматов - `backend/src/tools/args.rs` -> Централизовать резолюцию fps (см. resolve_fps) для всех форматов включая gif, независимо от внешней нормализации.
364. 🟡 [проблема/ISP] video_filters принимает весь &EditRequest вместо среза полей, скрывая реальные зависимости фильтра от 20+ полей структуры - `backend/src/tools/args.rs` -> Либо разбить EditRequest на под-структуры (GeometryEdit, ColorEdit, TemporalEdit) и передавать их отдельно, либо явно перечислить используемые поля в doc-комментарии функции.
365. 🟡 [проблема/DRY] format_secs используется только для input-side -ss/-t, а сегменты и fade используют format! напрямую с той же точностью .3, но без общей функции - `backend/src/tools/args.rs` -> Заменить прямые format!("{:.3}", ...) на format_secs(...) везде, где форматируется временная величина в секундах, для единообразия.
366. 🟠 [баг] push_video_codec не имеет audio-ветки для 'mp3'/'png'/'jpg'/'gif' и не используется для них — эти форматы вообще не проходят через общую точку сборки видео-кодека - `backend/src/tools/args.rs` -> Добавить doc-комментарий на push_video_codec, поясняющий, что она вызывается только для форматов из matches! в build_ffmpeg_args, чтобы связь между двумя списками была явной, а не подразумеваемой.
367. 🟠 [баг] edit.quality игнорируется в prores-ветке, хотя поле принимается и документируется как общий CRF-параметр - `backend/src/tools/args.rs` -> Либо использовать edit.quality как -profile:v (0-5 у prores_ks) с валидацией диапазона, либо задокументировать в model.rs, что quality не применяется к формату prores.
368. 🟡 [проблема] validate_scale допускает как -1, так и -2 для одного и того же поля без объяснения разницы в поведении ffmpeg - `backend/src/handlers/mod.rs` -> Задокументировать разницу в doc-комментарии validate_scale или сузить whitelist до -2, если чётность обязательна для всех текущих кодеков (все они требуют yuv420p/yuv422p10le).

<a id="module-persistence"></a>
### Модуль: Persistence layer (DIP) (41)

Persistence layer (`db.rs`, `library.rs`). DIP: два параллельных хранилища без общего трейта-порта, миграции, транзакции, retention.

369. 🔴 [проблема/DIP] Хендлеры и AppState жёстко зависят от конкретных типов Db и Library, общего порта-трейта для хранилища нет. - `backend/src/state.rs` -> Выделить трейты ProjectStore/JobStore/RenderCacheStore и MediaStore и параметризовать AppState ими вместо конкретных Db/Library.
370. 🟠 [проблема] Удаление медиафайла и очистка render_cache не атомарны между Library и Db. - `backend/src/main.rs` -> Обернуть удаление файла из Library и очистку render_cache в одну единицу работы с откатом или сначала чистить кэш, потом файл.
371. 🟠 [проблема] Схема БД объявлена одной константой без системы миграций. - `backend/src/db.rs` -> Ввести пронумерованные миграции (например через sqlx::migrate!) вместо одной статической DDL-строки.
372. 🟠 [проблема] Таблицы jobs и render_cache не имеют retention-политики и растут неограниченно. - `backend/src/db.rs` -> Добавить периодическую очистку старых записей jobs/render_cache по updated_at/created_at, аналогично файловому TTL в main.rs.
373. 🟡 [улучшение/SRP] db.rs смешивает DDL-схему, SQL-запросы и DTO-маппинг в одном файле без разделения по агрегатам. - `backend/src/db.rs` -> Разбить db.rs на модули db/projects.rs, db/jobs.rs, db/render_cache.rs с общим db/mod.rs для пула соединений.
374. 🟡 [баг] load_recent_jobs(0) неотличим от 'без лимита' в вызывающем коде. - `backend/src/db.rs` -> Явно задокументировать в сигнатуре load_recent_jobs, что limit<=0 означает пустой результат, а не 'без лимита', либо ввести Option<i64> для явности.
375. 🟠 [проблема/DIP] Db и Library не тестируемы через in-memory реализацию, тесты гоняют реальную SQLite и файловую систему. - `backend/src/db.rs` -> Добавить трейт-порт хранилища с in-memory реализацией для быстрых юнит-тестов бизнес-логики поверх Db/Library.
376. 🟠 [проблема] upsert_project делает три последовательных запроса без транзакции, гонка при параллельных автосейвах. - `backend/src/db.rs` -> Обернуть SELECT+UPSERT в одну SQL-транзакцию или использовать INSERT ... ON CONFLICT(video_id) DO UPDATE вместо ручного SELECT-then-branch.
377. 🟠 [проблема] video_id не имеет UNIQUE-ограничения на уровне схемы, хотя upsert_project трактует его как уникальный ключ. - `backend/src/db.rs` -> Добавить UNIQUE(video_id) в определение таблицы projects, чтобы гонка приводила к контролируемой ошибке конфликта, а не к тихому дублированию.
378. 🟠 [проблема] Library.save() пишет tmp-файл с фиксированным именем, конкурентные вызовы add/remove гоняют друг друга по одному .tmp пути. - `backend/src/library.rs` -> Использовать уникальное имя tmp-файла (например с суффиксом Uuid или PID) для каждого вызова save().
379. 🟡 [проблема] file_path() в Library жёстко зашивает подкаталоги 'outputs'/'sources' по kind без валидации значения. - `backend/src/library.rs` -> Заменить kind: String на enum MediaKind { Source, Output } с сериализацией, чтобы неверное значение отклонялось на десериализации.
380. 🟡 [проблема] MediaEntry::from_result молча подставляет пустые id/filename/url при отсутствии полей в JSON. - `backend/src/library.rs` -> Вернуть Result<MediaEntry, String> из from_result и явно проверять обязательные поля (id, filename, url) на этапе создания, а не полагаться на проверку в add().
381. 🟡 [проблема] cache_delete_filename ищет по точному совпадению filename, но столбец не индексирован. - `backend/src/db.rs` -> Добавить CREATE INDEX ON render_cache(filename), раз это основной путь очистки при удалении файла.
382. 🟡 [улучшение/OCP] row_to_job и row_to_project - свободные функции вне impl Db, разрывающие связность маппинга со схемой. - `backend/src/db.rs` -> Реализовать TryFrom<SqliteRow> для Job и Project вместо свободных функций.
383. 🟡 [проблема] Job::result хранится как TEXT-блоб JSON без структуры и индекса. - `backend/src/db.rs` -> Вынести часто используемые поля результата (url, filename) в отдельные колонки, оставив result_json для остального.
384. 🟠 [баг] persist_job перезаписывает created_at текущим временем при каждом апдейте вместо сохранения исходной даты создания. - `backend/src/db.rs` -> Передавать и сохранять оригинальный created_at из Job вместо пересчёта now при каждом INSERT.
385. 🟡 [проблема] list_projects и load_jobs не имеют пагинации и грузят все записи одним запросом. - `backend/src/db.rs` -> Добавить необязательный лимит/offset к list_projects и load_jobs, аналогично load_recent_jobs.
386. 🟡 [проблема] Db::open жёстко фиксирует max_connections(5) без возможности конфигурации. - `backend/src/db.rs` -> Прокинуть max_connections через env-переменную по аналогии с MAX_CONCURRENT_JOBS в main.rs.
387. 🟠 [проблема] Library::load молча проглатывает ошибку чтения файла без логирования и без различения 'файла нет' от 'файл повреждён'. - `backend/src/library.rs` -> Логировать через tracing::warn любую ошибку чтения, кроме ErrorKind::NotFound, и по-прежнему падать обратно на пустой список.
388. 🟠 [проблема] serde_json::from_slice в Library::load проглатывает ошибку парсинга без логирования. - `backend/src/library.rs` -> Логировать через tracing::error причину сбоя парсинга перед откатом на пустой список, чтобы потеря данных была видна в логах.
389. 🟡 [проблема/SRP] MediaEntry (доменная DTO) и Library (хранилище) находятся в одном файле без разделения на модель и репозиторий. - `backend/src/library.rs` -> Вынести MediaEntry в отдельный model.rs или library/model.rs, оставив library.rs только за хранилищем.
390. 🟠 [проблема] Library::list делает fs::metadata на каждый файл последовательно при каждом вызове. - `backend/src/library.rs` -> Запускать проверки metadata параллельно через futures::future::join_all вместо последовательного цикла.
391. 🟡 [проблема/DIP] Нет общего понятия 'storage root' между Db и Library - два разных API для одного корня хранилища. - `backend/src/db.rs` -> Ввести общий StorageRoot/AppStorage тип, который оба конструктора принимают, вместо двух разных сигнатур пути.
392. 🟡 [проблема] get_project_by_video при гипотетическом дубле video_id молча возвращает только последний обновлённый. - `backend/src/db.rs` -> После добавления UNIQUE(video_id) эта проблема исчезнет сама; до этого - хотя бы логировать случай, когда SELECT COUNT(*) по video_id > 1.
393. 🟡 [улучшение/KISS] upsert_project делает финальный get_project(&id) вместо построения Project из уже известных данных. - `backend/src/db.rs` -> Возвращать Project напрямую из уже известных полей, делая отдельный SELECT только в ветке обновления для восстановления created_at.
394. 🟡 [проблема] Тесты db.rs и library.rs не проверяют поведение при повреждённых/некорректных данных на диске. - `backend/src/db.rs` -> Добавить тесты, которые пишут заведомо некорректные данные (например, произвольную строку status) в БД/файл и проверяют, что row_to_job/Library::load не паникуют и логируют проблему.
395. 🟠 [баг] Ошибки cache_get маскируются под cache miss в вызывающем коде без логирования. - `backend/src/handlers/mod.rs` -> Логировать через tracing::warn ветку Err(e) отдельно от Ok(None) перед возвратом false.
396. 🟡 [проблема] Все вызовы db.rs для render_cache в вызывающем коде молча игнорируют ошибки без логирования, в отличие от Library. - `backend/src/handlers/mod.rs` -> Заменить let _ = ... на явное логирование ошибки cache_put/cache_delete по аналогии с Library::save.
397. 🟡 [проблема/SRP] db.rs импортирует now_secs из library.rs ради тривиальной утилиты времени. - `backend/src/db.rs` -> Вынести now_secs в отдельный util-модуль (например crate::time), не привязанный ни к db, ни к library.
398. 🟡 [проблема] Колонка schema_version объявлена в схеме projects, но нигде не читается и не задаётся явно приложением. - `backend/src/db.rs` -> Либо использовать schema_version для версионирования формата video/edit JSON, либо убрать колонку, пока не появится нужда.
399. 🟡 [проблема] idx_render_cache_created_at - мёртвый индекс, ни один запрос не сортирует и не фильтрует по created_at. - `backend/src/db.rs` -> Либо удалить неиспользуемый индекс, либо добавить retention-запрос (см. находку про отсутствие очистки), который реально будет использовать сортировку по created_at.
400. 🟡 [проблема] Таблица projects не имеет индекса по updated_at, хотя list_projects и get_project_by_video сортируют именно по нему. - `backend/src/db.rs` -> Добавить CREATE INDEX ON projects(updated_at DESC).
401. 🟡 [улучшение/SRP] GET /api/projects возвращает полные video/edit JSON-блобы для каждого проекта в списке. - `backend/src/handlers/projects.rs` -> Добавить облегчённый list-запрос без video/edit колонок для отображения списка проектов, подгружая полный JSON только по get_project.
402. 🟡 [проблема/DIP] Db::open неявно требует, чтобы каталог storage уже существовал, но создаёт его только main.rs. - `backend/src/db.rs` -> Создавать storage-каталог внутри Db::open через tokio::fs::create_dir_all перед подключением, чтобы модуль не зависел от вызывающего кода.
403. 🟠 [улучшение/DRY] Маппинг kind -> подкаталог продублирован в main.rs отдельным трейтом вместо переиспользования Library::file_path. - `backend/src/main.rs` -> Сделать Library::file_path (или его логику маппинга kind->subdir) публичным методом MediaEntry/Library и переиспользовать в main.rs вместо отдельного трейта.
404. 🟠 [проблема] cache-hit путь finish_from_render_cache не продлевает жизнь записи в render_cache при повторном использовании. - `backend/src/db.rs` -> При успешном cache-hit обновлять created_at (или отдельный last_used_at) записи render_cache, чтобы TTL считался от последнего использования.
405. 🟡 [проблема/ISP] Project (только Serialize) заставляет хендлеры парсить входящий JSON вручную через serde_json::Value. - `backend/src/handlers/projects.rs` -> Ввести отдельный ProjectUpsertRequest со строгими полями и Deserialize, заменив ручной разбор serde_json::Value в хендлере.
406. 🟡 [проблема] Db::open не задаёт synchronous pragma, WAL с дефолтным synchronous=FULL избыточно тормозит запись для локального инструмента. - `backend/src/db.rs` -> Добавить .synchronous(SqliteSynchronous::Normal) к SqliteConnectOptions в Db::open.
407. 🟡 [проблема] upsert_project не ограничивает длину name, хотя video/edit ограничены MAX_PROJECT_JSON_BYTES на уровне хендлера. - `backend/src/db.rs` -> Добавить проверку длины name (например, до нескольких сотен символов) либо в хендлере, либо как защиту внутри upsert_project.
408. 🟡 [баг] cancel_open_job персистит job через persist_job (INSERT ON CONFLICT), что перегенерирует created_at=now даже для уже существующей записи при отмене. - `backend/src/state.rs` -> Передавать в db.persist_job исходный job.created_at (после добавления этого поля в Job/DB, см. отдельную находку про created_at) вместо всегда использования now при INSERT.
409. 🟡 [улучшение/OCP] JobStatus::from_token сворачивает любую нераспознанную строку в Pending без сигнала об ошибке. - `backend/src/model.rs` -> Добавить вариант JobStatus::Unknown(String) или логировать через tracing::warn при попадании в catch-all, прежде чем возвращать Pending.

<a id="module-security"></a>
### Модуль: Security и сеть (45)

Security и сеть (`tools/net.rs`, CORS/ServeDir в `lib.rs`, upload, Docker-хардening).

410. ✅ [проблема] SSRF-проверка URL и фактический HTTP-запрос yt-dlp были разделены во времени (TOCTOU/DNS rebinding) - **закрыто в раунде 6:** proxy резолвит, валидирует всю DNS-выборку и делает connect к тому же pinned `SocketAddr`. `backend/src/tools/egress_proxy.rs`
411. ✅ [проблема] validate_url не проверял редиректы, на которые пойдёт yt-dlp - **закрыто в раунде 6:** все HTTP/CONNECT targets проходят одну policy независимо от redirect-hop. `backend/src/tools/egress_proxy.rs`
412. 🟠 [баг] is_blocked_ip не перечисляет явно IPv4 broadcast и все зарезервированные диапазоны, полагаясь на широкий octets[0] >= 240 - `backend/src/tools/net.rs` -> Добавить явную проверку v4.is_broadcast() (255.255.255.255) и тест на неё, не полагаясь молча на побочный эффект диапазона 240+.
413. 🟡 [улучшение/DRY] looks_like_noncanonical_ip дублирует парсинг IP-подобных меток вместо переиспользования std IpAddr parse - `backend/src/tools/net.rs` -> Вынести общую логику разбора octal/hex/decimal представлений IP в одну функцию, используемую и для канонического, и для неканонического случая.
414. ✅ [проблема] validate_url разрешал произвольный порт на публичном хосте - **закрыто в раунде 6:** initial guard и каждый proxy-hop принимают только 80/443. `backend/src/tools/net.rs`
415. ✅ [дизайн] Комментарий 'basic SSRF guard' занижал фактический объём защиты - **закрыто в раунде 6:** module docs разделяют URL-policy и enforced per-request transport, включая DNS pinning. `backend/src/tools/net.rs`, `backend/src/tools/egress_proxy.rs`
416. 🟠 [проблема] CORS default origins в lib.rs жёстко зашиты под dev-порты 5173/8088 без явного требования CORS_ALLOW_ORIGINS в проде - `backend/src/lib.rs` -> При старте в non-dev окружении (например, когда BIND_ADDR не 127.0.0.1) логировать warn, если CORS_ALLOW_ORIGINS не задан, а дефолты все еще localhost.
417. 🟡 [улучшение] cors_origins_from_env читает CORS_ALLOW_ORIGINS один раз при старте роутера без отдельного этапа валидации конфигурации - `backend/src/lib.rs` -> Валидировать CORS_ALLOW_ORIGINS в main.rs на старте и завершать процесс с понятной ошибкой, если ни одна валидная origin не была распознана при непустой переменной.
418. 🟠 [проблема] ServeDir для /files/sources и /files/outputs не ограничивает конкурентные Range-запросы к одному большому видео - `backend/src/lib.rs` -> Добавить rate-limiting/concurrency-cap middleware (например tower::limit::ConcurrencyLimitLayer) перед ServeDir или ограничить на уровне nginx.
419. 🟡 [идея] Нет теста, подтверждающего что SQLite-файл не отдаётся через ServeDir - `backend/src/lib.rs` -> Добавить интеграционный тест в backend/tests, который убеждается, что запрос к БД-файлу через /files/sources или /files/outputs возвращает 404, а не 200.
420. 🟡 [проблема] CORS allow_headers ограничен только CONTENT_TYPE, что заблокирует будущий Authorization без правки CORS-слоя - `backend/src/lib.rs` -> Зафиксировать в комментарии рядом с cors_layer, что список allow_headers нужно расширить при добавлении аутентификации.
421. 🔴 [проблема] Ни один API-эндпоинт не требует аутентификации - `backend/src/lib.rs` -> Добавить хотя бы простую проверку статического API-ключа или basic-auth middleware перед /api и /files роутами, конфигурируемую через env.
422. 🟠 [проблема/SRP] upload_handler пишет файл на диск до какой-либо валидации содержимого, позволяя гигабайтные не-видео файлы полностью записываться прежде чем быть отклонёнными - `backend/src/handlers/mod.rs` -> Проверять Content-Type и/или делать раннюю частичную проверку (например, ffprobe по первым N МБ через pipe) прежде чем стримить весь файл на диск.
423. 🟡 [улучшение/SRP] upload_handler совмещает парсинг multipart, запись на диск, санитизацию расширения, пробирование видео и обновление библиотеки в одной функции - `backend/src/handlers/mod.rs` -> Выделить сохранение файла, санитизацию имени и построение JSON-ответа в отдельные вспомогательные функции с независимыми unit-тестами.
424. ✅ [баг] sanitize_ext допускал опасные клиентские расширения - **закрыто в раунде 5:** имя клиента display-only, server suffix выводится из `ffprobe format_name` через `safe_upload_extension`. `backend/src/handlers/upload.rs`
425. ✅ [проблема] probe_video не сверял реальный контейнер с клиентским расширением - **закрыто в раунде 5:** клиентское расширение больше не участвует в storage/MIME, контейнер и WebM codec combination определяют server-selected suffix. `backend/src/handlers/upload.rs`
426. 🟡 [проблема] upload_handler читает только первое подходящее multipart-поле и молча возвращает Ok, игнорируя остальные части запроса - `backend/src/handlers/mod.rs` -> Либо явно документировать и валидировать, что запрос должен содержать ровно одно файловое поле, отклоняя лишние поля с 400, либо поддержать множественную загрузку осознанно.
427. 🟡 [улучшение/DRY] Формирование JSON-ответа VideoInfo дублируется почти дословно в import_handler и upload_handler - `backend/src/handlers/mod.rs` -> Вынести построение этого JSON в общую функцию, принимающую id, filename, ProbeInfo, title и sizeBytes.
428. 🟡 [проблема] job_timeout читает переменную окружения JOB_TIMEOUT_SECS при каждом вызове import_handler/edit_handler вместо однократного чтения при старте - `backend/src/handlers/mod.rs` -> Прочитать JOB_TIMEOUT_SECS один раз в main.rs и передавать значение через AppState, как сделано для MAX_CONCURRENT_JOBS.
429. 🟡 [дизайн/OCP] finish_job принимает kind как строковый тег вместо enum, не защищённый компилятором от опечаток - `backend/src/handlers/mod.rs` -> Заменить &str на enum MediaKind { Source, Output } с явным match при сериализации в MediaEntry.
430. 🟡 [проблема] render_cache_key схлопывает все несериализуемые EditRequest в один и тот же пустой ключ - `backend/src/handlers/mod.rs` -> Заменить unwrap_or_default() на явную ошибку/bail при сбое сериализации вместо тихого schlopping в один ключ.
431. 🟠 [проблема] video_id в EditRequest не валидируется как UUID и используется в find_source/find_by_id, который линейно сравнивает file_stem по всей директории - `backend/src/handlers/mod.rs` -> Валидировать video_id как UUID на входе edit_handler/import_handler и возвращать 400 до похода в файловую систему.
432. 🟡 [проблема] find_by_id делает полный линейный перебор директории sources при каждом /api/edit запросе - `backend/src/tools/mod.rs` -> Хранить сопоставление video_id -> путь к файлу в индексе (например, в SQLite или in-memory HashMap), заполняемом при импорте/загрузке, вместо перебора директории.
433. 🟠 [проблема] Backend Docker-образ запускается от root: нет директивы USER для непривилегированного пользователя - `backend/Dockerfile` -> Добавить непривилегированного пользователя (RUN useradd -m appuser) и переключиться на него директивой USER appuser перед CMD.
434. 🟠 [проблема] yt-dlp устанавливается через curl без проверки контрольной суммы или подписи - `backend/Dockerfile` -> Скачивать также файл контрольных сумм релиза и проверять sha256sum скачанного бинарника перед chmod +x.
435. 🟡 [проблема] Build-стадия делает COPY . . перед cargo build, копируя весь build-контекст backend/ в промежуточный слой - `backend/Dockerfile` -> Добавить в backend/.dockerignore явные записи для *.md, tests/ (если не нужны в runtime-образе) и любых потенциальных .env файлов, чтобы гарантия не зависела молча от расположения build-контекста.
436. 🟡 [проблема] Версии базовых образов rust:1-bookworm и debian:bookworm-slim не зафиксированы по digest - `backend/Dockerfile` -> Закрепить оба образа по sha256-digest (FROM debian:bookworm-slim@sha256:...) для воспроизводимых сборок.
437. 🟡 [проблема] apt-get install ставит curl и python3 в финальный рантайм-образ, хотя они нужны только на этапе установки yt-dlp - `backend/Dockerfile` -> Удалить curl (apt-get purge -y curl) в конце той же RUN-инструкции после скачивания yt-dlp, оставив только действительно нужные для рантайма пакеты.
438. 🟡 [проблема] Нет HEALTHCHECK в Dockerfile и healthcheck в docker-compose, несмотря на существующий /api/health эндпоинт - `backend/Dockerfile` -> Добавить HEALTHCHECK CMD curl -f http://localhost:8080/api/health в Dockerfile или healthcheck: секцию в docker-compose.yml, ссылающуюся на этот эндпоинт.
439. 🟡 [проблема] backend не публикует порт наружу, но и не изолирован explicit internal-сетью — полагается только на отсутствие ports: - `docker-compose.yml` -> Объявить явную internal-сеть для backend и подключить к ней frontend отдельным вторым интерфейсом, либо задокументировать, что изоляция опирается только на отсутствие ports: и это осознанный компромисс MVP.
440. 🟠 [проблема] volumes.storage не имеет ограничения размера, при MAX_UPLOAD_BYTES=2GiB на файл и FILE_TTL_HOURS=0 по умолчанию диск может расти неограниченно - `docker-compose.yml` -> Либо задать FILE_TTL_HOURS в environment backend-сервиса по умолчанию, либо ограничить объём volume через driver_opts, либо явно задокументировать это в README как эксплуатационное требование.
441. 🟡 [улучшение] docker-compose.yml не пробрасывает MAX_CONCURRENT_JOBS, MAX_UPLOAD_BYTES, JOB_TIMEOUT_SECS, FILE_TTL_HOURS, CORS_ALLOW_ORIGINS явно, полагаясь целиком на дефолты в main.rs - `docker-compose.yml` -> Добавить хотя бы закомментированные примеры этих переменных в environment: секцию docker-compose.yml, чтобы конфигурация была обнаруживаемой без чтения исходников main.rs.
442. 🟡 [дизайн] BIND_ADDR=127.0.0.1 в Dockerfile и docker-compose.yml=0.0.0.0 расходятся, что критично при прямом docker run без compose - `backend/Dockerfile` -> Уточнить комментарий в Dockerfile, что при самостоятельном docker run с -p потребуется явно передать -e BIND_ADDR=0.0.0.0.
443. 🟡 [проблема] ffmpeg-аргументы логируются целиком через tracing::info! без редактирования путей - `backend/src/handlers/mod.rs` -> Понизить подробность до DEBUG или логировать только идентификаторы video_id/out_id вместо полных ffmpeg-аргументов на уровне INFO.
444. ✅ [проблема] upload_handler не ограничивал число одновременно принимаемых upload-запросов - **закрыто в раунде 7:** независимый pool ограничивает upload тем же capacity, что jobs, но не создаёт starvation между классами работы. `backend/src/handlers/upload.rs`, `backend/src/state.rs`
445. 🔴 [баг] Файл с расширением .html/.svg, прошедший sanitize_ext, отдаётся ServeDir с соответствующим Content-Type, создавая stored XSS - `backend/src/lib.rs` -> Ограничить sanitize_ext allowlist'ом видео-расширений (см. связанную находку в handlers/mod.rs) и/или добавить Content-Disposition: attachment для не-video Content-Type в ServeDir. **Закрыто в раунде 5.**
446. 🟡 [проблема] Ни backend, ни nginx не выставляют X-Content-Type-Options: nosniff или Content-Security-Policy - `frontend/nginx.conf` -> Добавить add_header X-Content-Type-Options nosniff; и базовый Content-Security-Policy в блок server nginx.conf.
447. 🟡 [проблема] /api/import не ограничивает размер скачиваемого видео, только высоту через MAX_HEIGHT и общий таймаут - `backend/src/tools/mod.rs` -> Добавить флаг yt-dlp --max-filesize с лимитом, согласованным с MAX_UPLOAD_BYTES, чтобы download не мог превысить тот же порог, что и ручная загрузка.
448. 🟡 [улучшение] upload_handler не проверяет Content-Type multipart-поля, полагаясь только на имя файла и позднюю проверку ffprobe - `backend/src/handlers/mod.rs` -> Проверять field.content_type() на префикс video/ как раннюю (не единственную) эвристику до начала записи файла.
449. 🟡 [проблема] Заголовок yt-dlp title сохраняется в библиотеку без ограничения длины и без санитизации - `backend/src/tools/mod.rs` -> Ограничить длину title (например, до 500 символов) и отфильтровать управляющие символы перед сохранением в MediaEntry.
450. 🟡 [баг] BIND_ADDR с некорректным значением молча откатывается на 127.0.0.1 вместо явной ошибки старта - `backend/src/main.rs` -> Заменить unwrap_or_else на явный panic!/expect с сообщением о некорректном BIND_ADDR, чтобы ошибка конфигурации была видна сразу при старте контейнера.
451. 🟡 [проблема] /api/health отдаёт точные версии ffmpeg и yt-dlp без аутентификации - `backend/src/handlers/mod.rs` -> Либо скрыть версии инструментов за отдельным защищённым эндпоинтом, либо осознанно принять риск для MVP и задокументировать это в комментарии к health_handler.
452. 🟡 [проблема] job.error, включающий хвост stderr ffmpeg/yt-dlp, персистится в SQLite и отдаётся клиенту через GET /api/jobs/:id без редактирования путей - `backend/src/handlers/mod.rs` -> Санитизировать job.error перед сохранением/выдачей клиенту, отфильтровывая абсолютные пути файловой системы сервера.
453. 🟡 [проблема] clamp_rect_to_source не проверяет переполнение u32 при вычитании source_width/source_height - `backend/src/handlers/mod.rs` -> Заменить прямое вычитание на checked_sub с fallback на 0, чтобы корректность не зависела от точного порядка условий выше по функции.
454. 🟡 [проблема] render_cache_key не проверяет, что video_id в EditRequest всё ещё принадлежит существующему файлу на момент создания кэш-ключа - `backend/src/handlers/mod.rs` -> Явно задокументировать, что кэш-ключ полагается на уникальность UUID video_id на весь срок жизни файла, либо включить в ключ хэш содержимого/mtime исходника.

<a id="module-api-contract"></a>
### Модуль: API contract и обработка ошибок (45)

API contract и ошибки (`model.rs` DTO, ответы хендлеров, `frontend/src/api.ts`, `types.ts`). Типизация границы wire-контракта, единый error-boundary.

455. ✅ [проблема/SRP] Не было общего типа ошибки/IntoResponse - **закрыто в раунде 8:** `error.rs` централизует `AppError`/`AppResult`, safe internal logging и JSON `{error, code}` для handlers/extractors/fallbacks. `backend/src/error.rs`
456. 🔴 [баг] Ошибка валидации EditRequest из normalize_edit_request никогда не попадает в тело HTTP-ответа POST /api/edit - `backend/src/handlers/mod.rs` -> Выполнить дешёвую часть валидации (то, что не требует probe_video) синхронно до spawn и вернуть 400 сразу, либо явно задокументировать это в контракте и убрать соответствующий фронтенд-код, ожидающий немедленной ошибки.
457. ✅ [баг] cancel_handler имел отдельную форму ошибки - **закрыто в раунде 8:** not-found/conflict используют общий `AppError`. `backend/src/handlers/mod.rs`
458. 🟠 [проблема/DRY] import_handler и upload_handler дублируют ручную сборку VideoInfo-JSON с разными допущениями о title - `backend/src/handlers/mod.rs` -> Вынести общий builder fn video_info_json(id, path, title, size) -> Value и передавать title как параметр из каждого источника.
459. 🟠 [проблема/DRY] Ручной json!() вместо typed DTO во всех async job-хендлерах - `backend/src/handlers/mod.rs` -> Добавить в model.rs структуры VideoInfo и EditResult с Serialize и заменить json!() на них в обоих хендлерах.
460. ✅ [проблема/DIP] frontend угадывал форму ошибки по HTTP-статусу - **закрыто в раунде 8:** один parser читает `{error, code}`, сохраняет text fallback и создаёт typed `ApiError`. `frontend/src/api.ts`
461. 🟠 [проблема] Job.result типизирован как serde_json::Value / TS unknown — нет единой формы результата job - `backend/src/model.rs` -> Ввести серверный enum JobResult { Video(VideoInfo), Output(EditResult) } с serde(untagged) и зеркальный union-тип в types.ts вместо unknown.
462. 🟡 [проблема] GET /api/health всегда отвечает 200, даже когда status: "degraded" - `backend/src/handlers/health.rs` -> Возвращать (StatusCode::SERVICE_UNAVAILABLE, Json(...)) при status == "degraded", чтобы код ответа и тело были согласованы.
463. 🟡 [проблема] health-эндпоинт не используется фронтендом вовсе - `frontend/src/api.ts` -> Добавить getHealth() в api.ts и показывать статус в UI (например баннер при status !== "ok").
464. 🟡 [проблема] Нет версионирования API — все пути живут под /api без /v1 - `backend/src/lib.rs` -> Либо задокументировать as-is для локального MVP как осознанное решение, либо ввести /api/v1 префикс до появления второго клиента контракта.
465. 🟠 [проблема/OCP] EditRequest — одна плоская структура на 27+ полей без группировки по фиче - `backend/src/model.rs` -> Разбить EditRequest на вложенные группы с #[serde(flatten)] (TimingOptions, ColorOptions, ExportOptions), сохранив совместимость сериализации.
466. 🟠 [проблема] GET /api/library и GET /api/projects отдают весь список без пагинации - `backend/src/handlers/library.rs` -> Добавить query-параметры limit/offset (или курсор) в оба хендлера и соответствующие типы в api.ts.
467. 🟠 [баг] MediaEntry.kind в Rust — произвольная String, в TS — union 'source'|'output' без валидации на границе - `backend/src/library.rs` -> Заменить String на enum MediaKind { Source, Output } с #[serde(rename_all="lowercase")] в library.rs, что сделает несоответствие невозможным по построению.
468. 🟠 [проблема] POST /api/edit не возвращает 404, если video_id не существует — ошибка видна только после факта в error job - `backend/src/handlers/mod.rs` -> Либо проверить существование source синхронно до spawn и вернуть 404, либо задокументировать асинхронную семантику как контракт и не пытаться её "чинить" частично.
469. ✅ [баг] upload_handler выдавал клиенту сырой `io::Error` - **закрыто в раунде 8:** I/O source логируется как internal cause, наружу идёт безопасный `internal_error`. `backend/src/handlers/upload.rs`, `backend/src/error.rs`
470. 🟡 [проблема] ImportRequest.start/end не валидируются на уровне модели - `backend/src/model.rs` -> Добавить #[serde(deny_unknown_fields)] и явную проверку 0 <= start < end в отдельной validate()-функции ImportRequest, вызываемой синхронно в import_handler до spawn.
471. 🟠 [проблема/SRP] normalize_edit_request совмещает валидацию и мутацию/клэмпинг в одной функции с 10+ независимыми проверками - `backend/src/handlers/mod.rs` -> Разделить на validate_edit_request (только Err) и clamp_edit_request (только приведение к границам), вызываемые последовательно.
472. ✅ [проблема] job_status_handler возвращал пустой 404 - **закрыто в раунде 8:** возвращает общий `not_found` envelope. `backend/src/handlers/mod.rs`
473. 🟡 [проблема] getProjectByVideo — единственное место во фронте, где 404 трактуется как валидный null-результат - `frontend/src/api.ts` -> Ввести общий helper fetchOrNull/fetchOkOr404, явно кодирующий семантику "404 = ожидаемое отсутствие" одним способом для всех трёх мест.
474. 🟠 [проблема] ProjectDto.edit — Partial<EditState> во фронте, но бэкенд хранит edit как serde_json::Value без проверки формы - `backend/src/db.rs` -> Переиспользовать EditRequest (или его подмножество) как typed Deserialize для поля edit в project_upsert_handler вместо произвольного Value.
475. 🟡 [проблема] Project.video — тоже нетипизированный Value, дублирующий VideoInfo без проверки полей - `backend/src/db.rs` -> Десериализовать video как typed VideoInfo DTO (после его введения по пункту дублирования json!()) вместо серого Value.
476. 🟡 [проблема] MAX_PROJECT_JSON_BYTES = 64KB — magic number без сообщения клиенту о лимите заранее - `backend/src/handlers/projects.rs` -> Экспортировать лимит в GET /api/health или отдельный /api/config эндпоинт, чтобы фронтенд мог предупреждать до отправки большого edit-состояния (например при работе с очень длинным списком segments).
477. 🟡 [проблема/DRY] Формирование URL /files/sources/... и /files/outputs/... строковой интерполяцией в нескольких местах бэкенда - `backend/src/handlers/mod.rs` -> Ввести helper fn source_url(filename) / output_url(filename) в handlers/mod.rs и переиспользовать во всех трёх местах.
478. 🟡 [проблема] pollJob сравнивает статус со строковыми литералами вручную вместо exhaustive switch по JobStatus - `frontend/src/api.ts` -> Переписать на switch (job.status) с exhaustive проверкой через never в default, что заставит компилятор упасть при добавлении нового статуса без обработки.
479. 🟡 [проблема] Захардкоженный интервал поллинга 500мс в pollJob не настраивается и не учитывает backoff - `frontend/src/api.ts` -> Ввести нарастающий интервал (например 500мс -> 2с после первых 10 тиков) или вынести константу с комментарием, почему выбрано именно 500мс.
480. 🟡 [проблема] BACKEND_DOWN — единственное клиентское сообщение об ошибке, зашитое на русском в api.ts, тогда как остальные тексты приходят с сервера - `frontend/src/api.ts` -> Задокументировать BACKEND_DOWN как осознанное клиентское исключение (единственный случай, когда сервер физически недостижим и не может прислать текст), не смешивая с остальными серверными сообщениями.
481. 🟡 [проблема] ResultInfo в types.ts не имеет соответствующего Rust DTO — форма выведена только из ручного json!() в edit_handler - `frontend/src/types.ts` -> После введения typed EditResult DTO (см. отдельный пункт про json!()) держать ResultInfo как его прямое зеркало и проверять расхождение в CI-тесте контракта, если такой существует.
482. 🟠 [проблема/ISP] Job — одна структура на все статусы; result/error/progress/stage валидны только в подмножестве состояний - `backend/src/model.rs` -> Смоделировать как enum JobState { Pending, Running{progress,stage}, Done{result}, Error{message}, Cancelled, Interrupted } с serde(tag="status") вместо плоской структуры с опциональными полями.
483. 🟠 [проблема] POST /api/projects принимает произвольный serde_json::Value без typed DTO для тела запроса - `backend/src/handlers/projects.rs` -> Ввести struct ProjectUpsertRequest { video_id: String, name: Option<String>, video: Value, edit: Value } с Deserialize и убрать ручное индексирование Value.
484. 🟡 [проблема] CORS allow_methods не включает PATCH/PUT, ограничивая эволюцию контракта на уровне инфраструктуры - `backend/src/lib.rs` -> Либо оставить as-is как осознанный минимализм MVP, либо добавить Method::PATCH заранее, если планируется частичное обновление проектов.
485. 🟡 [баг] Ошибка «source video not found» из tools::find_source долетает до job.error на английском - `backend/src/handlers/mod.rs` -> Обернуть ошибку find_source в edit_handler через .map_err в русское сообщение ("источник не найден: {video_id}") перед пробросом в outcome.
486. 🟡 [проблема/DRY] import_handler и edit_handler дублируют последовательность finish_job/drain/tx, но edit_handler дополнительно пишет в render cache только внутри себя - `backend/src/handlers/mod.rs` -> Вынести общий хвост в helper fn finalize_job(st, jid, tx, drain, outcome, kind) -> bool, а cache_put оставить отдельным вызовом только в edit-пути после helper'а.
487. 🟠 [баг] upload_handler возвращает 400 BAD_REQUEST на ошибку чтения multipart-поля, даже если она вызвана обрывом соединения клиента - `backend/src/handlers/mod.rs` -> Различать multipart::Error по типу (обрыв потока -> просто прервать без ответа/499-подобная семантика, реальная ошибка формата -> 400).
488. ✅ [проблема/SRP] project_upsert_handler смешивал parsing/name/persistence - **закрыто в раунде 8:** `parse_project_body` и `resolve_project_name` возвращают `ParsedProject`, handler только вызывает repository. `backend/src/handlers/projects.rs`
489. 🟡 [баг] ensure_project_json_size сериализует JSON дважды на каждый upsert без необходимости в успешном пути - `backend/src/handlers/projects.rs` -> Считать размер по одной комбинированной сериализации {video, edit} или переиспользовать уже посчитанные байты для последующей записи в БД вместо повторной сериализации.
490. 🟡 [проблема/DIP] Формат Job.error — plain String — не различает пользовательскую ошибку валидации от внутренней ошибки ffmpeg/IO - `backend/src/model.rs` -> Добавить в Job поле error_kind: Option<ErrorKind> (Validation | Internal) либо разделить сообщение на user-facing и internal (логируемое отдельно через tracing) в finish_job.
491. 🟡 [проблема/DRY] getJob и pollJob не переиспользуют список терминальных статусов, уже определённый на бэкенде через JobStatus::is_terminal - `frontend/src/api.ts` -> Добавить в types.ts функцию isTerminalStatus(status: JobStatus): boolean и использовать её и в pollJob, и в любом другом месте фронта, проверяющем завершённость job.
492. 🟠 [баг] cancelJob проглатывает даже успешный не-2xx ответ (404/409 CancelJobOutcome), не давая вызывающему коду отличить исходы - `frontend/src/api.ts` -> Вернуть из cancelJob Promise<'cancelled'|'not_found'|'already_finished'|'network_error'> вместо void, разобрав тело/статус ответа.
493. 🟡 [проблема/OCP] normalize_edit_request жёстко перечисляет пары (поле, русское сообщение) построчно вместо декларативного описания - `backend/src/handlers/mod.rs` -> Осознанно оставить как есть для текущего размера EditRequest (27 полей управляемо) либо ввести макрос/массив описаний только если список продолжит расти.
494. 🟡 [проблема/ISP] ProjectDto на фронте требует video: VideoInfo целиком, хотя store.ts передаёт в saveProject произвольный state.video без структурной проверки - `frontend/src/api.ts` -> Типизировать параметр saveProject как { videoId: string; name?: string; video: VideoInfo; edit: Partial<EditState> } вместо Record<string, unknown>, чтобы TS проверял вызывающий код, а не только ответ.
495. 🟠 [баг] project_get_handler (GET /api/projects/:id) и project_list_handler/getProjects/deleteProject объявлены и экспортированы, но не используются нигде на фронтенде - `backend/src/handlers/projects.rs` -> Либо удалить неиспользуемые эндпоинты/функции, либо подключить их к UI (например список сохранённых проектов), если такая фича планируется.
496. 🟡 [проблема/SRP] finish_from_render_cache совмещает чтение кэша, валидацию имени файла, проверку существования файла на диске и обновление статуса job в одной функции - `backend/src/handlers/mod.rs` -> Вынести валидацию имени + существования файла в отдельную fn cached_output_is_usable(st, filename) -> bool и оставить в finish_from_render_cache только оркестрацию.
497. 🟡 [проблема] stage: Option<String> в Job — произвольная строка без enum - `backend/src/model.rs` -> Ввести enum JobStage { Queued, Downloading, Processing } с #[serde(rename_all="lowercase")] и использовать его вместо строковых литералов во всех трёх местах присвоения.
498. 🟡 [проблема] MAX_PROJECT_JSON_BYTES проверяется отдельно для video и edit по 64KB каждый, но нет предела на итоговый размер строки, которую пишет upsert_project - `backend/src/handlers/projects.rs` -> Добавить дополнительную проверку суммарного размера (video.len() + edit.len() <= MAX_PROJECT_TOTAL_BYTES) в project_upsert_handler.
499. 🟡 [баг] deleteLibraryItem и deleteProject трактуют 404 как success молча, cancelJob не проверяет статус вовсе — три DELETE/POST-подобные мутации обрабатывают отсутствие ресурса по-разному - `frontend/src/api.ts` -> Вынести общий helper типа async function deleteOrNotFound(path): Promise<void> и использовать его в обоих DELETE-вызовах; для cancelJob явно решить, какой из трёх паттернов уместен, и привести к нему же.

<a id="module-frontend-store"></a>
### Модуль: Frontend state и store (50)

Frontend state/store (`store.ts`, `toasts.ts`). God-module: SRP через домены (import/export/library/projects/history/presets/theme/player), DRY между async-экшенами.

500. 🟠 [проблема/SRP] store.ts на 675 строк совмещает импорт, экспорт, библиотеку, историю, пресеты, тему, плеер и автосейв проектов в одном модуле - `frontend/src/store.ts` -> Разбить на отдельные модули (import/export, library, history, presets, theme, project-autosave) с явными публичными API.
501. 🟠 [проблема/DRY] doImport и doUpload дублируют блок сброса state.edit из VideoInfo - `frontend/src/store.ts` -> Вынести общий хелпер applyFreshEditFor(v: VideoInfo) и использовать его в doImport/doUpload/openFromLibrary.
502. 🟠 [проблема/DRY] doImport, doUpload и doExport дублируют skeleton try/catch/finally для флагов importing/exporting - `frontend/src/store.ts` -> Обобщить в helper вида runJob({flagKey, statusKey, ...}, fn) или единый композабл useAsyncJob.
503. 🟠 [баг] doUpload не поддерживает отмену загрузки, хотя использует тот же UI-паттерн importing, что и doImport - `frontend/src/store.ts` -> Либо поддержать AbortController в api.uploadFile и прокинуть отмену, либо явно задокументировать невозможность отмены в UI.
504. 🟡 [проблема/SRP] Модуль регистрирует watch() на верхнем уровне при импорте, а не при инициализации приложения - `frontend/src/store.ts` -> Обернуть регистрацию watch в экспортируемую функцию initStore(), вызываемую один раз из main.ts/App.vue.
505. 🟡 [проблема] История и автосейв-watch не имеют cleanup API, таймеры остаются висеть при потере компонента - `frontend/src/store.ts` -> Вернуть handle от watch() и таймеров в объекте, который можно явно dispose() из тестов или при смене видео.
506. 🟠 [проблема/DIP] Автосейв проекта построен на трёх module-level mutable let-переменных с неявными состояниями гонки - `frontend/src/store.ts` -> Свести три переменные в один объект/enum состояния autosaveState = 'idle'|'restoring'|'restored' с явными переходами.
507. 🟡 [баг] savePreset использует name как ключ идентичности без нормализации регистра - `frontend/src/store.ts` -> Нормализовать ключ сравнения через toLowerCase() при поиске совпадения, сохраняя оригинальный регистр для отображения.
508. 🟡 [баг] loadLibrary молча глотает ошибку без уведомления пользователя - `frontend/src/store.ts` -> Добавить toast('error', ...) в catch loadLibrary для единообразия с остальными сетевыми операциями.
509. 🟠 [проблема] applySnapshot перезаписывает весь state.edit целиком через JSON.parse при undo/redo - `frontend/src/store.ts` -> Использовать Object.assign(state.edit, JSON.parse(json)) с явным пересозданием вложенных объектов, чтобы сохранить единый реактивный корень.
510. 🟡 [проблема/DRY] toast() вызывается с разнородными форматами сообщений без единой точки форматирования ошибок - `frontend/src/store.ts` -> Ввести helper errorText(e: unknown): string и использовать его во всех catch-блоках вместо повторяющегося тернарника.
511. 🟡 [дизайн] Toast всегда автозакрывается через фиксированные 4 секунды независимо от длины текста и вида - `frontend/src/toasts.ts` -> Добавить необязательный параметр durationMs (или вычислять из text.length) и увеличить дефолт для kind='error'.
512. 🟡 [проблема] dismissToast использует findIndex+splice вместо filter, а nextId — незащищённая module-level переменная - `frontend/src/toasts.ts` -> Заменить на toasts.splice(0, toasts.length, ...toasts.filter(t => t.id !== id)) или просто оставить filter-присваивание и добавить экспортируемый resetToasts() для тестов.
513. 🟠 [проблема/ISP] Компоненты импортируют широкий срез store вместо только нужных им полей - `frontend/src/components/MediaLibrary.vue, frontend/src/components/EditPanel.vue, frontend/src/components/UrlImport.vue` -> Выделить под каждый домен (import, library, edit) отдельный reactive-срез или composable с точечным API вместо общего state.
514. 🟡 [баг] resetHistory и history.past хранят до 100 полных JSON-снапшотов EditState без сжатия - `frontend/src/store.ts` -> Либо уведомлять пользователя при достижении лимита истории, либо хранить дифф вместо полной копии state.edit на каждый шаг.
515. 🟡 [проблема/DIP] store.ts напрямую импортирует toast из конкретного модуля и обращается к document/localStorage напрямую - `frontend/src/store.ts` -> Ввести интерфейсы NotificationPort и StoragePort, инжектируемые в store, чтобы логика была тестируема без реального DOM/localStorage.
516. 🟡 [баг] loadPresets и initTheme не различают 'нет данных' и 'испорченные данные' в localStorage - `frontend/src/store.ts` -> В catch логировать/чистить повреждённый ключ localStorage.removeItem, чтобы не пытаться парсить его повторно на каждой загрузке.
517. 🟡 [улучшение/KISS] buildEditPayload — 60-строчная функция с плотной бизнес-логикой внутри store.ts - `frontend/src/store.ts` -> Вынести buildEditPayload, tierToCrf, sanitizeRect в отдельный модуль edit-payload.ts, не зависящий от reactive state импорта/экспорта. **Закрыто в раунде 5:** чистое ядро находится в `domain/edit.ts`, `store.ts` оставляет совместимый фасад.
518. 🟠 [проблема] seekTo не защищён от NaN/Infinity во входном значении - `frontend/src/store.ts` -> Добавить явную проверку Number.isFinite(t) в начале функции и игнорировать вызов при невалидном значении.
519. 🟡 [проблема] setTrimStartFromPlayer/setTrimEndFromPlayer используют фиксированный зазор 0.1 секунды не связанный с fps видео - `frontend/src/store.ts` -> Вычислять минимальный зазор как 1/fps (если fps видео известен), с fallback на текущую константу 0.1.
520. 🟡 [проблема/DRY] onImportTick и onExportTick — идентичные по структуре функции с разными префиксами полей state - `frontend/src/store.ts` -> Обобщить через фабрику makeTickHandler(prefix) или общий helper, принимающий ref-пару progress/stage.
521. 🟠 [баг] applyPreset делает Object.assign без валидации значений из localStorage - `frontend/src/store.ts` -> Валидировать каждое поле p.edit по известной схеме EditState перед Object.assign, отбрасывая недопустимые значения.
522. 🟡 [проблема/SRP] Модуль экспортирует данные и функции плоским набором из ~35 именованных экспортов без фасада - `frontend/src/store.ts` -> Сгруппировать экспорты в именованные объекты-фасады (editorActions, libraryActions, presetActions) либо разнести по отдельным файлам per предыдущий пункт SRP.
523. 🟡 [проблема] toasts.ts не ограничивает количество одновременно показанных тостов - `frontend/src/toasts.ts` -> Ограничить очередь (например, максимум 5 одновременно, старые вытеснять) в самой toast().
524. 🟡 [проблема] toast() не даёт возможности отменить свой setTimeout при ручном dismissToast - `frontend/src/toasts.ts` -> Сохранять handle таймера и очищать его в dismissToast через clearTimeout при ручном закрытии.
525. 🟠 [проблема] restoreProject молча проглатывает ошибку сети без уведомления пользователя - `frontend/src/store.ts` -> Добавить toast('error', 'Не удалось восстановить сохранённые настройки') в catch restoreProject.
526. 🟠 [проблема] persistProject не даёт пользователю знать, что автосейв не удался - `frontend/src/store.ts` -> Показать неинтрузивный индикатор статуса автосейва (например, точку 'не сохранено' рядом с именем клипа) и/или toast при повторных неудачах подряд.
527. 🟡 [проблема/OCP] tierToCrf хранит таблицу CRF-значений как захардкоженный литерал внутри функции - `frontend/src/store.ts` -> Вынести table в модульную константу верхнего уровня (или отдельный конфиг-файл), экспортируемую отдельно от функции поиска.
528. 🟠 [баг] buildEditPayload не защищён от инвертированного trim (trimStart > trimEnd) - `frontend/src/store.ts` -> В начале buildEditPayload явно клампить trimEnd = Math.max(e.trimStart, e.trimEnd) перед дальнейшими расчётами cut/trim.
529. 🟡 [идея] Нет способа переименовать сохранённый пресет - `frontend/src/store.ts` -> Добавить renamePreset(oldName, newName), обновляющую entry.name на месте без создания нового элемента списка.
530. 🟡 [проблема] Все toast-сообщения на русском захардкожены прямо в бизнес-логике store.ts - `frontend/src/store.ts` -> Вынести строки в отдельный messages.ts/i18n-словарь и ссылаться на ключи вместо инлайновых литералов.
531. 🟠 [баг] deleteFromLibrary оставляет state.edit/state.result/history в противоречивом состоянии при удалении текущего клипа - `frontend/src/store.ts` -> При сбросе state.video также вызывать resetHistory() и обнулять state.edit/state.result до дефолтных значений.
532. 🟠 [баг] cancelImport/cancelExport не защищены от повторного клика во время отмены - `frontend/src/store.ts` -> Добавить локальный флаг cancelling и обернуть api.cancelJob в try/catch с toast('error', ...) при неудаче.
533. 🟡 [баг] doUpload не проверяет пустой/некорректный файл и не защищён от двойного клика во время uploading - `frontend/src/store.ts` -> Добавить проверку file && file.size > 0 с toast('error', ...) при пустом/битом файле перед вызовом api.uploadFile.
534. 🟡 [баг] seekTo при отсутствующем видео не ограничивает t снизу после последующей загрузки - `frontend/src/store.ts` -> Явно возвращать без действия (или клампить к 0), если state.video === null, вместо использования t как собственного потолка.
535. 🟡 [проблема/DRY] Дублирование форматирования длительности/размера файла между компонентами дублирует проблему из store.ts - `frontend/src/components/VideoPreview.vue, frontend/src/components/MediaLibrary.vue` -> Добавить formatDuration/formatSize рядом с parseTime в store.ts (или отдельном utils.ts) и переиспользовать в обоих компонентах.
536. 🟡 [баг] toasts.ts не защищает nextId от коллизий между HMR-перезагрузками модуля в dev-режиме - `frontend/src/toasts.ts` -> Использовать более устойчивый генератор id (например crypto.randomUUID()) вместо инкрементного module-level счётчика.
537. 🟡 [улучшение/SRP] openFromLibrary смешивает синхронный сброс дефолтного edit с последующим асинхронным restoreProject - `frontend/src/store.ts` -> Показать состояние загрузки (skeleton/disabled edit panel) на время restoreProject вместо промежуточного показа дефолтного edit.
538. 🟡 [проблема/DIP] pollJob использует единый жёстко закодированный интервал опроса без backoff, вне контроля store.ts - `frontend/src/store.ts, frontend/src/api.ts` -> Дать api.pollJob принимать опциональный интервал/backoff-стратегию и передавать её из store.ts в зависимости от типа операции (импорт vs экспорт).
539. 🟡 [проблема/KISS] buildEditPayload использует магический порог 0.05 секунды в четырёх местах без именованной константы - `frontend/src/store.ts` -> Вынести в именованную константу EPSILON_SECONDS = 0.05 с комментарием об источнике значения.
540. 🟠 [баг] setTrimStartFromPlayer/setTrimEndFromPlayer не обновляют cut.start/cut.end при сужении диапазона trim - `frontend/src/store.ts` -> После изменения trim пересчитывать/клампить state.edit.cut в те же функции, аналогично тому, как это уже частично делает watcher в EditPanel.vue при cutEnabled.
541. 🟡 [проблема] toast() не ограничивает длину text, длинные сообщения об ошибке рендерятся без обрезки - `frontend/src/toasts.ts, frontend/src/store.ts` -> Обрезать текст в toast() до разумной длины (например 200 символов) с многоточием, либо добавить CSS line-clamp в Toasts.vue.
542. 🟠 [баг] doImport при ошибке валидации диапазона выходит раньше установки importing, разрешая спам одинаковых тостов - `frontend/src/store.ts` -> Либо дебаунсить повторные идентичные ошибки валидации, либо кратковременно блокировать повторный вызов (например через локальный флаг validating).
543. 🟡 [улучшение/OCP] PRESET_KEYS — захардкоженный список полей EditState, требующий ручной синхронизации - `frontend/src/store.ts` -> Либо генерировать PRESET_KEYS из схемы EditState с явным исключением геометрических полей, либо добавить тест, проверяющий покрытие всех полей EditState (кроме геометрии) в PRESET_KEYS.
544. 🟡 [баг] savePreset/deletePreset не синхронизируют presets.list между вкладками браузера - `frontend/src/store.ts` -> Добавить storage-event listener, перечитывающий presets.list при изменении ключа ve_presets в другой вкладке.
545. 🟡 [улучшение/DRY] initTheme и loadPresets повторяют один и тот же паттерн безопасного чтения из localStorage с разной обработкой ошибок - `frontend/src/store.ts` -> Вынести общий helper safeReadLocalStorage<T>(key, validate, fallback): T, используемый в обеих функциях и в persistPresets/applyTheme.
546. 🟡 [баг] restoreProject: promise более раннего вызова продолжает выполняться впустую при быстрой смене клипов - `frontend/src/store.ts` -> Использовать AbortController на каждый вызов restoreProject и отменять предыдущий запрос при смене videoId.
547. 🟡 [проблема/SRP] applyTheme одновременно мутирует реактивное состояние, DOM и localStorage в одной функции - `frontend/src/store.ts` -> Разбить на setThemeState(t), applyThemeToDom(t) и persistTheme(t), вызываемые последовательно из toggleTheme/initTheme.
548. 🟡 [проблема/DRY] EditPanel.vue дублирует ту же формулу клампа cut.start/cut.end, что и buildEditPayload в store.ts - `frontend/src/components/EditPanel.vue` -> Вынести clampToRange(value, trimStart, trimEnd) в store.ts (или общий utils) и переиспользовать в EditPanel.vue и buildEditPayload.
549. 🟡 [проблема] Нет экспортируемой функции сброса всего state/presets/ui для тестов - `frontend/src/store.ts, frontend/src/store.test.ts` -> Добавить экспортируемую resetStateForTests() в store.ts, переустанавливающую все поля state/presets/ui/history к дефолтам, и использовать её в beforeEach.

<a id="module-frontend-components"></a>
### Модуль: Frontend компоненты и интерактивность (45)

Frontend компоненты (`EditPanel.vue` и остальные `.vue`). God-component, дублирование drag/pointer-логики, отсутствие переиспользуемых примитивов.

550. 🔴 [проблема/SRP] EditPanel.vue — компонент на 725 строк объединяет пресеты, тайминг, кадр, цвет, звук и экспорт - `frontend/src/components/EditPanel.vue` -> Разбить на подкомпоненты по секциям (TimingSection, FrameSection, ColorSection, AudioSection, ExportSection), EditPanel оставить оркестратором.
551. 🟠 [проблема/DRY] Массивы опций (formats, filters, aspects, speeds, rotations, censorColors, padAspects, widthPresets, fpsPresets, qualityTiers) захардкожены в EditPanel.vue - `frontend/src/components/EditPanel.vue` -> Вынести в отдельный модуль (например edit-options.ts) с типами и экспортировать в EditPanel.
552. 🟠 [проблема/DRY] Pointer-драг дублируется в RectOverlay и TrimSlider с разными правилами клампинга - `frontend/src/components/RectOverlay.vue` -> Вынести общий composable useDragHandle(window pointermove/up + cleanup) и переиспользовать в обоих компонентах.
553. 🔴 [баг] RectOverlay не снимает window pointermove/pointerup при размонтировании во время активного драга - `frontend/src/components/RectOverlay.vue` -> Добавить onUnmounted(() => { window.removeEventListener('pointermove', onMove); window.removeEventListener('pointerup', onUp) }). **Закрыто в раунде 5.**
554. 🔴 [баг] TrimSlider не снимает window pointermove/pointerup при размонтировании во время активного драга - `frontend/src/components/TrimSlider.vue` -> Добавить onUnmounted, снимающий pointermove/pointerup, аналогично для onTrackDown. **Закрыто в раунде 5.**
555. 🟡 [дизайн] RectOverlay в режиме censor стартует с нулевым прямоугольником до срабатывания watcher в EditPanel - `frontend/src/components/RectOverlay.vue` -> Инициализировать censor сразу валидным прямоугольником в самом sеттере cropEnabled/censorEnabled, а не через отдельный watcher.
556. 🟠 [проблема/DIP] Все компоненты редактора напрямую импортируют глобальный singleton state/actions из store.ts - `frontend/src/components/EditPanel.vue` -> Ввести props/emit или provide/inject границу между презентационными компонентами и store, либо явно принять глобальный стор как осознанный архитектурный выбор для MVP.
557. 🟠 [проблема/DRY] fmt()/fmtDuration() форматирование времени продублировано тремя разными реализациями - `frontend/src/components/EditPanel.vue` -> Вынести единый formatTime(seconds, {withTenths?}) в общий utils-модуль и использовать во всех трёх компонентах.
558. 🟡 [проблема/DRY] fmtSize (форматирование байтов) дословно продублирован в VideoPreview.vue и MediaLibrary.vue - `frontend/src/components/VideoPreview.vue` -> Вынести fmtSize(bytes) в общий utils-модуль и импортировать в обоих компонентах.
559. 🟡 [баг] TrimSlider.onKey использует фиксированный шаг Shift=1с независимо от диапазона слайдера - `frontend/src/components/TrimSlider.vue` -> Сделать big = e.shiftKey ? (props.max - props.min) * 0.05 (или отдельный proп bigStep) вместо жёсткой единицы.
560. 🟡 [баг] TrimSlider.onTrackDown может выбрать не ту ручку при клике между близко расположенными хэndlами - `frontend/src/components/TrimSlider.vue` -> Учитывать реальную ширину .trim-handle в пикселях при выборе ближайшей ручки, а не только числовое значение.
561. 🟡 [улучшение/KISS] RectOverlay.dims()/normalizeRect() пересчитываются на каждый computed и каждый onMove без мемоизации - `frontend/src/components/RectOverlay.vue` -> Кэшировать { W, H } в computed(() => ({W: state.video?.width||1, H: state.video?.height||1})) и переиспользовать.
562. 🟠 [дизайн] Нет визуальной индикации, какой из двух RectOverlay (crop или censor) активен, если оба включены одновременно - `frontend/src/components/VideoPreview.vue` -> Добавить подпись рядом с каждым прямоугольником ('Кадрирование' / 'Замазка') или временно скрывать неактивный, пока не наведена мышь.
563. 🟡 [баг] Минимальный зазор между trimStart/trimEnd в EditPanel.commitStart/commitEnd (0.1с) не совпадает с GAP в TrimSlider - `frontend/src/components/EditPanel.vue` -> Вынести GAP в общий модуль-константу и импортировать в обоих местах вместо дублирования литерала 0.1.
564. 🟡 [дизайн] Инпуты времени startStr/endStr не валидируют вживую, только по blur/enter - `frontend/src/components/EditPanel.vue` -> Показывать текстовую подсказку рядом с полем при startInvalid/endInvalid вместо одной только CSS-подсветки.
565. 🟡 [проблема/SRP] applyPlatform() смешивает доменную логику пресетов платформ с UI-компонентом EditPanel - `frontend/src/components/EditPanel.vue` -> Вынести applyPlatform в store.ts или отдельный platform-presets.ts модуль, EditPanel вызывает готовую функцию.
566. 🟡 [проблема/OCP] setAspect()/aspectActive() жёстко привязаны к массиву aspects, добавление нового соотношения требует правки функции и вызывающего кода - `frontend/src/components/EditPanel.vue` -> Описать aspects как конфигурацию с fn-обработчиком внутри самого объекта, а не как отдельные параметры, разбираемые в компоненте.
567. 🟠 [баг] widthPresets 'active' подсветка сравнивает только scale.w, игнорируя h - `frontend/src/components/EditPanel.vue` -> Сравнивать также h или хранить id применённого preset отдельно от вычисляемых scale.w/h.
568. 🟡 [дизайн] TrimSlider визуально неотличим для секций 'Обрезка' (границы клипа) и 'Вырезать кусок из середины' (cut) - `frontend/src/components/EditPanel.vue` -> Добавить differentiating-стиль (например другой accent-цвет заливки) для cut-слайдера через prop variant.
569. 🟡 [улучшение/KISS] watch(cutEnabled) и watch(censorEnabled) реализуют одинаковый паттерн 'seed default rect on enable' независимо друг от друга - `frontend/src/components/EditPanel.vue` -> Вынести generic watchAndSeed(source, isValid, seedFn) helper и переиспользовать для обоих случаев.
570. 🟡 [дизайн] UrlImport: текстовые поля importStart/importEnd не валидируются на лету - `frontend/src/components/UrlImport.vue` -> Добавить локальную валидацию через parseTime() с подсветкой поля при вводе некорректного формата, аналогично startInvalid в EditPanel.
571. 🟡 [дизайн] UrlImport: URL-инпут и dropzone-кнопка визуально равнозначны без индикатора выбранного источника - `frontend/src/components/UrlImport.vue` -> Добавить радио-подобное переключение источника или блокировать URL-инпут после выбора файла и наоборот.
572. 🟡 [баг] UrlImport.onDrop не сбрасывает state.url при drop файла - `frontend/src/components/UrlImport.vue` -> Очищать state.url в onDrop перед вызовом doUpload или показывать явное предупреждение о том, что URL игнорируется.
573. 🟡 [баг] UrlImport dragover-подсветка мигает при пересечении дочерних элементов dropzone - `frontend/src/components/UrlImport.vue` -> Считать глубину вложенности через dragenter/dragleave-counter или использовать pointer-events:none на детях во время dragover.
574. 🟡 [баг] ResultPanel.downloadName не убирает управляющие символы и переносы строк из video.title - `frontend/src/components/ResultPanel.vue` -> Расширить regex до /[\\/:*?"<>|\x00-\x1f]+/g или использовать общий sanitizeFilename-хелпер.
575. 🟡 [проблема/DRY] downloadName в ResultPanel дублирует regex-санитайзинг имени файла, не вынесенный в общий модуль - `frontend/src/components/ResultPanel.vue` -> Вынести sanitizeFilename(name, ext) в utils-модуль.
576. 🟠 [дизайн] MediaLibrary: кнопка удаления '✕' не запрашивает подтверждение - `frontend/src/components/MediaLibrary.vue` -> Добавить confirm-диалог или двухшаговое подтверждение (например, требовать повторный клик в течение 3с).
577. 🟠 [проблема/ISP] ProgressBar.stage типизирован как голый string, а не union известных значений - `frontend/src/components/ProgressBar.vue` -> Типизировать stage как union 'queued'|'downloading'|'uploading'|'processing' и синхронизировать с типами в store.ts/types.ts.
578. 🟡 [баг] ProgressBar.progress-fill всегда показывает минимум 2% ширины даже при progress=0 - `frontend/src/components/ProgressBar.vue` -> Показывать 0% как реальный 0, а минимальную видимую полоску применять только к indeterminate-состоянию.
579. 🟡 [проблема/DRY] ProgressBar.progress-fill inline-style дублирует условие 'known' вместо единого computed - `frontend/src/components/ProgressBar.vue` -> Вынести fillWidth в computed(() => known.value ? Math.max(2, props.progress!) + '%' : undefined).
580. 🟡 [дизайн] Toasts не имеют явной кнопки закрытия, только клик по всему телу - `frontend/src/components/Toasts.vue` -> Добавить явную кнопку закрытия и увеличить таймаут (или сделать его бесконечным) для kind='error'.
581. 🟡 [баг] App.vue.onKey не проверяет state.exporting/state.importing перед обработкой хоткеев - `frontend/src/App.vue` -> Добавить ранний return при state.exporting (или ограничить только undo/redo/seek, не трогающие edit во время экспорта).
582. 🟡 [дизайн] Хоткеи Space/I/O/стрелки/,/. нигде в UI не задокументированы, кроме мелкой подписи в футере - `frontend/src/App.vue` -> Добавить всплывающую справку по хоткеям (например по ? или иконке помощи) с полным списком.
583. 🟡 [улучшение/SRP] App.vue.onKey — монолитная функция смешивает undo/redo, playback control и trim-seeking в одном switch - `frontend/src/App.vue` -> Разбить на handleHistoryKeys/handlePlaybackKeys/handleSeekKeys и вызывать по очереди с early-return.
584. 🟠 [дизайн] VideoPreview использует нативные controls браузера, не синхронизированные визуально с TrimSlider - `frontend/src/components/VideoPreview.vue` -> Скрыть нативный seek или кастомизировать progress-бар плеера, синхронизировав его с trim-диапазоном визуально.
585. 🟠 [баг] VideoPreview.onTimeUpdate зацикливает только по trimStart/trimEnd, не пропуская cut-диапазон - `frontend/src/components/VideoPreview.vue` -> При cutEnabled проверять попадание currentTime в cut.start..cut.end и перескакивать на cut.end, как это будет в финальном рендере.
586. 🟡 [дизайн] CSS-фильтры videoStyle — грубое приближение серверного ffmpeg-рендера без явной пометки 'превью примерное' - `frontend/src/components/VideoPreview.vue` -> Добавить отдельную пометку 'цветокоррекция в превью приблизительная' рядом с секцией эффектов в EditPanel.
587. 🟡 [баг] denoise/sharpen/grain/vignette эффекты никак не отражены в живом превью - `frontend/src/components/VideoPreview.vue` -> Добавить хотя бы грубые CSS-аналоги (box-shadow inset для vignette, SVG turbulence overlay для grain) или явно пометить эти контролы как 'без предпросмотра'.
588. 🟡 [проблема/DIP] RectOverlay жёстко использует var(--accent)/var(--danger) как дефолтные цвета через props default - `frontend/src/components/RectOverlay.vue` -> Требовать явную передачу color от родителя без дефолта, завязанного на тему, либо вынести цвета в отдельный theme-config, импортируемый в местах использования.
589. 🟡 [улучшение/KISS] MediaLibrary.ext() не защищён от файлов без точки в имени - `frontend/src/components/MediaLibrary.vue` -> Проверять filename.includes('.') перед split и возвращать '' при отсутствии расширения.
590. 🟡 [дизайн] MediaLibrary не показывает, какой source-файл сейчас открыт в редакторе - `frontend/src/components/MediaLibrary.vue` -> Добавить class active при e.id === state.video?.id и подсветить строку в списке.
591. 🟡 [баг] EditPanel.onSavePreset не блокирует Enter в пустом поле имени пресета - `frontend/src/components/EditPanel.vue` -> Добавить ту же проверку presetName.trim() перед вызовом onSavePreset по Enter, как у кнопки.
592. 🟡 [баг] applyPlatform для shorts/reels может оставить crop несогласованным с уже включённым censor - `frontend/src/components/EditPanel.vue` -> После смены aspect в applyPlatform дополнительно кламповать censor через sanitizeRect с новыми границами.
593. 🟡 [баг] parseTime не ограничивает число сегментов, разделённых двоеточием - `frontend/src/store.ts` -> Ограничить parts.length <= 3 (hh:mm:ss) и возвращать null при превышении.
594. 🟡 [дизайн] ProgressBar.cancellable в UrlImport скрывает кнопку без объяснения, почему отмена недоступна - `frontend/src/components/UrlImport.vue` -> Показывать disabled-кнопку с title-объяснением вместо полного скрытия элемента управления.

<a id="module-design"></a>
### Модуль: Визуальный дизайн и UX (взгляд дизайнера) (55)

Визуальный дизайн и UX. Иерархия, spacing/type scale, цвет, консистентность состояний контролов, responsive, empty/loading/error states — взгляд дизайнера, не только код.

595. 🟠 [дизайн] Пустое состояние без загруженного видео обрывается пустым фоном без сообщения - `frontend/src/App.vue` -> Добавить `v-else` блок с иллюстрацией/текстом-подсказкой 'Импортируй видео, чтобы начать'.
596. 🟡 [дизайн/SRP] Карточка импорта не сворачивается после загрузки клипа - `frontend/src/components/UrlImport.vue` -> Сворачивать `.card.import` в компактный collapsed-заголовок, когда `state.video` уже установлен.
597. 🟠 [баг] Колонки редактора .left/.right не имеют CSS-правил, порядок держится только на DOM - `frontend/src/style.css` -> Добавить явные grid-column/flex правила для `.left`/`.right` или удалить неиспользуемые классы, положившись на порядок в `.editor`.
598. 🟡 [дизайн] Двухколоночный редактор не sticky, левая колонка обрывается при скролле длинной формы - `frontend/src/style.css` -> Добавить `position: sticky; top: 16px` для `.left` в пределах `.editor`.
599. 🟡 [дизайн/ISP] Нативные чекбоксы визуально не согласованы с pill-кнопками для однотипных toggle-паттернов - `frontend/src/style.css` -> Свести все boolean-переключатели к единому компоненту (кастомный switch или chip), убрав нативные чекбоксы из `.toggle`.
600. 🟡 [дизайн] Плоская типографическая шкала между заголовком, лейблами и подсказками - `frontend/src/style.css` -> Добавить `font-weight: 600` для `.card h2` и `.group-title`, оставив 400 для обычного текста полей.
601. 🟡 [дизайн] Подзаголовок и футер почти неразличимы по контрасту относительно заголовка - `frontend/src/style.css` -> Дать `.foot` более приглушённый `--faint` вместо `--muted`, чтобы визуально развести вступление и подвал.
602. 🟠 [дизайн] Акцентный цвет применяется только к кнопке импорта и активным чипам, остальной интерфейс монотонно тёмно-серый - `frontend/src/style.css` -> Добавить лёгкий акцентный левый бордер или фоновую подсветку для активных/заполненных секций EditPanel.
603. 🟡 [дизайн] Disabled-состояние текстовых полей визуально не обозначено - `frontend/src/style.css` -> Добавить `.url-input:disabled, .time-input:disabled { opacity: 0.55; cursor: not-allowed; }`.
604. 🟡 [баг] Невалидные time-input поля не имеют focus-состояния, отличного от error-состояния - `frontend/src/style.css` -> Добавить `.time-input.invalid:focus { border-color: var(--danger); box-shadow: 0 0 0 3px rgba(255,107,107,0.25); }`.
605. 🟡 [дизайн] Индикатор невалидности time-input не имеет aria-атрибутов, только цвет рамки - `frontend/src/components/EditPanel.vue` -> Добавить `:aria-invalid="startInvalid"` на оба time-input в EditPanel.vue.
606. 🟠 [проблема/DRY] Форматирование длительности и размера дублируется в MediaLibrary.vue, VideoPreview.vue и EditPanel.vue - `frontend/src/components/MediaLibrary.vue` -> Вынести fmtDuration/fmtSize/fmt в общий `frontend/src/format.ts` и импортировать во всех трёх компонентах.
607. 🟡 [дизайн] Плотность полей EditPanel одинаковая для простых toggle и составных контролов - `frontend/src/style.css` -> Дать составным полям (с вложенными chips+grid2) увеличенный нижний отступ или разделитель, отличный от простых toggle-полей.
608. 🟠 [проблема] Chip-группы визуально идентичны несмотря на разную семантику выбора - `frontend/src/components/EditPanel.vue` -> Добавить отдельный визуальный стиль (например, с иконкой-стрелкой) для action-чипов вроде 'Под платформу', отличный от persistent-toggle чипов.
609. 🟡 [дизайн] Значок медиатеки .lib-badge для источника не имеет цветового отличия от результата - `frontend/src/style.css` -> Добавить `.lib-badge.source { color: var(--accent); border-color: var(--accent); }` для симметрии с output.
610. 🟡 [дизайн] Группировка 'Появление/Затухание' и цветовых слайдеров в grid2 не показывает единицы визуально - `frontend/src/components/EditPanel.vue` -> Обернуть числовое значение в `<span class="value">` с tabular-nums и чуть большим весом шрифта.
611. 🟡 [дизайн] Заголовок с эмодзи не имеет aria-hidden, попадает в озвучку скринридера - `frontend/src/App.vue` -> Обернуть эмодзи в `<span aria-hidden="true">🎬</span>` отдельно от текста заголовка.
612. 🟡 [дизайн] Кнопка темы использует эмодзи как визуальный индикатор состояния, дублирующий текст - `frontend/src/App.vue` -> Оставить один канал: либо только текст, либо иконку без дублирующего слова, но не оба одновременно.
613. 🟡 [дизайн] Ручки crop/censor overlay имеют фиксированный радиус 14px независимо от размера видео-превью - `frontend/src/style.css` -> Задать размер ручек в `clamp()` относительно ширины `.player-wrap`, а не фиксированным px.
614. 🟠 [дизайн] На узких вьюпортах правая колонка EditPanel остаётся очень длинной без якорей/табов - `frontend/src/style.css` -> На мобильной ширине превратить `.group-title` в кликабельные аккордеон-заголовки с обычным `<details>`/toggle.
615. 🟡 [дизайн] progress-fill.indeterminate использует !important для ширины - `frontend/src/style.css` -> Убрать `!important`, так как ProgressBar.vue уже не выставляет `:style` в indeterminate-режиме (строка 40).
616. 🟠 [дизайн] Кнопка 'Отмена' в ProgressBar визуально идентична обычным ghost-кнопкам действий - `frontend/src/components/ProgressBar.vue` -> Добавить модификатор вроде `.btn.ghost.sm.warn` с цветом `--danger` для кнопки отмены прогресса.
617. 🟡 [дизайн] Тосты позиционируются fixed поверх контента без учёта видео-превью снизу справа - `frontend/src/style.css` -> Сдвинуть `.toasts` в левый нижний угол или под видео-плеер, подальше от основных кнопок действия.
618. 🟡 [дизайн] grid2 в блоке кадрирования не выравнивает подписи по базовой линии с range-полями цвета - `frontend/src/style.css` -> Задать единую высоту строки лейбла (`line-height`/`min-height`) в `.grid2 label`, независимо от типа вложенного input.
619. 🟡 [дизайн] Кнопки 'Сброс кадрирования' и 'Сбросить цвет' стилизованы по-разному - `frontend/src/components/EditPanel.vue` -> Привести оба reset-действия к единому компоненту (например, всегда `.btn.ghost.sm`), расположенному по единому правилу (например, справа от заголовка группы).
620. 🟡 [дизайн] Подсказка про реверс — обычный .hint, а не предупреждение, хотя описывает риск памяти - `frontend/src/components/EditPanel.vue` -> Использовать `--warn` цвет или отдельный класс `.hint.warn` для этой строки вместо обычного `.hint`.
621. 🟡 [дизайн] Подсказка для GIF рекомендует указать размер и отрезок, но рядом нет перехода к этим контролам - `frontend/src/components/EditPanel.vue` -> Добавить ссылку-кнопку в hint, которая скроллит/фокусирует на поля обрезки времени и изменения размера.
622. 🟠 [дизайн] Кнопка 'Экспортировать' не имеет визуальной иерархии относительно 7 секций формы выше - `frontend/src/components/EditPanel.vue` -> Добавить видимый разделитель (padding-top + border-top акцентного цвета) или sticky-футер для кнопки экспорта.
623. 🟡 [дизайн] Активный чип 'Поворот 0°' неотличим от невыбранной чипы других групп при беглом сканировании - `frontend/src/style.css` -> Не подсвечивать чип, соответствующий дефолтному/неизменённому значению, как активный, либо выделять изменённые от дефолта группы отдельно.
624. 🟠 [проблема] Числовые поля кропа X/Y/Ширина/Высота не имеют верхней границы max относительно размеров видео - `frontend/src/components/EditPanel.vue` -> Добавить `:max="state.video?.width"`/`:max="state.video?.height"` на соответствующие поля кропа.
625. 🟡 [дизайн] Единственная шкала border-radius не документирует, какой компонент какой уровень использует - `frontend/src/style.css` -> Добавить CSS-комментарий у объявления переменных с правилом: card=lg, поля/кнопки=radius, мелкие бейджи/пилюли=sm или 999px.
626. 🟠 [дизайн] Медиатека не показывает пустое состояние при первом визите - `frontend/src/components/MediaLibrary.vue` -> Показывать `.card.library` всегда с сообщением-заглушкой, когда `state.library.length === 0`.
627. 🟡 [дизайн] Drag-over состояние применяется ко всей карточке импорта, включая уже заполненные поля - `frontend/src/style.css` -> Ограничить визуальную реакцию на dragover только зоной `.dropzone`, не подсвечивая всю карточку целиком.
628. 🟡 [дизайн/KISS] Кнопка 'Скачать' в медиатеке — это <a>, а не <button>, визуально неотличима от btn ghost sm кнопок рядом - `frontend/src/components/MediaLibrary.vue` -> Ничего не менять визуально, но задокументировать в компоненте, что это осознанный выбор `<a download>`, либо унифицировать через общий `.btn` wrapper-компонент, скрывающий разницу тега.
629. 🟡 [дизайн] meta-chip и lib-badge используют одинаковый pill-паттерн для разных по смыслу данных - `frontend/src/style.css` -> Дать `.lib-badge` прямоугольную форму (`--radius-sm`) вместо pill, чтобы визуально отделить категорию от метаданных.
630. 🟡 [дизайн] Indeterminate-прогресс не сообщает реальный этап явно, если progress долго равен null - `frontend/src/components/ProgressBar.vue` -> После N секунд в indeterminate-режиме показывать дополнительный текст вроде 'это может занять некоторое время' в progress-label.
631. 🟡 [дизайн] topbar-row размещает h1 и переключатель темы в одну строку без обработки переноса на узких вьюпортах - `frontend/src/style.css` -> Добавить `flex-wrap: wrap` для `.topbar-row` на мобильной ширине.
632. 🟠 [баг] У censor-прямоугольника нет числовых полей X/Y/W/H в отличие от кропа - `frontend/src/components/EditPanel.vue` -> Добавить аналогичный `.grid2` с X/Y/Ширина/Высота, привязанный к `state.edit.censor`, как у кропа.
633. 🟡 [дизайн] Чипы 'Под платформу' визуально идентичны persistent-toggle чипам, хотя это одноразовые действия - `frontend/src/components/EditPanel.vue` -> Стилизовать action-чипы как `.btn.ghost.sm` вместо `.chip`, чтобы визуально отличать 'применить пресет' от 'выбрать значение'.
634. 🟠 [проблема/ISP] Ни одна из chip-групп не имеет role=radiogroup и aria-pressed/aria-checked на активном чипе - `frontend/src/components/EditPanel.vue` -> Добавить `:aria-pressed="activeCondition"` на каждую `.chip`-кнопку и `role="group"` с `aria-label` на обёртку `.chips`.
635. 🟡 [баг] Нет :disabled стилей для .url-input и .time-input во время импорта - `frontend/src/style.css` -> См. фикс выше: добавить общее правило `:disabled { opacity: 0.55; cursor: not-allowed; }` для обоих классов полей.
636. 🟡 [дизайн] В style.css задан ровно один font-weight на весь файл - `frontend/src/style.css` -> См. фикс про типографическую шкалу выше — добавить 2-3 уровня font-weight (400/500/600) для разных ролей текста.
637. 🟡 [дизайн] Кнопка 'Сбросить цвет' занимает 4-ю ячейку grid2 рядом с тремя слайдерами - `frontend/src/components/EditPanel.vue` -> Вынести кнопку сброса цвета из `.grid2` в отдельную строку над или под сеткой слайдеров.
638. 🟡 [дизайн] Подписи 'Под платформу' и 'Поля под пропорции (letterbox)' разной длины нарушают вертикальный ритм списка полей - `frontend/src/components/EditPanel.vue` -> Ограничить длину лейблов одной строкой везде или вынести пояснение '(letterbox)' в отдельный `.hint` под чипами.
639. 🟠 [баг] crop.w/crop.h можно обнулить через number input, normalizeCrop вызывается только по blur - `frontend/src/components/EditPanel.vue` -> Добавить debounce-валидацию на `@input`, а не только на `@blur`, либо clamp значение сразу в RectOverlay через normalizeRect (частично уже есть, но crop.w=0 может дать деление на 0 в rectStyle до нормализации).
640. 🟡 [дизайн] meta-chip растянут без ограничения ширины — длинные codec-строки переполняют ряд - `frontend/src/components/VideoPreview.vue` -> Добавить `max-width` и `text-overflow: ellipsis` с `title`-атрибутом на `.meta-chip` для длинных значений codec.
641. 🟡 [дизайн] Кнопки истории и primary-кнопка экспорта используют одинаковый .btn паттерн без учёта частоты использования - `frontend/src/components/EditPanel.vue` -> Оставить как есть по сути, но явно закрепить hotkey-подсказку (уже есть title у истории) и рассмотреть увеличение hit-area кнопок истории, раз они используются чаще.
642. 🟡 [проблема/SRP] watch на cutEnabled и censorEnabled дублируют паттерн 'seed default rect on enable' без общей абстракции - `frontend/src/components/EditPanel.vue` -> Вынести общую функцию `seedRectIfEmpty(enabledRef, rectRef, seedFn)` в store и переиспользовать для cut/censor.
643. 🟡 [дизайн] trim-times использует flex-wrap с justify-content space-between, но pos-btns центрируется, ломая порядок на средних ширинах - `frontend/src/style.css` -> Задать явный `order` или обернуть `.pos-btns` в отдельный ряд с `flex-basis: 100%` при переносе.
644. 🟡 [дизайн] lib-badge и .chip.active оба красят через border-color/background два разных паттерна 'активного' состояния - `frontend/src/style.css` -> Привести `.lib-badge.output` к той же заливке, что `.chip.active`, либо явно задокументировать разницу как 'статус' vs 'выбор'.
645. 🟡 [баг] Минимальная ширина 2% индикатора прогресса визуально неотличима от близких к нулю значений на треке высотой 8px - `frontend/src/components/ProgressBar.vue` -> Поднять минимальный порог до 4-5% либо добавить текстовый процент рядом, который уже показывается через progress-label, чтобы не полагаться только на визуальную полоску.
646. 🟡 [дизайн] Чипы поворота и FPS используют одинаковую ширину при разной длине текста, что даёт неровный ритм внутри группы - `frontend/src/components/EditPanel.vue` -> Задать `.chips .chip { min-width: 56px }` для числовых/коротких групп, чтобы выровнять ширину визуально близких по смыслу чипов.
647. 🟡 [дизайн] h1 использует letter-spacing, а card h2 и group-title — нет, несогласованный трекинг заголовков - `frontend/src/style.css` -> Свести к осознанной шкале: например, отрицательный tracking только для крупных заголовков (h1/h2), положительный — только для uppercase-лейблов (group-title), без промежуточных исключений.
648. 🟡 [проблема] aspectActive использует epsilon-сравнение 0.02, которое может ложно подсвечивать чип 4:3 при похожем кастомном кропе - `frontend/src/components/EditPanel.vue` -> Сузить epsilon до значения, дающего false positive только при реально идентичном соотношении (например, 0.005), или сравнивать по нормализованному отношению с учётом округления до чётных пикселей.
649. 🟡 [дизайн] Поле имени пресета использует class time-input, смешивая семантику 'ввод времени' и 'произвольный текст' - `frontend/src/components/EditPanel.vue` -> Ввести отдельный класс `.text-input` для полей произвольного текста и использовать его для имени пресета, оставив `.time-input` только для времени.

<a id="module-testing"></a>
### Модуль: Тесты и quality gates (40)

Тесты и quality gates (`backend/tests/`, `store.test.ts`, CI). Конкретные непокрытые пути и тесты с ложным чувством защищённости.

650. 🔴 [проблема] Ни один тест не бьёт по гонке двух одновременных идентичных edit-запросов - `backend/tests/api.rs` -> Добавить тест, который параллельно (tokio::join!) отправляет два идентичных /api/edit и проверяет единственный результат рендера и общий cache-хит.
651. 🟠 [проблема] cancel_queued_edit_does_not_wait_for_permit не проверяет отсутствие орфанного ffmpeg-процесса - `backend/tests/api.rs` -> Добавить тест с реальным ffmpeg (аналогично render.rs), который отменяет job после старта процесса и проверяет через ps/pgrep, что процесс действительно завершён.
652. 🔴 [проблема] pollJob и весь api.ts не покрыты ни одним тестом - `frontend/src/api.ts` -> Добавить frontend/src/api.test.ts с fetch-моками (msw или vi.stubGlobal('fetch', ...)), покрывающий pollJob (включая обработку ошибок и терминальных статусов) и остальные экспортируемые функции.
653. 🟡 [баг] Тест 'flushes a pending edit before redo' не проверяет ветку flushPendingHistory без pending-таймера - `frontend/src/store.test.ts` -> Добавить кейс, где redo()/undo() вызывается сразу после предыдущего commit без новых изменений, и проверить, что history не меняется лишний раз.
654. 🟠 [проблема] normalizeCrop и sanitizeRect не тестируются с video=null или width/height=0 - `frontend/src/store.test.ts` -> Добавить тест normalizeCrop() при state.video = null и при width/height = 0, проверяющий, что функция не падает и не производит NaN/Infinity в state.edit.crop.
655. 🟠 [проблема] Нет теста на doImport/doUpload/doExport целиком, только их составные части - `frontend/src/store.ts` -> Добавить тесты doImport/doUpload/doExport с замоканными api.importUrl/uploadFile/edit/pollJob, проверяющие обновление state.video, state.jobs и обработку ошибок.
656. 🟠 [проблема] restoreProject не тестируется на смену videoId во время in-flight запроса - `frontend/src/store.ts` -> Добавить тест: вызвать openFromLibrary для видео A, затем до resolve промиса переключиться на видео B, и проверить, что применённый результат A не перезаписывает состояние B.
657. 🟠 [проблема] Нет regression-корпуса реальных ffmpeg-запусков за пределами трёх сценариев - `backend/tests/render.rs` -> Добавить минимум один тест на build_concat_args с segments и один на одновременные crop+censor, оба через реальный ffmpeg.
658. 🟠 [проблема] Реальные ffmpeg-тесты не проверяют ошибочный путь - `backend/tests/render.rs` -> Добавить тест, который подсовывает run_ffmpeg заведомо некорректные args (или повреждённый source) и проверяет Done::Failed вместо паники или зависания.
659. 🟠 [проблема] JOB_TIMEOUT_SECS нигде не тестируется на реальное срабатывание - `backend/src/handlers/mod.rs` -> Добавить render.rs-тест, который выставляет JOB_TIMEOUT_SECS в малое значение через env и запускает заведомо долгий ffmpeg-рендер, проверяя переход job в error/timeout.
660. 🟡 [проблема] Нет теста на восстановление после падения посреди рендера с недописанным output-файлом - `backend/tests/api.rs` -> Добавить в jobs_survive_restart создание недописанного *.part или .mp4 файла в outputs/ перед recover_jobs и проверить, что он не отдаётся как валидный результат.
661. 🟠 [проблема] Нет теста на гонку cancel_open_job и finish_job в момент завершения worker'а - `backend/src/handlers/mod.rs` -> Добавить тест с tokio::join! на cancel_open_job и finish_job над одной job, запущенный многократно (loom или stress-repeat), чтобы отловить неатомарность.
662. 🟡 [проблема] Нет property/fuzz-тестов для normalize_edit_request и clamp_rect_to_source - `backend/src/handlers/mod.rs` -> Добавить proptest, генерирующий source_width/height и rect в диапазоне 0..=8000, и утверждающий, что clamp_rect_to_source никогда не паникует и всегда возвращает rect внутри границ источника.
663. ✅ [проблема] SSRF-тесты не покрывали DNS rebinding между валидацией URL и фактическим скачиванием - **закрыто в раунде 6:** injected resolver меняет public initial answer на private download answer; тест подтверждает policy block до connect. `backend/src/tools/egress_proxy.rs`
664. 🟡 [проблема/DRY] make_state дублируется почти дословно между api.rs и handlers/mod.rs::tests::state() - `backend/tests/api.rs` -> Вынести общую фабрику AppState для тестов в отдельный test-util модуль (например backend/src/test_support.rs с #[cfg(test)]) и переиспользовать из обоих мест.
665. 🟡 [проблема] Нет теста на конкурентный upload двух файлов с коллизией video_id - `backend/tests/api.rs` -> Добавить тест, отправляющий два параллельных multipart upload и проверяющий, что оба файла сохраняются под разными id без порчи данных.
666. 🟡 [проблема] upload-тесты не проверяют путь ошибки probe_video на реальном файле - `backend/tests/api.rs` -> Добавить тест с реальным ffmpeg (в render.rs или отдельном ffmpeg-gated тесте), который заливает файл с мусорным содержимым и проверяет 400/ошибку от probe_video, а не панику.
667. ✅ [проблема] клиентский extension selector не был покрыт прямым или HTTP-level тестом - **закрыто в раунде 5:** `safe_upload_extension` unit-тестирован, HTTP regression загружает MP4 как `payload.html` и проверяет server-selected `.mp4`/MIME/headers. `backend/src/handlers/upload.rs`, `backend/tests/api.rs`
668. 🟡 [проблема] cache_key коллизии между разными videoId с одинаковым edit не тестируются с реальным разделением файлов - `backend/tests/api.rs` -> Добавить тест: закэшировать результат для videoId='vidX', затем отправить идентичный edit с videoId='vidY' и проверить, что кэш не срабатывает (реальный рендер или явная ошибка отсутствия источника).
669. 🟡 [проблема] CI не запускает Makefile check, дублируя его шаги вручную вместо переиспользования - `.github/workflows/ci.yml` -> Заменить пошаговые run-команды в ci.yml на вызов `make check` (или отдельных `make lint`/`make test`), чтобы Makefile оставался единственным источником истины.
670. 🟡 [проблема] CI не публикует и не проверяет покрытие тестами ни для backend, ни для frontend - `.github/workflows/ci.yml` -> Добавить cargo-llvm-cov для backend и vitest --coverage для frontend с минимальным порогом, публикуемым как job summary или артефакт.
671. 🟠 [проблема] Frontend CI не запускает никаких компонентных тестов — их просто нет - `frontend/src/store.test.ts` -> Добавить @vue/test-utils и минимум smoke-тест на монтирование App.vue, ловящий явные runtime-ошибки рендера.
672. 🟡 [проблема] Нет теста project_upsert_rejects_oversized_json_fields для поля edit - `backend/tests/api.rs` -> Добавить тест с huge-строкой внутри edit вместо video и проверить тот же PAYLOAD_TOO_LARGE с текстом 'edit' в сообщении об ошибке.
673. 🟡 [проблема] Нет теста на некорректный EditRequest на HTTP-уровне через /api/edit - `backend/tests/api.rs` -> Добавить тест POST /api/edit с телом невалидного JSON и с полем неверного типа, проверяющий явный 400 Bad Request, а не паническую 500.
674. 🟡 [проблема] health_reflects_tool_availability не проверяет сериализацию ffmpegVersion/ytdlpVersion - `backend/tests/api.rs` -> Добавить сценарий make_state с заполненными ffmpeg_version/ytdlp_version и проверить, что они появляются в JSON-ответе под ожидаемыми ключами.
675. 🟠 [проблема] files_route_serves_only_media_subdirectories не проверяет path traversal через ../ - `backend/tests/api.rs` -> Добавить тест GET /files/sources/../app.db (и URL-encoded вариант) и проверить, что ответ не 200 и не содержит содержимого БД.
676. 🟡 [проблема] Нет теста двойной параллельной отмены одной и той же задачи - `backend/src/state.rs` -> Добавить тест, вызывающий cancel_open_job дважды параллельно (tokio::join!) на одной job и проверяющий, что ровно один вызов вернул Cancelled, а другой — AlreadyFinished.
677. 🟡 [проблема] edit_cache_hit_returns_existing_output не проверяет отсутствие повторного добавления в library при cache-хите - `backend/tests/api.rs` -> Добавить в тест проверку library.list().len() до и после запроса — должно остаться 1 запись с id='cached', а не появиться вторая.
678. 🟠 [проблема] Нет теста buildEditPayload на censor с уменьшенным crop - `frontend/src/store.test.ts` -> Добавить тест: включить cropEnabled с ненулевым x/y, задать censor и проверить, что buildEditPayload().censor либо пересчитан относительно crop, либо документированно передаётся в абсолютных координатах и backend это ожидает.
679. 🟡 [улучшение/SRP] poll_terminal использует busy-poll с sleep(10ms)×400 итераций - `backend/tests/api.rs` -> Заменить busy-poll на watch-канал/уведомление о завершении job в AppState, которое тест может ожидать напрямую без фиксированного интервала опроса.
680. 🟡 [проблема] Нет теста library_delete на симметричный случай удаления source-записи - `backend/tests/api.rs` -> Добавить тест: создать source-запись и кэш-запись с ссылкой на неё, удалить source через DELETE /api/library/{id} и проверить, зависит ли инвалидация кэша от source, а не только от output.
681. 🟡 [проблема] Нет теста store.ts на seekTo/seekRelative/togglePlay/setTrimStartFromPlayer/setTrimEndFromPlayer - `frontend/src/store.ts` -> Добавить тесты с моком HTMLVideoElement (или state.playerEl-эквивалентом), проверяющие корректность seekTo/seekRelative границ и обновление trimStart/trimEnd от текущей позиции плеера.
682. 🟡 [проблема] render_locks в AppState растёт без ограничений и не покрыт тестом на очистку - `backend/src/state.rs` -> Добавить периодическую очистку render_locks для ключей без активных Mutex-владельцев (Arc::strong_count == 1) в spawn_cleanup и тест, подтверждающий, что размер карты не растёт бесконечно.
683. 🟡 [проблема] spawn_cleanup (TTL-очистка sources/outputs) не покрыт ни одним тестом - `backend/src/main.rs` -> Вынести тело spawn_cleanup в тестируемую async-функцию, принимающую TTL и текущее время как параметры, и добавить тест с искусственно состаренными файлами.
684. 🟡 [проблема/DRY] PRESET_KEYS в store.ts не синхронизирован с EditState и не покрыт regression-тестом - `frontend/src/store.ts` -> Добавить тест, который через Object.keys(defaultEdit()) сверяет, что каждое поле EditState либо явно есть в PRESET_KEYS, либо явно в списке геометрических/clip-specific исключений — чтобы забытое поле проваливало тест, а не тихо терялось.
685. 🟡 [проблема] Контракт округления crop между frontend sanitizeRect и backend clamp_rect_to_source не тестируется совместно - `frontend/src/store.test.ts` -> Добавить cross-стековый integration-тест (или задокументированный контрактный тест), который прогоняет одно и то же значение crop через buildEditPayload(), затем через clamp_rect_to_source и проверяет идемпотентность.
686. 🟡 [проблема] pollJob не имеет верхнего предела попыток/времени, и это не тестируется - `frontend/src/api.ts` -> Добавить frontend/src/api.test.ts тест, который мокает getJob всегда возвращающим running, и проверяет либо явный таймаут/лимит попыток в pollJob, либо документирует его отсутствие как осознанное поведение.
687. 🟡 [проблема] Нет теста на повторный finish_job/library.add при restart с уже существующей library-записью - `backend/tests/api.rs` -> Добавить в jobs_survive_restart шаг, вызывающий finish_job второй раз для уже терминальной 'done1' job после recover_jobs, и проверить отсутствие дублирующей library-записи.
688. 🟡 [проблема] cleanup_files_with_prefix не тестируется на префикс, совпадающий с UUID-подстрокой другого видео - `backend/src/handlers/mod.rs` -> Добавить тест с парой реалистичных UUID-подобных id, где один является строгим префиксом другого, и проверить, что cleanup для короткого id не удаляет файлы длинного.
689. 🟡 [проблема/SRP] make_state в api.rs строит ToolInfo вручную вместо переиспользования ToolInfo::default - `backend/tests/api.rs` -> Заменить литерал в make_state на ToolInfo { ffmpeg, ytdlp, ..Default::default() }, чтобы новые поля ToolInfo подхватывались автоматически.

<a id="module-ops"></a>
### Модуль: Config, build, Docker, CI, observability (46)

Config/build/Docker/CI/observability. Env разбросан по местам, отсутствие structured logging/metrics, Docker hardening, toolchain drift.

690. 🟠 [проблема/SRP] Конфигурация читается россыпью из env в 5+ файлах вместо единой точки загрузки - `backend/src/main.rs` -> Ввести единую структуру Config::from_env(), читаемую один раз в main.rs и передаваемую по цепочке вместо точечных std::env::var в модулях.
691. 🔴 [проблема] Оба Docker-образа запускают процесс от root без USER-директивы - `backend/Dockerfile` -> Добавить RUN useradd + USER в обоих Dockerfile перед CMD/финальным слоем.
692. 🟠 [проблема] Ни один Dockerfile не объявляет HEALTHCHECK - `backend/Dockerfile` -> Добавить HEALTHCHECK CMD curl -f http://localhost:8080/api/health в backend/Dockerfile и аналогичный для nginx в frontend/Dockerfile.
693. 🟠 [проблема] Базовые образы и Rust-тулчейн не запиннены — версии плавают независимо друг от друга - `backend/Dockerfile` -> Запиннить базовые образы по sha256-digest и зафиксировать конкретную версию Rust (например rust:1.80-bookworm) одинаково в Dockerfile и CI.
694. 🟡 [проблема] apt-get install ffmpeg без версии — рантайм ffmpeg не зафиксирован - `backend/Dockerfile` -> Указать ffmpeg=<version> явно или зафиксировать образ по digest, чтобы версия ffmpeg была воспроизводимой.
695. 🟡 [проблема] Docker build не кэширует cargo-зависимости — любое изменение исходников пересобирает все крейты заново - `backend/Dockerfile` -> Сначала COPY Cargo.toml/Cargo.lock и собрать зависимости отдельным слоем (cargo build с пустым src/main.rs-заглушкой), затем COPY остальных исходников.
696. 🟡 [проблема] CI не собирает и не проверяет сами Docker-образы - `.github/workflows/ci.yml` -> Добавить job docker в ci.yml с docker build ./backend и docker build ./frontend, либо docker compose build.
697. 🟡 [идея] В CI нет сканирования зависимостей на известные уязвимости - `.github/workflows/ci.yml` -> Добавить шаг cargo audit в backend job и npm audit --audit-level=high в frontend job.
698. ✅ [проблема] README требует Node.js 18+, но CI и Dockerfile используют Node 20 — версии не синхронизированы - `README.md` -> README поднят до Node.js 20+, добавлен `.nvmrc` со значением `20`.
699. ✅ [проблема] README не документирует CORS_ALLOW_ORIGINS и RECOVER_JOBS_LIMIT, хотя это реальные переменные окружения - `README.md` -> README теперь перечисляет `CORS_ALLOW_ORIGINS`, `RECOVER_JOBS_LIMIT` и фактические дефолты.
700. 🟠 [баг] BIND_ADDR парсится с молчаливым фоллбэком на 127.0.0.1 при невалидном значении - `backend/src/main.rs` -> Заменить unwrap_or_else на явный panic!/expect с сообщением о некорректном BIND_ADDR при ошибке парсинга.
701. 🟡 [проблема/DIP] main.rs напрямую создаёт пути sources/outputs вместо делегирования Library - `backend/src/main.rs` -> Вынести создание storage-подкаталогов в Library::load или AppState::new, чтобы main.rs не знал о конкретных именах поддиректорий.
702. 🟠 [проблема] MAX_CONCURRENT_JOBS, FILE_TTL_HOURS, MAX_UPLOAD_BYTES читаются через .ok().and_then(parse) без проверки на 0 или неразумные значения - `backend/src/main.rs` -> Добавить валидацию с понятным log::error!/panic при значениях вне разумного диапазона (например MAX_CONCURRENT_JOBS < 1).
703. 🟡 [улучшение] Логи пишутся в текстовом формате без структурированного JSON-вывода - `backend/src/main.rs` -> Добавить опциональный JSON-форматтер (tracing_subscriber::fmt().json()), включаемый через переменную окружения LOG_FORMAT=json.
704. 🟡 [проблема] Нет trace-идентификатора на уровне job — лог-строки одной задачи не сопоставить друг с другом - `backend/src/handlers/mod.rs` -> Обернуть обработку каждой задачи в tracing::info_span!("job", job_id = %id) при её запуске.
705. 🟡 [проблема] Нет метрик Prometheus/OpenMetrics для очереди задач и рендеров - `backend/src/main.rs` -> Подключить metrics-exporter-prometheus и отдавать /metrics с счётчиками активных/завершённых/упавших задач.
706. 🟠 [проблема] /api/health не проверяет реальное состояние SQLite или диска, только статичные флаги ffmpeg/ytdlp - `backend/src/handlers/health.rs` -> Добавить в health_handler лёгкий SELECT 1 к Db и проверку доступности STORAGE_DIR на запись, включив их в ответ.
707. 🟠 [баг] Доступность ffmpeg/yt-dlp проверяется один раз при старте и никогда не переоценивается - `backend/src/main.rs` -> Переоценивать доступность инструментов периодически (например раз в 5 минут) или прямо в health_handler с троттлингом.
708. 🟠 [проблема] docker-compose.yml не задаёт ресурсные лимиты (memory/cpu) ни для backend, ни для frontend - `docker-compose.yml` -> Добавить mem_limit/cpus (или deploy.resources.limits в compose v3) для обоих сервисов.
709. 🟠 [проблема] docker-compose не пробрасывает MAX_CONCURRENT_JOBS/JOB_TIMEOUT_SECS/MAX_UPLOAD_BYTES/CORS_ALLOW_ORIGINS — все лимиты жёстко на дефолтах контейнера - `docker-compose.yml` -> Добавить эти переменные в environment (или через env_file) с возможностью переопределения через .env при деплое.
710. 🟠 [проблема] CORS_ALLOW_ORIGINS не задан в docker-compose — дефолтные origin'ы для localhost не подходят за реальным доменом - `docker-compose.yml` -> Задать CORS_ALLOW_ORIGINS в docker-compose.yml (или через .env) под реальный домен деплоя.
711. 🟡 [проблема] nginx.conf не устанавливает заголовки безопасности (X-Content-Type-Options, X-Frame-Options, CSP) - `frontend/nginx.conf` -> Добавить add_header X-Content-Type-Options nosniff, X-Frame-Options DENY и базовый Content-Security-Policy в server-блок.
712. 🟡 [проблема] nginx проксирует /files/ на backend без кэширующих заголовков для статики - `frontend/nginx.conf` -> Добавить expires/Cache-Control с длинным TTL для проксируемых /files/ ответов, раз содержимое по id не меняется.
713. 🟡 [улучшение] nginx.conf слушает только IPv4 (listen 80) без явного IPv6-листенера - `frontend/nginx.conf` -> Добавить listen [::]:80; рядом с существующим listen 80;.
714. 🟡 [проблема] restart: unless-stopped без ограничения количества попыток — crash loop будет длиться бесконечно - `docker-compose.yml` -> Рассмотреть restart: on-failure:5 либо добавить внешний мониторинг перезапусков через HEALTHCHECK.
715. 🟡 [проблема] Frontend Dockerfile не поддерживает передачу VITE_*-переменных на этапе сборки - `frontend/Dockerfile` -> Добавить ARG VITE_API_BASE (и подобные) с ENV перед RUN npm run build, документировать в README.
716. 🟡 [проблема/KISS] Makefile дублирует cd backend && / cd frontend && в каждой строке вместо -C - `Makefile` -> Использовать $(MAKE) -C backend <target> / $(MAKE) -C frontend <target> либо объединить команды одного каталога в одну строку с &&.
717. 🟡 [проблема/DRY] Makefile-таргеты check и lint дублируют одинаковые clippy/eslint-команды вместо переиспользования lint - `Makefile` -> Сделать check: lint test build своими зависимостями в Makefile вместо копирования команд.
718. 🟡 [идея] CI не публикует и не кэширует собранные Docker-образы для последующего деплоя - `.github/workflows/ci.yml` -> Добавить отдельный job с docker/build-push-action, публикующий образ в GHCR при пуше в main.
719. 🟡 [проблема] CI не кэширует сборку Vite, только node_modules через cache: npm - `.github/workflows/ci.yml` -> Добавить actions/cache для node_modules/.vite с ключом по хэшу lock-файла и исходников.
720. 🟡 [проблема] Cargo.toml не задаёт профиль release — нет тюнинга LTO/codegen-units для рантайм-образа - `backend/Cargo.toml` -> Добавить [profile.release] с lto = true и codegen-units = 1 для более быстрого и компактного бинарника.
721. 🟡 [проблема] README не описывает поведение приложения при недоступности ffmpeg/yt-dlp после старта - `README.md` -> Добавить абзац в README о поведении при рантайм-сбоях внешних инструментов (задачи упадут с ошибкой, /api/health не обновится до перезапуска).
722. 🟡 [проблема] Makefile-таргет dev (scripts/dev.sh) не имеет соответствующего CI-джоба - `Makefile` -> Добавить shellcheck scripts/dev.sh как отдельный шаг в CI либо в make lint.
723. 🟡 [баг] FILE_TTL_HOURS из env умножается на 3600 без проверки переполнения u64 - `backend/src/main.rs` -> Использовать ttl_hours.saturating_mul(3600) или checked_mul с логированием ошибки вместо прямого умножения.
724. 🟠 [проблема] Путь скачивания через yt-dlp не покрыт ни одним тестом, и CI даже не устанавливает yt-dlp - `.github/workflows/ci.yml` -> Либо установить yt-dlp в CI и добавить интеграционный тест с фиктивным/локальным медиа-источником, либо явно задокументировать это ограничение покрытия.
725. 🟡 [проблема/SRP] MAX_HEIGHT читается из env внутри download_video при каждом вызове вместо однократного чтения при старте - `backend/src/tools/mod.rs` -> Перенести чтение MAX_HEIGHT в main.rs/Config и передавать значение параметром в download_video.
726. 🟡 [проблема] MAX_HEIGHT не покрыт unit-тестом, в отличие от аналогичного RECOVER_JOBS_LIMIT - `backend/src/tools/mod.rs` -> Вынести чтение MAX_HEIGHT в отдельную функцию с unit-тестом по образцу recover_jobs_limit().
727. ✅ [проблема] RUST_LOG не задокументирован в README, хотя реально читается через EnvFilter - `README.md` -> README теперь перечисляет `RUST_LOG` и дефолт `info,tower_http=info`.
728. 🟡 [улучшение/ISP] tokio подключён с features = ["full"] вместо реально используемого подмножества - `backend/Cargo.toml` -> Заменить "full" на явный список используемых фич (rt-multi-thread, macros, fs, process, signal, net, time, sync).
729. 🟡 [проблема/DRY] tower объявлен дважды в Cargo.toml — в [dependencies] и [dev-dependencies] с разными наборами фич - `backend/Cargo.toml` -> Объединить в одну запись tower в [dependencies] с нужными фичами, либо явно прокомментировать, почему нужно раздельное объявление.
730. 🟡 [проблема] docker-compose.yml не используется и не проверяется нигде в CI - `docker-compose.yml` -> Добавить шаг docker compose config -q (валидация синтаксиса) или полноценный docker compose build в CI.
731. 🟡 [проблема] depends_on в docker-compose без condition: service_healthy — frontend стартует раньше готовности backend - `docker-compose.yml` -> Добавить HEALTHCHECK в backend/Dockerfile и заменить depends_on на форму condition: service_healthy.
732. 🟡 [проблема/DIP] Порт backend жёстко продублирован в 3 местах без единого источника истины - `frontend/nginx.conf` -> Параметризовать порт через build-arg/env в nginx.conf (envsubst в entrypoint) и явно задавать PORT в docker-compose.yml backend-сервиса.
733. 🟡 [улучшение] nginx.conf не включает gzip для текстовых ассетов SPA - `frontend/nginx.conf` -> Добавить gzip on; gzip_types text/css application/javascript application/json; в server-блок.
734. 🟡 [проблема] CI не имеет concurrency-группы — параллельные пуши не отменяют устаревшие прогоны - `.github/workflows/ci.yml` -> Добавить concurrency: { group: ci-${{ github.ref }}, cancel-in-progress: true } на верхнем уровне ci.yml.
735. 🟡 [проблема] curl остаётся в финальном рантайм-образе backend после однократного использования для скачивания yt-dlp - `backend/Dockerfile` -> Скачать yt-dlp в отдельном build-стейдже с curl и скопировать только бинарник в финальный образ через COPY --from, не устанавливая curl в рантайм-слой.

<a id="module-domain"></a>
### Модуль: Доменная модель и архитектура целиком (cross-cutting SOLID и DRY) (48)

Доменная модель и архитектура целиком (cross-cutting SOLID/DRY). Тройная роль `EditRequest`, контракт правки в 6 местах, отсутствие Timeline-IR/OutputSpec.

736. 🔴 [проблема/SRP] EditRequest — это одновременно wire DTO, доменная модель и вход билдера ffmpeg - `backend/src/model.rs` -> Развести на EditRequestDto (serde-граница), EditPlan/EditState (валидированная доменная модель) и FfmpegSpec (вход билдера), с явным преобразованием между ними.
737. 🔴 [проблема/DRY] Контракт полей правки продублирован в шести местах без единого источника истины - `backend/src/model.rs` -> Сгенерировать TS-тип из Rust (например, ts-rs/specta) и вывести PRESET_KEYS/дефолты из единственного описания полей.
738. 🔴 [проблема/OCP] Новый эффект требует правок в 4+ несвязанных файлах backend - `backend/src/tools/args.rs` -> Ввести таблицу/реестр эффектов (имя, тип параметра, диапазон, генератор фильтра) как единый источник для валидации, сборки фильтров и UI.
739. 🟠 [проблема] edit_json в проектах хранится как непроверенный serde_json::Value - `backend/src/db.rs` -> Десериализовать edit в EditState/EditRequest перед записью в БД и отклонять невалидные значения на project_upsert_handler.
740. 🟠 [дизайн] На бэкенде нет доменного типа Project/EditState — только строка edit_json + Value на границе - `backend/src/db.rs` -> Ввести ProjectUpsertRequest { video_id, name, video: VideoInfo, edit: EditRequest } и десериализовать тело запроса в него целиком.
741. 🟠 [проблема/SRP] normalize_edit_request мутирует EditRequest на месте, смешивая валидацию, клэмпинг и парсинг ошибок - `backend/src/handlers/mod.rs` -> Разделить на чистую функцию validate(&EditRequest) -> Result<(), ValidationError> и normalize(&mut EditRequest) без сообщений об ошибках внутри валидации.
742. 🔴 [баг] render_cache_key считается до normalize_edit_request, что делает кэш нестабильным для эквивалентных запросов - `backend/src/handlers/mod.rs` -> Переместить вызов normalize_edit_request перед вычислением render_cache_key, до входа в spawn (после первого probe_video, если размеры нужны для клэмпа rect).
743. 🟠 [проблема/DRY] Форматно-специфичные наборы кодеков дублируются между push_video_codec и build_concat_args - `backend/src/tools/args.rs` -> Вынести единую функцию audio_codec_for_format(format) -> &str и переиспользовать её в push_audio-вызовах и build_concat_args.
744. 🟡 [улучшение/OCP] filter_preset и qualityTier/tierToCrf — строковые enum без типовой защиты на границе backend/frontend - `backend/src/tools/args.rs` -> Определить общий enum LookPreset с сериализацией serde(rename_all) на бэке и сгенерировать соответствующий TS union.
745. 🟠 [проблема] Неизвестное значение filter молча игнорируется вместо ошибки валидации - `backend/src/tools/args.rs` -> В normalize_edit_request проверять edit.filter по белому списку допустимых пресетов и возвращать anyhow::bail! при несовпадении.
746. 🟡 [проблема/SRP] buildEditPayload дублирует логику default-значений, уже описанную в defaultEdit - `frontend/src/store.ts` -> Сравнивать state.edit с результатом defaultEdit() программно (diff по ключам) вместо ручного перечисления условий.
747. 🟡 [проблема/DRY] tierToCrf дублирует пороги качества, независимо заданные как unwrap_or в push_video_codec - `frontend/src/store.ts` -> Переносить дефолтный CRF только на бэкенд и убрать qualityTier/tierToCrf с фронта, либо наоборот сделать таблицу серверной и отдавать её через /api.
748. 🟠 [проблема/ISP] EditPanel.vue читает и пишет весь глобальный state.edit напрямую, а не через props/emit - `frontend/src/components/EditPanel.vue` -> Ввести props для конкретных секций (trim, crop, effects) и emit('update:...') вместо прямого чтения/записи в общий reactive state.
749. 🟠 [проблема/DIP] store.ts напрямую знает о localStorage и HTTP/SQLite-эндпоинтах вместо абстракции хранилища - `frontend/src/store.ts` -> Выделить интерфейс PresetStore/ProjectStore с реализациями поверх localStorage и HTTP, инжектируемыми в store.ts.
750. 🟡 [идея] Нет Timeline IR — EditRequest моделирует одну операцию на весь клип, а не последовательность операций - `backend/src/model.rs` -> Спроектировать Timeline { clips: Vec<ClipRef>, ops: Vec<Operation> } как отдельный IR поверх текущего EditRequest для одноклипового MVP.
751. 🟡 [идея] Нет OutputSpec, отделённого от EditRequest — формат/кодек/качество перемешаны с эффектами обработки - `backend/src/model.rs` -> Вынести format/codec/quality в отдельный OutputSpec, передаваемый вместе с EditRequest, но независимо валидируемый и кэшируемый.
752. 🟡 [проблема] Нет domain events / audit trail для отредактированных клипов - `backend/src/db.rs` -> При желании версионирования — добавить таблицу project_history с append-only записями вместо UPDATE по video_id.
753. 🟡 [улучшение/KISS] capturePreset использует небезопасный `as unknown as Record<string, unknown>` вместо типобезопасного маппинга - `frontend/src/store.ts` -> Заменить на `(Object.keys(state.edit) as (keyof EditState)[])` без unknown-каста, либо явный switch по ключам с корректной типизацией.
754. 🟡 [проблема] PRESET_KEYS не включает поля geometry (crop/censor/scale/trim/cut), но критерий 'reusable look' не проверяется тестами - `frontend/src/store.ts` -> Либо исключить censorColor из PRESET_KEYS вместе с остальной geometry, либо добавить тест, фиксирующий намеренный список полей пресета.
755. 🟡 [проблема/SRP] JobStatus::from_token имеет неявный fallback на Pending, скрывающий реальные ошибки БД - `backend/src/model.rs` -> Вернуть Result<JobStatus, String> или залогировать tracing::warn при непойманном значении вместо тихого fallback.
756. 🟡 [проблема/OCP] Job.stage — свободная строка без enum, допустимые значения перечислены только в комментарии - `backend/src/model.rs` -> Заменить на enum JobStage { Queued, Downloading, Processing } с serde(rename_all = "lowercase").
757. 🟡 [проблема/DRY] censorColor whitelist (sanitize_color) не синхронизирован с UI-опциями цвета на фронте - `backend/src/tools/args.rs` -> Экспортировать список допустимых цветов с бэкенда (например, через /api/health или отдельный constants-эндпоинт) и генерировать из него select-опции на фронте.
758. 🟠 [проблема/LSP] Trim/Crop/Scale не гарантируют инвариант end > start / w,h > 0 на уровне типа - `backend/src/model.rs` -> Ввести smart-constructor (TryFrom) для Trim/Crop/Scale, возвращающий Result при нарушении инварианта прямо на этапе десериализации.
759. 🟡 [идея] Нет endpoint/типа для получения списка допустимых значений effect enum'ов клиентом - `backend/src/tools/args.rs` -> Добавить /api/edit-options, отдающий JSON с допустимыми пресетами/цветами/форматами, и генерировать UI-опции из него.
760. 🟠 [проблема] codec='h265' проверяется строковым сравнением в двух независимых функциях - `backend/src/tools/args.rs` -> Валидировать codec по белому списку ['h264','h265'] в normalize_edit_request и завести Codec enum вместо Option<String>.
761. 🟡 [дизайн] qualityTier на фронте не имеет прямого отражения в EditRequest — переводится в quality: CRF асимметрично формату - `frontend/src/types.ts` -> Либо хранить qualityTier прямо в EditRequest как typed enum и переводить в CRF только на бэкенде, либо восстанавливать tier обратным поиском по CRF при загрузке проекта.
762. 🟡 [баг] quality: Option<u32> не валидируется в normalize_edit_request вообще - `backend/src/handlers/mod.rs` -> Добавить clamp по разумному диапазону CRF (например 0..=51) в normalize_edit_request перед использованием quality.
763. 🟡 [проблема/DRY] format_secs — тривиальная однострочная обёртка, дублирующая format! напрямую использованный в другом месте - `backend/src/tools/args.rs` -> Использовать format_secs везде, где форматируется время в секундах внутри этого файла, либо удалить обёртку и оставить прямой format!.
764. 🟡 [идея] Нет versioning/schema-migration стратегии для EditState, персистентно хранимого в localStorage и SQLite - `backend/src/db.rs` -> Либо реализовать фактическую миграцию по schema_version при чтении старых edit_json, либо убрать неиспользуемую колонку из схемы, чтобы не создавать ложное ощущение защиты.
765. 🟡 [проблема] EditRequest не Clone, что вынуждает handlers/mod.rs использовать `let mut req = req;` shadow вместо req.clone() - `backend/src/handlers/mod.rs` -> Добавить #[derive(Clone)] к EditRequest заранее, если планируется логировать/кэшировать сырой запрос отдельно от нормализованного.
766. 🟠 [баг] video_filters строит drawbox/crop по исходным координатам, но segments-путь применяет их уже после конкатенации нескольких кусков - `backend/src/tools/args.rs` -> Задокументировать явно, что censor/crop-координаты валидны только пока все сегменты берутся из одного source с постоянным разрешением.
767. 🟠 [баг] reverse при segments переворачивает уже склеенный ролик целиком, а не порядок и содержимое каждого сегмента отдельно - `backend/src/tools/args.rs` -> Задокументировать точную семантику reverse+segments в комментарии к video_filters либо запретить их комбинацию в normalize_edit_request.
768. 🟡 [проблема/DRY] expected_output_secs пересчитывается дважды на разных стадиях с разным клэмпом speed - `backend/src/tools/args.rs` -> Сделать expected_output_secs приватной деталью build_ffmpeg_args и возвращать (Vec<String>, f64) одним вызовом, чтобы длительность не пересчитывалась внешним кодом отдельно.
769. 🟡 [проблема/DRY] MediaEntry::from_result парсит JSON вручную теми же ключами, что json!({...}) в handlers/mod.rs, без общего типа-источника - `backend/src/library.rs` -> Ввести общий struct ResultInfo/SourceInfo с Serialize и строить его напрямую вместо json!({...}), передавая typed-значение в MediaEntry::from_result.
770. 🟡 [проблема/SRP] clamp_rect_to_source не обеспечивает чётность w/h в общем случае, только для источников с обеими сторонами >= 2 - `backend/src/handlers/mod.rs` -> Централизовать форсирование чётности в одном месте (либо только normalize_edit_request, либо только video_filters), не дублируя в обоих.
771. 🟡 [проблема/DRY] Job.stage строковые литералы 'queued'/'downloading'/'processing' захардкожены в handlers/mod.rs без общего источника - `backend/src/handlers/mod.rs` -> Ввести JobStage enum (см. отдельный пункт про Job.stage) и заменить строковые литералы на его варианты.
772. 🟡 [проблема/OCP] output_ext и push_video_codec независимо перечисляют один и тот же список форматов - `backend/src/tools/args.rs` -> Определить единый Format enum с методом .extension() и .video_codec_kind(), чтобы оба свойства выводились из одного описания формата.
773. 🟠 [проблема/SRP] project_upsert_handler валидирует форму project JSON вручную через Value-индексацию вместо десериализации в типизированный DTO - `backend/src/handlers/projects.rs` -> Определить `#[derive(Deserialize)] struct ProjectUpsertRequest { video_id: String, video: Value, edit: Value, name: Option<String> }` и заменить Json<Value> на Json<ProjectUpsertRequest>.
774. 🟡 [проблема/SRP] ensure_project_json_size сериализует video/edit ещё раз только для проверки размера, а upsert_project сериализует их снова - `backend/src/handlers/projects.rs` -> Сериализовать video/edit один раз в handler, передать готовые строки в Db::upsert_project (изменив сигнатуру на &str) и проверять их len() напрямую.
775. 🟠 [проблема/SRP] Db::upsert_project делает SELECT id, затем UPDATE/INSERT, затем ещё раз SELECT * без единой транзакции - `backend/src/db.rs` -> Обернуть три запроса в одну транзакцию (pool.begin()) или использовать один INSERT ... ON CONFLICT(video_id) DO UPDATE ... RETURNING *.
776. 🟡 [проблема/DRY] row_to_job и row_to_project — почти идентичный шаблон ручного маппинга SqliteRow -> struct, повторённый для каждой таблицы - `backend/src/db.rs` -> Вынести sqlx::FromRow (derive или ручной impl) для Job/Project вместо отдельных функций row_to_*, либо helper `fn json_column<T>(row, name) -> Result<T>`.
777. 🟡 [проблема/ISP] EditRequest сериализуется в render_cache_key со всеми полями включая video_id, что делает кэш непереносимым между источниками с идентичным edit - `backend/src/handlers/mod.rs` -> Если нужен переносимый кэш, хэшировать video_id отдельно от содержимого edit и учитывать это явно в схеме ключа; иначе явно задокументировать, что кэш всегда per-source.
778. 🟡 [проблема/SRP] watch(() => [state.video, state.edit]) в автосейве триггерится на любое изменение video, включая переключение на null при deleteFromLibrary - `frontend/src/store.ts` -> Разделить на два отдельных watch: один для смены клипа (сброс/восстановление проекта), другой только для state.edit (debounced autosave).
779. 🟡 [проблема/DRY] MediaEntry (frontend types.ts) не отражает поля vcodec/acodec/fps, которые есть в VideoInfo, из-за чего openFromLibrary теряет эти данные при реоткрытии клипа - `frontend/src/types.ts` -> Добавить vcodec/acodec/fps в MediaEntry и в MediaEntry::from_result (library.rs:33-46), либо запрашивать полный VideoInfo отдельным эндпоинтом при открытии из библиотеки.
780. 🟡 [улучшение/OCP] video_filters добавляет каждый новый эффект через последовательный if-блок, порядок эффектов задан императивно и не выражен декларативно - `backend/src/tools/args.rs` -> Вынести список эффектов как Vec<(EffectKind, impl Fn(&EditRequest) -> Option<String>)> в фиксированном порядке, чтобы порядок был декларативным и виден без чтения всего тела функции.
781. 🟡 [баг] output_ext(Some("av1")) возвращает mp4, но non-concat путь build_ffmpeg_args не проверяет совпадение с output_ext при формировании output_path - `backend/src/handlers/mod.rs` -> Сделать build_ffmpeg_args принимать уже вычисленный output_ext как параметр вместо того, чтобы оба места independently решали расширение по строке format.
782. 🟡 [улучшение/DIP] state.rs напрямую использует std::collections::HashMap с ручной блокировкой Mutex вместо инкапсуляции job-хранилища за отдельным типом - `backend/src/state.rs` -> Вынести JobStore { jobs: Mutex<HashMap<...>>, cancels: Mutex<HashMap<...>> } в отдельный тип со своим API, инжектируемый в AppState.
783. 🟠 [баг] render_locks в AppState растёт неограниченно — записи никогда не удаляются после завершения рендера - `backend/src/state.rs` -> После освобождения _render_guard в edit_handler удалять запись из render_locks (например, через weak-reference или periodic sweep неиспользуемых Arc с strong_count == 1).

## Исследовательский слой: 100 репозиториев (14 июля 2026)

100 активных проектов с высоким рейтингом изучены по первичным репозиториям,
архитектурным материалам, papers и стандартам. Полная выборка, точный снимок
звёзд, методика и источники находятся в
[docs/research-100.md](docs/research-100.md). Ниже не список зависимостей, а 100
отдельных решений №784-883: каждое добавляет отсутствующий контракт, критерий
приёмки или измеримый эксперимент и не меняет исторические 565 пунктов
SOLID/DRY-раунда. Общий синхронизированный набор `architecture.md` и
`recommendation.md` после этого слоя - 665 пунктов.

### A. NLE и media pipeline (784-793)

784. 🟠 [архитектура/FFmpeg] `Timeline` описан как цель, но у компилятора нет канонического типизированного media DAG - `backend/src/domain/filter_graph.rs` (target) -> Ввести `FilterGraph<Node, Pad, Edge>` с типами audio/video, topological validation, стабильной сериализацией и DOT/snapshot output; невалидная связь должна падать до запуска ffmpeg.
785. 🟠 [архитектура/GStreamer] Preview управляется набором DOM/store-команд без явной модели жизненного цикла и media clock - `frontend/src/features/player/previewSession.ts` (target) -> Ввести состояния `Idle/Ready/Paused/Playing/Draining/Failed`, монотонный clock и таблицу допустимых переходов; race `seek/load/play` покрыть fake-time тестами.
786. 🟡 [DIP/MLT] Целевой pipeline всё ещё описан через конкретный ffmpeg compiler, а роли источника, эффекта, перехода и потребителя не являются портами - `backend/src/domain/media_pipeline.rs` (target) -> Определить узкие `Producer/Filter/Transition/Consumer` contracts и registry адаптеров; домен не импортирует CLI/process типы.
787. 🟠 [домен/Olive] Будущий timeline не фиксирует идентичность клипов и операций, поэтому reorder/undo/migration могут ломать ссылки - `backend/src/domain/timeline.rs` (target) -> Добавить стабильные `ClipId`/`OperationId` и immutable operation graph; тесты доказывают сохранение ссылок после reorder, undo и serialize/deserialize.
788. 🟡 [совместимость/OpenShot] Пункт о schema versioning не задаёт проверяемую политику эволюции проектов - `backend/tests/fixtures/projects/` (target) -> Хранить golden fixture каждой версии, мигрировать в latest и делать reopen/re-save test; отдельно зафиксировать reject/preserve policy для неизвестных операций.
789. 🟠 [perf/UX/Kdenlive] Для тяжёлых исходников нет proxy-media workflow - `backend/src/analysis/proxy.rs` (target) -> Сделать proxy производным артефактом по checksum источника с фоновой генерацией, relink и прозрачной заменой на full-resolution при export; удаление proxy не должно затрагивать проект.
790. 🟠 [контракт/Shotcut] Статический список форматов не отражает реальные версии и возможности установленного ffmpeg - `backend/src/capabilities.rs` (target) -> Генерировать runtime manifest codecs/containers/filters/hardware с tool fingerprint и reason для unavailable; frontend показывает disabled-state, а не молча скрывает опцию.
791. 🟡 [perf/Blender] Инвалидация render/probe/analysis cache задана отдельно для каждого хранилища - `backend/src/domain/artifact_graph.rs` (target) -> Ввести dependency graph производных артефактов и fingerprint входов; изменение edit invalidates только downstream nodes, что проверяется матрицей операций.
792. 🟠 [SRP/OBS] Preview и export используют общую модель, но их разные latency/quality/resource policy формально не разделены - `backend/src/services/preview.rs`, `render.rs` (target) -> Оставить общий `EditPlan`, но завести разные execution profiles; тест запрещает preview-настройкам менять финальный output spec.
793. 🟡 [масштабирование/Remotion] Рендер предполагается одним процессом и не имеет frame-level детерминизма - `backend/src/render/frame_renderer.rs` (target) -> Определить `render(frame_no, plan_hash, source_hash)` как детерминированный контракт, chunk manifest с checksum и idempotent retry; stitch стартует только при полном проверенном наборе кадров.

### B. Кодеки, качество и packaging (794-803)

794. 🟡 [качество/VMAF] CRF считается достаточным сигналом качества результата - `backend/src/analysis/quality.rs` (target) -> Добавить opt-in отчёт VMAF + PSNR/SSIM с явной model/viewing-condition версией и per-scene значениями; до калибровки отчёт advisory и не блокирует export.
795. 🟡 [perf/Av1an] Длинный AV1 export нельзя продолжить с готовых сцен после сбоя - `backend/src/render/chunks.rs` (target) -> Делить по подтверждённым scene boundaries, атомарно сохранять manifest/segment checksums и перезапускать только отсутствующие chunks; итоговый mux проверяет совместимость параметров.
796. 🟠 [ресурсы/rav1e] Настройки encoder threads/tiles/speed/memory не сведены в один бюджет - `backend/src/config/encode_budget.rs` (target) -> Ввести валидируемый `EncodeBudget`, связать его с `Config` и cgroup limits, затем зафиксировать benchmark matrix latency/RAM/quality для low/default/high profiles.
797. 🟡 [домен/Opus] Audio export описывается разрозненными строками codec/bitrate/container - `backend/src/domain/audio_output.rs` (target) -> Ввести `AudioOutputSpec` с bitrate mode, channels, sample rate и loudness policy; contract tests отклоняют несовместимые container/codec combinations.
798. 🟡 [OCP/Shaka Packager] Encoding и streaming packaging пока не имеют отдельной границы - `backend/src/packaging/mod.rs` (target) -> Описать `OutputBundle { manifest, segments, init, retention }` и HLS/DASH adapters после encode; обычный file export не зависит от packaging-модуля.
799. 🟠 [корректность/libavif] Still export не проверяет сохранность color primaries/transfer/matrix, ICC, alpha и orientation - `backend/tests/media/still_metadata.rs` (target) -> Добавить round-trip fixtures и `ffprobe`-assertions; metadata-loss возвращает предупреждение или ошибку согласно выбранному profile.
800. 🟡 [OCP/libjxl] Добавление нового still-кодека потребует менять домен и UI одновременно - `backend/src/ports/still_encoder.rs` (target) -> Ввести `StillImageEncoder` + capability descriptor; JPEG XL или другой адаптер подключается без изменения `EditPlan`, а UI строится из manifest №790.
801. 🟠 [валидация/libheif] Container brand, image item и codec смешиваются в одной строке format - `backend/src/domain/still_container.rs` (target) -> Типизировать эти три уровня отдельно и отклонять unsupported HEIF/AVIF combinations синхронно, до process spawn.
802. 🟡 [идея/SRT] Remote/live ingest при добавлении протокола рискует проникнуть в editor core - `backend/src/ingest/srt.rs` (target) -> Оформить внешний ingest adapter с reconnect/latency/clock budgets, который выдаёт обычный immutable source artifact; домен редактора не знает сетевой протокол.
803. 🟠 [контракт/PyAV] Probe metadata остаётся частично сырым JSON и теряет time-base/container semantics - `backend/src/domain/media_probe.rs` (target) -> Нормализовать streams, dispositions, frame rate, time base, color и rotation в `ProbeResult`; ffprobe/PyAV-подобные реализации проходят один contract corpus.

### C. Playback и streaming (804-813)

804. 🟠 [DIP/Video.js] Store зависит от `HTMLVideoElement`, из-за чего playback нельзя тестировать или менять независимо - `frontend/src/ports/player.ts` (target) -> Ввести `PlayerAdapter` для source/play/pause/seek/buffered/error events и browser implementation; domain stores не импортируют DOM types.
805. 🟠 [UX/hls.js] Playback failures не имеют recovery taxonomy - `frontend/src/features/player/errors.ts` (target) -> Разделить network/media/config/unsupported и fatal/recoverable, задать bounded retry budget и конкретное действие UI; бесконечный recovery запрещён тестом.
806. 🟡 [контракт/Shaka Player] Неподдерживаемый codec/container/key-system может выглядеть как пустой player - `frontend/src/features/player/capabilities.ts` (target) -> Возвращать typed unsupported result с причиной и fallback; даже при отсутствии DRM capability branch остаётся явным.
807. 🟡 [perf/dash.js] Streaming preview может выбирать качество по тем же целям, что final export - `frontend/src/features/player/representationPolicy.ts` (target) -> Отдельная политика ограничивает resolution/buffer для быстрого seek и никогда не меняет `OutputSpec`; сценарии slow-network покрыты тестами.
808. 🟠 [a11y/Plyr] Доступность media controls не оформлена как единый контракт - `frontend/src/ui/media-controls/` (target) -> Зафиксировать keyboard map, focus order, labels и time `aria-valuetext` по WAI-ARIA; проверить screen reader semantics и touch target на desktop/mobile.
809. 🟡 [OCP/MediaElement] Local file, progressive HTTP и будущие HLS/DASH sources создадут разные player branches - `frontend/src/adapters/player/` (target) -> Нормализовать source adapters в единый event model; один playback test suite запускается для каждого поддерживаемого source kind.
810. 🟡 [UI/Media Chrome] `VideoPreview` рискует снова стать большим компонентом при кастомных controls - `frontend/src/ui/media-controls/` (target) -> Разделить play, seek, volume, time, fullscreen на headless primitives, состояние получать из media events; ни один primitive не читает global store.
811. 🟡 [архитектура/MediaMTX] Live protocols нельзя обслуживать внутри основного backend без роста attack/resource surface - `services/ingest-gateway` (future boundary) -> Gateway аутентифицирует и завершает запись, а editor получает immutable artifact по узкому API; общий process pool не используется.
812. 🟠 [ресурсы/SRS] Live ingest без load shedding способен вытеснить интерактивные export/probe задачи - `backend/src/config/resource_classes.rs` (target) -> Развести quotas/pools по классам ingest/analysis/export, ввести admission control и тест насыщения, где export сохраняет заданный p95.
813. 🟡 [масштаб/Jellyfin] Library list синхронно зависит от уже готового metadata и не имеет incremental index contract - `backend/src/services/media_indexer.rs` (target) -> Добавить cursor scan, изоляцию повреждённых entries и eventual search index; pagination не блокируется одним плохим файлом.

### D. Editor interactions и canvas (814-823)

814. 🟠 [домен/Excalidraw] История хранит JSON snapshots всего `EditState`, что плохо масштабируется и скрывает семантику изменений - `frontend/src/domain/history.ts` (target) -> Команды получают `apply/invert/merge` и transaction boundary; тесты проверяют coalescing drag и точный undo без сериализации чужого состояния.
815. 🟠 [UI/tldraw] Selection/crop/censor/pan/trim конкурируют за pointer events без общей tool state machine - `frontend/src/features/canvas/toolMachine.ts` (target) -> Ввести конечные состояния и один pointer-capture lifecycle; unmount/cancel/lost-capture завершают gesture идемпотентно.
816. 🟡 [дизайн/Penpot] CSS tokens существуют как значения, но не как versioned public contract компонентов - `frontend/src/ui/tokens.css`, `tokens.test.ts` (target) -> Зафиксировать semantic color/space/type/motion tokens, states и contrast assertions; feature CSS не использует raw palette values.
817. 🟠 [корректность/Fabric.js] Preview pixels, source pixels, normalized rect и export pixels не различаются типами - `frontend/src/domain/geometry.ts` (target) -> Добавить branded coordinate spaces и `Transform2D`; property tests проверяют round-trip, resize, rotation и letterbox mapping.
818. 🟡 [UI/Konva] Media, guides, overlays и handles находятся в одной event/render плоскости - `frontend/src/features/canvas/scene.ts` (target) -> Разделить scene layers, hit testing оставить только интерактивному слою; visual layer не может перехватывать pointer.
819. 🟡 [perf/PixiJS] Переход на GPU для waveform/overlays пока не имеет порога окупаемости - `frontend/bench/canvas/` (target) -> Сравнить DOM/Canvas2D/WebGL на длинной timeline и low-end device; WebGL вводится только при заранее заданном выигрыше и с fallback/context-loss test.
820. 🟠 [домен/Paper.js] Геометрические clamp/intersection/bounds распределены между Rust и Vue - `shared geometry corpus`, `frontend/src/domain/geometry.ts` (target) -> Создать immutable `Rect/Point/Transform` kernel и общий fixture corpus для Rust/TS; fuzz проверяет containment и отсутствие NaN.
821. 🟡 [OCP/TUI Image Editor] Каждый новый инструмент потребует вручную менять panel, shortcuts, availability и serialization - `frontend/src/features/tools/registry.ts` (target) -> Ввести декларативный descriptor `{ id, icon, shortcut, capability, editor, serializer }`; registry валидирует уникальность ID/shortcut.
822. 🟡 [diagnostics/xyflow] Media DAG трудно объяснить без чтения compiler output - `frontend/src/dev/renderGraph/` (target) -> Добавить только в dev-сборку visualizer nodes/edges/cost/cache-hit/validation error; production bundle не содержит этот feature.
823. 🟡 [домен/Motionity] Для анимации нет независимой модели keyframes/easing/interpolation - `backend/src/domain/keyframes.rs` (target) -> Ввести `KeyframeTrack<T>` с deterministic sampling и отдельными preview/ffmpeg adapters; значения между keyframes покрыть golden corpus.

### E. Rust backend (824-833)

824. 🔴 [надёжность/Tokio] Shutdown не владеет всеми spawned tasks и child processes как одной структурой - `backend/src/runtime/task_supervisor.rs` (target) -> Root `CancellationToken` + `TaskTracker`: закрыть intake, notify, bounded wait, затем escalation для process groups; integration test не оставляет task/child после shutdown.
825. 🟠 [DIP/Axum] Router tests всё ещё требуют конкретный `AppState` с БД/filesystem - `backend/src/http/mod.rs` (target) -> Хендлеры зависят от узких service ports, а contract tests поднимают `Router` с in-memory fakes; HTTP DTO/status остаются одинаковыми.
826. 🟠 [архитектура/Tower] Request ID, body limit, auth, rate limit, timeout и tracing рискуют подключаться в разном порядке по маршрутам - `backend/src/http/policy.rs` (target) -> Описать route classes и один ordered middleware stack; snapshot test фиксирует порядок и исключения для health/files.
827. 🟡 [проектирование/Actix Web] Смена HTTP framework может быть предложена без доказанного bottleneck - `backend/benches/http_baseline.rs` (target) -> Зафиксировать Axum throughput/p50/p95/p99/RSS для upload, polling и range response; framework rewrite допустим только после профиля и ADR.
828. 🟠 [perf/Hyper] Нет теста bounded memory при очень медленном upload/download клиенте и disconnect - `backend/tests/http_backpressure.rs` (target) -> Стримить тело с контролируемой скоростью, оборвать соединение и доказать bounded buffering, отмену reader/task и удаление staging.
829. 🟠 [контракт/Serde] Одинаковая permissive policy может случайно примениться к wire DTO и старым persisted documents - `backend/src/http/dto.rs`, `persistence/schema.rs` (target) -> Wire types strict + versioned, storage types migration-tolerant; negative corpus фиксирует неизвестные поля для обеих границ.
830. 🟡 [quality/SQLx] Query/schema drift обнаруживается только при выполнении теста с конкретной БД - `.github/workflows/ci.yml` (target) -> Добавить offline metadata и `cargo sqlx prepare --check`; migration + query change без обновления metadata проваливает CI.
831. 🟠 [observability/tracing] Отдельные spans не образуют стабильный trace contract и могут утечь paths/URLs - `backend/src/telemetry/schema.rs` (target) -> Зафиксировать дерево `request -> job -> process`, allowlist полей и redaction wrapper; golden JSON-log test содержит canary secret и доказывает его отсутствие.
832. 🟠 [perf/Rayon] Будущие thumbnail/waveform/hash вычисления могут блокировать Tokio workers - `backend/src/runtime/cpu_pool.rs` (target) -> Выделить bounded CPU executor с queue budget/cancellation и метриками saturation; async runtime thread не выполняет CPU-heavy closure.
833. 🟠 [security/rustls] Не определено, где завершается TLS и каким proxy headers доверять - `docs/deployment-security.md` (target) -> Зафиксировать один из профилей: trusted reverse proxy + private bind/allowlist headers либо direct rustls; public plain HTTP profile запрещён readiness check.

### F. Jobs и persistence (834-843)

834. 🟠 [надёжность/Temporal] Текущая строка Job не позволяет детерминированно воспроизвести переходы после crash - `backend/src/jobs/event_log.rs` (target) -> Append-only события + idempotency key и reducer в состояние; fault-injection после каждой границы даёт тот же terminal result после replay.
835. 🟠 [домен/Airflow] Повторный запуск смешивается с сущностью Job и общей строкой ошибки - `backend/src/jobs/attempt.rs` (target) -> Разделить `Job`/`JobAttempt`, execution timeout и retry policy по `ErrorKind`; validation/security errors никогда не retry.
836. 🟠 [ops/Celery] Failed jobs нельзя просмотреть и осознанно retry/discard - `backend/src/jobs/failed_registry.rs` (target) -> Хранить reason/attempt/next_retry/tool version, добавить operator API с audit trail; retry имеет cap и новую attempt запись.
837. 🟠 [API/BullMQ] Дубли import/edit при повторе HTTP создают независимую работу - `backend/src/jobs/dedupe.rs` (target) -> Stable idempotency/dedupe key, TTL и queue rate limiter; duplicate request возвращает существующий job ID и не запускает второй process.
838. 🟡 [ops/RQ] Lifecycle views собираются из одного списка без явных registries/reconciliation - `backend/src/jobs/registry.rs` (target) -> Определить queued/started/deferred/failed/finished queries и startup reconciliation; зависшая started attempt получает объяснимый interrupted state.
839. 🔴 [целостность/River] Enqueue в памяти и запись durable state могут разойтись при crash - `backend/src/jobs/outbox.rs` (target) -> Записывать job + outbox в одной SQLite transaction, dispatcher подтверждает delivery идемпотентно; crash tests исключают job без durable row и row без eventual execution.
840. 🟠 [ops/Restic] Backup остаётся инструкцией без проверяемого артефакта и restore drill - `backend/src/bin/backup.rs` (target) -> Content-addressed snapshot DB+media, manifest/checksums/tool version и `verify`; CI fixture делает backup, удаление и полный restore.
841. 🟡 [perf/Borg] Неизвестно, окупится ли chunk dedup на похожих source/output - `bench/backup-dedup.md` (target) -> Измерить storage/time на реальном corpus и задать prune policy; внешняя dedup dependency вводится только при зафиксированном выигрыше.
842. 🟡 [проектирование/RocksDB] Замена SQLite может преждевременно увеличить сложность - `backend/benches/persistence.rs` (target) -> Сначала benchmark SQLite WAL при заданных N assets/jobs/cache writes и определить migration threshold; до него RocksDB явно не рассматривается.
843. 🟡 [DIP/Meilisearch] Добавление поиска может напрямую связать library с отдельным сервером - `backend/src/ports/media_search.rs` (target) -> `MediaSearch` port с SQLite FTS default и optional external adapter после scale threshold; indexing eventual и rebuildable из source of truth.

### G. Vue, frontend и testing (844-853)

844. 🟠 [модульность/Vue] Feature boundaries описаны в документах, но imports их не защищают - `frontend/eslint.config.*` (target) -> Определить public API каждого feature и forbidden cross-feature imports; dependency rule падает в lint при обходе facade.
845. 🟠 [SRP/Pinia] Разделение store остаётся намерением без контракта взаимодействия - `frontend/src/stores/` (target) -> Pilot `project` и `ui` stores с compatibility facade; каждый store зависит только от domain/api ports, cross-store действие идёт через команду, а не mutable import.
846. 🟡 [perf/Vite] Нет бюджета initial JS/CSS и причины для lazy boundaries - `frontend/vite.config.ts`, CI (target) -> Генерировать bundle report и падать при превышении согласованного gzip budget; тяжёлые analysis/dev features загружаются отдельно.
847. 🟠 [тесты/Vitest] Polling/autosave/history/retry тестируются реальным временем или отдельными примерами - `frontend/src/**/*.test.ts` (target) -> Fake timers + table/property cases для backoff, deadline, debounce, undo merge и cancellation; suite не содержит sleep.
848. 🟡 [DRY/VueUse] Lifecycle-sensitive listeners/resize/online logic легко снова разойдутся по компонентам - `frontend/src/composables/` (target) -> Использовать общие composables с automatic cleanup и lint-аудит: прямой global `addEventListener` разрешён только внутри lifecycle wrapper.
849. 🟠 [дизайн/Storybook] Состояния компонентов проверяются только внутри целого приложения - `frontend/src/**/*.stories.ts` (target) -> Каталог empty/loading/error/long text/localization/mobile/reduced-motion для каждого tool surface; a11y и screenshot checks запускаются изолированно.
850. 🔴 [smoke/Playwright] Нет гарантии, что shell редактора открывается без backend и корректно объясняет offline - `frontend/e2e/smoke.spec.ts` (target) -> Mock API contract, Chromium/Firefox/WebKit + 390px; проверить boot, offline state, import/edit/export happy path и отсутствие overflow.
851. 🟡 [проектирование/Cypress] Подключение второго E2E runner удвоит fixtures и ожидания - `docs/adr/e2e-runner.md` (target) -> Сравнить network-fault/debug/CI speed на одном сценарии и выбрать один runner; Playwright и Cypress одновременно не поддерживать.
852. 🟠 [архитектура/TanStack Query] Server state library/projects/jobs смешан с mutable UI/edit state - `frontend/src/data/` (target) -> Выделить cache/invalidation/poll ownership за query-port; domain UI stores хранят только selection/draft, не копии API entities.
853. 🟠 [UX/Floating UI] Tooltip/menu/popover рискуют по-разному решать collision, focus, Escape и outside click - `frontend/src/ui/overlay/` (target) -> Один accessible overlay primitive с focus return и visual-viewport tests; unfamiliar icon всегда получает tooltip через него.

### H. Security и supply chain (854-863)

854. 🔴 [security/OWASP] Upload защищён probe/allowlist, но нет одной threat matrix, проверяющей всю цепочку - `docs/threat-model-upload.md`, `backend/tests/upload_security.rs` (target) -> Extension + declared MIME + signature/probe + generated name + quarantine + storage boundary + size/count limits; каждый control связан с negative fixture.
855. 🟠 [security/OSS-Fuzz] Parser boundary не получает непрерывного fuzzing вне обычного CI - `fuzz/oss-fuzz/` (target) -> Подготовить hermetic targets/corpus, sanitizer build и triage SLA; найденный crash автоматически становится regression fixture.
856. 🟠 [security/cargo-fuzz] Локальный fuzz охватывает только намеченную geometry-функцию - `backend/fuzz/` (target) -> Targets для edit normalization, multipart filename/path, library JSON, URL policy и cache key; corpus versioned, panic/OOM/time budget являются failure.
857. 🟠 [supply-chain/RustSec] `cargo audit` без policy приведёт к вечным ignore - `.cargo/audit.toml` (target) -> Каждое исключение содержит owner, rationale и expiry; просроченный advisory exception проваливает CI.
858. 🟠 [supply-chain/cargo-deny] Нет формальной политики лицензий, git sources и duplicate crates - `deny.toml` (target) -> Allow/deny license list, запрет неизвестных sources и budget дублей; исключения ревьюятся поштучно.
859. 🟠 [security/Trivy] Dependency audit не видит runtime image, OS packages и Compose/IaC - `.github/workflows/security.yml` (target) -> Сканировать built image/filesystem/config, публиковать SARIF; severity exception также имеет owner/expiry.
860. 🟡 [supply-chain/OSV-Scanner] Rust и npm advisories проверяются разными правилами и могут оставить blind spot - `.github/workflows/security.yml` (target) -> OSV scan обоих lockfiles + сравнение с ecosystem-native tools; отчёт содержит package path и fixed version.
861. 🟠 [release/Cosign] Release artifact нельзя криптографически связать с commit/SBOM/builder - `.github/workflows/release.yml` (target) -> Генерировать SBOM + SLSA provenance, подписывать digest и проверять policy перед deploy; tag без verification не публикуется.
862. 🔴 [privacy/Gitleaks] Нет secret scan истории и специальных правил для URL query tokens - `.gitleaks.toml` (target) -> PR + history scan, custom patterns для access/token/signature params и минимальный reviewed baseline; blanket allowlist запрещён.
863. 🟠 [secrets/SOPS] Deployment secrets могут оказаться в plaintext `.env` или diagnostic bundle - `ops/secrets/` (target) -> SOPS-encrypted manifests, внешние age/KMS keys и rotation drill; runtime decrypt не пишет plaintext на диск/в лог.

### I. Observability и performance (864-873)

864. 🟠 [observability/Prometheus] Пункт о `/metrics` не задаёт schema/cardinality budget - `backend/src/telemetry/metrics.rs` (target) -> Queue wait, process duration, failures, saturation, cache hit и bytes с bounded labels; `job_id`, URL и filename никогда не labels.
865. 🟠 [privacy/Loki] Структурированные логи могут превратить чувствительные IDs/URLs в дорогие labels - `backend/src/telemetry/log_policy.rs` (target) -> Labels только service/env/level/error_kind; high-cardinality/redacted values остаются fields, проверяемыми canary test.
866. 🟠 [observability/Jaeger] HTTP request и фоновый job теряют причинную связь после enqueue - `backend/src/telemetry/context.rs` (target) -> Сохранять trace/link context в job metadata и восстанавливать в worker; slow/error jobs sampled, normal jobs используют budget.
867. 🟠 [DIP/OpenTelemetry] Прямое подключение Prometheus/Jaeger закрепит домен за exporter API - `backend/src/ports/telemetry.rs` (target) -> Exporter-neutral telemetry port и OpenTelemetry semantic names; noop/test/export adapters подставляются без изменения services.
868. 🔴 [privacy/Vector] Redaction logs и будущего diagnostics bundle может разойтись - `backend/src/privacy/redaction.rs` (target) -> Один allowlist/redaction transform до любого sink; fixture с URL token, path, headers и filename не оставляет canary ни в JSON log, ни в bundle.
869. 🟡 [perf/Parca] CPU regressions видны только по разовым локальным flamegraphs - `ops/profiling/` (target) -> Continuous profiling только staging, symbolized build и ограниченная retention; before/after profile обязателен для perf PR.
870. 🟠 [concurrency/Loom] Стресс-тест race не перебирает все interleavings cancel/finish/lock/semaphore - `backend/src/jobs/job_cell.rs` (target) -> Сначала выделить минимальный synchronization primitive и model-check его Loom; инварианты: один terminal state, permit released once, no lost cancel.
871. 🟡 [perf/Hyperfine] Нет версионированного corpus для быстрых CLI/API путей - `bench/perf/` (target) -> Warm/cold probe, library list, cache hit и plan compile с environment/tool metadata; хранить median/p95 и сигнализировать о согласованной регрессии.
872. 🟡 [perf/Flamegraph] Оптимизации могут приниматься по интуиции - `docs/performance.md` (target) -> Для probe/import/edit/library workloads сохранять profile command и folded artifact; изменение hot path без baseline/profile не принимается как perf fix.
873. 🟡 [diagnostics/tokio-console] Нет наблюдения за age/busy/polls async tasks и удержанием sync resources - `backend/src/telemetry/console.rs` (target) -> Опциональный staging-only console subscriber, runbook task-leak investigation и alert threshold; production exposure закрыт auth/network policy.

### J. ML-assisted media (874-883)

874. 🟡 [идея/Whisper] Transcript при добавлении легко станет невалидируемым blob внутри Project - `backend/src/analysis/transcript.rs` (target) -> Versioned derived artifact с source checksum, model/version, language, segments и confidence; смена source/model инвалидирует его независимо от edit.
875. 🟡 [privacy/whisper.cpp] Cloud-only transcription конфликтует с локальной моделью продукта - `backend/src/adapters/asr/local.rs` (target) -> Local/offline adapter с capability/resource estimate и explicit privacy mode; отсутствие GPU не ломает editor и объясняет ожидаемое время.
876. 🟡 [perf/faster-whisper] Выбор ASR backend/model/quantization без corpus даст случайный trade-off - `bench/asr/` (target) -> Измерить real-time factor, RAM/VRAM и word error proxy на языковом corpus; default выбирается ADR, не популярностью repo.
877. 🟠 [UX/WhisperX] Segment timestamps недостаточны для точного text-based cut - `frontend/src/features/transcript/` (target) -> Word alignment + confidence позволяет выделять слова и создавать draft range; low-confidence boundary требует ручного preview/подтверждения.
878. 🟡 [privacy/pyannote] Speaker diarization несёт biometric/privacy смысл и не имеет доменной границы - `backend/src/analysis/speakers.rs` (target) -> Отдельный speaker track с локальными labels, consent warning, model/version/confidence и delete action; raw embeddings не сохранять по умолчанию.
879. 🟠 [UX/PySceneDetect] Scene detection без provenance превратит эвристику в необъяснимые автонарезки - `backend/src/analysis/scenes.rs` (target) -> Artifact `{ detector, version, threshold, metrics, ranges }`, использовать как snap/chapter suggestions; manual override и отключение обязательны.
880. 🟡 [DIP/OpenCV] Thumbnail/blur/black-frame/motion/crop analysis может протащить OpenCV types во весь домен - `backend/src/ports/visual_analysis.rs` (target) -> Отдельный service/adapter возвращает versioned DTO artifacts; core не импортирует `Mat` и может использовать mock/native/remote implementation.
881. 🟡 [perf/librosa] Waveform/loudness/beat/onset при каждом открытии клипа будут пересчитываться - `backend/src/analysis/audio_features.rs` (target) -> Кэшировать chunked artifact по audio fingerprint/algorithm version, отдавать progressive chunks; UI работает до полной готовности.
882. 🟡 [идея/PaddleOCR] OCR нельзя хранить только как плоский текст без временно-пространственной привязки - `backend/src/analysis/ocr.rs` (target) -> Track с time range, bbox, language, confidence и model version; поиск/субтитры используют его через port, local-first privacy profile.
883. 🟠 [safety/Ultralytics] Object detection не должна автоматически становиться committed crop/censor edit - `backend/src/analysis/object_tracks.rs` (target) -> Versioned tracks с confidence/model/license; follow-crop/censor создаются как previewable suggestions и применяются только после human approval.

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
download/          net/                         config.rs       messages.rs
  ytdlp.rs           policy.rs (чистый)          (env один раз) (i18n)
  egress_proxy.rs    resolver.rs (bounded I/O)
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
- `domain`/`ffmpeg::compile`/`net::policy`/config parsing - **чистые**;
  `net::resolver`/egress-proxy - узкая I/O-граница с injected resolver/connector.
- Пользовательские строки - только в `messages` (i18n), не в доменном коде.
- Внешние процессы (ffmpeg/yt-dlp) - только за `ffmpeg`/`download`; всё через
  один process-раннер с отменой/таймаутом.
- Конфиг читается один раз (`Config::from_env`) и прокидывается явно.

## Текущее → целевое (карта миграции)

| Область | Сейчас | Цель | Статус |
|---|---|---|---|
| ffmpeg-аргументы | `tools/args.rs` (чистый) | `ffmpeg/compile.rs` от IR | ✅ выделено, IR - позже |
| SSRF transport | `tools/{net,egress_proxy}.rs` | policy + контролируемый downloader adapter | ✅ initial/redirect/rebinding закрыты |
| HTTP god-file | `handlers/mod.rs` + 3 группы | `http/*` по ресурсам | ◐ частично |
| Оркестрация задач | копипаста в import/edit | `jobs::JobService::spawn` + `Task` | ☐ |
| Ошибки | `AppError` + `{error,code}` | typed domain/job errors | ✅ HTTP boundary; job errors позже |
| Персистентность | конкретный `Db` + `Library` JSON | трейты-репозитории + SQLite | ☐ |
| Модель правок | плоский `EditRequest` | `EditPlan`/Timeline-IR | ☐ |
| Конфиг | env в ~9 местах | `Config` один раз | ☐ |
| Frontend стор | `store.ts` god-модуль | core + features composables | ☐ |
| Frontend панель | `EditPanel.vue` god-компонент | секции-компоненты | ☐ |

Приоритизированный план «что делать первым» - в [recommendation.md](recommendation.md);
порядок и трудоёмкость каждого шага рефакторинга - в [docs/refactor-plan.md](docs/refactor-plan.md).
Известные дефекты (не модульность), верифицированный топ - в
[docs/audit.md](docs/audit.md) (118 подтверждено двумя проходами adversarial-
верификации, 14 ложных срабатываний отсеяно; раунд 2 от 1 июля 2026 добавил ещё
23 подтверждённых пункта и 2 опровержения - см. «Топ-50 актуальных проблем» в
начале `docs/audit.md`). Большая часть исходного «самого острого» списка теперь
закрыта: ✅ вырезание сегмента для AV1/ProRes, ✅ гонка cancel↔finish
(терминальные переходы), ✅ crop-валидация, ✅ пропущенный `persist_job` при
ошибке URL, ✅ рост `recover_jobs`, ✅ upload stored XSS, ✅ SSRF initial/
redirect/DNS rebinding, ✅ upload concurrency/probe timeout, ✅ единый API error
boundary и frontend HTTP/network distinction. Остаётся открытым:
TTL-чистка не проверяет активные job (№24, частично); нет auth и ownership перед
внешней публикацией (№34/421), process/resource sandbox и import filesize cap.
Свежие high раунда 2 закрыты: №202 в раунде 5, №201 в раунде 6.
