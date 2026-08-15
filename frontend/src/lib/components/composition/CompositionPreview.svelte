<script lang="ts">
  import {
    clipDurationTicks,
    clipEndTicks,
    COMPOSITION_TIME_BASE,
    type CompositionClip,
    type CompositionTransition,
    type VideoClip,
    type VideoTrack,
    type VisualClip,
    type VisualTrack,
  } from '$lib/composition/types.js'
  import {
    sampleAnimatableValue,
    sampleAudioProperty,
    sampleVisualProperty,
  } from '$lib/composition/keyframes.js'
  import { clipSpeedAtTimelineTick, sourceClipTickAtTimelineTick } from '$lib/composition/playback.js'
  import { primaryCompositionVideoTrack } from '$lib/composition/validation.js'
  import type { CompositionPreviewSourceRequest } from '$lib/proxy/compositionPreview.js'
  import {
    compositionPreviewProxyAdapter,
    type CompositionPreviewProxyAdapter,
  } from '$lib/proxy/compositionPreviewController.js'
  import CompositionPreviewProxyStatus from '$lib/proxy/CompositionPreviewProxyStatus.svelte'
  import {
    compositionDuration,
    compositionState,
    setCompositionPlayhead,
    toggleCompositionPlayback,
  } from '$lib/state/composition.svelte.js'
  import CompositionMediaLayer from './CompositionMediaLayer.svelte'

  interface Props {
    proxyAdapter?: CompositionPreviewProxyAdapter
  }

  let { proxyAdapter = compositionPreviewProxyAdapter }: Props = $props()

  interface ActiveClip {
    track: VisualTrack
    clip: VisualClip
    z: number
    transitionRole?: 'from' | 'to'
    transitionProgress?: number
  }

  interface ActiveTransition {
    track: VideoTrack
    transition: CompositionTransition
    from: VideoClip
    to: VideoClip
    progress: number
  }

  let frame: number | null = null
  let lastFrameTime: number | null = null
  let limitedTransitionId = $state<string | null>(null)

  const primaryTrack = $derived(primaryCompositionVideoTrack(compositionState.document))
  const activeTransition = $derived.by(findActiveTransition)
  const activeVisuals = $derived.by(() => {
    const playhead = compositionState.transport.playheadTicks
    const result = compositionState.document.tracks.flatMap((track, index): ActiveClip[] => {
      if (track.kind === 'audio' || track.hidden) return []
      const visualTrack = track as VisualTrack
      if (activeTransition?.track.id === track.id) return []
      const clip = visualTrack.clips.find(
        (candidate) => candidate.timelineStartTicks <= playhead && clipEndTicks(candidate) > playhead,
      )
      return clip ? [{ track: visualTrack, clip, z: compositionState.document.tracks.length - index }] : []
    })
    if (activeTransition) {
      const index = compositionState.document.tracks.findIndex((track) => track.id === activeTransition.track.id)
      const z = compositionState.document.tracks.length - index
      result.push(
        { track: activeTransition.track, clip: activeTransition.from, z, transitionRole: 'from', transitionProgress: activeTransition.progress },
        { track: activeTransition.track, clip: activeTransition.to, z: z + 1, transitionRole: 'to', transitionProgress: activeTransition.progress },
      )
    }
    return result
  })

  const activeAudio = $derived.by(() => {
    const playhead = compositionState.transport.playheadTicks
    const hasSolo = compositionState.document.tracks.some(
      (track) => track.kind === 'audio' && !track.muted && (track.solo ?? false),
    )
    return compositionState.document.tracks.flatMap((track) => {
      if (track.kind !== 'audio' || track.muted || (hasSolo && !(track.solo ?? false))) return []
      const clip = track.clips.find(
        (candidate) => candidate.timelineStartTicks <= playhead && clipEndTicks(candidate) > playhead,
      )
      return clip && clip.speedRamp?.audioPolicy !== 'mute' ? [clip] : []
    })
  })

  const activeProxySources = $derived.by(() => {
    const requests: CompositionPreviewSourceRequest[] = []
    for (const active of activeVisuals) {
      const request = proxySource(active)
      if (!request) continue
      const existing = requests.find((candidate) => candidate.sourceId === request.sourceId)
      if (!existing) requests.push(request)
      else if (request.audibleSourceAudio) existing.audibleSourceAudio = true
    }
    return requests
  })

  const previewNotes = $derived.by(() => {
    const notes: string[] = []
    if (activeTransition) notes.push(`Переход: ${activeTransition.transition.kind}`)
    if (activeTransition?.transition.id === limitedTransitionId) {
      notes.push('Browser seek не даёт показать source handle; экспорт остаётся точным')
    }
    if (activeVisuals.some((active) => active.clip.kind === 'video' && active.clip.chromaKey?.enabled)) {
      notes.push('Chroma key виден только в экспорте')
    }
    if (activeVisuals.some(
      (active) => active.clip.kind === 'video' && (active.clip.frameInterpolation ?? 'duplicate') === 'optical_flow',
    )) {
      notes.push('Optical flow виден точно только в экспорте')
    }
    if (activeVisuals.some((active) => active.clip.kind === 'video' && active.clip.playbackMode?.mode === 'reverse')) {
      notes.push('Reverse preview — seek approximation; motion и audio точны в экспорте')
    }
    if (activeVisuals.some((active) => active.clip.kind === 'video' && active.clip.playbackMode?.mode === 'freeze')) {
      notes.push('Freeze держит source frame; embedded audio выключен')
    }
    if (activeVisuals.some((active) => active.clip.kind === 'video' && active.clip.stabilization?.mode === 'deshake')) {
      notes.push('Deshake stabilization применяется точно только в экспорте')
    }
    if (compositionState.document.tracks.some((track) => track.clips.some((clip) =>
      (clip.kind === 'video' || clip.kind === 'audio') &&
      clip.speedRamp !== undefined &&
      clip.timelineStartTicks <= compositionState.transport.playheadTicks &&
      clipEndTicks(clip) > compositionState.transport.playheadTicks,
    ))) {
      notes.push('Speed ramp: source-time точен; browser playbackRate приблизительный, экспорт точный')
    }
    const masks = activeVisuals.flatMap((active) => active.clip.kind === 'video' ? active.clip.masks ?? [] : [])
    if (masks.length) {
      notes.push('Mask preview — clip-path approximation')
      if (masks.some((mask) => mask.feather > 0)) notes.push('feather только в экспорте')
      if (masks.length > 1 || masks.some((mask) => mask.inverted)) notes.push('intersection/invert точны в экспорте')
    }
    return notes.join(' · ')
  })

  function findActiveTransition(): ActiveTransition | null {
    const track = primaryTrack
    if (!track) return null
    const playhead = compositionState.transport.playheadTicks
    const clips = [...track.clips].sort(
      (left, right) => left.timelineStartTicks - right.timelineStartTicks || left.id.localeCompare(right.id),
    )
    for (const transition of track.transitions ?? []) {
      const from = clips.find((clip) => clip.id === transition.fromClipId)
      const to = clips.find((clip) => clip.id === transition.toClipId)
      if (!from || !to) continue
      const before = Math.floor(transition.durationTicks / 2)
      const start = to.timelineStartTicks - before
      const end = start + transition.durationTicks
      if (playhead >= start && playhead < end) {
        return {
          track,
          transition,
          from,
          to,
          progress: Math.max(0, Math.min(1, (playhead - start) / transition.durationTicks)),
        }
      }
    }
    return null
  }

  function visualStyle(active: ActiveClip): string {
    const { canvas } = compositionState.document
    const isPrimary = active.track.id === primaryTrack?.id
    const transition = transitionStyle(active)
    const localTicks = Math.max(0, compositionState.transport.playheadTicks - active.clip.timelineStartTicks)
    if (active.clip.kind === 'video' || active.clip.kind === 'image') {
      const source = compositionState.document.sources[active.clip.sourceId]
      const x = sampleVisualProperty(compositionState.document, active.clip, 'x', localTicks)
      const y = sampleVisualProperty(compositionState.document, active.clip, 'y', localTicks)
      const scaleX = sampleVisualProperty(compositionState.document, active.clip, 'scaleX', localTicks)
      const scaleY = sampleVisualProperty(compositionState.document, active.clip, 'scaleY', localTicks)
      const rotation = sampleVisualProperty(compositionState.document, active.clip, 'rotationDegrees', localTicks)
      const opacity = sampleVisualProperty(compositionState.document, active.clip, 'opacity', localTicks)
      const blend = active.clip.blendMode === 'addition' ? 'plus-lighter' : active.clip.blendMode ?? 'normal'
      const placement = isPrimary
        ? ['left:0', 'top:0', 'width:100%', 'height:100%', 'transform:none']
        : [
            `left:${50 + (x / canvas.width) * 100}%`,
            `top:${50 + (y / canvas.height) * 100}%`,
            `width:${(((source?.width ?? canvas.width) * scaleX) / canvas.width) * 100}%`,
            `height:${(((source?.height ?? canvas.height) * scaleY) / canvas.height) * 100}%`,
            `transform:translate(-50%, -50%) rotate(${rotation}deg)`,
          ]
      return [
        ...placement,
        `object-fit:${isPrimary ? 'contain' : 'fill'}`,
        `opacity:${isPrimary ? 1 : opacity}`,
        `mix-blend-mode:${blend}`,
        `z-index:${active.z}`,
        maskClipPath(active, localTicks),
        transition,
      ].filter(Boolean).join(';')
    }
    const style = active.clip.style
    const x = sampleVisualProperty(compositionState.document, active.clip, 'x', localTicks)
    const y = sampleVisualProperty(compositionState.document, active.clip, 'y', localTicks)
    const scaleX = sampleVisualProperty(compositionState.document, active.clip, 'scaleX', localTicks)
    const scaleY = sampleVisualProperty(compositionState.document, active.clip, 'scaleY', localTicks)
    const rotation = sampleVisualProperty(compositionState.document, active.clip, 'rotationDegrees', localTicks)
    const opacity = sampleVisualProperty(compositionState.document, active.clip, 'opacity', localTicks)
    const shadow = style.shadowColor
      ? `${style.shadowX ?? 0}px ${style.shadowY ?? 0}px ${style.shadowColor}`
      : 'none'
    return [
      `left:${50 + (x / canvas.width) * 100}%`,
      `top:${50 + (y / canvas.height) * 100}%`,
      `transform:translate(-50%, -50%) rotate(${rotation}deg) scale(${scaleX}, ${scaleY})`,
      `opacity:${opacity}`,
      `color:${style.color}`,
      `background:${style.backgroundColor ?? 'transparent'}`,
      `font-family:${JSON.stringify(style.fontFamily ?? 'Noto Sans')},sans-serif`,
      `font-size:${Math.max(10, (style.fontSizePx / canvas.height) * 100)}cqh`,
      `text-align:${style.align}`,
      `-webkit-text-stroke:${style.strokeWidthPx ?? 0}px ${style.strokeColor ?? 'transparent'}`,
      `text-shadow:${shadow}`,
      `z-index:${active.z}`,
      transition,
    ].filter(Boolean).join(';')
  }

  function maskClipPath(active: ActiveClip, localTicks: number): string {
    if (active.clip.kind !== 'video') return ''
    const mask = active.clip.masks?.[0]
    if (!mask || mask.shape === 'linear') return ''
    const x = sampleAnimatableValue(mask.x, localTicks)
    const y = sampleAnimatableValue(mask.y, localTicks)
    const width = sampleAnimatableValue(mask.width, localTicks)
    const height = sampleAnimatableValue(mask.height, localTicks)
    if (mask.shape === 'ellipse') {
      return `clip-path:ellipse(${width * 50}% ${height * 50}% at ${x * 100}% ${y * 100}%)`
    }
    const top = (y - height / 2) * 100
    const right = (1 - x - width / 2) * 100
    const bottom = (1 - y - height / 2) * 100
    const left = (x - width / 2) * 100
    return `clip-path:inset(${top}% ${right}% ${bottom}% ${left}%)`
  }

  function transitionStyle(active: ActiveClip): string {
    if (!active.transitionRole || active.transitionProgress === undefined || !activeTransition) return ''
    const progress = active.transitionProgress
    const incoming = active.transitionRole === 'to'
    switch (activeTransition.transition.kind) {
      case 'dissolve': return `opacity:${incoming ? progress : 1 - progress}`
      case 'fade_black': return `opacity:${incoming ? Math.max(0, progress * 2 - 1) : Math.max(0, 1 - progress * 2)}`
      case 'wipe_left': return incoming ? `clip-path:inset(0 0 0 ${(1 - progress) * 100}%)` : ''
      case 'wipe_right': return incoming ? `clip-path:inset(0 ${(1 - progress) * 100}% 0 0)` : ''
      case 'slide_left': return `translate:${incoming ? (1 - progress) * 100 : -progress * 100}% 0`
      case 'slide_right': return `translate:${incoming ? -(1 - progress) * 100 : progress * 100}% 0`
    }
  }

  function sourceTime(clip: CompositionClip): number {
    if (clip.kind !== 'video' && clip.kind !== 'audio') return 0
    return sourceClipTickAtTimelineTick(clip, compositionState.transport.playheadTicks) / COMPOSITION_TIME_BASE
  }

  function visualSourceTime(active: ActiveClip): number {
    const clip = active.clip
    if (clip.kind !== 'video' || !active.transitionRole || !activeTransition) return sourceTime(clip)
    const endpoint = active.transitionRole === 'from' ? activeTransition.from : activeTransition.to
    const speed = clip.speed ?? 1
    if (
      endpoint.id !== clip.id || clip.speedRamp ||
      (clip.playbackMode?.mode ?? 'forward') !== 'forward' ||
      !Number.isFinite(speed) || speed <= 0
    ) return sourceTime(clip)

    // Mirrors backend composition_args: the outgoing clip gains the post-edit
    // tail while the incoming clip gains the pre-edit head around this boundary.
    const before = Math.floor(activeTransition.transition.durationTicks / 2)
    const after = activeTransition.transition.durationTicks - before
    const anchor = active.transitionRole === 'from' ? clip.sourceOutTicks : clip.sourceInTicks
    const mapped = anchor + (compositionState.transport.playheadTicks - activeTransition.to.timelineStartTicks) * speed
    const bounded = Math.max(anchor - before * speed, Math.min(anchor + after * speed, mapped))
    const sourceDuration = compositionState.document.sources[clip.sourceId]?.durationTicks ?? bounded
    return Math.max(0, Math.min(sourceDuration, bounded)) / COMPOSITION_TIME_BASE
  }

  function markTransitionSeekLimited(active: ActiveClip): void {
    if (active.transitionRole && activeTransition) limitedTransitionId = activeTransition.transition.id
  }

  function videoMuted(active: ActiveClip): boolean {
    if (active.clip.kind !== 'video' || active.track.kind !== 'video') return true
    const playhead = compositionState.transport.playheadTicks
    return active.track.id !== primaryTrack?.id || active.track.muted || !active.clip.sourceAudioEnabled ||
      active.clip.speedRamp?.audioPolicy === 'mute' ||
      (active.clip.playbackMode?.mode ?? 'forward') !== 'forward' ||
      playhead < active.clip.timelineStartTicks || playhead >= clipEndTicks(active.clip)
  }

  function proxySource(active: ActiveClip): CompositionPreviewSourceRequest | null {
    if (active.clip.kind !== 'video') return null
    const media = compositionState.media[active.clip.sourceId]
    if (!media) return null
    return {
      sourceId: active.clip.sourceId,
      kind: 'video',
      active: true,
      originalUrl: media.url,
      audibleSourceAudio: !videoMuted(active),
    }
  }

  function audioVolume(clip: (typeof activeAudio)[number]): number {
    const playhead = compositionState.transport.playheadTicks
    const elapsed = playhead - clip.timelineStartTicks
    const remaining = clipEndTicks(clip) - playhead
    const fadeIn = clip.fadeInTicks ?? 0
    const fadeOut = clip.fadeOutTicks ?? 0
    const fadeFactor = Math.min(
      1,
      fadeIn > 0 ? Math.max(0, elapsed / fadeIn) : 1,
      fadeOut > 0 ? Math.max(0, remaining / fadeOut) : 1,
    )
    const localTicks = Math.max(0, Math.min(clipDurationTicks(clip), elapsed))
    return sampleAudioProperty(clip, 'gain', localTicks) * fadeFactor
  }

  function tickFrame(now: number): void {
    if (!compositionState.transport.playing) {
      frame = null
      lastFrameTime = null
      return
    }
    if (lastFrameTime !== null) {
      setCompositionPlayhead(
        compositionState.transport.playheadTicks + (now - lastFrameTime) * (COMPOSITION_TIME_BASE / 1000),
      )
    }
    lastFrameTime = now
    if (compositionState.transport.playing) frame = requestAnimationFrame(tickFrame)
  }

  $effect(() => {
    if (compositionState.transport.playing && frame === null) {
      lastFrameTime = null
      frame = requestAnimationFrame(tickFrame)
    } else if (!compositionState.transport.playing && frame !== null) {
      cancelAnimationFrame(frame)
      frame = null
      lastFrameTime = null
    }
  })

  $effect(() => () => {
    if (frame !== null) cancelAnimationFrame(frame)
    frame = null
    lastFrameTime = null
  })

  function seekFromInput(event: Event): void {
    setCompositionPlayhead(Number((event.currentTarget as HTMLInputElement).value))
  }

  function formatTime(ticks: number): string {
    const seconds = Math.max(0, ticks / COMPOSITION_TIME_BASE)
    const minutes = Math.floor(seconds / 60)
    return `${minutes}:${(seconds % 60).toFixed(1).padStart(4, '0')}`
  }
