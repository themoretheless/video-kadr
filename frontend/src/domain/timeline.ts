import type { JsonObject, ProjectClip, ProjectDocument, ProjectMarker, ProjectMedia, ProjectMulticamGroup, ProjectTrack, SequenceSettings } from '../project-schema'
import { validateProjectDocument } from '../project-schema'

export type TimelineCommand =
  | { kind: 'batch'; commands: TimelineCommand[] }
  | { kind: 'set_multicam_groups'; groups: ProjectMulticamGroup[] }
  | { kind: 'attach_multicam_group'; sequenceId: string; clipId: string; groupId: string | null }
  | {
      kind: 'insert_media_clip'
      sequenceId: string
      trackId: string
      index: number
      media: ProjectMedia
      clip: ProjectClip
    }
  | { kind: 'insert_clip'; sequenceId: string; trackId: string; index: number; clip: ProjectClip }
  | { kind: 'remove_clip'; sequenceId: string; trackId: string; clipId: string }
  | {
      kind: 'split_clip'
      sequenceId: string
      trackId: string
      clipId: string
      leftClip: ProjectClip
      rightClip: ProjectClip
    }
  | {
      kind: 'move_clip'
      sequenceId: string
      clipId: string
      targetTrackId: string
      targetIndex: number
      timelineStartTick: number
    }
  | {
      kind: 'trim_clip'
      sequenceId: string
      clipId: string
      sourceInTick: number
      sourceOutTick: number
      timelineStartTick: number
    }
  | { kind: 'set_track_state'; sequenceId: string; trackId: string; patch: TrackStatePatch }
  | { kind: 'set_media_metadata'; sequenceId: string; mediaId: string; patch: JsonObject }
  | { kind: 'set_clip_transition'; sequenceId: string; trackId: string; clipId: string; transition: ClipTransition | null }
  | { kind: 'close_track_gap'; sequenceId: string; trackId: string; gapStartTick: number }
  | { kind: 'set_clip_opacity'; sequenceId: string; trackId: string; clipId: string; opacity: number | null }
  | { kind: 'add_marker'; sequenceId: string; marker: ProjectMarker }
  | { kind: 'update_marker'; sequenceId: string; markerId: string; patch: { timelineTick?: number; color?: string | null; label?: string | null } }
  | { kind: 'remove_marker'; sequenceId: string; markerId: string }

export const CLIP_TRANSITION_EFFECT_KIND = 'transition-in'

export type ClipTransitionType =
  | 'crossfade' | 'fade-black'
  | 'wipe-left' | 'wipe-right' | 'wipe-up' | 'wipe-down'
  | 'slide-left' | 'slide-right' | 'slide-up' | 'slide-down'
  | 'circle-open' | 'circle-close'

export const CLIP_TRANSITION_TYPES: readonly ClipTransitionType[] = [
  'crossfade', 'fade-black',
  'wipe-left', 'wipe-right', 'wipe-up', 'wipe-down',
  'slide-left', 'slide-right', 'slide-up', 'slide-down',
  'circle-open', 'circle-close',
]

const CLIP_XFADE_NAMES: Partial<Record<ClipTransitionType, string>> = {
  'wipe-left': 'wipeleft',
  'wipe-right': 'wiperight',
  'wipe-up': 'wipeup',
  'wipe-down': 'wipedown',
  'slide-left': 'slideleft',
  'slide-right': 'slideright',
  'slide-up': 'slideup',
  'slide-down': 'slidedown',
  'circle-open': 'circleopen',
  'circle-close': 'circleclose',
}

/** FFmpeg xfade transition name, or null for the fade-only types. */
export function clipTransitionXfadeType(type: ClipTransitionType): string | null {
  return CLIP_XFADE_NAMES[type] ?? null
}

export interface ClipTransition {
  type: ClipTransitionType
  durationTicks: number
}

export interface TimelineGap {
  startTick: number
  endTick: number
}

