import {
  COMPOSITION_TIME_BASE,
  MAX_COMPOSITION_CLIPS,
  MAX_COMPOSITION_DURATION_TICKS,
  type AudioClip,
  type Composition,
  type VideoClip,
} from './types'
import { isSafeTick, isStableId } from './validation'

export const MAX_MULTICAM_ANGLES = 8
export const MIN_MULTICAM_ANGLES = 2
export const MAX_MULTICAM_SWITCHES = Math.min(512, MAX_COMPOSITION_CLIPS)

export interface MulticamAngle {
  readonly id: string
  readonly label: string
  readonly sourceId: string
  /** Source tick aligned with `group.timelineStartTicks`. */
  readonly sourceTickAtGroupStart: number
}

export interface MulticamSwitch {
  readonly id: string
  /** Stable output clip id, so rebuilding the cut list is deterministic. */
  readonly clipId: string
  readonly timelineTick: number
  readonly angleId: string
}

export interface MulticamGroup {
  readonly id: string
  readonly name: string
  readonly timelineStartTicks: number
  readonly durationTicks: number
  readonly videoTrackId: string
  readonly audioTrackId?: string
  readonly audioClipId?: string
  readonly audioAngleId?: string
  readonly angles: readonly MulticamAngle[]
  readonly switches: readonly MulticamSwitch[]
}

export interface CompiledMulticamCuts {
  readonly videoTrackId: string
  readonly videoClips: readonly VideoClip[]
  readonly audioTrackId?: string
  readonly audioClip?: AudioClip
}

/** Convert waveform correlation's signed timeline offset to group source time. */
export function sourceTickAtGroupStartFromOffset(
  candidateStartOffsetSeconds: number,
  timeBase = COMPOSITION_TIME_BASE,
  groupStartShiftSeconds = 0,
): number {
  if (
    !Number.isFinite(candidateStartOffsetSeconds) ||
    !Number.isFinite(groupStartShiftSeconds) ||
    groupStartShiftSeconds < 0 ||
    !Number.isSafeInteger(timeBase) ||
    timeBase <= 0
  ) {
    throw new Error('Некорректное multicam sync offset')
  }
  const tick = Math.round((groupStartShiftSeconds - candidateStartOffsetSeconds) * timeBase)
  if (tick < 0) throw new Error('Сдвиньте начало multicam group после самого позднего angle')
  return tick
}

/** Insert or replace an angle switch without changing unrelated group data. */
export function recordMulticamSwitch(
  group: MulticamGroup,
  change: MulticamSwitch,
): MulticamGroup {
  validateGroupShape(group)
  validateSwitchList(group)
  validateSwitch(change, group)
  const switches = [
    ...group.switches.filter((candidate) => candidate.timelineTick !== change.timelineTick),
    { ...change },
  ].sort(compareSwitches)
  if (switches.length > MAX_MULTICAM_SWITCHES) {
    throw new Error(`Multicam switch limit is ${MAX_MULTICAM_SWITCHES}`)
  }
  if (switches[0]!.timelineTick !== group.timelineStartTicks) {
    throw new Error('Multicam cut list must retain a switch at group start')
  }
  return { ...group, switches }
}

/**
 * Materialize recorded angle switches as ordinary composition clips. Rendering
 * then uses the existing, tested primary video/audio pipeline with no hidden
 * multicam semantics in the backend.
 */
export function compileMulticamCuts(
  composition: Composition,
  group: MulticamGroup,
): CompiledMulticamCuts {
  validateGroupShape(group)
  validateSwitchList(group)
  const end = group.timelineStartTicks + group.durationTicks
  if (!Number.isSafeInteger(end) || end > MAX_COMPOSITION_DURATION_TICKS) {
    throw new Error('Multicam group exceeds the composition duration limit')
  }
  const videoTrack = composition.tracks.find((track) => track.id === group.videoTrackId)
  if (!videoTrack || videoTrack.kind !== 'video' || videoTrack.locked) {
    throw new Error('Multicam video track is missing, incompatible or locked')
  }
  if (!group.switches.length || group.switches.length > MAX_MULTICAM_SWITCHES) {
    throw new Error('Multicam group needs a bounded switch list')
  }
  const switches = [...group.switches].sort(compareSwitches)
  if (switches[0]!.timelineTick !== group.timelineStartTicks) {
    throw new Error('First multicam switch must start at the group boundary')
  }
  const angleById = new Map(group.angles.map((angle) => [angle.id, angle]))
  const videoClips = switches.map((change, index): VideoClip => {
    validateSwitch(change, group)
    const angle = angleById.get(change.angleId)!
    const source = composition.sources[angle.sourceId]
    if (!source || source.kind !== 'video') throw new Error(`Multicam angle ${angle.id} needs a video source`)
    const segmentEnd = switches[index + 1]?.timelineTick ?? end
    if (segmentEnd <= change.timelineTick) throw new Error('Multicam switches must have unique increasing ticks')
    const relativeStart = change.timelineTick - group.timelineStartTicks
    const sourceInTicks = angle.sourceTickAtGroupStart + relativeStart
    const sourceOutTicks = sourceInTicks + (segmentEnd - change.timelineTick)
    if (sourceInTicks < 0 || sourceOutTicks > source.durationTicks) {
      throw new Error(`Multicam angle ${angle.id} does not cover the requested group interval`)
    }
    return {
      id: change.clipId,
      kind: 'video',
      sourceId: angle.sourceId,
      timelineStartTicks: change.timelineTick,
      sourceInTicks,
      sourceOutTicks,
      speed: 1,
      frameInterpolation: 'duplicate',
      playbackMode: { mode: 'forward' },
      transform: {
        x: 0,
        y: 0,
        width: composition.canvas.width,
        height: composition.canvas.height,
        fit: 'contain',
      },
      opacity: 1,
      rotationDegrees: 0,
      blendMode: 'normal',
      sourceAudioEnabled: false,
      audioGain: 1,
      audioPan: 0,
    }
  })

  const audio = compileMasterAudio(composition, group)
  return {
    videoTrackId: group.videoTrackId,
    videoClips,
    ...(audio ? { audioTrackId: group.audioTrackId, audioClip: audio } : {}),
  }
}

