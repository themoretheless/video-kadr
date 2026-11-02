<script lang="ts">
  import { untrack } from 'svelte'
  import { NativeAudioPreview } from '$lib/audio/nativePreview'
  import { buildNativeAudioPreviewProject, needsNativeAudioPreview } from '$lib/audio/nativePreviewProject'
  import { buildIndependentTransitionPreviewProject } from '$lib/composition/transitionPreview'
  import NestedCompositionPreview from './NestedCompositionPreview.svelte'
  import { buildCompositionRenderRequest } from '$lib/composition/payload'
  import { authState } from '$lib/state/auth.svelte.js'
  import type { NativeNestedProject } from '$lib/api'
  import { positionConversionPreview } from '$lib/state/positionConversionPreview.svelte'
  import { colorCurvePreview } from '$lib/state/colorCurvePreview.svelte'
  import CompositionMotionPathOverlay from './CompositionMotionPathOverlay.svelte'
  import { evaluateMotionPath } from '$lib/api'
  import { prepareColorPreview } from '$lib/native/colorPreview'
  import { packColorAnimation,colorToNative,nativeColorToHex } from '$lib/composition/colorAnimation'
  import type { CompositionPathPoint } from '$lib/composition/types'
  import { audioFadeMultiplier, audioCrossfadeMultiplier } from '$lib/audio/fade'
  import { resetMasterPeakHold } from '$lib/audio/mediaGain'
  import { previewAudioState, previewAudioRoleGain } from '$lib/audio/previewAudio.svelte'
  import {
    clipDurationTicks,
    clipEndTicks,
    COMPOSITION_TIME_BASE,
    type CompositionClip,
    type AudioClip,
    type CompositionTransition,
    type VideoClip,
    type VideoTrack,
    type TextClip, type ColorClip,
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
    projectAutomationValues,
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
    crossfadeCurve?: AudioClip['crossfadeCurve']
  }

  let frame: number | null = null
  let lastFrameTime: number | null = null
  let limitedTransitionId = $state<string | null>(null)

  const draftDocument = $derived(colorCurvePreview.source === compositionState.document && colorCurvePreview.document ? colorCurvePreview.document : positionConversionPreview.source === compositionState.document && positionConversionPreview.document ? positionConversionPreview.document : compositionState.document)
  const previewDocument = $derived(projectAutomationValues(draftDocument, compositionState.ui.keyframePreviews))
  const nativeAudio = new NativeAudioPreview()
  let nativeAudioReady = $state(false)
  let nativeAudioError = $state('')
  let lastNativePlayhead: number | null = null
  const nativeAudioRequested = $derived(needsNativeAudioPreview(previewDocument))
  const nativeAudioRequest = $derived.by(() => {
    if (!nativeAudioRequested) return { project: null, error: '' }
    try { return { project: buildNativeAudioPreviewProject(previewDocument, previewAudioState.roleGains, compositionDuration()), error: '' } }
    catch (error) { return { project: null, error: error instanceof Error ? error.message : String(error) } }
  })
  $effect(() => {
    const project = nativeAudioRequest.project
    const playing = compositionState.transport.playing
    const token = authState.token ?? undefined
    nativeAudioReady = false
    if (playing || !nativeAudioRequested) nativeAudioError = nativeAudioRequest.error
    if (nativeAudioRequested && playing) {
      if (project) {
        const head = untrack(() => compositionState.transport.playheadTicks)
        lastNativePlayhead = head
        nativeAudio.start(project, head, untrack(compositionDuration), token)
      } else untrack(toggleCompositionPlayback)
    }
    return () => nativeAudio.stop()
  })
  $effect(() => {
    const head = compositionState.transport.playheadTicks
    untrack(() => {
      if (!nativeAudioRequested || !compositionState.transport.playing || !nativeAudioRequest.project || head === lastNativePlayhead) return
      lastNativePlayhead = head
      nativeAudioReady = false
      nativeAudioError = ''
      nativeAudio.start(nativeAudioRequest.project, head, compositionDuration(), authState.token ?? undefined)
    })
  })
  const graphicPreviewRequest = $derived.by((): {project: NativeNestedProject | null;error: string} => {
    try {
      const independent = buildIndependentTransitionPreviewProject(previewDocument)
      if (independent) return {project:independent,error:''}
    } catch (reason) { return {project:null,error:reason instanceof Error ? reason.message : String(reason)} }
    if (!previewDocument.tracks.some(track => track.kind === 'graphic' && !track.hidden && track.clips.some(clip => clip.enabled !== false)) && !previewDocument.captionTracks?.some(track => !track.hidden && track.clips.length > 0)) return {project:null,error:''}
    try {
      return {project:{ schemaVersion: 1, root: 'editor', compositions: { editor: { content: buildCompositionRenderRequest(previewDocument).composition, instances: [] } } },error:''}
    } catch (reason) { return {project:null,error:reason instanceof Error ? reason.message : String(reason)} }
  })
  const graphicPreviewProject = $derived(graphicPreviewRequest.project)
  const primaryTrack = $derived(primaryCompositionVideoTrack(previewDocument))
  const activeTransition = $derived.by(findActiveTransition)
  const activeVisuals = $derived.by(() => {
    const playhead = compositionState.transport.playheadTicks
    const result = previewDocument.tracks.flatMap((track, index): ActiveClip[] => {
      if (track.kind === 'audio' || track.hidden) return []
      const visualTrack = track as VisualTrack
      if (activeTransition?.track.id === track.id) return []
      const clip = visualTrack.clips.find(
        (candidate) => candidate.enabled !== false && candidate.timelineStartTicks <= playhead && clipEndTicks(candidate) > playhead,
      )
      return clip ? [{ track: visualTrack, clip, z: previewDocument.tracks.length - index }] : []
    })
    if (activeTransition) {
      const index = previewDocument.tracks.findIndex((track) => track.id === activeTransition.track.id)
      const z = previewDocument.tracks.length - index
      result.push(
        { track: activeTransition.track, clip: activeTransition.from, z, transitionRole: 'from', transitionProgress: activeTransition.progress },
        { track: activeTransition.track, clip: activeTransition.to, z: z + 1, transitionRole: 'to', transitionProgress: activeTransition.progress },
      )
    }
    return result
  })

  const activeAudio = $derived.by(() => {
    const playhead = compositionState.transport.playheadTicks
    const hasSolo = previewDocument.tracks.some(
      (track) => track.kind === 'audio' && !track.muted && (track.solo ?? false),
    )
    return previewDocument.tracks.flatMap((track): ActiveAudioClip[] => {
      if (track.kind !== 'audio' || track.muted || (hasSolo && !(track.solo ?? false))) return []
      const ordered = track.clips.filter((clip) => clip.enabled !== false).sort((left, right) => left.timelineStartTicks - right.timelineStartTicks)
      for (let index = 1; index < ordered.length; index += 1) {
        const to = ordered[index]!
        const from = ordered[index - 1]!
        const duration = to.crossfadeInTicks ?? 0
        const before = Math.floor(duration / 2)
        const after = duration - before
        const boundary = to.timelineStartTicks
        if (duration > 0 && clipEndTicks(from) === boundary && playhead >= boundary - before && playhead < boundary + after) {
          return [
            { clip: from, crossfadeRole: 'from', boundaryTicks: boundary, crossfadeTicks: duration, crossfadeCurve: to.crossfadeCurve },
            { clip: to, crossfadeRole: 'to', boundaryTicks: boundary, crossfadeTicks: duration, crossfadeCurve: to.crossfadeCurve },
          ]
        }
      }
      const clip = track.clips.find(
        (candidate) => candidate.enabled !== false && candidate.timelineStartTicks <= playhead && clipEndTicks(candidate) > playhead,
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
    if (activeTransition) notes.push(`Переход: ${activeTransition.transition.concatOverlap?.legacy ?? activeTransition.transition.kind}`)
    if (activeAudio.some((active) => active.crossfadeRole)) notes.push('Audio crossfade')
    if (!nativeAudioRequested && activeAudio.some((active) => (active.clip.voiceEffect ?? 'none') !== 'none')) notes.push('Voice effect точен только в экспорте')
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
    if (activeVisuals.some((active) => active.clip.kind === 'video' && (active.clip.stabilization?.mode === 'deshake' || active.clip.stabilization?.mode === 'vidstab'))) {
      notes.push('Stabilization применяется точно только в экспорте')
    }
    if (previewDocument.tracks.some((track) => track.clips.some((clip) =>
      (clip.kind === 'video' || clip.kind === 'audio') &&
      clip.speedRamp !== undefined &&
      clip.timelineStartTicks <= compositionState.transport.playheadTicks &&
      clipEndTicks(clip) > compositionState.transport.playheadTicks,
    ))) {
      notes.push('Speed ramp: source-time точен; browser playbackRate приблизительный, экспорт точный')
    }
    if (previewDocument.tracks.some(track=>track.kind==='video'&&!track.hidden&&track.transitions?.some(transition=>transition.timeline))) notes.push(graphicPreviewRequest.error ? 'Нативный предпросмотр перехода недоступен' : 'Нативный предпросмотр переходов; звук browser playback приблизительный')
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
    for (const candidate of previewDocument.tracks) {
      if (candidate.kind !== 'video' || candidate.hidden) continue
      const track = candidate
      const clips = [...track.clips].sort(
        (left, right) => left.timelineStartTicks - right.timelineStartTicks || left.id.localeCompare(right.id),
      )
      for (const transition of track.transitions ?? []) {
        if (transition.timeline) continue
        const from = clips.find((clip) => clip.id === transition.fromClipId)
        const to = clips.find((clip) => clip.id === transition.toClipId)
        if (!from || !to || from.enabled === false || to.enabled === false) continue
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

  type ColorSampler = ((time: number) => import('$lib/api').NativeColor) & { dispose?: () => void }
  let colorSamplers = $state.raw<Record<string,{ key: string;sample: ColorSampler }>>({})
  const colorTrackKey = (clip: TextClip | ColorClip) => JSON.stringify([clip.colorAnimation,(clip.kind === 'text' ? clip.style.color : clip.color)])
  // Intentionally nonreactive: preparation must not subscribe to its own results.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const loadedColors = new Map<string,{ key: string;sample: ColorSampler }>()
  // Promise cache participates in the same nonreactive preparation lifecycle.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const preparedColors = new Map<string,Promise<ColorSampler>>()
  $effect(() => {
    // Track preparation depends on authored data, never on the playhead.
    const texts = previewDocument.tracks.flatMap<TextClip | ColorClip>(track => (track.kind === 'text' || track.kind === 'color') ? track.clips.filter(clip => clip.colorAnimation?.mode === 'keyframes') : [])
    const wanted = new Set(texts.map(colorTrackKey))
    for (const [key,pending] of preparedColors) if (!wanted.has(key)) {
      void pending.then(sample => sample.dispose?.()).catch(() => {})
      preparedColors.delete(key)
    }
    let live = true
    const next: Record<string,{ key: string;sample: ColorSampler }> = {}
    for (const clip of texts) {
      const key = colorTrackKey(clip)
      const prior = loadedColors.get(clip.id)
      if (prior?.key === key) { next[clip.id] = prior;continue }
      let preparation = preparedColors.get(key)
      if (!preparation) {
        preparation = prepareColorPreview(packColorAnimation(clip.colorAnimation!),colorToNative((clip.kind === 'text' ? clip.style.color : clip.color)))
        preparedColors.set(key,preparation)
      }
      void preparation.then(sample => {
        if (live) {
          loadedColors.set(clip.id,{ key,sample })
          colorSamplers = Object.fromEntries(loadedColors)
        }
      }).catch(error => {
        if (preparedColors.get(key) === preparation) preparedColors.delete(key)
        if (live) compositionState.ui.message = error instanceof Error ? error.message : String(error)
      })
    }
    loadedColors.clear()
    for (const [id,entry] of Object.entries(next)) loadedColors.set(id,entry)
    colorSamplers = next
    return () => { live = false }
  })
  $effect(() => () => {
    for (const pending of preparedColors.values()) void pending.then(sample => sample.dispose?.()).catch(() => {})
    preparedColors.clear()
    loadedColors.clear()
  })
  function textColor(clip: TextClip | ColorClip,local: number): string | null {
    if (!clip.colorAnimation) return (clip.kind === 'text' ? clip.style.color : clip.color)
    if (clip.colorAnimation.mode === 'constant') return clip.colorAnimation.value
    const entry = colorSamplers[clip.id]
    if (entry?.key !== colorTrackKey(clip)) return null
    return nativeColorToHex(entry.sample(local))
  }

  let nativePositions = $state<Record<string,{ key: string; point: CompositionPathPoint }>>({})
  const nativePositionCache: Record<string,CompositionPathPoint> = {}
  const nativePositionCacheKeys: string[] = []
  function pathKey(clip: VisualClip,localTicks: number): string {
    if (clip.position?.mode !== 'path') return ''
    return JSON.stringify([clip.position.path,sampleAnimatableValue(clip.position.progress,localTicks)])
  }
  $effect(() => {
    const playhead = compositionState.transport.playheadTicks
    const requests = activeVisuals.filter(active => active.clip.position?.mode === 'path').map(({ clip }) => {
      const local = Math.max(0,playhead-clip.timelineStartTicks)
      return { clip,key: pathKey(clip,local),local }
    })
    const controller = new AbortController()
    let live = true
    for (const { clip,key,local } of requests) {
      const cached = nativePositionCache[key]
      if (cached) { nativePositions[clip.id] = { key,point: cached }; continue }
      const position = clip.position
      if (position?.mode !== 'path') continue
      void evaluateMotionPath({ operation: 'sample',path: position.path,progresses: [sampleAnimatableValue(position.progress,local)] },controller.signal).then(response => {
        if (!live || !response.points[0]) return
        const point = response.points[0]
        if (!Object.hasOwn(nativePositionCache,key)) nativePositionCacheKeys.push(key)
        while (nativePositionCacheKeys.length > 256) delete nativePositionCache[nativePositionCacheKeys.shift()!]
        nativePositionCache[key] = point
        nativePositions[clip.id] = { key,point }
      }).catch(error => {
        if (live && !controller.signal.aborted) compositionState.ui.message = error instanceof Error ? error.message : String(error)
      })
    }
    return () => { live = false; controller.abort() }
  })
  function positionPoint(clip: VisualClip,localTicks: number): CompositionPathPoint | null {
    if (clip.position?.mode === 'xy') return { x: sampleAnimatableValue(clip.position.x,localTicks),y: sampleAnimatableValue(clip.position.y,localTicks) }
    if (clip.position?.mode === 'path') {
      const resolved = nativePositions[clip.id]
      return resolved?.key === pathKey(clip,localTicks) ? resolved.point : null
    }
    return { x: sampleVisualProperty(previewDocument,clip,'x',localTicks),y: sampleVisualProperty(previewDocument,clip,'y',localTicks) }
  }
  function visualStyle(active: ActiveClip): string {
    const { canvas } = previewDocument
    const isPrimary = active.track.id === primaryTrack?.id
    const transition = transitionStyle(active)
    const localTicks = Math.max(0, compositionState.transport.playheadTicks - active.clip.timelineStartTicks)
    if (active.clip.kind === 'video' || active.clip.kind === 'image' || active.clip.kind === 'color') {
      if (active.clip.kind === 'color' && textColor(active.clip,localTicks) === null) return 'visibility:hidden'
      const source = active.clip.kind === 'color' ? null : previewDocument.sources[active.clip.sourceId]
      const point = positionPoint(active.clip,localTicks)
      if (!point) return 'visibility:hidden'
      const { x,y } = point
      const scaleX = sampleVisualProperty(previewDocument, active.clip, 'scaleX', localTicks)
      const scaleY = sampleVisualProperty(previewDocument, active.clip, 'scaleY', localTicks)
      const rotation = sampleVisualProperty(previewDocument, active.clip, 'rotationDegrees', localTicks)
      const opacity = sampleVisualProperty(previewDocument, active.clip, 'opacity', localTicks)
      const blend = active.clip.blendMode === 'addition' ? 'plus-lighter' : active.clip.blendMode ?? 'normal'
      const placement = isPrimary
        ? ['left:0', 'top:0', 'width:100%', 'height:100%', 'transform:none']
        : [
            `left:${50 + (x / canvas.width) * 100}%`,
            `top:${50 + (y / canvas.height) * 100}%`,
            `width:${(((source?.width ?? (active.clip.kind === 'color' ? active.clip.width : canvas.width)) * scaleX) / canvas.width) * 100}%`,
            `height:${(((source?.height ?? (active.clip.kind === 'color' ? active.clip.height : canvas.height)) * scaleY) / canvas.height) * 100}%`,
            `transform:translate(-50%, -50%) rotate(${rotation}deg)`,
          ]
      return [
        ...placement,
        ...(active.clip.kind === 'color' ? [`background-color:${textColor(active.clip,localTicks) ?? 'transparent'}`] : []),
        `object-fit:${isPrimary ? 'contain' : 'fill'}`,
        `opacity:${isPrimary ? 1 : opacity}`,
        `mix-blend-mode:${blend}`,
        `z-index:${active.z}`,
        maskClipPath(active, localTicks),
        videoEffectPreviewStyle(active.clip),
        transition,
      ].filter(Boolean).join(';')
    }
    if (active.clip.kind === 'graphic') return 'visibility:hidden' // The native canvas paints the complete graphic composition.
    const style = active.clip.style
    const color = textColor(active.clip,localTicks)
    if (!color) return 'visibility:hidden'
    const point = positionPoint(active.clip,localTicks)
    if (!point) return 'visibility:hidden'
    const { x,y } = point
    const scaleX = sampleVisualProperty(previewDocument, active.clip, 'scaleX', localTicks)
    const scaleY = sampleVisualProperty(previewDocument, active.clip, 'scaleY', localTicks)
    const rotation = sampleVisualProperty(previewDocument, active.clip, 'rotationDegrees', localTicks)
    const opacity = sampleVisualProperty(previewDocument, active.clip, 'opacity', localTicks)
    const shadow = style.shadowColor
      ? `${style.shadowX ?? 0}px ${style.shadowY ?? 0}px ${style.shadowColor}`
      : 'none'
    return [
      `left:${50 + (x / canvas.width) * 100}%`,
      `top:${50 + (y / canvas.height) * 100}%`,
      `transform:translate(-50%, -50%) rotate(${rotation}deg) scale(${scaleX}, ${scaleY})`,
      `opacity:${opacity}`,
      `color:${color}`,
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
    const canvas = previewDocument.canvas
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
      `filter:blur(${previewDocument.canvas.backgroundBlur ?? 24}px)`,
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
      // These kinds require an independent window; that path displays the native-preview notice.
      case 'concat_dissolve':
      case 'concat_fade_black':
      case 'concat_fade_white':
      case 'procedural':
      case 'film_dissolve':
      case 'additive_dissolve':
      case 'fade_white': return ''
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
    const sourceDuration = previewDocument.sources[clip.sourceId]?.durationTicks ?? bounded
    return Math.max(0, Math.min(sourceDuration, bounded)) / COMPOSITION_TIME_BASE
  }

  function markTransitionSeekLimited(active: ActiveClip): void {
    if (active.transitionRole && activeTransition) limitedTransitionId = activeTransition.transition.id
  }

  function videoMuted(active: ActiveClip): boolean {
    if (nativeAudioRequested || active.clip.kind !== 'video' || active.track.kind !== 'video') return true
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
    const fadeFactor = audioFadeMultiplier(elapsed, clipDurationTicks(clip), clip.fadeInTicks, clip.fadeOutTicks)
    const localTicks = Math.max(0, Math.min(clipDurationTicks(clip), elapsed))
    const crossfadeFactor = active.crossfadeRole && active.boundaryTicks !== undefined && active.crossfadeTicks
      ? audioCrossfadeMultiplier((playhead - (active.boundaryTicks - Math.floor(active.crossfadeTicks / 2))) / active.crossfadeTicks, active.crossfadeRole === 'to', active.crossfadeCurve)
      : 1
    return sampleAudioProperty(clip, 'gain', localTicks) * fadeFactor * crossfadeFactor * previewAudioRoleGain(previewDocument, clip.role)
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
    if (nativeAudioRequested) {
      nativeAudioReady = nativeAudio.ready
      if (nativeAudio.error) {
        nativeAudioError = nativeAudio.error
        toggleCompositionPlayback()
      } else {
        const head = nativeAudio.finished ? compositionDuration() : nativeAudio.timeUs()
        lastNativePlayhead = head
        setCompositionPlayhead(head)
      }
    } else if (lastFrameTime !== null) {
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
    resetMasterPeakHold()
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
    style:aspect-ratio={`${previewDocument.canvas.width} / ${previewDocument.canvas.height}`}
    style={canvasStyle()}
  >
    {#if previewDocument.canvas.backgroundMode === 'blur'}
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
            playing={compositionState.transport.playing && (!nativeAudioRequested || nativeAudioReady) && (active.clip.playbackMode?.mode ?? 'forward') === 'forward'}
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
            playing={compositionState.transport.playing && (!nativeAudioRequested || nativeAudioReady) && (active.clip.playbackMode?.mode ?? 'forward') === 'forward'}
            muted={videoMuted(active)}
            pan={sampleAudioProperty(active.clip, 'pan', Math.max(0, Math.min(clipDurationTicks(active.clip), compositionState.transport.playheadTicks - active.clip.timelineStartTicks)))}
            volume={nativeAudioRequested ? 0 : sampleAudioProperty(
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
      {:else if active.clip.kind === 'color'}
        <div data-color-clip={active.clip.id} class="composition-visual-media" style={visualStyle(active)}></div>
      {:else if active.clip.kind === 'text'}
        <div class="composition-text-layer" style={visualStyle(active)}>{active.clip.text}</div>
      {/if}
    {/each}
    {#if graphicPreviewProject || graphicPreviewRequest.error}
      <div class="graphic-native-preview">
        {#if graphicPreviewProject}
          <NestedCompositionPreview project={graphicPreviewProject} timeUs={compositionState.transport.playheadTicks} renderedOutputs={{}} token={authState.token ?? ''} label={previewDocument.tracks.some(track=>track.kind==='video'&&!track.hidden&&track.transitions?.some(transition=>transition.timeline)) ? "Нативный предпросмотр переходов" : "Предпросмотр графической композиции"} />
        {:else}<p role="alert">{graphicPreviewRequest.error}</p>{/if}
      </div>
    {/if}
    {#each activeVisuals.filter(active => active.clip.id === compositionState.ui.selectedClipId && active.clip.position?.mode === 'path' && active.track.id !== primaryTrack?.id) as active (active.clip.id)}
      <CompositionMotionPathOverlay clip={active.clip} width={previewDocument.canvas.width} height={previewDocument.canvas.height} />
    {/each}
    {#if activeVisuals.length === 0 && !graphicPreviewProject && !graphicPreviewRequest.error}
      <p class="composition-empty-stage">Добавьте видео, изображение или текст</p>
    {/if}
    {#if previewNotes}<span class="composition-preview-badge">{previewNotes}</span>{/if}
  </div>

  {#if !nativeAudioRequested}
  {#each activeAudio as active (`${active.clip.id}-${active.crossfadeRole ?? 'active'}`)}
    {@const media = compositionState.media[active.clip.sourceId]}
    {#if media}
      <CompositionMediaLayer
        kind="audio"
        url={media.url}
        sourceTime={audioSourceTime(active)}
        playing={compositionState.transport.playing}
        volume={audioVolume(active)}
        pan={sampleAudioProperty(active.clip, 'pan', Math.max(0, Math.min(clipDurationTicks(active.clip), compositionState.transport.playheadTicks - active.clip.timelineStartTicks)))}
        audioRole={active.clip.role ?? 'dialogue'}
        playbackRate={clipSpeedAtTimelineTick(active.clip, compositionState.transport.playheadTicks)}
      />
    {/if}
  {/each}

  {/if}
  {#if nativeAudioError}<p role="alert">Аудиопредпросмотр: {nativeAudioError}</p>{/if}
  {#if nativeAudioRequested && compositionState.transport.playing && !nativeAudioReady}<p role="status">Подготовка нативного звука…</p>{/if}

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

<style>
  .graphic-native-preview { position:absolute; inset:0; z-index:20; pointer-events:none; }
</style>