</script>

<section class="composition-preview card" aria-label="Предпросмотр композиции">
  <div
    class="composition-stage"
    style:aspect-ratio={`${compositionState.document.canvas.width} / ${compositionState.document.canvas.height}`}
    style:background={compositionState.document.canvas.backgroundColor}
  >
    {#each activeVisuals as active (`${active.clip.id}-${active.transitionRole ?? 'active'}`)}
      {#if active.clip.kind === 'video'}
        {@const media = compositionState.media[active.clip.sourceId]}
        {#if media}
          {@const proxyRequest = proxySource(active)}
          {@const proxySelection = proxyRequest ? proxyAdapter.resolve(proxyRequest) : null}
          <CompositionMediaLayer
            kind="video"
            url={proxySelection?.url ?? media.url}
            sourceTime={visualSourceTime(active)}
            playing={compositionState.transport.playing && (active.clip.playbackMode?.mode ?? 'forward') === 'forward'}
            muted={videoMuted(active)}
            volume={sampleAudioProperty(
              active.clip,
              'gain',
              Math.max(0, Math.min(
                clipDurationTicks(active.clip),
                compositionState.transport.playheadTicks - active.clip.timelineStartTicks,
              )),
            )}
            playbackRate={(active.clip.playbackMode?.mode ?? 'forward') === 'forward'
              ? clipSpeedAtTimelineTick(active.clip, compositionState.transport.playheadTicks)
              : 1}
            class="composition-visual-media"
            style={visualStyle(active)}
            onmediaerror={proxyRequest && proxySelection?.kind === 'proxy'
              ? () => proxyAdapter.markMediaFailed(proxyRequest, proxySelection)
              : undefined}
            onseeklimited={active.transitionRole ? () => markTransitionSeekLimited(active) : undefined}
          />
        {:else}
          <div class="composition-missing-media" style={visualStyle(active)}>Источник недоступен</div>
        {/if}
      {:else if active.clip.kind === 'image'}
        {@const media = compositionState.media[active.clip.sourceId]}
        {#if media}
          <img class="composition-visual-media" style={visualStyle(active)} src={media.url} alt={media.filename} />
        {:else}
          <div class="composition-missing-media" style={visualStyle(active)}>Изображение недоступно</div>
        {/if}
      {:else if active.clip.kind === 'text'}
        <div class="composition-text-layer" style={visualStyle(active)}>{active.clip.text}</div>
      {/if}
    {/each}
    {#if activeVisuals.length === 0}
      <p class="composition-empty-stage">Добавьте видео, изображение или текст</p>
    {/if}
    {#if previewNotes}<span class="composition-preview-badge">{previewNotes}</span>{/if}
  </div>

  {#each activeAudio as clip (clip.id)}
    {@const media = compositionState.media[clip.sourceId]}
    {#if media}
      <CompositionMediaLayer
        kind="audio"
        url={media.url}
        sourceTime={sourceTime(clip)}
        playing={compositionState.transport.playing}
        volume={audioVolume(clip)}
        playbackRate={clipSpeedAtTimelineTick(clip, compositionState.transport.playheadTicks)}
      />
    {/if}
  {/each}

  {#each activeProxySources as source (source.sourceId)}
    <CompositionPreviewProxyStatus {source} adapter={proxyAdapter} />
  {/each}

  <div class="composition-transport">
    <button class="btn ghost sm" onclick={toggleCompositionPlayback} disabled={!compositionDuration()}>
      {compositionState.transport.playing ? '⏸ Пауза' : '▶ Играть'}
    </button>
    <input
      aria-label="Плейхед"
      type="range"
      min="0"
      max={Math.max(1, compositionDuration())}
      step="1000"
      value={compositionState.transport.playheadTicks}
      oninput={seekFromInput}
    />
    <output>{formatTime(compositionState.transport.playheadTicks)} / {formatTime(compositionDuration())}</output>
  </div>
</section>
