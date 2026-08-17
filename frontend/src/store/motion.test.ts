import { beforeEach, describe, expect, it } from 'vitest'
import { putKeyframe } from '../domain/keyframes'
import {
  applyMotionSnapshot,
  MOTION_RANGES,
  motionActive,
  motionPayload,
  motionState,
  resetMotion,
  setMotionInterpolation,
  setMotionTrack,
} from './motion'

describe('motion store writes', () => {
  beforeEach(resetMotion)

  it('clamps zoom into the range the backend accepts, not the wider contract one', () => {
    // `domain::motion::MIN_ZOOM/MAX_ZOOM` reject the request outright, so a
    // value outside 1..8 must never reach the wire.
    expect(MOTION_RANGES.zoom).toEqual({ min: 1, max: 8, neutral: 1 })
    setMotionTrack('zoom', [
      { t: 0, v: 0.25, interp: 'linear' },
      { t: 2, v: 99, interp: 'linear' },
    ])
    expect(motionState.zoom.map((point) => point.v)).toEqual([1, 8])
  })

  it('snaps times onto the millisecond grid so the wire cannot carry a duplicate tick', () => {
    setMotionTrack('panX', [
      { t: 1.00049, v: 0.5, interp: 'linear' },
      { t: 1.00051, v: -0.5, interp: 'linear' },
    ])
    expect(motionState.panX).toEqual([
      { t: 1, v: 0.5, interp: 'linear' },
      { t: 1.001, v: -0.5, interp: 'linear' },
    ])
  })

  it('caps a track at 64 keyframes', () => {
    setMotionTrack(
      'rotation',
      Array.from({ length: 90 }, (_, index) => ({ t: index, v: 10, interp: 'linear' as const })),
    )
    expect(motionState.rotation).toHaveLength(64)
  })

  it('moves the whole track to one interpolation', () => {
    setMotionTrack('zoom', putKeyframe(putKeyframe([], 0, 1), 3, 1.5))
    setMotionInterpolation('zoom', 'smooth')
    expect(motionState.zoom.every((point) => point.interp === 'smooth')).toBe(true)
  })

  it('builds the wire payload from a Ken Burns pair and clears it again', () => {
    expect(motionActive()).toBe(false)
    setMotionTrack('zoom', putKeyframe(putKeyframe([], 0, 1), 4, 1.4))
    setMotionTrack('panX', putKeyframe(putKeyframe([], 0, -1), 4, 1))

    expect(motionPayload()).toEqual({
      motion: {
        zoom: [
          { t: 0, v: 1, interp: 'linear' },
          { t: 4, v: 1.4, interp: 'linear' },
        ],
        panX: [
          { t: 0, v: -1, interp: 'linear' },
          { t: 4, v: 1, interp: 'linear' },
        ],
      },
    })
    expect(motionActive()).toBe(true)

    setMotionTrack('zoom', [])
    setMotionTrack('panX', [])
    expect(motionPayload()).toEqual({})
  })

  it('keeps speed ramps a sibling of the transform block', () => {
    setMotionTrack('speedRamps', [
      { t: 0, v: 1, interp: 'hold' },
      { t: 2, v: 0.1, interp: 'hold' },
    ])
    expect(motionPayload()).toEqual({
      speedRamps: [
        { t: 0, v: 1, interp: 'hold' },
        { t: 2, v: 0.25, interp: 'hold' },
      ],
    })
  })

  it('survives a hostile snapshot', () => {
    applyMotionSnapshot({ motion: { zoom: 'nope', panY: [{ t: -1, v: Number.NaN }] } })
    expect(motionPayload()).toEqual({})
    expect(motionState.panY).toEqual([])
  })
})
