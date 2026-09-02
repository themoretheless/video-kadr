# ADR: external process isolation tiers

Дата решения: 19 июля 2026. Статус: accepted, public tier fail-closed.

## Контекст

Backend запускает `ffmpeg`, `ffprobe` и `yt-dlp` над недоверенными media и URL.
Timeout и cooperative cancellation ограничивают обычные ошибки, но не являются
границей безопасности. Любой новый production spawn поэтому обязан пройти через
`ProcessRuntime::prepare` и получить typed `ProcessPolicy`.

Policy всегда задаёт:

- tool role и filesystem read-only/read-write scope;
- denied network или единственный pinned loopback proxy downloader;
- allowlist наследуемого environment;
- CPU, address-space, child-process, FD и file-size budgets;
- maximum retained bytes на stdout/stderr и maximum bytes одной строки.

Только `PreparedCommand` владеет методом `spawn`. Raw `Command::spawn` в
production modules запрещён архитектурной границей и review search.

Реализация разделена по одной причине изменения на модуль:

- `policy.rs` — capability model и инварианты;
- `runtime.rs` — компиляция policy в scrubbed `PreparedCommand`;
- `limits.rs` — platform kernel controls;
- `execution.rs` — bounded pipes, timeout/cancel и TERM→KILL process-group cleanup.

## Тиры

| Tier | Bind | Назначение | Startup policy |
| --- | --- | --- | --- |
| `local` | Только loopback | Один доверенный desktop user | Non-loopback bind отклоняется |
| `lan` | Явно разрешён | Доверенная приватная сеть | Требует явного `ISOLATION_TIER=lan`; auth и tenant isolation отсутствуют |
| `public` | Потенциально внешний | Недоверенные users/media | Всегда отклоняется до auth, ownership, cgroup и executable sandbox adapter |

`PROCESS_SANDBOX=nsjail|bubblewrap` по-прежнему отклоняется на startup: выбор
имени адаптера не считается deployment authorization. В коде есть исполняемый
компилятор профиля NsJail 3.6 (`sandbox.rs`) с mandatory mount allowlist,
отдельными user/mount/PID/IPC/UTS/network/cgroup namespaces, loopback off,
versioned seccomp checksum, cgroup v2 memory/PID/CPU budgets и отдельным
tenant UID/GID. Public wiring включается только вместе с auth и ownership, без
fallback на прямой запуск.

## Реальное enforcement

| Контроль | macOS local/LAN | Linux local/LAN | Public |
| --- | --- | --- | --- |
| Mandatory typed policy | Enforced | Enforced | Startup blocked |
| Environment clear + allowlist | Enforced | Enforced | Startup blocked |
| Process group + terminate/kill | Enforced | Enforced | Startup blocked |
| Bounded stdout/stderr and line | Enforced | Enforced | Startup blocked |
| CPU limit | `RLIMIT_CPU` | `RLIMIT_CPU` | Требуется cgroup + rlimit |
| File size limit | `RLIMIT_FSIZE` | `RLIMIT_FSIZE` | Требуется cgroup + rlimit |
| Open files | `RLIMIT_NOFILE` | `RLIMIT_NOFILE` | Требуется cgroup + rlimit |
| Address space | Недоступно надёжно | `RLIMIT_AS` | Требуется cgroup memory |
| PID tree | Только budget в policy | Только budget в policy | Требуется cgroup pids или отдельный uid |
| Filesystem mounts | Только typed scope | Только typed scope | Требуется NsJail/Bubblewrap mounts |
| Network namespace | Нет | Нет | Требуется namespace deny-by-default |
| Tool-level network deny | FFmpeg/ffprobe protocol allowlist | FFmpeg/ffprobe protocol allowlist | Дополнительная защита, не замена namespace |
| Downloader egress | Pinned loopback proxy | Pinned loopback proxy | То же внутри sandbox |

`RLIMIT_NPROC` намеренно не применяется в local/LAN: это per-UID limit, а не
лимит job tree. При общем desktop UID он может блокировать unrelated processes и
не даёт требуемой tenant isolation. На macOS `RLIMIT_AS` возвращает `EINVAL`,
поэтому memory capability явно false, а не silently best-effort.

## Environment и network

Probe/render/discovery наследуют только `PATH`, temp и locale variables.
Downloader дополнительно может наследовать certificate paths. `HOME`, user
proxy variables, loader injection variables и произвольные secrets не
наследуются. Downloader получает proxy URL только из запущенного backend
loopback egress proxy; credentials, query, fragment и non-loopback host
отклоняются.

FFmpeg и ffprobe запускаются с protocol allowlist
`file,pipe,fd,crypto,data`. Это блокирует обычные HTTP/TCP inputs, но не является
защитой от native-code exploit, поэтому public tier всё равно закрыт.

## Конфигурация

- `ISOLATION_TIER=local|lan|public`, default `local`.
- `PROCESS_SANDBOX=none|nsjail|bubblewrap`, сейчас допустим только `none`.
- `PROCESS_MAX_CPU_SECONDS`, default 7200.
- `PROCESS_MAX_MEMORY_MIB`, default min(runtime memory, 2048).
- `PROCESS_MAX_CHILDREN`, default 64, declarative до per-tenant uid/cgroup.
- `PROCESS_MAX_OPEN_FILES`, default 256.
- `PROCESS_MAX_FILE_BYTES`, default равен upload limit.
- `PROCESS_MAX_CAPTURE_BYTES`, default 2 MiB на stream.
- `PROCESS_MAX_LINE_BYTES`, default 64 KiB.

Нулевые, переполненные и противоречивые значения останавливают startup.

## Следующие обязательные шаги

1. Подготовить и подписать production seccomp policy на codec corpus; модель
   связывает policy checksum с tool fingerprint, но сам policy не генерируется.
2. Добавить auth и ownership до снятия public startup block.
3. Прогнать adversarial Linux deployment suite с реальным NsJail 3.6 и cgroup v2.
4. Для downloader спроектировать отдельный namespace transport к pinned proxy;
   обычные probe/render остаются полностью offline.
