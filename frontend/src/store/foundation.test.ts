import { describe, it, expect, beforeEach } from 'vitest'
import { buildEditPayload, defaultEdit, hasMeaningfulChanges } from '../domain/edit'
import type { EditState, VideoInfo } from '../types'
import { applyAudioMixSnapshot, audioMixPayload, audioMixState, resetAudioMix } from './audioMix'
import {
  applyColorAdvancedSnapshot,
  colorAdvancedPayload,
  colorAdvancedState,
  resetColorAdvanced,
} from './colorAdvanced'
import {
  applyCompositionSnapshot,
  compositionPayload,
  compositionState,
  resetComposition,
} from './composition'
import { applyMotionSnapshot, motionPayload, motionState, resetMotion } from './motion'
import {
  applyFeatureModules,
  captureFeatureModules,
  featureModulePayload,
  featureModulesActive,
  resetFeatureModules,
} from './modules'
import { applyOverlaysSnapshot, overlaysPayload, overlaysState, resetOverlays } from './overlays'
import { applySpatialSnapshot, resetSpatial, spatialPayload, spatialState } from './spatial'
import { sanitizeKeyframeTrack, textOr } from './validation'

const VIDEO: VideoInfo = {
  id: 'vid',
  url: '/files/sources/vid.mp4',
  filename: 'vid.mp4',
  duration: 10,
  width: 1280,
  height: 720,
}

const ASSET_A = 'ast_abcdefghijklmnop'
const ASSET_B = 'ast_qrstuvwxyz012345'

function fullClipEdit(): EditState {
  const edit = defaultEdit()
  edit.trimEnd = VIDEO.duration
  edit.crop = { x: 0, y: 0, w: VIDEO.width, h: VIDEO.height }
  edit.scale = { w: VIDEO.width, h: -2 }
  return edit
}

beforeEach(() => resetFeatureModules())

describe('default project payload', () => {
  it('produces exactly the payload the app sent before the feature wave', () => {
    expect(buildEditPayload(fullClipEdit(), VIDEO, featureModulePayload())).toEqual({
      videoId: 'vid',
      mute: false,
      speed: 1,
    })
  })

  it('reports every module as inactive and contributing nothing', () => {
    expect(compositionPayload()).toEqual({})
    expect(overlaysPayload()).toEqual({})
    expect(audioMixPayload()).toEqual({})
    expect(motionPayload()).toEqual({})
    expect(spatialPayload()).toEqual({})
    expect(colorAdvancedPayload()).toEqual({})
    expect(featureModulesActive()).toBe(false)
    expect(hasMeaningfulChanges(fullClipEdit(), VIDEO, featureModulePayload())).toBe(false)
  })
})

describe('composition module', () => {
  it('serializes clips and drops a transition on the first clip', () => {
    applyCompositionSnapshot({
      clips: [
        { sourceId: 'vid-a', start: 0, end: 4, transitionIn: { kind: 'fade', duration: 1 } },
        { sourceId: 'vid-b', start: 2, end: 9, speed: 2, transitionIn: { kind: 'wipeleft', duration: 0.4 } },
      ],
    })

    expect(compositionPayload()).toEqual({
      clips: [
        { sourceId: 'vid-a', start: 0, end: 4 },
        {
          sourceId: 'vid-b',
          start: 2,
          end: 9,
          speed: 2,
          transitionIn: { kind: 'wipeleft', duration: 0.4 },
        },
      ],
    })
  })

  it('rejects clips without a usable range and unknown transition kinds', () => {
    applyCompositionSnapshot({
      clips: [
        { sourceId: 'vid-a', start: 5, end: 5 },
        { sourceId: '', start: 0, end: 3 },
        { sourceId: 'vid-b', start: 0, end: Number.POSITIVE_INFINITY },
      ],
      segmentTransition: { kind: 'evil; rm -rf', duration: 1 },
    })

    expect(compositionState.clips).toEqual([])
    expect(compositionState.segmentTransition).toBeNull()
    expect(compositionPayload()).toEqual({})
  })

  it('clamps speed and transition duration into the contract range', () => {
    applyCompositionSnapshot({
      clips: [
        { sourceId: 'a', start: 0, end: 1 },
        { sourceId: 'b', start: 0, end: 1, speed: 99, transitionIn: { kind: 'fade', duration: 99 } },
      ],
    })

    const clips = compositionPayload().clips as Record<string, unknown>[]
    expect(clips[1].speed).toBe(4)
    expect(clips[1].transitionIn).toEqual({ kind: 'fade', duration: 3 })
  })

  it('resets back to the inactive state', () => {
    applyCompositionSnapshot({ segmentTransition: { kind: 'fade', duration: 0.5 } })
    expect(compositionPayload()).not.toEqual({})
    resetComposition()
    expect(compositionPayload()).toEqual({})
  })
})

