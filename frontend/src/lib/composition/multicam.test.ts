import { describe, expect, it } from 'vitest'

import { addTrack, createComposition, registerSource } from './commands'
import {
  compileMulticamCuts,
  recordMulticamSwitch,
  sourceTickAtGroupStartFromOffset,
  type MulticamGroup,
} from './multicam'
import { COMPOSITION_TIME_BASE, type Composition } from './types'

const second = COMPOSITION_TIME_BASE

function document(): Composition {
  let composition = createComposition({ width: 1920, height: 1080, fps: 30, backgroundColor: '#000000' })
  for (const [id, duration] of [['cam-a', 12], ['cam-b', 13], ['cam-c', 14]] as const) {
    composition = registerSource(composition, {
      id,
      kind: 'video',
      durationTicks: duration * second,
      width: 1920,
      height: 1080,
      hasAudio: true,
    })
  }
  composition = addTrack(composition, {
    id: 'program-video',
    kind: 'video',
    name: 'Multicam program',
    locked: false,
    hidden: false,
    muted: true,
    transitions: [],
    clips: [],
  })
  return addTrack(composition, {
    id: 'master-audio',
    kind: 'audio',
    name: 'Multicam master',
    locked: false,
    muted: false,
    solo: false,
    clips: [],
  })
}

function group(): MulticamGroup {
  return {
    id: 'concert',
    name: 'Concert multicam',
    timelineStartTicks: 2 * second,
    durationTicks: 6 * second,
    videoTrackId: 'program-video',
    audioTrackId: 'master-audio',
    audioClipId: 'concert-master-audio',
    audioAngleId: 'angle-a',
    angles: [
      { id: 'angle-a', label: 'Wide', sourceId: 'cam-a', sourceTickAtGroupStart: second },
      { id: 'angle-b', label: 'Close', sourceId: 'cam-b', sourceTickAtGroupStart: 2 * second },
      { id: 'angle-c', label: 'Side', sourceId: 'cam-c', sourceTickAtGroupStart: 3 * second },
    ],
    switches: [
      { id: 'switch-a', clipId: 'cut-a', timelineTick: 2 * second, angleId: 'angle-a' },
      { id: 'switch-b', clipId: 'cut-b', timelineTick: 4 * second, angleId: 'angle-b' },
      { id: 'switch-c', clipId: 'cut-c', timelineTick: 6 * second, angleId: 'angle-c' },
    ],
  }
}

describe('multicam cut compiler', () => {
  it('converts live angle switches to ordinary gapless video cuts and one master audio clip', () => {
    const compiled = compileMulticamCuts(document(), group())

    expect(compiled.videoClips.map((clip) => ({
      id: clip.id,
      sourceId: clip.sourceId,
      timelineStartTicks: clip.timelineStartTicks,
      sourceInTicks: clip.sourceInTicks,
      sourceOutTicks: clip.sourceOutTicks,
      sourceAudioEnabled: clip.sourceAudioEnabled,
    }))).toEqual([
      { id: 'cut-a', sourceId: 'cam-a', timelineStartTicks: 2 * second, sourceInTicks: second, sourceOutTicks: 3 * second, sourceAudioEnabled: false },
      { id: 'cut-b', sourceId: 'cam-b', timelineStartTicks: 4 * second, sourceInTicks: 4 * second, sourceOutTicks: 6 * second, sourceAudioEnabled: false },
      { id: 'cut-c', sourceId: 'cam-c', timelineStartTicks: 6 * second, sourceInTicks: 7 * second, sourceOutTicks: 9 * second, sourceAudioEnabled: false },
    ])
    expect(compiled.audioClip).toMatchObject({
      id: 'concert-master-audio',
      sourceId: 'cam-a',
      timelineStartTicks: 2 * second,
      sourceInTicks: second,
      sourceOutTicks: 7 * second,
    })
  })

  it('records a deterministic switch at a new time and replaces an existing boundary', () => {
    const original = group()
    const inserted = recordMulticamSwitch(original, {
      id: 'switch-extra',
      clipId: 'cut-extra',
      timelineTick: 5 * second,
      angleId: 'angle-a',
    })
    expect(inserted.switches.map((change) => change.timelineTick)).toEqual([2, 4, 5, 6].map((value) => value * second))

    const replaced = recordMulticamSwitch(inserted, {
      id: 'switch-replace',
      clipId: 'cut-replace',
      timelineTick: 4 * second,
      angleId: 'angle-c',
    })
    expect(replaced.switches.find((change) => change.timelineTick === 4 * second)).toMatchObject({
      id: 'switch-replace',
      angleId: 'angle-c',
    })
  })

  it('uses the waveform offset sign convention and fails closed on insufficient source coverage', () => {
    expect(sourceTickAtGroupStartFromOffset(-0.2)).toBe(200_000)
    expect(() => sourceTickAtGroupStartFromOffset(0.2)).toThrow('Сдвиньте начало')
    expect(sourceTickAtGroupStartFromOffset(0.2, COMPOSITION_TIME_BASE, 0.2)).toBe(0)
    const original = group()
    const invalid: MulticamGroup = {
      ...original,
      angles: original.angles.map((angle, index) => index === 2
        ? { ...angle, sourceTickAtGroupStart: 13 * second }
        : angle),
    }
    expect(() => compileMulticamCuts(document(), invalid)).toThrow('does not cover')
  })
})