function compileMasterAudio(composition: Composition, group: MulticamGroup): AudioClip | undefined {
  if (!group.audioAngleId) return undefined
  if (!group.audioTrackId || !group.audioClipId) {
    throw new Error('Multicam master audio needs stable track and clip ids')
  }
  const track = composition.tracks.find((candidate) => candidate.id === group.audioTrackId)
  if (!track || track.kind !== 'audio' || track.locked) {
    throw new Error('Multicam audio track is missing, incompatible or locked')
  }
  const angle = group.angles.find((candidate) => candidate.id === group.audioAngleId)
  if (!angle) throw new Error('Multicam master audio angle is missing')
  const source = composition.sources[angle.sourceId]
  if (!source?.hasAudio) throw new Error('Multicam master angle has no audio')
  const sourceOutTicks = angle.sourceTickAtGroupStart + group.durationTicks
  if (sourceOutTicks > source.durationTicks) throw new Error('Multicam master audio does not cover the group')
  return {
    id: group.audioClipId,
    kind: 'audio',
    sourceId: angle.sourceId,
    timelineStartTicks: group.timelineStartTicks,
    sourceInTicks: angle.sourceTickAtGroupStart,
    sourceOutTicks,
    speed: 1,
    gain: 1,
    pan: 0,
    fadeInTicks: 0,
    fadeOutTicks: 0,
  }
}

function validateGroupShape(group: MulticamGroup): void {
  for (const id of [group.id, group.videoTrackId, group.audioTrackId, group.audioClipId].filter(Boolean) as string[]) {
    if (!isStableId(id)) throw new Error(`Multicam id ${id} is not stable`)
  }
  if (!group.name.trim() || [...group.name].length > 128) throw new Error('Multicam name must be 1..128 characters')
  if (!isSafeTick(group.timelineStartTicks) || !isSafeTick(group.durationTicks) || group.durationTicks <= 0) {
    throw new Error('Multicam group has an invalid timeline range')
  }
  if (group.angles.length < MIN_MULTICAM_ANGLES || group.angles.length > MAX_MULTICAM_ANGLES) {
    throw new Error(`Multicam angle count must be ${MIN_MULTICAM_ANGLES}..${MAX_MULTICAM_ANGLES}`)
  }
  const ids = new Set<string>()
  for (const angle of group.angles) {
    if (!isStableId(angle.id) || !isStableId(angle.sourceId) || ids.has(angle.id)) {
      throw new Error('Multicam angles need unique stable ids')
    }
    if (!angle.label.trim() || [...angle.label].length > 128 || !isSafeTick(angle.sourceTickAtGroupStart)) {
      throw new Error(`Multicam angle ${angle.id} is invalid`)
    }
    ids.add(angle.id)
  }
}

function validateSwitch(change: MulticamSwitch, group: MulticamGroup): void {
  if (!isStableId(change.id) || !isStableId(change.clipId)) throw new Error('Multicam switch ids are not stable')
  if (!group.angles.some((angle) => angle.id === change.angleId)) throw new Error('Multicam switch angle is missing')
  const end = group.timelineStartTicks + group.durationTicks
  if (!isSafeTick(change.timelineTick) || change.timelineTick < group.timelineStartTicks || change.timelineTick >= end) {
    throw new Error('Multicam switch lies outside the group')
  }
}

function validateSwitchList(group: MulticamGroup): void {
  if (!group.switches.length || group.switches.length > MAX_MULTICAM_SWITCHES) {
    throw new Error(`Multicam switch count must be 1..${MAX_MULTICAM_SWITCHES}`)
  }
  const ids = new Set<string>()
  const clipIds = new Set<string>()
  const ticks = new Set<number>()
  for (const change of group.switches) {
    validateSwitch(change, group)
    if (ids.has(change.id) || clipIds.has(change.clipId) || ticks.has(change.timelineTick)) {
      throw new Error('Multicam switches need unique ids, clip ids and timeline ticks')
    }
    ids.add(change.id)
    clipIds.add(change.clipId)
    ticks.add(change.timelineTick)
  }
}

function compareSwitches(left: MulticamSwitch, right: MulticamSwitch): number {
  return left.timelineTick - right.timelineTick || left.id.localeCompare(right.id)
}
