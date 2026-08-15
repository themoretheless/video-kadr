import { describe, expect, it } from 'vitest'

import { addClip, addTrack, createComposition, registerSource } from './commands'
import { COMPOSITION_TIME_BASE, type Composition, type VideoClip } from './types'
import { magnetizeTrack, rippleDeleteClip } from './timelineProductivity'

const second = COMPOSITION_TIME_BASE

function clip(id: string, start: number, duration: number): VideoClip {
  return {
    id,
    kind: 'video',
    sourceId: 'source',
    timelineStartTicks: start,
    sourceInTicks: 0,
    sourceOutTicks: duration,
    speed: 1,
    transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' },
    opacity: 1,
    sourceAudioEnabled: true,
    audioGain: 1,
    audioPan: 0,
  }
}

function document(): Composition {
  let value = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  value = registerSource(value, {
    id: 'source',
    kind: 'video',
    durationTicks: 20 * second,
    width: 1280,
    height: 720,
    hasAudio: true,
  })
  value = addTrack(value, {
    id: 'main',
    kind: 'video',
    name: 'Main',
    locked: false,
    hidden: false,
    muted: false,
    transitions: [],
    clips: [],
  })
  value = addTrack(value, {
    id: 'overlay',
    kind: 'video',
    name: 'Overlay',
    locked: false,
    hidden: false,
    muted: true,
    transitions: [],
    clips: [],
  })
  value = addClip(value, 'main', clip('a', 0, 2 * second))
  value = addClip(value, 'main', clip('b', 3 * second, 2 * second))
  value = addClip(value, 'main', clip('c', 7 * second, second))
  return addClip(value, 'overlay', clip('overlay-clip', 5 * second, second))
}

describe('timeline productivity commands', () => {
  it('ripple-deletes only the selected track interval and preserves source timing', () => {
    const next = rippleDeleteClip(document(), 'b')
    const main = next.tracks.find((track) => track.id === 'main')!
    const overlay = next.tracks.find((track) => track.id === 'overlay')!

    expect(main.clips.map((item) => [item.id, item.timelineStartTicks])).toEqual([
      ['a', 0],
      ['c', 5 * second],
    ])
    expect(main.clips[1]).toMatchObject({ sourceInTicks: 0, sourceOutTicks: second })
    expect(overlay.clips[0]!.timelineStartTicks).toBe(5 * second)
  })

  it('magnetizes later clips without moving a clip that crosses the anchor', () => {
    const next = magnetizeTrack(document(), 'main', second)
    const main = next.tracks.find((track) => track.id === 'main')!

    expect(main.clips.map((item) => [item.id, item.timelineStartTicks])).toEqual([
      ['a', 0],
      ['b', 2 * second],
      ['c', 4 * second],
    ])
  })

  it('fails closed for locked tracks and invalid anchors', () => {
    const value = document()
    const locked = {
      ...value,
      tracks: value.tracks.map((track) => track.id === 'main' ? { ...track, locked: true } : track),
    } as Composition

    expect(() => rippleDeleteClip(locked, 'a')).toThrow('locked')
    expect(() => magnetizeTrack(value, 'main', -1)).toThrow('safe tick')
  })
})
