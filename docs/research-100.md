# Исследовательский раунд: 100 сильных репозиториев

Снимок на **14 июля 2026 года**. Этот документ отвечает не на вопрос «какие
зависимости добавить», а на вопрос «какие проверенные решения стоит перенести в
архитектуру video-editor». Звёзды GitHub - только сигнал масштаба сообщества, не
оценка качества и не основание для автоматического выбора технологии.

## Методика

- Отобраны 100 активных, неархивированных репозиториев в 10 релевантных группах;
  минимальный рейтинг в снимке - 1 814 звёзд.
- Для каждого проекта просмотрены публичная архитектура, границы модулей,
  эксплуатационные механизмы или пользовательские паттерны, относящиеся к этому
  редактору.
- Один репозиторий дал ровно одно новое архитектурное решение №784-883. Похожие
  старые пункты не повторяются: новая карточка добавляет отсутствующий контракт,
  критерий приёмки, эксперимент или явное решение «не внедрять до порога».
- Технологии не предлагается подключать вслепую. Сначала порт/контракт и
  benchmark, затем адаптер; прямое зацепление домена за сторонний framework
  считается регрессией.

## Что следует из первичных источников

| Источник | Наблюдение | Решение для проекта |
|---|---|---|
| [Parnas, On the Criteria To Be Used in Decomposing Systems into Modules](https://doi.org/10.1145/361598.361623) | Модуль должен скрывать изменчивое проектное решение, а не просто группировать шаги выполнения. | Границы строятся вокруг media graph, job lifecycle, persistence, playback и analysis, а не вокруг одного большого HTTP-flow. |
| [Liskov, Data Abstraction and Hierarchy](https://doi.org/10.1145/62138.62141) | Подстановка реализаций требует одинакового наблюдаемого поведения, а не только совпадения сигнатур. | Порты renderer/player/repository получают contract tests, общие для production и test-адаптеров. |
| [Out of the Tar Pit](https://moss.cs.iit.edu/cs100/papers/out-of-the-tar-pit.pdf) | Неконтролируемое состояние и его неявные связи - главный источник случайной сложности. | Производные данные (proxy, waveform, transcript, scenes) становятся версионированными артефактами, а не полями общего mutable store. |
| [FFmpeg Filters](https://ffmpeg.org/ffmpeg-filters.html) | Filtergraph является ориентированным графом с именованными входами/выходами, timeline и runtime-командами. | Нужен типизированный `FilterGraph`, а не дальнейшее наращивание строк в `args.rs`. |
| [FFmpeg command-line reference](https://ffmpeg.org/ffmpeg.html) | FFmpeg позволяет ограничивать buffered frames и filter threads и отдаёт машинный progress. | Ресурсная политика компилятора должна быть явной и тестируемой. |
| [GStreamer pipeline design](https://gstreamer.freedesktop.org/documentation/additional/design/gstpipeline.html) и [bus](https://gstreamer.freedesktop.org/documentation/application-development/basics/bus.html) | Pipeline имеет явные состояния, clock/running time и message bus между streaming и application threads. | Preview получает собственную state machine и событийную границу, отделённую от export. |
| [MLT Framework](https://www.mltframework.org/docs/framework/) и [MLT XML](https://mltframework.org/docs/mltxml/) | Producer/filter/transition/consumer образуют ленивый сериализуемый media graph. | Роли pipeline оформляются узкими портами, а проект хранит граф, не детали конкретного CLI. |
| [Tokio graceful shutdown](https://tokio.rs/tokio/topics/shutdown), [channels](https://tokio.rs/tokio/tutorial/channels) и [testing](https://tokio.rs/tokio/topics/testing) | Корректное завершение состоит из detect/notify/wait; bounded channels задают backpressure; время можно тестировать paused-clock. | Один root cancellation token, task tracker, bounded queues и детерминированные timeout/retry-тесты. |
| [The Tail at Scale](https://research.google/pubs/the-tail-at-scale/) | Пользовательскую отзывчивость определяет хвост распределения, а не только среднее. | Perf-gates измеряют p50/p95/p99 для очереди, probe, preview и API. |
| [Airflow tasks](https://airflow.apache.org/docs/apache-airflow/stable/core-concepts/tasks.html), [overview](https://airflow.apache.org/docs/apache-airflow/stable/core-concepts/overview.html) и [task state store](https://airflow.apache.org/docs/apache-airflow/stable/core-concepts/task-and-asset-state-store.html) | Task template, attempt, retry policy, pool и внешний checkpoint - разные сущности. | `Job`, `JobAttempt` и process checkpoint разделяются; retry зависит от класса ошибки. |
| [SQLite WAL](https://sqlite.org/wal.html) | WAL меняет модель concurrency/checkpoint и требует эксплуатационной политики. | До смены БД измерить SQLite WAL, задать busy timeout/checkpoint и проверить crash recovery. |
| [OWASP File Upload Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html) | Защита upload требует совместной проверки extension, MIME/signature/content, имени, прав, места хранения и лимитов. | Текущий probe дополняется формальной threat matrix и quarantine boundary. |
| [WebCodecs](https://www.w3.org/TR/webcodecs/) | Codec queues могут насыщаться, ресурсы нужно освобождать, а media pipeline рекомендуется выносить с main thread. | Browser analysis/preview использует worker, bounded queue и явный fallback по capabilities. |
| [WAI-ARIA Slider Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/slider/) | Media slider обязан поддерживать стрелки/Home/End и понятный `aria-valuetext`; touch AT требует отдельного теста. | Timeline/trim проходят keyboard и assistive-technology contract tests. |
| [OpenTelemetry Semantic Conventions](https://opentelemetry.io/docs/concepts/semantic-conventions/) | Общая схема имён связывает traces, metrics, logs, profiles и resources. | Telemetry не привязывается напрямую к одному exporter и использует стабильный словарь полей. |
| [SLSA 1.2](https://slsa.dev/spec/v1.2/) и [provenance](https://slsa.dev/spec/v1.2/provenance) | Provenance связывает артефакт с исходниками, builder и входами; более высокие уровни защищают от подмены. | Release получает SBOM, provenance, подпись и verification policy. |
| [B-Script](https://arxiv.org/abs/1902.11216) | В исследовании с 110 участниками transcript-интерфейс ускорил B-roll editing, а рекомендации повысили вовлечённость результата. | Transcript становится альтернативной навигацией и способом выбирать диапазон, не заменяя timeline. |
| [AVscript](https://arxiv.org/abs/2302.14117) | Audio-visual script снизил mental demand и повысил самостоятельность незрячих и слабовидящих авторов. | Analysis track должен описывать сцены, речь и визуальные проблемы доступным текстом. |
| [ExpressEdit](https://arxiv.org/abs/2403.17693) | Комбинация natural language и sketch помогает новичкам выражать temporal/spatial intent, но требует итеративного подтверждения. | AI-команда компилируется в previewable draft `EditPlan`, а не применяет правку автоматически. |
| [YouTube UGC subjective quality](https://research.google/pubs/subjective-quality-assessment-for-youtube-ugc-dataset/) и [rich features](https://research.google/pubs/rich-features-for-perceptual-quality-assessment-of-ugc-videos/) | MOS зависит от content, technical quality, compression и variation между chunks. | Quality report хранит per-scene показатели и не сводится к одному CRF. |
| [Netflix VMAF](https://github.com/Netflix/vmaf) и [model guidance](https://github.com/Netflix/vmaf/blob/master/resource/doc/models.md) | Перцептуальная оценка зависит от viewing conditions и выбранной модели. | VMAF-профиль явно фиксирует model/display assumptions и сначала работает как advisory. |
| [Whisper paper](https://arxiv.org/abs/2212.04356) и [WhisperX](https://arxiv.org/abs/2303.00747) | Multilingual ASR и word alignment позволяют навигацию по речи, но точность и ресурсы зависят от модели. | Transcript хранит model/version/language/confidence и не считается исходной истиной. |
| [PySceneDetect API](https://www.scenedetect.com/docs/latest/api.html) | Разные detector-алгоритмы дают timecode pairs и сохраняемые frame metrics. | Scene boundaries становятся кэшируемым analysis artifact с detector/version/threshold. |
| [GLANCE](https://arxiv.org/abs/2604.05076) и [EditDuet](https://arxiv.org/abs/2509.10761) | Новые preprint-подходы используют global/local planning, critic loop и проверку ограничений. | Это emerging-направление: сначала безопасный propose-review-apply протокол и benchmark, без автономного монтажа в production. |

## 100 репозиториев

Звёзды округлять нельзя: ниже сохранён точный снимок GitHub GraphQL на дату
исследования. Ссылка ведёт прямо на первичный репозиторий.

### A. NLE и media pipeline

| # | Репозиторий | Stars | Что переносим | Пункт |
|---:|---|---:|---|---:|
| 1 | [FFmpeg/FFmpeg](https://github.com/FFmpeg/FFmpeg) | 62 022 | Типизированный DAG filtergraph и его debug-представление | 784 |
| 2 | [GStreamer/gstreamer](https://github.com/GStreamer/gstreamer) | 3 257 | Состояния preview pipeline, clock и message boundary | 785 |
| 3 | [mltframework/mlt](https://github.com/mltframework/mlt) | 1 814 | Порты producer/filter/transition/consumer | 786 |
| 4 | [olive-editor/olive](https://github.com/olive-editor/olive) | 9 080 | Стабильные ID и недеструктивный operation graph | 787 |
| 5 | [OpenShot/openshot-qt](https://github.com/OpenShot/openshot-qt) | 6 055 | Compatibility corpus для project schema | 788 |
| 6 | [KDE/kdenlive](https://github.com/KDE/kdenlive) | 5 296 | Proxy media как производный артефакт | 789 |
| 7 | [mltframework/shotcut](https://github.com/mltframework/shotcut) | 14 535 | Runtime capability manifest | 790 |
| 8 | [blender/blender](https://github.com/blender/blender) | 19 152 | Dependency-graph invalidation для кэшей | 791 |
| 9 | [obsproject/obs-studio](https://github.com/obsproject/obs-studio) | 73 849 | Разные политики preview и offline export | 792 |
| 10 | [remotion-dev/remotion](https://github.com/remotion-dev/remotion) | 53 060 | Детерминированный frame renderer и sharding | 793 |

### B. Кодеки, качество и packaging

| # | Репозиторий | Stars | Что переносим | Пункт |
|---:|---|---:|---|---:|
| 11 | [Netflix/vmaf](https://github.com/Netflix/vmaf) | 5 411 | Advisory post-export quality report | 794 |
| 12 | [rust-av/Av1an](https://github.com/rust-av/Av1an) | 1 950 | Scene-aware resumable encoding | 795 |
| 13 | [xiph/rav1e](https://github.com/xiph/rav1e) | 4 137 | Явный encode resource budget | 796 |
| 14 | [xiph/opus](https://github.com/xiph/opus) | 3 240 | Типизированный audio output contract | 797 |
| 15 | [shaka-project/shaka-packager](https://github.com/shaka-project/shaka-packager) | 2 551 | Packaging как стадия после encode | 798 |
| 16 | [AOMediaCodec/libavif](https://github.com/AOMediaCodec/libavif) | 2 135 | Color/alpha/orientation round-trip для still export | 799 |
| 17 | [libjxl/libjxl](https://github.com/libjxl/libjxl) | 3 576 | Расширяемый `StillImageEncoder` port | 800 |
| 18 | [strukturag/libheif](https://github.com/strukturag/libheif) | 2 265 | Разделение container brand, item и codec | 801 |
| 19 | [Haivision/srt](https://github.com/Haivision/srt) | 3 556 | Изолированный reliable ingest adapter | 802 |
| 20 | [PyAV-Org/PyAV](https://github.com/PyAV-Org/PyAV) | 3 243 | Нормализованный typed stream metadata model | 803 |

### C. Playback и streaming

| # | Репозиторий | Stars | Что переносим | Пункт |
|---:|---|---:|---|---:|
| 21 | [videojs/video.js](https://github.com/videojs/video.js) | 39 816 | `PlayerAdapter` вместо DOM-ссылки в store | 804 |
| 22 | [video-dev/hls.js](https://github.com/video-dev/hls.js) | 16 820 | Error taxonomy и bounded recovery | 805 |
| 23 | [shaka-project/shaka-player](https://github.com/shaka-project/shaka-player) | 8 147 | Явный unsupported-capability result | 806 |
| 24 | [Dash-Industry-Forum/dash.js](https://github.com/Dash-Industry-Forum/dash.js) | 5 539 | Отдельная политика preview representation | 807 |
| 25 | [sampotts/plyr](https://github.com/sampotts/plyr) | 29 891 | Accessibility contract для media controls | 808 |
| 26 | [mediaelement/mediaelement](https://github.com/mediaelement/mediaelement) | 8 296 | Единые player events для разных source types | 809 |
| 27 | [muxinc/media-chrome](https://github.com/muxinc/media-chrome) | 2 708 | Компонуемые media-control primitives | 810 |
| 28 | [bluenviron/mediamtx](https://github.com/bluenviron/mediamtx) | 19 482 | Ingest gateway вне editor core | 811 |
| 29 | [ossrs/srs](https://github.com/ossrs/srs) | 29 045 | Load shedding и квоты live ingest | 812 |
| 30 | [jellyfin/jellyfin](https://github.com/jellyfin/jellyfin) | 54 252 | Incremental media indexer | 813 |

### D. Editor interactions и canvas

| # | Репозиторий | Stars | Что переносим | Пункт |
|---:|---|---:|---|---:|
| 31 | [excalidraw/excalidraw](https://github.com/excalidraw/excalidraw) | 127 380 | Operation-based history с транзакциями | 814 |
| 32 | [tldraw/tldraw](https://github.com/tldraw/tldraw) | 48 734 | Tool state machine и единый pointer lifecycle | 815 |
| 33 | [penpot/penpot](https://github.com/penpot/penpot) | 55 674 | Versioned design-token contract | 816 |
| 34 | [fabricjs/fabric.js](https://github.com/fabricjs/fabric.js) | 31 314 | Типы coordinate spaces и transform matrix | 817 |
| 35 | [konvajs/konva](https://github.com/konvajs/konva) | 14 612 | Layered scene graph и hit testing | 818 |
| 36 | [pixijs/pixijs](https://github.com/pixijs/pixijs) | 47 764 | Evidence gate для GPU renderer | 819 |
| 37 | [paperjs/paper.js](https://github.com/paperjs/paper.js) | 15 061 | Независимый geometry kernel | 820 |
| 38 | [nhn/tui.image-editor](https://github.com/nhn/tui.image-editor) | 7 660 | Декларативный tool registry | 821 |
| 39 | [xyflow/xyflow](https://github.com/xyflow/xyflow) | 37 618 | Dev-only visualizer render DAG | 822 |
| 40 | [alyssaxuu/motionity](https://github.com/alyssaxuu/motionity) | 4 057 | Домен keyframes/easing/interpolation | 823 |

### E. Rust backend

| # | Репозиторий | Stars | Что переносим | Пункт |
|---:|---|---:|---|---:|
| 41 | [tokio-rs/tokio](https://github.com/tokio-rs/tokio) | 32 525 | Structured shutdown и task tracking | 824 |
| 42 | [tokio-rs/axum](https://github.com/tokio-rs/axum) | 26 508 | Router contract tests поверх service ports | 825 |
| 43 | [tower-rs/tower](https://github.com/tower-rs/tower) | 4 241 | Route-class middleware policy stack | 826 |
| 44 | [actix/actix-web](https://github.com/actix/actix-web) | 24 730 | Benchmark до смены HTTP framework | 827 |
| 45 | [hyperium/hyper](https://github.com/hyperium/hyper) | 16 219 | Slow-client backpressure/disconnect tests | 828 |
| 46 | [serde-rs/serde](https://github.com/serde-rs/serde) | 10 718 | Разные strictness policy для wire и storage | 829 |
| 47 | [transact-rs/sqlx](https://github.com/transact-rs/sqlx) | 17 306 | CI-проверка query/schema drift | 830 |
| 48 | [tokio-rs/tracing](https://github.com/tokio-rs/tracing) | 6 775 | Стабильная span schema и redaction tests | 831 |
| 49 | [rayon-rs/rayon](https://github.com/rayon-rs/rayon) | 13 151 | Отдельный bounded CPU executor | 832 |
| 50 | [rustls/rustls](https://github.com/rustls/rustls) | 7 518 | Явный TLS termination profile | 833 |

### F. Jobs и persistence

| # | Репозиторий | Stars | Что переносим | Пункт |
|---:|---|---:|---|---:|
| 51 | [temporalio/temporal](https://github.com/temporalio/temporal) | 21 620 | Replayable job event log | 834 |
| 52 | [apache/airflow](https://github.com/apache/airflow) | 46 107 | `JobAttempt` и retry по error kind | 835 |
| 53 | [celery/celery](https://github.com/celery/celery) | 28 680 | Failed-job registry и operator actions | 836 |
| 54 | [taskforcesh/bullmq](https://github.com/taskforcesh/bullmq) | 9 099 | Dedupe key и queue rate limit | 837 |
| 55 | [rq/rq](https://github.com/rq/rq) | 10 665 | Lifecycle registries и reconciliation | 838 |
| 56 | [riverqueue/river](https://github.com/riverqueue/river) | 5 442 | Transactional enqueue/outbox | 839 |
| 57 | [restic/restic](https://github.com/restic/restic) | 34 993 | Content-addressed backup/restore check | 840 |
| 58 | [borgbackup/borg](https://github.com/borgbackup/borg) | 13 508 | Chunk dedup benchmark для backup | 841 |
| 59 | [facebook/rocksdb](https://github.com/facebook/rocksdb) | 31 863 | Измеримый порог до смены SQLite | 842 |
| 60 | [meilisearch/meilisearch](https://github.com/meilisearch/meilisearch) | 58 555 | Search port с SQLite FTS по умолчанию | 843 |

### G. Vue, frontend и testing

| # | Репозиторий | Stars | Что переносим | Пункт |
|---:|---|---:|---|---:|
| 61 | [vuejs/core](https://github.com/vuejs/core) | 53 950 | Feature public APIs и forbidden imports | 844 |
| 62 | [vuejs/pinia](https://github.com/vuejs/pinia) | 14 643 | Проверяемые domain stores с facade | 845 |
| 63 | [vitejs/vite](https://github.com/vitejs/vite) | 82 045 | Initial-bundle budget | 846 |
| 64 | [vitest-dev/vitest](https://github.com/vitest-dev/vitest) | 16 836 | Fake-time state-machine tests | 847 |
| 65 | [vueuse/vueuse](https://github.com/vueuse/vueuse) | 22 302 | Lifecycle-safe composables | 848 |
| 66 | [storybookjs/storybook](https://github.com/storybookjs/storybook) | 90 679 | Полный каталог UI-состояний | 849 |
| 67 | [microsoft/playwright](https://github.com/microsoft/playwright) | 92 750 | Backendless и cross-browser smoke | 850 |
| 68 | [cypress-io/cypress](https://github.com/cypress-io/cypress) | 50 678 | ADR выбора одного E2E runner | 851 |
| 69 | [TanStack/query](https://github.com/TanStack/query) | 49 986 | Server-state cache вне UI store | 852 |
| 70 | [floating-ui/floating-ui](https://github.com/floating-ui/floating-ui) | 32 659 | Единый accessible overlay primitive | 853 |

### H. Security и supply chain

| # | Репозиторий | Stars | Что переносим | Пункт |
|---:|---|---:|---|---:|
| 71 | [OWASP/CheatSheetSeries](https://github.com/OWASP/CheatSheetSeries) | 32 572 | Проверяемая upload threat matrix | 854 |
| 72 | [google/oss-fuzz](https://github.com/google/oss-fuzz) | 12 426 | Continuous parser fuzzing | 855 |
| 73 | [rust-fuzz/cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) | 1 845 | Локальный fuzz corpus для границ | 856 |
| 74 | [rustsec/rustsec](https://github.com/rustsec/rustsec) | 1 920 | Advisory policy с expiring exceptions | 857 |
| 75 | [EmbarkStudios/cargo-deny](https://github.com/EmbarkStudios/cargo-deny) | 2 367 | License/source/duplicate policy | 858 |
| 76 | [aquasecurity/trivy](https://github.com/aquasecurity/trivy) | 36 899 | Image/filesystem/IaC scan | 859 |
| 77 | [google/osv-scanner](https://github.com/google/osv-scanner) | 10 645 | Cross-ecosystem lockfile scan | 860 |
| 78 | [sigstore/cosign](https://github.com/sigstore/cosign) | 6 123 | Подпись artifact/SBOM/provenance | 861 |
| 79 | [gitleaks/gitleaks](https://github.com/gitleaks/gitleaks) | 28 119 | Secret scan с query-token rules | 862 |
| 80 | [getsops/sops](https://github.com/getsops/sops) | 22 478 | Шифрованные deploy secrets | 863 |

### I. Observability и performance

| # | Репозиторий | Stars | Что переносим | Пункт |
|---:|---|---:|---|---:|
| 81 | [prometheus/prometheus](https://github.com/prometheus/prometheus) | 65 186 | Low-cardinality metrics schema | 864 |
| 82 | [grafana/loki](https://github.com/grafana/loki) | 28 554 | Безопасная log-label policy | 865 |
| 83 | [jaegertracing/jaeger](https://github.com/jaegertracing/jaeger) | 22 992 | Trace context через queue boundary | 866 |
| 84 | [open-telemetry/opentelemetry-rust](https://github.com/open-telemetry/opentelemetry-rust) | 2 646 | Exporter-neutral telemetry port | 867 |
| 85 | [vectordotdev/vector](https://github.com/vectordotdev/vector) | 22 173 | Единый redaction pipeline | 868 |
| 86 | [parca-dev/parca](https://github.com/parca-dev/parca) | 4 909 | Continuous profiling в staging | 869 |
| 87 | [tokio-rs/loom](https://github.com/tokio-rs/loom) | 2 751 | Model-checking нового `JobCell` | 870 |
| 88 | [sharkdp/hyperfine](https://github.com/sharkdp/hyperfine) | 28 470 | Версионированный perf corpus | 871 |
| 89 | [flamegraph-rs/flamegraph](https://github.com/flamegraph-rs/flamegraph) | 5 973 | Profile-before-optimize gate | 872 |
| 90 | [tokio-rs/console](https://github.com/tokio-rs/console) | 4 554 | Async task/resource diagnostics | 873 |

### J. ML-assisted media

| # | Репозиторий | Stars | Что переносим | Пункт |
|---:|---|---:|---|---:|
| 91 | [openai/whisper](https://github.com/openai/whisper) | 104 884 | Transcript как versioned derived artifact | 874 |
| 92 | [ggml-org/whisper.cpp](https://github.com/ggml-org/whisper.cpp) | 51 770 | Local/offline inference adapter | 875 |
| 93 | [SYSTRAN/faster-whisper](https://github.com/SYSTRAN/faster-whisper) | 24 255 | Benchmark model/quantization backend | 876 |
| 94 | [m-bain/whisperX](https://github.com/m-bain/whisperX) | 23 054 | Word-aligned transcript selection | 877 |
| 95 | [pyannote/pyannote-audio](https://github.com/pyannote/pyannote-audio) | 10 271 | Speaker diarization track | 878 |
| 96 | [Breakthrough/PySceneDetect](https://github.com/Breakthrough/PySceneDetect) | 5 007 | Scene boundaries как snap suggestions | 879 |
| 97 | [opencv/opencv](https://github.com/opencv/opencv) | 89 880 | Изолированный visual-analysis service | 880 |
| 98 | [librosa/librosa](https://github.com/librosa/librosa) | 8 498 | Waveform/beat/onset artifacts | 881 |
| 99 | [PaddlePaddle/PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR) | 85 388 | OCR track с bbox/confidence/language | 882 |
| 100 | [ultralytics/ultralytics](https://github.com/ultralytics/ultralytics) | 59 444 | Human-approved object tracks | 883 |

## Исполнение пакетами

Все 100 решений исполняются десятью волнами по 10. Полный состав волн и
актуальные checkbox-статусы находятся в
[recommendation.md](../recommendation.md#волны-исполнения-по-10-пунктов), а
архитектурная карта - в
[architecture.md](../architecture.md#исполнение-десятью-волнами).

Первая волна закрыта 14 июля 2026: №790, 824, 828, 829, 831, 844, 846, 847,
850 и 854. Она намеренно поставила guardrails перед тяжёлым media refactor:
encoder/muxer/filter capabilities, SIGINT/SIGTERM bounded shutdown, HTTP
backpressure, strict/versioned wire DTO, privacy-safe tracing, frontend dependency
rules, bundle budget,
deterministic async tests, cross-browser backendless smoke и upload threat model.

Вторая волна закрыта 14 июля 2026: №784, 786, 787, 803, 814, 817, 820, 823,
825 и 826. Она ввела pure media/timeline domain, normalized probe adapter,
command history, branded/shared geometry corpus, deterministic keyframes и
port-based HTTP contracts с route policy. FFmpeg compiler и crop/censor UI уже
используют новые границы; public auth при этом не имитируется и остаётся P0.

## Приоритетный вывод

После закрытия первых двух волн ближайший research-фокус - волна 3: durable job
events/attempts, retry taxonomy, dedupe/rate limits, transactional outbox,
backup/search contracts и concurrency model-checking (№834-840, 842, 843, 870).
Отдельный старый P0 перед внешней публикацией остаётся: auth, ownership и process
resource limits.

Полные формулировки и критерии приёмки находятся в
[architecture.md](../architecture.md#исследовательский-слой-100-репозиториев-14-июля-2026),
исполняемый чеклист - в
[recommendation.md](../recommendation.md#исследовательский-чеклист-100-репозиториев-14-июля-2026).
