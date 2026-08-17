// Multi-source timeline: ordered clips with in/out points plus the transition
// used between plain `segments` when no clip list is present. Beyond the four
// contract symbols this module owns the timeline mutators the panel drives:
// split, ripple delete, reorder, trim and editor-only markers.
//
// Mutators never reach into the app store: the caller passes the source id and
// duration it already knows, which keeps this module free of import cycles and
// trivially testable.

import { reactive } from 'vue'
import type { ClipSpec, Transition, TransitionKind } from '../types'
import {
  boolOr,
  clampNumber,
  enumOr,
  finiteOr,
  isRecord,
  MAX_CLIPS,
  sanitizeList,
  textOr,
} from './validation'

/** FFmpeg `xfade` ids, contract section 3. */
export const TRANSITION_KINDS: readonly TransitionKind[] = [
  'fade',
  'wipeleft',
  'wiperight',
  'wipeup',
  'wipedown',
  'slideleft',
  'slideright',
  'slideup',
  'slidedown',
  'circleopen',
  'circleclose',
  'dissolve',
  'pixelize',
  'radial',
  'smoothleft',
  'smoothright',
  'zoomin',
]

const MIN_TRANSITION = 0.05
const MAX_TRANSITION = 3
const MIN_SPEED = 0.25
const MAX_SPEED = 4
const MAX_CLIP_VOLUME = 4
/** No source is longer than this in practice; it only bounds hostile input. */
const MAX_SOURCE_SECONDS = 24 * 60 * 60
/** Shorter than this the backend drops the piece, so the UI refuses to make it. */
export const MIN_CLIP_SECONDS = 0.05
/** Markers are an editing aid, not a wire field; the cap only bounds state. */
export const MAX_MARKERS = 64
const MARKER_LABEL_MAX = 64
/** Guards float comparisons when a click lands exactly on a clip boundary. */
const EPSILON = 1e-6

/** A named position on the output timeline. Never serialized to the wire. */
export interface Marker {
  t: number
  label: string
}

export interface CompositionState {
  /** Empty means "use videoId + segments", i.e. today's single-source path. */
  clips: ClipSpec[]
  /** Transition inserted between plain segments when `clips` is empty. */
  segmentTransition: Transition | null
  /** Editor-only markers, sorted by time. Not part of `EditRequest`. */
  markers: Marker[]
}

function defaults(): CompositionState {
  return { clips: [], segmentTransition: null, markers: [] }
}

export const compositionState = reactive<CompositionState>(defaults())

export function resetComposition(): void {
  compositionState.clips = []
  compositionState.segmentTransition = null
  compositionState.markers = []
}

/** Validate an untrusted transition object; null when it is unusable. */
export function sanitizeTransition(value: unknown): Transition | null {
  if (!isRecord(value)) return null
  if (typeof value.kind !== 'string') return null
  if (!TRANSITION_KINDS.includes(value.kind as TransitionKind)) return null
  return {
    kind: enumOr(value.kind, TRANSITION_KINDS, 'fade'),
    duration: clampNumber(value.duration, MIN_TRANSITION, MAX_TRANSITION, 0.5),
  }
}

/** Validate an untrusted clip; null when it carries no usable source/range. */
export function sanitizeClip(value: unknown, index: number): ClipSpec | null {
  if (!isRecord(value)) return null
  const sourceId = typeof value.sourceId === 'string' ? value.sourceId.trim() : ''
  if (!sourceId || sourceId.length > 128) return null
  const start = clampNumber(value.start, 0, MAX_SOURCE_SECONDS, 0)
  const end = clampNumber(value.end, 0, MAX_SOURCE_SECONDS, 0)
  if (!(end > start)) return null
  const clip: ClipSpec = {
    sourceId,
    start,
    end,
    speed: clampNumber(value.speed, MIN_SPEED, MAX_SPEED, 1),
    volume: clampNumber(value.volume, 0, MAX_CLIP_VOLUME, 1),
    muted: boolOr(value.muted, false),
    // The first clip has nothing to blend from, so its transition is dropped.
    transitionIn: index === 0 ? null : sanitizeTransition(value.transitionIn),
  }
  return clip
}

