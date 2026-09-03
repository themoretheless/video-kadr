import {
  COMPOSITION_SCHEMA_VERSION,
  COMPOSITION_TIME_BASE,
  MAX_COMPOSITION_CLIPS,
  MAX_COMPOSITION_DURATION_TICKS,
  MAX_COMPOSITION_SOURCES,
  MAX_COMPOSITION_TRACKS,
  MAX_ACTIVE_AUDIO_KEYFRAMES,
  MAX_ACTIVE_VISUAL_KEYFRAMES,
  MAX_ACTIVE_SPEED_RAMP_SEGMENTS,
  MAX_KEYFRAMES_PER_VALUE,
  MAX_TEXT_LENGTH,
  MAX_VISIBLE_VISUAL_TRACKS,
  COMPOSITION_BLEND_MODES,
  COMPOSITION_AUDIO_PROPERTIES,
  COMPOSITION_MASK_PROPERTIES,
  COMPOSITION_STABILIZATION_RADII,
  COMPOSITION_TRANSITION_KINDS,
  COMPOSITION_VISUAL_PROPERTIES,
  clipEndTicks,
  type Composition,
  type AudioClip,
  type CompositionClip,
  type CompositionSource,
  type CompositionSpeedRamp,
  type CompositionTransition,
  type CompositionTrack,
  type VideoTrack,
} from './types'
import {
  audioPropertyBounds,
  animatableExtents,
  constantAnimatable,
  keyframeCount,
  normalizeInterpolation,
  visualPropertyBounds,
  visualPropertyValue,
} from './keyframes'
import { compositionSpeedRampSegments, minimumCompositionSpeed } from './speedRamp'

export const MIN_CANVAS_DIMENSION = 16
export const MAX_CANVAS_WIDTH = 3840
export const MAX_CANVAS_HEIGHT = 2160
export const MAX_CANVAS_FPS = 60
export const MAX_AUDIO_GAIN = 16
export const MIN_COMPOSITION_SPEED = 0.05
export const MAX_COMPOSITION_SPEED = 16
export const MAX_VISUAL_POSITION = 32_768
export const MAX_VISUAL_FILTER_DIMENSION = 8_192

const SAFE_ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/
const RESERVED_IDS = new Set(['__proto__', 'prototype', 'constructor'])
const SAFE_COLOR = /^#[0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?$/
const SOURCE_KINDS = new Set(['video', 'audio', 'image'])
const TRACK_KINDS = new Set(['video', 'audio', 'image', 'text'])
const BLEND_MODES = new Set<string>(COMPOSITION_BLEND_MODES)
const TRANSITION_KINDS = new Set<string>(COMPOSITION_TRANSITION_KINDS)
const TEXT_FONTS = new Set(['Noto Sans', 'Arial Unicode MS', 'DejaVu Sans', 'Arial'])
const INTERPOLATIONS = new Set(['hold', 'linear', 'ease_in', 'ease_out', 'ease_in_out', 'ease_in_out_cubic'])
const MAX_AUTHORING_MARKERS = 256
const MAX_AUTHORING_MARKER_LABEL = 128
const MAX_AUTHORING_MULTICAM_GROUPS = MAX_COMPOSITION_TRACKS
const MIN_AUTHORING_MULTICAM_ANGLES = 2
const MAX_AUTHORING_MULTICAM_ANGLES = 8
const MAX_AUTHORING_MULTICAM_SWITCHES = Math.min(512, MAX_COMPOSITION_CLIPS)

export interface CompositionValidationIssue {
  readonly code: string
  readonly path: string
  readonly message: string
}

export class CompositionValidationError extends Error {
  constructor(readonly issues: readonly CompositionValidationIssue[]) {
    super(issues[0]?.message ?? 'Invalid composition')
    this.name = 'CompositionValidationError'
  }
}

export function isStableId(value: unknown): value is string {
  return typeof value === 'string' && SAFE_ID.test(value) && !RESERVED_IDS.has(value)
}

export function isSafeTick(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0
}

export function validateComposition(value: unknown): CompositionValidationIssue[] {
  const issues: CompositionValidationIssue[] = []
  const add = (code: string, path: string, message: string): void => {
    issues.push({ code, path, message })
  }

  if (!isRecord(value)) {
    add('invalid-composition', '$', 'Composition must be an object')
    return issues
  }

  if (value.schemaVersion !== COMPOSITION_SCHEMA_VERSION) {
    add('schema-version', 'schemaVersion', `schemaVersion must be ${COMPOSITION_SCHEMA_VERSION}`)
  }
  if (value.timeBase !== COMPOSITION_TIME_BASE) {
    add('time-base', 'timeBase', `timeBase must be ${COMPOSITION_TIME_BASE}`)
  }

  validateCanvas(value.canvas, add)
  validateAuthoringMarkers(value.markers, add)

  const sources = new Map<string, CompositionSource>()
  if (!isRecord(value.sources)) {
    add('sources', 'sources', 'sources must be an object registry')
  } else {
    const entries = Object.entries(value.sources)
    if (entries.length > MAX_COMPOSITION_SOURCES) {
      add('source-limit', 'sources', `A composition can contain at most ${MAX_COMPOSITION_SOURCES} sources`)
    }
    for (const [key, candidate] of entries) {
      const path = `sources.${key}`
      if (!isRecord(candidate)) {
        add('source', path, 'Source must be an object')
        continue
      }
      validateId(candidate.id, `${path}.id`, add)
      if (!isStableId(key)) add('source-key', path, 'Source registry key is not a stable id')
      if (candidate.id !== key) add('source-key', path, 'Source registry key must equal source.id')
      if (!SOURCE_KINDS.has(String(candidate.kind))) {
        add('source-kind', `${path}.kind`, 'Source kind must be video, audio, or image')
      }
      validateSourceMetadata(candidate, path, add)
      if (isStableId(candidate.id) && SOURCE_KINDS.has(String(candidate.kind))) {
        sources.set(candidate.id, candidate as unknown as CompositionSource)
      }
    }
  }

  if (!Array.isArray(value.tracks)) {
    add('tracks', 'tracks', 'tracks must be an array')
    return issues
  }
  if (value.tracks.length > MAX_COMPOSITION_TRACKS) {
    add('track-limit', 'tracks', `A composition can contain at most ${MAX_COMPOSITION_TRACKS} tracks`)
  }

  const trackIds = new Set<string>()
  const clipIds = new Set<string>()
  let totalClips = 0
  let visibleVisualTracks = 0
  for (const [trackIndex, candidate] of value.tracks.entries()) {
    const path = `tracks[${trackIndex}]`
    if (!isRecord(candidate)) {
      add('track', path, 'Track must be an object')
      continue
    }
    validateId(candidate.id, `${path}.id`, add)
    if (isStableId(candidate.id) && trackIds.has(candidate.id)) {
      add('duplicate-track-id', `${path}.id`, `Duplicate track id ${candidate.id}`)
    } else if (isStableId(candidate.id)) {
      trackIds.add(candidate.id)
    }
    if (!TRACK_KINDS.has(String(candidate.kind))) {
      add('track-kind', `${path}.kind`, 'Track kind must be video, audio, image, or text')
    }
    if (typeof candidate.name !== 'string' || !candidate.name.trim() || candidate.name.length > 128) {
      add('track-name', `${path}.name`, 'Track name must contain 1 to 128 characters')
    }
    if (typeof candidate.locked !== 'boolean') {
      add('track-locked', `${path}.locked`, 'locked must be a boolean')
    }
    if (candidate.kind === 'audio') {
      if (typeof candidate.muted !== 'boolean') add('track-muted', `${path}.muted`, 'muted must be a boolean')
      if (candidate.solo !== undefined && typeof candidate.solo !== 'boolean') {
        add('track-solo', `${path}.solo`, 'solo must be a boolean when provided')
      }
    } else if (TRACK_KINDS.has(String(candidate.kind))) {
      if (typeof candidate.hidden !== 'boolean') add('track-hidden', `${path}.hidden`, 'hidden must be a boolean')
      if (candidate.kind === 'video' && typeof candidate.muted !== 'boolean') {
        add('track-muted', `${path}.muted`, 'muted must be a boolean')
      }
      if (candidate.hidden === false) visibleVisualTracks += 1
    }

    if (!Array.isArray(candidate.clips)) {
      add('clips', `${path}.clips`, 'clips must be an array')
      continue
    }
    totalClips += candidate.clips.length
    const intervals: Array<{ start: number; end: number; id: string; path: string }> = []
    for (const [clipIndex, clipCandidate] of candidate.clips.entries()) {
      const clipPath = `${path}.clips[${clipIndex}]`
      if (!isRecord(clipCandidate)) {
        add('clip', clipPath, 'Clip must be an object')
        continue
      }
      validateId(clipCandidate.id, `${clipPath}.id`, add)
      if (isStableId(clipCandidate.id) && clipIds.has(clipCandidate.id)) {
        add('duplicate-clip-id', `${clipPath}.id`, `Duplicate clip id ${clipCandidate.id}`)
      } else if (isStableId(clipCandidate.id)) {
        clipIds.add(clipCandidate.id)
      }
      if (clipCandidate.kind !== candidate.kind) {
        add('clip-kind', `${clipPath}.kind`, 'Clip kind must match its track kind')
      }
      validateClip(clipCandidate, clipPath, sources, add)
      if (clipCandidate.kind === 'video' && clipCandidate.frameInterpolation === 'optical_flow') {
        if (candidate.kind !== 'video' || candidate.hidden !== false) {
          add('optical-flow-active', `${clipPath}.frameInterpolation`, 'Optical flow requires an active clip on a visible video track')
        }
        const minimumSpeed = runtimeMinimumSpeed(clipCandidate)
        if (minimumSpeed === null || minimumSpeed >= 1) {
          add('optical-flow-speed', `${clipPath}.frameInterpolation`, 'Optical flow is available only for slow motion below 1x')
        }
      }
      if (
        clipCandidate.kind === 'video' &&
        isRecord(clipCandidate.stabilization) &&
        clipCandidate.stabilization.mode === 'deshake' &&
        (candidate.kind !== 'video' || candidate.hidden !== false)
      ) {
        add('stabilization-active', `${clipPath}.stabilization`, 'Deshake requires an active clip on a visible video track')
      }

      if (isSafeTick(clipCandidate.timelineStartTicks)) {
        const duration = runtimeClipDuration(clipCandidate)
        if (duration !== null && duration > 0) {
          const end = clipCandidate.timelineStartTicks + duration
          if (!Number.isSafeInteger(end) || end > MAX_COMPOSITION_DURATION_TICKS) {
            add('composition-duration', clipPath, 'Clip extends beyond the 24 hour composition limit')
          } else {
            intervals.push({
              start: clipCandidate.timelineStartTicks,
              end,
              id: typeof clipCandidate.id === 'string' ? clipCandidate.id : String(clipIndex),
              path: clipPath,
            })
          }
        }
      }
    }
    if (candidate.kind === 'video') {
      validateTransitions(candidate.transitions, candidate.clips, `${path}.transitions`, add)
    } else if (candidate.kind === 'audio') {
      validateAudioCrossfades(candidate.clips, path, sources, add)
    }
    intervals.sort((left, right) => left.start - right.start || left.end - right.end || left.id.localeCompare(right.id))
    for (let index = 1; index < intervals.length; index += 1) {
      const previous = intervals[index - 1]!
      const current = intervals[index]!
      if (current.start < previous.end) {
        add('track-overlap', current.path, `Clip overlaps ${previous.id} in the same track`)
      }
    }
  }

  if (totalClips > MAX_COMPOSITION_CLIPS) {
    add('clip-limit', 'tracks', `A composition can contain at most ${MAX_COMPOSITION_CLIPS} clips`)
  }
  const activeSpeedRampSegments = activeSpeedRampSegmentCount(value.tracks)
  if (activeSpeedRampSegments > MAX_ACTIVE_SPEED_RAMP_SEGMENTS) {
    add(
      'speed-ramp-segment-budget',
      'tracks',
      `Active speed ramps contain ${activeSpeedRampSegments} segments; maximum is ${MAX_ACTIVE_SPEED_RAMP_SEGMENTS}`,
    )
  }
  const activeAudioKeyframes = issues.length ? 0 : activeAudioKeyframeCount(value as unknown as Composition)
  if (activeAudioKeyframes > MAX_ACTIVE_AUDIO_KEYFRAMES) {
    add(
      'audio-keyframe-budget',
      'tracks',
      `Active audio contains ${activeAudioKeyframes} keyframes; maximum is ${MAX_ACTIVE_AUDIO_KEYFRAMES}`,
    )
  }
  if (visibleVisualTracks > MAX_VISIBLE_VISUAL_TRACKS) {
    add(
      'visible-track-limit',
      'tracks',
      `A composition can contain at most ${MAX_VISIBLE_VISUAL_TRACKS} visible visual tracks`,
    )
  }
  validateAuthoringMulticamGroups(value.multicamGroups, value.tracks, sources, add)
  return issues
}

