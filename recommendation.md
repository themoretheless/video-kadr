# Рекомендации: что делать дальше

Приоритизированный план действий, синхронизированный с
[architecture.md](architecture.md) (целевой модульный дизайн),
[docs/refactor-plan.md](docs/refactor-plan.md) (пошаговый рефакторинг) и
[docs/ideas/round-13.md](docs/ideas/round-13.md) (приоритеты фич).

Принцип порядка: **корректность → дешёвая модульность → глубокий рефактор**;
фичи вклиниваются параллельно по готовности. Каждый шаг идёт под зелёным
`make check`. Оценка: **S** = часы, **M** = день-два, **L** = неделя+.

## P0 — Корректность (чинить первым, S)

1. **`persist_job` при ошибке URL в импорте.** `handlers/mod.rs:53-59`: ветка
   плохого URL ставит `Error` в память и `clear_cancel`, но **не** `persist_job`
   → в БД джоба остаётся `pending` → после рестарта показывается `interrupted`,
   а не `error`. Фикс: один `st.persist_job(&jid).await;` перед `clear_cancel`.
   (Снимется автоматически при JobRunner, P2 #9.) **S.**
2. **DNS-rebinding в `validate_url`** (`tools/net.rs`). Проверяется только
   хост-литерал, DNS не резолвится: домен, резолвящийся в 127.0.0.1/RFC1918,
   проходит SSRF-guard. Минимум - задокументировать как известное ограничение;
   полноценно - резолвить хост и проверять все адреса (или egress-allowlist). **S→M.**
3. **Ретеншн `recover_jobs`** (`state.rs`). Грузит все исторические джобы в память
   при старте (рост от рестарта к рестарту). Фикс: грузить только N последних / по
   времени, либо удалять терминальные старше TTL. **S.**

## P1 — Безопасная модульность (дни, прикрыто тестами, поведение не меняется)

Выравнивает код с [architecture.md](architecture.md) без смены поведения.

4. **`Config`-struct** (architecture.md → config). env читается в ~9 местах на
   лету (`MAX_HEIGHT` в `tools`, `JOB_TIMEOUT_SECS` в `handlers` - на каждый
   запрос). Собрать `Config::from_env()` один раз в `main`, прокинуть явно;
   fail-fast на кривом `BIND_ADDR`/`PORT`. **M.**
5. **Frontend `store.ts` → модули** (architecture.md → frontend). Чистые функции
   в `lib/`, тема/пресеты/история - в свои composables; реэкспорт из `store.ts`,
   чтобы потребители и `store.test.ts` не менялись. **S→M.**
6. **Frontend `EditPanel.vue` → секции** (architecture.md → frontend). Каталоги
   опций в `lib/editOptions.ts` (S); секции `AudioControls`/`PresetBar`/
   `ColorControls`/`Timing`/`Frame`/`Export`; развязать скрытую связь
   Export→Frame (вынести `applyPlatform`/`setAspect`). **S→M.**
7. **Единый источник дефолтов контракта** (architecture.md → контракт). Один
   `EDIT_DEFAULTS` + `diffFromDefault`; из него вывести `PRESET_KEYS` и плоскую
   часть `buildEditPayload`. Снижает «новое поле = ~6 мест» до ~3. **S.**
8. **`messages.rs` (i18n-каталог).** Вынести русские строки-ошибки из `tools`/
   `handlers` в один модуль по ключам. **S→M.**

## P2 — Глубокий рефактор (недели, меняет контракты/контрол-флоу, нужны новые тесты)

Порядок и детали - в [docs/refactor-plan.md](docs/refactor-plan.md).

9. **JobRunner + трейт `Task`** (architecture.md → jobs/). Убрать копипасту
   оркестрации `import`/`edit`; **заодно закрывает P0 #1** (persist в одном
   месте). Нужны юнит-тесты на runner (отмена в queued, no-hang `drop(tx)`). **L.**
10. **Репозитории-трейты** (`ProjectRepo`/`JobRepo`/`MediaRepo`/`RenderCache`).
    Шаг A: трейты поверх существующего `Db` (in-memory тесты без sqlite). Шаг B:
    мигрировать `Library` (JSON) в SQLite + вынести `FileStore`. **M→L.**
11. **`AppError` + `IntoResponse`.** Один enum вместо россыпи `(StatusCode,
    String)`; маппинг кодов в одном месте; `ErrorKind` для `job.error`. **L.**
12. **`JobService`.** Инвариант «терминал → persist + clear_cancel» в одном
    методе вместо дисциплины вызовов в 5 местах. **M.**
13. **Timeline IR** (architecture.md → доменная модель). Разделить wire-DTO →
    `EditPlan`/Timeline → чистый `compile()` в ffmpeg. Первый шаг - вынести
    `OutputSpec` (S); полный IR - **L**, и только он разблокирует мультитрек.

## P3 — Фичи (параллельно, из round-13)

- **Тир-1, ложатся на текущий `build_ffmpeg_args` (S–M):** хромакей (зелёный
  экран), LUT-импорт `.cube`, стабилизация `vidstab`, scopes (гистограмма/
  waveform/vectorscope), режим «до/после», авто-обрезка чёрных полос, boomerang.
- **Тир-0 (разблокировщик):** мультитрек-таймлайн - **только после Timeline IR
  (P2 #13)**, иначе строить не на чем.

## Рекомендуемая последовательность

```
P0 (часы)  →  P1 #4-#8 (модульность, безопасно)  →  P2 #9 JobRunner
           →  #10 репозитории  →  #11 AppError  →  #12 JobService  →  #13 Timeline IR
Тир-1 фичи (P3) — в любой момент;  мультитрек — после #13.
```

Самый высокий ROI прямо сейчас: **P0 целиком** (часы, реальная корректность) +
**P1 #4/#5/#6** (модульность под тестами). Самый ценный крупный шаг: **#9
JobRunner** (убирает дублирование и чинит баг), затем **#13 Timeline IR** (открывает
мультитрек и половину фич-бэклога).
