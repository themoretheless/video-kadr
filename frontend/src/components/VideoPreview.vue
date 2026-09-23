<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from 'vue'
import { beginEditTransaction, buildEditPayload, clientOnlyMode, endEditTransaction, isIdentityCurves, publishPlayerState, setProjectProxyPolicy, state, timelineState, type ProjectProxyPolicy } from '../store'
import {
  primaryCorrectionsActive,
} from '../domain/primary-color'
import { applyPrimaryAndWheelsSrgb, applyPrimaryAndWheelsToImageData } from '../domain/color-wheels'
import { colorWheelsActive } from '../domain/edit'
import {
  applyHslSelectiveEncodedSrgb,
  encodedSrgbToHsl,
  hslSelectionMask,
  hslSelectiveActive,
  type Rgb,
} from '../domain/hsl-selective'
import { derivedTaskState, regenerateMissingBrowserProxy } from '../derived-task-center'
import { browserProxyCapability, deleteBrowserProxyArtifact, resolveBrowserPreviewSource, type ProxyPreviewSource } from '../browser-proxy-artifacts'
import { attachedMulticamOutputBounds, attachedMulticamOutputSeconds, attachedMulticamProgram, resolveAttachedMulticamProgramAt, resolveMulticamAngleSource } from '../multicam'
import { invalidateBackendProxyArtifact, resolveBackendPreviewSource } from '../proxy-preview'
import { PreviewFrameCache, legacySingleClipPreviewEligible, previewFrameKey, previewGraphFingerprint, previewTimelineTick, requestBackendPreviewFrame, sourceMediaTimeToEditedSeconds, type PreviewRenderSettings } from '../optimized-preview-cache'
import { videoScopeFrameBroker, type ScopeAccuracy, type ScopeTapId } from '../video-scopes/frame-broker'
import { videoScopesController } from '../video-scopes/controller'
import { outputColorStatus, sourceColorStatus } from '../domain/color-management'
import RectOverlay from './RectOverlay.vue'
import { PREVIEW_GUIDES, loadEnabledPreviewGuides, saveEnabledPreviewGuides, type PreviewGuide } from '../domain/preview-guides'

const enabledGuides = new Set<PreviewGuide>(loadEnabledPreviewGuides(localStorage))
const guides = reactive<Record<PreviewGuide, boolean>>({
  thirds: enabledGuides.has('thirds'),
  center: enabledGuides.has('center'),
  action: enabledGuides.has('action'),
  title: enabledGuides.has('title'),
})
const anyGuideActive = computed(() =>
  PREVIEW_GUIDES.some(({ key }) => guides[key]))

function toggleGuide(key: PreviewGuide): void {
  guides[key] = !guides[key]
  saveEnabledPreviewGuides(
    localStorage,
    PREVIEW_GUIDES.map(({ key: item }) => item).filter((item) => guides[item]),
  )
}

const videoEl = ref<HTMLVideoElement | null>(null)
const cachedFrameCanvas = ref<HTMLCanvasElement | null>(null)
const cachedFrameVisible = ref(false)
const cachedFrameNeedsCss = ref(true)
const optimizedPreviewStatus = ref<'idle' | 'loading' | 'ready' | 'fallback'>('idle')
const decodedFrameCache = new PreviewFrameCache()
let optimizedPreviewController: AbortController | null = null
let scopeTapController: AbortController | null = null
let optimizedPreviewGeneration = 0
let activeOptimizedPreviewKey: string | null = null
let previewMappingGeneration = 0
let programAngleId: string | null = null
let programPlaybackRate = 1
const multicamProgramKey = computed(() => {
  const program = attachedMulticamProgram()
  return program ? `${program.groupId}:${program.angleId}` : ''
})
const multicamGapActive = computed(() => {
  const bounds = attachedMulticamOutputBounds()
  return Boolean(bounds && (state.playerTime < bounds.start || state.playerTime >= bounds.end))
})
interface ActiveTextLayer {
  id: string
  text: string
  fontSizeRatio: number
  color: string
  xRatio: number
  yRatio: number
  opacity: number
}
const activeTextLayers = computed<ActiveTextLayer[]>(() => {
  const document = timelineState.document
  if (!document) return []
  const sequence = document.sequences.find((item) => item.id === document.activeSequenceId)
  if (!sequence) return []
  const tick = Math.round(state.playerTime * sequence.settings.timeBase)
  const anySolo = sequence.tracks.some((track) => track.solo)
  const layers: ActiveTextLayer[] = []
  for (const track of sequence.tracks) {
    if (track.kind !== 'video' || track.hidden || (anySolo && !track.solo)) continue
    for (const clip of track.clips) {
      if (tick < clip.timelineStartTick || tick >= clip.timelineStartTick + clip.durationTicks) continue
      const media = document.media.find((item) => item.id === clip.mediaId)
      if (media?.kind !== 'text') continue
      const metadata = media.metadata as Record<string, unknown>
      if (metadata.contract !== 'text-layer-v1' || typeof metadata.text !== 'string') continue
      const ratio = (value: unknown, fallback: number) =>
        typeof value === 'number' && Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : fallback
      layers.push({
        id: media.id,
        text: metadata.text,
        fontSizeRatio: typeof metadata.fontSizeRatio === 'number' && metadata.fontSizeRatio > 0 ? metadata.fontSizeRatio : 0.08,
        color: typeof metadata.color === 'string' ? metadata.color : '#ffffff',
        xRatio: ratio(metadata.xRatio, 0.5),
        yRatio: ratio(metadata.yRatio, 0.85),
        opacity: ratio(metadata.opacity, 1),
      })
    }
  }
  return layers
})

