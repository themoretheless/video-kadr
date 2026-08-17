import { beforeEach, describe, expect, it } from 'vitest'
// The reframe tracks are sampled with the shared keyframe helpers; these tests
// pin the semantics the viewport and the render stage agree on.
import { sampleTrack } from '../../domain/keyframes'
import {
  applySpatialSnapshot,
  clearReframeKeyframes,
  hasReframeAnimation,
  isViewOffKeyframes,
  reframeKeyframeTimes,
  removeReframeKeyframe,
  resetSpatial,
  setReframeKeyframe,
  setReframeTrack,
  setView,
  spatialPayload,
  spatialState,
  syncViewToPlayhead,
  viewAt,
  wrapDegrees,
} from '../../store/spatial'
import type { Reframe360Spec } from '../../types'

function reframe(): Reframe360Spec {
  const payload = spatialPayload().reframe360
  if (!payload) throw new Error('reframe360 missing from the payload')
  return payload as Reframe360Spec
}

beforeEach(() => {
  resetSpatial()
  spatialState.reframe360.enabled = true
})

describe('reframe camera', () => {
  it('sends the dragged framing as a single keyframe the render reads statically', () => {
    setView({ yaw: 120, pitch: 20, fov: 60 })

    expect(reframe().yaw).toEqual([{ t: 0, v: 120, interp: 'linear' }])
    expect(reframe().pitch).toEqual([{ t: 0, v: 20, interp: 'linear' }])
    expect(reframe().fov).toEqual([{ t: 0, v: 60, interp: 'linear' }])
    // An axis that never left its default stays out of the payload.
    expect(reframe().roll).toBeUndefined()
  })

  it('clamps the camera to the pole and folds yaw into a half turn', () => {
    setView({ pitch: 400, yaw: 200, roll: 200 })
    expect(spatialState.reframe360.view.pitch).toBe(90)
    expect(spatialState.reframe360.view.yaw).toBe(-160)
    expect(spatialState.reframe360.view.roll).toBe(-160)
    expect(wrapDegrees(270)).toBe(-90)
  })

  it('keeps the field of view inside the band its projection can show', () => {
    setView({ fov: 300 })
    expect(spatialState.reframe360.view.fov).toBe(170)

    spatialState.reframe360.outputProjection = 'fisheye'
    setView({ fov: 300 })
    expect(spatialState.reframe360.view.fov).toBe(300)
  })

  it('drops the field of view for an equirectangular output, which ignores it', () => {
    setView({ fov: 60 })
    spatialState.reframe360.outputProjection = 'equirect'
    expect(reframe().fov).toBeUndefined()
    expect(reframe().outputProjection).toBe('equirect')
  })

  it('never sends roll while the horizon is locked', () => {
    setView({ roll: 30 })
    expect(spatialState.reframe360.horizonLock).toBe(true)
    expect(reframe().roll).toBeUndefined()
    expect(viewAt(0).roll).toBe(0)

    spatialState.reframe360.horizonLock = false
    expect(reframe().roll).toEqual([{ t: 0, v: 30, interp: 'linear' }])
  })
})