function sanitizeMarker(value: unknown): Marker | null {
  if (!isRecord(value)) return null
  if (typeof value.t !== 'number' || !Number.isFinite(value.t)) return null
  return {
    t: clampNumber(value.t, 0, MAX_SOURCE_SECONDS, 0),
    label: textOr(value.label, '', MARKER_LABEL_MAX),
  }
}

function serializeClip(clip: ClipSpec, index: number): Record<string, unknown> {
  const out: Record<string, unknown> = {
    sourceId: clip.sourceId,
    start: finiteOr(clip.start, 0),
    end: finiteOr(clip.end, 0),
  }
  const speed = clampNumber(clip.speed, MIN_SPEED, MAX_SPEED, 1)
  if (speed !== 1) out.speed = speed
  const volume = clampNumber(clip.volume, 0, MAX_CLIP_VOLUME, 1)
  if (volume !== 1) out.volume = volume
  if (clip.muted) out.muted = true
  const transition = index === 0 ? null : sanitizeTransition(clip.transitionIn)
  if (transition) out.transitionIn = { ...transition }
  return out
}

export function compositionPayload(): Record<string, unknown> {
  const payload: Record<string, unknown> = {}
  const clips = compositionState.clips
    .map((clip, index) => sanitizeClip(clip, index))
    .filter((clip): clip is ClipSpec => clip !== null)
    .slice(0, MAX_CLIPS)
  if (clips.length) {
    payload.clips = clips.map((clip, index) => serializeClip(clip, index))
  }
  const segmentTransition = sanitizeTransition(compositionState.segmentTransition)
  if (segmentTransition) payload.segmentTransition = { ...segmentTransition }
  return payload
}

export function applyCompositionSnapshot(raw: unknown): void {
  const source = isRecord(raw) ? raw : {}
  compositionState.clips = sanitizeList(source.clips, MAX_CLIPS, (item, index) =>
    sanitizeClip(item, index),
  )
  compositionState.segmentTransition = sanitizeTransition(source.segmentTransition)
  compositionState.markers = sortMarkers(
    sanitizeList(source.markers, MAX_MARKERS, (item) => sanitizeMarker(item)),
  )
}

// --- timeline geometry ---
// The panel lays clips out back to back on the OUTPUT timeline. Transition
// overlap is not folded into the layout (a shortened block is harder to grab);
// `expectedOutputSeconds` reports the real render length separately.

/** Output length of one clip in seconds, after its playback rate. */
export function clipDuration(clip: ClipSpec): number {
  const start = clampNumber(clip.start, 0, MAX_SOURCE_SECONDS, 0)
  const end = clampNumber(clip.end, 0, MAX_SOURCE_SECONDS, 0)
  const speed = clampNumber(clip.speed, MIN_SPEED, MAX_SPEED, 1)
  return Math.max(0, end - start) / speed
}

/** One clip placed on the output timeline. */
export interface TimelineSlot {
  index: number
  clip: ClipSpec
  start: number
  end: number
}

export function timelineSlots(): TimelineSlot[] {
  let cursor = 0
  return compositionState.clips.map((clip, index) => {
    const slot: TimelineSlot = { index, clip, start: cursor, end: cursor + clipDuration(clip) }
    cursor = slot.end
    return slot
  })
}

/** Laid-out length of the timeline, i.e. the width the panel has to draw. */
export function timelineDuration(): number {
  return compositionState.clips.reduce((total, clip) => total + clipDuration(clip), 0)
}

/**
 * Length the render will actually produce. Mirrors the backend rule: a
 * transition consumes overlap from both neighbours and degrades to a hard cut
 * once there is nothing meaningful left to blend.
 */
export function expectedOutputSeconds(): number {
  const clips = compositionState.clips
  let total = 0
  let previous = 0
  clips.forEach((clip, index) => {
    const current = clipDuration(clip)
    total += current
    if (index > 0) {
      const transition = sanitizeTransition(clip.transitionIn)
      if (transition) {
        const room = Math.min(previous, current) - MIN_CLIP_SECONDS
        const overlap = Math.min(transition.duration, room)
        if (overlap >= MIN_TRANSITION) total -= overlap
      }
    }
    previous = current
  })
  return Math.max(0, total)
}