describe('overlays module', () => {
  it('keeps only assets with a valid id and emits non-default fields', () => {
    applyOverlaysSnapshot({
      overlays: [
        { assetId: ASSET_A, kind: 'image', x: 0.05, y: 0.05, width: 0.25, opacity: 0.5 },
        { assetId: '../../etc/passwd', kind: 'image', x: 0, y: 0, width: 1 },
      ],
    })

    expect(overlaysPayload()).toEqual({
      overlays: [
        { assetId: ASSET_A, kind: 'image', x: 0.05, y: 0.05, width: 0.25, height: null, opacity: 0.5 },
      ],
    })
  })

  it('drops overlay audio for image overlays', () => {
    applyOverlaysSnapshot({
      overlays: [
        {
          assetId: ASSET_A,
          kind: 'image',
          x: 0,
          y: 0,
          width: 0.5,
          audio: { enabled: true, volume: 2 },
        },
      ],
    })

    expect(overlaysState.overlays[0]?.audio).toBeNull()
    expect(overlaysPayload().overlays).toEqual([
      { assetId: ASSET_A, kind: 'image', x: 0, y: 0, width: 0.5, height: null },
    ])
  })

  it('caps title text at 512 characters and rejects blank titles', () => {
    applyOverlaysSnapshot({
      titles: [
        { text: 'x'.repeat(900), fontSize: 48, color: '#ffffff', x: 0.5, y: 0.85, align: 'center' },
        { text: '   ', fontSize: 48 },
      ],
    })

    expect(overlaysState.titles).toHaveLength(1)
    expect(overlaysState.titles[0]?.text).toHaveLength(512)
    expect(overlaysState.titles[0]?.color).toBe('#FFFFFF')
  })

  it('rejects a subtitle block with no asset', () => {
    applyOverlaysSnapshot({ subtitles: { burnIn: true, fontSize: 24 } })
    expect(overlaysState.subtitles).toBeNull()
    expect(overlaysPayload()).toEqual({})
  })

  it('resets back to the inactive state', () => {
    applyOverlaysSnapshot({
      overlays: [{ assetId: ASSET_A, kind: 'video', x: 0, y: 0, width: 0.4 }],
    })
    expect(overlaysPayload()).not.toEqual({})
    resetOverlays()
    expect(overlaysPayload()).toEqual({})
  })
})

