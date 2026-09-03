import { describe, expect, it } from 'vitest'
import { sourceClipTickAtTimelineTick, videoPlaybackIsForward, videoSourceTickAtTimelineTick } from './playback'
import { COMPOSITION_TIME_BASE, type AudioClip, type VideoClip } from './types'

const seconds = (value: number): number => value * COMPOSITION_TIME_BASE

function clip(playbackMode?: VideoClip['playbackMode']): VideoClip {
  return {
    id: 'playback-clip',
    kind: 'video',
    sourceId: 'source-video',
    timelineStartTicks: seconds(10),
    sourceInTicks: seconds(2),
    sourceOutTicks: seconds(6),
    speed: 1,
    transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' },
    opacity: 1,
    sourceAudioEnabled: true,
    audioGain: 1,
    ...(playbackMode ? { playbackMode } : {}),
  }
}

describe('composition playback mapping', () => {
  it('migrates missing mode logically to forward and clamps to the half-open source range', () => {
    const forward = clip()
    expect(videoPlaybackIsForward(forward)).toBe(true)
    expect(videoSourceTickAtTimelineTick(forward, seconds(11))).toBe(seconds(3))
    expect(videoSourceTickAtTimelineTick(forward, seconds(20))).toBe(seconds(6) - 1)
  })

  it('maps reverse time backwards and holds an exact freeze tick', () => {
    const reverse = clip({ mode: 'reverse' })
    expect(videoPlaybackIsForward(reverse)).toBe(false)
    expect(videoSourceTickAtTimelineTick(reverse, seconds(10))).toBe(seconds(6) - 1)
    expect(videoSourceTickAtTimelineTick(reverse, seconds(11))).toBe(seconds(5) - 1)

    const frozen = clip({ mode: 'freeze', sourceTick: seconds(4) })
    expect(videoSourceTickAtTimelineTick(frozen, seconds(10))).toBe(seconds(4))
    expect(videoSourceTickAtTimelineTick(frozen, seconds(13))).toBe(seconds(4))
  })

  it('maps an independent reversed audio clip over the same half-open source range', () => {
    const audio: AudioClip = {
      id: 'audio', kind: 'audio', sourceId: 'source-video', timelineStartTicks: seconds(10),
      sourceInTicks: seconds(2), sourceOutTicks: seconds(6), speed: 1, gain: 1, reversed: true,
    }
    expect(sourceClipTickAtTimelineTick(audio, seconds(10))).toBe(seconds(6) - 1)
    expect(sourceClipTickAtTimelineTick(audio, seconds(11))).toBe(seconds(5) - 1)
  })
})
