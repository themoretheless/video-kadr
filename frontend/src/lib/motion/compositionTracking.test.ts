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

  it('tracks the nominal clip interval when the source clip owns transition handles', () => {
    const base = fixture()
    const primary = base.tracks[1]!
    if (primary.kind !== 'video') throw new Error('fixture')
    const composition: Composition = {
      ...base,
      tracks: [base.tracks[0]!, {
        ...primary,
        transitions: [{
          id: 'source-transition',
          fromClipId: 'source-clip',
          toClipId: 'next-clip',
          kind: 'dissolve',
          durationTicks: 200_000,
        }],
      }],
    }

    const plan = buildCompositionPointTrackingPlan(composition, 'source-clip', 'target', {
      startTicks: 1_000_000,
      endTicks: 1_300_000,
      sampleFps: 10,
    })

    expect(plan.sampleTimelineTicks).toEqual([1_000_000, 1_100_000, 1_200_000])
    expect(plan.sampleSourceSeconds).toEqual([3, 3.1, 3.2])
  })

  it('maps tracking deltas through an overlay video transform and rotation', () => {
    const base = fixture()
    const sourceTrack = {
      id: 'source-overlay-track', kind: 'video' as const, name: 'Source overlay', locked: false, hidden: false, muted: true,
      transitions: [],
      clips: [{
        id: 'source-overlay', kind: 'video' as const, sourceId: 'source', timelineStartTicks: 0,
        sourceInTicks: 0, sourceOutTicks: 2_000_000, speed: 1,
        transform: { x: 100, y: 0, width: 640, height: 360, fit: 'contain' as const },
        rotationDegrees: 90, opacity: 1, sourceAudioEnabled: false, audioGain: 1,
      }],
    }
    const composition: Composition = { ...base, tracks: [base.tracks[0]!, sourceTrack, base.tracks[1]!] }
    const plan = buildCompositionPointTrackingPlan(composition, 'source-overlay', 'target', {
      startTicks: 1_000_000, endTicks: 1_200_000, sampleFps: 10,
    })
    const animation = pointTrackToTargetAnimation(plan, {
      status: 'completed',
      points: [
        { frameIndex: 0, x: 640, y: 360, confidence: 1 },
        { frameIndex: 1, x: 650, y: 360, confidence: 1 },
      ],
    })

    expect(animation.x).toMatchObject({
      mode: 'keyframes', track: { keyframes: [{ tick: 0, value: 10 }, { tick: 100_000, value: 10 }] },
    })
    expect(animation.y).toMatchObject({
      mode: 'keyframes', track: { keyframes: [{ tick: 0, value: -20 }, { tick: 100_000, value: -15 }] },
    })
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

  it('maps reverse and speed-ramp playback into source sampling order', () => {
    const reverse = fixture()
    const primary = reverse.tracks[1]!
    if (primary.kind !== 'video') throw new Error('fixture')
    const clip = primary.clips[0]!
    const changed: Composition = {
      ...reverse,
      tracks: [reverse.tracks[0]!, { ...primary, clips: [{ ...clip, playbackMode: { mode: 'reverse' } }] }],
    }
    const reversePlan = buildCompositionPointTrackingPlan(changed, 'source-clip', 'target', {
      startTicks: 1_000_000, endTicks: 1_300_000, sampleFps: 10,
    })
    expect(reversePlan.sampleSourceSeconds).toEqual([4.999999, 4.899999, 4.799999])

    const ramped: Composition = {
      ...reverse,
      tracks: [reverse.tracks[0]!, {
        ...primary,
        clips: [{
          ...clip,
          speedRamp: {
            interpolation: 'linear',
            points: [
              { sourceProgressTick: 0, speed: 1 },
              { sourceProgressTick: 2_000_000, speed: 2 },
              { sourceProgressTick: 4_000_000, speed: 2 },
            ],
            audioPolicy: 'preserve_pitch',
          },
        }],
      }],
    }
    const rampPlan = buildCompositionPointTrackingPlan(ramped, 'source-clip', 'target', {
      startTicks: 1_000_000, endTicks: 1_300_000, sampleFps: 10,
    })
    expect(rampPlan.sampleSourceSeconds).toEqual([3.297443, 3.466507, 3.644238])
  })

  it('fails closed for freeze/stabilization domains and lost tracks', () => {
    const frozen = fixture()
    const primary = frozen.tracks[1]!
    if (primary.kind !== 'video') throw new Error('fixture')
    const clip = primary.clips[0]!
    const changed: Composition = {
      ...frozen,
      tracks: [frozen.tracks[0]!, { ...primary, clips: [{ ...clip, playbackMode: { mode: 'freeze', sourceTick: 3_000_000 } }] }],
    }
    expect(() => buildCompositionPointTrackingPlan(changed, 'source-clip', 'target')).toThrow('Freeze')

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