function outputPlayerTime(sourceTime: number): number {
  return programAngleId ? attachedMulticamOutputSeconds(programAngleId, sourceTime) ?? sourceTime : sourceTime
}
let pausedPreviewTimer: ReturnType<typeof setTimeout> | null = null
let lastPresentedMediaTime: number | null = null
let frameTrackingId: number | null = null
let lastLiveScopeCaptureAt = 0
const previewSource = ref<ProxyPreviewSource>({ url: '', usingProxy: false, status: 'original' })
const proxyError = ref('')
let previewGeneration = 0
let pendingSwitch: { time: number; playing: boolean; revoke?: () => void } | null = null
let pendingRestoreCleanup: (() => void) | null = null
let outputSeekGeneration = 0
let pendingOutputSeek: { generation: number; outputTime: number; sourceTime: number; angleId: string } | null = null

async function seekOutputTime(outputTime: number): Promise<void> {
  const generation = ++outputSeekGeneration
  const target = await resolveAttachedMulticamProgramAt(outputTime)
  if (generation !== outputSeekGeneration) return
  const el = videoEl.value
  if (!target) {
    const previous = previewSource.value
    const original = state.video?.url ?? ''
    if (programAngleId || previewSource.value.url !== original) {
      pendingSwitch = { time: outputTime, playing: Boolean(el && !el.paused), revoke: previous.revoke }
      previewSource.value = { url: original, usingProxy: false, status: 'original' }
      programAngleId = null
      programPlaybackRate = 1
      previewMappingGeneration++
    } else if (el) el.currentTime = outputTime
    return
  }
  pendingOutputSeek = { generation, outputTime, sourceTime: target.sourceSeconds, angleId: target.angleId }
  publishPlayerState({ time: outputTime, seeking: true })
  const playing = Boolean(el && !el.paused)
  if (programAngleId === target.angleId && previewSource.value.url === target.url) {
    programPlaybackRate = target.rate
    if (el) {
      applyPlayback(el)
      if (Math.abs(el.currentTime - target.sourceSeconds) < 0.001) {
        pendingOutputSeek = null
        publishPlayerState({ time: outputTime, seeking: false })
      } else el.currentTime = target.sourceSeconds
    }
    return
  }
  const previous = previewSource.value
  pendingSwitch = { time: target.sourceSeconds, playing, revoke: previous.revoke }
  previewSource.value = { url: target.url, usingProxy: false, status: 'original' }
  programAngleId = target.angleId
  programPlaybackRate = target.rate
  previewMappingGeneration++
}

const proxyPolicy = computed<ProjectProxyPolicy>(() => timelineState.document?.proxyPolicy ?? 'auto')
const proxyCapability = computed(() => {
  if (!clientOnlyMode) return true
  return browserProxyCapability().supported
})
const optimizedPreviewEligible = computed(() => legacySingleClipPreviewEligible(timelineState.document) && !state.edit.pad)

function releasePreview(source = previewSource.value): void { source.revoke?.() }

function frameIdentity(seconds: number) {
  const video = state.video
  if (!video?.fingerprint) return null
  const editPayload = buildEditPayload()
  // Preserve the media element's actual clock (including VFR timestamps) and
  // only then map it into the edited output clock. Quantizing edited time by
  // source FPS is wrong whenever speed changes that clock.
  const editedSeconds = sourceMediaTimeToEditedSeconds(seconds, video.duration, editPayload)
  if (editedSeconds === null) return null
  let renderWidth = video.width || videoEl.value?.videoWidth || 1280
  let renderHeight = video.height || videoEl.value?.videoHeight || 720
  if (!clientOnlyMode && !state.hslMaskPreview) {
    if (state.edit.cropEnabled) { renderWidth = state.edit.crop.w; renderHeight = state.edit.crop.h }
    if (Math.abs(state.edit.rotate) % 180 === 90) [renderWidth, renderHeight] = [renderHeight, renderWidth]
    if (state.edit.scaleEnabled) {
      const target = state.edit.scale
      if (target.w > 0 && target.h > 0) { renderWidth = target.w; renderHeight = target.h }
      else if (target.w > 0) { renderHeight = Math.max(1, Math.round(renderHeight * target.w / renderWidth)); renderWidth = target.w }
      else if (target.h > 0) { renderWidth = Math.max(1, Math.round(renderWidth * target.h / renderHeight)); renderHeight = target.h }
    }
  }
  const fit = Math.min(1, 1280 / Math.max(renderWidth, renderHeight))
  const width = Math.max(1, Math.round(renderWidth * fit))
  const height = Math.max(1, Math.round(renderHeight * fit))
  const settings: PreviewRenderSettings = {
    width, height, pixelRatioMilli: 1000,
    sourceMode: previewSource.value.usingProxy ? 'proxy' : 'original',
    rendererCompatibility: `browser-canvas-selective-hsl-v4:${state.hslMaskPreview ? 'mask' : 'grade'}:${previewSource.value.mappingIdentity ?? 'original'}:${previewMappingGeneration}`,
  }
  const graphVersion = previewGraphFingerprint(video.fingerprint, {
    edit: state.edit,
    timeline: timelineState.document,
  })
  return { sourceFingerprint: video.fingerprint, graphVersion, timelineTick: previewTimelineTick(editedSeconds), settings }
}

function drawCachedFrame(frame: { bitmap: ImageBitmap; width: number; height: number }): void {
  const canvas = cachedFrameCanvas.value
  if (!canvas) return
  canvas.width = frame.width; canvas.height = frame.height
  canvas.getContext('2d')?.drawImage(frame.bitmap, 0, 0, frame.width, frame.height)
  cachedFrameVisible.value = true
}

function scopeTapIdentity(identity: NonNullable<ReturnType<typeof frameIdentity>>, tapId: ScopeTapId) {
  return {
    sourceFingerprint: identity.sourceFingerprint,
    timelineTick: identity.timelineTick,
    graphVersion: identity.graphVersion,
    tapId,
    colorDescriptorId: 'straight-rgba8-encoded-srgb' as const,
    width: identity.settings.width,
    height: identity.settings.height,
    sourceMode: identity.settings.sourceMode,
    mappingIdentity: previewSource.value.mappingIdentity ?? identity.settings.sourceMode,
  }
}