describe('audio mix module', () => {
  it('emits tracks and only the dynamics that leave the neutral chain', () => {
    applyAudioMixSnapshot({
      tracks: [{ assetId: ASSET_B, role: 'music', gain: 0.4, start: 2 }],
      dynamics: { denoise: 0.3, bitrateKbps: 192 },
    })

    expect(audioMixPayload()).toEqual({
      audioTracks: [{ assetId: ASSET_B, role: 'music', gain: 0.4, start: 2 }],
      audioDynamics: { denoise: 0.3, bitrateKbps: 192 },
    })
  })

  it('clamps the bitrate and drops it again at the default', () => {
    applyAudioMixSnapshot({ dynamics: { bitrateKbps: 5000 } })
    expect(audioMixPayload().audioDynamics).toEqual({ bitrateKbps: 320 })

    applyAudioMixSnapshot({ dynamics: { bitrateKbps: 128 } })
    expect(audioMixPayload()).toEqual({})
  })

  it('sorts, dedupes and caps the volume envelope', () => {
    applyAudioMixSnapshot({
      dynamics: {
        volumeEnvelope: [
          { t: 3, v: 0.5, interp: 'smooth' },
          { t: 1, v: 9 },
          { t: 1, v: 0.25, interp: 'hold' },
          { t: Number.NaN, v: 1 },
        ],
      },
    })

    expect(audioMixState.dynamics.volumeEnvelope).toEqual([
      { t: 1, v: 0.25, interp: 'hold' },
      { t: 3, v: 0.5, interp: 'smooth' },
    ])
  })

  it('resets back to the inactive state', () => {
    applyAudioMixSnapshot({ dynamics: { deesser: true } })
    expect(audioMixPayload()).not.toEqual({})
    resetAudioMix()
    expect(audioMixPayload()).toEqual({})
  })
})

describe('motion module', () => {
  it('nests the transform tracks and keeps speed ramps as a sibling', () => {
    applyMotionSnapshot({
      motion: { zoom: [{ t: 0, v: 1, interp: 'linear' }, { t: 4, v: 1.4, interp: 'smooth' }] },
      speedRamps: [{ t: 0, v: 8 }],
    })

    expect(motionPayload()).toEqual({
      motion: {
        zoom: [
          { t: 0, v: 1, interp: 'linear' },
          { t: 4, v: 1.4, interp: 'smooth' },
        ],
      },
      speedRamps: [{ t: 0, v: 4, interp: 'linear' }],
    })
  })

  it('accepts the flat state shape as well as the wire shape', () => {
    applyMotionSnapshot({ panX: [{ t: 1, v: -5 }] })
    expect(motionState.panX).toEqual([{ t: 1, v: -1, interp: 'linear' }])
  })

  it('resets back to the inactive state', () => {
    applyMotionSnapshot({ speedRamps: [{ t: 0, v: 2 }] })
    expect(motionPayload()).not.toEqual({})
    resetMotion()
    expect(motionPayload()).toEqual({})
  })
})

describe('spatial module', () => {
  it('emits reframe360 only while it is enabled', () => {
    applySpatialSnapshot({ reframe360: { enabled: false, outputWidth: 3840, outputHeight: 2160 } })
    expect(spatialPayload()).toEqual({})

    spatialState.reframe360.enabled = true
    expect(spatialPayload().reframe360).toEqual({
      inputProjection: 'equirect',
      outputProjection: 'flat',
      outputWidth: 3840,
      outputHeight: 2160,
      horizonLock: true,
    })
  })

  it('rounds an odd output size down to an even one', () => {
    applySpatialSnapshot({ reframe360: { enabled: true, outputWidth: 1921, outputHeight: 1081 } })
    expect(spatialState.reframe360.outputWidth).toBe(1920)
    expect(spatialState.reframe360.outputHeight).toBe(1080)
  })

  it('emits stabilize only when it is not off and clamps smoothing', () => {
    applySpatialSnapshot({ stabilize: { mode: 'precise', smoothing: 900 } })
    expect(spatialPayload().stabilize).toEqual({
      mode: 'precise',
      smoothing: 100,
      zoom: 0,
      horizonLock: false,
    })
  })

  it('emits lens correction only when a coefficient is non-zero', () => {
    applySpatialSnapshot({ lensCorrection: { k1: 0, k2: 0 } })
    expect(spatialPayload()).toEqual({})
    applySpatialSnapshot({ lensCorrection: { k1: -0.2, k2: 0 } })
    expect(spatialPayload().lensCorrection).toEqual({ k1: -0.2, k2: 0 })
  })

  it('resets back to the inactive state', () => {
    applySpatialSnapshot({ stabilize: { mode: 'fast' } })
    expect(spatialPayload()).not.toEqual({})
    resetSpatial()
    expect(spatialPayload()).toEqual({})
  })
})