/**
 * Materialize optional authoring-v1 defaults at every persistence boundary.
 * Object spreads deliberately retain unknown fields so commands/history and
 * templates do not erase data written by a newer compatible editor.
 */
export function normalizeComposition(value: unknown): Composition {
  const cloned = cloneUnknown(value)
  if (isRecord(cloned) && cloned.multicamGroups === undefined) cloned.multicamGroups = []
  if (isRecord(cloned) && isRecord(cloned.canvas)) {
    cloned.canvas.backgroundMode ??= 'color'
    cloned.canvas.backgroundBlur ??= 24
  }
  if (isRecord(cloned) && Array.isArray(cloned.tracks)) {
    cloned.tracks = cloned.tracks.map((candidate) => {
      if (!isRecord(candidate)) return candidate
      const clips = Array.isArray(candidate.clips)
        ? candidate.clips.map((clip) => normalizeClip(clip))
        : candidate.clips
      if (candidate.kind === 'video') {
        return { ...candidate, clips, transitions: Array.isArray(candidate.transitions) ? candidate.transitions : [] }
      }
      if (candidate.kind === 'audio') {
        return { ...candidate, clips, solo: candidate.solo ?? false }
      }
      return { ...candidate, clips }
    })
  }
  assertValidComposition(cloned)
  return cloned
}

function validateAuthoringMulticamGroups(
  value: unknown,
  tracks: readonly unknown[],
  sources: ReadonlyMap<string, CompositionSource>,
  add: (code: string, path: string, message: string) => void,
): void {
  if (value === undefined) return
  if (!Array.isArray(value)) {
    add('multicam-groups', 'multicamGroups', 'Multicam groups must be an array')
    return
  }
  if (value.length > MAX_AUTHORING_MULTICAM_GROUPS) {
    add('multicam-group-limit', 'multicamGroups', `A composition can contain at most ${MAX_AUTHORING_MULTICAM_GROUPS} multicam groups`)
  }
  const trackById = new Map<string, Record<string, unknown>>()
  for (const track of tracks) {
    if (isRecord(track) && isStableId(track.id)) trackById.set(track.id, track)
  }
  const groupIds = new Set<string>()
  const ownedTrackIds = new Set<string>()
  for (const [groupIndex, candidate] of value.entries()) {
    const path = `multicamGroups[${groupIndex}]`
    if (!isRecord(candidate)) {
      add('multicam-group', path, 'Multicam group must be an object')
      continue
    }
    validateId(candidate.id, `${path}.id`, add)
    if (isStableId(candidate.id) && groupIds.has(candidate.id)) add('duplicate-multicam-group-id', `${path}.id`, 'Multicam group id must be unique')
    else if (isStableId(candidate.id)) groupIds.add(candidate.id)
    if (typeof candidate.name !== 'string' || !candidate.name.trim() || [...candidate.name].length > 128) {
      add('multicam-name', `${path}.name`, 'Multicam name must contain 1 to 128 characters')
    }
    const start = candidate.timelineStartTicks
    const duration = candidate.durationTicks
    if (!isSafeTick(start) || !isSafeTick(duration) || duration <= 0 || !Number.isSafeInteger(start + duration) || start + duration > MAX_COMPOSITION_DURATION_TICKS) {
      add('multicam-range', path, 'Multicam group range is outside the composition limit')
    }
    validateMulticamTrack(candidate.videoTrackId, 'video', `${path}.videoTrackId`, trackById, ownedTrackIds, add)

    if (!Array.isArray(candidate.angles) || candidate.angles.length < MIN_AUTHORING_MULTICAM_ANGLES || candidate.angles.length > MAX_AUTHORING_MULTICAM_ANGLES) {
      add('multicam-angle-count', `${path}.angles`, `Multicam group needs ${MIN_AUTHORING_MULTICAM_ANGLES} to ${MAX_AUTHORING_MULTICAM_ANGLES} angles`)
      continue
    }
    const angleIds = new Set<string>()
    const angleSourceIds = new Set<string>()
    const angleById = new Map<string, Record<string, unknown>>()
    for (const [angleIndex, angle] of candidate.angles.entries()) {
      const anglePath = `${path}.angles[${angleIndex}]`
      if (!isRecord(angle)) {
        add('multicam-angle', anglePath, 'Multicam angle must be an object')
        continue
      }
      validateId(angle.id, `${anglePath}.id`, add)
      validateId(angle.sourceId, `${anglePath}.sourceId`, add)
      if (isStableId(angle.id) && angleIds.has(angle.id)) add('duplicate-multicam-angle-id', `${anglePath}.id`, 'Multicam angle id must be unique')
      else if (isStableId(angle.id)) {
        angleIds.add(angle.id)
        angleById.set(angle.id, angle)
      }
      if (isStableId(angle.sourceId) && angleSourceIds.has(angle.sourceId)) add('duplicate-multicam-source', `${anglePath}.sourceId`, 'Each multicam angle must use a different source')
      else if (isStableId(angle.sourceId)) angleSourceIds.add(angle.sourceId)
      if (typeof angle.label !== 'string' || !angle.label.trim() || [...angle.label].length > 128) {
        add('multicam-angle-label', `${anglePath}.label`, 'Multicam angle label must contain 1 to 128 characters')
      }
      if (!isSafeTick(angle.sourceTickAtGroupStart)) {
        add('multicam-angle-offset', `${anglePath}.sourceTickAtGroupStart`, 'Multicam source offset must be a safe tick')
      }
      const source = typeof angle.sourceId === 'string' ? sources.get(angle.sourceId) : undefined
      if (!source || source.kind !== 'video') {
        add('multicam-angle-source', `${anglePath}.sourceId`, 'Multicam angle needs a registered video source')
      } else if (isSafeTick(angle.sourceTickAtGroupStart) && isSafeTick(duration) && angle.sourceTickAtGroupStart + duration > source.durationTicks) {
        add('multicam-angle-coverage', anglePath, 'Multicam angle does not cover the whole group')
      }
    }

    if (!Array.isArray(candidate.switches) || candidate.switches.length < 1 || candidate.switches.length > MAX_AUTHORING_MULTICAM_SWITCHES) {
      add('multicam-switch-count', `${path}.switches`, `Multicam group needs 1 to ${MAX_AUTHORING_MULTICAM_SWITCHES} switches`)
    } else {
      const switchIds = new Set<string>()
      const clipIds = new Set<string>()
      const ticks = new Set<number>()
      for (const [switchIndex, change] of candidate.switches.entries()) {
        const switchPath = `${path}.switches[${switchIndex}]`
        if (!isRecord(change)) {
          add('multicam-switch', switchPath, 'Multicam switch must be an object')
          continue
        }
        validateId(change.id, `${switchPath}.id`, add)
        validateId(change.clipId, `${switchPath}.clipId`, add)
        if (isStableId(change.id) && switchIds.has(change.id)) add('duplicate-multicam-switch-id', `${switchPath}.id`, 'Multicam switch id must be unique')
        else if (isStableId(change.id)) switchIds.add(change.id)
        if (isStableId(change.clipId) && clipIds.has(change.clipId)) add('duplicate-multicam-clip-id', `${switchPath}.clipId`, 'Multicam output clip id must be unique')
        else if (isStableId(change.clipId)) clipIds.add(change.clipId)
        if (!isStableId(change.angleId) || !angleById.has(change.angleId)) add('multicam-switch-angle', `${switchPath}.angleId`, 'Multicam switch angle is missing')
        if (!isSafeTick(change.timelineTick) || !isSafeTick(start) || !isSafeTick(duration) || change.timelineTick < start || change.timelineTick >= start + duration) {
          add('multicam-switch-tick', `${switchPath}.timelineTick`, 'Multicam switch lies outside its group')
        } else if (ticks.has(change.timelineTick)) add('duplicate-multicam-switch-tick', `${switchPath}.timelineTick`, 'Multicam switch ticks must be unique')
        else ticks.add(change.timelineTick)
      }
      const firstTick = candidate.switches
        .filter(isRecord)
        .map((change) => change.timelineTick)
        .filter(isSafeTick)
        .sort((left, right) => left - right)[0]
      if (isSafeTick(start) && firstTick !== start) add('multicam-first-switch', `${path}.switches`, 'First multicam switch must start at the group boundary')
    }

    const hasAudioAngle = candidate.audioAngleId !== undefined
    if (hasAudioAngle) {
      validateId(candidate.audioAngleId, `${path}.audioAngleId`, add)
      validateId(candidate.audioClipId, `${path}.audioClipId`, add)
      validateMulticamTrack(candidate.audioTrackId, 'audio', `${path}.audioTrackId`, trackById, ownedTrackIds, add)
      const angle = typeof candidate.audioAngleId === 'string' ? angleById.get(candidate.audioAngleId) : undefined
      const source = angle && typeof angle.sourceId === 'string' ? sources.get(angle.sourceId) : undefined
      if (!angle || !source?.hasAudio) add('multicam-audio-angle', `${path}.audioAngleId`, 'Multicam master audio angle is missing or silent')
    } else if (candidate.audioTrackId !== undefined || candidate.audioClipId !== undefined) {
      add('multicam-audio-fields', path, 'Multicam audio track/clip ids require an audioAngleId')
    }
  }
}