function scopeGeometryUnavailableReason(): string | null {
  const edit = state.edit
  if (edit.denoise) return 'Pre/Post scopes недоступны: browser preview не применяет denoise точно.'
  if (edit.cropEnabled || edit.rotate || edit.flipH || edit.flipV || edit.censorEnabled) {
    return 'Pre/Post scopes недоступны: browser tap не содержит точную геометрию кадра.'
  }
  return null
}

function postGradeUnavailableReason(): string | null {
  const geometry = scopeGeometryUnavailableReason()
  if (geometry) return geometry
  const edit = state.edit
  if (edit.brightness !== 0 || edit.contrast !== 1 || edit.saturation !== 1 || edit.filter) {
    return 'Post-grade scope ждёт точный кадр: CSS color fallback не анализируется.'
  }
  if (edit.lutId && edit.lutIntensity > 0) return 'Post-grade scope ждёт точный кадр после LUT.'
  if (!isIdentityCurves(edit.curves)) return 'Post-grade scope ждёт точный кадр после кривых.'
  return null
}

function publishScopePixels(
  identity: NonNullable<ReturnType<typeof frameIdentity>>,
  tapId: ScopeTapId,
  accuracy: ScopeAccuracy,
  pixels: ImageData,
): void {
  if (!videoScopesController.state.enabled || videoScopesController.state.tapId !== tapId) return
  videoScopeFrameBroker.publish({
    identity: scopeTapIdentity(identity, tapId),
    accuracy,
    rgba: new Uint8ClampedArray(pixels.data),
  })
}

function renderAnalysisSurface(el: HTMLVideoElement, identity: NonNullable<ReturnType<typeof frameIdentity>>) {
  const surface = document.createElement('canvas')
  surface.width = identity.settings.width
  surface.height = identity.settings.height
  const context = surface.getContext('2d')
  if (!context) return null
  context.drawImage(el, 0, 0, surface.width, surface.height)
  return { surface, context }
}

function publishExactClientTaps(
  identity: NonNullable<ReturnType<typeof frameIdentity>>,
  sourcePixels: ImageData,
  accuracy: ScopeAccuracy,
): ImageData {
  const geometryReason = scopeGeometryUnavailableReason()
  if (!geometryReason) publishScopePixels(identity, 'pre-grade', accuracy, sourcePixels)
  else videoScopeFrameBroker.unavailable('pre-grade', geometryReason)

  const graded = new ImageData(new Uint8ClampedArray(sourcePixels.data), sourcePixels.width, sourcePixels.height)
  const linearActive = primaryCorrectionsActive(state.edit) || colorWheelsActive(state.edit)
  const selectiveActive = hslSelectiveActive(state.edit.hslSelective)
  if (linearActive || selectiveActive) applyPausedColorStages(graded, linearActive, selectiveActive, false)
  const postReason = postGradeUnavailableReason()
  if (!postReason) publishScopePixels(identity, 'post-grade', accuracy, graded)
  else videoScopeFrameBroker.unavailable('post-grade', postReason)
  return graded
}

function captureLiveScopeTaps(mediaTime: number): void {
  if (!videoScopesController.state.enabled || !videoScopesController.state.visible) return
  const now = performance.now()
  if (now - lastLiveScopeCaptureAt < 125) return
  lastLiveScopeCaptureAt = now
  const el = videoEl.value
  if (!el || el.readyState < 2) return
  const identity = frameIdentity(mediaTime)
  if (!identity) return
  const rendered = renderAnalysisSurface(el, identity)
  if (!rendered) return
  const pixels = rendered.context.getImageData(0, 0, rendered.surface.width, rendered.surface.height)
  publishExactClientTaps(identity, pixels, 'exact-live')
}

const GRADE_PAYLOAD_KEYS = [
  'temperature', 'tint', 'highlights', 'shadows', 'colorWheels', 'hslSelective',
  'brightness', 'contrast', 'saturation', 'filter', 'lut', 'curves',
] as const
const FINISHING_PAYLOAD_KEYS = ['sharpen', 'vignette', 'grain', 'pad'] as const

function backendScopePayload(tapId: ScopeTapId): Record<string, unknown> {
  const payload = { ...buildEditPayload() }
  for (const key of FINISHING_PAYLOAD_KEYS) delete payload[key]
  if (tapId === 'pre-grade') for (const key of GRADE_PAYLOAD_KEYS) delete payload[key]
  return payload
}

async function requestBackendScopeTap(seconds: number): Promise<void> {
  if (clientOnlyMode || !videoScopesController.state.enabled || !videoScopesController.state.visible) return
  const tapId = videoScopesController.state.tapId
  const identity = frameIdentity(seconds)
  if (!identity) return
  scopeTapController?.abort()
  const controller = new AbortController()
  scopeTapController = controller
  const requestIdentity = {
    ...identity,
    settings: {
      ...identity.settings,
      rendererCompatibility: `video-scopes-v1:${tapId}:${identity.settings.rendererCompatibility}`,
    },
  }
  try {
    const { blob } = await requestBackendPreviewFrame(backendScopePayload(tapId), requestIdentity, controller.signal)
    const bitmap = await createImageBitmap(blob)
    if (controller.signal.aborted || scopeTapController !== controller) { bitmap.close(); return }
    const surface = document.createElement('canvas')
    surface.width = requestIdentity.settings.width
    surface.height = requestIdentity.settings.height
    const context = surface.getContext('2d')
    if (!context) { bitmap.close(); return }
    context.drawImage(bitmap, 0, 0, surface.width, surface.height)
    bitmap.close()
    publishScopePixels(identity, tapId, 'exact-paused', context.getImageData(0, 0, surface.width, surface.height))
  } catch (error) {
    if (!controller.signal.aborted) {
      videoScopeFrameBroker.unavailable(tapId, error instanceof Error ? error.message : 'Exact backend scope tap недоступен.')
    }
  } finally {
    if (scopeTapController === controller) scopeTapController = null
  }
}

