<script lang="ts">
  import {
    clipDurationTicks,
    clipEndTicks,
    COMPOSITION_TIME_BASE,
    type CompositionClip,
    type AudioClip,
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

  interface ActiveAudioClip {
    clip: AudioClip
    crossfadeRole?: 'from' | 'to'
    boundaryTicks?: number
    crossfadeTicks?: number
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
    return compositionState.document.tracks.flatMap((track): ActiveAudioClip[] => {
      if (track.kind !== 'audio' || track.muted || (hasSolo && !(track.solo ?? false))) return []
      const ordered = [...track.clips].sort((left, right) => left.timelineStartTicks - right.timelineStartTicks)
      for (let index = 1; index < ordered.length; index += 1) {
        const to = ordered[index]!
        const from = ordered[index - 1]!
        const duration = to.crossfadeInTicks ?? 0
        const before = Math.floor(duration / 2)
        const after = duration - before
        const boundary = to.timelineStartTicks
        if (duration > 0 && clipEndTicks(from) === boundary && playhead >= boundary - before && playhead < boundary + after) {
          return [
            { clip: from, crossfadeRole: 'from', boundaryTicks: boundary, crossfadeTicks: duration },
            { clip: to, crossfadeRole: 'to', boundaryTicks: boundary, crossfadeTicks: duration },
          ]
        }
      }
      const clip = track.clips.find(
        (candidate) => candidate.timelineStartTicks <= playhead && clipEndTicks(candidate) > playhead,
      )
      return clip && clip.speedRamp?.audioPolicy !== 'mute' ? [{ clip }] : []
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
    if (activeAudio.some((active) => active.crossfadeRole)) notes.push('Audio crossfade')
    if (activeAudio.some((active) => (active.clip.voiceEffect ?? 'none') !== 'none')) notes.push('Voice effect точен только в экспорте')
    if (activeTransition?.transition.id === limitedTransitionId) {
      notes.push('Browser seek не даёт показать source handle; экспорт остаётся точным')
    }
    if (activeVisuals.some((active) => active.clip.kind === 'video' && active.clip.chromaKey?.enabled)) {
      notes.push('Chroma key виден только в экспорте')
    }
    if (activeVisuals.some((active) => active.clip.kind === 'video' && active.clip.videoEffects?.some((effect) => effect.preset !== 'blur'))) {
      notes.push('Pixelate/Vignette/Sharpen/Edge/RGB Split/Posterize точны только в экспорте')
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
    const playhead = compositionState.transport.playheadTicks
    for (const candidate of compositionState.document.tracks) {
      if (candidate.kind !== 'video' || candidate.hidden) continue
      const track = candidate
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
        videoEffectPreviewStyle(active.clip),
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

  function videoEffectPreviewStyle(clip: VisualClip): string {
    if (clip.kind !== 'video') return ''
    const blurs = (clip.videoEffects ?? []).filter((effect) => effect.preset === 'blur')
    if (!blurs.length) return ''
    return `filter:${blurs.map((effect) => `blur(${(1 + effect.intensity * 7).toFixed(2)}px)`).join(' ')}`
  }

  function canvasStyle(): string {
    const canvas = compositionState.document.canvas
    if (canvas.backgroundMode !== 'checker') return `background:${canvas.backgroundColor}`
    return [
      `background-color:${canvas.backgroundColor}`,
      `background-image:linear-gradient(45deg,rgba(255,255,255,.11) 25%,transparent 25%,transparent 75%,rgba(255,255,255,.11) 75%),linear-gradient(45deg,rgba(255,255,255,.11) 25%,transparent 25%,transparent 75%,rgba(255,255,255,.11) 75%)`,
      'background-position:0 0,32px 32px',
      'background-size:64px 64px',
    ].join(';')
  }

  function blurBackgroundStyle(active: ActiveClip): string {
    return [
      'left:-6%',
      'top:-6%',
      'width:112%',
      'height:112%',
      'object-fit:cover',
      `filter:blur(${compositionState.document.canvas.backgroundBlur ?? 24}px)`,
      'z-index:0',
      transitionStyle(active),
    ].filter(Boolean).join(';')
  }

  function maskClipPath(active: ActiveClip, localTicks: number): string {
    if (active.clip.kind !== 'video') return ''
    const mask = active.clip.masks?.[0]
    if (!mask) return ''
    const x = sampleAnimatableValue(mask.x, localTicks)
    const y = sampleAnimatableValue(mask.y, localTicks)
    const rotation = sampleAnimatableValue(mask.rotationDegrees ?? { mode: 'constant', value: 0 }, localTicks)
    if (mask.shape === 'linear') return linearMaskClipPath(x, y, rotation, mask.inverted)
    const width = sampleAnimatableValue(mask.width, localTicks)
    const height = sampleAnimatableValue(mask.height, localTicks)
    if (mask.shape === 'ellipse') {
      if (rotation === 0) return `clip-path:ellipse(${width * 50}% ${height * 50}% at ${x * 100}% ${y * 100}%)`
      const points = Array.from({ length: 48 }, (_, index) => {
        const angle = index * Math.PI * 2 / 48
        return rotateMaskPoint(x, y, Math.cos(angle) * width / 2, Math.sin(angle) * height / 2, rotation)
      })
      return maskPolygonClipPath(points)
    }
    if (rotation !== 0) {
      return maskPolygonClipPath([
        rotateMaskPoint(x, y, -width / 2, -height / 2, rotation),
        rotateMaskPoint(x, y, width / 2, -height / 2, rotation),
        rotateMaskPoint(x, y, width / 2, height / 2, rotation),
        rotateMaskPoint(x, y, -width / 2, height / 2, rotation),
      ])
    }
    const top = (y - height / 2) * 100
    const right = (1 - x - width / 2) * 100
    const bottom = (1 - y - height / 2) * 100
    const left = (x - width / 2) * 100
    return `clip-path:inset(${top}% ${right}% ${bottom}% ${left}%)`
  }

  function rotateMaskPoint(x: number, y: number, dx: number, dy: number, degrees: number): readonly [number, number] {
    const radians = degrees * Math.PI / 180
    const cosine = Math.cos(radians)
    const sine = Math.sin(radians)
    return [x + dx * cosine - dy * sine, y + dx * sine + dy * cosine]
  }

  function maskPolygonClipPath(points: readonly (readonly [number, number])[]): string {
    return `clip-path:polygon(${points.map(([x, y]) => `${x * 100}% ${y * 100}%`).join(',')})`
  }

  function linearMaskClipPath(x: number, y: number, degrees: number, inverted: boolean): string {
    const radians = degrees * Math.PI / 180
    const normalX = Math.cos(radians) * (inverted ? -1 : 1)
    const normalY = Math.sin(radians) * (inverted ? -1 : 1)
    const distance = ([px, py]: readonly [number, number]): number => (px - x) * normalX + (py - y) * normalY
    const input: Array<readonly [number, number]> = [[0, 0], [1, 0], [1, 1], [0, 1]]
    const output: Array<readonly [number, number]> = []
    for (let index = 0; index < input.length; index += 1) {
      const current = input[index]!
      const previous = input[(index + input.length - 1) % input.length]!
      const currentDistance = distance(current)
      const previousDistance = distance(previous)
      if (currentDistance <= 0) {
        if (previousDistance > 0) {
          const ratio = previousDistance / (previousDistance - currentDistance)
          output.push([previous[0] + (current[0] - previous[0]) * ratio, previous[1] + (current[1] - previous[1]) * ratio])
        }
        output.push(current)
      } else if (previousDistance <= 0) {
        const ratio = previousDistance / (previousDistance - currentDistance)
        output.push([previous[0] + (current[0] - previous[0]) * ratio, previous[1] + (current[1] - previous[1]) * ratio])
      }
    }
    if (output.length < 3) return 'clip-path:polygon(0 0,0 0,0 0)'
    return `clip-path:polygon(${output.map(([px, py]) => `${px * 100}% ${py * 100}%`).join(',')})`
  }

  function transitionStyle(active: ActiveClip): string {
    if (!active.transitionRole || active.transitionProgress === undefined || !activeTransition) return ''
    const progress = active.transitionProgress
    const incoming = active.transitionRole === 'to'
    const kind = activeTransition.transition.kind
    switch (kind) {
      case 'dissolve': return `opacity:${incoming ? progress : 1 - progress}`
      case 'fade_black': return `opacity:${incoming ? Math.max(0, progress * 2 - 1) : Math.max(0, 1 - progress * 2)}`
      case 'wipe_left': return incoming ? `clip-path:inset(0 0 0 ${(1 - progress) * 100}%)` : ''
      case 'wipe_right': return incoming ? `clip-path:inset(0 ${(1 - progress) * 100}% 0 0)` : ''
      case 'wipe_up': return incoming ? `clip-path:inset(${(1 - progress) * 100}% 0 0 0)` : ''
      case 'wipe_down': return incoming ? `clip-path:inset(0 0 ${(1 - progress) * 100}% 0)` : ''
      case 'smooth_left': return incoming ? `clip-path:inset(0 0 0 ${(1 - progress) * 100}%)` : ''
      case 'smooth_right': return incoming ? `clip-path:inset(0 ${(1 - progress) * 100}% 0 0)` : ''
      case 'smooth_up': return incoming ? `clip-path:inset(${(1 - progress) * 100}% 0 0 0)` : ''
      case 'smooth_down': return incoming ? `clip-path:inset(0 0 ${(1 - progress) * 100}% 0)` : ''
      case 'slide_left': return `translate:${incoming ? (1 - progress) * 100 : -progress * 100}% 0`
      case 'slide_right': return `translate:${incoming ? -(1 - progress) * 100 : progress * 100}% 0`
      case 'slide_up': return `translate:0 ${incoming ? (1 - progress) * 100 : -progress * 100}%`
      case 'slide_down': return `translate:0 ${incoming ? -(1 - progress) * 100 : progress * 100}%`
      case 'circle_open': return incoming ? `clip-path:circle(${progress * 71}% at 50% 50%)` : ''
      case 'circle_close': return incoming ? `mask:radial-gradient(transparent ${(1 - progress) * 71}%,#000 0)` : ''
      case 'wipe_top_left':
      case 'wipe_top_right':
      case 'wipe_bottom_left':
      case 'wipe_bottom_right':
        return incoming
          ? diagonalWipeClip(progress, kind.includes('right'), kind.includes('bottom'))
          : ''
      case 'vertical_open': return incoming ? `clip-path:inset(0 ${(1 - progress) * 50}%)` : ''
      case 'vertical_close': return incoming ? `mask:linear-gradient(90deg,#000 ${progress * 50}%,transparent 0 ${100 - progress * 50}%,#000 0)` : ''
      case 'horizontal_open': return incoming ? `clip-path:inset(${(1 - progress) * 50}% 0)` : ''
      case 'horizontal_close': return incoming ? `mask:linear-gradient(#000 ${progress * 50}%,transparent 0 ${100 - progress * 50}%,#000 0)` : ''
    }
  }

  function diagonalWipeClip(progress: number, fromRight: boolean, fromBottom: boolean): string {
    const extent = progress * 2
    const points = extent <= 1
      ? [[0, 0], [extent, 0], [0, extent]]
      : [[0, 0], [1, 0], [1, extent - 1], [extent - 1, 1], [0, 1]]
    return `clip-path:polygon(${points.map(([x, y]) =>
      `${(fromRight ? 1 - x : x) * 100}% ${(fromBottom ? 1 - y : y) * 100}%`,
    ).join()})`
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

  function audioVolume(active: ActiveAudioClip): number {
    const clip = active.clip
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
    const crossfadeFactor = active.crossfadeRole && active.boundaryTicks !== undefined && active.crossfadeTicks
      ? active.crossfadeRole === 'to'
        ? Math.max(0, Math.min(1, (playhead - (active.boundaryTicks - Math.floor(active.crossfadeTicks / 2))) / active.crossfadeTicks))
        : 1 - Math.max(0, Math.min(1, (playhead - (active.boundaryTicks - Math.floor(active.crossfadeTicks / 2))) / active.crossfadeTicks))
      : 1
    return sampleAudioProperty(clip, 'gain', localTicks) * fadeFactor * crossfadeFactor
  }

  function audioSourceTime(active: ActiveAudioClip): number {
    const clip = active.clip
    if (!active.crossfadeRole || active.boundaryTicks === undefined || !active.crossfadeTicks) return sourceTime(clip)
    const speed = clip.speed ?? 1
    const before = Math.floor(active.crossfadeTicks / 2)
    const after = active.crossfadeTicks - before
    const anchor = active.crossfadeRole === 'from' ? clip.sourceOutTicks : clip.sourceInTicks
    const mapped = anchor + (compositionState.transport.playheadTicks - active.boundaryTicks) * speed
    return Math.max(anchor - before * speed, Math.min(anchor + after * speed, mapped)) / COMPOSITION_TIME_BASE
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
    style={canvasStyle()}
  >
    {#if compositionState.document.canvas.backgroundMode === 'blur'}
      {#each activeVisuals.filter((active) => active.track.id === primaryTrack?.id && active.clip.kind === 'video') as active (`canvas-blur-${active.clip.id}-${active.transitionRole ?? 'active'}`)}
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
            muted
            playbackRate={(active.clip.playbackMode?.mode ?? 'forward') === 'forward'
              ? clipSpeedAtTimelineTick(active.clip, compositionState.transport.playheadTicks)
              : 1}
            class="composition-visual-media composition-canvas-blur"
            style={blurBackgroundStyle(active)}
          />
          {/if}
        {/if}
      {/each}
    {/if}
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

  {#each activeAudio as active (`${active.clip.id}-${active.crossfadeRole ?? 'active'}`)}
    {@const media = compositionState.media[active.clip.sourceId]}
    {#if media}
      <CompositionMediaLayer
        kind="audio"
        url={media.url}
        sourceTime={audioSourceTime(active)}
        playing={compositionState.transport.playing}
        volume={audioVolume(active)}
        playbackRate={clipSpeedAtTimelineTick(active.clip, compositionState.transport.playheadTicks)}
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
