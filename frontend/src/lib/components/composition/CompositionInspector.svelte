<script lang="ts">
  import ProgressBar from '$lib/components/ProgressBar.svelte'
  import { DEFAULT_SILENCE_REMOVAL, detectAudibleRanges } from '$lib/audio/silence.js'
  import { localWaveformCache, type LocalWaveformCache } from '$lib/audio/waveformCache.js'
  import { spaceBrandState } from '$lib/state/spaceBrand.svelte.js'
  import { spacesState } from '$lib/state/spaces.svelte.js'
  import {
    clipDurationTicks,
    clipEndTicks,
    COMPOSITION_BLEND_MODES,
    COMPOSITION_DELIVERY_PROFILE_OPTIONS,
    COMPOSITION_STABILIZATION_RADII,
    COMPOSITION_TIME_BASE,
    COMPOSITION_TRANSITION_KINDS,
    COMPOSITION_VOICE_EFFECTS,
    type CompositionChromaKey,
    type CompositionClip,
    type CompositionDeliveryProfileId,
    type CompositionStabilization,
    type CompositionAnimatableValue,
    type CompositionTransition,
    type CompositionTransitionKind,
    type CompositionVideoMask,
    type CompositionVideoEffectPreset,
    type TextClip,
    type VideoClip,
    type VideoTrack,
    compositionDeliveryProfileOption,
  } from '$lib/composition/types.js'
  import { sampleAnimatableValue, visualPropertyFallback } from '$lib/composition/keyframes.js'
  import { buildAnimationPreset, type CompositionAnimationPreset } from '$lib/composition/animationPresets.js'
  import { minimumCompositionSpeed } from '$lib/composition/speedRamp.js'
  import {
    compositionTransitionUnavailableReason,
    primaryCompositionVideoTrack,
  } from '$lib/composition/validation.js'
  import {
    addTextToComposition,
    applyCompositionTextStyleToTrack,
    applyCompositionVisualAnimation,
    addCompositionVideoMask,
    cancelCompositionExport,
    clearCompositionVisualAnimations,
    compositionState,
    exportComposition,
    freezeCompositionClipAtPlayhead,
    getCompositionExportUnavailableReason,
    removeCompositionTransition,
    removeSilenceFromSelectedCompositionClip,
    deleteCompositionVideoMask,
    detachCompositionVideoAudio,
    selectedCompositionClip,
    selectedCompositionTrack,
    setCompositionTransition,
    setCompositionDeliveryProfile,
    trimCompositionClip,
    updateCompositionAudioMix,
    updateCompositionCanvas,
    updateCompositionBlendMode,
    updateCompositionChromaKey,
    updateCompositionClipGain,
    updateCompositionClipOpacity,
    updateCompositionClipSpeed,
    updateCompositionClipTarget,
    updateCompositionFrameInterpolation,
    updateCompositionExportSettings,
    updateCompositionPlaybackMode,
    updateCompositionRotation,
    updateCompositionStabilization,
    updateCompositionText,
    updateCompositionTextStyle,
    updateCompositionVideoAudio,
    updateCompositionVideoMask,
    updateCompositionVideoEffects,
    updateCompositionVisualTransform,
  } from '$lib/state/composition.svelte.js'
  import { state as legacyState } from '$lib/state/store.svelte.js'
  import CompositionKeyframeEditor from './CompositionKeyframeEditor.svelte'
  import CompositionSpeedRampEditor from './CompositionSpeedRampEditor.svelte'

  interface Props { waveformCache?: Pick<LocalWaveformCache, 'load'> }
  let { waveformCache = localWaveformCache }: Props = $props()

  interface TransitionContext {
    track: VideoTrack
    from: VideoClip
    to: VideoClip
    transition?: CompositionTransition
  }

  const exportReason = $derived(getCompositionExportUnavailableReason(legacyState.capabilities))
  function filterUnavailable(id: string, missing: string): string {
    if (!legacyState.capabilities) return ''
    const feature = legacyState.capabilities.filters?.find((option) => option.id === id)
    return !feature ? missing : feature.available ? '' : feature.reason || missing
  }

  const canvasPresets = [
    { id: '16:9', label: 'YouTube 16:9', width: 1920, height: 1080 },
    { id: '9:16', label: 'TikTok / Reels 9:16', width: 1080, height: 1920 },
    { id: '1:1', label: 'Square 1:1', width: 1080, height: 1080 },
    { id: '4:5', label: 'Instagram 4:5', width: 1080, height: 1350 },
    { id: '4:3', label: 'Classic 4:3', width: 1440, height: 1080 },
  ] as const

  function selectedCanvasPreset(): string {
    const { width, height } = compositionState.document.canvas
    return canvasPresets.find((preset) => preset.width === width && preset.height === height)?.id ?? 'custom'
  }

  function applyCanvasPreset(id: string): void {
    const preset = canvasPresets.find((candidate) => candidate.id === id)
    if (preset) updateCompositionCanvas({ width: preset.width, height: preset.height })
  }
  const duckingUnavailable = $derived(filterUnavailable('audio-ducking', 'Нужен sidechaincompress.'))
  const echoUnavailable = $derived(filterUnavailable('audio-voice-echo', 'Нужен aecho.'))
  const robotUnavailable = $derived(filterUnavailable('audio-voice-robot', 'Нужен tremolo.'))
  const toneUnavailable = $derived(filterUnavailable('audio-tone', 'Нужны bass/treble.'))
  const deliveryOption = $derived(compositionDeliveryProfileOption(compositionState.export.profile))
  const clip = $derived(selectedCompositionClip())
  const track = $derived(selectedCompositionTrack())
  const primaryTrack = $derived(primaryCompositionVideoTrack(compositionState.document))
  const transitionContext = $derived.by(findTransitionContext)
  let transitionKind = $state<CompositionTransitionKind>('dissolve')
  let transitionSeconds = $state(0.5)
  let transitionBoundaryKey = $state('')
  let silenceThresholdDb = $state(DEFAULT_SILENCE_REMOVAL.thresholdDb)
  let minimumSilenceSeconds = $state(DEFAULT_SILENCE_REMOVAL.minimumSilenceSeconds)
  let silencePaddingSeconds = $state(DEFAULT_SILENCE_REMOVAL.paddingSeconds)
  let silenceBusy = $state(false)
  let videoEffectPreset = $state<CompositionVideoEffectPreset>('blur')
  let animationPreset = $state<CompositionAnimationPreset>('fade_in')
  let animationPresetSeconds = $state(0.6)

  const animationPresetLabels: Readonly<Record<CompositionAnimationPreset, string>> = {
    fade_in: 'Fade In',
    fade_out: 'Fade Out',
    slide_left_in: 'Slide Left In',
    slide_right_in: 'Slide Right In',
    zoom_in: 'Zoom In',
    zoom_out: 'Zoom Out',
    pulse_loop: 'Pulse Loop',
    spin_loop: 'Spin Loop',
  }

  const videoEffectLabels: Readonly<Record<CompositionVideoEffectPreset, string>> = {
    blur: 'Blur', pixelate: 'Pixelate', vignette: 'Vignette', sharpen: 'Sharpen', edge: 'Edge',
    rgb_split: 'RGB Split', posterize: 'Posterize',
  }

  function addVideoEffect(clip: VideoClip): void {
    updateCompositionVideoEffects(clip.id, [...(clip.videoEffects ?? []), { preset: videoEffectPreset, intensity: 0.5 }])
  }

  function patchVideoEffect(clip: VideoClip, index: number, intensity: number): void {
    updateCompositionVideoEffects(clip.id, (clip.videoEffects ?? []).map((effect, candidate) =>
      candidate === index ? { ...effect, intensity } : effect))
  }

  function removeVideoEffect(clip: VideoClip, index: number): void {
    updateCompositionVideoEffects(clip.id, (clip.videoEffects ?? []).filter((_, candidate) => candidate !== index))
  }

  function moveVideoEffect(clip: VideoClip, index: number, delta: -1 | 1): void {
    const effects = [...(clip.videoEffects ?? [])]
    const target = index + delta
    if (target < 0 || target >= effects.length) return
    ;[effects[index], effects[target]] = [effects[target]!, effects[index]!]
    updateCompositionVideoEffects(clip.id, effects)
  }

  function applyAnimationPreset(clip: Exclude<CompositionClip, { kind: 'audio' }>): void {
    const duration = clipDurationTicks(clip)
    const properties = ['x', 'y', 'scaleX', 'scaleY', 'rotationDegrees', 'opacity'] as const
    const values = Object.fromEntries(properties.map((property) => [
      property,
      visualPropertyFallback(compositionState.document, clip, property),
    ])) as Record<(typeof properties)[number], number>
    applyCompositionVisualAnimation(clip.id, buildAnimationPreset(animationPreset, {
      clipDurationTicks: duration,
      presetDurationTicks: Math.max(1, Math.min(duration, Math.round(animationPresetSeconds * COMPOSITION_TIME_BASE))),
      canvasWidth: compositionState.document.canvas.width,
      values,
    }))
  }

  $effect(() => {
    const context = transitionContext
    const nextKey = context ? `${context.track.id}:${context.from.id}:${context.to.id}:${context.transition?.id ?? ''}` : ''
    if (nextKey !== transitionBoundaryKey) {
      transitionBoundaryKey = nextKey
      transitionKind = context?.transition?.kind ?? 'dissolve'
      transitionSeconds = context?.transition
        ? seconds(context.transition.durationTicks)
        : Math.min(0.5, transitionHandleSeconds(context)) || 0.5
    }
  })

  const transitionReason = $derived.by(() => {
    const context = transitionContext
    if (!context) return null
    return compositionTransitionUnavailableReason(compositionState.document, context.track.id, {
      id: context.transition?.id ?? 'transition-preview',
      fromClipId: context.from.id,
      toClipId: context.to.id,
      kind: transitionKind,
      durationTicks: Math.max(1, Math.round(transitionSeconds * COMPOSITION_TIME_BASE)),
    })
  })

  function run(action: () => void): void {
    try {
      compositionState.ui.message = ''
      action()
    } catch (error) {
      compositionState.ui.message = error instanceof Error ? error.message : String(error)
    }
  }

  async function removeSelectedSilence(candidate: VideoClip | Extract<CompositionClip, { kind: 'audio' }>): Promise<void> {
    const media = compositionState.media[candidate.sourceId]
    if (!media?.url || silenceBusy) return
    silenceBusy = true
    compositionState.ui.message = ''
    try {
      const summary = await waveformCache.load(media.url)
      const audible = detectAudibleRanges(summary, {
        thresholdDb: silenceThresholdDb,
        minimumSilenceSeconds,
        paddingSeconds: silencePaddingSeconds,
      })
      const count = removeSilenceFromSelectedCompositionClip(audible)
      compositionState.ui.message = `Silence removal: ${count} audible фрагм.`
    } catch (error) {
      compositionState.ui.message = error instanceof Error ? error.message : String(error)
    } finally {
      silenceBusy = false
    }
  }

  function seconds(value: number): number {
    return Number((value / COMPOSITION_TIME_BASE).toFixed(3))
  }

  function ticksFromInput(event: Event): number {
    return Math.max(0, Math.round(Number((event.currentTarget as HTMLInputElement).value) * COMPOSITION_TIME_BASE))
  }

  function audioCrossfadeMaxTicks(candidate: Extract<CompositionClip, { kind: 'audio' }>): number {
    if (track?.kind !== 'audio' || candidate.reversed || candidate.speedRamp || candidate.audioAnimation || (candidate.fadeInTicks ?? 0) > 0) return 0
    const ordered = [...track.clips].sort((left, right) => left.timelineStartTicks - right.timelineStartTicks)
    const index = ordered.findIndex((item) => item.id === candidate.id)
    const previous = ordered[index - 1]
    if (!previous || previous.reversed || clipEndTicks(previous) !== candidate.timelineStartTicks || previous.speedRamp || previous.audioAnimation || (previous.fadeOutTicks ?? 0) > 0) return 0
    const previousSource = compositionState.document.sources[previous.sourceId]
    const currentSource = compositionState.document.sources[candidate.sourceId]
    if (!previousSource || !currentSource) return 0
    const tail = Math.floor((previousSource.durationTicks - previous.sourceOutTicks) / (previous.speed ?? 1))
    const head = Math.floor(candidate.sourceInTicks / (candidate.speed ?? 1))
    return Math.max(0, Math.min(clipDurationTicks(previous), clipDurationTicks(candidate), 2 * tail, 2 * head))
  }

  function audioClipHasCrossfade(candidate: Extract<CompositionClip, { kind: 'audio' }>): boolean {
    if (track?.kind !== 'audio') return false
    if ((candidate.crossfadeInTicks ?? 0) > 0) return true
    return track.clips.some((next) =>
      next.id !== candidate.id
      && next.timelineStartTicks === clipEndTicks(candidate)
      && (next.crossfadeInTicks ?? 0) > 0,
    )
  }

  const DEFAULT_DUCKING = { thresholdDb: -24, ratio: 8, attackMs: 20, releaseMs: 300 } as const

  function setPlaybackMode(candidate: VideoClip, mode: 'forward' | 'reverse' | 'freeze'): void {
    if (mode === 'freeze') freezeCompositionClipAtPlayhead(candidate.id)
    else updateCompositionPlaybackMode(candidate.id, { mode })
  }

  function stabilization(candidate: VideoClip): CompositionStabilization {
    return candidate.stabilization ?? { mode: 'disabled' }
  }

  function stabilizationReason(candidate: VideoClip): string | null {
    if (track?.kind !== 'video' || track.hidden) return 'Deshake доступен только на видимой video-дорожке.'
    if (candidate.playbackMode?.mode === 'freeze') return 'Freeze frame нельзя совмещать с deshake stabilization.'
    const capabilities = legacyState.capabilities
    if (!capabilities) return 'Проверяем поддержку stabilization на сервере…'
    const feature = capabilities.features?.find((option) => option.id === 'stabilization')
    if (!feature) return 'Сервер не объявил поддержку stabilization.'
    if (!feature.available) return feature.reason?.trim() || 'Stabilization недоступна на этом сервере.'
    return null
  }

  function setStabilizationMode(candidate: VideoClip, mode: CompositionStabilization['mode']): void {
    const current = stabilization(candidate)
    updateCompositionStabilization(candidate.id, mode === 'deshake'
      ? {
          mode,
          radiusX: current.mode === 'deshake' ? current.radiusX : 32,
          radiusY: current.mode === 'deshake' ? current.radiusY : 32,
        }
      : { mode: 'disabled' })
  }

  function setStabilizationRadius(
    candidate: VideoClip,
    field: 'radiusX' | 'radiusY',
    value: number,
  ): void {
    const current = stabilization(candidate)
    if (current.mode !== 'deshake') return
    updateCompositionStabilization(candidate.id, { ...current, [field]: value } as CompositionStabilization)
  }

  function playheadInside(candidate: VideoClip): boolean {
    const playhead = compositionState.transport.playheadTicks
    return playhead >= candidate.timelineStartTicks && playhead < clipEndTicks(candidate)
  }

  function inputNumber(event: Event): number {
    return Number((event.currentTarget as HTMLInputElement).value)
  }

  function findTransitionContext(): TransitionContext | null {
    if (!clip || clip.kind !== 'video' || !track || track.kind !== 'video') return null
    const ordered = [...track.clips].sort(
      (left, right) => left.timelineStartTicks - right.timelineStartTicks || left.id.localeCompare(right.id),
    )
    const index = ordered.findIndex((candidate) => candidate.id === clip.id)
    if (index < 1) return null
    const from = ordered[index - 1]!
    if (clipEndTicks(from) !== clip.timelineStartTicks) return null
    return {
      track,
      from,
      to: clip,
      transition: (track.transitions ?? []).find((candidate) => candidate.toClipId === clip.id),
    }
  }

  function transitionHandleSeconds(context: TransitionContext | null): number {
    if (!context) return 0
    if (context.from.speedRamp || context.to.speedRamp) return 0
    const fromSource = compositionState.document.sources[context.from.sourceId]
    const tail = ((fromSource?.durationTicks ?? 0) - context.from.sourceOutTicks) / (context.from.speed ?? 1)
    const head = context.to.sourceInTicks / (context.to.speed ?? 1)
    return Math.max(0, (2 * Math.min(tail, head)) / COMPOSITION_TIME_BASE)
  }

  function saveTransition(): void {
    const context = transitionContext
    if (!context) return
    setCompositionTransition(
      context.track.id,
      context.from.id,
      context.to.id,
      transitionKind,
      Math.round(transitionSeconds * COMPOSITION_TIME_BASE),
      context.transition?.id,
    )
  }

  function visualScalePercent(candidate: CompositionClip): number {
    if (candidate.kind !== 'video' && candidate.kind !== 'image') return 100
    const isPrimary = track?.id === primaryTrack?.id
    const source = compositionState.document.sources[candidate.sourceId]
    const baseWidth = isPrimary ? compositionState.document.canvas.width : source?.width
    return baseWidth ? Number(((candidate.transform.width / baseWidth) * 100).toFixed(2)) : 100
  }

  function updateVisualScale(candidate: VideoClip | Extract<CompositionClip, { kind: 'image' }>, percent: number): void {
    const isPrimary = track?.id === primaryTrack?.id
    const source = compositionState.document.sources[candidate.sourceId]
    const baseWidth = isPrimary ? compositionState.document.canvas.width : source?.width
    const baseHeight = isPrimary ? compositionState.document.canvas.height : source?.height
    if (!baseWidth || !baseHeight) throw new Error('Нет размеров источника для пропорционального scale')
    const scale = Math.max(0.01, Math.min(16, percent / 100))
    updateCompositionVisualTransform(candidate.id, { width: baseWidth * scale, height: baseHeight * scale })
  }

  function chroma(candidate: VideoClip): CompositionChromaKey {
    return candidate.chromaKey ?? {
      enabled: false,
      color: '#00ff00',
      similarity: 0.2,
      softness: 0.08,
      spill: 0.15,
    }
  }

  function patchChroma(candidate: VideoClip, patch: Partial<CompositionChromaKey>): void {
    updateCompositionChromaKey(candidate.id, { ...chroma(candidate), ...patch })
  }

  function patchText(candidate: TextClip, patch: Partial<TextClip['style']>): void {
    updateCompositionTextStyle(candidate.id, patch)
  }

  function maskValue(value: CompositionAnimatableValue, candidate: VideoClip): number {
    const localTicks = Math.max(0, Math.min(
      clipDurationTicks(candidate),
      compositionState.transport.playheadTicks - candidate.timelineStartTicks,
    ))
    return sampleAnimatableValue(value, localTicks)
  }

  function patchMask(candidate: VideoClip, mask: CompositionVideoMask, patch: Parameters<typeof updateCompositionVideoMask>[2]): void {
    updateCompositionVideoMask(candidate.id, mask.id, patch)
  }
