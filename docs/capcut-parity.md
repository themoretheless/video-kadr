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
- `этот срез` — ordered timeline одного source и связанный preview UX,
  реализованные в текущей версии.
- `следующий` — локальная функция редактора, которую можно строить после её
  зависимости.
- `внешний AI-cloud` — требует моделей, вычислительного/provider слоя, аккаунтов
  или облачной инфраструктуры за пределами текущего local-first приложения.

Порядок реализации: **core timeline → layers/audio/text → AI → cloud**. Этот срез
не является мультитреком: он переставляет и повторяет диапазоны одного исходного
клипа; отдельные video/audio/image/text tracks начинаются на следующем этапе.

## Нормализованная матрица

| Область | Нормализованная возможность CapCut | Статус | Граница репозитория |
|---|---|---|---|
| Media | Импорт локального файла/URL, медиатека, сохранение проекта | готово | `yt-dlp`, upload, library и autosave/restore работают локально. |
| Single-clip edit | Trim, middle cut, crop, resize, rotate, mirror, reverse, speed, fade | готово | Один source, плоский edit plan, экспорт через FFmpeg. |
| Color | Brightness/contrast/saturation, looks, LUT, RGB/Master curves, denoise, sharpen, grain | готово | LUT/curves точны в экспорте; browser preview для них ограничен. |
| Keying | Chroma key и подавление chroma spill | готово | Одноцветный key на single source; результат виден в FFmpeg-экспорте, доступность fail-closed по filters `chromakey`/`despill`. |
| Audio basics | Mute, gain, fades, loudness normalization, high-pass, audio export | готово | Нет отдельных дорожек, mixer automation или vocal tools. |
| Export | MP4/WebM/AV1/ProRes/GIF/still/MP3, codec/quality/FPS и social presets | готово | Локальный file export; прямой publish в соцсети не подключён. |
| Ordered timeline | Split, duplicate, delete и reorder диапазонов одного source | этот срез | Порядок массива — порядок playback/export; duplicate и overlap намеренно повторяют материал. До 512 фрагментов и 24 часов post-speed output. |
| Preview compare | Переключение «Оригинал / С правками» | этот срез | Сравнивается browser preview; финальная истина export-only эффектов остаётся в FFmpeg-результате. |
| Capture | Screen/window/region, webcam, mic/system audio, annotations и teleprompter | следующий | Нужны capture permissions, device selection и отдельный recording workflow. |
| Core timeline | Несколько video/audio/image/text tracks, несколько source clips, merge и snapping | следующий | Нужны composition DTO, multi-input compiler, project migration и track UI. |
| Layers | Overlays, opacity, masks, blend modes и keyframes/graph curves | следующий | Строится поверх настоящего track/layer IR, не новыми полями single-source DTO. |
| Motion | Transitions, stabilization, optical-flow slow motion, motion tracking и auto reframe | следующий | Сначала timeline scopes/keyframes; ML-варианты tracking/reframe могут перейти во внешний слой. |
| Advanced color | HSL, color wheels, scopes, auto-adjust и color match | следующий | Scopes и ручная коррекция локальны; AI auto/color-match требуют отдельной оценки. |
| Advanced audio | Multi-track mixer, automation, noise reduction, voice enhancement, vocal isolation, music/SFX | следующий | Track mixer предшествует speech/voice моделям. |
| Text | Text layers/templates, manual captions, subtitle import/export и styling | следующий | Сначала timed-text track и font/subtitle packaging. |
| Captions/transcript | Auto captions, bilingual subtitles, transcript editing, filler/silence removal | внешний AI-cloud | Нужны ASR, alignment, translation и model governance. |
| Assisted editing | Auto Cut и long-video-to-shorts с highlights, reframing и captions | внешний AI-cloud | Нужны analysis/model pipeline и evaluation corpus поверх готового timeline. |
| AI media tools | Background removal, relight, upscale, generative fill и semantic media search | внешний AI-cloud | Не имитировать локальными заглушками; нужен явный provider contract. |
| Generative creation | Script/storyboard, text/image/script-to-video, Director Mode и dialogue scenes | внешний AI-cloud | Отдельный generative workflow, assets, branches, credits и provenance. |
| Generative audio/design | AI music/stickers/images, product scenes, virtual try-on и restoration tools | внешний AI-cloud | Нужны media-specific providers, licensing, moderation и provenance. |
| Voice/avatar | TTS, voice cloning/changer, dubbing/translation, avatars и lip-sync | внешний AI-cloud | Биометрические consent/privacy и provider policies обязательны. |
| Cloud/team | Cross-device sync, Spaces, roles, edit handoff, review links/comments и permissions | внешний AI-cloud | Требуются identity, ownership, tenancy, object storage и realtime collaboration. |
| Templates/social | Stock media, community/team templates, brand library и direct publishing | внешний AI-cloud | Требуются catalog/licensing, moderation и внешние platform APIs. |

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
