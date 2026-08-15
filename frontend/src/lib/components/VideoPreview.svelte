<script lang="ts">
  import RectOverlay from './RectOverlay.svelte'
  import ColorScopes from './edit/ColorScopes.svelte'
  import { formatProxyProfile } from '$lib/proxy/model.js'
  import { proxyController } from '$lib/proxy/state.svelte.js'
  import {
    enterOrderedTimeline,
    remapOrderedTimelineCursor,
    stepOrderedTimeline,
  } from './videoPreviewPlayback.js'
  import {
    activeTimelineSegments,
    beginEditTransaction,
    endEditTransaction,
    isIdentityColorWheels,
    isIdentityCurves,
    isIdentitySelectiveHsl,
    state as appState,
  } from '$lib/state/store.svelte.js'

  let videoEl = $state<HTMLVideoElement>()
  let timelineSegmentIndex = $state(0)
  let timelineSegmentId = $state('')
  let showOriginal = $state(false)
  let previousVideoId: string | undefined
  let previousPlaybackUrl: string | undefined
  let ensuredProxySourceId: string | undefined
  let previousPlayToggle = appState.playToggle

  let proxyPlayback = $derived.by(() => {
    const video = appState.video
    return video ? proxyController.playback(video.id, video.url) : null
  })
  let previewUrl = $derived(proxyPlayback?.url)
  let proxyIndicator = $derived.by(() => {
    if (proxyPlayback?.kind === 'proxy' && proxyPlayback.artifact) {
      return `Proxy · ${formatProxyProfile(proxyPlayback.artifact.profile)}`
    }
    return 'Original'
  })
  let proxyStatusDetail = $derived.by(() => {
    if (!appState.video) return ''
    const proxyUi = proxyController.state.sources[appState.video.id]
    return proxyUi?.playbackError || proxyUi?.error || proxyPlayback?.fallbackReason || ''
  })

  function previewDuration(element: HTMLVideoElement): number {
    const sourceDuration = appState.video?.duration
    return typeof sourceDuration === 'number' && Number.isFinite(sourceDuration) && sourceDuration > 0
      ? sourceDuration
      : Number.isFinite(element.duration) && element.duration > 0 ? element.duration : 0
  }
  function seekPreview(element: HTMLVideoElement, time: number): void {
    if (!Number.isFinite(time)) return
    element.currentTime = time
    appState.playerTime = time
  }
  function setTimelineCursor(segments: readonly { id: string }[], index: number): void {
    timelineSegmentIndex = index
    timelineSegmentId = segments[index]?.id ?? ''
  }
  function alignTimelinePlayback(element: HTMLVideoElement): void {
    const segments = activeTimelineSegments(appState.edit, previewDuration(element))
    if (!segments.length) return
    const preferred = remapOrderedTimelineCursor(segments, timelineSegmentId, timelineSegmentIndex)
    const cursor = enterOrderedTimeline(segments, element.currentTime, preferred)
    setTimelineCursor(segments, cursor.index)
    if (cursor.seekTo != null) seekPreview(element, cursor.seekTo)
  }
  function applyPlayback(element: HTMLVideoElement): void {
    const speed = showOriginal ? 1 : appState.edit.speed
    if (speed > 0 && element.playbackRate !== speed) element.playbackRate = speed
    const volume = showOriginal ? 1 : Math.min(1, Math.max(0, appState.edit.volume))
    if (element.volume !== volume) element.volume = volume
  }
  function onTimeUpdate(): void {
    if (!videoEl) return
    appState.playerTime = videoEl.currentTime
    applyPlayback(videoEl)
    if (showOriginal || videoEl.paused) return
    if (appState.edit.timelineEnabled) {
      const segments = activeTimelineSegments(appState.edit, previewDuration(videoEl))
      if (segments.length) {
        const cursor = stepOrderedTimeline(segments, videoEl.currentTime, timelineSegmentIndex)
        setTimelineCursor(segments, cursor.index)
        if (cursor.seekTo != null) seekPreview(videoEl, cursor.seekTo)
        return
      }
    }
    if (videoEl.currentTime > appState.edit.trimEnd) videoEl.currentTime = appState.edit.trimStart
  }
  function onPlay(): void {
    if (!videoEl) return
    applyPlayback(videoEl)
    if (!showOriginal && appState.edit.timelineEnabled) alignTimelinePlayback(videoEl)
  }
  function onEnded(): void {
    if (!videoEl || showOriginal || !appState.edit.timelineEnabled) return
    const segments = activeTimelineSegments(appState.edit, previewDuration(videoEl))
    if (!segments.length) return
    const current = segments[timelineSegmentIndex]
    const cursor = stepOrderedTimeline(segments, current?.end ?? videoEl.currentTime, timelineSegmentIndex)
    setTimelineCursor(segments, cursor.index)
    if (cursor.seekTo != null) seekPreview(videoEl, cursor.seekTo)
    void videoEl.play()
  }

  let timelinePlaybackKey = $derived(appState.video && appState.edit.timelineEnabled
    ? activeTimelineSegments(appState.edit, appState.video.duration).map((segment) => `${segment.id}:${segment.start}:${segment.end}`).join('|')
    : '')

  $effect(() => {
    if (videoEl) {
      applyPlayback(videoEl)
      if (!showOriginal && appState.edit.timelineEnabled) alignTimelinePlayback(videoEl)
    }
  })
  $effect(() => {
    if (!timelinePlaybackKey || !appState.video || !appState.edit.timelineEnabled) setTimelineCursor([], 0)
    else {
      const segments = activeTimelineSegments(appState.edit, appState.video.duration)
      setTimelineCursor(segments, remapOrderedTimelineCursor(segments, timelineSegmentId, timelineSegmentIndex))
      if (videoEl && !showOriginal) alignTimelinePlayback(videoEl)
    }
  })
  $effect(() => {
    const sourceId = appState.video?.id
    if (sourceId !== previousVideoId) {
      previousVideoId = sourceId
      setTimelineCursor([], 0)
      showOriginal = false
    }
  })
  $effect(() => {
    const sourceId = appState.video?.id
    if (sourceId && sourceId !== ensuredProxySourceId) {
      ensuredProxySourceId = sourceId
      proxyController.ensure(sourceId)
    }
  })
  $effect(() => {
    if (previewUrl !== previousPlaybackUrl) {
      previousPlaybackUrl = previewUrl
      videoEl?.load()
    }
  })
  $effect(() => {
    const requested = appState.seekTo
    if (requested != null && videoEl) {
      if (appState.edit.timelineEnabled) {
        const segments = activeTimelineSegments(appState.edit, previewDuration(videoEl))
        const index = appState.seekTimelineSegmentId ? segments.findIndex((segment) => segment.id === appState.seekTimelineSegmentId) : -1
        const cursor = enterOrderedTimeline(segments, requested, index >= 0 ? index : null)
        setTimelineCursor(segments, cursor.index)
      }
      videoEl.currentTime = requested
      appState.seekTo = null
      appState.seekTimelineSegmentId = null
    }
  })
  $effect(() => {
    const value = appState.playToggle
    if (value !== previousPlayToggle && videoEl) {
      previousPlayToggle = value
      if (videoEl.paused) void videoEl.play()
      else videoEl.pause()
    }
  })

  const presets: Record<string, string> = {
    grayscale: 'grayscale(1)', sepia: 'sepia(0.6)', warm: 'sepia(0.4) saturate(1.3)',
    cold: 'hue-rotate(-12deg) saturate(1.15)', 'teal-orange': 'contrast(1.1) saturate(1.2) hue-rotate(-6deg)',
    faded: 'contrast(0.85) brightness(1.05) saturate(0.9)', noir: 'grayscale(1) contrast(1.4)',
    vintage: 'sepia(0.3) contrast(0.95) saturate(1.1)',
  }
  let videoFilter = $derived.by(() => {
    if (showOriginal) return ''
    const filters: string[] = []
    if (appState.edit.brightness) filters.push(`brightness(${(1 + appState.edit.brightness).toFixed(3)})`)
    if (appState.edit.contrast !== 1) filters.push(`contrast(${appState.edit.contrast})`)
    if (appState.edit.saturation !== 1) filters.push(`saturate(${appState.edit.saturation})`)
    if (appState.edit.filter && presets[appState.edit.filter]) filters.push(presets[appState.edit.filter]!)
    return filters.join(' ')
  })
  let videoTransform = $derived(showOriginal || (!appState.edit.flipH && !appState.edit.flipV)
    ? '' : `scaleX(${appState.edit.flipH ? -1 : 1}) scaleY(${appState.edit.flipV ? -1 : 1})`)
  let advancedColorNotice = $derived.by(() => {
    const lut = Boolean(appState.edit.lutId) && appState.edit.lutIntensity > 0
    const curves = !isIdentityCurves(appState.edit.curves)
    const hsl = !isIdentitySelectiveHsl(appState.edit.hsl)
    const wheels = !isIdentityColorWheels(appState.edit.colorWheels)
    const enabled = [lut && 'LUT', curves && 'кривые', hsl && 'HSL', wheels && 'цветовые колёса'].filter(Boolean)
    return enabled.length ? `Активно: ${enabled.join(', ')}.` : ''
  })
  let compareStatus = $derived(showOriginal
    ? 'Оригинал: монтаж, скорость, громкость, mute, CSS-эффекты и области редактирования отключены.'
    : 'С правками: монтаж, звук, эффекты и области редактирования включены.')
  let playbackHint = $derived.by(() => {
    if (!appState.video) return ''
    if (showOriginal) return 'Оригинал воспроизводится непрерывно с исходной скоростью и громкостью.'
    if (appState.edit.timelineEnabled) return `Таймлайн воспроизводится в заданном порядке и зацикливается. Сегментов: ${activeTimelineSegments(appState.edit, appState.video.duration).length}.`
    return 'Обрезка зациклена внутри выбранного отрезка.'
  })
  const fmtDuration = (time: number) => `${Math.floor(time / 60)}:${Math.floor(time % 60).toString().padStart(2, '0')}`
  function fmtSize(bytes: number): string {
    if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} ГБ`
    if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} МБ`
    return `${Math.max(1, Math.round(bytes / 1024))} КБ`
  }
  function onMediaError(): void {
    const video = appState.video
    const artifact = proxyPlayback?.artifact
    if (video && artifact) proxyController.markPlaybackFailed(video.id, artifact.key)
  }
  let meta = $derived.by(() => {
    const video = appState.video
    if (!video) return [] as string[]
    const parts = [video.width && video.height ? `${video.width}×${video.height}` : '', fmtDuration(video.duration)]
    if (video.fps) parts.push(`${video.fps.toFixed(1)} fps`)
    const codecs = [video.vcodec, video.acodec].filter(Boolean).join(' / ')
    if (codecs) parts.push(codecs)
    if (typeof video.sizeBytes === 'number') parts.push(fmtSize(video.sizeBytes))
    return parts.filter(Boolean)
  })
