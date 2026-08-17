import { beforeEach, describe, expect, it } from 'vitest'
import { snapTime } from '../components/timeline/snapping'
import {
  addMarker,
  applyCompositionSnapshot,
  compositionPayload,
  compositionState,
  ensureClips,
  expectedOutputSeconds,
  MAX_MARKERS,
  moveClip,
  nextMarker,
  removeClip,
  removeMarker,
  resetComposition,
  rippleDelete,
  setClipTransition,
  splitAt,
  timelineDuration,
  timelineSlots,
  timelineToSource,
  trimClip,
} from './composition'

const SOURCE = 'vid_source'

function clipRanges(): [number, number][] {
  return compositionState.clips.map((clip) => [clip.start, clip.end])
}

beforeEach(() => {
  resetComposition()
})

describe('ensureClips', () => {
  it('materializes one clip covering the whole source', () => {
    expect(ensureClips(SOURCE, 12)).toBe(true)
    expect(clipRanges()).toEqual([[0, 12]])
    // A second call is a no-op: the timeline already exists.
    expect(ensureClips(SOURCE, 99)).toBe(true)
    expect(clipRanges()).toEqual([[0, 12]])
  })

  it('refuses a missing source or a degenerate duration', () => {
    expect(ensureClips('', 12)).toBe(false)
    expect(ensureClips(SOURCE, 0)).toBe(false)
    expect(ensureClips(SOURCE, Number.NaN)).toBe(false)
    expect(compositionState.clips).toHaveLength(0)
  })
})

describe('splitAt', () => {
  it('cuts the covering clip in two and leaves the tail without a transition', () => {
    ensureClips(SOURCE, 12)
    setClipTransition(0, { kind: 'fade', duration: 0.5 })
    expect(splitAt(5)).toBe(true)
    expect(clipRanges()).toEqual([
      [0, 5],
      [5, 12],
    ])
    expect(compositionState.clips[1]?.transitionIn).toBeNull()
  })

  it('respects clip speed when mapping the cut back into the source', () => {
    ensureClips(SOURCE, 12)
    compositionState.clips[0]!.speed = 2
    // At 2x the 12 second source plays for 6 seconds, so t=3 is source 6.
    expect(splitAt(3)).toBe(true)
    expect(clipRanges()).toEqual([
      [0, 6],
      [6, 12],
    ])
  })

  it('refuses a cut on a boundary or one that leaves a sliver', () => {
    ensureClips(SOURCE, 12)
    expect(splitAt(0)).toBe(false)
    expect(splitAt(12)).toBe(false)
    expect(splitAt(0.01)).toBe(false)
    expect(splitAt(Number.NaN)).toBe(false)
    expect(compositionState.clips).toHaveLength(1)
  })
})

describe('rippleDelete', () => {
  it('keeps the two surviving ranges as separate clips', () => {
    ensureClips(SOURCE, 12)
    expect(rippleDelete(4, 7)).toBe(true)
    expect(clipRanges()).toEqual([
      [0, 4],
      [7, 12],
    ])
    expect(timelineDuration()).toBeCloseTo(9)
  })

  it('spans several clips and closes the gap', () => {
    ensureClips(SOURCE, 12)
    splitAt(4)
    splitAt(8)
    expect(rippleDelete(3, 9)).toBe(true)
    expect(clipRanges()).toEqual([
      [0, 3],
      [9, 12],
    ])
  })

  it('drops markers inside the range and shifts the later ones left', () => {
    ensureClips(SOURCE, 12)
    addMarker(2)
    addMarker(5)
    addMarker(10)
    rippleDelete(4, 7)
    expect(compositionState.markers.map((marker) => marker.t)).toEqual([2, 7])
  })

  it('ignores a range that is empty or misses every clip', () => {
    ensureClips(SOURCE, 12)
    expect(rippleDelete(3, 3)).toBe(false)
    expect(rippleDelete(20, 30)).toBe(false)
    expect(clipRanges()).toEqual([[0, 12]])
  })
})

describe('reorder and trim', () => {
  it('moves a clip and clears the transition of the new opener', () => {
    ensureClips(SOURCE, 12)
    splitAt(6)
    setClipTransition(1, { kind: 'wipeleft', duration: 0.4 })
    expect(moveClip(1, 0)).toBe(true)
    expect(clipRanges()).toEqual([
      [6, 12],
      [0, 6],
    ])
    expect(compositionState.clips[0]?.transitionIn).toBeNull()
  })

  it('rejects an out-of-range move', () => {
    ensureClips(SOURCE, 12)
    expect(moveClip(0, 4)).toBe(false)
    expect(moveClip(0, 0)).toBe(false)
  })

  it('refuses a trim that would leave less than the minimum clip length', () => {
    ensureClips(SOURCE, 12)
    expect(trimClip(0, 2, 8)).toBe(true)
    expect(clipRanges()).toEqual([[2, 8]])
    expect(trimClip(0, 5, 5.01)).toBe(false)
    expect(trimClip(0, Number.POSITIVE_INFINITY, 8)).toBe(true)
    expect(compositionState.clips[0]?.start).toBe(2)
  })

  it('removes a clip and promotes the next one to opener', () => {
    ensureClips(SOURCE, 12)
    splitAt(6)
    setClipTransition(1, { kind: 'fade', duration: 0.5 })
    expect(removeClip(0)).toBe(true)
    expect(compositionState.clips).toHaveLength(1)
    expect(compositionState.clips[0]?.transitionIn).toBeNull()
  })
})

