import { MEMFS_FALLBACK_MAX_INPUT_BYTES, MEMFS_MAX_OUTPUT_BYTES } from './browser-resource-plan'

const ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/
const SHA256 = /^[a-f0-9]{64}$/
const MAX_SAFE = Number.MAX_SAFE_INTEGER
const MAX_CLIPS = 96

export type BrowserTimelineMediaKind = 'video' | 'audio' | 'image' | 'text'

export interface BrowserTimelineTextLayerStyle {
  text: string
  fontSizeRatio: number
  color: string
  xRatio: number
  yRatio: number
  opacity: number
}

export type BrowserTimelineVideoFadeMode = 'none' | 'alpha' | 'black' | 'xfade'

export type BrowserTimelineXfadeName =
  | 'wipeleft' | 'wiperight' | 'wipeup' | 'wipedown'
  | 'slideleft' | 'slideright' | 'slideup' | 'slidedown'
  | 'circleopen' | 'circleclose'

const XFADE_NAMES: readonly string[] = [
  'wipeleft', 'wiperight', 'wipeup', 'wipedown',
  'slideleft', 'slideright', 'slideup', 'slidedown',
  'circleopen', 'circleclose',
]

export interface BrowserTimelineClipFades {
  videoInTicks: number
  videoInMode: BrowserTimelineVideoFadeMode
  videoInXfade: BrowserTimelineXfadeName | null
  videoOutTicks: number
  audioInTicks: number
  audioOutTicks: number
}

const NO_FADES: BrowserTimelineClipFades = {
  videoInTicks: 0, videoInMode: 'none', videoInXfade: null, videoOutTicks: 0, audioInTicks: 0, audioOutTicks: 0,
}

export interface BrowserTimelineRenderClip {
  id: string
  assetRef: string | null
  fingerprint: string | null
  mediaKind: BrowserTimelineMediaKind
  style: BrowserTimelineTextLayerStyle | null
  trackKind: 'video' | 'audio'
  trackIndex: number
  audioEnabled: boolean
  timelineStartTick: number
  durationTicks: number
  sourceInTick: number
  sourceOutTick: number
  opacity: number | null
  fades: BrowserTimelineClipFades
}

/** Inputs map key: assetRef for media clips, `text:<clipId>` for rasterized text layers. */
export function timelineClipInputKey(clip: Pick<BrowserTimelineRenderClip, 'assetRef' | 'id'>): string {
  return clip.assetRef ?? `text:${clip.id}`
}

export interface BrowserTimelineRender {
  contract: 'timeline-render-v1'
  timeBase: number
  durationTicks: number
  target: { width: number; height: number; fps: number }
  clips: BrowserTimelineRenderClip[]
}

export interface MaterializedTimelineInput {
  path: string
  sizeBytes: number
  hasAudio: boolean
}

export interface BrowserTimelineResourceEstimate {
  uniqueInputBytes: number
  estimatedOutputBytes: number
  estimatedPeakMemoryBytes: number
  risk: 'safe' | 'warning' | 'blocked'
  reason: string | null
}

export interface BrowserTimelineArgvOptions {
  inputs: ReadonlyMap<string, MaterializedTimelineInput>
  workerFs: boolean
  memoryBudgetBytes: number
  maxOutputBytes?: number
  postVideoFilters?: readonly string[]
  lutFilter?: string | null
  lutIntensity?: number
  postVideoAfterLutFilters?: readonly string[]
  postAudioFilters?: readonly string[]
  output: { filename: string; args: readonly string[] }
  muteAudio?: boolean
}

export interface BrowserTimelineArgvPlan {
  argv: string[]
  inputAssetRefs: string[]
  expectedDurationSeconds: number
  resources: BrowserTimelineResourceEstimate
}