export interface TrackStatePatch {
  name?: string
  muted?: boolean
  solo?: boolean
  locked?: boolean
  hidden?: boolean
}

export function projectFrameDurationTicks(settings: SequenceSettings): number {
  const fps = settings.frameRate ?? 30
  const rationals: Array<[number, number, number]> = [
    [23.976, 24_000, 1_001],
    [29.97, 30_000, 1_001],
    [59.94, 60_000, 1_001],
  ]
  const matched = rationals.find(([candidate]) => Math.abs(candidate - fps) < 0.001)
  const numerator = matched?.[1] ?? Math.round(fps * 1_000)
  const denominator = matched?.[2] ?? 1_000
  const ticks = Math.round((settings.timeBase * denominator) / numerator)
  if (!Number.isSafeInteger(ticks) || ticks < 1) throw new Error('invalid project frame rate')
  return ticks
}

interface HistoryEntry {
  before: ProjectDocument
  after: ProjectDocument
  bytes: number
  group?: string
  ownedMediaIds: string[]
}

export class StructuralHistory {
  private past: HistoryEntry[] = []
  private future: HistoryEntry[] = []
  private usedBytes = 0

  constructor(private readonly maximumBytes = 8 * 1024 * 1024) {
    if (!Number.isSafeInteger(maximumBytes) || maximumBytes < 1) {
      throw new Error('invalid structural history budget')
    }
  }

  get canUndo(): boolean {
    return this.past.length > 0
  }

  get canRedo(): boolean {
    return this.future.length > 0
  }

  get bytes(): number {
    return this.usedBytes
  }

  execute(document: ProjectDocument, command: TimelineCommand, group?: string): ProjectDocument {
    const before = cloneJson(document)
    const after = applyTimelineCommand(before, command)
    const entry = snapshotEntry(document, after, group, ownedMediaIdsForCommand(command))
    const previous = group ? this.past.at(-1) : undefined
    if (previous && previous.group === group) {
      const mergedBytes = snapshotBytes(previous.before, entry.after)
      if (mergedBytes > this.maximumBytes) throw new Error('structural history budget exceeded')
      this.usedBytes -= previous.bytes
      this.past[this.past.length - 1] = {
        before: previous.before,
        after: entry.after,
        bytes: mergedBytes,
        group,
        ownedMediaIds: [...new Set([...previous.ownedMediaIds, ...entry.ownedMediaIds])],
      }
      this.usedBytes += this.past.at(-1)!.bytes
    } else {
      if (entry.bytes > this.maximumBytes) throw new Error('structural history budget exceeded')
      this.past.push(entry)
      this.usedBytes += entry.bytes
    }
    this.future = []
    this.evictToBudget()
    return after
  }

  undo(document: ProjectDocument): ProjectDocument {
    const entry = this.past.at(-1)
    if (!entry) return document
    if (!sameStructure(document, entry.after)) throw new Error('structural history precondition failed')
    this.past.pop()
    this.usedBytes -= entry.bytes
    this.future.push(entry)
    return preserveCompatibilityValues(entry.before, document, new Set(entry.ownedMediaIds))
  }

  redo(document: ProjectDocument): ProjectDocument {
    const entry = this.future.at(-1)
    if (!entry) return document
    if (!sameStructure(document, entry.before)) throw new Error('structural history precondition failed')
    this.future.pop()
    this.past.push(entry)
    this.usedBytes += entry.bytes
    this.evictToBudget()
    return preserveCompatibilityValues(entry.after, document, new Set(entry.ownedMediaIds))
  }

  clear(): void {
    this.past = []
    this.future = []
    this.usedBytes = 0
  }

  private evictToBudget(): void {
    while (this.usedBytes > this.maximumBytes && this.past.length > 1) {
      const removed = this.past.shift()!
      this.usedBytes -= removed.bytes
    }
  }
}