function validateMulticamTrack(
  value: unknown,
  kind: 'video' | 'audio',
  path: string,
  tracks: ReadonlyMap<string, Record<string, unknown>>,
  ownedTrackIds: Set<string>,
  add: (code: string, path: string, message: string) => void,
): void {
  validateId(value, path, add)
  if (!isStableId(value)) return
  const track = tracks.get(value)
  if (!track || track.kind !== kind) add('multicam-track', path, `Multicam ${kind} track is missing or incompatible`)
  if (ownedTrackIds.has(value)) add('duplicate-multicam-track', path, 'Multicam groups must own different output tracks')
  else ownedTrackIds.add(value)
}

export function assertValidComposition(value: unknown): asserts value is Composition {
  const issues = validateComposition(value)
  if (issues.length) throw new CompositionValidationError(issues)
}

/** Useful for timeline math after a composition has passed validation. */
export function compositionDurationTicks(composition: Composition): number {
  let duration = 0
  for (const track of composition.tracks) {
    for (const clip of track.clips) duration = Math.max(duration, clipEndTicks(clip))
  }
  return duration
}

export function compositionClipCount(composition: Composition): number {
  return composition.tracks.reduce((count, track) => count + track.clips.length, 0)
}

/** Exact client-side mirror of the currently compiled composition render slice. */
export function compositionRenderUnavailableReason(composition: Composition): string | null {
  const unsupported = unsupportedAuthoredFeature(composition)
  if (unsupported) return unsupported
  const opticalFlowReason = compositionOpticalFlowUnavailableReason(composition)
  if (opticalFlowReason) return opticalFlowReason
  const stabilizationReason = compositionStabilizationUnavailableReason(composition)
  if (stabilizationReason) return stabilizationReason

  const primary = primaryCompositionVideoTrack(composition)
  if (!primary) {
    const activeVisual = composition.tracks.filter((track) => track.kind !== 'audio' && !track.hidden && track.clips.length)
    return activeVisual.length
      ? 'Нижняя видимая visual-дорожка должна быть основной видеодорожкой.'
      : 'Для экспорта нужна видимая основная видеодорожка.'
  }
  const clips = [...primary.clips].sort(compareClips)
  let primaryEnd = 0
  for (const clip of clips) {
    if (clip.timelineStartTicks !== primaryEnd) {
      return 'Основная видеодорожка должна начинаться с 0 и не содержать gaps или overlaps.'
    }
    const visualReason = visualClipReason(composition, clip)
    if (visualReason) return visualReason
    primaryEnd = clipEndTicks(clip)
  }
  if (!primaryEnd) return 'Основная видеодорожка пуста.'

  const transitionReason = primaryTransitionReason(composition, primary, clips)
  if (transitionReason) return transitionReason

  const primaryIndex = composition.tracks.findIndex((track) => track.id === primary.id)
  for (const track of composition.tracks.slice(0, primaryIndex)) {
    if (track.kind === 'audio' || track.hidden) continue
    for (const clip of track.clips) {
      if (
        clip.kind === 'video' &&
        clip.audioAnimation &&
        COMPOSITION_AUDIO_PROPERTIES.some((property) => clip.audioAnimation?.[property] !== undefined)
      ) {
        return `Audio automation clip «${clip.id}» поддерживается только на основной видеодорожке; вынесите overlay audio на audio track.`
      }
      if (clipEndTicks(clip) > primaryEnd) return `Overlay «${clip.id}» выходит за длительность основной видеодорожки.`
      const visualReason = visualClipReason(composition, clip)
      if (visualReason) return visualReason
    }
  }

  const activeVisualKeyframes = composition.tracks.reduce((count, track) => {
    if (track.kind === 'audio' || track.hidden) return count
    return count + track.clips.reduce((clipCount, clip) => clipCount + visualKeyframeCount(clip), 0)
  }, 0)
  if (activeVisualKeyframes > MAX_ACTIVE_VISUAL_KEYFRAMES) {
    return `Активные visual-дорожки содержат ${activeVisualKeyframes} keyframes; максимум ${MAX_ACTIVE_VISUAL_KEYFRAMES}.`
  }

  const activeAudioKeyframes = activeAudioKeyframeCount(composition)
  if (activeAudioKeyframes > MAX_ACTIVE_AUDIO_KEYFRAMES) {
    return `Активные audio-дорожки содержат ${activeAudioKeyframes} keyframes; максимум ${MAX_ACTIVE_AUDIO_KEYFRAMES}.`
  }

  const hasSolo = composition.tracks.some((track) => track.kind === 'audio' && !track.muted && (track.solo ?? false))
  for (const track of composition.tracks) {
    if (track.kind !== 'audio' || track.muted || (hasSolo && !(track.solo ?? false))) continue
    for (const clip of track.clips) {
      if (clipEndTicks(clip) > primaryEnd) return `Аудиоклип «${clip.id}» выходит за длительность основной видеодорожки.`
    }
  }
  return null
}

export function compositionUsesOpticalFlow(composition: Composition): boolean {
  return composition.tracks.some(
    (track) => track.kind === 'video' && !track.hidden &&
      track.clips.some((clip) => (clip.frameInterpolation ?? 'duplicate') === 'optical_flow'),
  )
}

export function compositionUsesReversePlayback(composition: Composition): boolean {
  return composition.tracks.some(
    (track) => track.kind === 'video' && !track.hidden &&
      track.clips.some((clip) => (clip.playbackMode?.mode ?? 'forward') === 'reverse'),
  )
}

export function compositionUsesFreezeFrame(composition: Composition): boolean {
  return composition.tracks.some(
    (track) => track.kind === 'video' && !track.hidden &&
      track.clips.some((clip) => clip.playbackMode?.mode === 'freeze'),
  )
}

export function compositionUsesStabilization(composition: Composition): boolean {
  return composition.tracks.some(
    (track) => track.kind === 'video' && !track.hidden &&
      track.clips.some((clip) => clip.stabilization?.mode === 'deshake'),
  )
}

export function compositionUsesSpeedRamp(composition: Composition): boolean {
  if (composition.tracks.some(
    (track) => track.kind === 'video' && !track.hidden && track.clips.some((clip) => clip.speedRamp !== undefined),
  )) return true
  const hasSolo = composition.tracks.some(
    (track) => track.kind === 'audio' && !track.muted && (track.solo ?? false),
  )
  return composition.tracks.some(
    (track) => track.kind === 'audio' && !track.muted && (!hasSolo || (track.solo ?? false)) &&
      track.clips.some(
        (clip) => clip.speedRamp !== undefined && (clip.speedRamp.audioPolicy ?? 'preserve_pitch') === 'preserve_pitch',
      ),
  )
}

export function compositionStabilizationUnavailableReason(composition: Composition): string | null {
  for (const track of composition.tracks) {
    if (track.kind !== 'video') continue
    for (const clip of track.clips) {
      if (clip.stabilization?.mode !== 'deshake') continue
      if (track.hidden) return `Deshake clip «${clip.id}» должен находиться на видимой video-дорожке.`
      if (clip.playbackMode?.mode === 'freeze') return `Deshake clip «${clip.id}» нельзя совмещать с freeze frame.`
    }
  }
  return null
}

export function compositionOpticalFlowUnavailableReason(composition: Composition): string | null {
  for (const track of composition.tracks) {
    if (track.kind !== 'video') continue
    for (const clip of track.clips) {
      if ((clip.frameInterpolation ?? 'duplicate') !== 'optical_flow') continue
      if (track.hidden) return `Optical flow clip «${clip.id}» должен находиться на видимой video-дорожке.`
      if (minimumCompositionSpeed(clip.speed ?? 1, clip.speedRamp) >= 1) {
        return `Optical flow clip «${clip.id}» требует хотя бы один speed ramp участок меньше 1x.`
      }
    }
  }
  return null
}

export function primaryCompositionVideoTrack(composition: Composition): VideoTrack | null {
  const active = composition.tracks.filter((track) => track.kind !== 'audio' && !track.hidden && track.clips.length)
  const lowest = active.at(-1)
  return lowest?.kind === 'video' ? lowest : null
}

export function compositionTransitionUnavailableReason(
  composition: Composition,
  trackId: string,
  transition: CompositionTransition,
): string | null {
  const track = composition.tracks.find((candidate): candidate is VideoTrack => candidate.id === trackId && candidate.kind === 'video')
  if (!track) return 'Переход можно добавить только на видеодорожку.'
  const transitions = [...(track.transitions ?? []).filter((candidate) => candidate.id !== transition.id), transition]
  return primaryTransitionReason(composition, { ...track, transitions }, [...track.clips].sort(compareClips))
}

function primaryTransitionReason(
  composition: Composition,
  track: VideoTrack,
  clips: readonly VideoTrack['clips'][number][],
): string | null {
  const transitions = track.transitions ?? []
  const indexes = new Map(clips.map((clip, index) => [clip.id, index]))
  const fromIds = new Set<string>()
  const toIds = new Set<string>()
  const head = new Map<string, number>()
  const tail = new Map<string, number>()
  for (const transition of transitions) {
    const fromIndex = indexes.get(transition.fromClipId)
    const toIndex = indexes.get(transition.toClipId)
    if (fromIndex === undefined || toIndex !== fromIndex + 1 || fromIds.has(transition.fromClipId) || toIds.has(transition.toClipId)) {
      return `Переход «${transition.id}» должен соединять одну уникальную пару соседних video clips.`
    }
    const from = clips[fromIndex]!
    const to = clips[toIndex]!
    if (from.speedRamp || to.speedRamp) {
      return `Переход «${transition.id}» нельзя совмещать со speed ramp на endpoint clips.`
    }
    if ((from.playbackMode?.mode ?? 'forward') !== 'forward' || (to.playbackMode?.mode ?? 'forward') !== 'forward') {
      return `Переход «${transition.id}» поддерживает только forward playback на обоих clips.`
    }
    if (clipEndTicks(from) !== to.timelineStartTicks || transition.durationTicks <= 0) {
      return `Переход «${transition.id}» требует точной общей границы соседних clips.`
    }
    const before = Math.floor(transition.durationTicks / 2)
    const after = transition.durationTicks - before
    const fromSource = composition.sources[from.sourceId]
    const availableTail = (fromSource?.durationTicks ?? 0) - from.sourceOutTicks
    const requiredTail = after * (from.speed ?? 1)
    const availableHead = to.sourceInTicks
    const requiredHead = before * (to.speed ?? 1)
    if (availableTail + Number.EPSILON < requiredTail || availableHead + Number.EPSILON < requiredHead) {
      return `Для перехода «${transition.id}» не хватает точных source handles: хвоста outgoing или начала incoming clip.`
    }
    fromIds.add(transition.fromClipId)
    toIds.add(transition.toClipId)
    tail.set(from.id, before)
    head.set(to.id, after)
  }
  for (const clip of clips) {
    if ((head.get(clip.id) ?? 0) + (tail.get(clip.id) ?? 0) > clipEndTicks(clip) - clip.timelineStartTicks) {
      return `Окна переходов перекрываются внутри clip «${clip.id}».`
    }
  }
  return null
}