describe('timeline geometry', () => {
  it('maps a timeline point back onto the source of the covering clip', () => {
    ensureClips(SOURCE, 12)
    splitAt(4)
    moveClip(1, 0)
    // The tail (source 4..12) now plays first, so t=1 is source 5.
    expect(timelineToSource(1)).toEqual({ index: 0, seconds: 5 })
    expect(timelineToSource(9)).toEqual({ index: 1, seconds: 1 })
    expect(timelineSlots().map((slot) => slot.start)).toEqual([0, 8])
  })

  it('subtracts transition overlap from the expected output length', () => {
    ensureClips(SOURCE, 12)
    splitAt(6)
    expect(expectedOutputSeconds()).toBeCloseTo(12)
    setClipTransition(1, { kind: 'fade', duration: 0.5 })
    expect(expectedOutputSeconds()).toBeCloseTo(11.5)
  })

  it('degrades a transition longer than its neighbours to a hard cut', () => {
    ensureClips(SOURCE, 12)
    splitAt(0.06)
    setClipTransition(1, { kind: 'fade', duration: 3 })
    // 0.06 seconds of room minus the minimum piece leaves nothing to blend.
    expect(expectedOutputSeconds()).toBeCloseTo(12)
  })

  it('shortens a transition to the room its neighbours leave', () => {
    ensureClips(SOURCE, 12)
    splitAt(0.2)
    setClipTransition(1, { kind: 'fade', duration: 3 })
    expect(expectedOutputSeconds()).toBeCloseTo(11.85)
  })
})

describe('markers', () => {
  it('keeps markers sorted, unique and capped', () => {
    expect(addMarker(5, 'сцена')).toBe(true)
    expect(addMarker(1)).toBe(true)
    expect(addMarker(5.001)).toBe(false)
    expect(compositionState.markers.map((marker) => marker.t)).toEqual([1, 5])
    for (let index = 0; index < MAX_MARKERS + 5; index += 1) addMarker(100 + index)
    expect(compositionState.markers).toHaveLength(MAX_MARKERS)
  })

  it('navigates forwards and backwards and deletes by index', () => {
    addMarker(2)
    addMarker(8)
    expect(nextMarker(0, 1)?.t).toBe(2)
    expect(nextMarker(2, 1)?.t).toBe(8)
    expect(nextMarker(8, 1)).toBeNull()
    expect(nextMarker(8, -1)?.t).toBe(2)
    expect(removeMarker(0)).toBe(true)
    expect(removeMarker(9)).toBe(false)
    expect(compositionState.markers.map((marker) => marker.t)).toEqual([8])
  })
})

describe('payload and snapshot', () => {
  it('contributes nothing while the timeline is untouched', () => {
    expect(compositionPayload()).toEqual({})
  })

  it('serializes the split timeline onto the clips array', () => {
    ensureClips(SOURCE, 12)
    splitAt(6)
    setClipTransition(1, { kind: 'dissolve', duration: 0.4 })
    compositionState.clips[1]!.muted = true
    expect(compositionPayload()).toEqual({
      clips: [
        { sourceId: SOURCE, start: 0, end: 6 },
        {
          sourceId: SOURCE,
          start: 6,
          end: 12,
          muted: true,
          transitionIn: { kind: 'dissolve', duration: 0.4 },
        },
      ],
    })
  })

  it('never puts markers on the wire', () => {
    ensureClips(SOURCE, 12)
    addMarker(3, 'сцена')
    expect(Object.keys(compositionPayload())).toEqual(['clips'])
  })

  it('restores markers from an untrusted snapshot and drops the unusable ones', () => {
    applyCompositionSnapshot({
      clips: [{ sourceId: SOURCE, start: 0, end: 4 }],
      markers: [{ t: 9 }, { t: 'x' }, { t: Number.NaN }, { t: 2, label: 'a'.repeat(500) }],
    })
    expect(compositionState.markers.map((marker) => marker.t)).toEqual([2, 9])
    expect(compositionState.markers[0]?.label).toHaveLength(64)
  })

  it('clears markers on reset', () => {
    addMarker(3)
    resetComposition()
    expect(compositionState.markers).toEqual([])
  })
})

describe('snapTime', () => {
  it('captures the nearest target inside the tolerance', () => {
    const targets = [
      { t: 4, kind: 'clip' as const },
      { t: 4.4, kind: 'marker' as const },
    ]
    expect(snapTime(4.35, targets, 0.2)).toEqual({ t: 4.4, hit: targets[1] })
    expect(snapTime(4.35, targets, 0.02)).toEqual({ t: 4.35, hit: null })
    expect(snapTime(Number.NaN, targets, 0.2)).toEqual({ t: 0, hit: null })
  })
})
