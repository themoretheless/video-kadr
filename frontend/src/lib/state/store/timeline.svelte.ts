import {
  retainAudibleTimelineSegments,
  type SourceRange
} from '../../audio/silence.js'
import {
  activeTimelineSegments,
  MAX_TIMELINE_SEGMENTS,
  nextTimelineSegmentId,
  TIMELINE_MIN_SEGMENT_DURATION,
  timelineSegmentsFromLegacy
} from '../../domain/timeline.js'
import type {
  TimelineSegment
} from '../../types'
import {
  beginEditTransaction,
  endEditTransaction
} from './history.svelte.js'
import {
  state
} from './core.svelte.js'

function timelineTransaction<T>(key: string, mutate: () => T): T {
  beginEditTransaction(key)
  try {
    return mutate()
  } finally {
    endEditTransaction()
  }
}

export function activateTimeline(): string | null {
  const duration = state.video?.duration ?? 0
  if (duration <= 0) return null
  if (state.edit.timelineEnabled) return state.edit.timelineSegments[0]?.id ?? null
  const segments = timelineSegmentsFromLegacy(state.edit, duration)
  if (!segments.length) return null
  return timelineTransaction('timeline-activate', () => {
    state.edit.timelineSegments = segments
    state.edit.timelineEnabled = true
    return segments[0]!.id
  })
}

/** Apply locally detected audible source ranges to the canonical ordered timeline. */

export function applyAudibleTimelineRanges(audibleRanges: readonly SourceRange[]): number {
  const duration = state.video?.duration ?? 0
  if (duration <= 0) return 0
  const sourceSegments = activeTimelineSegments(state.edit, duration)
  const next = retainAudibleTimelineSegments(sourceSegments, audibleRanges)
  timelineTransaction('timeline-remove-silence', () => {
    state.edit.timelineSegments = next
    state.edit.timelineEnabled = true
    state.timelineSelectedSegmentId = next[0]!.id
  })
  return next.length
}

/** Replace one range while preserving its id and the exact UI/playback order. */

export function updateTimelineSegmentRange(
  id: string,
  patch: Partial<Pick<TimelineSegment, 'start' | 'end'>>,
): boolean {
  const duration = state.video?.duration ?? 0
  const index = state.edit.timelineSegments.findIndex((segment) => segment.id === id)
  const current = state.edit.timelineSegments[index]
  if (!state.edit.timelineEnabled || !current || duration <= 0) return false

  if (
    (patch.start !== undefined && !Number.isFinite(patch.start)) ||
    (patch.end !== undefined && !Number.isFinite(patch.end))
  ) {
    return false
  }
  let start = current.start
  let end = current.end
  if (patch.start !== undefined) {
    start = Math.max(0, Math.min(patch.start, end - TIMELINE_MIN_SEGMENT_DURATION))
  }
  if (patch.end !== undefined) {
    end = Math.min(duration, Math.max(patch.end, start + TIMELINE_MIN_SEGMENT_DURATION))
  }
  if (start === current.start && end === current.end) return false

  timelineTransaction('timeline-range', () => {
    state.edit.timelineSegments = state.edit.timelineSegments.map((segment, segmentIndex) =>
      segmentIndex === index ? { ...segment, start, end } : segment,
    )
  })
  return true
}

/** Split the selected range at an absolute source time. Returns the new right-hand id. */

export function splitTimelineSegment(id: string, at = state.playerTime): string | null {
  const index = state.edit.timelineSegments.findIndex((segment) => segment.id === id)
  const segment = state.edit.timelineSegments[index]
  if (
    !state.edit.timelineEnabled ||
    !segment ||
    state.edit.timelineSegments.length >= MAX_TIMELINE_SEGMENTS ||
    !Number.isFinite(at) ||
    at - segment.start < TIMELINE_MIN_SEGMENT_DURATION ||
    segment.end - at < TIMELINE_MIN_SEGMENT_DURATION
  ) {
    return null
  }

  const newId = nextTimelineSegmentId(state.edit.timelineSegments)
  return timelineTransaction('timeline-split', () => {
    const next = [...state.edit.timelineSegments]
    next.splice(
      index,
      1,
      { ...segment, end: at },
      { id: newId, start: at, end: segment.end },
    )
    state.edit.timelineSegments = next
    return newId
  })
}

/** Duplicate a range immediately after itself without collapsing equal source ranges. */

export function duplicateTimelineSegment(id: string): string | null {
  const index = state.edit.timelineSegments.findIndex((segment) => segment.id === id)
  const segment = state.edit.timelineSegments[index]
  if (
    !state.edit.timelineEnabled ||
    !segment ||
    state.edit.timelineSegments.length >= MAX_TIMELINE_SEGMENTS
  ) {
    return null
  }
  const newId = nextTimelineSegmentId(state.edit.timelineSegments)
  return timelineTransaction('timeline-duplicate', () => {
    const next = [...state.edit.timelineSegments]
    next.splice(index + 1, 0, { ...segment, id: newId })
    state.edit.timelineSegments = next
    return newId
  })
}

/** Delete a range, retaining at least one. Returns the nearest remaining selection. */

export function deleteTimelineSegment(id: string): string | null {
  const index = state.edit.timelineSegments.findIndex((segment) => segment.id === id)
  if (!state.edit.timelineEnabled || index < 0 || state.edit.timelineSegments.length <= 1) {
    return null
  }
  return timelineTransaction('timeline-delete', () => {
    const next = state.edit.timelineSegments.filter((segment) => segment.id !== id)
    state.edit.timelineSegments = next
    return next[Math.min(index, next.length - 1)]!.id
  })
}

/** Move a range one position in the UI/playback order. */

export function moveTimelineSegment(id: string, direction: -1 | 1): boolean {
  const index = state.edit.timelineSegments.findIndex((segment) => segment.id === id)
  const target = index + direction
  if (
    !state.edit.timelineEnabled ||
    index < 0 ||
    target < 0 ||
    target >= state.edit.timelineSegments.length
  ) {
    return false
  }
  timelineTransaction('timeline-move', () => {
    const next = [...state.edit.timelineSegments]
    ;[next[index], next[target]] = [next[target]!, next[index]!]
    state.edit.timelineSegments = next
  })
  return true
}

// --- media library ---