function applyPausedColorStages(
  pixels: ImageData,
  linearActive: boolean,
  selectiveActive: boolean,
  maskPreview: boolean,
): void {
  if (!selectiveActive && !maskPreview) {
    if (linearActive) applyPrimaryAndWheelsToImageData(pixels, state.edit, state.edit)
    return
  }

  const bytes = pixels.data
  for (let index = 0; index < bytes.length; index += 4) {
    const source: Rgb = [bytes[index]! / 255, bytes[index + 1]! / 255, bytes[index + 2]! / 255]
    const primaryAndWheels = linearActive
      ? applyPrimaryAndWheelsSrgb(source, state.edit, state.edit)
      : source
    if (maskPreview) {
      const byte = Math.round(hslSelectionMask(
        encodedSrgbToHsl(primaryAndWheels),
        state.edit.hslSelective.selection,
      ) * 255)
      bytes[index] = byte
      bytes[index + 1] = byte
      bytes[index + 2] = byte
      continue
    }
    const output = applyHslSelectiveEncodedSrgb(primaryAndWheels, state.edit.hslSelective)
    bytes[index] = Math.round(output[0] * 255)
    bytes[index + 1] = Math.round(output[1] * 255)
    bytes[index + 2] = Math.round(output[2] * 255)
  }
}

async function captureCurrentFrame(mediaTime?: number): Promise<void> {
  if (!clientOnlyMode && !state.hslMaskPreview) return
  const el = videoEl.value
  if (!el || typeof createImageBitmap !== 'function' || el.readyState < 2) return
  const identity = frameIdentity(mediaTime ?? el.currentTime)
  if (!identity) return
  try {
    const rendered = renderAnalysisSurface(el, identity)
    if (!rendered) return
    const { surface, context } = rendered
    const sourcePixels = context.getImageData(0, 0, surface.width, surface.height)
    const gradedPixels = publishExactClientTaps(identity, sourcePixels, 'exact-paused')
    const linearActive = primaryCorrectionsActive(state.edit) || colorWheelsActive(state.edit)
    const selectiveActive = hslSelectiveActive(state.edit.hslSelective)
    if (state.hslMaskPreview) {
      const pixels = new ImageData(new Uint8ClampedArray(sourcePixels.data), sourcePixels.width, sourcePixels.height)
      // Keep the encoded-sRGB HSL stage directly after the primary/LGG encode,
      // with a single byte write at the end so the two stages do not quantise
      // the intermediate colour independently.
      applyPausedColorStages(pixels, linearActive, selectiveActive, state.hslMaskPreview)
      context.putImageData(pixels, 0, 0)
    } else context.putImageData(gradedPixels, 0, 0)
    const key = previewFrameKey(identity)
    decodedFrameCache.put(key, await createImageBitmap(surface))
    if (el.paused) {
      const cached = decodedFrameCache.get(key)
      if (cached) {
        cachedFrameNeedsCss.value = !state.hslMaskPreview
        drawCachedFrame(cached)
        optimizedPreviewStatus.value = 'ready'
      }
    }
  } catch { /* preview cache is disposable */ }
}

function showCachedOrRequest(seconds: number): void {
  if (videoScopesController.state.enabled && !clientOnlyMode) void requestBackendScopeTap(seconds)
  optimizedPreviewController?.abort()
  optimizedPreviewController = null
  const generation = ++optimizedPreviewGeneration
  const identity = frameIdentity(seconds)
  if (!identity) { activeOptimizedPreviewKey = null; cachedFrameVisible.value = false; optimizedPreviewStatus.value = 'idle'; return }
  const localKey = previewFrameKey(identity)
  activeOptimizedPreviewKey = localKey
  const cached = decodedFrameCache.get(localKey)
  if (cached) {
    cachedFrameNeedsCss.value = clientOnlyMode && !state.hslMaskPreview
    drawCachedFrame(cached)
    optimizedPreviewStatus.value = 'ready'
    return
  }
  cachedFrameVisible.value = false
  if (clientOnlyMode || state.hslMaskPreview || !optimizedPreviewEligible.value) { optimizedPreviewStatus.value = 'idle'; return }
  const controller = new AbortController(); optimizedPreviewController = controller
  optimizedPreviewStatus.value = 'loading'
  void requestBackendPreviewFrame(buildEditPayload(), identity, controller.signal).then(async ({ blob }) => {
    const bitmap = await createImageBitmap(blob)
    if (generation !== optimizedPreviewGeneration || activeOptimizedPreviewKey !== localKey || controller.signal.aborted || previewFrameKey(frameIdentity(seconds) ?? identity) !== localKey) { bitmap.close(); return }
    decodedFrameCache.put(localKey, bitmap)
    const ready = decodedFrameCache.get(localKey)
    if (ready) { cachedFrameNeedsCss.value = false; drawCachedFrame(ready); optimizedPreviewStatus.value = 'ready' }
  }).catch(error => {
    if (!controller.signal.aborted) { optimizedPreviewStatus.value = 'fallback'; proxyError.value ||= error instanceof Error ? error.message : String(error) }
  })
}

