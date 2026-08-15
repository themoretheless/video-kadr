import { describe, expect, it } from 'vitest'
import type {
  AudioTrack,
  Composition,
  CompositionClip,
  CompositionSource,
  VideoClip,
  VideoTrack,
} from './types'
import {
  CompositionRelinkError,
  compatibleCompositionRelinkSources,
  compositionRelinkRequirements,
  relinkCompositionSource,
} from './relink'

const second = 1_000_000
const oldVideo: CompositionSource = {
  id: 'missing-video',
  kind: 'video',
  durationTicks: 12 * second,
  width: 1920,
  height: 1080,
  hasAudio: true,
}
const replacement: CompositionSource = {
  id: 'replacement-video',
  kind: 'video',
  durationTicks: 20 * second,
  width: 3840,
  height: 2160,
  hasAudio: true,
}

function videoClip(id: string, sourceInTicks: number, sourceOutTicks: number, sourceAudioEnabled = true): VideoClip {
  return {
    id,
    kind: 'video',
    sourceId: oldVideo.id,
    timelineStartTicks: sourceInTicks,
    sourceInTicks,
    sourceOutTicks,
    transform: { x: 0, y: 0, width: 1920, height: 1080, fit: 'contain' },
    opacity: 1,
    sourceAudioEnabled,
    audioGain: 1,
  }
}

function project(): Composition {
  const videoTrack: VideoTrack = {
    id: 'video-track',
    kind: 'video',
    name: 'Video',
    locked: false,
    hidden: false,
    muted: false,
    clips: [videoClip('clip-a', 0, 4 * second), videoClip('clip-b', 5 * second, 11 * second)],
    transitions: [],
  }
  const audioTrack: AudioTrack = {
    id: 'audio-track',
    kind: 'audio',
    name: 'Audio',
    locked: false,
    muted: false,
    solo: false,
    clips: [{
      id: 'audio-from-video',
      kind: 'audio',
      sourceId: oldVideo.id,
      timelineStartTicks: 0,
      sourceInTicks: 2 * second,
      sourceOutTicks: 9 * second,
      gain: 1,
    }],
  }
  return {
    schemaVersion: 1,
    timeBase: second,
    canvas: { width: 1920, height: 1080, fps: 30, backgroundColor: '#000000' },
    sources: { [oldVideo.id]: oldVideo },
    tracks: [videoTrack, audioTrack],
  }
}

function allClips(composition: Composition): CompositionClip[] {
  const clips: CompositionClip[] = []
  for (const track of composition.tracks) clips.push(...track.clips)
  return clips
}

describe('composition source relink', () => {
  it('derives the longest used source range and audio requirement from clips', () => {
    expect(compositionRelinkRequirements(project(), oldVideo.id)).toEqual({
      sourceId: oldVideo.id,
      kind: 'video',
      minimumDurationTicks: 11 * second,
      requiresAudio: true,
      referencedClipIds: ['clip-a', 'clip-b', 'audio-from-video'],
    })
  })

  it('filters candidates by kind, duration and required audio', () => {
    const compatible = compatibleCompositionRelinkSources(project(), oldVideo.id, [
      replacement,
      { ...replacement, id: 'silent', hasAudio: false },
      { ...replacement, id: 'short', durationTicks: 10 * second },
      { id: 'audio', kind: 'audio', durationTicks: 20 * second, width: 0, height: 0, hasAudio: true },
    ])

    expect(compatible).toEqual([replacement])
  })

  it('atomically rewrites every reference and removes the missing registry entry', () => {
    const input = project()
    const snapshot = JSON.stringify(input)
    const result = relinkCompositionSource(input, oldVideo.id, replacement)

    expect(JSON.stringify(input)).toBe(snapshot)
    expect(result.sources).not.toHaveProperty(oldVideo.id)
    expect(result.sources[replacement.id]).toEqual(replacement)
    expect(allClips(result).filter((clip) => 'sourceId' in clip)).toHaveLength(3)
    expect(allClips(result).every(
      (clip) => !('sourceId' in clip) || clip.sourceId === replacement.id,
    )).toBe(true)
  })

  it('can merge references into an identical source that is already registered', () => {
    const input = { ...project(), sources: { [oldVideo.id]: oldVideo, [replacement.id]: replacement } }
    const result = relinkCompositionSource(input, oldVideo.id, replacement)

    expect(Object.keys(result.sources)).toEqual([replacement.id])
    expect(allClips(result).every(
      (clip) => !('sourceId' in clip) || clip.sourceId === replacement.id,
    )).toBe(true)
  })

  it('fails closed for silent, short and conflicting replacements', () => {
    expect(() => relinkCompositionSource(project(), oldVideo.id, { ...replacement, hasAudio: false }))
      .toThrow(expect.objectContaining<Partial<CompositionRelinkError>>({ code: 'missing-audio' }))
    expect(() => relinkCompositionSource(project(), oldVideo.id, { ...replacement, durationTicks: 10 * second }))
      .toThrow(expect.objectContaining<Partial<CompositionRelinkError>>({ code: 'source-too-short' }))
    const conflicting = {
      ...project(),
      sources: { [oldVideo.id]: oldVideo, [replacement.id]: { ...replacement, width: 1280 } },
    }
    expect(() => relinkCompositionSource(conflicting, oldVideo.id, replacement))
      .toThrow(expect.objectContaining<Partial<CompositionRelinkError>>({ code: 'conflicting-source' }))
  })
})
