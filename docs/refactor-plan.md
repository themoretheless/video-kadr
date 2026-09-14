# Модульный рефакторинг: план из 10-критического ревью

Свод 10 независимых code-grounded ревью (каждый критик читал реальный код) по оси
**модульность / разбиение / слабая зацепленность**. Ранжировано по
польза÷риск. Принцип безопасности: «чистое перемещение, поведение не меняется,
ловится компилятором + тестами».

**Статус 2026-09-14:** продукт на Svelte 5 (`store.svelte.ts`,
`composition.svelte.ts`), не Vue. HTTP `AppError` и `Config::from_env` уже
введены. Ghost/spec-контракты (`render/`, `packaging/`, `services/preview`,
часть `domain/*`) вынесены за feature `spec-contracts` и не входят в product
graph handlers. Мультитрек живёт через `domain/composition` + composition
render path, а не через orphaned `domain/timeline`.

## Уже сделано

- **`tools.rs` → `tools/{mod,args,net}`** (`8cd508b`).
- **`handlers.rs` → `handlers/{mod,projects,library,health}`** (`349ed0d`).
- **`EditRequest → EditPlan → ExportCommandCompiler`** для single-clip export.
- **`OutputSpec`**, HTTP **`AppError`/`IntoResponse`**, **`Config::from_env`**.
- **Frontend:** `domain/edit.ts`, `ExportControls.svelte`, EditPanel ужат.
- **Ghost quarantine:** `spec-contracts` feature (2026-09-14).
- **JobRunner / plan_fingerprint cache / thin handlers / repo traits A /
  ffmpeg_filters / messages** (SOLID/DRY waves, 2026-09-14).
- **Frontend god-state split:** `composition.svelte.ts` / `store.svelte.ts` —
  тонкие façades над `composition/*` и `store/*` (2026-09-14).
- **CapCut coverage SoT:** [capcut-parity.md](capcut-parity.md) — локальный
  non-AI editor `готово`; cloud/social `в работе`; AI excluded.

## Дальше - безопасно сейчас (S, прикрыто тестами)

- **Единый источник дефолтов:** добить `diffFromDefault` / `PRESET_KEYS` из
  `EDIT_DEFAULTS` в `domain/edit.ts`. **S.**
- **`messages.rs` (i18n-каталог).** Русские строки-ошибки ещё в tools/handlers.
  **S→M.**

## Дальше - глубже (M/L)

- **JobRunner + `JobService::spawn/transition`** — один скелет queued→finish
  для import/edit/composition/proxy/publish. **L.**
- **Cache key = `plan_fingerprint`** после probe/compile (не wire JSON). **M.**
- **Тонкие handlers** после JobRunner. **M.**
- **Repo traits** `ProjectRepo`/`JobRepo`/`MediaRepo`/`RenderCache`. **M→L.**
- **Shared ffmpeg filter kernel** между `args.rs` и `composition_args.rs`. **M.**
- **Typed `ErrorKind` в async `job.error`.** **M.**

## Порядок

Ghost quarantine (done) → JobRunner → plan_fingerprint cache → thin handlers →
repos A → frontend split → ffmpeg DRY → ErrorKind/messages.