export function applyTimelineCommand(
  document: ProjectDocument,
  command: TimelineCommand,
): ProjectDocument {
  const next = cloneJson(document)
  if (command.kind === 'set_multicam_groups') {
    next.multicamGroups = cloneJson(command.groups)
    validateProjectDocument(next)
    return next
  }
  if (command.kind === 'batch') {
    if (command.commands.length === 0) throw new Error('empty timeline batch')
    return command.commands.reduce(
      (current, child) => applyTimelineCommand(current, child),
      next,
    )
  }
  const sequence = next.sequences.find((item) => item.id === command.sequenceId)
  if (!sequence) throw new Error(`missing sequence ${command.sequenceId}`)

  switch (command.kind) {
    case 'attach_multicam_group': {
      const located = locateClip(sequence.tracks, command.clipId)
      ensureUnlocked(located.track)
      if (command.groupId === null) delete located.clip.multicamGroupId
      else {
        if (!next.multicamGroups.some(group => group.id === command.groupId)) throw new Error(`missing multicam group ${command.groupId}`)
        located.clip.multicamGroupId = command.groupId
      }
      break
    }
    case 'insert_media_clip': {
      const track = requiredTrack(sequence.tracks, command.trackId)
      ensureUnlocked(track)
      if (command.index < 0 || command.index > track.clips.length) throw new Error('invalid clip index')
      if (command.clip.mediaId !== command.media.id) throw new Error('media clip identity mismatch')
      if (findClip(next, command.clip.id)) throw new Error(`duplicate clip ${command.clip.id}`)
      const existingMedia = next.media.find((media) => media.id === command.media.id)
      if (existingMedia && !compatibleMediaDescriptor(existingMedia, command.media)) {
        throw new Error(`conflicting media ${command.media.id}`)
      }
      if (!existingMedia) next.media.push(cloneJson(command.media))
      ensureTrackCompatibility(next, track, command.clip)
      track.clips.splice(command.index, 0, cloneJson(command.clip))
      break
    }
    case 'insert_clip': {
      const track = requiredTrack(sequence.tracks, command.trackId)
      ensureUnlocked(track)
      if (command.index < 0 || command.index > track.clips.length) throw new Error('invalid clip index')
      if (findClip(next, command.clip.id)) throw new Error(`duplicate clip ${command.clip.id}`)
      ensureTrackCompatibility(next, track, command.clip)
      track.clips.splice(command.index, 0, cloneJson(command.clip))
      break
    }
    case 'remove_clip': {
      const track = requiredTrack(sequence.tracks, command.trackId)
      ensureUnlocked(track)
      const index = track.clips.findIndex((clip) => clip.id === command.clipId)
      if (index < 0) throw new Error(`missing clip ${command.clipId}`)
      track.clips.splice(index, 1)
      break
    }
    case 'split_clip': {
      const track = requiredTrack(sequence.tracks, command.trackId)
      ensureUnlocked(track)
      const index = track.clips.findIndex((clip) => clip.id === command.clipId)
      if (index < 0) throw new Error(`missing clip ${command.clipId}`)
      const original = track.clips[index]!
      const collides = (candidate: string) =>
        candidate !== command.clipId && Boolean(findClip(next, candidate))
      if (
        command.leftClip.id === command.rightClip.id ||
        collides(command.leftClip.id) ||
        collides(command.rightClip.id)
      ) {
        throw new Error('duplicate clip id after split')
      }
      if (
        command.leftClip.timelineStartTick !== original.timelineStartTick ||
        command.leftClip.timelineStartTick + command.leftClip.durationTicks !== command.rightClip.timelineStartTick ||
        command.rightClip.timelineStartTick + command.rightClip.durationTicks !==
          original.timelineStartTick + original.durationTicks ||
        command.leftClip.sourceInTick !== original.sourceInTick ||
        command.rightClip.sourceOutTick !== original.sourceOutTick ||
        command.leftClip.sourceOutTick !== command.rightClip.sourceInTick ||
        command.leftClip.mediaId !== original.mediaId ||
        command.rightClip.mediaId !== original.mediaId
      ) {
        throw new Error('invalid split geometry')
      }
      track.clips.splice(index, 1, cloneJson(command.leftClip), cloneJson(command.rightClip))
      break
    }
    case 'move_clip': {
      const located = locateClip(sequence.tracks, command.clipId)
      const target = requiredTrack(sequence.tracks, command.targetTrackId)
      ensureUnlocked(located.track)
      ensureUnlocked(target)
      ensureTick(command.timelineStartTick, true)
      ensureTrackCompatibility(next, target, located.clip)
      located.track.clips.splice(located.index, 1)
      if (command.targetIndex < 0 || command.targetIndex > target.clips.length) {
        throw new Error('invalid clip index')
      }
      located.clip.timelineStartTick = command.timelineStartTick
      target.clips.splice(command.targetIndex, 0, located.clip)
      break
    }
    case 'trim_clip': {
      const located = locateClip(sequence.tracks, command.clipId)
      ensureUnlocked(located.track)
      ensureTick(command.sourceInTick, true)
      ensureTick(command.sourceOutTick, false)
      ensureTick(command.timelineStartTick, true)
      if (command.sourceOutTick <= command.sourceInTick) throw new Error('invalid trim range')
      const sourceDuration = mediaDurationTicks(next, located.clip.mediaId, sequence.settings.timeBase)
      if (sourceDuration !== undefined && command.sourceOutTick > sourceDuration) {
        throw new Error('trim exceeds source handles')
      }
      located.clip.sourceInTick = command.sourceInTick
      located.clip.sourceOutTick = command.sourceOutTick
      located.clip.durationTicks = command.sourceOutTick - command.sourceInTick
      located.clip.timelineStartTick = command.timelineStartTick
      break
    }
    case 'set_track_state': {
      const track = requiredTrack(sequence.tracks, command.trackId)
      if (command.patch.name !== undefined) {
        if (!command.patch.name.trim()) throw new Error('track name is required')
        track.name = command.patch.name
      }
      for (const key of ['muted', 'solo', 'locked', 'hidden'] as const) {
        const value = command.patch[key]
        if (value !== undefined) track[key] = value
      }
      break
    }
    case 'set_media_metadata': {
      const media = next.media.find((item) => item.id === command.mediaId)
      if (!media) throw new Error(`missing media ${command.mediaId}`)
      for (const [key, value] of Object.entries(command.patch)) {
        if (value === null) delete media.metadata[key]
        else media.metadata[key] = cloneJson(value)
      }
      break
    }
    case 'set_clip_transition': {
      const track = requiredTrack(sequence.tracks, command.trackId)
      ensureUnlocked(track)
      const clip = track.clips.find((item) => item.id === command.clipId)
      if (!clip) throw new Error(`missing clip ${command.clipId}`)
      clip.effects = clip.effects.filter((effect) => effect.kind !== CLIP_TRANSITION_EFFECT_KIND)
      if (command.transition) {
        const { type, durationTicks } = command.transition
        if (!CLIP_TRANSITION_TYPES.includes(type)) throw new Error('invalid transition type')
        if (!Number.isSafeInteger(durationTicks) || durationTicks < 1) throw new Error('invalid transition duration')
        if (clip.sourceInTick < durationTicks) throw new Error('transition exceeds source handles')
        const incomingMedia = next.media.find((item) => item.id === clip.mediaId)
        if (!incomingMedia || incomingMedia.kind !== 'video') throw new Error('transition requires a video incoming clip')
        const previous = track.clips.find((item) =>
          item.id !== clip.id &&
          item.timelineStartTick + item.durationTicks === clip.timelineStartTick,
        )
        if (!previous) throw new Error('transition requires an adjacent outgoing clip')
        const previousMedia = next.media.find((item) => item.id === previous.mediaId)
        if (!previousMedia || (previousMedia.kind !== 'video' && previousMedia.kind !== 'image')) {
          throw new Error('transition requires a video outgoing clip')
        }
        if (getClipTransition(previous)) throw new Error('transition cannot follow another transition')
        if (previous.durationTicks < durationTicks) throw new Error('transition exceeds outgoing clip duration')
        clip.effects.push({
          id: `${clip.id}-${CLIP_TRANSITION_EFFECT_KIND}`,
          kind: CLIP_TRANSITION_EFFECT_KIND,
          enabled: true,
          parameters: { contract: 'transition-v1', type, durationTicks },
        })
      }
      break
    }
    case 'set_clip_opacity': {
      const track = requiredTrack(sequence.tracks, command.trackId)
      ensureUnlocked(track)
      const clip = track.clips.find((item) => item.id === command.clipId)
      if (!clip) throw new Error(`missing clip ${command.clipId}`)
      if (command.opacity === null) {
        delete clip.opacity
        break
      }
      if (typeof command.opacity !== 'number' || !Number.isFinite(command.opacity)
        || command.opacity < 0 || command.opacity > 1) {
        throw new Error('invalid clip opacity')
      }
      clip.opacity = command.opacity
      break
    }
    case 'close_track_gap': {
      const track = requiredTrack(sequence.tracks, command.trackId)
      ensureUnlocked(track)
      ensureTick(command.gapStartTick, true)
      const gap = projectTrackGaps(track).find((item) => item.startTick === command.gapStartTick)
      if (!gap) throw new Error(`no gap at ${command.gapStartTick} on ${command.trackId}`)
      const sizeTicks = gap.endTick - gap.startTick
      for (const clip of track.clips) {
        if (clip.timelineStartTick >= gap.endTick) clip.timelineStartTick -= sizeTicks
      }
      break
    }
    case 'add_marker': {
      const markers = sequence.markers ?? []
      if (markers.some((item) => item.id === command.marker.id)) {
        throw new Error(`duplicate marker ${command.marker.id}`)
      }
      sequence.markers = [...markers, cloneJson(command.marker)]
      break
    }
    case 'update_marker': {
      const markers = sequence.markers ?? []
      const marker = markers.find((item) => item.id === command.markerId)
      if (!marker) throw new Error(`missing marker ${command.markerId}`)
      if (command.patch.timelineTick !== undefined) {
        if (!Number.isInteger(command.patch.timelineTick) || command.patch.timelineTick < 0) {
          throw new Error('invalid marker tick')
        }
        marker.timelineTick = command.patch.timelineTick
      }
      if (command.patch.color !== undefined) {
        if (command.patch.color === null) delete marker.color
        else {
          if (!/^#[0-9a-f]{6}$/i.test(command.patch.color)) throw new Error('invalid marker color')
          marker.color = command.patch.color
        }
      }
      if (command.patch.label !== undefined) {
        if (command.patch.label === null) delete marker.label
        else {
          if (command.patch.label.length > 200) throw new Error('invalid marker label')
          marker.label = command.patch.label
        }
      }
      break
    }
    case 'remove_marker': {
      const markers = sequence.markers ?? []
      const index = markers.findIndex((item) => item.id === command.markerId)
      if (index < 0) throw new Error(`missing marker ${command.markerId}`)
      sequence.markers = markers.filter((_, item) => item !== index)
      break
    }
  }
  validateTimelineSemantics(next)
  validateProjectDocument(next)
  return next
}

/** Split a timeline clip at a project tick into two source-continuous halves. */
export function splitClipAt(
  document: ProjectDocument,
  sequenceId: string,
  clipId: string,
  splitTick: number,
): Extract<TimelineCommand, { kind: 'split_clip' }> {
  const sequence = document.sequences.find((item) => item.id === sequenceId)
  if (!sequence) throw new Error(`missing sequence ${sequenceId}`)
  const located = locateClip(sequence.tracks, clipId)
  const { leftClip, rightClip } = buildSplitClips(document, located.clip, splitTick)
  return {
    kind: 'split_clip',
    sequenceId,
    trackId: located.track.id,
    clipId,
    leftClip,
    rightClip,
  }
}

function buildSplitClips(
  document: ProjectDocument,
  clip: ProjectClip,
  splitTick: number,
): { leftClip: ProjectClip; rightClip: ProjectClip } {
  if (!Number.isSafeInteger(splitTick)) throw new Error('invalid split tick')
  const offset = splitTick - clip.timelineStartTick
  if (offset <= 0 || offset >= clip.durationTicks) throw new Error('split point outside clip')
  const usedClipIds = new Set(
    document.sequences.flatMap((sequence) => sequence.tracks)
      .flatMap((track) => track.clips)
      .map((candidate) => candidate.id),
  )
  const usedEffectIds = new Set(
    document.sequences.flatMap((sequence) => sequence.tracks)
      .flatMap((track) => track.clips)
      .flatMap((candidate) => candidate.effects)
      .map((effect) => effect.id),
  )
  const uniqueId = (base: string, used: Set<string>): string => {
    let id = base
    let suffix = 1
    while (used.has(id)) id = `${base}-${++suffix}`
    used.add(id)
    return id
  }
  const left = cloneJson(clip)
  left.durationTicks = offset
  left.sourceOutTick = clip.sourceInTick + offset
  left.effects = left.effects.filter((effect) => effect.kind !== CLIP_TRANSITION_EFFECT_KIND)
  const right = cloneJson(clip)
  right.id = uniqueId(`${clip.id}-right`, usedClipIds)
  right.timelineStartTick = splitTick
  right.durationTicks = clip.durationTicks - offset
  right.sourceInTick = left.sourceOutTick
  right.effects = right.effects
    .filter((effect) => effect.kind !== CLIP_TRANSITION_EFFECT_KIND)
    .map((effect) => ({
      ...effect,
      id: uniqueId(`${effect.id}-right`, usedEffectIds),
    }))
  // The schema allows at most one multicam-attached clip per sequence; the
  // attachment stays with the left half.
  delete right.multicamGroupId
  return { leftClip: left, rightClip: right }
}

/** Read the transition attached to a clip's head, if any. */
export function getClipTransition(clip: ProjectClip): ClipTransition | null {
  const effect = clip.effects.find((item) => item.kind === CLIP_TRANSITION_EFFECT_KIND && item.enabled)
  if (!effect) return null
  const parameters = effect.parameters as Record<string, unknown>
  if (parameters.contract !== 'transition-v1') return null
  const type = parameters.type
  if (typeof type !== 'string' || !CLIP_TRANSITION_TYPES.includes(type as ClipTransitionType)) return null
  const durationTicks = parameters.durationTicks
  if (!Number.isSafeInteger(durationTicks) || (durationTicks as number) < 1) return null
  return { type: type as ClipTransitionType, durationTicks: durationTicks as number }
}

/** Remove a clip and shift every later same-track clip left by its duration. */
export function rippleDeleteClip(
  document: ProjectDocument,
  sequenceId: string,
  trackId: string,
  clipId: string,
): TimelineCommand {
  const sequence = document.sequences.find((item) => item.id === sequenceId)
  if (!sequence) throw new Error(`missing sequence ${sequenceId}`)
  const track = sequence.tracks.find((item) => item.id === trackId)
  if (!track) throw new Error(`missing track ${trackId}`)
  const removedIndex = track.clips.findIndex((clip) => clip.id === clipId)
  if (removedIndex < 0) throw new Error(`missing clip ${clipId}`)
  const removed = track.clips[removedIndex]!
  const removedEnd = removed.timelineStartTick + removed.durationTicks
  const commands: TimelineCommand[] = [
    { kind: 'remove_clip', sequenceId, trackId, clipId },
  ]
  const downstream = track.clips
    .filter((clip) => clip.id !== clipId && clip.timelineStartTick >= removedEnd)
    .sort((left, right) => left.timelineStartTick - right.timelineStartTick)
  for (const clip of downstream) {
    const currentIndex = track.clips.findIndex((candidate) => candidate.id === clip.id)
    const targetIndex = currentIndex > removedIndex ? currentIndex - 1 : currentIndex
    commands.push({
      kind: 'move_clip',
      sequenceId,
      clipId: clip.id,
      targetTrackId: trackId,
      targetIndex,
      timelineStartTick: clip.timelineStartTick - removed.durationTicks,
    })
  }
  return { kind: 'batch', commands }
}

function compatibleMediaDescriptor(left: ProjectMedia, right: ProjectMedia): boolean {
  if (left.kind !== right.kind) return false
  for (const key of ['filename', 'duration', 'width', 'height', 'fps', 'vcodec', 'acodec']) {
    const leftValue = left.metadata[key]
    const rightValue = right.metadata[key]
    if (leftValue !== undefined && rightValue !== undefined && leftValue !== rightValue) return false
  }
  return true
}

/** Free intervals on a track before or between clips, in timeline order.
 * Trailing space after the last clip is not a gap: closing it is a no-op. */
export function projectTrackGaps(track: ProjectTrack): TimelineGap[] {
  const gaps: TimelineGap[] = []
  let cursor = 0
  const ordered = [...track.clips].sort(
    (left, right) => left.timelineStartTick - right.timelineStartTick || left.id.localeCompare(right.id),
  )
  for (const clip of ordered) {
    if (clip.timelineStartTick > cursor) gaps.push({ startTick: cursor, endTick: clip.timelineStartTick })
    cursor = Math.max(cursor, clip.timelineStartTick + clip.durationTicks)
  }
  return gaps
}

export function validateTimelineSemantics(document: ProjectDocument): void {
  for (const sequence of document.sequences) {
    for (const track of sequence.tracks) {
      const ordered = [...track.clips].sort(
        (left, right) => left.timelineStartTick - right.timelineStartTick || left.id.localeCompare(right.id),
      )
      for (let index = 1; index < ordered.length; index++) {
        const previous = ordered[index - 1]!
        const current = ordered[index]!
        if (previous.timelineStartTick + previous.durationTicks > current.timelineStartTick) {
          throw new Error(`same-track overlap on ${track.id}`)
        }
      }
    }
  }
}

function requiredTrack(tracks: ProjectTrack[], id: string): ProjectTrack {
  const track = tracks.find((item) => item.id === id)
  if (!track) throw new Error(`missing track ${id}`)
  return track
}

function locateClip(tracks: ProjectTrack[], clipId: string) {
  for (const track of tracks) {
    const index = track.clips.findIndex((clip) => clip.id === clipId)
    if (index >= 0) return { track, index, clip: track.clips[index]! }
  }
  throw new Error(`missing clip ${clipId}`)
}

function findClip(document: ProjectDocument, clipId: string): ProjectClip | undefined {
  return document.sequences
    .flatMap((sequence) => sequence.tracks)
    .flatMap((track) => track.clips)
    .find((clip) => clip.id === clipId)
}

function ensureUnlocked(track: ProjectTrack): void {
  if (track.locked === true) throw new Error(`track ${track.id} is locked`)
}

function ensureTrackCompatibility(
  document: ProjectDocument,
  track: ProjectTrack,
  clip: ProjectClip,
): void {
  const media = document.media.find((item) => item.id === clip.mediaId)
  if (!media) throw new Error(`missing media ${clip.mediaId}`)
  if (track.kind === 'audio' && media.kind !== 'audio') throw new Error('incompatible track kind')
  if (track.kind === 'video' && !['video', 'image', 'text'].includes(media.kind)) {
    throw new Error('incompatible track kind')
  }
}

function mediaDurationTicks(
  document: ProjectDocument,
  mediaId: string,
  timeBase: number,
): number | undefined {
  const duration = document.media.find((item) => item.id === mediaId)?.metadata.duration
  if (typeof duration !== 'number' || !Number.isFinite(duration) || duration <= 0) return undefined
  const ticks = Math.round(duration * timeBase)
  return Number.isSafeInteger(ticks) ? ticks : undefined
}

function ensureTick(value: number, allowZero: boolean): void {
  if (!Number.isSafeInteger(value) || (allowZero ? value < 0 : value <= 0)) {
    throw new Error('invalid timeline tick')
  }
}

function snapshotEntry(
  before: ProjectDocument,
  after: ProjectDocument,
  group?: string,
  ownedMediaIds: readonly string[] = [],
): HistoryEntry {
  const beforeSnapshot = cloneJson(before)
  const afterSnapshot = cloneJson(after)
  return {
    before: beforeSnapshot,
    after: afterSnapshot,
    bytes: snapshotBytes(beforeSnapshot, afterSnapshot),
    group,
    ownedMediaIds: [...new Set(ownedMediaIds)],
  }
}

function ownedMediaIdsForCommand(command: TimelineCommand): string[] {
  if (command.kind === 'set_media_metadata') return [command.mediaId]
  if (command.kind === 'batch') return command.commands.flatMap(ownedMediaIdsForCommand)
  return []
}

function snapshotBytes(before: ProjectDocument, after: ProjectDocument): number {
  return new TextEncoder().encode(JSON.stringify(before) + JSON.stringify(after)).byteLength
}

function sameStructure(left: ProjectDocument, right: ProjectDocument): boolean {
  return stableStringify(structuralProjection(left)) === stableStringify(structuralProjection(right))
}

function stableStringify(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(',')}]`
  if (value !== null && typeof value === 'object') {
    const object = value as Record<string, unknown>
    return `{${Object.keys(object)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${stableStringify(object[key])}`)
      .join(',')}}`
  }
  return JSON.stringify(value)
}