function onPreviewSeeked(event: Event): void {
  const mediaTime = (event.currentTarget as HTMLVideoElement).currentTime
  const pending = pendingOutputSeek
  if (pending && pending.angleId === programAngleId && Math.abs(mediaTime - pending.sourceTime) < 0.05) {
    pendingOutputSeek = null
    publishPlayerState({ time: pending.outputTime, seeking: false })
  } else if (!pending) publishPlayerState({ time: outputPlayerTime(mediaTime), seeking: false })
  const el = videoEl.value as (HTMLVideoElement & { requestVideoFrameCallback?: (callback: (_now: number, metadata: { mediaTime: number }) => void) => number }) | null
  if (clientOnlyMode || state.hslMaskPreview) cachedFrameVisible.value = false
  if (el?.requestVideoFrameCallback) {
    el.requestVideoFrameCallback((_now, metadata) => {
      lastPresentedMediaTime = metadata.mediaTime
      if (clientOnlyMode || state.hslMaskPreview) void captureCurrentFrame(metadata.mediaTime)
      else showCachedOrRequest(metadata.mediaTime)
    })
  } else if (clientOnlyMode || state.hslMaskPreview) void captureCurrentFrame()
  else if (el) showCachedOrRequest(el.currentTime)
}

function onPreviewSeeking(event: Event): void {
  if (!pendingOutputSeek) publishPlayerState({ time: outputPlayerTime((event.currentTarget as HTMLVideoElement).currentTime), seeking: true })
  if (clientOnlyMode || state.hslMaskPreview) showCachedOrRequest((event.currentTarget as HTMLVideoElement).currentTime)
  else { optimizedPreviewController?.abort(); cachedFrameVisible.value = false; optimizedPreviewStatus.value = 'idle' }
}

function requestCurrentBackendFrame(el: HTMLVideoElement): void {
  showCachedOrRequest(lastPresentedMediaTime ?? el.currentTime)
}

function trackPresentedFrames(): void {
  const el = videoEl.value as (HTMLVideoElement & { requestVideoFrameCallback?: (callback: (_now: number, metadata: { mediaTime: number }) => void) => number }) | null
  if (!el?.requestVideoFrameCallback || frameTrackingId !== null) return
  frameTrackingId = el.requestVideoFrameCallback((_now, metadata) => {
    frameTrackingId = null
    lastPresentedMediaTime = metadata.mediaTime
    captureLiveScopeTaps(metadata.mediaTime)
    if (!el.paused) trackPresentedFrames()
  })
}

function onPreviewPaused(event: Event): void {
  const el = event.currentTarget as HTMLVideoElement
  if (!pendingOutputSeek) publishPlayerState({ time: outputPlayerTime(el.currentTime), playing: false })
  if (clientOnlyMode || state.hslMaskPreview) { void captureCurrentFrame(el.currentTime); return }
  requestCurrentBackendFrame(el)
}

function onPreviewPlaying(): void {
  const el = videoEl.value
  if (state.hslMaskPreview) {
    el?.pause()
    return
  }
  if (el && !pendingOutputSeek) publishPlayerState({ time: outputPlayerTime(el.currentTime), playing: true, seeking: el.seeking })
  trackPresentedFrames()
}

async function resolvePreview(): Promise<void> {
  const video = state.video
  const generation = ++previewGeneration
  const program = attachedMulticamProgram()
  let original = video?.url ?? ''
  if (!video) {
    releasePreview(); previewSource.value = { url: '', usingProxy: false, status: 'original' }; return
  }
  let next: ProxyPreviewSource = { url: original, usingProxy: false, status: 'missing' }
  proxyError.value = ''
  try {
    if (program) {
      original = await resolveMulticamAngleSource(program.groupId, program.angleId)
      next = { url: original, usingProxy: false, status: 'original' }
    } else
    if (clientOnlyMode) {
      next = await resolveBrowserPreviewSource(video, proxyPolicy.value, proxyCapability.value)
      if (next.status === 'missing' && video.fingerprint) void regenerateMissingBrowserProxy(video.fingerprint)
    } else {
      next = await resolveBackendPreviewSource(video, proxyPolicy.value, derivedTaskState.tasks)
    }
  } catch (error) {
    proxyError.value = error instanceof Error ? error.message : String(error)
  }
  if (generation !== previewGeneration || state.video?.id !== video.id) { next.revoke?.(); return }
  const previous = previewSource.value
  const el = videoEl.value
  pendingSwitch = { time: program?.sourceSeconds ?? el?.currentTime ?? state.playerTime, playing: Boolean(el && !el.paused), revoke: previous.revoke }
  previewSource.value = next
  programAngleId = program?.angleId ?? null
  programPlaybackRate = program?.rate ?? 1
  previewMappingGeneration++
  if (previous.url === next.url) { pendingSwitch = null; previous.revoke?.() }
}

function changeProxyPolicy(event: Event): void {
  setProjectProxyPolicy((event.target as HTMLSelectElement).value as ProjectProxyPolicy)
}

// While playing, loop within the trim region so the preview reflects the cut.
function onTimeUpdate() {
  const el = videoEl.value
  if (!el) return
  if (!el.paused) cachedFrameVisible.value = false
  if (pendingOutputSeek) return
  const outputTime = outputPlayerTime(el.currentTime)
  publishPlayerState({ time: outputTime, playing: !el.paused, seeking: el.seeking })
  applyPlayback(el)
  if (el.paused) return
  const bounds = attachedMulticamOutputBounds()
  const trimStart = bounds?.start ?? state.edit.trimStart
  const trimEnd = bounds?.end ?? state.edit.trimEnd
  if (outputTime >= trimEnd) {
    void seekOutputTime(trimStart)
  }
}

// Live preview of speed/volume; lightweight colour and flip effects use videoStyle.
function applyPlayback(el: HTMLVideoElement) {
  const s = state.edit.speed * programPlaybackRate
  if (s > 0 && el.playbackRate !== s) el.playbackRate = s
  const v = Math.min(1, Math.max(0, state.edit.volume))
  if (el.volume !== v) el.volume = v
}

watch(
  () => [state.edit.speed, state.edit.volume],
  () => {
    videoScopeFrameBroker.invalidate()
    if (videoEl.value) applyPlayback(videoEl.value)
  },
)

