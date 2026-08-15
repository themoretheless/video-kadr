import { describe, expect, it } from 'vitest'
import {
  addClip,
  addTrack,
  createComposition,
  duplicateClip,
  registerSource,
  splitClip,
  trimClip,
} from './commands'
import { buildCompositionRenderRequest } from './payload'
import { relinkCompositionSource } from './relink'
import {
  createCompositionTemplate,
  instantiateCompositionTemplate,
  parseCompositionTemplate,
  serializeCompositionTemplate,
} from './templates'
import { magnetizeTrack, rippleDeleteClip } from './timelineProductivity'
import {
  COMPOSITION_TIME_BASE,
  clipEndTicks,
  type Composition,
  type CompositionSource,
  type CompositionSpeedRamp,
  type VideoClip,
  type VideoTrack,
} from './types'
import { normalizeComposition, validateComposition } from './validation'

const second = COMPOSITION_TIME_BASE
const source: CompositionSource = {
  id: 'speed-source',
  kind: 'video',
  durationTicks: 20 * second,
  width: 1920,
  height: 1080,
  hasAudio: true,
}
const transform = { x: 0, y: 0, width: 1920, height: 1080, fit: 'contain' as const }

function clip(
  id: string,
  timelineStartTicks = 0,
  sourceInTicks = 0,
  sourceOutTicks = 8 * second,
): VideoClip {
  return {
    id,
    kind: 'video',
    sourceId: source.id,
    timelineStartTicks,
    sourceInTicks,
    sourceOutTicks,
    transform,
    opacity: 1,
    sourceAudioEnabled: true,
    audioGain: 1,
  }
}

function holdRamp(span = 8 * second): CompositionSpeedRamp {
  return {
    interpolation: 'hold',
    points: [
      { sourceProgressTick: 0, speed: 1 },
      { sourceProgressTick: span / 2, speed: 2 },
      { sourceProgressTick: span, speed: 2 },
    ],
    audioPolicy: 'preserve_pitch',
  }
}

function project(value: VideoClip): Composition {
  const track: VideoTrack = {
    id: 'primary-track',
    kind: 'video',
    name: 'Primary',
    locked: false,
    hidden: false,
    muted: false,
    clips: [value],
    transitions: [],
  }
  return {
    schemaVersion: 1,
    timeBase: second,
    canvas: { width: 1920, height: 1080, fps: 30, backgroundColor: '#000000' },
    sources: { [source.id]: source },
    tracks: [track],
  }
}