</script>

<div class="card preview">
  {#if appState.video?.title}<h2 class="preview-title" title={appState.video.title}>{appState.video.title}</h2>{/if}
  {#if appState.video}
    <div class="preview-toolbar">
      <button
        type="button"
        class="preview-compare-toggle"
        aria-label="Оригинал / С правками"
        aria-controls="preview-media"
        aria-describedby="preview-compare-status"
        aria-pressed={showOriginal}
        onclick={() => { showOriginal = !showOriginal }}
      >
        <span class:active={showOriginal} class="preview-compare-option">Оригинал</span>
        <span class="preview-compare-separator" aria-hidden="true">/</span>
        <span class:active={!showOriginal} class="preview-compare-option">С правками</span>
      </button>
      <p id="preview-compare-status" class="preview-compare-status" role="status" aria-live="polite">{compareStatus}</p>
      <div class="preview-source-status" data-preview-source={proxyPlayback?.kind ?? 'original'} role="status" aria-live="polite">
        <strong>Медиа: {proxyIndicator}</strong>
        {#if proxyStatusDetail}<span>{proxyStatusDetail}</span>{/if}
      </div>
    </div>
  {/if}
  <div class="player-wrap">
    <video
      id="preview-media"
      bind:this={videoEl}
      class="player"
      src={previewUrl}
      style:filter={videoFilter || undefined}
      style:transform={videoTransform || undefined}
      muted={showOriginal ? false : appState.edit.mute}
      controls
      playsinline
      onplay={onPlay}
      ontimeupdate={onTimeUpdate}
      onended={onEnded}
      onerror={onMediaError}
    ></video>
    {#if appState.video && !showOriginal && appState.edit.cropEnabled}
      <RectOverlay rect={appState.edit.crop} onrectchange={(rect) => { appState.edit.crop = rect }} oninteractionstart={() => beginEditTransaction('crop-drag')} oninteractionend={endEditTransaction} />
    {/if}
    {#if appState.video && !showOriginal && appState.edit.censorEnabled}
      <RectOverlay rect={appState.edit.censor} color="var(--danger)" mode="mask" onrectchange={(rect) => { appState.edit.censor = rect }} oninteractionstart={() => beginEditTransaction('censor-drag')} oninteractionend={endEditTransaction} />
    {/if}
  </div>
  {#if appState.video}<div class="meta">{#each meta as item, index (index)}<span class="meta-chip">{item}</span>{/each}</div>{/if}
  {#if advancedColorNotice}
    <p class="preview-color-notice" role="status"><strong>{advancedColorNotice}</strong> Эти настройки не отображаются в предпросмотре; точный результат виден после экспорта.</p>
  {/if}
  {#if appState.video}<ColorScopes video={videoEl} />{/if}
  {#if appState.video}<p class="hint">{playbackHint}</p>{/if}
</div>

<style>
  .preview-source-status {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1 0 100%;
    min-width: 0;
    color: var(--muted);
    font-size: 11px;
    line-height: 1.35;
  }
  .preview-source-status strong {
    flex: none;
    padding: 3px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text);
  }
  .preview-source-status[data-preview-source='proxy'] strong {
    border-color: var(--accent);
    color: var(--accent);
  }
  .preview-source-status span { color: var(--warn); }
  @media (max-width: 560px) {
    .preview-source-status { align-items: flex-start; flex-direction: column; }
  }
</style>
