import { beforeEach, describe, expect, it } from 'vitest'
import {
  addAudioTrack,
  addEnvelopePoint,
  audioMixPayload,
  audioMixState,
  applyAudioMixSnapshot,
  defaultCompressor,
  defaultDucking,
  ENVELOPE_TIME_STEP,
  envelopeInterpolation,
  envelopeTime,
  moveEnvelopePoint,
  removeAudioTrack,
  removeEnvelopePoint,
  resetAudioMix,
  sampleEnvelope,
  withEnvelopeInterpolation,
} from './audioMix'
import { MAX_AUDIO_TRACKS } from './validation'
import type { KeyframeTrack } from '../types'

const MUSIC = 'ast_0123456789abcdef'
const VOICE = 'ast_fedcba9876543210'

beforeEach(() => resetAudioMix())

describe('track list', () => {
  it('adds a neutral track that contributes only its id and role', () => {
    expect(addAudioTrack(MUSIC, 'sfx')).toBe(true)
    expect(audioMixPayload()).toEqual({ audioTracks: [{ assetId: MUSIC, role: 'sfx' }] })
  })

  it('gives a music track ducking by default and keeps it on the wire', () => {
    addAudioTrack(MUSIC, 'music')
    const payload = audioMixPayload() as { audioTracks: Record<string, unknown>[] }
    expect(payload.audioTracks[0]!.ducking).toEqual(defaultDucking())
  })

  it('refuses an id that is not an asset reference and a full list', () => {
    expect(addAudioTrack('../../etc/passwd')).toBe(false)
    expect(addAudioTrack(null)).toBe(false)
    for (let index = 0; index < MAX_AUDIO_TRACKS; index += 1) addAudioTrack(MUSIC, 'sfx')
    expect(audioMixState.tracks).toHaveLength(MAX_AUDIO_TRACKS)
    expect(addAudioTrack(VOICE, 'voiceover')).toBe(false)
  })

  it('removes only the requested index and ignores an out of range one', () => {
    addAudioTrack(MUSIC, 'music')
    addAudioTrack(VOICE, 'voiceover')
    removeAudioTrack(5)
    expect(audioMixState.tracks).toHaveLength(2)
    removeAudioTrack(0)
    expect(audioMixState.tracks.map((track) => track.assetId)).toEqual([VOICE])
  })

  it('clamps a hostile snapshot before the payload is built', () => {
    applyAudioMixSnapshot({
      tracks: [{ assetId: MUSIC, role: 'music', gain: 99, start: Number.NaN, fadeIn: -4 }],
      dynamics: { denoise: 5, bitrateKbps: 4000, highpassHz: Number.POSITIVE_INFINITY },
    })
    expect(audioMixPayload()).toEqual({
      audioTracks: [{ assetId: MUSIC, role: 'music', gain: 4 }],
      audioDynamics: { denoise: 1, bitrateKbps: 320 },
    })
  })
})

describe('voice cleanup payload', () => {
  it('sends nothing while the chain is neutral', () => {
    expect(audioMixPayload()).toEqual({})
  })

  it('sends only the blocks the user switched on', () => {
    audioMixState.dynamics.compressor = defaultCompressor()
    audioMixState.dynamics.deesser = true
    audioMixState.dynamics.lowpassHz = 12000
    expect(audioMixPayload()).toEqual({
      audioDynamics: {
        compressor: defaultCompressor(),
        deesser: true,
        lowpassHz: 12000,
      },
    })
  })

  it('keeps the default bitrate off the wire and a chosen one on it', () => {
    expect(audioMixPayload()).toEqual({})
    audioMixState.dynamics.bitrateKbps = 256
    expect(audioMixPayload()).toEqual({ audioDynamics: { bitrateKbps: 256 } })
  })
})

describe('volume envelope', () => {
  function track(): KeyframeTrack {
    return [
      { t: 0, v: 1, interp: 'linear' },
      { t: 4, v: 0.25, interp: 'linear' },
    ]
  }

  it('quantizes times to the millisecond tick the backend uses', () => {
    expect(envelopeTime(1.23456, 10)).toBe(1.235)
    expect(envelopeTime(-3, 10)).toBe(0)
    expect(envelopeTime(99, 10)).toBe(10)
    expect(envelopeTime('x', 10)).toBe(0)
  })

  it('inserts sorted points and overwrites a point on the same tick', () => {
    const added = addEnvelopePoint(track(), 2, 0.5, 10)
    expect(added.map((point) => point.t)).toEqual([0, 2, 4])
    const replaced = addEnvelopePoint(added, 2.0004, 0.9, 10)
    expect(replaced).toHaveLength(3)
    expect(replaced[1]).toEqual({ t: 2, v: 0.9, interp: 'linear' })
  })

  it('never lets a drag collapse two points onto the same tick', () => {
    const points = addEnvelopePoint(track(), 2, 0.5, 10)
    const dragged = moveEnvelopePoint(points, 1, 4, 0.5, 10)
    expect(dragged[1]!.t).toBe(4 - ENVELOPE_TIME_STEP)
    expect(new Set(dragged.map((point) => point.t)).size).toBe(dragged.length)
  })

  it('clamps a dragged value into the 0..4 gain range', () => {
    expect(moveEnvelopePoint(track(), 0, 0, 99, 10)[0]!.v).toBe(4)
    expect(moveEnvelopePoint(track(), 0, 0, -1, 10)[0]!.v).toBe(0)
  })

  it('applies one interpolation to the whole track, as the backend reads it', () => {
    const smooth = withEnvelopeInterpolation(track(), 'smooth')
    expect(smooth.every((point) => point.interp === 'smooth')).toBe(true)
    expect(envelopeInterpolation(smooth)).toBe('smooth')
    expect(envelopeInterpolation([])).toBe('linear')
  })

  it('samples like the backend: holds at both ends, eases in between', () => {
    expect(sampleEnvelope([], 3)).toBe(1)
    expect(sampleEnvelope(track(), -5)).toBe(1)
    expect(sampleEnvelope(track(), 99)).toBe(0.25)
    expect(sampleEnvelope(track(), 2)).toBeCloseTo(0.625, 6)
    expect(sampleEnvelope(withEnvelopeInterpolation(track(), 'hold'), 3.9)).toBe(1)
    expect(sampleEnvelope(withEnvelopeInterpolation(track(), 'smooth'), 2)).toBeCloseTo(0.625, 6)
    // progress 0.25 -> 4 * 0.25^3 = 0.0625 of the way from 1 down to 0.25.
    expect(sampleEnvelope(withEnvelopeInterpolation(track(), 'smooth'), 1)).toBeCloseTo(
      0.953125,
      6,
    )
  })

  it('removes a point and ignores a missing index', () => {
    expect(removeEnvelopePoint(track(), 9)).toHaveLength(2)
    expect(removeEnvelopePoint(track(), 0)).toEqual([{ t: 4, v: 0.25, interp: 'linear' }])
  })

  it('puts the envelope on the wire only when it has points', () => {
    audioMixState.dynamics.volumeEnvelope = track()
    expect(audioMixPayload()).toEqual({ audioDynamics: { volumeEnvelope: track() } })
    audioMixState.dynamics.volumeEnvelope = []
    expect(audioMixPayload()).toEqual({})
  })
})