watch(multicamProgramKey, () => void resolvePreview())

// CSS approximation of the colour/flip effects so the user sees them live.
const videoStyle = computed(() => {
  const e = state.edit
  const f: string[] = []
  if (e.brightness) f.push(`brightness(${(1 + e.brightness).toFixed(3)})`)
  if (e.contrast !== 1) f.push(`contrast(${e.contrast})`)
  if (e.saturation !== 1) f.push(`saturate(${e.saturation})`)
  const presets: Record<string, string> = {
    grayscale: 'grayscale(1)',
    sepia: 'sepia(0.6)',
    warm: 'sepia(0.4) saturate(1.3)',
    cold: 'hue-rotate(-12deg) saturate(1.15)',
    'teal-orange': 'contrast(1.1) saturate(1.2) hue-rotate(-6deg)',
    faded: 'contrast(0.85) brightness(1.05) saturate(0.9)',
    noir: 'grayscale(1) contrast(1.4)',
    vintage: 'sepia(0.3) contrast(0.95) saturate(1.1)',
  }
  if (e.filter && presets[e.filter]) f.push(presets[e.filter])
  const sx = e.flipH ? -1 : 1
  const sy = e.flipV ? -1 : 1
  return {
    filter: f.length ? f.join(' ') : undefined,
    transform: sx !== 1 || sy !== 1 ? `scaleX(${sx}) scaleY(${sy})` : undefined,
  }
})

const advancedColorNotice = computed(() => {
  const primaryActive = primaryCorrectionsActive(state.edit)
  const wheelsActive = colorWheelsActive(state.edit)
  const selectiveActive = hslSelectiveActive(state.edit.hslSelective)
  const lutActive = Boolean(state.edit.lutId) && state.edit.lutIntensity > 0
  const curvesActive = !isIdentityCurves(state.edit.curves)
  const active: string[] = []
  if (primaryActive) active.push('Температура / оттенок / света / тени')
  if (wheelsActive) active.push('Lift / Gamma / Gain')
  if (selectiveActive) active.push('Selective HSL')
  if (state.hslMaskPreview) active.push('маска Selective HSL')
  if (lutActive) active.push('LUT')
  if (curvesActive) active.push('кривые')
  return active.length ? `${active.join(', ')} включены.` : ''
})

const advancedColorDetail = computed(() => {
  if (state.hslMaskPreview) return 'Точная маска показана на остановленном кадре: белое выбрано, серое — растушёвка, чёрное исключено.'
  if (!clientOnlyMode) return 'Точный кадр появляется после остановки или перемотки; во время воспроизведения используется быстрый fallback.'
  const primaryActive = primaryCorrectionsActive(state.edit)
  const wheelsActive = colorWheelsActive(state.edit)
  const selectiveActive = hslSelectiveActive(state.edit.hslSelective)
  const advancedActive = Boolean(state.edit.lutId) && state.edit.lutIntensity > 0
    || !isIdentityCurves(state.edit.curves)
  const details: string[] = []
  if (primaryActive || wheelsActive || selectiveActive) details.push('Точная primary/Lift/Gamma/Gain/Selective HSL-коррекция появляется на кадре после паузы или перемотки; во время воспроизведения она не имитируется CSS.')
  if (advancedActive) details.push('Точный LUT и кривые доступны после экспорта.')
  return details.join(' ')
})

// Reload the player when a new source is imported.
watch(
  () => [state.video?.id, state.video?.url, state.video?.fingerprint, proxyPolicy.value,
    derivedTaskState.tasks.map(task => `${task.id}:${task.state}:${task.idempotencyKey}`).join('|')],
  () => void resolvePreview(), { immediate: true },
)

watch(
  () => [state.video?.fingerprint, JSON.stringify(state.edit), state.hslMaskPreview, JSON.stringify(timelineState.document), previewSource.value.usingProxy, previewSource.value.url, previewSource.value.mappingIdentity],
  () => {
    optimizedPreviewController?.abort(); optimizedPreviewController = null; optimizedPreviewGeneration++; activeOptimizedPreviewKey = null; cachedFrameVisible.value = false; optimizedPreviewStatus.value = 'idle'
    if (pausedPreviewTimer) clearTimeout(pausedPreviewTimer)
    const el = videoEl.value
    if (state.hslMaskPreview && el && !el.paused) el.pause()
    if (el?.paused) {
      pausedPreviewTimer = setTimeout(() => {
        pausedPreviewTimer = null
        const current = videoEl.value
        if (!current?.paused) return
        if (clientOnlyMode || state.hslMaskPreview) void captureCurrentFrame(current.currentTime)
        else requestCurrentBackendFrame(current)
      }, 180)
    }
  },
)

watch(() => previewSource.value.url, () => {
  videoScopeFrameBroker.invalidate()
  const el = videoEl.value
  if (!el) return
  const frameAware = el as HTMLVideoElement & { cancelVideoFrameCallback?: (id: number) => void }
  if (frameTrackingId !== null) frameAware.cancelVideoFrameCallback?.(frameTrackingId)
  frameTrackingId = null
  lastPresentedMediaTime = null
  pendingRestoreCleanup?.()
  const switching = pendingSwitch ?? { time: el.currentTime, playing: !el.paused }
  pendingSwitch = null
  el.load()
  let released = false
  const release = () => { if (!released) { released = true; switching.revoke?.() } }
  const restore = () => {
    el.currentTime = Math.min(switching.time, Number.isFinite(el.duration) ? el.duration : switching.time)
    applyPlayback(el)
    if (switching.playing) void el.play().catch(() => undefined)
    release()
    pendingRestoreCleanup = null
  }
  const cleanup = () => {
    el.removeEventListener('loadedmetadata', restore)
    release()
    if (pendingRestoreCleanup === cleanup) pendingRestoreCleanup = null
  }
  pendingRestoreCleanup = cleanup
  el.addEventListener('loadedmetadata', restore, { once: true })
})

