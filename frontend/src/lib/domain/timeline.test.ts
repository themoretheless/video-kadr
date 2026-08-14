import { describe, expect, it } from 'vitest'
import { defaultEdit } from './edit'
import {
  activeTimelineSegments,
  MAX_TIMELINE_SEGMENTS,
  sanitizeTimelineSegments,
  timelineSegmentsFromLegacy,
  totalTimelineDuration,
} from './timeline'

describe('sanitizeTimelineSegments', () => {
  it('preserves UI order, clamps duration and repairs duplicate or unsafe ids', () => {
    expect(
      sanitizeTimelineSegments(
        [
          { id: 'same', start: 8, end: 12 },
          { id: 'same', start: 1, end: 3 },
          { id: 'unsafe id', start: 4, end: 5 },
          { id: 'nan', start: Number.NaN, end: 8 },
          { id: 'tiny', start: 6, end: 6.01 },
        ],
        10,
      ),
    ).toEqual([
      { id: 'same', start: 8, end: 10 },
      { id: 'segment-1', start: 1, end: 3 },
      { id: 'segment-2', start: 4, end: 5 },
    ])
  })

  it('rejects a missing or non-finite source duration', () => {
    expect(sanitizeTimelineSegments([{ id: 'x', start: 0, end: 1 }], Number.NaN)).toEqual([])
    expect(sanitizeTimelineSegments([{ id: 'x', start: 0, end: 1 }], 0)).toEqual([])
  })

  it('caps untrusted persisted timelines to the backend graph limit', () => {
    const oversized = Array.from({ length: MAX_TIMELINE_SEGMENTS + 10 }, (_, index) => ({
      id: `item-${index}`,
      start: 0,
      end: 1,
    }))

    expect(sanitizeTimelineSegments(oversized, 10)).toHaveLength(MAX_TIMELINE_SEGMENTS)
  })
})

describe('timeline resolution', () => {
  it('converts legacy trim/cut to keep ranges', () => {
    const edit = defaultEdit()
    edit.trimStart = 2
    edit.trimEnd = 12
    edit.cutEnabled = true
    edit.cut = { start: 5, end: 8 }

    expect(timelineSegmentsFromLegacy(edit, 20)).toEqual([
      { id: 'segment-1', start: 2, end: 5 },
      { id: 'segment-2', start: 8, end: 12 },
    ])
  })

  it('uses enabled timeline order and includes duplicate source ranges', () => {
    const edit = defaultEdit()
    edit.trimEnd = 10
    edit.timelineEnabled = true
    edit.timelineSegments = [
      { id: 'b', start: 7, end: 9 },
      { id: 'a', start: 1, end: 4 },
      { id: 'a-copy', start: 1, end: 4 },
    ]
    const active = activeTimelineSegments(edit, 10)

    expect(active.map(({ start, end }) => ({ start, end }))).toEqual([
      { start: 7, end: 9 },
      { start: 1, end: 4 },
      { start: 1, end: 4 },
    ])
    expect(totalTimelineDuration(active)).toBe(8)
  })
})
