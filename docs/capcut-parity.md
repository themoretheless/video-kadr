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
| Single-clip edit | Trim, middle cut, crop, resize, rotate, mirror, reverse, speed, fade | готово | Один source, плоский edit plan, экспорт через FFmpeg. |
| Color | Brightness/contrast/saturation, HSL, wheels, looks, LUT, RGB/Master curves, scopes, denoise, sharpen, grain | готово | Детерминированный FFmpeg export; browser scopes локальны, export-only эффекты помечены явно. |
| Keying | Chroma key и подавление chroma spill | готово | Key работает в single-clip и composition overlays; доступность fail-closed по filters `chromakey`/`despill`. |
| Audio basics | Mute, gain/pan, fades, loudness, high-pass, EQ, compressor/limiter и voiceover | готово | Browser recording/DSP и single-clip export не используют модели или remote inference. |
| Export | MP4/WebM/AV1/ProRes/GIF/still/MP3, codec/quality/FPS и social presets | готово | Локальный file export; прямой publish в соцсети не подключён. |
| Ordered timeline | Split, duplicate, delete и reorder диапазонов одного source | готово | Порядок массива — порядок playback/export; duplicate и overlap намеренно повторяют материал. До 512 фрагментов и 24 часов post-speed output. |
| Preview compare | Переключение «Оригинал / С правками» | готово | Сравнивается browser preview; финальная истина export-only эффектов остаётся в FFmpeg-результате. |
| Capture | Screen/window/tab, webcam PiP, mic/system audio, annotations и teleprompter | готово | Permissions, device selection, countdown/pause/resume и bounded local upload реализованы; region зависит от системного picker. |
| Core timeline | Несколько video/audio/image/text tracks, несколько source clips, merge, markers и snapping | готово | Versioned composition, immutable commands, ripple delete, track magnet, shortcuts, durable render jobs и v2 projects работают end-to-end. |
| Layers | Overlays, opacity, Rectangle/Ellipse masks, blend modes и keyframes/graph curves | готово | Static и animated transforms/opacity/masks компилируются; неоднозначный `Linear` mask fail-closed. |
| Playback/motion | Speed curves, transitions, reverse/freeze, optical-flow slow motion, stabilization и classical point tracking | готово | Hold/Linear speed ramps с точной reciprocal-integral длительностью и preserve-pitch/mute audio policy, шесть transitions, reverse/freeze, real-tested minterpolate/deshake и bounded ZNCC tracker работают без моделей. |
| Multicam | Audio/manual sync, angle viewer, live switch recording и program cut list | готово | 2–8 angles, local waveform correlation, continuous master audio и deterministic owned-only EDL rebuild. |
| Performance | Production proxies, preview selection, thumbnails/waveforms/filmstrip и cache cleanup | готово | H.264/ProRes proxies с jobs/cancel/delete, verified proxy fallback/status в composition preview, immutable thumbnails и 8-frame hover scrub работают; final export всегда читает originals. |
| Advanced audio | Multi-track mixer, gain/pan automation, fades, solo/mute, limiter и Auto Beat | готово | Video source audio и independent audio clips компилируются с bounded keyframes; локальный energy-flux detector добавляет typed beat markers, learned separation/denoise исключены. |
| Text | Text layers/templates, manual captions, subtitle import/export и styling | готово | Private UTF-8 textfiles, font allowlist, Unicode, SRT round-trip и local templates работают без ASR. |
| Composition delivery | MP4/H.264/H.265, WebM VP9/AV1 и MOV ProRes profiles | готово | Canonical output profiles, legacy MP4/H.264 compatibility, request-specific capability checks, truthful extensions/codecs и реальная ffprobe-матрица работают end-to-end. |
| Local assets/templates | Favorites/tags/search, reusable templates и portable packages | готово | JSON templates со replaceable slots и `.veproj` package не содержат произвольных путей. Hosted licensed catalog остаётся отдельной задачей. |
| Captions/transcript | Auto captions, bilingual subtitles, transcript editing, filler/silence removal | исключено (AI) | ASR, learned alignment и translation не реализуются в текущем scope. |
| Assisted editing | Auto Cut и long-video-to-shorts с highlights, reframing и captions | исключено (AI) | Семантический/model pipeline намеренно отсутствует. |
| AI media tools | Learned background removal, relight, upscale, generative fill и semantic search | исключено (AI) | Chroma key, ручные masks и классические CV/DSP остаются разрешёнными. |
| Generative creation | Script/storyboard, text/image/script-to-video, Director Mode и dialogue scenes | исключено (AI) | Генеративные providers и заглушки не добавляются. |
| Generative audio/design | AI music/stickers/images, product scenes и restoration models | исключено (AI) | Локальные лицензированные assets/templates остаются в scope. |
| Voice/avatar | TTS, voice cloning, AI dubbing, avatars и lip-sync | исключено (AI) | Обычная voiceover recording и DSP voice effects остаются в scope. |
| Cloud/team | Cross-device sync, Spaces, roles, edit handoff, review links/comments и permissions | запланировано | Это отдельный non-AI surface: auth, ownership, tenancy, object storage и audit. |
| Templates/social | Hosted stock media, team templates, brand library и direct publishing | запланировано | Локальные templates готовы; catalog/licensing и platform OAuth/API требуют отдельной инфраструктуры, но не AI. |

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
