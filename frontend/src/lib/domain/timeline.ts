import type { EditState, TimelineSegment } from '../types'

export const TIMELINE_MIN_SEGMENT_DURATION = 0.05
export const MAX_TIMELINE_SEGMENTS = 512

const SAFE_SEGMENT_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/
const TIMELINE_OUTPUT_FORMATS = new Set(['mp4', 'webm', 'av1', 'prores'])

export function supportsTimelineFormat(format: string): boolean {
  return TIMELINE_OUTPUT_FORMATS.has(format)
}

function finiteDuration(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : 0
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(value, max))
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function safeId(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const id = value.trim()
  return SAFE_SEGMENT_ID.test(id) ? id : null
}

/** Return the first deterministic id not used by the supplied timeline. */
export function nextTimelineSegmentId(segments: readonly Pick<TimelineSegment, 'id'>[]): string {
  const used = new Set(segments.map((segment) => segment.id))
  return nextId(used)
}

function nextId(used: ReadonlySet<string>): string {
  let suffix = 1
  while (used.has(`segment-${suffix}`)) suffix += 1
  return `segment-${suffix}`
}

/**
 * Canonicalize untrusted timeline JSON without changing its playback order.
 * Invalid ranges are dropped, endpoints are clamped to the source duration,
 * and missing/unsafe/duplicate ids receive deterministic unique ids.
 */
export function sanitizeTimelineSegments(value: unknown, duration: number): TimelineSegment[] {
  const max = finiteDuration(duration)
  if (!Array.isArray(value) || max <= 0) return []

  const result: TimelineSegment[] = []
  const usedIds = new Set<string>()
  for (const candidate of value) {
    if (!isRecord(candidate)) continue
    if (
      typeof candidate.start !== 'number' ||
      !Number.isFinite(candidate.start) ||
      typeof candidate.end !== 'number' ||
      !Number.isFinite(candidate.end)
    ) {
      continue
    }
    const start = clamp(candidate.start, 0, max)
    const end = clamp(candidate.end, 0, max)
    if (end - start < TIMELINE_MIN_SEGMENT_DURATION) continue

    let id = safeId(candidate.id)
    if (!id || usedIds.has(id)) id = nextId(usedIds)
    usedIds.add(id)
    result.push({ id, start, end })
    if (result.length >= MAX_TIMELINE_SEGMENTS) break
  }
  return result
}

/** Convert the legacy trim plus optional middle cut into ordered keep-ranges. */
export function timelineSegmentsFromLegacy(
  edit: Pick<EditState, 'trimStart' | 'trimEnd' | 'cutEnabled' | 'cut'>,
  duration: number,
): TimelineSegment[] {
  const max = finiteDuration(duration)
  if (max <= 0) return []
  const rawStart = Number.isFinite(edit.trimStart) ? edit.trimStart : 0
  const rawEnd = Number.isFinite(edit.trimEnd) ? edit.trimEnd : max
  const start = clamp(rawStart, 0, max)
  const end = clamp(rawEnd, start, max)
  if (end - start < TIMELINE_MIN_SEGMENT_DURATION) {
    return [{ id: 'segment-1', start: 0, end: max }]
  }

  const ranges: Array<{ start: number; end: number }> = []
  const cutStart = Number.isFinite(edit.cut.start) ? clamp(edit.cut.start, start, end) : start
  const cutEnd = Number.isFinite(edit.cut.end) ? clamp(edit.cut.end, start, end) : cutStart
  if (edit.cutEnabled && cutEnd - cutStart >= TIMELINE_MIN_SEGMENT_DURATION) {
    if (cutStart - start >= TIMELINE_MIN_SEGMENT_DURATION) {
      ranges.push({ start, end: cutStart })
    }
    if (end - cutEnd >= TIMELINE_MIN_SEGMENT_DURATION) {
      ranges.push({ start: cutEnd, end })
    }
  }
  if (!ranges.length) ranges.push({ start, end })
  return ranges.map((range, index) => ({ id: `segment-${index + 1}`, ...range }))
}

/**
 * Resolve the ranges that define playback/export. Enabled timeline state wins;
 * malformed in-memory state fails safely to the legacy/full-clip conversion.
 */
export function activeTimelineSegments(edit: EditState, duration: number): TimelineSegment[] {
  if (edit.timelineEnabled) {
    const timeline = sanitizeTimelineSegments(edit.timelineSegments, duration)
    if (timeline.length) return timeline
  }
  return timelineSegmentsFromLegacy(edit, duration)
}

export function totalTimelineDuration(segments: readonly TimelineSegment[]): number {
  return segments.reduce((total, segment) => total + Math.max(0, segment.end - segment.start), 0)
}
