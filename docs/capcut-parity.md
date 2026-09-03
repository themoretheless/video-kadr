# Матрица функционального паритета с CapCut

Это нормализованная карта возможностей, а не обещание полного клона. Набор функций
CapCut зависит от региона, Web/Desktop/Mobile, версии, устройства, тарифа и AI
credits. Статус `готово` ниже означает только достижимую end-to-end функцию этого
репозитория; маркетинговая страница CapCut сама по себе не делает функцию готовой.
Внешние возможности должны включаться через registry platform/region/plan/credits,
а не одним глобальным флагом. Pippit, Dreamina и Hypic — связанные, но отдельные
продукты, поэтому в CapCut-parity ниже не включены.

## Статусы и порядок зависимостей

- `готово` — работает в текущем редакторе и экспорте.
- `в работе` — контракт и часть сквозного пути уже реализованы, но семейство
  ещё не прошло полную acceptance-матрицу.
- `запланировано` — не-AI функция с определёнными зависимостями и критериями.
- `исключено (AI)` — намеренно не входит в текущий scope: никаких моделей,
  удалённого inference, ASR/TTS или генеративных заглушек.

Порядок реализации: **core timeline → layers/audio/text → capture/performance →
local assets/templates → cloud/team/delivery**. AI не является промежуточной
зависимостью и не блокирует ни локальный редактор, ни collaboration.

## Нормализованная матрица

