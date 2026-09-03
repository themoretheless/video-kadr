import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, removeSilenceFromClip } from './commands'
import { COMPOSITION_TIME_BASE, type Composition, type VideoClip, type VideoTrack } from './types'

const s = (value: number) => value * COMPOSITION_TIME_BASE
const source = { id: 'source', kind: 'video' as const, durationTicks: s(12), width: 640, height: 360, hasAudio: true }

function clip(id: string, start: number, reverse = false): VideoClip {
  return {
    id, kind: 'video', sourceId: source.id, timelineStartTicks: s(start),
    sourceInTicks: 0, sourceOutTicks: s(8), speed: 1,
    transform: { x: 0, y: 0, width: 640, height: 360, fit: 'contain' },
    opacity: 1, sourceAudioEnabled: true, audioGain: 1,
    ...(reverse ? { playbackMode: { mode: 'reverse' as const } } : {}),
  }
}

function document(reverse = false): Composition {
  let result = createComposition({ width: 640, height: 360, fps: 30, backgroundColor: '#000000' }, { source })
  result = addTrack(result, { id: 'video', kind: 'video', name: 'Video', locked: false, hidden: false, muted: false, transitions: [], clips: [] })
  result = addClip(result, 'video', clip('target', 2, reverse))
  return addClip(result, 'video', { ...clip('later', 12), sourceOutTicks: s(2) })
}

describe('multitrack silence removal command', () => {
  it('slices audible source ranges, closes gaps, and ripples later clips', () => {
    const result = removeSilenceFromClip(document(), 'target', [
      { start: s(0), end: s(2) },
      { start: s(6), end: s(8) },
    ], ['audible-tail'])
    const clips = (result.tracks[0] as VideoTrack).clips
    expect(clips).toEqual([
      expect.objectContaining({ id: 'target', timelineStartTicks: s(2), sourceInTicks: 0, sourceOutTicks: s(2) }),
      expect.objectContaining({ id: 'audible-tail', timelineStartTicks: s(4), sourceInTicks: s(6), sourceOutTicks: s(8) }),
      expect.objectContaining({ id: 'later', timelineStartTicks: s(8) }),
    ])
  })

  it('keeps reverse playback order while packing source ranges', () => {
    const result = removeSilenceFromClip(document(true), 'target', [
      { start: s(0), end: s(2) },
      { start: s(6), end: s(8) },
    ], ['reverse-tail'])
    const clips = (result.tracks[0] as VideoTrack).clips
    expect(clips.slice(0, 2)).toEqual([
      expect.objectContaining({ id: 'target', timelineStartTicks: s(2), sourceInTicks: s(6), sourceOutTicks: s(8) }),
      expect.objectContaining({ id: 'reverse-tail', timelineStartTicks: s(4), sourceInTicks: 0, sourceOutTicks: s(2) }),
    ])
  })

  it('slices speed ramps through the canonical reciprocal timeline mapping', () => {
    const base = document()
    const track = base.tracks[0] as VideoTrack
    const ramped: Composition = {
      ...base,
      tracks: [{
        ...track,
        clips: track.clips.map((candidate) => candidate.id === 'target' ? {
          ...candidate,
          speedRamp: {
            interpolation: 'hold' as const,
            points: [
              { sourceProgressTick: 0, speed: 1 },
              { sourceProgressTick: s(4), speed: 2 },
              { sourceProgressTick: s(8), speed: 2 },
            ],
            audioPolicy: 'preserve_pitch' as const,
          },
        } : candidate),
      }],
    }
    const result = removeSilenceFromClip(ramped, 'target', [
      { start: 0, end: s(2) },
      { start: s(6), end: s(8) },
    ], ['ramp-tail'])
    const clips = (result.tracks[0] as VideoTrack).clips
    expect(clips[0]).toMatchObject({ id: 'target', timelineStartTicks: s(2), sourceOutTicks: s(2) })
    expect(clips[1]).toMatchObject({ id: 'ramp-tail', timelineStartTicks: s(4), sourceInTicks: s(6), sourceOutTicks: s(8) })
    expect(clips[2]).toMatchObject({ id: 'later', timelineStartTicks: s(9) })
  })

  it('fails closed when the clip owns a transition', () => {
    const base = document()
    const track = base.tracks[0] as VideoTrack
    const later = { ...track.clips.find((candidate) => candidate.id === 'later')!, timelineStartTicks: s(10), sourceInTicks: s(1), sourceOutTicks: s(3) }
    const transitioned: Composition = {
      ...base,
      tracks: [{ ...track, clips: [track.clips[0]!, later], transitions: [{ id: 'transition', fromClipId: 'target', toClipId: 'later', kind: 'dissolve', durationTicks: s(0.5) }] }],
    }
    expect(() => removeSilenceFromClip(transitioned, 'target', [{ start: 0, end: s(2) }], [])).toThrow(/transition/i)
  })
})