function finite(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

export interface BrowserTimelineTiming {
  trimStartSeconds: number
  trimEndSeconds: number
  cutStartSeconds: number | null
  cutEndSeconds: number | null
  selectedSeconds: number
  outputSeconds: number
}

/** Normalize export-window trim and a middle cut against the flattened
 * timeline clock (which always starts at zero). */
export function browserTimelineTiming(
  value: unknown,
  edit: Record<string, unknown>,
): BrowserTimelineTiming {
  const contract = parseBrowserTimelineRender(value)
  const duration = contract.durationTicks / contract.timeBase
  const trim = edit.trim && typeof edit.trim === 'object' && !Array.isArray(edit.trim)
    ? edit.trim as Record<string, unknown> : null
  const trimStartSeconds = Math.max(0, Math.min(duration, finite(trim?.start, 0)))
  const trimEndSeconds = Math.max(trimStartSeconds, Math.min(duration, finite(trim?.end, duration)))
  const segments = Array.isArray(edit.segments)
    ? edit.segments.filter(item => item && typeof item === 'object' && !Array.isArray(item)) as Record<string, unknown>[]
    : []
  let cutStartSeconds: number | null = null
  let cutEndSeconds: number | null = null
  let removedSeconds = 0
  if (segments.length === 2) {
    const absoluteCutStart = finite(segments[0]?.end, 0)
    const absoluteCutEnd = finite(segments[1]?.start, 0)
    const overlapStart = Math.max(trimStartSeconds, Math.min(trimEndSeconds, absoluteCutStart))
    const overlapEnd = Math.max(overlapStart, Math.min(trimEndSeconds, absoluteCutEnd))
    if (overlapEnd > overlapStart) {
      cutStartSeconds = overlapStart - trimStartSeconds
      cutEndSeconds = overlapEnd - trimStartSeconds
      removedSeconds = overlapEnd - overlapStart
    }
  }
  const selectedSeconds = Math.max(0, trimEndSeconds - trimStartSeconds - removedSeconds)
  const speed = Math.max(0.5, Math.min(2, finite(edit.speed, 1)))
  return { trimStartSeconds, trimEndSeconds, cutStartSeconds, cutEndSeconds, selectedSeconds, outputSeconds: selectedSeconds / speed }
}

function object(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`invalid ${label}`)
  return value as Record<string, unknown>
}

function exact(value: Record<string, unknown>, keys: readonly string[], label: string): void {
  const allowed = new Set(keys)
  if (Object.keys(value).some(key => !allowed.has(key))) throw new Error(`invalid ${label} keys`)
}

function id(value: unknown, label: string): string {
  if (typeof value !== 'string' || !ID.test(value)) throw new Error(`invalid ${label}`)
  return value
}

function integer(value: unknown, label: string, positive = false): number {
  if (!Number.isSafeInteger(value) || (value as number) < (positive ? 1 : 0) || (value as number) > MAX_SAFE) throw new Error(`invalid ${label}`)
  return value as number
}

function finitePositive(value: unknown, label: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) throw new Error(`invalid ${label}`)
  return value as number
}

/** Parse an untrusted timeline render contract. Unknown fields are rejected so
 * the renderer never silently assigns semantics to a newer contract. */