function structuralProjection(document: ProjectDocument): ProjectDocument {
  const projected = cloneJson(document)
  projected.name = ''
  for (const media of projected.media) media.metadata = {}
  for (const effect of projected.sequences
    .flatMap((sequence) => sequence.tracks)
    .flatMap((track) => track.clips)
    .flatMap((clip) => clip.effects)) {
    if (effect.kind === 'legacy_edit') effect.parameters = {}
  }
  return projected
}

function preserveCompatibilityValues(
  structuralDocument: ProjectDocument,
  currentDocument: ProjectDocument,
  skipMediaIds: ReadonlySet<string> = new Set(),
): ProjectDocument {
  const preserved = cloneJson(structuralDocument)
  preserved.name = currentDocument.name
  for (const media of preserved.media) {
    if (skipMediaIds.has(media.id)) continue
    const current = currentDocument.media.find((candidate) => candidate.id === media.id)
    if (current) media.metadata = cloneJson(current.metadata)
  }
  for (const clip of preserved.sequences
    .flatMap((sequence) => sequence.tracks)
    .flatMap((track) => track.clips)) {
    const currentClip = currentDocument.sequences
      .flatMap((sequence) => sequence.tracks)
      .flatMap((track) => track.clips)
      .find((candidate) => candidate.id === clip.id)
    if (!currentClip) continue
    for (const effect of clip.effects.filter((candidate) => candidate.kind === 'legacy_edit')) {
      const currentEffect = currentClip.effects.find(
        (candidate) => candidate.id === effect.id && candidate.kind === 'legacy_edit',
      )
      if (currentEffect) effect.parameters = cloneJson(currentEffect.parameters)
    }
  }
  validateProjectDocument(preserved)
  return preserved
}

function cloneJson<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}
