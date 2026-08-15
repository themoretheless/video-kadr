import { describe, expect, it } from 'vitest'
import type { Composition } from '../composition/types'
import { buildCompositionPointTrackingPlan, pointTrackToTargetAnimation } from './compositionTracking'

function fixture(): Composition {
  return {
    schemaVersion: 1,
    timeBase: 1_000_000,
    canvas: { width: 1_920, height: 1_080, fps: 30, backgroundColor: '#000000' },
    sources: {
      source: { id: 'source', kind: 'video', durationTicks: 10_000_000, width: 1_280, height: 720, hasAudio: true },
      badge: { id: 'badge', kind: 'image', durationTicks: 0, width: 100, height: 100, hasAudio: false },
    },
    tracks: [
      {
        id: 'overlay-track', kind: 'image', name: 'Overlay', locked: false, hidden: false,
        clips: [{
          id: 'target', kind: 'image', sourceId: 'badge', timelineStartTicks: 1_000_000,
          durationTicks: 3_000_000, transform: { x: 10, y: -20, width: 100, height: 100, fit: 'contain' }, opacity: 1,
        }],
      },
      {
        id: 'primary', kind: 'video', name: 'Primary', locked: false, hidden: false, muted: false,
        clips: [{
          id: 'source-clip', kind: 'video', sourceId: 'source', timelineStartTicks: 0,
          sourceInTicks: 2_000_000, sourceOutTicks: 6_000_000, speed: 1,
          transform: { x: 0, y: 0, width: 1_920, height: 1_080, fit: 'contain' }, opacity: 1,
          sourceAudioEnabled: true, audioGain: 1,
        }],
      },
    ],
  }
}

describe('composition point tracking bridge', () => {
  it('maps the common timeline range to source timestamps and canvas scale', () => {
    const plan = buildCompositionPointTrackingPlan(fixture(), 'source-clip', 'target', {
      startTicks: 1_000_000,
      endTicks: 1_300_000,
      sampleFps: 10,
    })
    expect(plan.sampleTimelineTicks).toEqual([1_000_000, 1_100_000, 1_200_000])
    expect(plan.sampleSourceSeconds).toEqual([3, 3.1, 3.2])
    expect(plan.sourcePixelToCanvasScale).toBe(1.5)
    expect(plan.targetLocalStartTicks).toBe(0)
    expect([plan.targetOriginX, plan.targetOriginY]).toEqual([10, -20])
  })

  it('converts tracked deltas into paired target-local position keyframes', () => {
    const plan = buildCompositionPointTrackingPlan(fixture(), 'source-clip', 'target', {
      startTicks: 1_000_000,
      endTicks: 1_300_000,
      sampleFps: 10,
    })
    const animation = pointTrackToTargetAnimation(plan, {
      status: 'completed',
      points: [
        { frameIndex: 0, x: 20, y: 20, confidence: 1 },
        { frameIndex: 1, x: 22, y: 19, confidence: 0.9 },
        { frameIndex: 2, x: 24, y: 18, confidence: 0.9 },
      ],
    }, { opacity: { mode: 'constant', value: 0.5 } }, 1.5, { x: 2, y: 2 })
    expect(animation.opacity).toEqual({ mode: 'constant', value: 0.5 })
    expect(animation.x).toMatchObject({
      mode: 'keyframes',
      track: { keyframes: [{ tick: 0, value: 10 }, { tick: 200_000, value: 22 }] },
    })
    expect(animation.y).toMatchObject({
      mode: 'keyframes',
      track: { keyframes: [{ tick: 0, value: -20 }, { tick: 200_000, value: -26 }] },
    })
  })

  it('fails closed for transformed temporal domains and lost tracks', () => {
    const reverse = fixture()
    const primary = reverse.tracks[1]!
    if (primary.kind !== 'video') throw new Error('fixture')
    const clip = primary.clips[0]!
    const changed: Composition = {
      ...reverse,
      tracks: [reverse.tracks[0]!, { ...primary, clips: [{ ...clip, playbackMode: { mode: 'reverse' } }] }],
    }
    expect(() => buildCompositionPointTrackingPlan(changed, 'source-clip', 'target')).toThrow('forward')

    const plan = buildCompositionPointTrackingPlan(fixture(), 'source-clip', 'target', {
      startTicks: 1_000_000, endTicks: 1_200_000, sampleFps: 10,
    })
    expect(() => pointTrackToTargetAnimation(plan, {
      status: 'lost', points: [{ frameIndex: 0, x: 1, y: 1, confidence: 1 }], lostAtFrame: 1,
    })).toThrow('потерял')
  })

  it('enforces overlap and the bounded sample budget', () => {
    expect(() => buildCompositionPointTrackingPlan(fixture(), 'source-clip', 'target', {
      startTicks: 0, endTicks: 1_100_000, sampleFps: 10,
    })).toThrow('пересечении')
    const long = fixture()
    const overlay = long.tracks[0]!
    const primary = long.tracks[1]!
    if (overlay.kind !== 'image' || primary.kind !== 'video') throw new Error('fixture')
    const expanded: Composition = {
      ...long,
      sources: {
        ...long.sources,
        source: { ...long.sources.source!, durationTicks: 30_000_000 },
      },
      tracks: [
        { ...overlay, clips: [{ ...overlay.clips[0]!, durationTicks: 20_000_000 }] },
        { ...primary, clips: [{ ...primary.clips[0]!, sourceOutTicks: 24_000_000 }] },
      ],
    }
    expect(() => buildCompositionPointTrackingPlan(expanded, 'source-clip', 'target', {
      startTicks: 1_000_000, endTicks: 12_000_000, sampleFps: 30,
    })).toThrow('300')
  })
})