describe('speed ramp authoring integration', () => {
  it('migrates legacy source clips without inventing a ramp', () => {
    const legacy = structuredClone(project(clip('legacy'))) as unknown as {
      tracks: Array<{ clips: Array<Record<string, unknown>> }>
    }
    delete legacy.tracks[0]!.clips[0]!.speed
    const normalized = normalizeComposition(legacy)
    const migrated = normalized.tracks[0]!.clips[0]
    expect(migrated).toMatchObject({ speed: 1 })
    expect(migrated).not.toHaveProperty('speedRamp')
  })

  it('emits the exact canonical placement and omits the field for old clips', () => {
    const ramp = holdRamp()
    const ramped = project({ ...clip('ramped'), speed: 1, speedRamp: ramp })
    const placement = (buildCompositionRenderRequest(ramped).composition.tracks[0] as Extract<
      ReturnType<typeof buildCompositionRenderRequest>['composition']['tracks'][number],
      { kind: 'video' }
    >).clips[0]!.placement
    expect(placement).toEqual({
      timelineStartTick: 0,
      sourceInTick: 0,
      sourceOutTick: 8 * second,
      speed: 1,
      speedRamp: ramp,
    })

    const legacyPlacement = (buildCompositionRenderRequest(project(clip('legacy'))).composition.tracks[0] as Extract<
      ReturnType<typeof buildCompositionRenderRequest>['composition']['tracks'][number],
      { kind: 'video' }
    >).clips[0]!.placement
    expect(legacyPlacement).not.toHaveProperty('speedRamp')
  })

  it('trims and splits a reverse ramp in presentation order and deep-clones it', () => {
    const original = project({
      ...clip('reverse', 0, 2 * second, 10 * second),
      speed: 1,
      speedRamp: holdRamp(),
      playbackMode: { mode: 'reverse' },
    })
    const trimmed = trimClip(original, 'reverse', second, 5 * second)
    expect(trimmed.tracks[0]!.clips[0]).toMatchObject({
      sourceInTicks: 4 * second,
      sourceOutTicks: 9 * second,
      speedRamp: {
        points: [
          { sourceProgressTick: 0, speed: 1 },
          { sourceProgressTick: 3 * second, speed: 2 },
          { sourceProgressTick: 5 * second, speed: 2 },
        ],
      },
    })

    const split = splitClip(original, 'reverse', 4 * second, 'reverse-right')
    const [left, right] = split.tracks[0]!.clips as readonly VideoClip[]
    expect(left).toMatchObject({ sourceInTicks: 6 * second, sourceOutTicks: 10 * second })
    expect(right).toMatchObject({ timelineStartTicks: 4 * second, sourceInTicks: 2 * second, sourceOutTicks: 6 * second })
    expect(clipEndTicks(left!)).toBe(right!.timelineStartTicks)
    expect(clipEndTicks(right!)).toBe(6 * second)

    const duplicated = duplicateClip(original, 'reverse', { id: 'reverse-copy', timelineStartTicks: 7 * second })
    const copy = duplicated.tracks[0]!.clips[1] as VideoClip
    expect(copy.speedRamp).toEqual(holdRamp())
    expect(copy.speedRamp).not.toBe((original.tracks[0]!.clips[0] as VideoClip).speedRamp)
    expect(copy.speedRamp?.points).not.toBe((original.tracks[0]!.clips[0] as VideoClip).speedRamp?.points)
  })

  it('snaps cumulative rounding to a source boundary without changing the total or creating a seam', () => {
    const ramp: CompositionSpeedRamp = {
      interpolation: 'linear',
      points: [
        { sourceProgressTick: 0, speed: 0.9 },
        { sourceProgressTick: 2_069, speed: 1.1 },
      ],
    }
    const original = project({
      ...clip('quantized', 0, 0, 2_069),
      speed: 0.9,
      speedRamp: ramp,
    })
    const split = splitClip(original, 'quantized', 1_072, 'quantized-right')
    const [left, right] = split.tracks[0]!.clips as readonly VideoClip[]
    expect(clipEndTicks(left!)).toBe(right!.timelineStartTicks)
    expect(clipEndTicks(right!)).toBe(2_076)
    expect(left!.sourceOutTicks).toBe(right!.sourceInTicks)
    expect(Math.abs(clipEndTicks(left!) - 1_072)).toBeLessThanOrEqual(1)

    const trimmed = trimClip(original, 'quantized', 1_072, 2_076)
    expect(trimmed.tracks[0]!.clips[0]).toMatchObject({ timelineStartTicks: 1_073, sourceInTicks: 1_017 })
    expect(clipEndTicks(trimmed.tracks[0]!.clips[0]!)).toBe(2_076)
  })

  it('preserves ramp data through templates, ripple delete, magnet and relink', () => {
    let composition = createComposition({ width: 1920, height: 1080, fps: 30, backgroundColor: '#000000' })
    composition = registerSource(composition, source)
    composition = addTrack(composition, {
      id: 'timeline', kind: 'video', name: 'Timeline', locked: false, hidden: false, muted: false, clips: [], transitions: [],
    })
    composition = addClip(composition, 'timeline', clip('plain', 0, 0, second))
    composition = addClip(composition, 'timeline', {
      ...clip('ramped', 2 * second), speed: 1, speedRamp: holdRamp(),
    })
    const magnetized = magnetizeTrack(composition, 'timeline')
    const rampedAfterMagnet = magnetized.tracks[0]!.clips[1] as VideoClip
    expect(rampedAfterMagnet.timelineStartTicks).toBe(second)
    expect(rampedAfterMagnet.speedRamp).toEqual(holdRamp())
    const rippled = rippleDeleteClip(magnetized, 'plain')
    expect(rippled.tracks[0]!.clips[0]).toMatchObject({ id: 'ramped', timelineStartTicks: 0, speedRamp: holdRamp() })

    const template = createCompositionTemplate('speed-template', 'Speed ramp', rippled, [])
    const instantiated = instantiateCompositionTemplate(parseCompositionTemplate(serializeCompositionTemplate(template)), {})
    expect(instantiated.tracks[0]!.clips[0]).toMatchObject({ speedRamp: holdRamp() })

    const silentReplacement: CompositionSource = { ...source, id: 'silent-replacement', hasAudio: false }
    const muted = {
      ...rippled,
      tracks: [{
        ...(rippled.tracks[0] as VideoTrack),
        clips: [{
          ...(rippled.tracks[0]!.clips[0] as VideoClip),
          speedRamp: { ...holdRamp(), audioPolicy: 'mute' as const },
        }],
      }],
    }
    expect(relinkCompositionSource(muted, source.id, silentReplacement).tracks[0]!.clips[0])
      .toMatchObject({ sourceId: silentReplacement.id, speedRamp: { audioPolicy: 'mute' } })
  })

  it('fails closed above the 256 active segment budget', () => {
    const points = Array.from({ length: 32 }, (_, index) => ({ sourceProgressTick: index * 1_000, speed: 1 }))
    const clips = Array.from({ length: 9 }, (_, index): VideoClip => ({
      ...clip(`budget-${index}`, index * 31_000, 0, 31_000),
      speed: 1,
      speedRamp: { interpolation: 'hold', points },
    }))
    const composition = project(clips[0]!)
    const overBudget: Composition = {
      ...composition,
      tracks: [{ ...(composition.tracks[0] as VideoTrack), clips }],
    }
    expect(validateComposition(overBudget).map((issue) => issue.code)).toContain('speed-ramp-segment-budget')
  })
})
