import type { ProjectClip, ProjectDocument, ProjectMedia, ProjectTrack, SequenceSettings } from '../project-schema'
import { validateProjectDocument } from '../project-schema'

export type TimelineCommand =
  | { kind: 'batch'; commands: TimelineCommand[] }
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
    const entry = snapshotEntry(document, after, group)
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
    return preserveCompatibilityValues(entry.before, document)
  }

  redo(document: ProjectDocument): ProjectDocument {
    const entry = this.future.at(-1)
    if (!entry) return document
    if (!sameStructure(document, entry.before)) throw new Error('structural history precondition failed')
    this.future.pop()
    this.past.push(entry)
    this.usedBytes += entry.bytes
    this.evictToBudget()
    return preserveCompatibilityValues(entry.after, document)
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
  }
  validateTimelineSemantics(next)
  validateProjectDocument(next)
  return next
}

function compatibleMediaDescriptor(left: ProjectMedia, right: ProjectMedia): boolean {
  if (left.kind !== right.kind) return false
  for (const key of ['url', 'filename', 'duration', 'width', 'height', 'fps', 'vcodec', 'acodec']) {
    const leftValue = left.metadata[key]
    const rightValue = right.metadata[key]
    if (leftValue !== undefined && rightValue !== undefined && leftValue !== rightValue) return false
  }
  return true
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
  if (track.kind === 'video' && !['video', 'image'].includes(media.kind)) {
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
): HistoryEntry {
  const beforeSnapshot = cloneJson(before)
  const afterSnapshot = cloneJson(after)
  return {
    before: beforeSnapshot,
    after: afterSnapshot,
    bytes: snapshotBytes(beforeSnapshot, afterSnapshot),
    group,
  }
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
): ProjectDocument {
  const preserved = cloneJson(structuralDocument)
  preserved.name = currentDocument.name
  for (const media of preserved.media) {
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