</script>

<aside class="composition-inspector card" aria-label="Инспектор композиции">
  <div class="composition-inspector-head">
    <h2>Инспектор</h2>
    <button class="btn ghost sm" type="button" onclick={() => run(() => addTextToComposition())}>+ Текст</button>
  </div>

  <fieldset class="composition-fieldset">
    <legend>Canvas</legend>
    <div class="composition-form-grid">
      <label>
        Соотношение сторон
        <select aria-label="Canvas preset" value={selectedCanvasPreset()} onchange={(event) => run(() => applyCanvasPreset(event.currentTarget.value))}>
          {#each canvasPresets as preset (preset.id)}<option value={preset.id}>{preset.label}</option>{/each}
          <option value="custom">Custom</option>
        </select>
      </label>
      <label>
        Ширина
        <input aria-label="Canvas width" type="number" min="2" max="3840" step="2" value={compositionState.document.canvas.width} onchange={(event) => run(() => updateCompositionCanvas({ width: inputNumber(event) }))} />
      </label>
      <label>
        Высота
        <input aria-label="Canvas height" type="number" min="2" max="2160" step="2" value={compositionState.document.canvas.height} onchange={(event) => run(() => updateCompositionCanvas({ height: inputNumber(event) }))} />
      </label>
      <label>
        Частота кадров
        <select aria-label="Canvas FPS" value={compositionState.document.canvas.fps} onchange={(event) => run(() => updateCompositionCanvas({ fps: Number(event.currentTarget.value) }))}>
          {#each [23.976, 24, 25, 29.97, 30, 50, 59.94, 60] as fps (fps)}<option value={fps}>{fps} fps</option>{/each}
        </select>
      </label>
      <label>
        Тип фона
        <select aria-label="Canvas background mode" value={compositionState.document.canvas.backgroundMode ?? 'color'} onchange={(event) => run(() => updateCompositionCanvas({ backgroundMode: event.currentTarget.value as 'color' | 'blur' | 'checker' }))}>
          <option value="color">Цвет</option>
          <option value="blur">Blur видео</option>
          <option value="checker">Шахматный pattern</option>
        </select>
      </label>
      <label>
        {compositionState.document.canvas.backgroundMode === 'checker' ? 'Базовый цвет pattern' : 'Цвет фона'}
        <input class="composition-color-field" aria-label="Canvas background color" type="color" value={compositionState.document.canvas.backgroundColor.slice(0, 7)} onchange={(event) => run(() => updateCompositionCanvas({ backgroundColor: event.currentTarget.value }))} />
      </label>
      {#if compositionState.document.canvas.backgroundMode === 'blur'}
        <label class="composition-grid-wide">
          Blur {compositionState.document.canvas.backgroundBlur ?? 24}
          <input aria-label="Canvas background blur" type="range" min="1" max="100" step="1" value={compositionState.document.canvas.backgroundBlur ?? 24} oninput={(event) => run(() => updateCompositionCanvas({ backgroundBlur: inputNumber(event) }))} />
        </label>
      {/if}
    </div>
    <p class="composition-help">Color и pattern заполняют пустые области; Blur растягивает активный primary video под contain-клипом. Preview и финальный файл используют один режим. Размеры должны быть чётными.</p>
  </fieldset>

  {#if clip && track}
    <div class="composition-inspector-section">
      <strong>{clip.kind === 'text' ? clip.text : compositionState.media[clip.sourceId]?.filename ?? clip.sourceId}</strong>
      <span class={`composition-kind ${clip.kind}`}>{clip.kind}</span>
    </div>

    <fieldset class="composition-fieldset">
      <legend>Размещение</legend>
      <div class="composition-form-grid">
        <label>
          Дорожка
          <select value={track.id} onchange={(event) => run(() => updateCompositionClipTarget(clip.id, event.currentTarget.value))}>
            {#each compositionState.document.tracks.filter((candidate) => candidate.kind === clip.kind) as candidate (candidate.id)}
              <option value={candidate.id}>{candidate.name}{candidate.locked ? ' · locked' : ''}</option>
            {/each}
          </select>
        </label>
        <label>
          Начало, с
          <input
            type="number"
            min="0"
            step="0.01"
            value={seconds(clip.timelineStartTicks)}
            onchange={(event) => run(() => trimCompositionClip(clip.id, ticksFromInput(event), clipEndTicks(clip)))}
          />
        </label>
        <label>
          Конец, с
          <input
            type="number"
            min="0.01"
            step="0.01"
            value={seconds(clipEndTicks(clip))}
            onchange={(event) => run(() => trimCompositionClip(clip.id, clip.timelineStartTicks, ticksFromInput(event)))}
          />
        </label>
        {#if clip.kind === 'video' || clip.kind === 'audio'}
          <label>
            Скорость
            <input aria-label="Скорость клипа" type="number" min="0.05" max="16" step="0.05" value={clip.speed ?? 1} onchange={(event) => run(() => updateCompositionClipSpeed(clip.id, inputNumber(event)))} />
          </label>
        {/if}
        {#if clip.kind === 'video'}
          <label>
            Режим воспроизведения
            <select aria-label="Режим воспроизведения" value={clip.playbackMode?.mode ?? 'forward'} onchange={(event) => run(() => setPlaybackMode(clip, event.currentTarget.value as 'forward' | 'reverse' | 'freeze'))}>
              <option value="forward">Forward</option>
              <option value="reverse">Reverse</option>
              <option value="freeze" disabled={(clip.playbackMode?.mode ?? 'forward') !== 'freeze' && (!playheadInside(clip) || (clip.frameInterpolation ?? 'duplicate') === 'optical_flow' || clip.stabilization?.mode === 'deshake' || clip.speedRamp !== undefined)}>Freeze</option>
            </select>
          </label>
          <label>
            Интерполяция кадров
            <select aria-label="Интерполяция кадров" value={clip.frameInterpolation ?? 'duplicate'} onchange={(event) => run(() => updateCompositionFrameInterpolation(clip.id, event.currentTarget.value as 'duplicate' | 'optical_flow'))}>
              <option value="duplicate">Duplicate/drop</option>
              <option value="optical_flow" disabled={track.kind !== 'video' || track.hidden || minimumCompositionSpeed(clip.speed ?? 1, clip.speedRamp) >= 1 || clip.playbackMode?.mode === 'freeze'}>Optical flow</option>
            </select>
          </label>
          <div class="composition-inspector-actions composition-grid-wide">
            <button class="btn ghost sm" type="button" disabled={!playheadInside(clip) || (clip.frameInterpolation ?? 'duplicate') === 'optical_flow' || clip.stabilization?.mode === 'deshake' || clip.speedRamp !== undefined} onclick={() => run(() => freezeCompositionClipAtPlayhead(clip.id))}>Freeze at playhead</button>
            {#if clip.playbackMode?.mode === 'freeze'}<output>Source: {seconds(clip.playbackMode.sourceTick)} с</output>{/if}
          </div>
          <p class="composition-help composition-grid-wide">Reverse и Freeze требуют отдельных server capabilities. Freeze фиксирует clip-local source tick, выключает встроенный звук и несовместим с optical flow, stabilization и speed ramp. Reverse/Freeze preview использует seek approximation; экспорт точный.</p>
          <p class="composition-help composition-grid-wide">Optical flow доступен только для slow motion &lt; 1x на видимой video-дорожке, требует отдельной server capability и виден точно только в экспорте.</p>
          <label>
            Стабилизация
            <select aria-label="Стабилизация" value={stabilization(clip).mode} onchange={(event) => run(() => setStabilizationMode(clip, event.currentTarget.value as CompositionStabilization['mode']))}>
              <option value="disabled">Disabled</option>
              <option value="deshake" disabled={stabilization(clip).mode !== 'deshake' && Boolean(stabilizationReason(clip))}>Deshake</option>
            </select>
          </label>
          {#if stabilization(clip).mode === 'deshake'}
            {@const stabilized = stabilization(clip)}
            {#if stabilized.mode === 'deshake'}
              <label>
                Radius X
                <select aria-label="Радиус стабилизации X" value={stabilized.radiusX} disabled={Boolean(stabilizationReason(clip))} onchange={(event) => run(() => setStabilizationRadius(clip, 'radiusX', Number(event.currentTarget.value)))}>
                  {#each COMPOSITION_STABILIZATION_RADII as radius (radius)}<option value={radius}>{radius}</option>{/each}
                </select>
              </label>
              <label>
                Radius Y
                <select aria-label="Радиус стабилизации Y" value={stabilized.radiusY} disabled={Boolean(stabilizationReason(clip))} onchange={(event) => run(() => setStabilizationRadius(clip, 'radiusY', Number(event.currentTarget.value)))}>
                  {#each COMPOSITION_STABILIZATION_RADII as radius (radius)}<option value={radius}>{radius}</option>{/each}
                </select>
              </label>
            {/if}
          {/if}
          {#if stabilizationReason(clip)}
            <p class="composition-inline-error composition-grid-wide" role="status">{stabilizationReason(clip)}</p>
          {:else}
            <p class="composition-help composition-grid-wide">Deshake требует отдельной server capability и применяется точно только в экспорте; canvas-preview его не симулирует.</p>
          {/if}
        {/if}
      </div>
    </fieldset>

    {#if clip.kind === 'video' || clip.kind === 'audio'}
      <CompositionSpeedRampEditor {clip} />
      <fieldset class="composition-fieldset">
        <legend>Silence removal</legend>
        <div class="composition-form-grid">
          <label>Порог, dB<input aria-label="Multitrack silence threshold" type="number" min="-80" max="-6" step="1" bind:value={silenceThresholdDb} /></label>
          <label>Мин. пауза, с<input aria-label="Multitrack minimum silence" type="number" min="0.1" max="10" step="0.05" bind:value={minimumSilenceSeconds} /></label>
          <label>Запас, с<input aria-label="Multitrack silence padding" type="number" min="0" max="2" step="0.01" bind:value={silencePaddingSeconds} /></label>
          <button class="btn ghost sm" type="button" disabled={silenceBusy || !compositionState.document.sources[clip.sourceId]?.hasAudio || clip.kind === 'video' && clip.playbackMode?.mode === 'freeze'} onclick={() => removeSelectedSilence(clip)}>{silenceBusy ? 'Анализ…' : 'Убрать тишину'}</button>
        </div>
        <p class="composition-help">Локальный RMS-анализ нарезает выбранный clip, уплотняет результат и ripple-сдвигает следующие clips этой дорожки. Transition сначала нужно удалить.</p>
      </fieldset>
    {/if}

    {#if clip.kind === 'video' || clip.kind === 'image' || clip.kind === 'text'}
      <fieldset class="composition-fieldset">
        <legend>Визуальный слой</legend>
        <div class="composition-form-grid">
          <label>
            X, px от центра
            <input aria-label="Позиция X" type="number" step="1" value={clip.kind === 'text' ? clip.x : clip.transform.x} onchange={(event) => run(() => updateCompositionVisualTransform(clip.id, { x: inputNumber(event) }))} />
          </label>
          <label>
            Y, px от центра
            <input aria-label="Позиция Y" type="number" step="1" value={clip.kind === 'text' ? clip.y : clip.transform.y} onchange={(event) => run(() => updateCompositionVisualTransform(clip.id, { y: inputNumber(event) }))} />
          </label>
          {#if clip.kind !== 'text'}
            <label>
              Scale, %
              <input aria-label="Масштаб слоя" type="number" min="1" max="1600" step="1" value={visualScalePercent(clip)} onchange={(event) => run(() => updateVisualScale(clip, inputNumber(event)))} />
            </label>
            <label>
              Ширина, px
              <input aria-label="Ширина слоя" type="number" min="1" step="1" value={clip.transform.width} onchange={(event) => run(() => updateCompositionVisualTransform(clip.id, { width: inputNumber(event) }))} />
            </label>
            <label>
              Высота, px
              <input aria-label="Высота слоя" type="number" min="1" step="1" value={clip.transform.height} onchange={(event) => run(() => updateCompositionVisualTransform(clip.id, { height: inputNumber(event) }))} />
            </label>
          {/if}
          <label>
            Поворот, °
            <input aria-label="Поворот слоя" type="number" min="-3600" max="3600" step="1" value={clip.rotationDegrees ?? 0} onchange={(event) => run(() => updateCompositionRotation(clip.id, inputNumber(event)))} />
          </label>
          {#if clip.kind === 'video' || clip.kind === 'image'}
            <label>
              Смешивание
              <select aria-label="Режим смешивания" value={clip.blendMode ?? 'normal'} onchange={(event) => run(() => updateCompositionBlendMode(clip.id, event.currentTarget.value as typeof clip.blendMode & string))}>
                {#each COMPOSITION_BLEND_MODES as mode (mode)}<option value={mode}>{mode}</option>{/each}
              </select>
            </label>
          {/if}
        </div>
        <label class="composition-range-field">
          <span>Прозрачность <output>{Math.round(clip.opacity * 100)}%</output></span>
          <input aria-label="Прозрачность слоя" type="range" min="0" max="1" step="0.01" value={clip.opacity} oninput={(event) => run(() => updateCompositionClipOpacity(clip.id, Number(event.currentTarget.value)))} />
        </label>
        {#if track.id === primaryTrack?.id}
          <p class="composition-help">Основной video поддерживает transform, opacity, chroma, masks и blend modes.</p>
        {/if}
      </fieldset>

      <fieldset class="composition-fieldset">
        <legend>Animation presets</legend>
        <div class="composition-form-grid">
          <label>
            Пресет
            <select aria-label="Animation preset" value={animationPreset} onchange={(event) => { animationPreset = event.currentTarget.value as CompositionAnimationPreset }}>
              {#each Object.entries(animationPresetLabels) as [preset, label] (preset)}<option value={preset}>{label}</option>{/each}
            </select>
          </label>
          <label>
            Длительность, с
            <input aria-label="Animation preset duration" type="number" min="0.05" max={seconds(clipDurationTicks(clip))} step="0.05" value={animationPresetSeconds} disabled={animationPreset.endsWith('_loop')} onchange={(event) => { animationPresetSeconds = inputNumber(event) }} />
          </label>
        </div>
        <div class="composition-inspector-actions">
          <button class="btn primary sm" type="button" onclick={() => run(() => applyAnimationPreset(clip))}>Применить анимацию</button>
          <button class="btn ghost sm" type="button" disabled={!clip.animation} onclick={() => run(() => clearCompositionVisualAnimations(clip.id))}>Очистить анимацию</button>
        </div>
        <p class="composition-help">In/Out меняют затронутые свойства на выбранном краю clip; Loop использует всю длительность. Результат остаётся обычными редактируемыми keyframes.</p>
      </fieldset>

      <CompositionKeyframeEditor {clip} />
    {/if}

    {#if clip.kind === 'text'}
      <fieldset class="composition-fieldset">
        <legend>Текст</legend>
        <label class="composition-wide-field">
          Содержимое
          <textarea rows="4" value={clip.text} oninput={(event) => run(() => updateCompositionText(clip.id, event.currentTarget.value))}></textarea>
        </label>
        <div class="composition-form-grid">
          <label>Шрифт
            <select value={clip.style.fontFamily ?? 'Noto Sans'} onchange={(event) => run(() => patchText(clip, { fontFamily: event.currentTarget.value as NonNullable<TextClip['style']['fontFamily']> }))}>
              <option>Noto Sans</option><option>Arial Unicode MS</option><option>DejaVu Sans</option><option>Arial</option>
            </select>
          </label>
          <label>Размер, px<input type="number" min="1" max="512" value={clip.style.fontSizePx} onchange={(event) => run(() => patchText(clip, { fontSizePx: inputNumber(event) }))} /></label>
          <label>Цвет<input class="composition-color-field" aria-label="Цвет текста" value={clip.style.color} onchange={(event) => run(() => patchText(clip, { color: event.currentTarget.value }))} /></label>
          <label>Фон<input class="composition-color-field" aria-label="Цвет фона текста" value={clip.style.backgroundColor ?? '#00000000'} onchange={(event) => run(() => patchText(clip, { backgroundColor: event.currentTarget.value }))} /></label>
          <label>Выравнивание
            <select value={clip.style.align} onchange={(event) => run(() => patchText(clip, { align: event.currentTarget.value as TextClip['style']['align'] }))}>
              <option value="left">left</option><option value="center">center</option><option value="right">right</option>
            </select>
          </label>
          <label>Обводка, px<input type="number" min="0" max="100" value={clip.style.strokeWidthPx ?? 0} onchange={(event) => run(() => patchText(clip, { strokeWidthPx: inputNumber(event) }))} /></label>
          <label>Цвет обводки<input class="composition-color-field" value={clip.style.strokeColor ?? '#00000000'} onchange={(event) => run(() => patchText(clip, { strokeColor: event.currentTarget.value }))} /></label>
          <label>Цвет тени<input class="composition-color-field" value={clip.style.shadowColor ?? '#00000000'} onchange={(event) => run(() => patchText(clip, { shadowColor: event.currentTarget.value }))} /></label>
          <label>Тень X<input type="number" step="1" value={clip.style.shadowX ?? 0} onchange={(event) => run(() => patchText(clip, { shadowX: inputNumber(event) }))} /></label>
          <label>Тень Y<input type="number" step="1" value={clip.style.shadowY ?? 0} onchange={(event) => run(() => patchText(clip, { shadowY: inputNumber(event) }))} /></label>
        </div>
        {#if spaceBrandState.spaceId === spacesState.selectedId && (spaceBrandState.kit.colors.length || spaceBrandState.kit.fonts.length)}
          <div class="composition-brand-presets" aria-label="Пресеты бренд-кита">
            {#each spaceBrandState.kit.colors as color (color.name)}
              <button type="button" class="btn ghost sm" title={color.value} onclick={() => run(() => patchText(clip, { color: color.value }))}>
                <span class="composition-brand-swatch" style={`background:${color.value}`}></span>{color.name}
              </button>
            {/each}
            {#each spaceBrandState.kit.fonts as font (font)}
              <button type="button" class="btn ghost sm" style={`font-family:${JSON.stringify(font)},sans-serif`} onclick={() => run(() => patchText(clip, { fontFamily: font }))}>{font}</button>
            {/each}
          </div>
        {/if}
        <div class="composition-inspector-actions">
          <button class="btn ghost sm" type="button" onclick={() => run(() => applyCompositionTextStyleToTrack(clip.id))}>Применить стиль ко всей дорожке</button>
        </div>
      </fieldset>
    {/if}

    {#if clip.kind === 'video'}
      <fieldset class="composition-fieldset">
        <legend>Video effects</legend>
        <div class="composition-inspector-actions">
          <select aria-label="Новый video effect" value={videoEffectPreset} onchange={(event) => { videoEffectPreset = event.currentTarget.value as CompositionVideoEffectPreset }}>
            {#each Object.entries(videoEffectLabels) as [preset, label] (preset)}
              <option value={preset}>{label}</option>
            {/each}
          </select>
          <button class="btn ghost sm" type="button" disabled={(clip.videoEffects?.length ?? 0) >= 5} onclick={() => run(() => addVideoEffect(clip))}>+ Эффект</button>
        </div>
        {#each clip.videoEffects ?? [] as effect, index (`${effect.preset}-${index}`)}
          <section class="composition-mask-card" aria-label={`Video effect ${index + 1}`}>
            <div class="composition-mask-head">
              <strong>{videoEffectLabels[effect.preset]}</strong>
              <span class="composition-inspector-actions">
                <button class="btn ghost sm" type="button" aria-label={`Video effect ${index + 1} вверх`} disabled={index === 0} onclick={() => run(() => moveVideoEffect(clip, index, -1))}>↑</button>
                <button class="btn ghost sm" type="button" aria-label={`Video effect ${index + 1} вниз`} disabled={index === (clip.videoEffects?.length ?? 0) - 1} onclick={() => run(() => moveVideoEffect(clip, index, 1))}>↓</button>
                <button class="btn ghost sm danger" type="button" aria-label={`Удалить video effect ${index + 1}`} onclick={() => run(() => removeVideoEffect(clip, index))}>Удалить</button>
              </span>
            </div>
            <label class="composition-range-field">
              <span>Intensity <output>{Math.round(effect.intensity * 100)}%</output></span>
              <input aria-label={`Video effect ${index + 1} intensity`} type="range" min="0.01" max="1" step="0.01" value={effect.intensity} oninput={(event) => run(() => patchVideoEffect(clip, index, Number(event.currentTarget.value)))} />
            </label>
          </section>
        {/each}
        <p class="composition-help">До пяти эффектов применяются сверху вниз после chroma key и перед masks. Blur preview приблизительный; остальные гарантируются финальным FFmpeg export.</p>
      </fieldset>

    {/if}

    {#if clip.kind === 'video'}
      <fieldset class="composition-fieldset">
        <legend>Chroma key</legend>
        <label class="composition-check"><input type="checkbox" checked={chroma(clip).enabled} onchange={(event) => run(() => patchChroma(clip, { enabled: event.currentTarget.checked }))} /> включить chroma key</label>
        <div class="composition-form-grid">
          <label>Цвет<input aria-label="Цвет chroma key" class="composition-color-field" value={chroma(clip).color} onchange={(event) => run(() => patchChroma(clip, { color: event.currentTarget.value }))} /></label>
          <label>Similarity<input type="number" min="0.00001" max="1" step="0.01" value={chroma(clip).similarity} onchange={(event) => run(() => patchChroma(clip, { similarity: inputNumber(event) }))} /></label>
          <label>Softness<input type="number" min="0" max="1" step="0.01" value={chroma(clip).softness} onchange={(event) => run(() => patchChroma(clip, { softness: inputNumber(event) }))} /></label>
          <label>Despill<input type="number" min="0" max="1" step="0.01" value={chroma(clip).spill} onchange={(event) => run(() => patchChroma(clip, { spill: inputNumber(event) }))} /></label>
        </div>
        <p class="composition-help">Chroma key и despill применяются при экспорте; canvas-preview показывает исходный слой.</p>
      </fieldset>
    {/if}

    {#if clip.kind === 'video'}
      <fieldset class="composition-fieldset composition-mask-editor">
        <legend>Masks</legend>
        <div class="composition-inspector-actions">
          <button class="btn ghost sm" type="button" onclick={() => run(() => addCompositionVideoMask(clip.id, 'rectangle'))}>+ Rectangle</button>
          <button class="btn ghost sm" type="button" onclick={() => run(() => addCompositionVideoMask(clip.id, 'ellipse'))}>+ Ellipse</button>
          <button class="btn ghost sm" type="button" onclick={() => run(() => addCompositionVideoMask(clip.id, 'linear'))}>+ Linear</button>
        </div>
        <p class="composition-help">Порядок экспорта фиксирован: chroma key, затем masks. Feather и inverted-контур показываются точно только в экспорте.</p>
        {#each clip.masks ?? [] as mask, index (mask.id)}
          <section class="composition-mask-card" aria-label={`Mask ${index + 1}`}>
            <div class="composition-mask-head">
              <strong>Mask {index + 1}</strong>
              <button class="btn ghost sm danger" type="button" aria-label={`Удалить mask ${index + 1}`} onclick={() => run(() => deleteCompositionVideoMask(clip.id, mask.id))}>Удалить</button>
            </div>
              <div class="composition-form-grid">
                <label>Форма
                  <select aria-label={`Форма mask ${index + 1}`} value={mask.shape} onchange={(event) => run(() => patchMask(clip, mask, { shape: event.currentTarget.value as CompositionVideoMask['shape'] }))}>
                    <option value="rectangle">Rectangle</option>
                    <option value="ellipse">Ellipse</option>
                    <option value="linear">Linear</option>
                  </select>
                </label>
                <label>Center X<input aria-label={`Mask ${index + 1} X`} type="number" min="0" max="1" step="0.01" value={maskValue(mask.x, clip)} onchange={(event) => run(() => patchMask(clip, mask, { x: inputNumber(event) }))} /></label>
                <label>Center Y<input aria-label={`Mask ${index + 1} Y`} type="number" min="0" max="1" step="0.01" value={maskValue(mask.y, clip)} onchange={(event) => run(() => patchMask(clip, mask, { y: inputNumber(event) }))} /></label>
                <label>Angle<input aria-label={`Mask ${index + 1} angle`} type="number" min="-180" max="180" step="1" value={maskValue(mask.rotationDegrees ?? { mode: 'constant', value: 0 }, clip)} onchange={(event) => run(() => patchMask(clip, mask, { rotationDegrees: inputNumber(event) }))} /></label>
                {#if mask.shape !== 'linear'}
                  <label>Width<input aria-label={`Mask ${index + 1} width`} type="number" min="0.01" max="2" step="0.01" value={maskValue(mask.width, clip)} onchange={(event) => run(() => patchMask(clip, mask, { width: inputNumber(event) }))} /></label>
                  <label>Height<input aria-label={`Mask ${index + 1} height`} type="number" min="0.01" max="2" step="0.01" value={maskValue(mask.height, clip)} onchange={(event) => run(() => patchMask(clip, mask, { height: inputNumber(event) }))} /></label>
                {/if}
                <label>Feather (export-only)<input aria-label={`Mask ${index + 1} feather`} type="number" min="0" max="1" step="0.01" value={mask.feather} onchange={(event) => run(() => patchMask(clip, mask, { feather: inputNumber(event) }))} /></label>
              </div>
              <label class="composition-check"><input aria-label={`Инвертировать mask ${index + 1}`} type="checkbox" checked={mask.inverted} onchange={(event) => run(() => patchMask(clip, mask, { inverted: event.currentTarget.checked }))} /> инвертировать</label>
              <CompositionKeyframeEditor {clip} mode="mask" {mask} />
          </section>
        {/each}
        {#if !(clip.masks?.length)}<p class="composition-help">Rectangle, Ellipse и Linear используют normalized clip-local координаты 0…1.</p>{/if}
      </fieldset>
    {/if}

    {#if clip.kind === 'audio'}
      <fieldset class="composition-fieldset">
        <legend>Аудио</legend>
        <div class="composition-form-grid">
          <label>Gain<input aria-label="Громкость аудиоклипа" type="number" min="0" max="16" step="0.05" value={clip.gain} onchange={(event) => run(() => updateCompositionClipGain(clip.id, inputNumber(event)))} /></label>
          <label>Pan<input aria-label="Панорама аудиоклипа" type="number" min="-1" max="1" step="0.05" value={clip.pan ?? 0} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { pan: inputNumber(event) }))} /></label>
          <label class="composition-check"><input aria-label="Reverse audio" type="checkbox" checked={clip.reversed ?? false} disabled={audioClipHasCrossfade(clip)} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { reversed: event.currentTarget.checked }))} /> reverse</label>
          <label>Fade in, с<input aria-label="Fade in" type="number" min="0" max={seconds(clipDurationTicks(clip))} step="0.01" value={seconds(clip.fadeInTicks ?? 0)} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { fadeInTicks: ticksFromInput(event) }))} /></label>
          <label>Fade out, с<input aria-label="Fade out" type="number" min="0" max={seconds(clipDurationTicks(clip))} step="0.01" value={seconds(clip.fadeOutTicks ?? 0)} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { fadeOutTicks: ticksFromInput(event) }))} /></label>
          <label>Voice effect<select aria-label="Voice effect" value={clip.voiceEffect ?? 'none'} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { voiceEffect: event.currentTarget.value as typeof COMPOSITION_VOICE_EFFECTS[number] }))}>{#each COMPOSITION_VOICE_EFFECTS as effect (effect)}<option value={effect} disabled={(effect === 'echo' && Boolean(echoUnavailable)) || (effect === 'robot' && Boolean(robotUnavailable))}>{effect}</option>{/each}</select></label>
          <label>Pitch, semitones<input aria-label="Pitch semitones" type="number" min="-12" max="12" step="1" value={clip.pitchSemitones ?? 0} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { pitchSemitones: inputNumber(event) }))} /></label>
          <label title={toneUnavailable}>Tone, dB<input aria-label="Tone dB" type="number" min="-12" max="12" step="1" disabled={Boolean(toneUnavailable)} value={clip.toneDb ?? 0} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { toneDb: inputNumber(event) }))} /></label>
          <label>Crossfade с предыдущим, с<input aria-label="Audio crossfade" type="number" min="0" max={seconds(audioCrossfadeMaxTicks(clip))} step="0.01" disabled={audioCrossfadeMaxTicks(clip) === 0} value={seconds(clip.crossfadeInTicks ?? 0)} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { crossfadeInTicks: ticksFromInput(event) }))} /></label>
        </div>
        <label class="composition-check"><input aria-label="Auto ducking" type="checkbox" disabled={Boolean(duckingUnavailable)} checked={Boolean(clip.ducking)} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { ducking: event.currentTarget.checked ? DEFAULT_DUCKING : undefined }))} /> приглушать под primary voice</label>
        {#if duckingUnavailable}<p class="composition-help" role="status">{duckingUnavailable}</p>{/if}
        {#if clip.ducking}
          <div class="composition-form-grid">
            <label>Ducking threshold, dB<input aria-label="Ducking threshold" type="number" min="-60" max="0" step="1" value={clip.ducking.thresholdDb} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { ducking: { ...clip.ducking!, thresholdDb: inputNumber(event) } }))} /></label>
            <label>Ducking ratio<input aria-label="Ducking ratio" type="number" min="1" max="20" step="0.5" value={clip.ducking.ratio} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { ducking: { ...clip.ducking!, ratio: inputNumber(event) } }))} /></label>
            <label>Ducking attack, ms<input aria-label="Ducking attack" type="number" min="0.1" max="500" step="1" value={clip.ducking.attackMs} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { ducking: { ...clip.ducking!, attackMs: inputNumber(event) } }))} /></label>
            <label>Ducking release, ms<input aria-label="Ducking release" type="number" min="1" max="5000" step="10" value={clip.ducking.releaseMs} onchange={(event) => run(() => updateCompositionAudioMix(clip.id, { ducking: { ...clip.ducking!, releaseMs: inputNumber(event) } }))} /></label>
          </div>
        {/if}
        <CompositionKeyframeEditor {clip} mode="audio" />
        <p class="composition-help">Fade и animated gain слышны в preview; stereo pan, voice effect и auto ducking применяются точно при экспорте.</p>
      </fieldset>
    {:else if clip.kind === 'video'}
      <fieldset class="composition-fieldset" disabled={track.id !== primaryTrack?.id || !compositionState.document.sources[clip.sourceId]?.hasAudio || clip.playbackMode?.mode === 'freeze'}>
        <legend>Source audio</legend>
        <label class="composition-check"><input aria-label="Использовать встроенный звук" type="checkbox" checked={clip.sourceAudioEnabled} disabled={!compositionState.document.sources[clip.sourceId]?.hasAudio || clip.playbackMode?.mode === 'freeze'} onchange={(event) => run(() => updateCompositionVideoAudio(clip.id, { sourceAudioEnabled: event.currentTarget.checked }))} /> использовать встроенный звук</label>
        <div class="composition-form-grid">
          <label>Gain<input aria-label="Громкость source audio" type="number" min="0" max="16" step="0.05" value={clip.audioGain} onchange={(event) => run(() => updateCompositionClipGain(clip.id, inputNumber(event)))} /></label>
          <label>Pan<input aria-label="Панорама source audio" type="number" min="-1" max="1" step="0.05" value={clip.audioPan ?? 0} onchange={(event) => run(() => updateCompositionVideoAudio(clip.id, { audioPan: inputNumber(event) }))} /></label>
        </div>
        <button class="btn ghost sm" type="button" disabled={!clip.sourceAudioEnabled || clip.playbackMode?.mode === 'freeze' || clip.speedRamp?.audioPolicy === 'mute'} onclick={() => run(() => detachCompositionVideoAudio(clip.id))}>Отделить звук</button>
        {#if track.id === primaryTrack?.id}<CompositionKeyframeEditor {clip} mode="audio" />{/if}
        <p class="composition-help">{clip.playbackMode?.mode === 'freeze' ? 'Freeze frame всегда экспортируется без embedded source audio.' : track.id === primaryTrack?.id ? 'Mute, enable и animated gain слышны в preview; stereo pan применяется точно при экспорте.' : 'Backend использует embedded audio только у primary video. Параметры сохранятся, но для overlay вынесите звук на audio track.'}</p>
      </fieldset>
    {/if}

    {#if clip.kind === 'video'}
      <fieldset class="composition-fieldset composition-transition-editor">
        <legend>Переход с предыдущего клипа</legend>
        {#if transitionContext}
          <div class="composition-form-grid">
            <label>Тип
              <select aria-label="Тип перехода" value={transitionKind} onchange={(event) => { transitionKind = event.currentTarget.value as CompositionTransitionKind }}>
                {#each COMPOSITION_TRANSITION_KINDS as kind (kind)}<option value={kind}>{kind}</option>{/each}
              </select>
            </label>
            <label>Длительность, с<input aria-label="Длительность перехода" type="number" min="0.001" max="30" step="0.05" value={transitionSeconds} oninput={(event) => { transitionSeconds = Number(event.currentTarget.value) }} /></label>
          </div>
          <p class="composition-help">Доступно по source handles примерно до {transitionHandleSeconds(transitionContext).toFixed(3)} с. Граница клипов должна быть общей; handles не меняют authored duration.</p>
          {#if transitionReason}<p class="composition-inline-error" role="status">{transitionReason}</p>{/if}
          <div class="composition-inspector-actions">
            <button class="btn primary sm" type="button" disabled={Boolean(transitionReason)} onclick={() => run(saveTransition)}>{transitionContext.transition ? 'Обновить переход' : 'Добавить переход'}</button>
            {#if transitionContext.transition}<button class="btn ghost sm danger" type="button" onclick={() => run(() => removeCompositionTransition(transitionContext!.track.id, transitionContext!.transition!.id))}>Удалить переход</button>{/if}
          </div>
        {:else}
          <p class="composition-help">Выберите второй или следующий video clip на точной общей границе с предыдущим.</p>
        {/if}
      </fieldset>
    {/if}
  {:else}
    <p class="composition-inspector-empty">Выберите клип на монтажной линии. Изменения сохраняются локально автоматически.</p>
  {/if}

  <div class="composition-export-panel">
    <h2>Экспорт</h2>
    <label>
      Формат и кодек
      <select
        aria-label="Delivery profile"
        value={deliveryOption.id}
        disabled={compositionState.export.running}
        onchange={(event) => run(() => setCompositionDeliveryProfile(event.currentTarget.value as CompositionDeliveryProfileId))}
      >
        {#each COMPOSITION_DELIVERY_PROFILE_OPTIONS as option (option.id)}
          <option value={option.id}>{option.label} · .{option.extension}</option>
        {/each}
      </select>
    </label>
    <label>
      Качество
      <select
        aria-label="Качество delivery"
        value={compositionState.export.qualityTier}
        disabled={compositionState.export.running || deliveryOption.profile.container === 'audio' || compositionState.export.videoBitrateKbps !== null}
        onchange={(event) => run(() => updateCompositionExportSettings({ qualityTier: event.currentTarget.value as 'high' | 'medium' | 'compact' }))}
      >
        <option value="high">Высокое</option>
        <option value="medium">Среднее</option>
        <option value="compact">Компактное</option>
      </select>
    </label>
    {#if deliveryOption.profile.container === 'mp4' || deliveryOption.profile.container === 'webm'}
      <label class="composition-check composition-grid-wide">
        <input
          type="checkbox"
          aria-label="Custom video bitrate"
          checked={compositionState.export.videoBitrateKbps !== null}
          disabled={compositionState.export.running}
          onchange={(event) => run(() => updateCompositionExportSettings({
            videoBitrateKbps: event.currentTarget.checked ? 12_000 : undefined,
          }))}
        />
        Custom bitrate
      </label>
      {#if compositionState.export.videoBitrateKbps !== null}
        <label class="composition-grid-wide">
          Video bitrate, Kbps
          <input
            type="number"
            aria-label="Video bitrate Kbps"
            min="100"
            max="200000"
            step="100"
            value={compositionState.export.videoBitrateKbps}
            disabled={compositionState.export.running}
            onchange={(event) => run(() => updateCompositionExportSettings({
              videoBitrateKbps: Number(event.currentTarget.value),
            }))}
          />
        </label>
      {/if}
    {/if}
    <p class="composition-help" aria-live="polite">
      Файл .{deliveryOption.extension} · video {deliveryOption.videoCodec} · audio {deliveryOption.audioCodec}
      {deliveryOption.profile.container === 'audio' ? ' · итоговый multitrack mix' : ''}
    </p>
    {#if compositionState.export.rangeInTicks !== null || compositionState.export.rangeOutTicks !== null}
      <p class="composition-help" role="status">
        Диапазон: In {compositionState.export.rangeInTicks === null ? '—' : `${(compositionState.export.rangeInTicks / COMPOSITION_TIME_BASE).toFixed(3)}s`}
        · Out {compositionState.export.rangeOutTicks === null ? '—' : `${(compositionState.export.rangeOutTicks / COMPOSITION_TIME_BASE).toFixed(3)}s`}
      </p>
    {/if}
    {#if exportReason}<p class="composition-limit-note" role="status">{exportReason}</p>{/if}
    <button
      class="btn primary composition-export-button"
      disabled={compositionState.export.running || Boolean(exportReason)}
      title={exportReason ?? `Экспортировать ${deliveryOption.label} (.${deliveryOption.extension})`}
      onclick={() => void exportComposition(legacyState.capabilities)}
    >
      {compositionState.export.running ? 'Экспортирую…' : `Экспорт .${deliveryOption.extension}`}
    </button>
    {#if compositionState.export.running}
      <ProgressBar
        progress={compositionState.export.progress}
        stage={compositionState.export.stage}
        cancellable
        oncancel={() => void cancelCompositionExport()}
      />
    {/if}
    {#if compositionState.export.error}<p class="error" role="alert">{compositionState.export.error}</p>{/if}
    {#if compositionState.export.result}
      <a class="btn ghost composition-download" href={compositionState.export.result.url} download={compositionState.export.result.filename}>Скачать {compositionState.export.result.filename}</a>
    {/if}
  </div>
</aside>
