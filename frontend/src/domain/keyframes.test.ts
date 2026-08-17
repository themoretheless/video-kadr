import { describe, expect, it } from 'vitest'
import {
  easeProgress,
  indexOfTime,
  moveKeyframe,
  normalizeTrack,
  previewTransform,
  putKeyframe,
  rampOutputDuration,
  removeKeyframe,
  sampleMotion,
  sampleTrack,
  sampleTrackCurve,
  setTrackInterpolation,
  toTick,
  trackBounds,
  windowFromZoomPan,
  zoomPanFromWindow,
} from './keyframes'
import type { Interpolation, KeyframeTrack } from '../types'

/** Same shape the backend tests build, so both sides sample one fixture. */
function track(points: [number, number][], interp: Interpolation = 'linear'): KeyframeTrack {
  return points.map(([t, v]) => ({ t, v, interp }))
}

describe('sampleTrack', () => {
  // Fixture of `sampling_is_deterministic_at_boundaries_and_between_frames`
  // in backend/src/domain/keyframes.rs.
  const ramp = track([
    [0, 0],
    [1, 10],
    [2, 20],
  ])

  it('matches the backend fixture at boundaries and between keyframes', () => {
    expect(sampleTrack(ramp, 0.5, 0)).toBe(5)
    expect(sampleTrack(ramp, 1, 0)).toBe(10)
    expect(sampleTrack(ramp, 9, 0)).toBe(20)
  })

  it('holds the endpoint values outside the track', () => {
    expect(sampleTrack(ramp, -3, 0)).toBe(0)
    expect(sampleTrack(track([[2, 7]]), 0, 1)).toBe(7)
  })

  it('holds the left value until the next tick when the track holds', () => {
    const held = setTrackInterpolation(ramp, 'hold')
    expect(sampleTrack(held, 0.999, 0)).toBe(0)
    expect(sampleTrack(held, 1, 0)).toBe(10)
  })

  it('reads the interpolation from the first keyframe, like the wire conversion', () => {
    const mixed: KeyframeTrack = [
      { t: 0, v: 0, interp: 'hold' },
      { t: 1, v: 10, interp: 'linear' },
    ]
    expect(sampleTrack(mixed, 0.5, 0)).toBe(0)
  })

  it('eases smoothly with the backend cubic', () => {
    expect(easeProgress(0.25, 'smooth')).toBeCloseTo(4 * 0.25 ** 3, 12)
    expect(easeProgress(0.5, 'smooth')).toBeCloseTo(0.5, 12)
    expect(sampleTrack(track([[0, 0], [2, 8]], 'smooth'), 1, 0)).toBeCloseTo(4, 12)
  })

  it('returns the fallback for an unused parameter', () => {
    expect(sampleTrack([], 1.5, 1)).toBe(1)
  })

  it('quantizes time onto the millisecond grid the backend stores', () => {
    expect(toTick(1.00049)).toBe(1000)
    expect(sampleTrack(ramp, 1.00049, 0)).toBe(10)
  })
})

describe('motion tracks', () => {
  // Fixtures of `a_two_point_zoom_track_emits_the_expected_window_expression`
  // and `a_pan_track_offsets_the_window_by_the_available_headroom`.
  it('samples the zoom and pan tracks the render animates', () => {
    const tracks = {
      zoom: track([
        [0, 1],
        [4, 2],
      ]),
      panX: track([
        [0, -1],
        [2, 1],
      ]),
      panY: [],
      rotation: track([
        [0, 0],
        [2, 90],
      ]),
    }
    expect(sampleMotion(tracks, 0).zoom).toBe(1)
    expect(sampleMotion(tracks, 1).zoom).toBe(1.25)
    expect(sampleMotion(tracks, 5).zoom).toBe(2)
    expect(sampleMotion(tracks, 1).panX).toBe(0)
    expect(sampleMotion(tracks, 3).panX).toBe(1)
    expect(sampleMotion(tracks, 1).rotation).toBe(45)
    expect(sampleMotion(tracks, 1).panY).toBe(0)
  })

  it('clamps a sampled value into the range the backend accepts', () => {
    const tracks = { zoom: track([[0, 99]]), panX: [], panY: [], rotation: [] }
    expect(sampleMotion(tracks, 0).zoom).toBe(8)
  })

  it('samples a curve across the whole timeline', () => {
    const curve = sampleTrackCurve(track([[0, 1], [4, 2]]), 4, 4, 1)
    expect(curve).toEqual([1, 1.25, 1.5, 1.75, 2])
  })
})

describe('windowFromZoomPan', () => {
  it('centres the window when nothing pans', () => {
    expect(windowFromZoomPan(2, 0, 0)).toEqual({ x: 0.25, y: 0.25, size: 0.5 })
  })

  it('pins the window to the frame edges at the pan limits', () => {
    expect(windowFromZoomPan(2, -1, 1)).toEqual({ x: 0, y: 0.5, size: 0.5 })
  })

  it('round-trips through zoomPanFromWindow', () => {
    const restored = zoomPanFromWindow(windowFromZoomPan(4, -0.5, 0.25))
    expect(restored.zoom).toBeCloseTo(4, 9)
    expect(restored.panX).toBeCloseTo(-0.5, 9)
    expect(restored.panY).toBeCloseTo(0.25, 9)
  })

  it('reports no pan when the zoom leaves no headroom', () => {
    expect(zoomPanFromWindow({ x: 0, y: 0, size: 1 })).toEqual({
      zoom: 1,
      panX: 0,
      panY: 0,
      rotation: 0,
    })
  })
})

