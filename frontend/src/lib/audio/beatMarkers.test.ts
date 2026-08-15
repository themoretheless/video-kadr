import { describe, expect, it } from 'vitest'
import type { Composition } from '../composition/types'
import { mapDetectedBeatsToTimeline } from './beatMarkers'

const second = 1_000_000

function fixture(): Composition {
  return {
    schemaVersion: 1,
    timeBase: second,
    canvas: { width: 1280, height: 720, fps: 30, backgroundColor: '#000000' },
    sources: {
      audio: { id: 'audio', kind: 'audio', durationTicks: 20 * second, width: 0, height: 0, hasAudio: true },
      video: { id: 'video', kind: 'video', durationTicks: 20 * second, width: 1280, height: 720, hasAudio: true },
    },
    tracks: [
      {
        id: 'video-track', kind: 'video', name: 'Video', locked: false, hidden: false, muted: false,
        clips: [{
          id: 'video-clip', kind: 'video', sourceId: 'video', timelineStartTicks: 0,
          sourceInTicks: 2 * second, sourceOutTicks: 8 * second, speed: 1,
          transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' }, opacity: 1,
          sourceAudioEnabled: true, audioGain: 1,
        }],
      },
      {
        id: 'audio-track', kind: 'audio', name: 'Music', locked: false, muted: false, solo: false,
        clips: [{
          id: 'audio-clip', kind: 'audio', sourceId: 'audio', timelineStartTicks: 5 * second,
          sourceInTicks: second, sourceOutTicks: 9 * second, speed: 2, gain: 1,
        }],
      },
    ],
  }
}

describe('Auto Beat composition mapping', () => {
  it('maps full-source audio onsets through trim, placement and speed', () => {
    const beats = mapDetectedBeatsToTimeline(fixture(), 'audio-clip', {
      estimatedBpm: 120,
      beats: [
        { timeSeconds: 0.5, strength: 2 },
        { timeSeconds: 1, strength: 3 },
        { timeSeconds: 3, strength: 4 },
        { timeSeconds: 8.999, strength: 5 },
        { timeSeconds: 9, strength: 6 },
      ],
    })
    expect(beats).toEqual([
      { tick: 5 * second, strength: 3 },
      { tick: 6 * second, strength: 4 },
      { tick: 8_999_500, strength: 5 },
    ])
  })

  it('honors video source-audio and forward-playback gates', () => {
    expect(mapDetectedBeatsToTimeline(fixture(), 'video-clip', {
      estimatedBpm: null,
      beats: [{ timeSeconds: 3, strength: 2 }],
    })).toEqual([{ tick: second, strength: 2 }])

    const composition = fixture()
    const track = composition.tracks[0]!
    if (track.kind !== 'video') throw new Error('fixture')
    const reverse: Composition = {
      ...composition,
      tracks: [{ ...track, clips: [{ ...track.clips[0]!, playbackMode: { mode: 'reverse' } }] }, composition.tracks[1]!],
    }
    expect(() => mapDetectedBeatsToTimeline(reverse, 'video-clip', { estimatedBpm: null, beats: [] })).toThrow('forward')
  })

  it('fails closed for muted or unprobed audio', () => {
    const composition = fixture()
    const track = composition.tracks[1]!
    if (track.kind !== 'audio') throw new Error('fixture')
    expect(() => mapDetectedBeatsToTimeline({
      ...composition,
      tracks: [composition.tracks[0]!, { ...track, muted: true }],
    }, 'audio-clip', { estimatedBpm: null, beats: [] })).toThrow('выключена')

    expect(() => mapDetectedBeatsToTimeline({
      ...composition,
      sources: { ...composition.sources, audio: { ...composition.sources.audio!, hasAudio: false } },
    }, 'audio-clip', { estimatedBpm: null, beats: [] })).toThrow('не содержит подтверждённого audio')

    const videoTrack = composition.tracks[0]!
    if (videoTrack.kind !== 'video') throw new Error('fixture')
    expect(() => mapDetectedBeatsToTimeline({
      ...composition,
      tracks: [{ ...videoTrack, clips: [{ ...videoTrack.clips[0]!, sourceAudioEnabled: false }] }, track],
    }, 'video-clip', { estimatedBpm: null, beats: [] })).toThrow('Source audio выключен')
  })
})