watch(
  () => [
    videoScopesController.state.enabled,
    videoScopesController.state.visible,
    videoScopesController.state.tapId,
    videoScopesController.state.kinds.join('|'),
  ],
  () => {
    const el = videoEl.value
    if (!videoScopesController.state.enabled || !videoScopesController.state.visible || !el?.paused) return
    if (clientOnlyMode || state.hslMaskPreview) void captureCurrentFrame(el.currentTime)
    else void requestBackendScopeTap(el.currentTime)
  },
)

function onPreviewError(): void {
  outputSeekGeneration++
  pendingOutputSeek = null
  publishPlayerState({ playing: false, seeking: false })
  if (!previewSource.value.usingProxy || !state.video) return
  const previous = previewSource.value
  previewSource.value = { url: state.video.url, usingProxy: false, status: 'stale' }
  previous.revoke?.()
  proxyError.value = 'Proxy недоступен; предпросмотр продолжен с оригинала.'
  if (clientOnlyMode && state.video.fingerprint) {
    const failedFingerprint = state.video.fingerprint
    void deleteBrowserProxyArtifact(failedFingerprint)
      .catch(() => undefined)
      .finally(() => regenerateMissingBrowserProxy(failedFingerprint))
  } else if (previous.artifactKey) {
    void invalidateBackendProxyArtifact(previous.artifactKey).catch(() => undefined)
  }
}

onBeforeUnmount(() => {
  outputSeekGeneration++
  pendingOutputSeek = null
  publishPlayerState({ playing: false, seeking: false })
  videoScopeFrameBroker.invalidate()
  scopeTapController?.abort()
  scopeTapController = null
  previewGeneration++
  pendingRestoreCleanup?.()
  pendingSwitch?.revoke?.()
  pendingSwitch = null
  releasePreview()
  decodedFrameCache.clear()
  optimizedPreviewController?.abort()
  optimizedPreviewController = null
  optimizedPreviewGeneration++
  activeOptimizedPreviewKey = null
  if (pausedPreviewTimer) clearTimeout(pausedPreviewTimer)
  const tracked = videoEl.value as (HTMLVideoElement & { cancelVideoFrameCallback?: (id: number) => void }) | null
  if (frameTrackingId !== null) tracked?.cancelVideoFrameCallback?.(frameTrackingId)
})

// Player bridge: react to seek requests and play/pause toggles from hotkeys.
watch(
  () => state.seekTo,
  (t) => {
    if (t != null && videoEl.value) {
      void seekOutputTime(t)
      state.seekTo = null
    }
  },
)

watch(
  () => state.playToggle,
  () => {
    const el = videoEl.value
    if (!el) return
    if (el.paused) void el.play()
    else el.pause()
  },
)