describe('previewTransform', () => {
  it('is the identity at rest', () => {
    expect(previewTransform({ zoom: 1, panX: 0, panY: 0, rotation: 0 })).toBe(
      'translate(0.000%, 0.000%) scale(1.0000) rotate(0.000deg)',
    )
  })

  it('shifts the frame by the pan headroom, opposite to the window', () => {
    // Window pinned left at zoom 2, so the frame moves right by half its width.
    expect(previewTransform({ zoom: 2, panX: -1, panY: 0, rotation: 0 })).toBe(
      'translate(50.000%, 0.000%) scale(2.0000) rotate(0.000deg)',
    )
  })
})

describe('rampOutputDuration', () => {
  // Fixture of `a_three_point_ramp_emits_the_expected_piecewise_setpts`:
  // 0..2 at 1x fills 2 s of output, 2..4 at 2x adds 1 s.
  const holds = track(
    [
      [0, 1],
      [2, 2],
      [4, 0.5],
    ],
    'hold',
  )

  it('sums the hold regions exactly like the setpts map', () => {
    expect(rampOutputDuration(holds, 4)).toBeCloseTo(3, 9)
    expect(rampOutputDuration(holds, 6)).toBeCloseTo(7, 9)
  })

  it('integrates a linear ramp like the render does', () => {
    // s(T) = 1 + 0.25*T over 0..4 gives log(2)/0.25 = 2.772589 seconds.
    expect(rampOutputDuration(track([[0, 1], [4, 2]]), 4)).toBeCloseTo(2.772589, 6)
  })

  it('integrates a smooth ramp as a linear one, matching the backend', () => {
    expect(rampOutputDuration(track([[0, 1], [4, 2]], 'smooth'), 4)).toBeCloseTo(2.772589, 6)
  })

  it('keeps the source duration when the parameter is unused', () => {
    expect(rampOutputDuration([], 7.5)).toBe(7.5)
  })

  it('cuts the ramp at the end of the source', () => {
    expect(rampOutputDuration(track([[0, 2], [10, 2]]), 4)).toBeCloseTo(2, 9)
  })

  it('clamps a speed into the range the backend accepts', () => {
    expect(rampOutputDuration(track([[0, 0.01]]), 1)).toBeCloseTo(4, 9)
  })
})

describe('track algebra', () => {
  it('sorts, deduplicates by tick and caps the track', () => {
    const normalized = normalizeTrack([
      { t: 2, v: 1, interp: 'linear' },
      { t: 0, v: 2, interp: 'linear' },
      { t: 2.0004, v: 3, interp: 'linear' },
    ])
    expect(normalized.map((point) => [point.t, point.v])).toEqual([
      [0, 2],
      [2, 3],
    ])
    expect(normalizeTrack(Array.from({ length: 80 }, (_, i) => ({ t: i, v: 1, interp: 'hold' as const })))).toHaveLength(64)
  })

  it('adds a keyframe at the playhead with the track interpolation', () => {
    const base = setTrackInterpolation(track([[0, 1]]), 'smooth')
    const next = putKeyframe(base, 1.5, 2)
    expect(next).toEqual([
      { t: 0, v: 1, interp: 'smooth' },
      { t: 1.5, v: 2, interp: 'smooth' },
    ])
  })

  it('replaces a keyframe that lands on the same tick', () => {
    const next = putKeyframe(track([[1, 1]]), 1.0004, 5)
    expect(next).toEqual([{ t: 1, v: 5, interp: 'linear' }])
  })

  it('nudges a dragged keyframe past its neighbour instead of eating it', () => {
    const base = track([
      [0, 1],
      [1, 2],
      [2, 3],
    ])
    const moved = moveKeyframe(base, 2, 1, 9)
    expect(moved).toHaveLength(3)
    expect(moved.map((point) => point.t)).toEqual([0, 1, 1.001])
    expect(indexOfTime(moved, 1.001)).toBe(2)
  })

  it('removes only the requested keyframe', () => {
    const base = track([
      [0, 1],
      [1, 2],
    ])
    expect(removeKeyframe(base, 0)).toEqual([{ t: 1, v: 2, interp: 'linear' }])
    expect(removeKeyframe(base, 9)).toBe(base)
  })

  it('bounds the plot around the neutral value', () => {
    expect(trackBounds([], 1, 1, 8).min).toBe(1)
    const bounds = trackBounds(track([[0, 2]]), 1, 1, 8)
    expect(bounds.min).toBe(1)
    expect(bounds.max).toBeGreaterThan(2)
    expect(bounds.max).toBeLessThanOrEqual(8)
  })
})