export function parseBrowserTimelineRender(value: unknown): BrowserTimelineRender {
  const root = object(value, 'timeline render')
  exact(root, ['contract', 'timeBase', 'durationTicks', 'target', 'clips'], 'timeline render')
  if (root.contract !== 'timeline-render-v1') throw new Error('invalid timeline render contract')
  const timeBase = integer(root.timeBase, 'timeline timeBase', true)
  const durationTicks = integer(root.durationTicks, 'timeline durationTicks', true)
  const targetRaw = object(root.target, 'timeline target')
  exact(targetRaw, ['width', 'height', 'fps'], 'timeline target')
  const target = {
    width: integer(targetRaw.width, 'timeline target.width', true),
    height: integer(targetRaw.height, 'timeline target.height', true),
    fps: finitePositive(targetRaw.fps, 'timeline target.fps'),
  }
  if (target.width > 8192 || target.height > 8192 || target.fps > 240) throw new Error('invalid timeline target bounds')
  if (!Array.isArray(root.clips) || root.clips.length < 1 || root.clips.length > MAX_CLIPS) throw new Error('invalid timeline clips')
  const clipIds = new Set<string>()
  const clips = root.clips.map((item, index): BrowserTimelineRenderClip => {
    const raw = object(item, `timeline clip ${index}`)
    const allowed = [
      'id', 'mediaKind', 'trackKind', 'trackIndex', 'audioEnabled',
      'timelineStartTick', 'durationTicks', 'sourceInTick', 'sourceOutTick', 'opacity', 'fades',
    ]
    const isTextKey = (item as Record<string, unknown>).mediaKind === 'text'
    exact(raw, isTextKey ? [...allowed, 'style'] : [...allowed, 'assetRef', 'fingerprint'], `timeline clip ${index}`)
    const clipId = id(raw.id, 'timeline clip.id')
    if (clipIds.has(clipId)) throw new Error('duplicate timeline clip')
    clipIds.add(clipId)
    if (raw.mediaKind !== 'video' && raw.mediaKind !== 'audio' && raw.mediaKind !== 'image' && raw.mediaKind !== 'text') throw new Error('invalid timeline mediaKind')
    const isText = raw.mediaKind === 'text'
    let assetRef: string | null = null
    let fingerprint: string | null = null
    let style: BrowserTimelineTextLayerStyle | null = null
    if (isText) {
      if ('assetRef' in raw || 'fingerprint' in raw) throw new Error('invalid timeline text clip identity')
      const rawStyle = object(raw.style, `timeline text style ${index}`)
      exact(rawStyle, ['text', 'fontSizeRatio', 'color', 'xRatio', 'yRatio', 'opacity'], `timeline text style ${index}`)
      if (typeof rawStyle.text !== 'string' || !rawStyle.text.trim() || rawStyle.text.length > 2000) throw new Error('invalid timeline text content')
      if (typeof rawStyle.fontSizeRatio !== 'number' || !Number.isFinite(rawStyle.fontSizeRatio) || rawStyle.fontSizeRatio <= 0 || rawStyle.fontSizeRatio > 1) throw new Error('invalid timeline text fontSizeRatio')
      if (typeof rawStyle.color !== 'string' || !/^#[0-9a-f]{6}$/i.test(rawStyle.color)) throw new Error('invalid timeline text color')
      const unit = (value: unknown, fallback: number) =>
        typeof value === 'number' && Number.isFinite(value) && value >= 0 && value <= 1 ? value : fallback
      style = {
        text: rawStyle.text,
        fontSizeRatio: rawStyle.fontSizeRatio,
        color: rawStyle.color,
        xRatio: unit(rawStyle.xRatio, 0.5),
        yRatio: unit(rawStyle.yRatio, 0.85),
        opacity: unit(rawStyle.opacity, 1),
      }
    } else {
      if ('style' in raw) throw new Error('invalid timeline media clip keys')
      if (typeof raw.fingerprint !== 'string' || !SHA256.test(raw.fingerprint)) throw new Error('invalid timeline fingerprint')
      assetRef = id(raw.assetRef, 'timeline clip.assetRef')
      fingerprint = raw.fingerprint
    }
    if (raw.trackKind !== 'video' && raw.trackKind !== 'audio') throw new Error('invalid timeline trackKind')
    const trackIndex = integer(raw.trackIndex, 'timeline clip.trackIndex')
    if (typeof raw.audioEnabled !== 'boolean') throw new Error(`invalid timeline clip ${index}.audioEnabled`)
    const timelineStartTick = integer(raw.timelineStartTick, 'timeline clip.timelineStartTick')
    const clipDuration = integer(raw.durationTicks, 'timeline clip.durationTicks', true)
    const sourceInTick = integer(raw.sourceInTick, 'timeline clip.sourceInTick')
    const sourceOutTick = integer(raw.sourceOutTick, 'timeline clip.sourceOutTick')
    if (timelineStartTick + clipDuration > durationTicks) throw new Error('invalid timeline clip range')
    if (raw.mediaKind === 'image' || isText ? sourceOutTick <= 0 : sourceOutTick <= sourceInTick) throw new Error('invalid timeline clip source range')
    if (sourceOutTick - sourceInTick !== clipDuration && raw.mediaKind !== 'image' && !isText) throw new Error('invalid timeline clip source duration')
    let opacity: number | null = null
    if (raw.opacity !== undefined && raw.opacity !== null) {
      if (typeof raw.opacity !== 'number' || !Number.isFinite(raw.opacity) || raw.opacity < 0 || raw.opacity > 1) {
        throw new Error('invalid timeline clip opacity')
      }
      opacity = raw.opacity
    }
    let fades: BrowserTimelineClipFades = NO_FADES
    if (raw.fades !== undefined) {
      const fadesRaw = object(raw.fades, `timeline clip ${index}.fades`)
      exact(fadesRaw, ['videoInTicks', 'videoInMode', 'videoInXfade', 'videoOutTicks', 'audioInTicks', 'audioOutTicks'], `timeline clip ${index}.fades`)
      const videoInTicks = integer(fadesRaw.videoInTicks, `timeline clip ${index}.fades.videoInTicks`)
      const videoOutTicks = integer(fadesRaw.videoOutTicks, `timeline clip ${index}.fades.videoOutTicks`)
      const audioInTicks = integer(fadesRaw.audioInTicks, `timeline clip ${index}.fades.audioInTicks`)
      const audioOutTicks = integer(fadesRaw.audioOutTicks, `timeline clip ${index}.fades.audioOutTicks`)
      const videoInMode = fadesRaw.videoInMode
      if (videoInMode !== 'none' && videoInMode !== 'alpha' && videoInMode !== 'black' && videoInMode !== 'xfade') throw new Error('invalid timeline fades.videoInMode')
      if ((videoInTicks > 0) !== (videoInMode !== 'none')) throw new Error('invalid timeline video fade mode')
      let videoInXfade: BrowserTimelineXfadeName | null = null
      if (fadesRaw.videoInXfade !== undefined && fadesRaw.videoInXfade !== null) {
        if (typeof fadesRaw.videoInXfade !== 'string' || !XFADE_NAMES.includes(fadesRaw.videoInXfade)) {
          throw new Error('invalid timeline fades.videoInXfade')
        }
        videoInXfade = fadesRaw.videoInXfade as BrowserTimelineXfadeName
      }
      if ((videoInMode === 'xfade') !== (videoInXfade !== null)) throw new Error('invalid timeline xfade transition name')
      if (videoInTicks + videoOutTicks > clipDuration || audioInTicks + audioOutTicks > clipDuration) {
        throw new Error('invalid timeline fade window')
      }
      fades = { videoInTicks, videoInMode, videoInXfade, videoOutTicks, audioInTicks, audioOutTicks }
    }
    return {
      id: clipId,
      assetRef,
      fingerprint,
      mediaKind: raw.mediaKind,
      style,
      trackKind: raw.trackKind,
      trackIndex,
      audioEnabled: raw.audioEnabled,
      timelineStartTick,
      durationTicks: clipDuration,
      sourceInTick,
      sourceOutTick,
      opacity,
      fades,
    }
  })
  return { contract: 'timeline-render-v1', timeBase, durationTicks, target, clips }
}

export function estimateBrowserTimelineResources(
  contract: BrowserTimelineRender,
  inputs: ReadonlyMap<string, MaterializedTimelineInput>,
  options: { workerFs: boolean; memoryBudgetBytes: number; maxOutputBytes?: number },
): BrowserTimelineResourceEstimate {
  const uniqueRefs = [...new Set(contract.clips.map(timelineClipInputKey))]
  let uniqueInputBytes = 0
  for (const ref of uniqueRefs) {
    const input = inputs.get(ref)
    if (!input || !Number.isSafeInteger(input.sizeBytes) || input.sizeBytes < 0) throw new Error(`missing timeline input ${ref}`)
    uniqueInputBytes += input.sizeBytes
    if (!Number.isSafeInteger(uniqueInputBytes)) throw new Error('invalid timeline aggregate input size')
  }
  const seconds = contract.durationTicks / contract.timeBase
  const rawRate = contract.target.width * contract.target.height * contract.target.fps * 1.5
  const maxOutputBytes = options.maxOutputBytes ?? MEMFS_MAX_OUTPUT_BYTES
  const estimatedOutputBytes = Math.min(maxOutputBytes, Math.ceil(seconds * Math.min(rawRate, 8_000_000 / 8)))
  const estimatedPeakMemoryBytes = estimatedOutputBytes + 64 * 1024 * 1024 + (options.workerFs ? 0 : uniqueInputBytes)
  const memfsBlocked = !options.workerFs && uniqueInputBytes > MEMFS_FALLBACK_MAX_INPUT_BYTES
  const memoryBlocked = estimatedPeakMemoryBytes > options.memoryBudgetBytes
  return {
    uniqueInputBytes, estimatedOutputBytes, estimatedPeakMemoryBytes,
    risk: memfsBlocked || memoryBlocked ? 'blocked' : estimatedPeakMemoryBytes > options.memoryBudgetBytes * 0.8 ? 'warning' : 'safe',
    reason: memfsBlocked ? 'Timeline inputs exceed bounded MEMFS fallback; WORKERFS is required.' : memoryBlocked ? 'Timeline export exceeds the browser memory budget.' : null,
  }
}

function decimal(value: number, label: string): string {
  if (!Number.isFinite(value) || value < 0) throw new Error(`invalid ${label}`)
  return value.toFixed(9).replace(/\.?0+$/, '') || '0'
}

function safePath(value: string, label: string): string {
  if (!value || /[\0\r\n]/.test(value)) throw new Error(`invalid ${label}`)
  return value
}

function trustedFilters(filters: readonly string[] | undefined): string[] {
  return (filters ?? []).map(filter => {
    if (!filter || [';', '[', ']', '\r', '\n'].some(character => filter.includes(character))) throw new Error('invalid injected FFmpeg filter')
    return filter
  })
}

/** Build argv for compositing a multi-clip timeline: visible video tracks are
 * overlaid bottom-up onto a black canvas at their timeline offsets, and every
 * audio-bearing enabled clip is delayed and amixed. Files must already be
 * materialized at the supplied virtual paths. */
export function buildBrowserTimelineFfmpegArgv(value: unknown, options: BrowserTimelineArgvOptions): BrowserTimelineArgvPlan {
  const contract = parseBrowserTimelineRender(value)
  const resources = estimateBrowserTimelineResources(contract, options.inputs, options)
  if (resources.risk === 'blocked') throw new Error(resources.reason ?? 'Timeline export is blocked')
  const refs = [...new Set(contract.clips.map(timelineClipInputKey))]
  const mediaKindByRef = new Map<string, BrowserTimelineMediaKind>()
  for (const clip of contract.clips) {
    const key = timelineClipInputKey(clip)
    const previous = mediaKindByRef.get(key)
    if (previous && previous !== clip.mediaKind) throw new Error('inconsistent timeline asset kind')
    mediaKindByRef.set(key, clip.mediaKind)
  }
  refs.sort()
  const inputIndex = new Map(refs.map((ref, index) => [ref, index]))
  const seconds = (tick: number) => tick / contract.timeBase
  const argv: string[] = []
  for (const ref of refs) {
    const input = options.inputs.get(ref)!
    const kind = mediaKindByRef.get(ref)!
    if (kind === 'image' || kind === 'text') {
      const loopSeconds = Math.max(...contract.clips
        .filter(clip => timelineClipInputKey(clip) === ref)
        .map(clip => clip.durationTicks))
      argv.push('-loop', '1', '-framerate', decimal(contract.target.fps, 'timeline loop framerate'), '-t', decimal(seconds(loopSeconds), 'timeline loop duration'))
    }
    argv.push('-i', safePath(input.path, 'timeline input path'))
  }
  const target = `${contract.target.width}:${contract.target.height}`
  const chains: string[] = []
  const durationSeconds = seconds(contract.durationTicks)
  chains.push(`color=c=black:s=${contract.target.width}x${contract.target.height}:r=${contract.target.fps}:d=${decimal(durationSeconds, 'timeline duration')},setsar=1[vbase]`)
  const videoClips = contract.clips
    .filter(clip => clip.trackKind === 'video' && clip.mediaKind !== 'audio')
    .sort((a, b) => b.trackIndex - a.trackIndex || a.timelineStartTick - b.timelineStartTick || (a.id < b.id ? -1 : 1))
  const videoFadeFilters = (clip: BrowserTimelineRenderClip): string[] => {
    const parts: string[] = []
    if (clip.fades.videoInTicks > 0 && clip.fades.videoInMode !== 'xfade') {
      const duration = decimal(seconds(clip.fades.videoInTicks), 'timeline fade seconds')
      parts.push(clip.fades.videoInMode === 'alpha'
        ? `format=yuva444p,fade=t=in:st=0:d=${duration}:alpha=1`
        : `fade=t=in:st=0:d=${duration}`)
    }
    if (clip.fades.videoOutTicks > 0) {
      const out = clip.fades.videoOutTicks
      parts.push(`fade=t=out:st=${decimal(seconds(clip.durationTicks - out), 'timeline fade out start')}:d=${decimal(seconds(out), 'timeline fade out duration')}`)
    }
    return parts
  }
  const frame = `scale=${target}:force_original_aspect_ratio=decrease,pad=${contract.target.width}:${contract.target.height}:(ow-iw)/2:(oh-ih)/2,fps=${contract.target.fps},setsar=1`
  // A normalized clip stream anchored at t=0, so xfade can blend a pair
  // before the result is shifted onto the absolute timeline.
  const localVideoChain = (clip: BrowserTimelineRenderClip): string => {
    const sourceIndex = inputIndex.get(timelineClipInputKey(clip))!
    if (clip.mediaKind === 'text') return `[${sourceIndex}:v]fps=${contract.target.fps},setpts=PTS-STARTPTS`
    if (clip.mediaKind === 'image') {
      return `[${sourceIndex}:v]${frame},trim=duration=${decimal(seconds(clip.durationTicks), 'timeline clip duration')},setpts=PTS-STARTPTS`
    }
    const start = decimal(seconds(clip.sourceInTick), 'timeline clip source start')
    const clipDuration = decimal(seconds(clip.durationTicks), 'timeline clip duration')
    return `[${sourceIndex}:v]trim=start=${start}:duration=${clipDuration},setpts=PTS-STARTPTS,${frame}`
  }
  // An xfade consumes the adjacent outgoing clip: its solo overlay is
  // replaced by the blended pair spanning both clips.
  const xfadePairs = new Map<string, { outgoing: BrowserTimelineRenderClip; name: string }>()
  const consumedOutgoing = new Set<string>()
  for (const clip of videoClips) {
    if (clip.fades.videoInMode !== 'xfade') continue
    const name = clip.fades.videoInXfade
    if (!name) throw new Error('invalid timeline xfade transition name')
    const overlap = clip.fades.videoInTicks
    const outgoing = videoClips.find(other => !consumedOutgoing.has(other.id)
      && other.trackIndex === clip.trackIndex
      && other.timelineStartTick + other.durationTicks === clip.timelineStartTick + overlap)
    if (!outgoing) throw new Error('timeline xfade requires an adjacent outgoing clip')
    if (outgoing.fades.videoInTicks > 0 || outgoing.fades.videoOutTicks > 0) throw new Error('timeline xfade cannot chain')
    if (clip.opacity !== null || outgoing.opacity !== null) throw new Error('timeline opacity is not supported inside xfade pairs')
    xfadePairs.set(clip.id, { outgoing, name })
    consumedOutgoing.add(outgoing.id)
  }
  // Clip opacity multiplies the alpha channel before the overlay composites
  // the clip onto the base canvas; 1 and null are rendered as no filter.
  const clipOpacityFilters = (clip: BrowserTimelineRenderClip): string[] => (
    clip.opacity === null || clip.opacity === 1
      ? []
      : [`format=yuva444p,colorchannelmixer=aa=${decimal(clip.opacity, 'timeline clip opacity')}`]
  )
  const audioFadeFilters = (clip: BrowserTimelineRenderClip): string[] => {
    const parts: string[] = []
    if (clip.fades.audioInTicks > 0) parts.push(`afade=t=in:st=0:d=${decimal(seconds(clip.fades.audioInTicks), 'timeline audio fade seconds')}`)
    if (clip.fades.audioOutTicks > 0) {
      const out = clip.fades.audioOutTicks
      parts.push(`afade=t=out:st=${decimal(seconds(clip.durationTicks - out), 'timeline audio fade out start')}:d=${decimal(seconds(out), 'timeline audio fade out duration')}`)
    }
    return parts
  }
  const overlayLabels: string[] = []
  videoClips.forEach((clip, index) => {
    if (consumedOutgoing.has(clip.id)) return
    const pair = xfadePairs.get(clip.id)
    if (pair) {
      const overlap = clip.fades.videoInTicks
      const xfadeOffset = decimal(seconds(pair.outgoing.durationTicks - overlap), 'timeline xfade offset')
      const xfadeDuration = decimal(seconds(overlap), 'timeline xfade duration')
      const pairStart = decimal(seconds(pair.outgoing.timelineStartTick), 'timeline clip offset')
      chains.push(`${localVideoChain(pair.outgoing)}[v${index}out]`)
      chains.push(`${localVideoChain(clip)}[v${index}in]`)
      chains.push(`[v${index}out][v${index}in]xfade=transition=${pair.name}:duration=${xfadeDuration}:offset=${xfadeOffset}[v${index}xf]`)
      chains.push(`[v${index}xf]setpts=PTS+${pairStart}/TB[v${index}]`)
      overlayLabels.push(`v${index}`)
      return
    }
    const sourceIndex = inputIndex.get(timelineClipInputKey(clip))!
    const offset = decimal(seconds(clip.timelineStartTick), 'timeline clip offset')
    const fadeFilters = [...videoFadeFilters(clip), ...clipOpacityFilters(clip)]
    const tail = fadeFilters.length
      ? `,${fadeFilters.join(',')},setpts=PTS+${offset}/TB[v${index}]`
      : `+${offset}/TB[v${index}]`
    if (clip.mediaKind === 'text') {
      chains.push(`[${sourceIndex}:v]fps=${contract.target.fps},setpts=PTS-STARTPTS${tail}`)
    } else if (clip.mediaKind === 'image') {
      chains.push(`[${sourceIndex}:v]${frame},trim=duration=${decimal(seconds(clip.durationTicks), 'timeline clip duration')},setpts=PTS-STARTPTS${tail}`)
    } else {
      const start = decimal(seconds(clip.sourceInTick), 'timeline clip source start')
      const clipDuration = decimal(seconds(clip.durationTicks), 'timeline clip duration')
      const videoTail = fadeFilters.length
        ? `,${fadeFilters.join(',')},setpts=PTS+${offset}/TB[v${index}]`
        : `,setpts=PTS+${offset}/TB[v${index}]`
      chains.push(`[${sourceIndex}:v]trim=start=${start}:duration=${clipDuration},setpts=PTS-STARTPTS,${frame}${videoTail}`)
    }
    overlayLabels.push(`v${index}`)
  })
  let videoLabel = 'vbase'
  overlayLabels.forEach((label, index) => {
    const next = `ov${index}`
    chains.push(`[${videoLabel}][${label}]overlay=eof_action=pass:format=auto[${next}]`)
    videoLabel = next
  })
  if (videoClips.length === 0) videoLabel = 'vbase'
  const postVideo = trustedFilters(options.postVideoFilters)
  const afterLut = trustedFilters(options.postVideoAfterLutFilters)
  const lutFilter = options.lutFilter ? trustedFilters([options.lutFilter])[0]! : null
  const lutIntensity = options.lutIntensity ?? (lutFilter ? 1 : 0)
  if (!Number.isFinite(lutIntensity) || lutIntensity < 0 || lutIntensity > 1) throw new Error('invalid timeline LUT intensity')
  if (lutFilter && lutIntensity > 0 && lutIntensity < 1) {
    const before = postVideo.length ? `${postVideo.join(',')},` : ''
    const after = afterLut.length ? `,${afterLut.join(',')}` : ''
    chains.push(`[${videoLabel}]${before}split=2[lutbase][lutin]`)
    chains.push(`[lutin]${lutFilter}[lutapplied]`)
    chains.push(`[lutbase][lutapplied]blend=all_expr='A*(1-${lutIntensity.toFixed(6)})+B*${lutIntensity.toFixed(6)}'${after}[vout]`)
    videoLabel = 'vout'
  } else {
    const linear = [...postVideo, ...(lutFilter && lutIntensity > 0 ? [lutFilter] : []), ...afterLut]
    if (linear.length) { chains.push(`[${videoLabel}]${linear.join(',')}[vout]`); videoLabel = 'vout' }
  }
  let audioLabel: string | null = null
  if (!options.muteAudio) {
    const audioChains: string[] = []
    const audioClips = contract.clips
      .filter(clip => clip.audioEnabled
        && clip.mediaKind !== 'image' && clip.mediaKind !== 'text'
        && options.inputs.get(timelineClipInputKey(clip))!.hasAudio)
      .sort((a, b) => a.trackIndex - b.trackIndex || a.timelineStartTick - b.timelineStartTick)
    audioClips.forEach((clip, index) => {
      const sourceIndex = inputIndex.get(timelineClipInputKey(clip))!
      const start = decimal(seconds(clip.sourceInTick), 'timeline audio source start')
      const clipDuration = decimal(seconds(clip.durationTicks), 'timeline audio duration')
      const delay = Math.round(seconds(clip.timelineStartTick) * 1000)
      const audioFades = audioFadeFilters(clip)
      const audioFadeJoin = audioFades.length ? `,${audioFades.join(',')}` : ''
      chains.push(`[${sourceIndex}:a]atrim=start=${start}:duration=${clipDuration},asetpts=PTS-STARTPTS${audioFadeJoin},adelay=${delay}:all=1[a${index}]`)
      audioChains.push(`a${index}`)
    })
    if (audioChains.length === 1) {
      chains.push(`[${audioChains[0]}]anull[aout]`)
      audioLabel = 'aout'
    } else if (audioChains.length > 1) {
      chains.push(`${audioChains.map(label => `[${label}]`).join('')}amix=inputs=${audioChains.length}:duration=longest:normalize=0,aresample[aoutm]`)
      audioLabel = 'aoutm'
    }
    if (audioLabel) {
      const postAudio = trustedFilters(options.postAudioFilters)
      if (postAudio.length) {
        chains.push(`[${audioLabel}]${postAudio.join(',')}[aout]`)
        audioLabel = 'aout'
      }
    }
  }
  const filename = safePath(options.output.filename, 'timeline output filename')
  if (filename.startsWith('-')) throw new Error('invalid timeline output filename')
  argv.push('-filter_complex', chains.join(';'), '-map', `[${videoLabel}]`)
  if (audioLabel) argv.push('-map', `[${audioLabel}]`)
  else argv.push('-an')
  argv.push('-t', decimal(durationSeconds, 'timeline duration'))
  argv.push('-fs', String(options.maxOutputBytes ?? MEMFS_MAX_OUTPUT_BYTES), ...options.output.args, filename)
  return { argv, inputAssetRefs: refs, expectedDurationSeconds: durationSeconds, resources }
}
