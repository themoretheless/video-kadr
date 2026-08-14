import { describe, expect, it } from 'vitest'
import {
  enterOrderedTimeline,
  remapOrderedTimelineCursor,
  stepOrderedTimeline,
} from './videoPreviewPlayback'

const orderedSegments = [
  { start: 10, end: 12 },
  { start: 2, end: 4 },
  { start: 7, end: 8 },
]

describe('ordered timeline preview playback', () => {
  it('advances in array order and loops instead of sorting by source time', () => {
    expect(stepOrderedTimeline(orderedSegments, 12, 0)).toEqual({ index: 1, seekTo: 2 })
    expect(stepOrderedTimeline(orderedSegments, 4, 1)).toEqual({ index: 2, seekTo: 7 })
    expect(stepOrderedTimeline(orderedSegments, 8, 2)).toEqual({ index: 0, seekTo: 10 })
  })

  it('starts in a containing segment or seeks to the first authored segment', () => {
    expect(enterOrderedTimeline(orderedSegments, 3)).toEqual({ index: 1, seekTo: null })
    expect(enterOrderedTimeline(orderedSegments, 6)).toEqual({ index: 0, seekTo: 10 })
    expect(
      enterOrderedTimeline(
        [
          { start: 0, end: 5 },
          { start: 2, end: 4 },
        ],
        3,
      ),
    ).toEqual({ index: 0, seekTo: null })
  })

  it('preserves an explicit duplicate cursor when playback resumes', () => {
    const duplicates = [
      { start: 0, end: 2 },
      { start: 0, end: 2 },
    ]

    expect(enterOrderedTimeline(duplicates, 1, 1)).toEqual({ index: 1, seekTo: null })
    expect(enterOrderedTimeline(duplicates, 1, 99)).toEqual({ index: 0, seekTo: null })
  })

  it('starts from the preferred authored item instead of a later range containing source time', () => {
    expect(enterOrderedTimeline(orderedSegments, 3, 0)).toEqual({ index: 0, seekTo: 10 })
  })

  it('handles an empty edit and a stale cursor without throwing', () => {
    expect(stepOrderedTimeline([], 5, 9)).toEqual({ index: 0, seekTo: null })
    expect(stepOrderedTimeline(orderedSegments, 7.5, 99)).toEqual({ index: 2, seekTo: null })
  })

  it('remaps the same duplicate occurrence after reorder and clamps after deletion', () => {
    const reordered = [{ id: 'copy' }, { id: 'first' }, { id: 'last' }]

    expect(remapOrderedTimelineCursor(reordered, 'copy', 1)).toBe(0)
    expect(remapOrderedTimelineCursor(reordered, 'deleted', 9)).toBe(2)
    expect(remapOrderedTimelineCursor([], 'copy', 1)).toBe(0)
  })
})