/** Timeline seconds to a position inside the source of the covering clip. */
export function timelineToSource(t: number): { index: number; seconds: number } | null {
  if (!Number.isFinite(t)) return null
  const slots = timelineSlots()
  for (const slot of slots) {
    if (t < slot.end + EPSILON) {
      const speed = clampNumber(slot.clip.speed, MIN_SPEED, MAX_SPEED, 1)
      const offset = Math.max(0, t - slot.start) * speed
      return { index: slot.index, seconds: slot.clip.start + offset }
    }
  }
  const last = slots[slots.length - 1]
  return last ? { index: last.index, seconds: last.clip.end } : null
}

/** Inverse mapping: the first clip covering that point of the source. */
export function sourceToTimeline(sourceId: string, seconds: number): number | null {
  if (!Number.isFinite(seconds)) return null
  for (const slot of timelineSlots()) {
    if (slot.clip.sourceId !== sourceId) continue
    if (seconds >= slot.clip.start - EPSILON && seconds <= slot.clip.end + EPSILON) {
      const speed = clampNumber(slot.clip.speed, MIN_SPEED, MAX_SPEED, 1)
      return slot.start + (seconds - slot.clip.start) / speed
    }
  }
  return null
}

// --- timeline mutators ---

function newClip(sourceId: string, start: number, end: number): ClipSpec {
  return { sourceId, start, end, speed: 1, volume: 1, muted: false, transitionIn: null }
}

/**
 * Turn the implicit single-source timeline into an explicit one-clip list.
 * Every other mutator calls this first, so the panel never has to special-case
 * "the user has not split anything yet".
 */
export function ensureClips(sourceId: string, durationSeconds: number): boolean {
  if (compositionState.clips.length) return true
  const id = sourceId.trim()
  const end = clampNumber(durationSeconds, 0, MAX_SOURCE_SECONDS, 0)
  if (!id || id.length > 128 || end <= MIN_CLIP_SECONDS) return false
  compositionState.clips = [newClip(id, 0, end)]
  return true
}

/** Razor cut at a point on the output timeline. False when nothing was cut. */
export function splitAt(t: number): boolean {
  if (!Number.isFinite(t)) return false
  const slots = timelineSlots()
  for (const slot of slots) {
    if (t <= slot.start + EPSILON || t >= slot.end - EPSILON) continue
    const clip = slot.clip
    const speed = clampNumber(clip.speed, MIN_SPEED, MAX_SPEED, 1)
    const cut = clip.start + (t - slot.start) * speed
    if (cut - clip.start < MIN_CLIP_SECONDS || clip.end - cut < MIN_CLIP_SECONDS) return false
    if (compositionState.clips.length >= MAX_CLIPS) return false
    const tail: ClipSpec = { ...clip, start: cut, transitionIn: null }
    compositionState.clips.splice(slot.index, 1, { ...clip, end: cut }, tail)
    return true
  }
  return false
}

/**
 * Remove a range of the output timeline and close the gap. Fragments of a clip
 * that survive on both sides stay separate clips, which is exactly the
 * multi-range "keep" list the backend renders from `clips`.
 */
export function rippleDelete(from: number, to: number): boolean {
  if (!Number.isFinite(from) || !Number.isFinite(to)) return false
  const start = Math.max(0, Math.min(from, to))
  const end = Math.max(from, to)
  if (end - start < MIN_CLIP_SECONDS) return false
  const kept: ClipSpec[] = []
  let changed = false
  for (const slot of timelineSlots()) {
    if (end <= slot.start + EPSILON || start >= slot.end - EPSILON) {
      kept.push(slot.clip)
      continue
    }
    changed = true
    const speed = clampNumber(slot.clip.speed, MIN_SPEED, MAX_SPEED, 1)
    const toSource = (point: number): number => slot.clip.start + (point - slot.start) * speed
    const head = Math.max(slot.start, Math.min(start, slot.end))
    const tail = Math.max(slot.start, Math.min(end, slot.end))
    if (head - slot.start >= MIN_CLIP_SECONDS) {
      kept.push({ ...slot.clip, end: toSource(head) })
    }
    if (slot.end - tail >= MIN_CLIP_SECONDS) {
      kept.push({ ...slot.clip, start: toSource(tail), transitionIn: null })
    }
  }
  if (!changed) return false
  compositionState.clips = kept.slice(0, MAX_CLIPS)
  shiftMarkers(start, end)
  return true
}