function visualClipReason(composition: Composition, clip: Exclude<CompositionClip, AudioClip>): string | null {
  const x = animatableExtents(visualPropertyValue(composition, clip, 'x'))
  const y = animatableExtents(visualPropertyValue(composition, clip, 'y'))
  if (Math.max(Math.abs(x.minimum), Math.abs(x.maximum)) > MAX_VISUAL_POSITION ||
      Math.max(Math.abs(y.minimum), Math.abs(y.maximum)) > MAX_VISUAL_POSITION) {
    return `Transform clip «${clip.id}» выходит за безопасный диапазон позиции.`
  }
  const source = clip.kind === 'text' ? null : composition.sources[clip.sourceId]
  const sourceWidth = source?.width ?? composition.canvas.width
  const sourceHeight = source?.height ?? composition.canvas.height
  const scaleX = animatableExtents(visualPropertyValue(composition, clip, 'scaleX'))
  const scaleY = animatableExtents(visualPropertyValue(composition, clip, 'scaleY'))
  const rotation = animatableExtents(visualPropertyValue(composition, clip, 'rotationDegrees'))
  const opacity = animatableExtents(visualPropertyValue(composition, clip, 'opacity'))
  const width = sourceWidth * scaleX.maximum
  const height = sourceHeight * scaleY.maximum
  const rotates = rotation.minimum !== 0 || rotation.maximum !== 0
  const filterWidth = rotates ? Math.hypot(width, height) : width
  const filterHeight = rotates ? Math.hypot(width, height) : height
  if (
    scaleX.minimum < 0.01 || scaleX.maximum > 16 ||
    scaleY.minimum < 0.01 || scaleY.maximum > 16 ||
    rotation.minimum < -3_600 || rotation.maximum > 3_600 ||
    opacity.minimum < 0 || opacity.maximum > 1 ||
    filterWidth > MAX_VISUAL_FILTER_DIMENSION || filterHeight > MAX_VISUAL_FILTER_DIMENSION
  ) {
    return `Transform clip «${clip.id}» создаёт неподдерживаемый размер.`
  }
  if (clip.kind === 'video' && clip.chromaKey?.enabled && clip.chromaKey.similarity < 0.000_01) {
    return `Chroma similarity clip «${clip.id}» должна быть не меньше 0.00001 для FFmpeg.`
  }
  return null
}

function visualKeyframeCount(clip: Exclude<CompositionClip, AudioClip>): number {
  let count = COMPOSITION_VISUAL_PROPERTIES.reduce(
    (total, property) => total + keyframeCount(clip.animation?.[property]),
    0,
  )
  if (clip.kind === 'video') {
    for (const mask of clip.masks ?? []) {
      count += keyframeCount(mask.x) + keyframeCount(mask.y) + keyframeCount(mask.width) +
        keyframeCount(mask.height) + keyframeCount(mask.rotationDegrees)
    }
  }
  return count
}

function activeAudioKeyframeCount(composition: Composition): number {
  const primary = primaryCompositionVideoTrack(composition)
  let total = 0
  if (primary && !primary.muted) {
    for (const clip of primary.clips) {
      if (
        !clip.sourceAudioEnabled ||
        !composition.sources[clip.sourceId]?.hasAudio ||
        clip.playbackMode?.mode === 'freeze' ||
        clip.speedRamp?.audioPolicy === 'mute'
      ) continue
      total += clipAudioKeyframeCount(clip)
    }
  }

  const hasSolo = composition.tracks.some((track) => track.kind === 'audio' && !track.muted && track.solo)
  for (const track of composition.tracks) {
    if (track.kind !== 'audio' || track.muted || (hasSolo && !track.solo)) continue
    for (const clip of track.clips) {
      if (clip.speedRamp?.audioPolicy !== 'mute') total += clipAudioKeyframeCount(clip)
    }
  }
  return total
}

function clipAudioKeyframeCount(clip: VideoTrack['clips'][number] | AudioClip): number {
  return COMPOSITION_AUDIO_PROPERTIES.reduce(
    (count, property) => count + keyframeCount(clip.audioAnimation?.[property]),
    0,
  )
}

function unsupportedAuthoredFeature(composition: Composition): string | null {
  for (const track of composition.tracks) {
    for (const clip of track.clips) {
      const candidate = clip as unknown as Record<string, unknown>
      if (Array.isArray(candidate.effects) && candidate.effects.length) {
        return 'Неподдерживаемые clip effects сохранены в проекте; используйте authoring chroma key и Rectangle/Ellipse/Linear masks.'
      }
      for (const [key, value] of Object.entries(candidate)) {
        if (key === 'animation' && clip.kind !== 'audio') {
          const reason = unsupportedAnimationField(value)
          if (reason) return reason
          continue
        }
        if (key === 'audioAnimation' && (clip.kind === 'video' || clip.kind === 'audio')) {
          const reason = unsupportedAudioAnimationField(value)
          if (reason) return reason
          continue
        }
        if (key === 'masks' && clip.kind === 'video') {
          const reason = unsupportedMaskField(value)
          if (reason) return reason
          continue
        }
        if (key === 'mask' || key === 'masks') {
          return 'Masks в неподдерживаемом clip type сохранены в проекте и не могут быть безопасно экспортированы.'
        }
        const reason = unsupportedNestedFeature(value)
        if (reason) return reason
      }
    }
  }
  return null
}

function unsupportedAudioAnimationField(value: unknown): string | null {
  if (!isRecord(value)) return null
  for (const key of Object.keys(value)) {
    if ((COMPOSITION_AUDIO_PROPERTIES as readonly string[]).includes(key)) continue
    return `Неизвестный audio animation property «${key}» не может быть безопасно экспортирован.`
  }
  return null
}

function unsupportedAnimationField(value: unknown): string | null {
  if (!isRecord(value)) return null
  for (const key of Object.keys(value)) {
    if ((COMPOSITION_VISUAL_PROPERTIES as readonly string[]).includes(key)) continue
    return `Неизвестный visual animation property «${key}» не может быть безопасно экспортирован.`
  }
  return null
}

function unsupportedMaskField(value: unknown): string | null {
  if (!Array.isArray(value)) return null
  const known = new Set(['id', 'shape', 'rotationDegrees', 'x', 'y', 'width', 'height', 'feather', 'inverted'])
  for (const candidate of value) {
    if (!isRecord(candidate)) continue
    for (const key of Object.keys(candidate)) {
      if (known.has(key)) continue
      return `Неподдерживаемое поле mask «${key}» не может быть безопасно экспортировано.`
    }
  }
  return null
}

function unsupportedNestedFeature(value: unknown): string | null {
  if (Array.isArray(value)) {
    for (const candidate of value) {
      const reason = unsupportedNestedFeature(candidate)
      if (reason) return reason
    }
    return null
  }
  if (!isRecord(value)) return null
  if (value.mode === 'keyframes' || Object.hasOwn(value, 'keyframes')) {
    return 'Keyframes в неизвестном поле сохранены в проекте и не могут быть безопасно экспортированы.'
  }
  if (
    Object.hasOwn(value, 'mask') ||
    Object.hasOwn(value, 'masks') ||
    (typeof value.kind === 'string' && value.kind.includes('mask'))
  ) {
    return 'Mask в неизвестном поле сохранена в проекте и не может быть безопасно экспортирована.'
  }
  if (Array.isArray(value.effects) && value.effects.length) {
    if (value.effects.some((effect) => isRecord(effect) && typeof effect.kind === 'string' && effect.kind.includes('mask'))) {
      return 'Mask в неизвестном effects-поле сохранена в проекте и не может быть безопасно экспортирована.'
    }
    return 'Неподдерживаемые clip effects сохранены в проекте; текущий adapter экспортирует только authoring chroma key.'
  }
  for (const candidate of Object.values(value)) {
    const reason = unsupportedNestedFeature(candidate)
    if (reason) return reason
  }
  return null
}

function compareClips(left: CompositionClip, right: CompositionClip): number {
  return left.timelineStartTicks - right.timelineStartTicks || left.id.localeCompare(right.id)
}

export function tracksOverlap(track: CompositionTrack): boolean {
  const sorted = [...track.clips].sort(
    (left, right) => left.timelineStartTicks - right.timelineStartTicks || clipEndTicks(left) - clipEndTicks(right),
  )
  return sorted.some((clip, index) => index > 0 && clip.timelineStartTicks < clipEndTicks(sorted[index - 1]!))
}

export function sourceSupportsClip(source: CompositionSource, clip: CompositionClip): boolean {
  switch (clip.kind) {
    case 'video':
      return source.kind === 'video'
    case 'audio':
      return source.kind === 'audio' || (source.kind === 'video' && source.hasAudio)
    case 'image':
      return source.kind === 'image'
    case 'text':
      return false
  }
}

export function canMoveClipToTrack(clip: CompositionClip, track: CompositionTrack): boolean {
  return clip.kind === track.kind
}

function validateCanvas(
  value: unknown,
  add: (code: string, path: string, message: string) => void,
): void {
  if (!isRecord(value)) {
    add('canvas', 'canvas', 'canvas must be an object')
    return
  }
  if (!isIntegerBetween(value.width, MIN_CANVAS_DIMENSION, MAX_CANVAS_WIDTH)) {
    add('canvas-width', 'canvas.width', `Canvas width must be an integer from ${MIN_CANVAS_DIMENSION} to ${MAX_CANVAS_WIDTH}`)
  }
  if (!isIntegerBetween(value.height, MIN_CANVAS_DIMENSION, MAX_CANVAS_HEIGHT)) {
    add('canvas-height', 'canvas.height', `Canvas height must be an integer from ${MIN_CANVAS_DIMENSION} to ${MAX_CANVAS_HEIGHT}`)
  }
  if (Number.isInteger(value.width) && (value.width as number) % 2 !== 0) {
    add('canvas-width-even', 'canvas.width', 'Canvas width must be even for MP4 export')
  }
  if (Number.isInteger(value.height) && (value.height as number) % 2 !== 0) {
    add('canvas-height-even', 'canvas.height', 'Canvas height must be even for MP4 export')
  }
  if (!isFiniteNumber(value.fps) || value.fps <= 0 || value.fps > MAX_CANVAS_FPS) {
    add('canvas-fps', 'canvas.fps', `Canvas fps must be greater than 0 and at most ${MAX_CANVAS_FPS}`)
  }
  validateColor(value.backgroundColor, 'canvas.backgroundColor', add)
  if (value.backgroundMode !== undefined && !['color', 'blur', 'checker'].includes(String(value.backgroundMode))) {
    add('canvas-background-mode', 'canvas.backgroundMode', 'Canvas background mode must be color, blur, or checker')
  }
  if (value.backgroundBlur !== undefined && (!isFiniteNumber(value.backgroundBlur) || value.backgroundBlur < 1 || value.backgroundBlur > 100)) {
    add('canvas-background-blur', 'canvas.backgroundBlur', 'Canvas background blur must be from 1 to 100')
  }
}