| Область | Нормализованная возможность CapCut | Статус | Граница репозитория |
|---|---|---|---|
| Media | Импорт локального файла/URL, медиатека, metadata и сохранение проекта | готово | `yt-dlp`, bounded upload, favorites/tags/search, codec metadata и autosave/restore работают локально. |
| Media recovery | Missing-media detection, relink и переносимый project package | готово | `.veproj` export/import с checksums и новым source mapping, missing-source UI и атомарный type/duration/audio-safe relink работают с undo/redo и autosave. |
| Single-clip edit | Trim, middle cut, crop, resize, rotate, mirror, reverse, speed, fade | готово | Один source, плоский edit plan и FFmpeg export; trim/segment boundaries компилируются с microsecond precision и real-tested на 60 fps; crop имеет persisted free/9:16/1:1/4:5/4:3/16:9 lock для handles и numeric dimensions; точная скорость `0.05×..16×` сохраняет A/V sync через bounded `atempo` chain. |
| Color | Brightness/contrast/saturation, HSL, wheels, looks, LUT, RGB/Master curves, scopes, denoise, sharpen, grain | готово | Детерминированный FFmpeg export; browser scopes локальны, export-only эффекты помечены явно. |
| Video effects | Clip-level Blur, Pixelate, Vignette, Sharpen, Edge, RGB Split и Posterize | готово | До пяти эффектов из семи bounded presets сохраняются в authored order, компилируются только в canonical `gblur`/`pixelize`/`vignette`/`unsharp`/`edgedetect`/`rgbashift`/deterministic `elbg` filters и request-gated по фактической сборке FFmpeg. Blur имеет browser approximation; остальные явно export-exact. Произвольные filter strings не принимаются. |
| Keying | Chroma key и подавление chroma spill | готово | Key работает в single-clip и composition overlays; доступность fail-closed по filters `chromakey`/`despill`. |
| Audio basics | Mute, gain/pan, fades, loudness, high-pass, EQ, compressor/limiter, voiceover и DSP voice effects | готово | Browser recording/DSP и single-clip export не используют модели или remote inference; multitrack audio clips поддерживают независимые duration-preserving pitch −12…+12 semitones и bass/treble tone tilt −12…+12 dB, `deep`/`high`/`chipmunk` presets, bounded `echo` и band-limited `robot` modulation с request-specific capability gates; real-render проверяет octave/custom и chipmunk shifts, tone spectrum, delayed repeat, robot sidebands и длительность; все single-clip/audio-only/ordered-segment exports нормализуют audio clock через `aresample=48000:async=1:first_pts=0` и reset PTS. |
| Export | MP4/WebM/AV1/ProRes/GIF/still/MP3/WAV/AAC/FLAC, codec/quality/FPS и social presets | готово | Локальный file export; WAV извлекает lossless PCM s16le 48 kHz stereo с теми же trim/audio DSP controls. Прямой publish в соцсети описан отдельно. |
| Ordered timeline | Split, duplicate, delete и reorder диапазонов одного source | готово | Порядок массива — порядок playback/export; duplicate и overlap намеренно повторяют материал, границы сохраняют microsecond precision. До 512 фрагментов и 24 часов post-speed output. |
| Preview compare | Переключение «Оригинал / С правками» | готово | Сравнивается browser preview; финальная истина export-only эффектов остаётся в FFmpeg-результате. |
| Capture | Screen/window/tab, webcam PiP, mic/system audio, annotations и teleprompter | готово | Permissions, device selection, countdown/pause/resume и bounded local upload реализованы; region зависит от системного picker. |
| Core timeline | Несколько video/audio/image/text tracks, несколько source clips, merge, markers, snapping и canvas | готово | Versioned composition, immutable commands, ripple delete, track magnet, shortcuts, durable render jobs и v2 projects работают end-to-end; undoable canvas authoring включает custom even dimensions, 16:9/9:16/1:1/4:5/4:3 social presets, fractional FPS, solid color, checker pattern и регулируемый source-derived blur с одинаковым browser/FFmpeg результатом и request-specific `gblur` gate. |
| Layers | Overlays, opacity, Rectangle/Ellipse/Linear masks, blend modes и keyframes/graph curves | готово | Primary и overlay transform/opacity keyframes, chroma, восемь blend modes, static/animated geometry и rotation для Rectangle/Ellipse/Linear, feather/invert и browser polygon preview компилируются; legacy неоднозначный wire `kind=mask, shape=linear` остаётся fail-closed. |
| Layer animation presets | Fade/Slide/Zoom In/Out, Pulse Loop и Spin Loop для video/image/text/stickers | готово | Восемь undoable presets материализуются в обычные bounded X/scale/rotation/opacity keyframes: их можно дальше править graph editor, preview семплирует тот же track, а FFmpeg получает canonical expressions. In/Out duration ограничена clip, Loop использует всю длительность; отдельного непрозрачного animation wire нет. |
| Playback/motion | Speed curves, transitions, reverse/freeze, optical-flow slow motion, stabilization и classical point tracking | готово | Hold/Linear speed ramps с точной reciprocal-integral длительностью и preserve-pitch/mute audio policy, двадцать четыре primary и overlay transitions (dissolve/fade-black, hard/smooth horizontal/vertical/diagonal wipe, slide, circle-open/close и central horizontal/vertical open/close) с точными source handles, browser preview и real FFmpeg render, reverse/freeze, real-tested minterpolate/deshake и bounded ZNCC tracker работают без моделей; tracker принимает любой видимый forward/reverse video, использует canonical temporal mapping для speed ramps, учитывает покадровые X/Y/scale/rotation и поддерживает transition-bearing clips в nominal interval. |
| Multicam | Audio/manual sync, angle viewer, live switch recording и program cut list | готово | 2–8 angles, local waveform correlation, continuous master audio и deterministic owned-only EDL rebuild. |
| Performance | Production proxies, preview selection, thumbnails/waveforms/filmstrip и cache cleanup | готово | H.264/ProRes proxies с jobs/cancel/delete, verified proxy fallback/status в composition preview, immutable thumbnails и 8-frame hover scrub работают; final export всегда читает originals. |
| Advanced audio | Multi-track mixer, detach/reverse audio, gain/pan automation, fades/crossfades, solo/mute, limiter, auto-ducking и Auto Beat | готово | Embedded audio отделяется одной undoable-командой в independent clip с точными timing, direction, speed ramp и gain/pan automation; reverse video становится reversed audio с browser source mapping и FFmpeg `areverse`, а original video sound выключается атомарно. Sidechain ducking, handle-backed crossfade и local Auto Beat работают сквозным путём; freeze, muted ramp и reverse-crossfade отклоняются fail-closed, learned separation/denoise исключены. |
| Silence removal | Автоматическое удаление длинных пауз без распознавания речи | готово | Legacy и выбранный Multitrack video/audio clip локально анализируют bounded RMS waveform и сохраняют настраиваемый pre/post-roll. Legacy пересекает результат с ordered/duplicated keep-ranges; Multitrack режет через canonical trim, поддерживает forward/reverse/speed ramps и ripple-сдвигает следующие clips одной undoable-командой. Transition-bearing clip отклоняется fail-closed. Web Audio decode, разрезание и Undo browser-tested на 8-секундном tone/silence corpus; ASR и модели не используются. |
| Text | Text layers/templates, manual captions, subtitle import/export и styling | готово | Private UTF-8 textfiles, font allowlist, Unicode, SRT round-trip, timecoded TXT import, track-wide caption styling и local templates работают без ASR. |
| Composition delivery | MP4/H.264/H.265, WebM VP9/AV1, MOV ProRes, audio-only MP3/WAV/AAC/FLAC, Custom Kbps и In/Out range export | готово | Canonical output profiles, legacy MP4/H.264 compatibility, request-specific capability checks, truthful extensions/codecs и реальная ffprobe-матрица работают end-to-end. MP4/WebM принимают сохраняемый bounded Custom bitrate 100…200000 Kbps вместо CRF; ProRes/audio-only отклоняют несовместимую настройку. Audio-only profiles экспортируют итоговый multitrack mix без video stream: MP3/AAC 192k, WAV PCM s16le 48 kHz stereo или lossless FLAC. Export-only In/Out ставятся кнопками либо клавишами `I`/`O`, не меняют clips и после полного graph точно trim/reset PTS обоих итоговых потоков. |
| Local assets/templates | Favorites/tags/search, reusable templates и portable packages | готово | JSON templates со replaceable slots и `.veproj` package не содержат произвольных путей. Hosted licensed catalog остаётся отдельной задачей. |
| Captions/transcript | Auto captions, bilingual subtitles, transcript editing и filler-word removal | исключено (AI) | ASR, learned alignment и translation не реализуются в текущем scope; обычное waveform-based silence removal вынесено в отдельную готовую функцию. |
| Assisted editing | Auto Cut и long-video-to-shorts с highlights, reframing и captions | исключено (AI) | Семантический/model pipeline намеренно отсутствует. |
| AI media tools | Learned background removal, relight, upscale, generative fill и semantic search | исключено (AI) | Chroma key, ручные masks и классические CV/DSP остаются разрешёнными. |
| Generative creation | Script/storyboard, text/image/script-to-video, Director Mode и dialogue scenes | исключено (AI) | Генеративные providers и заглушки не добавляются. |
| Generative audio/design | AI music/stickers/images, product scenes и restoration models | исключено (AI) | Локальные лицензированные assets/templates остаются в scope. |
| Voice/avatar | TTS, voice cloning, AI dubbing, avatars и lip-sync | исключено (AI) | Обычная voiceover recording и DSP voice effects остаются в scope. |
| Cloud/team | Cross-device sync, Spaces, roles, edit handoff, review links/comments и permissions | в работе | Durable review, roles, audit, public read-only links, Argon2id sessions, private Spaces, Space-bound media, hashed consume-once 7-day editor/viewer invite links, atomic ownership transfer, owner-only rename/member revoke, safe empty-Space deletion, confirmed bulk teardown и bulk export всех проектов в канонические `.veproj`, physical per-Space directories, membership-gated delivery, optimistic revisions with stale-write `409`, membership-filtered SSE create/update/delete push, ETag polling fallback и optional S3-compatible source mirror/hydration с periodic reconciliation готовы локально. Live provider E2E, replication и production tenancy ещё нужны. |
| Templates/social | Hosted stock media, team templates, brand library и direct publishing | в работе | Space-scoped team templates, brand kit и authenticated Pexels photo/video search с required attribution и импортом в composition готовы локально. Authenticated renders получают server-owned output grants (включая cache hits), deletion отзывает их. YouTube connect/callback использует narrow upload scope, hashed consume-once CSRF state и AES-256-GCM token vault с actor-bound AAD. Durable publish jobs проверяют output ownership, автоматически refresh access token, загружают 8 MiB chunks по resumable protocol, сохраняют session/offset зашифрованно, после рестарта сверяются с provider offset, заменяют истёкшую session, поддерживают отмену и возвращают video URL; disconnect сначала отзывает OAuth grant у Google. UI показывает прогресс и privacy. Live provider E2E ещё нужен. |