/** Drop one clip entirely; the rest ripple left. */
export function removeClip(index: number): boolean {
  if (!Number.isInteger(index) || index < 0 || index >= compositionState.clips.length) return false
  compositionState.clips.splice(index, 1)
  const first = compositionState.clips[0]
  if (first) first.transitionIn = null
  return true
}

/** Reorder by drag: move the clip at `from` so it lands at position `to`. */
export function moveClip(from: number, to: number): boolean {
  const clips = compositionState.clips
  if (!Number.isInteger(from) || !Number.isInteger(to)) return false
  if (from < 0 || from >= clips.length || to < 0 || to >= clips.length || from === to) return false
  const [moved] = clips.splice(from, 1)
  if (!moved) return false
  clips.splice(to, 0, moved)
  const first = clips[0]
  if (first) first.transitionIn = null
  return true
}

/** Drag a clip edge: set new in/out points in SOURCE seconds. */
export function trimClip(index: number, start: number, end: number): boolean {
  const clip = compositionState.clips[index]
  if (!clip) return false
  const nextStart = clampNumber(start, 0, MAX_SOURCE_SECONDS, clip.start)
  const nextEnd = clampNumber(end, 0, MAX_SOURCE_SECONDS, clip.end)
  if (nextEnd - nextStart < MIN_CLIP_SECONDS) return false
  clip.start = nextStart
  clip.end = nextEnd
  return true
}

export function setClipTransition(index: number, value: unknown): boolean {
  const clip = compositionState.clips[index]
  if (!clip || index === 0) return false
  clip.transitionIn = sanitizeTransition(value)
  return true
}

export function setClipMuted(index: number, muted: boolean): boolean {
  const clip = compositionState.clips[index]
  if (!clip) return false
  clip.muted = muted === true
  return true
}

export function setClipSpeed(index: number, speed: number): boolean {
  const clip = compositionState.clips[index]
  if (!clip) return false
  clip.speed = clampNumber(speed, MIN_SPEED, MAX_SPEED, 1)
  return true
}

// --- markers ---

function sortMarkers(markers: Marker[]): Marker[] {
  return [...markers].sort((a, b) => a.t - b.t)
}

/** Move markers left past a deleted range; markers inside it are dropped. */
function shiftMarkers(start: number, end: number): void {
  const width = end - start
  compositionState.markers = compositionState.markers
    .filter((marker) => marker.t <= start + EPSILON || marker.t >= end - EPSILON)
    .map((marker) => (marker.t >= end ? { ...marker, t: marker.t - width } : marker))
}

export function addMarker(t: number, label = ''): boolean {
  if (!Number.isFinite(t)) return false
  if (compositionState.markers.length >= MAX_MARKERS) return false
  const time = clampNumber(t, 0, MAX_SOURCE_SECONDS, 0)
  // One marker per position: a second M on the same frame is a no-op, not a pile.
  if (compositionState.markers.some((marker) => Math.abs(marker.t - time) < 0.01)) return false
  compositionState.markers = sortMarkers([
    ...compositionState.markers,
    { t: time, label: textOr(label, '', MARKER_LABEL_MAX) },
  ])
  return true
}

export function removeMarker(index: number): boolean {
  if (!Number.isInteger(index) || index < 0 || index >= compositionState.markers.length) return false
  compositionState.markers.splice(index, 1)
  return true
}

export function setMarkerLabel(index: number, label: string): boolean {
  const marker = compositionState.markers[index]
  if (!marker) return false
  marker.label = textOr(label, '', MARKER_LABEL_MAX)
  return true
}

/** Nearest marker strictly before/after `t`, for J-K-L style navigation. */
export function nextMarker(t: number, direction: 1 | -1): Marker | null {
  if (!Number.isFinite(t)) return null
  const ordered = direction === 1 ? compositionState.markers : [...compositionState.markers].reverse()
  for (const marker of ordered) {
    if (direction === 1 ? marker.t > t + 0.01 : marker.t < t - 0.01) return marker
  }
  return null
}