describe('advanced colour module', () => {
  it('emits only the wheels and bands that left neutral', () => {
    applyColorAdvancedSnapshot({
      exposure: 0.5,
      gamma: { r: 1, g: 1, b: 1 },
      gain: { r: 1.2, g: 1, b: 1 },
      hsl: [
        { band: 'red', hue: 10, saturation: 1, luminance: 1 },
        { band: 'blue', hue: 0, saturation: 1, luminance: 1 },
      ],
    })

    expect(colorAdvancedPayload()).toEqual({
      colorAdvanced: {
        exposure: 0.5,
        gain: { r: 1.2, g: 1, b: 1 },
        hsl: [{ band: 'red', hue: 10, saturation: 1, luminance: 1 }],
      },
    })
  })

  it('clamps out-of-range wheel values and drops unknown bands', () => {
    applyColorAdvancedSnapshot({
      lift: { r: 9, g: -9, b: 0 },
      gamma: { r: 0, g: 99, b: 1 },
      hsl: [{ band: 'ultraviolet', hue: 5, saturation: 1, luminance: 1 }],
    })

    expect(colorAdvancedState.lift).toEqual({ r: 0.5, g: -0.5, b: 0 })
    expect(colorAdvancedState.gamma).toEqual({ r: 0.1, g: 4, b: 1 })
    expect(colorAdvancedState.hsl).toEqual([])
  })

  it('keeps one adjustment per band, last one wins', () => {
    applyColorAdvancedSnapshot({
      hsl: [
        { band: 'green', hue: 1, saturation: 1, luminance: 1 },
        { band: 'green', hue: 7, saturation: 1, luminance: 1 },
      ],
    })
    expect(colorAdvancedState.hsl).toEqual([
      { band: 'green', hue: 7, saturation: 1, luminance: 1 },
    ])
  })

  it('resets back to the inactive state', () => {
    applyColorAdvancedSnapshot({ temperature: 0.3 })
    expect(colorAdvancedPayload()).not.toEqual({})
    resetColorAdvanced()
    expect(colorAdvancedPayload()).toEqual({})
  })
})

describe('module aggregate', () => {
  it('round-trips a capture through apply', () => {
    applyColorAdvancedSnapshot({ exposure: 1 })
    applyMotionSnapshot({ speedRamps: [{ t: 0, v: 2 }] })
    const snapshot = captureFeatureModules()
    const payload = buildEditPayload(fullClipEdit(), VIDEO, featureModulePayload())

    resetFeatureModules()
    expect(buildEditPayload(fullClipEdit(), VIDEO, featureModulePayload())).toEqual({
      videoId: 'vid',
      mute: false,
      speed: 1,
    })

    applyFeatureModules(snapshot)
    expect(buildEditPayload(fullClipEdit(), VIDEO, featureModulePayload())).toEqual(payload)
    expect(featureModulesActive()).toBe(true)
    expect(hasMeaningfulChanges(fullClipEdit(), VIDEO, featureModulePayload())).toBe(true)
  })

  it('ignores a snapshot that is not an object', () => {
    applyFeatureModules('not a snapshot')
    expect(featureModulesActive()).toBe(false)
  })
})

describe('validation helpers', () => {
  it('strips control characters but keeps newlines', () => {
    expect(textOr('a\u0001b\nc', '')).toBe('ab\nc')
  })

  it('caps a keyframe track at 64 points', () => {
    const track = Array.from({ length: 200 }, (_, index) => ({ t: index, v: 1 }))
    expect(sanitizeKeyframeTrack(track, 0, 4)).toHaveLength(64)
  })
})