## Официальные источники CapCut

- Core editor: [Desktop Editor](https://www.capcut.com/tools/desktop-video-editor),
  [Online Editor](https://www.capcut.com/tools/online-video-editor),
  [Creative Suite](https://www.capcut.com/creative-suite),
  [Color Correction](https://www.capcut.com/tools/video-color-correction),
  [Audio Mixing](https://www.capcut.com/tools/audio-mixing),
  [Export](https://www.capcut.com/help/export-videos-in-capcut),
  [Screen Recorder](https://www.capcut.com/tools/online-screen-recorder),
  [Teleprompter](https://www.capcut.com/tools/teleprompt-app).
- AI editing: [Transcript Editing](https://www.capcut.com/tools/video-transcript-editing),
  [Auto Cut](https://www.capcut.com/help/auto-cut-in-capcut),
  [Long Video to Shorts](https://www.capcut.com/tools/long-video-to-shorts),
  [AI Captions](https://www.capcut.com/tools/ai-caption-generator),
  [Bilingual Subtitles](https://www.capcut.com/help/bilingual-subtitles).
- Generative AI: [AI Video Generator](https://www.capcut.com/tools/ai-video-generator),
  [Director Mode](https://www.capcut.com/tools/web-video-studio-director-mode),
  [AI Dialogue](https://www.capcut.com/tools/ai-dialogue-generator),
  [AI Avatar](https://www.capcut.com/tools/ai-avatar),
  [Video Translator](https://www.capcut.com/tools/ai-video-translator),
  [AI Music](https://www.capcut.com/tools/ai-music-generator),
  [AI Design](https://www.capcut.com/tools/ai-design).
- Cloud/templates: [Collaboration](https://www.capcut.com/resource/collaborate-on-capcut-online),
  [Template Support](https://www.capcut.com/help/use-and-export-templates-in-capcut),
  [Template Creation](https://www.capcut.com/help/how-to-create-templates-in-capcut).