function validateAuthoringMarkers(
  value: unknown,
  add: (code: string, path: string, message: string) => void,
): void {
  if (value === undefined) return
  if (!Array.isArray(value)) {
    add('markers', 'markers', 'Markers must be an array')
    return
  }
  if (value.length > MAX_AUTHORING_MARKERS) {
    add('marker-limit', 'markers', `A composition can contain at most ${MAX_AUTHORING_MARKERS} markers`)
  }
  const ids = new Set<string>()
  for (const [index, candidate] of value.entries()) {
    const path = `markers[${index}]`
    if (!isRecord(candidate)) {
      add('marker', path, 'Marker must be an object')
      continue
    }
    validateId(candidate.id, `${path}.id`, add)
    if (isStableId(candidate.id) && ids.has(candidate.id)) add('duplicate-marker-id', `${path}.id`, 'Marker id must be unique')
    else if (isStableId(candidate.id)) ids.add(candidate.id)
    if (!isSafeTick(candidate.tick) || candidate.tick > MAX_COMPOSITION_DURATION_TICKS) {
      add('marker-tick', `${path}.tick`, 'Marker tick is outside the composition limit')
    }
    if (
      typeof candidate.label !== 'string' ||
      !candidate.label.trim() ||
      [...candidate.label.trim()].length > MAX_AUTHORING_MARKER_LABEL
    ) {
      add('marker-label', `${path}.label`, `Marker label must contain 1 to ${MAX_AUTHORING_MARKER_LABEL} characters`)
    }
    if (candidate.color !== undefined && (typeof candidate.color !== 'string' || !/^#[0-9a-f]{6}$/i.test(candidate.color))) {
      add('marker-color', `${path}.color`, 'Marker color must be #rrggbb')
    }
    if (candidate.origin !== undefined && candidate.origin !== 'manual' && candidate.origin !== 'auto_beat') {
      add('marker-origin', `${path}.origin`, 'Marker origin must be manual or auto_beat')
    }
  }
}

function validateSourceMetadata(
  source: Record<string, unknown>,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (!isSafeTick(source.durationTicks)) {
    add('source-duration', `${path}.durationTicks`, 'Source durationTicks must be a non-negative safe integer')
  }
  if (!isNonNegativeInteger(source.width) || !isNonNegativeInteger(source.height)) {
    add('source-dimensions', path, 'Source width and height must be non-negative integers')
  }
  if (typeof source.hasAudio !== 'boolean') {
    add('source-audio', `${path}.hasAudio`, 'Source hasAudio must be a boolean')
  }
  if (
    source.fps !== undefined &&
    source.fps !== null &&
    (!isFiniteNumber(source.fps) || source.fps <= 0 || source.fps > 1_000)
  ) {
    add('source-fps', `${path}.fps`, 'Source fps must be null or a finite value from 0 to 1000')
  }
  for (const field of ['vcodec', 'acodec'] as const) {
    const codec = source[field]
    if (
      codec !== undefined &&
      codec !== null &&
      (typeof codec !== 'string' || !codec.trim() || codec.length > 128 || codec.includes('\0'))
    ) {
      add('source-codec', `${path}.${field}`, `${field} must be null or a non-empty safe string`)
    }
  }
  if (source.kind === 'video') {
    if (!isSafeTick(source.durationTicks) || source.durationTicks <= 0) {
      add('video-duration', `${path}.durationTicks`, 'Video duration must be positive')
    }
    if (!isPositiveInteger(source.width) || !isPositiveInteger(source.height)) {
      add('video-dimensions', path, 'Video width and height must be positive')
    }
  } else if (source.kind === 'audio') {
    if (!isSafeTick(source.durationTicks) || source.durationTicks <= 0) {
      add('audio-duration', `${path}.durationTicks`, 'Audio duration must be positive')
    }
    if (source.width !== 0 || source.height !== 0 || source.hasAudio !== true) {
      add('audio-metadata', path, 'Audio sources must have zero dimensions and hasAudio=true')
    }
  } else if (source.kind === 'image') {
    if (source.durationTicks !== 0 || !isPositiveInteger(source.width) || !isPositiveInteger(source.height) || source.hasAudio !== false) {
      add('image-metadata', path, 'Image sources must have durationTicks=0, positive dimensions, and hasAudio=false')
    }
  }
}

function validateClip(
  clip: Record<string, unknown>,
  path: string,
  sources: ReadonlyMap<string, CompositionSource>,
  add: (code: string, path: string, message: string) => void,
): void {
  if (!isSafeTick(clip.timelineStartTicks)) {
    add('clip-start', `${path}.timelineStartTicks`, 'timelineStartTicks must be a non-negative safe integer')
  }
  if (clip.kind === 'video' || clip.kind === 'audio') {
    validateSourceRange(clip, path, sources, add)
    if (clip.speed !== undefined && (!isFiniteNumber(clip.speed) || clip.speed < MIN_COMPOSITION_SPEED || clip.speed > MAX_COMPOSITION_SPEED)) {
      add('clip-speed', `${path}.speed`, `Speed must be from ${MIN_COMPOSITION_SPEED} to ${MAX_COMPOSITION_SPEED}`)
    }
    validateSpeedRamp(clip, path, add)
  } else if (clip.kind === 'image') {
    validateDuration(clip.durationTicks, `${path}.durationTicks`, add)
    validateSourceReference(clip, path, sources, add)
    validateTransform(clip.transform, `${path}.transform`, add)
    validateOpacity(clip.opacity, `${path}.opacity`, add)
    validateRotation(clip.rotationDegrees, `${path}.rotationDegrees`, add)
    validateBlendMode(clip.blendMode, `${path}.blendMode`, add)
    validateVisualAnimation(clip.animation, `${path}.animation`, runtimeClipDuration(clip), add)
    if (clip.speedRamp !== undefined) add('speed-ramp-kind', `${path}.speedRamp`, 'Speed ramp is available only for video/audio clips')
  } else if (clip.kind === 'text') {
    validateDuration(clip.durationTicks, `${path}.durationTicks`, add)
    if (typeof clip.text !== 'string' || !clip.text.length || clip.text.length > MAX_TEXT_LENGTH || clip.text.includes('\0')) {
      add('text', `${path}.text`, `Text must contain 1 to ${MAX_TEXT_LENGTH} characters and no NUL bytes`)
    }
    if (!isFiniteNumber(clip.x) || !isFiniteNumber(clip.y)) {
      add('text-position', path, 'Text x and y must be finite numbers')
    }
    validateOpacity(clip.opacity, `${path}.opacity`, add)
    validateRotation(clip.rotationDegrees, `${path}.rotationDegrees`, add)
    validateTextStyle(clip.style, `${path}.style`, add)
    validateVisualAnimation(clip.animation, `${path}.animation`, runtimeClipDuration(clip), add)
    if (clip.speedRamp !== undefined) add('speed-ramp-kind', `${path}.speedRamp`, 'Speed ramp is available only for video/audio clips')
  }

  if (clip.kind === 'video') {
    validateTransform(clip.transform, `${path}.transform`, add)
    validateOpacity(clip.opacity, `${path}.opacity`, add)
    validateRotation(clip.rotationDegrees, `${path}.rotationDegrees`, add)
    validateBlendMode(clip.blendMode, `${path}.blendMode`, add)
    validateChromaKey(clip.chromaKey, `${path}.chromaKey`, add)
    validateVideoEffects(clip.videoEffects, `${path}.videoEffects`, add)
    const duration = runtimeClipDuration(clip)
    validateVisualAnimation(clip.animation, `${path}.animation`, duration, add)
    validateMasks(clip.masks, `${path}.masks`, duration, add)
    if (clip.frameInterpolation !== undefined && clip.frameInterpolation !== 'duplicate' && clip.frameInterpolation !== 'optical_flow') {
      add('frame-interpolation', `${path}.frameInterpolation`, 'Frame interpolation must be duplicate or optical_flow')
    }
    validatePlaybackMode(clip, path, add)
    validateStabilization(clip, path, add)
    if (typeof clip.sourceAudioEnabled !== 'boolean') {
      add('source-audio-enabled', `${path}.sourceAudioEnabled`, 'sourceAudioEnabled must be a boolean')
    }
    validateGain(clip.audioGain, `${path}.audioGain`, add)
    if (clip.audioPan !== undefined && (!isFiniteNumber(clip.audioPan) || clip.audioPan < -1 || clip.audioPan > 1)) {
      add('video-audio-pan', `${path}.audioPan`, 'Embedded video audio pan must be from -1 to 1')
    }
    validateAudioAnimation(clip.audioAnimation, `${path}.audioAnimation`, duration, add)
    const source = typeof clip.sourceId === 'string' ? sources.get(clip.sourceId) : undefined
    if (
      clip.sourceAudioEnabled === true &&
      (!isRecord(clip.speedRamp) || clip.speedRamp.audioPolicy !== 'mute') &&
      source &&
      !source.hasAudio
    ) {
      add('missing-source-audio', `${path}.sourceAudioEnabled`, 'Video source does not contain audio')
    }
  } else if (clip.kind === 'audio') {
    validateGain(clip.gain, `${path}.gain`, add)
    if (clip.pan !== undefined && (!isFiniteNumber(clip.pan) || clip.pan < -1 || clip.pan > 1)) {
      add('audio-pan', `${path}.pan`, 'Audio pan must be from -1 to 1')
    }
    const duration = runtimeClipDuration(clip)
    validateAudioAnimation(clip.audioAnimation, `${path}.audioAnimation`, duration, add)
    if (clip.reversed !== undefined && typeof clip.reversed !== 'boolean') {
      add('audio-reversed', `${path}.reversed`, 'Audio reversed must be a boolean')
    }
    if (clip.voiceEffect !== undefined && clip.voiceEffect !== 'none' && clip.voiceEffect !== 'deep' && clip.voiceEffect !== 'high' && clip.voiceEffect !== 'chipmunk' && clip.voiceEffect !== 'echo' && clip.voiceEffect !== 'robot') {
      add('audio-voice-effect', `${path}.voiceEffect`, 'Audio voice effect must be none, deep, high, chipmunk, echo, or robot')
    }
    if (clip.pitchSemitones !== undefined && (!isFiniteNumber(clip.pitchSemitones) || clip.pitchSemitones < -12 || clip.pitchSemitones > 12)) {
      add('audio-pitch', `${path}.pitchSemitones`, 'Audio pitch must be from -12 to 12 semitones')
    }
    if (clip.toneDb !== undefined && (!isFiniteNumber(clip.toneDb) || clip.toneDb < -12 || clip.toneDb > 12)) {
      add('audio-tone', `${path}.toneDb`, 'Audio tone must be from -12 to 12 dB')
    }
    if (clip.crossfadeInTicks !== undefined && (!isSafeTick(clip.crossfadeInTicks) || (duration !== null && clip.crossfadeInTicks > duration))) {
      add('audio-crossfade', `${path}.crossfadeInTicks`, 'Audio crossfade must be a safe tick within the clip duration')
    }
    if (clip.ducking !== undefined) {
      if (!isRecord(clip.ducking) || !isFiniteNumber(clip.ducking.thresholdDb) || clip.ducking.thresholdDb < -60 || clip.ducking.thresholdDb > 0 ||
          !isFiniteNumber(clip.ducking.ratio) || clip.ducking.ratio < 1 || clip.ducking.ratio > 20 ||
          !isFiniteNumber(clip.ducking.attackMs) || clip.ducking.attackMs < 0.1 || clip.ducking.attackMs > 500 ||
          !isFiniteNumber(clip.ducking.releaseMs) || clip.ducking.releaseMs < 1 || clip.ducking.releaseMs > 5_000) {
        add('audio-ducking', `${path}.ducking`, 'Audio ducking controls are outside supported bounds')
      }
    }
    for (const [field, value] of [['fadeInTicks', clip.fadeInTicks], ['fadeOutTicks', clip.fadeOutTicks]] as const) {
      if (value !== undefined && (!isSafeTick(value) || (duration !== null && value > duration))) {
        add('audio-fade', `${path}.${field}`, 'Audio fade must be a safe tick within the clip duration')
      }
    }
  }
}

function validateVideoEffects(
  value: unknown,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (value === undefined) return
  if (!Array.isArray(value) || value.length > 5) {
    add('video-effects', path, 'Video effects must be an array with at most 5 entries')
    return
  }
  for (const [index, effect] of value.entries()) {
    if (!isRecord(effect) || !['blur', 'pixelate', 'vignette', 'sharpen', 'edge', 'rgb_split', 'posterize'].includes(String(effect.preset)) ||
        !isFiniteNumber(effect.intensity) || effect.intensity < 0.01 || effect.intensity > 1) {
      add('video-effect', `${path}[${index}]`, 'Video effect preset or intensity is invalid')
    }
  }
}

function validatePlaybackMode(
  clip: Record<string, unknown>,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  const value = clip.playbackMode
  if (value === undefined) return
  if (!isRecord(value) || (value.mode !== 'forward' && value.mode !== 'reverse' && value.mode !== 'freeze')) {
    add('playback-mode', `${path}.playbackMode`, 'Playback mode must be forward, reverse, or freeze')
    return
  }
  if (value.mode !== 'freeze') return
  if (
    !isSafeTick(value.sourceTick) ||
    (isSafeTick(clip.sourceInTicks) && value.sourceTick < clip.sourceInTicks) ||
    (isSafeTick(clip.sourceOutTicks) && value.sourceTick >= clip.sourceOutTicks)
  ) {
    add('freeze-source-tick', `${path}.playbackMode.sourceTick`, 'Freeze sourceTick must be inside the half-open source range')
  }
  if (clip.sourceAudioEnabled !== false) {
    add('freeze-source-audio', `${path}.sourceAudioEnabled`, 'Freeze-frame clips must disable embedded source audio')
  }
  if (clip.frameInterpolation === 'optical_flow') {
    add('freeze-optical-flow', `${path}.frameInterpolation`, 'Freeze-frame clips cannot use optical flow')
  }
  if (clip.speedRamp !== undefined) {
    add('freeze-speed-ramp', `${path}.speedRamp`, 'Freeze-frame clips cannot use speed ramp')
  }
}

function validateSpeedRamp(
  clip: Record<string, unknown>,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  const value = clip.speedRamp
  if (value === undefined) return
  if (!isRecord(value)) {
    add('speed-ramp', `${path}.speedRamp`, 'Speed ramp must be an object')
    return
  }
  for (const field of Object.keys(value)) {
    if (field !== 'interpolation' && field !== 'points' && field !== 'audioPolicy') {
      add('speed-ramp-field', `${path}.speedRamp.${field}`, `Unsupported speed ramp field ${field}`)
    }
  }
  if (Array.isArray(value.points)) for (const [index, point] of value.points.entries()) {
    if (!isRecord(point)) continue
    for (const field of Object.keys(point)) if (field !== 'sourceProgressTick' && field !== 'speed') {
      add('speed-ramp-point-field', `${path}.speedRamp.points[${index}].${field}`, `Unsupported speed ramp point field ${field}`)
    }
  }
  const speed = clip.speed ?? 1
  if (!isSafeTick(clip.sourceInTicks) || !isSafeTick(clip.sourceOutTicks) || !isFiniteNumber(speed)) return
  try {
    compositionSpeedRampSegments(
      clip.sourceOutTicks - clip.sourceInTicks,
      speed,
      value as unknown as CompositionSpeedRamp,
    )
  } catch (error) {
    add('speed-ramp', `${path}.speedRamp`, error instanceof Error ? error.message : 'Invalid speed ramp')
  }
}

function validateStabilization(
  clip: Record<string, unknown>,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  const value = clip.stabilization
  if (value === undefined) return
  if (!isRecord(value) || (value.mode !== 'disabled' && value.mode !== 'deshake')) {
    add('stabilization', `${path}.stabilization`, 'Stabilization must be disabled or deshake')
    return
  }
  const allowedFields = value.mode === 'disabled'
    ? new Set(['mode'])
    : new Set(['mode', 'radiusX', 'radiusY'])
  for (const field of Object.keys(value)) {
    if (!allowedFields.has(field)) {
      add('stabilization-field', `${path}.stabilization.${field}`, `Unsupported stabilization field ${field}`)
    }
  }
  if (value.mode === 'disabled') return
  const radii = new Set<number>(COMPOSITION_STABILIZATION_RADII)
  if (typeof value.radiusX !== 'number' || !radii.has(value.radiusX)) {
    add('stabilization-radius', `${path}.stabilization.radiusX`, 'Deshake radiusX must be 16, 32, 48, or 64')
  }
  if (typeof value.radiusY !== 'number' || !radii.has(value.radiusY)) {
    add('stabilization-radius', `${path}.stabilization.radiusY`, 'Deshake radiusY must be 16, 32, 48, or 64')
  }
  if (isRecord(clip.playbackMode) && clip.playbackMode.mode === 'freeze') {
    add('freeze-stabilization', `${path}.stabilization`, 'Freeze-frame clips cannot use deshake stabilization')
  }
}

function validateSourceRange(
  clip: Record<string, unknown>,
  path: string,
  sources: ReadonlyMap<string, CompositionSource>,
  add: (code: string, path: string, message: string) => void,
): void {
  validateSourceReference(clip, path, sources, add)
  if (!isSafeTick(clip.sourceInTicks) || !isSafeTick(clip.sourceOutTicks) || clip.sourceOutTicks <= clip.sourceInTicks) {
    add('source-range', path, 'sourceInTicks/sourceOutTicks must form a positive safe range')
    return
  }
  const source = typeof clip.sourceId === 'string' ? sources.get(clip.sourceId) : undefined
  if (source && clip.sourceOutTicks > source.durationTicks) {
    add('source-range', `${path}.sourceOutTicks`, 'Clip source range exceeds source duration')
  }
}

function validateSourceReference(
  clip: Record<string, unknown>,
  path: string,
  sources: ReadonlyMap<string, CompositionSource>,
  add: (code: string, path: string, message: string) => void,
): void {
  if (!isStableId(clip.sourceId)) {
    add('source-id', `${path}.sourceId`, 'sourceId must be a stable id')
    return
  }
  const source = sources.get(clip.sourceId)
  if (!source) {
    add('missing-source', `${path}.sourceId`, `Unknown source ${clip.sourceId}`)
    return
  }
  if (!sourceSupportsClip(source, clip as unknown as CompositionClip)) {
    add('source-kind', `${path}.sourceId`, `Source ${clip.sourceId} is incompatible with ${String(clip.kind)} clip`)
  }
}

function validateTransform(
  value: unknown,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (!isRecord(value)) {
    add('transform', path, 'Visual transform must be an object')
    return
  }
  if (
    !isFiniteNumber(value.x) ||
    !isFiniteNumber(value.y) ||
    !isFiniteNumber(value.width) ||
    !isFiniteNumber(value.height)
  ) {
    add('transform-number', path, 'Transform values must be finite numbers')
  } else if (value.width <= 0 || value.height <= 0) {
    add('transform-size', path, 'Transform width and height must be positive')
  }
  if (value.fit !== 'contain' && value.fit !== 'cover') {
    add('transform-fit', `${path}.fit`, 'Transform fit must be contain or cover')
  }
}

function validateTextStyle(
  value: unknown,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (!isRecord(value)) {
    add('text-style', path, 'Text style must be an object')
    return
  }
  if (!isFiniteNumber(value.fontSizePx) || value.fontSizePx < 1 || value.fontSizePx > 512) {
    add('font-size', `${path}.fontSizePx`, 'fontSizePx must be from 1 to 512')
  }
  validateColor(value.color, `${path}.color`, add)
  if (value.backgroundColor !== undefined) validateColor(value.backgroundColor, `${path}.backgroundColor`, add)
  if (value.align !== 'left' && value.align !== 'center' && value.align !== 'right') {
    add('text-align', `${path}.align`, 'Text align must be left, center, or right')
  }
  if (value.fontFamily !== undefined && (typeof value.fontFamily !== 'string' || !TEXT_FONTS.has(value.fontFamily))) {
    add('font-family', `${path}.fontFamily`, 'fontFamily is not supported by composition export')
  }
  if (value.strokeColor !== undefined) validateColor(value.strokeColor, `${path}.strokeColor`, add)
  if (value.shadowColor !== undefined) validateColor(value.shadowColor, `${path}.shadowColor`, add)
  if (value.strokeWidthPx !== undefined && (!isFiniteNumber(value.strokeWidthPx) || value.strokeWidthPx < 0 || value.strokeWidthPx > 100)) {
    add('stroke-width', `${path}.strokeWidthPx`, 'strokeWidthPx must be from 0 to 100')
  }
  for (const field of ['shadowX', 'shadowY'] as const) {
    if (value[field] !== undefined && !isFiniteNumber(value[field])) {
      add('text-shadow', `${path}.${field}`, `${field} must be finite`)
    }
  }
}

function validateTransitions(
  value: unknown,
  clips: readonly unknown[],
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (value === undefined) return
  if (!Array.isArray(value)) {
    add('transitions', path, 'Video transitions must be an array')
    return
  }
  const clipIds = new Set(clips.filter(isRecord).map((clip) => clip.id).filter(isStableId))
  const ids = new Set<string>()
  for (const [index, candidate] of value.entries()) {
    const transitionPath = `${path}[${index}]`
    if (!isRecord(candidate)) {
      add('transition', transitionPath, 'Transition must be an object')
      continue
    }
    validateId(candidate.id, `${transitionPath}.id`, add)
    if (isStableId(candidate.id) && ids.has(candidate.id)) add('duplicate-transition-id', `${transitionPath}.id`, 'Transition id must be unique')
    else if (isStableId(candidate.id)) ids.add(candidate.id)
    if (!isStableId(candidate.fromClipId) || !clipIds.has(candidate.fromClipId)) {
      add('transition-from', `${transitionPath}.fromClipId`, 'Transition source clip must exist in the video track')
    }
    if (!isStableId(candidate.toClipId) || !clipIds.has(candidate.toClipId) || candidate.toClipId === candidate.fromClipId) {
      add('transition-to', `${transitionPath}.toClipId`, 'Transition target clip must be a different clip in the video track')
    }
    if (!isSafeTick(candidate.durationTicks) || candidate.durationTicks <= 0) {
      add('transition-duration', `${transitionPath}.durationTicks`, 'Transition duration must be a positive safe tick')
    }
    if (typeof candidate.kind !== 'string' || !TRANSITION_KINDS.has(candidate.kind)) {
      add('transition-kind', `${transitionPath}.kind`, 'Transition kind is not supported')
    }
    const from = clips.find((clip) => isRecord(clip) && clip.id === candidate.fromClipId)
    const to = clips.find((clip) => isRecord(clip) && clip.id === candidate.toClipId)
    if ((isRecord(from) && from.speedRamp !== undefined) || (isRecord(to) && to.speedRamp !== undefined)) {
      add('speed-ramp-transition', transitionPath, 'Transitions cannot touch speed-ramped endpoint clips')
    }
  }
}

function validateAudioCrossfades(
  clips: readonly unknown[],
  trackPath: string,
  sources: ReadonlyMap<string, CompositionSource>,
  add: (code: string, path: string, message: string) => void,
): void {
  const ordered = [...clips.filter(isRecord)].sort((left, right) =>
    Number(left.timelineStartTicks) - Number(right.timelineStartTicks) || String(left.id).localeCompare(String(right.id)))
  const headOccupancy = new Map<string, number>()
  const tailOccupancy = new Map<string, number>()
  for (let index = 0; index < ordered.length; index += 1) {
    const clip = ordered[index]!
    const duration = clip.crossfadeInTicks
    if (duration === undefined || duration === 0 || !isSafeTick(duration)) continue
    const path = `${trackPath}.clips[${clips.indexOf(clip)}].crossfadeInTicks`
    const previous = ordered[index - 1]
    const previousDuration = previous ? runtimeClipDuration(previous) : null
    const currentDuration = runtimeClipDuration(clip)
    if (!previous || previousDuration === null || currentDuration === null ||
        Number(previous.timelineStartTicks) + previousDuration !== Number(clip.timelineStartTicks)) {
      add('audio-crossfade-boundary', path, 'Crossfade requires an immediately preceding clip on a touching boundary')
      continue
    }
    if (previous.speedRamp !== undefined || clip.speedRamp !== undefined) {
      add('audio-crossfade-speed-ramp', path, 'Crossfade does not support speed-ramped endpoint clips')
    }
    if (previous.reversed === true || clip.reversed === true) {
      add('audio-crossfade-reverse', path, 'Crossfade does not support reversed endpoint clips')
    }
    if (previous.audioAnimation !== undefined || clip.audioAnimation !== undefined) {
      add('audio-crossfade-automation', path, 'Crossfade endpoints must use constant gain and pan')
    }
    if (Number(previous.fadeOutTicks ?? 0) > 0 || Number(clip.fadeInTicks ?? 0) > 0) {
      add('audio-crossfade-fade', path, 'Crossfade replaces fade out/in at this boundary')
    }
    const before = Math.floor(duration / 2)
    const after = duration - before
    const previousSource = sources.get(String(previous.sourceId))
    const previousTail = Math.round(after * Number(previous.speed ?? 1))
    const currentHead = Math.round(before * Number(clip.speed ?? 1))
    if (!previousSource || previousSource.durationTicks - Number(previous.sourceOutTicks) < previousTail || Number(clip.sourceInTicks) < currentHead) {
      add('audio-crossfade-handles', path, 'Crossfade requires enough source audio before and after the edit')
    }
    tailOccupancy.set(String(previous.id), before)
    headOccupancy.set(String(clip.id), after)
  }
  for (const clip of ordered) {
    const duration = runtimeClipDuration(clip)
    if (duration !== null && (headOccupancy.get(String(clip.id)) ?? 0) + (tailOccupancy.get(String(clip.id)) ?? 0) > duration) {
      add('audio-crossfade-overlap', trackPath, `Crossfade windows overlap inside clip ${String(clip.id)}`)
    }
  }
}

function validateRotation(
  value: unknown,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (value !== undefined && (!isFiniteNumber(value) || Math.abs(value) > 3_600)) {
    add('rotation', path, 'Static rotation must be from -3600 to 3600 degrees')
  }
}

function validateBlendMode(
  value: unknown,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (value !== undefined && (typeof value !== 'string' || !BLEND_MODES.has(value))) {
    add('blend-mode', path, 'Blend mode is not supported')
  }
}

function validateChromaKey(
  value: unknown,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (value === undefined) return
  if (!isRecord(value)) {
    add('chroma-key', path, 'Chroma key must be an object')
    return
  }
  if (typeof value.enabled !== 'boolean') add('chroma-enabled', `${path}.enabled`, 'Chroma key enabled must be boolean')
  validateColor(value.color, `${path}.color`, add)
  for (const field of ['similarity', 'softness', 'spill'] as const) {
    const candidate = value[field]
    if (!isFiniteNumber(candidate) || candidate < 0 || candidate > 1) {
      add('chroma-value', `${path}.${field}`, `${field} must be from 0 to 1`)
    }
  }
}

function validateVisualAnimation(
  value: unknown,
  path: string,
  durationTicks: number | null,
  add: (code: string, path: string, message: string) => void,
): void {
  if (value === undefined) return
  if (!isRecord(value)) {
    add('visual-animation', path, 'Visual animation must be an object')
    return
  }
  for (const property of COMPOSITION_VISUAL_PROPERTIES) {
    if (value[property] === undefined) continue
    const bounds = visualPropertyBounds(property)
    validateAnimatableValue(
      value[property],
      `${path}.${property}`,
      durationTicks,
      bounds.minimum,
      bounds.maximum,
      true,
      add,
    )
  }
}

function validateAudioAnimation(
  value: unknown,
  path: string,
  durationTicks: number | null,
  add: (code: string, path: string, message: string) => void,
): void {
  if (value === undefined) return
  if (!isRecord(value)) {
    add('audio-animation', path, 'Audio animation must be an object')
    return
  }
  for (const property of COMPOSITION_AUDIO_PROPERTIES) {
    if (value[property] === undefined) continue
    const bounds = audioPropertyBounds(property)
    validateAnimatableValue(
      value[property],
      `${path}.${property}`,
      durationTicks,
      bounds.minimum,
      bounds.maximum,
      true,
      add,
    )
  }
}

function validateMasks(
  value: unknown,
  path: string,
  durationTicks: number | null,
  add: (code: string, path: string, message: string) => void,
): void {
  if (value === undefined) return
  if (!Array.isArray(value)) {
    add('masks', path, 'Video masks must be an array')
    return
  }
  const ids = new Set<string>()
  for (const [index, candidate] of value.entries()) {
    const maskPath = `${path}[${index}]`
    if (!isRecord(candidate)) {
      add('mask', maskPath, 'Mask must be an object')
      continue
    }
    validateId(candidate.id, `${maskPath}.id`, add)
    if (isStableId(candidate.id) && ids.has(candidate.id)) add('duplicate-mask-id', `${maskPath}.id`, 'Mask id must be unique within a clip')
    else if (isStableId(candidate.id)) ids.add(candidate.id)
    if (candidate.shape !== 'rectangle' && candidate.shape !== 'ellipse' && candidate.shape !== 'linear') {
      add('mask-shape', `${maskPath}.shape`, 'Mask shape must be rectangle, ellipse, or linear')
    }
    if (candidate.rotationDegrees !== undefined) {
      validateAnimatableValue(candidate.rotationDegrees, `${maskPath}.rotationDegrees`, durationTicks, -180, 180, true, add)
    }
    validateAnimatableValue(candidate.x, `${maskPath}.x`, durationTicks, 0, 1, true, add)
    validateAnimatableValue(candidate.y, `${maskPath}.y`, durationTicks, 0, 1, true, add)
    validateAnimatableValue(candidate.width, `${maskPath}.width`, durationTicks, 0, 2, false, add)
    validateAnimatableValue(candidate.height, `${maskPath}.height`, durationTicks, 0, 2, false, add)
    if (!isFiniteNumber(candidate.feather) || candidate.feather < 0 || candidate.feather > 1) {
      add('mask-feather', `${maskPath}.feather`, 'Mask feather must be from 0 to 1')
    }
    if (typeof candidate.inverted !== 'boolean') add('mask-inverted', `${maskPath}.inverted`, 'Mask inverted must be boolean')
  }
}

function validateAnimatableValue(
  value: unknown,
  path: string,
  durationTicks: number | null,
  minimum: number,
  maximum: number,
  inclusiveMinimum: boolean,
  add: (code: string, path: string, message: string) => void,
): void {
  if (!isRecord(value)) {
    add('animatable', path, 'Animatable value must be an object')
    return
  }
  const validValue = (candidate: unknown): candidate is number => isFiniteNumber(candidate) &&
    candidate <= maximum && (inclusiveMinimum ? candidate >= minimum : candidate > minimum)
  if (value.mode === 'constant') {
    if (!validValue(value.value)) add('animatable-value', `${path}.value`, `Animated value must be within ${minimum}..${maximum}`)
    return
  }
  if (value.mode !== 'keyframes' || !isRecord(value.track)) {
    add('animatable-mode', `${path}.mode`, 'Animatable mode must be constant or keyframes')
    return
  }
  const track = value.track
  if (!Number.isSafeInteger(track.timeBase) || (track.timeBase as number) <= 0 || (track.timeBase as number) > 0xffff_ffff) {
    add('keyframe-time-base', `${path}.track.timeBase`, 'Keyframe timeBase must be a positive u32 integer')
  }
  if (typeof track.interpolation !== 'string' || !INTERPOLATIONS.has(track.interpolation)) {
    add('keyframe-interpolation', `${path}.track.interpolation`, 'Keyframe interpolation is not supported')
  }
  if (!Array.isArray(track.keyframes) || !track.keyframes.length || track.keyframes.length > MAX_KEYFRAMES_PER_VALUE) {
    add('keyframe-count', `${path}.track.keyframes`, `Keyframe track must contain 1 to ${MAX_KEYFRAMES_PER_VALUE} points`)
    return
  }
  let previousTick = -1
  for (const [index, keyframe] of track.keyframes.entries()) {
    const keyframePath = `${path}.track.keyframes[${index}]`
    if (!isRecord(keyframe) || !isSafeTick(keyframe.tick)) {
      add('keyframe-tick', `${keyframePath}.tick`, 'Keyframe tick must be a non-negative safe integer')
      continue
    }
    if (keyframe.tick <= previousTick) add('keyframe-order', `${keyframePath}.tick`, 'Keyframe ticks must be strictly increasing')
    previousTick = keyframe.tick
    if (!validValue(keyframe.value)) add('keyframe-value', `${keyframePath}.value`, `Keyframe value must be within ${minimum}..${maximum}`)
  }
  const last = track.keyframes.at(-1)
  if (
    durationTicks !== null &&
    isRecord(last) &&
    isSafeTick(last.tick) &&
    Number.isSafeInteger(track.timeBase) &&
    (track.timeBase as number) > 0 &&
    BigInt(last.tick) * BigInt(COMPOSITION_TIME_BASE) > BigInt(durationTicks) * BigInt(track.timeBase as number)
  ) {
    add('keyframe-duration', `${path}.track.keyframes`, 'Keyframes must stay within the clip-local duration')
  }
}

function validateDuration(
  value: unknown,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (!isSafeTick(value) || value <= 0) add('clip-duration', path, 'Clip duration must be a positive safe integer')
}

function validateOpacity(
  value: unknown,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (!isFiniteNumber(value) || value < 0 || value > 1) add('opacity', path, 'Opacity must be from 0 to 1')
}

function validateGain(
  value: unknown,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (!isFiniteNumber(value) || value < 0 || value > MAX_AUDIO_GAIN) {
    add('gain', path, `Audio gain must be from 0 to ${MAX_AUDIO_GAIN}`)
  }
}

function validateId(
  value: unknown,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (!isStableId(value)) add('stable-id', path, 'Id must be a safe stable id up to 128 characters')
}

function validateColor(
  value: unknown,
  path: string,
  add: (code: string, path: string, message: string) => void,
): void {
  if (typeof value !== 'string' || !SAFE_COLOR.test(value)) {
    add('color', path, 'Color must be #RRGGBB or #RRGGBBAA')
  }
}

function runtimeClipDuration(clip: Record<string, unknown>): number | null {
  if (clip.kind === 'video' || clip.kind === 'audio') {
    if (!isSafeTick(clip.sourceInTicks) || !isSafeTick(clip.sourceOutTicks)) return null
    const speed = clip.speed === undefined ? 1 : clip.speed
    if (!isFiniteNumber(speed) || speed < MIN_COMPOSITION_SPEED || speed > MAX_COMPOSITION_SPEED) return null
    if (clip.speedRamp !== undefined) {
      if (!isRecord(clip.speedRamp)) return null
      try {
        return compositionSpeedRampSegments(
          clip.sourceOutTicks - clip.sourceInTicks,
          speed,
          clip.speedRamp as unknown as CompositionSpeedRamp,
        ).at(-1)!.timelineEndTick
      } catch {
        return null
      }
    }
    return Math.round((clip.sourceOutTicks - clip.sourceInTicks) / speed)
  }
  return isSafeTick(clip.durationTicks) ? clip.durationTicks : null
}

function runtimeMinimumSpeed(clip: Record<string, unknown>): number | null {
  const speed = clip.speed === undefined ? 1 : clip.speed
  if (!isFiniteNumber(speed) || speed < MIN_COMPOSITION_SPEED || speed > MAX_COMPOSITION_SPEED) return null
  if (clip.speedRamp === undefined) return speed
  if (!isRecord(clip.speedRamp) || !isSafeTick(clip.sourceInTicks) || !isSafeTick(clip.sourceOutTicks)) return null
  try {
    compositionSpeedRampSegments(
      clip.sourceOutTicks - clip.sourceInTicks,
      speed,
      clip.speedRamp as unknown as CompositionSpeedRamp,
    )
    return minimumCompositionSpeed(speed, clip.speedRamp as unknown as CompositionSpeedRamp)
  } catch {
    return null
  }
}

function activeSpeedRampSegmentCount(tracks: readonly unknown[]): number {
  const hasSolo = tracks.some(
    (track) => isRecord(track) && track.kind === 'audio' && track.muted === false && track.solo === true,
  )
  let total = 0
  for (const track of tracks) {
    if (!isRecord(track) || !Array.isArray(track.clips)) continue
    const activeVideo = track.kind === 'video' && track.hidden === false
    const activeAudio = track.kind === 'audio' && track.muted === false && (!hasSolo || track.solo === true)
    if (!activeVideo && !activeAudio) continue
    for (const clip of track.clips) {
      if (!isRecord(clip) || !isRecord(clip.speedRamp) || !Array.isArray(clip.speedRamp.points)) continue
      if (activeAudio && (clip.speedRamp.audioPolicy ?? 'preserve_pitch') === 'mute') continue
      total += Math.max(0, clip.speedRamp.points.length - 1)
    }
  }
  return total
}

function normalizeClip(value: unknown): unknown {
  if (!isRecord(value)) return value
  if (value.kind === 'video') {
    return {
      ...value,
      speed: value.speed === undefined ? 1 : value.speed,
      ...(value.speedRamp === undefined ? {} : { speedRamp: normalizeSpeedRamp(value.speedRamp) }),
      rotationDegrees: value.rotationDegrees === undefined ? 0 : value.rotationDegrees,
      blendMode: value.blendMode === undefined ? 'normal' : value.blendMode,
      audioPan: value.audioPan === undefined ? 0 : value.audioPan,
      frameInterpolation: value.frameInterpolation === undefined ? 'duplicate' : value.frameInterpolation,
      playbackMode: normalizePlaybackMode(value.playbackMode),
      stabilization: normalizeStabilization(value.stabilization),
      ...(value.animation === undefined ? {} : { animation: normalizeAnimation(value.animation, COMPOSITION_VISUAL_PROPERTIES) }),
      ...(value.audioAnimation === undefined ? {} : { audioAnimation: normalizeAnimation(value.audioAnimation, COMPOSITION_AUDIO_PROPERTIES) }),
      ...(value.masks === undefined ? {} : { masks: normalizeMasks(value.masks) }),
    }
  }
  if (value.kind === 'audio') {
    return {
      ...value,
      speed: value.speed === undefined ? 1 : value.speed,
      ...(value.speedRamp === undefined ? {} : { speedRamp: normalizeSpeedRamp(value.speedRamp) }),
      pan: value.pan === undefined ? 0 : value.pan,
      reversed: value.reversed === true,
      fadeInTicks: value.fadeInTicks === undefined ? 0 : value.fadeInTicks,
      fadeOutTicks: value.fadeOutTicks === undefined ? 0 : value.fadeOutTicks,
      voiceEffect: value.voiceEffect === undefined ? 'none' : value.voiceEffect,
      pitchSemitones: value.pitchSemitones === undefined ? 0 : value.pitchSemitones,
      toneDb: value.toneDb === undefined ? 0 : value.toneDb,
      crossfadeInTicks: value.crossfadeInTicks === undefined ? 0 : value.crossfadeInTicks,
      ...(value.ducking === undefined ? {} : { ducking: value.ducking }),
      ...(value.audioAnimation === undefined ? {} : { audioAnimation: normalizeAnimation(value.audioAnimation, COMPOSITION_AUDIO_PROPERTIES) }),
    }
  }
  if (value.kind === 'image') {
    return {
      ...value,
      rotationDegrees: value.rotationDegrees === undefined ? 0 : value.rotationDegrees,
      blendMode: value.blendMode === undefined ? 'normal' : value.blendMode,
      ...(value.animation === undefined ? {} : { animation: normalizeAnimation(value.animation, COMPOSITION_VISUAL_PROPERTIES) }),
    }
  }
  if (value.kind === 'text') {
    return {
      ...value,
      rotationDegrees: value.rotationDegrees === undefined ? 0 : value.rotationDegrees,
      ...(value.animation === undefined ? {} : { animation: normalizeAnimation(value.animation, COMPOSITION_VISUAL_PROPERTIES) }),
    }
  }
  return value
}

function normalizePlaybackMode(value: unknown): unknown {
  if (value === undefined) return { mode: 'forward' }
  return isRecord(value) ? { ...value } : value
}

function normalizeStabilization(value: unknown): unknown {
  if (value === undefined) return { mode: 'disabled' }
  return isRecord(value) ? { ...value } : value
}

function normalizeAnimation(value: unknown, properties: readonly string[]): unknown {
  if (!isRecord(value)) return value
  const normalized: Record<string, unknown> = { ...value }
  for (const property of properties) {
    if (value[property] !== undefined) normalized[property] = normalizeAnimatableValue(value[property])
  }
  return normalized
}

function normalizeMasks(value: unknown): unknown {
  if (!Array.isArray(value)) return value
  return value.map((candidate, index) => {
    if (!isRecord(candidate)) return candidate
    const normalized = normalizeAnimation(candidate, COMPOSITION_MASK_PROPERTIES) as Record<string, unknown>
    return {
      ...normalized,
      id: candidate.id ?? `mask-${index + 1}`,
      rotationDegrees: normalizeAnimatableValue(candidate.rotationDegrees ?? 0),
    }
  })
}

function normalizeAnimatableValue(value: unknown): unknown {
  if (isFiniteNumber(value)) return constantAnimatable(value)
  if (!isRecord(value)) return value
  if (value.mode === 'constant') return { ...value }
  if (value.mode === 'keyframes' && isRecord(value.track)) {
    const interpolation = typeof value.track.interpolation === 'string'
      ? normalizeInterpolation(value.track.interpolation as import('./types').CompositionInterpolation)
      : value.track.interpolation
    const keyframes = Array.isArray(value.track.keyframes)
      ? [...value.track.keyframes].sort((left, right) => {
          const leftTick = isRecord(left) && isFiniteNumber(left.tick) ? left.tick : 0
          const rightTick = isRecord(right) && isFiniteNumber(right.tick) ? right.tick : 0
          return leftTick - rightTick
        })
      : value.track.keyframes
    return { ...value, track: { ...value.track, interpolation, keyframes } }
  }
  return { ...value }
}

function normalizeSpeedRamp(value: unknown): unknown {
  if (!isRecord(value)) return value
  return {
    ...value,
    audioPolicy: value.audioPolicy === undefined ? 'preserve_pitch' : value.audioPolicy,
    ...(Array.isArray(value.points)
      ? { points: value.points.map((point) => isRecord(point) ? { ...point } : point) }
      : {}),
  }
}

function cloneUnknown(value: unknown): unknown {
  try {
    return JSON.parse(JSON.stringify(value)) as unknown
  } catch {
    return value
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value)
}

function isPositiveInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) > 0
}

function isNonNegativeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && (value as number) >= 0
}

function isIntegerBetween(value: unknown, min: number, max: number): value is number {
  return Number.isSafeInteger(value) && (value as number) >= min && (value as number) <= max
}