function fmtDuration(t: number): string {
  if (!isFinite(t)) return '0:00'
  const m = Math.floor(t / 60)
  const s = Math.floor(t % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

function fmtSize(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} ГБ`
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} МБ`
  return `${Math.max(1, Math.round(bytes / 1024))} КБ`
}

const meta = computed(() => {
  const v = state.video
  if (!v) return [] as string[]
  const parts: string[] = []
  if (v.width && v.height) parts.push(`${v.width}×${v.height}`)
  parts.push(`${fmtDuration(v.duration)}`)
  if (v.fps) parts.push(`${v.fps.toFixed(1)} fps`)
  const codecs = [v.vcodec, v.acodec].filter(Boolean).join(' / ')
  if (codecs) parts.push(codecs)
  if (typeof v.sizeBytes === 'number') parts.push(fmtSize(v.sizeBytes))
  return parts
})
const sourceColor = computed(() => sourceColorStatus(state.video?.colorManagement))
const outputColor = computed(() => outputColorStatus(state.edit.format))
</script>

<template>
  <div class="card preview">
    <h2 v-if="state.video?.title" class="preview-title" :title="state.video.title">
      {{ state.video.title }}
    </h2>
    <div id="color-preview-surface" class="player-wrap">
      <video
        ref="videoEl"
        class="player"
        :src="programAngleId ? previewSource.url : (previewSource.url || state.video?.url)"
        :style="videoStyle"
        :muted="state.edit.mute"
        controls
        playsinline
        @timeupdate="onTimeUpdate"
        @playing="onPreviewPlaying"
        @pause="onPreviewPaused"
        @seeking="onPreviewSeeking"
        @seeked="onPreviewSeeked"
        @error="onPreviewError"
      ></video>
      <div v-if="multicamGapActive" class="multicam-gap" role="status" aria-live="polite">Вне multicam-клипа</div>
      <div v-if="activeTextLayers.length" class="preview-text-layer-host" aria-hidden="true">
        <div
          v-for="layer in activeTextLayers"
          :key="layer.id"
          class="preview-text-layer"
          :style="{
            left: `${layer.xRatio * 100}%`,
            top: `${layer.yRatio * 100}%`,
            fontSize: `${layer.fontSizeRatio * 100}cqh`,
            color: layer.color,
            opacity: layer.opacity,
          }"
        >{{ layer.text }}</div>
      </div>
      <canvas v-show="cachedFrameVisible" ref="cachedFrameCanvas" class="preview-frame-cache" :style="cachedFrameNeedsCss ? videoStyle : undefined" aria-hidden="true"></canvas>
      <svg
        v-if="anyGuideActive"
        class="preview-guides"
        viewBox="0 0 100 100"
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        <template v-if="guides.thirds">
          <line x1="33.333" y1="0" x2="33.333" y2="100" class="preview-guide-line" />
          <line x1="66.667" y1="0" x2="66.667" y2="100" class="preview-guide-line" />
          <line x1="0" y1="33.333" x2="100" y2="33.333" class="preview-guide-line" />
          <line x1="0" y1="66.667" x2="100" y2="66.667" class="preview-guide-line" />
        </template>
        <template v-if="guides.center">
          <line x1="50" y1="0" x2="50" y2="100" class="preview-guide-line" />
          <line x1="0" y1="50" x2="100" y2="50" class="preview-guide-line" />
        </template>
        <rect v-if="guides.action" x="5" y="5" width="90" height="90" class="preview-guide-action" />
        <rect v-if="guides.title" x="10" y="10" width="80" height="80" class="preview-guide-title" />
      </svg>
      <RectOverlay
        v-if="state.video && state.edit.cropEnabled"
        :rect="state.edit.crop"
        @update:rect="state.edit.crop = $event"
        @interaction-start="beginEditTransaction('crop-drag')"
        @interaction-end="endEditTransaction"
      />
      <RectOverlay
        v-if="state.video && state.edit.censorEnabled"
        :rect="state.edit.censor"
        color="var(--danger)"
        mode="mask"
        @update:rect="state.edit.censor = $event"
        @interaction-start="beginEditTransaction('censor-drag')"
        @interaction-end="endEditTransaction"
      />
    </div>
    <div v-if="state.video" class="meta">
      <span v-for="(m, i) in meta" :key="i" class="meta-chip">{{ m }}</span>
      <span class="meta-chip" :title="sourceColor.warning ?? undefined" role="status">
        {{ sourceColor.label }}<span v-if="sourceColor.warning" aria-hidden="true"> ⚠</span>
      </span>
      <span class="meta-chip" :title="outputColor.warning ?? undefined" role="status">
        {{ outputColor.label }}<span v-if="outputColor.warning" aria-hidden="true"> ⚠</span>
      </span>
    </div>
    <p v-if="state.video && sourceColor.warning" class="hint" role="status">{{ sourceColor.warning }}</p>
    <p v-if="state.video && outputColor.warning" class="hint" role="status">{{ outputColor.warning }}</p>
    <div v-if="state.video" class="proxy-controls">
      <span class="preview-guides-controls" role="group" aria-label="Направляющие превью">
        <button
          v-for="guide in PREVIEW_GUIDES"
          :key="guide.key"
          class="btn ghost sm"
          :class="{ active: guides[guide.key] }"
          :aria-pressed="guides[guide.key]"
          :title="`Направляющие: ${guide.label}`"
          @click="toggleGuide(guide.key)"
        >{{ guide.label }}</button>
      </span>
      <label for="proxy-policy">Источник предпросмотра</label>
      <select id="proxy-policy" :value="proxyPolicy" @change="changeProxyPolicy">
        <option value="auto">Авто</option>
        <option value="proxy">Proxy</option>
        <option value="original">Оригинал</option>
      </select>
      <span class="meta-chip" role="status">
        {{ previewSource.usingProxy ? 'Proxy' : previewSource.status === 'unsupported' ? 'Proxy не поддерживается' : previewSource.status === 'stale' ? 'Proxy устарел — оригинал' : previewSource.status === 'missing' ? 'Proxy готовится — оригинал' : 'Оригинал' }}
      </span>
      <span v-if="optimizedPreviewStatus !== 'idle'" class="meta-chip" role="status">
        {{ optimizedPreviewStatus === 'loading' ? 'Оптимизация кадра…' : optimizedPreviewStatus === 'ready' ? (clientOnlyMode ? 'Кадр в браузере' : 'Оптимизированный кадр') : 'Кэш недоступен — fallback' }}
      </span>
    </div>
    <p v-if="proxyError" class="hint" role="status">{{ proxyError }}</p>
    <p v-if="!clientOnlyMode && !optimizedPreviewEligible" class="hint" role="status">
      Оптимизированный кадр недоступен для изменённой topology timeline до подключения render graph; используется обычный preview.
    </p>
    <p v-if="previewSource.usingProxy && previewSource.hasAudio === false" class="hint" role="status">
      Этот browser proxy без аудиодорожки; выберите «Оригинал» для контроля звука.
    </p>
    <p v-if="advancedColorNotice" class="preview-color-notice" role="status">
      <strong>{{ advancedColorNotice }}</strong>
      {{ advancedColorDetail }}
    </p>
    <p v-if="state.video" class="hint">Обрезка зациклена внутри выбранного отрезка. Экспорт всегда читает оригинал.</p>
  </div>
</template>

<style scoped>
.preview-guides {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  z-index: 5;
  pointer-events: none;
}

.preview-guide-line {
  stroke: rgb(255 255 255 / 55%);
  stroke-width: 1;
  vector-effect: non-scaling-stroke;
}

.preview-guide-action {
  fill: none;
  stroke: rgb(255 255 255 / 70%);
  stroke-width: 1;
  stroke-dasharray: 6 4;
  vector-effect: non-scaling-stroke;
}

.preview-guide-title {
  fill: none;
  stroke: rgb(255 211 42 / 85%);
  stroke-width: 1;
  stroke-dasharray: 3 3;
  vector-effect: non-scaling-stroke;
}

.preview-guides-controls {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-inline-end: 8px;
}

.preview-text-layer-host {
  position: absolute;
  inset: 0;
  z-index: 4;
  container-type: size;
  pointer-events: none;
}

.preview-text-layer {
  position: absolute;
  max-width: 90%;
  transform: translate(-50%, -50%);
  white-space: pre-wrap;
  text-align: center;
  font-family: system-ui, sans-serif;
  font-weight: 600;
  line-height: 1.15;
  text-shadow: 0 1px 2px rgb(0 0 0 / 45%);
}

.multicam-gap {
  position: absolute;
  inset: 0;
  z-index: 3;
  display: grid;
  place-items: center;
  background: #000;
  color: var(--muted);
  pointer-events: none;
}
</style>