describe('reframe keyframes', () => {
  it('keyframes every axis together at the playhead', () => {
    setView({ yaw: 10, pitch: 5, fov: 80 })
    expect(setReframeKeyframe(0)).toBe(true)
    setView({ yaw: 90, pitch: -5, fov: 100 })
    expect(setReframeKeyframe(2.5004)).toBe(true)

    expect(reframeKeyframeTimes()).toEqual([0, 2.5])
    expect(reframe().yaw).toEqual([
      { t: 0, v: 10, interp: 'linear' },
      { t: 2.5, v: 90, interp: 'linear' },
    ])
    // Roll is locked out, so it is not keyframed either.
    expect(spatialState.reframe360.roll).toEqual([])
    expect(hasReframeAnimation()).toBe(true)
  })

  it('replaces a keyframe that already sits on the playhead', () => {
    setView({ yaw: 10 })
    setReframeKeyframe(1)
    setView({ yaw: 40 })
    setReframeKeyframe(1)

    expect(reframe().yaw).toEqual([{ t: 1, v: 40, interp: 'linear' }])
  })

  it('refuses a 65th keyframe instead of keyframing some axes and not others', () => {
    for (let index = 0; index < 64; index += 1) {
      setView({ yaw: index })
      expect(setReframeKeyframe(index)).toBe(true)
    }
    expect(setReframeKeyframe(100)).toBe(false)
    expect(spatialState.reframe360.yaw).toHaveLength(64)
    expect(spatialState.reframe360.pitch).toHaveLength(64)
  })

  it('takes an edited track from the lane and clamps it into the axis range', () => {
    setReframeTrack('pitch', [
      { t: 1, v: 500, interp: 'smooth' },
      { t: 0, v: -500, interp: 'smooth' },
      { t: 2, v: Number.NaN, interp: 'smooth' },
    ])

    expect(spatialState.reframe360.pitch).toEqual([
      { t: 0, v: -90, interp: 'smooth' },
      { t: 1, v: 90, interp: 'smooth' },
    ])
    expect(reframe().pitch?.[0]?.interp).toBe('smooth')
  })

  it('removes a single keyframe and clears the whole set', () => {
    setReframeKeyframe(0)
    setView({ yaw: 45 })
    setReframeKeyframe(1)
    removeReframeKeyframe(0)
    expect(reframeKeyframeTimes()).toEqual([1])

    clearReframeKeyframes(1)
    expect(reframeKeyframeTimes()).toEqual([])
    // The camera keeps the framing the cleared tracks were showing.
    expect(spatialState.reframe360.view.yaw).toBe(45)
  })
})

describe('track sampling', () => {
  it('matches the backend: linear between points, held outside them', () => {
    const track = [
      { t: 0, v: 0, interp: 'linear' as const },
      { t: 2, v: 90, interp: 'linear' as const },
    ]
    expect(sampleTrack(track, -1, 7)).toBe(0)
    expect(sampleTrack(track, 1, 7)).toBe(45)
    expect(sampleTrack(track, 5, 7)).toBe(90)
    expect(sampleTrack([], 1, 7)).toBe(7)
  })

  it('takes the interpolation from the first point of the track', () => {
    const held = [
      { t: 0, v: 0, interp: 'hold' as const },
      { t: 2, v: 90, interp: 'linear' as const },
    ]
    expect(sampleTrack(held, 1.9, 0)).toBe(0)

    const smooth = [
      { t: 0, v: 0, interp: 'smooth' as const },
      { t: 2, v: 100, interp: 'smooth' as const },
    ]
    expect(sampleTrack(smooth, 1, 0)).toBe(50)
    expect(sampleTrack(smooth, 0.5, 0)).toBeCloseTo(6.25, 6)
  })

  it('follows the animation as the playhead moves and flags an unsaved view', () => {
    setView({ yaw: 0 })
    setReframeKeyframe(0)
    setView({ yaw: 100 })
    setReframeKeyframe(2)

    syncViewToPlayhead(1)
    expect(spatialState.reframe360.view.yaw).toBe(50)
    expect(isViewOffKeyframes(1)).toBe(false)

    setView({ yaw: 80 })
    expect(isViewOffKeyframes(1)).toBe(true)
  })
})

describe('snapshot restore', () => {
  it('clamps a hostile snapshot into the ranges the wire accepts', () => {
    applySpatialSnapshot({
      reframe360: {
        enabled: true,
        inputProjection: 'evil',
        outputProjection: 'flat',
        view: { yaw: Number.NaN, pitch: 900, fov: Number.POSITIVE_INFINITY, roll: 'x' },
        yaw: [{ t: -5, v: 5000, interp: 'nope' }],
      },
      stabilize: { mode: 'precise', smoothing: -3, zoom: 99 },
      lensCorrection: { k1: 12, k2: Number.NaN },
    })

    const state = spatialState.reframe360
    expect(state.inputProjection).toBe('equirect')
    expect(state.view).toEqual({ yaw: 0, pitch: 90, roll: 0, fov: 90 })
    expect(state.yaw).toEqual([{ t: 0, v: 360, interp: 'linear' }])
    expect(spatialPayload().stabilize).toEqual({
      mode: 'precise',
      smoothing: 1,
      zoom: 20,
      horizonLock: false,
    })
    expect(spatialPayload().lensCorrection).toEqual({ k1: 1, k2: 0 })
  })

  it('contributes nothing once it is reset', () => {
    setView({ yaw: 30 })
    expect(spatialPayload()).not.toEqual({})
    resetSpatial()
    expect(spatialPayload()).toEqual({})
  })
})
