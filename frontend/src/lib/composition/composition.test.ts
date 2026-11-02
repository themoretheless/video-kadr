import { compositionMarkers } from './markers'
import {sampleAnimatableValue} from './keyframes'
import { describe, expect, it } from 'vitest'
import {
  CompositionCommandError,
  addClip,
  addTrack,
  collectSnapTargets,
  createComposition,
  deleteClip,
  duplicateClip,
  moveClip,
  moveClipPlacements,
  registerSource,
  reorderTrack,
  snapClipStart,
  snapTick,
  slipClip,
  splitClip,
  trimClip,
  withClipGridStart,
  upsertTransition,
} from './commands'
import {
  COMPOSITION_RENDER_ENDPOINT,
  CompositionPayloadError,
  buildCompositionRenderRequest,
  buildCompositionWireDocument,
  normalizeCompositionRenderOutput,
  targetVideoBitrateKbps,
} from './payload'
import {
  COMPOSITION_SCHEMA_VERSION,
  COMPOSITION_TIME_BASE,
  COMPOSITION_DELIVERY_PROFILE_OPTIONS,
  MAX_COMPOSITION_CLIPS,
  MAX_COMPOSITION_DURATION_TICKS,
  MAX_COMPOSITION_SOURCES,
  MAX_COMPOSITION_TRACKS,
  MAX_ACTIVE_AUDIO_KEYFRAMES,
  MAX_ACTIVE_VISUAL_KEYFRAMES,
  MAX_KEYFRAMES_PER_VALUE,
  MAX_VISIBLE_VISUAL_TRACKS,
  type AudioClip,
  type AudioTrack,
  type Composition,
  type CompositionAnimatableValue,
  type CompositionClip,
  type CompositionRenderRequest,
  type CompositionSource,
  type CompositionTrack,
  type CompositionTransition,
  type CompositionVideoMask,
  type ImageClip,
  type ImageTrack,
  type TextClip,
  type TextTrack,
  type VideoClip,
  type VideoTrack,
  type WireCompositionTrack,
} from './types'
import {
  CompositionValidationError,
  compositionDurationTicks,
  compositionRenderUnavailableReason,
  compositionTransitionUnavailableReason,
  normalizeComposition,
  validateComposition,
} from './validation'

const seconds = (value: number): number => value * COMPOSITION_TIME_BASE

const videoSource: CompositionSource = {
  id: 'source-video',
  kind: 'video',
  durationTicks: seconds(20),
  width: 1920,
  height: 1080,
  hasAudio: true,
}

const silentVideoSource: CompositionSource = {
  id: 'source-silent',
  kind: 'video',
  durationTicks: seconds(20),
  width: 1280,
  height: 720,
  hasAudio: false,
}

const audioSource: CompositionSource = {
  id: 'source-audio',
  kind: 'audio',
  durationTicks: seconds(30),
  width: 0,
  height: 0,
  hasAudio: true,
}

const imageSource: CompositionSource = {
  id: 'source-image',
  kind: 'image',
  durationTicks: 0,
  width: 800,
  height: 600,
  hasAudio: false,
}

const canvas = { width: 1920, height: 1080, fps: 30, backgroundColor: '#000000' }
const transform = { x: 0, y: 0, width: 1920, height: 1080, fit: 'contain' as const }

function videoClip(id = 'video-clip', start = 0, sourceIn = 0, sourceOut = seconds(5)): VideoClip {
  return {
    id,
    kind: 'video',
    sourceId: videoSource.id,
    timelineStartTicks: start,
    sourceInTicks: sourceIn,
    sourceOutTicks: sourceOut,
    transform,
    opacity: 1,
    sourceAudioEnabled: true,
    audioGain: 1,
  }
}

function audioClip(id = 'audio-clip', start = 0, sourceIn = 0, sourceOut = seconds(5)): AudioClip {
  return {
    id,
    kind: 'audio',
    sourceId: audioSource.id,
    timelineStartTicks: start,
    sourceInTicks: sourceIn,
    sourceOutTicks: sourceOut,
    gain: 1,
  }
}

function imageClip(id = 'image-clip', start = seconds(1), duration = seconds(3)): ImageClip {
  return {
    id,
    kind: 'image',
    sourceId: imageSource.id,
    timelineStartTicks: start,
    durationTicks: duration,
    transform: { x: 1500, y: 32, width: 320, height: 240, fit: 'contain' },
    opacity: 0.9,
  }
}

function textClip(id = 'text-clip', start = seconds(1), duration = seconds(2)): TextClip {
  return {
    id,
    kind: 'text',
    timelineStartTicks: start,
    durationTicks: duration,
    text: 'Title: 100% safe',
    x: 120,
    y: 800,
    opacity: 1,
    style: { fontSizePx: 64, color: '#ffffff', backgroundColor: '#000000aa', align: 'left' },
  }
}

function videoTrack(id: string, clips: readonly VideoClip[] = [], hidden = false): VideoTrack {
  return { id, kind: 'video', name: id, locked: false, hidden, muted: false, clips }
}

function audioTrack(id: string, clips: readonly AudioClip[] = []): AudioTrack {
  return { id, kind: 'audio', name: id, locked: false, muted: false, clips }
}

function imageTrack(id: string, clips: readonly ImageClip[] = [], hidden = false): ImageTrack {
  return { id, kind: 'image', name: id, locked: false, hidden, clips }
}

function textTrack(id: string, clips: readonly TextClip[] = [], hidden = false): TextTrack {
  return { id, kind: 'text', name: id, locked: false, hidden, clips }
}

function emptyComposition(): Composition {
  let composition = createComposition(canvas)
  for (const source of [videoSource, silentVideoSource, audioSource, imageSource]) {
    composition = registerSource(composition, source)
  }
  return composition
}

function populatedComposition(): Composition {
  const composition = emptyComposition()
  return {
    ...composition,
    tracks: [
      textTrack('text-track', [textClip()]),
      imageTrack('image-track', [imageClip()]),
      videoTrack('video-track', [videoClip()]),
      audioTrack('audio-track', [audioClip()]),
    ],
  }
}

function issueCodes(composition: unknown): string[] {
  return validateComposition(composition).map((issue) => issue.code)
}

function deepFreeze<T>(value: T): T {
  if (value && typeof value === 'object' && !Object.isFrozen(value)) {
    Object.freeze(value)
    for (const nested of Object.values(value as Record<string, unknown>)) deepFreeze(nested)
  }
  return value
}

function keyframedConstant(value: number, count = MAX_KEYFRAMES_PER_VALUE): CompositionAnimatableValue {
  return {
    mode: 'keyframes',
    track: {
      timeBase: COMPOSITION_TIME_BASE,
      interpolation: 'linear',
      keyframes: Array.from({ length: count }, (_, tick) => ({ tick, value })),
    },
  }
}

describe('composition validation', () => {
  it('accepts a complete video/audio/image/text composition and derives its duration', () => {
    const composition = populatedComposition()

    expect(validateComposition(composition)).toEqual([])
    expect(compositionDurationTicks(composition)).toBe(seconds(5))
  })

  it('rejects overlap in one track but permits the same interval across compatible tracks', () => {
    const validAcrossTracks: Composition = {
      ...emptyComposition(),
      tracks: [
        videoTrack('video-a', [videoClip('clip-a')]),
        videoTrack('video-b', [videoClip('clip-b')]),
      ],
    }
    const invalidWithinTrack: Composition = {
      ...emptyComposition(),
      tracks: [videoTrack('video-a', [videoClip('clip-a'), videoClip('clip-b', seconds(4), 0, seconds(2))])],
    }

    expect(validateComposition(validAcrossTracks)).toEqual([])
    expect(issueCodes(invalidWithinTrack)).toContain('track-overlap')
  })

  it('validates handle-backed audio crossfades at touching boundaries', () => {
    const first = audioClip('audio-a', 0, seconds(1), seconds(3))
    const second = { ...audioClip('audio-b', seconds(2), seconds(3), seconds(5)), crossfadeInTicks: seconds(1) }
    const valid: Composition = { ...emptyComposition(), tracks: [audioTrack('audio-main', [first, second])] }
    expect(validateComposition(valid)).toEqual([])

    expect(issueCodes({
      ...valid,
      tracks: [audioTrack('audio-main', [first, { ...second, sourceInTicks: 0 }])],
    })).toContain('audio-crossfade-handles')
    expect(issueCodes({
      ...valid,
      tracks: [audioTrack('audio-main', [first, { ...second, timelineStartTicks: seconds(3) }])],
    })).toContain('audio-crossfade-boundary')
  })

  it('validates stable ids and source/clip compatibility', () => {
    const badAudio: AudioClip = {
      ...audioClip(),
      sourceId: silentVideoSource.id,
    }
    const composition: Composition = {
      ...emptyComposition(),
      tracks: [audioTrack('bad track id', [badAudio])],
    }

    expect(issueCodes(composition)).toEqual(expect.arrayContaining(['stable-id', 'source-kind']))
  })

  it('enforces track, clip, source, visible-layer, and 24-hour limits', () => {
    const tooManyTracks: Composition = {
      ...emptyComposition(),
      tracks: Array.from({ length: MAX_COMPOSITION_TRACKS + 1 }, (_, index) => audioTrack(`audio-${index}`)),
    }
    const tooManyVisible: Composition = {
      ...emptyComposition(),
      tracks: Array.from({ length: MAX_VISIBLE_VISUAL_TRACKS + 1 }, (_, index) => textTrack(`text-${index}`)),
    }
    const tooManySources: Composition = {
      ...emptyComposition(),
      sources: Object.fromEntries(
        Array.from({ length: MAX_COMPOSITION_SOURCES + 1 }, (_, index) => {
          const source: CompositionSource = {
            id: `audio-source-${index}`,
            kind: 'audio',
            durationTicks: seconds(1),
            width: 0,
            height: 0,
            hasAudio: true,
          }
          return [source.id, source]
        }),
      ),
    }
    const tooManyClips: Composition = {
      ...emptyComposition(),
      tracks: [
        audioTrack(
          'audio-many',
          Array.from({ length: MAX_COMPOSITION_CLIPS + 1 }, (_, index) =>
            audioClip(`audio-clip-${index}`, index, 0, 1),
          ),
        ),
      ],
    }
    const tooLong: Composition = {
      ...emptyComposition(),
      tracks: [textTrack('text-long', [textClip('text-long-clip', MAX_COMPOSITION_DURATION_TICKS, 1)])],
    }

    expect(issueCodes(tooManyTracks)).toContain('track-limit')
    expect(issueCodes(tooManyVisible)).toContain('visible-track-limit')
    expect(issueCodes(tooManySources)).toContain('source-limit')
    expect(issueCodes(tooManyClips)).toContain('clip-limit')
    expect(issueCodes(tooLong)).toContain('composition-duration')
  })

  it('migrates old authoring-v1 defaults without erasing compatible unknown data', () => {
    const raw = JSON.parse(JSON.stringify(populatedComposition())) as Record<string, unknown> & {
      tracks: Array<Record<string, unknown> & { clips: Array<Record<string, unknown>> }>
    }
    const video = raw.tracks.find((track) => track.kind === 'video')!
    const audio = raw.tracks.find((track) => track.kind === 'audio')!
    const image = raw.tracks.find((track) => track.kind === 'image')!
    const text = raw.tracks.find((track) => track.kind === 'text')!
    delete video.transitions
    delete video.clips[0]!.speed
    delete video.clips[0]!.rotationDegrees
    delete video.clips[0]!.blendMode
    delete video.clips[0]!.audioPan
    delete video.clips[0]!.playbackMode
    delete video.clips[0]!.stabilization
    delete raw.multicamGroups
    const legacyCanvas = raw.canvas as Record<string, unknown>
    delete legacyCanvas.backgroundMode
    delete legacyCanvas.backgroundBlur
    delete audio.solo
    delete audio.clips[0]!.pan
    delete audio.clips[0]!.fadeInTicks
    delete image.clips[0]!.rotationDegrees
    delete text.clips[0]!.rotationDegrees
    raw.futureDocumentData = { revision: 7 }
    video.futureTrackData = 'keep-track'
    video.clips[0]!.futureClipData = { curve: [1, 2, 3] }
    image.clips[0]!.animation = {
      x: 48,
      opacity: {
        mode: 'keyframes',
        track: {
          timeBase: COMPOSITION_TIME_BASE,
          interpolation: 'ease_in_out_cubic',
          keyframes: [{ tick: seconds(1), value: 0.5 }, { tick: 0, value: 1 }],
        },
      },
    }
    video.clips[0]!.masks = [{
      id: 'legacy-mask',
      shape: 'rectangle',
      x: 0.5,
      y: 0.5,
      width: 0.8,
      height: 0.6,
      feather: 0.1,
      inverted: false,
      futureMaskData: { keep: true },
    }]
    video.clips[0]!.audioAnimation = {
      gain: 0.75,
      futureAudioData: { keep: true },
    }
    audio.clips[0]!.audioAnimation = {
      pan: {
        mode: 'keyframes',
        track: {
          timeBase: COMPOSITION_TIME_BASE,
          interpolation: 'ease_in_out_cubic',
          keyframes: [{ tick: seconds(2), value: 0.5 }, { tick: 0, value: -0.5 }],
        },
      },
    }

    const migrated = normalizeComposition(raw)
    const migratedVideo = migrated.tracks.find((track): track is VideoTrack => track.kind === 'video')!
    const migratedAudio = migrated.tracks.find((track): track is AudioTrack => track.kind === 'audio')!

    expect(migrated.canvas).toMatchObject({ backgroundMode: 'color', backgroundBlur: 24 })
    expect(migratedVideo.transitions).toEqual([])
    expect(migratedVideo.clips[0]).toMatchObject({
      speed: 1,
      rotationDegrees: 0,
      blendMode: 'normal',
      audioPan: 0,
      frameInterpolation: 'duplicate',
      playbackMode: { mode: 'forward' },
      stabilization: { mode: 'disabled' },
    })
    expect(migratedAudio).toMatchObject({ solo: false })
    expect(migratedAudio.clips[0]).toMatchObject({ speed: 1, pan: 0, fadeInTicks: 0, fadeOutTicks: 0, voiceEffect: 'none', crossfadeInTicks: 0 })
    expect(migratedVideo.clips[0]!.audioAnimation).toEqual({
      gain: { mode: 'constant', value: 0.75 },
      futureAudioData: { keep: true },
    })
    expect(migratedAudio.clips[0]!.audioAnimation?.pan).toEqual({
      mode: 'keyframes',
      track: {
        timeBase: COMPOSITION_TIME_BASE,
        interpolation: 'ease_in_out',
        keyframes: [{ tick: 0, value: -0.5 }, { tick: seconds(2), value: 0.5 }],
      },
    })
    expect(migrated.multicamGroups).toEqual([])
    expect((migrated as unknown as Record<string, unknown>).futureDocumentData).toEqual({ revision: 7 })
    expect((migratedVideo as unknown as Record<string, unknown>).futureTrackData).toBe('keep-track')
    const migratedImage = migrated.tracks.find((track): track is ImageTrack => track.kind === 'image')!
    expect(migratedImage.clips[0]!.animation).toEqual({
      x: { mode: 'constant', value: 48 },
      opacity: {
        mode: 'keyframes',
        track: {
          timeBase: COMPOSITION_TIME_BASE,
          interpolation: 'ease_in_out',
          keyframes: [{ tick: 0, value: 1 }, { tick: seconds(1), value: 0.5 }],
        },
      },
    })
    expect(migratedVideo.clips[0]!.masks?.[0]).toMatchObject({
      id: 'legacy-mask',
      x: { mode: 'constant', value: 0.5 },
      width: { mode: 'constant', value: 0.8 },
      futureMaskData: { keep: true },
    })

    const split = splitClip(migrated, migratedVideo.clips[0]!.id, seconds(2), 'migrated-right')
    const splitTrack = split.tracks.find((track): track is VideoTrack => track.id === migratedVideo.id)!
    expect(splitTrack.clips).toHaveLength(2)
    expect(splitTrack.clips.map((clip) => (clip as unknown as Record<string, unknown>).futureClipData)).toEqual([
      { curve: [1, 2, 3] },
      { curve: [1, 2, 3] },
    ])
  })

  it('validates keyframe ordering, count, values, and masks', () => {
    const base = imageClip('animated-image', 0, seconds(2))
    const invalid: Composition = {
      ...emptyComposition(),
      tracks: [imageTrack('animated-track', [{
        ...base,
        animation: {
          x: {
            mode: 'keyframes',
            track: {
              timeBase: COMPOSITION_TIME_BASE,
              interpolation: 'linear',
              keyframes: [{ tick: seconds(1), value: 0 }, { tick: seconds(0.5), value: 1 }],
            },
          },
          opacity: keyframedConstant(2),
          y: {
            mode: 'keyframes',
            track: {
              timeBase: COMPOSITION_TIME_BASE,
              interpolation: 'linear',
              keyframes: Array.from({ length: MAX_KEYFRAMES_PER_VALUE + 1 }, (_, tick) => ({ tick, value: 0 })),
            },
          },
          rotationDegrees: {
            mode: 'keyframes',
            track: {
              timeBase: COMPOSITION_TIME_BASE,
              interpolation: 'linear',
              keyframes: [{ tick: seconds(3), value: 0 }],
            },
          },
        },
      }])],
    }
    expect(issueCodes(invalid)).toEqual(expect.arrayContaining([
      'keyframe-order',
      'keyframe-value',
      'keyframe-count',
    ]))

    const invalidMask: Composition = {
      ...emptyComposition(),
      tracks: [videoTrack('mask-track', [{
        ...videoClip('masked-video'),
        masks: [{
          id: 'mask-invalid',
          shape: 'ellipse',
          x: { mode: 'constant', value: 1.1 },
          y: { mode: 'constant', value: 0.5 },
          width: { mode: 'constant', value: 0 },
          height: { mode: 'constant', value: 0.8 },
          feather: 2,
          inverted: false,
        }],
      }])],
    }
    expect(issueCodes(invalidMask)).toEqual(expect.arrayContaining(['animatable-value', 'mask-feather']))

    const invalidAudio: Composition = {
      ...emptyComposition(),
      tracks: [audioTrack('animated-audio', [{
        ...audioClip('invalid-animated-audio'),
        audioAnimation: {
          gain: { mode: 'constant', value: 17 },
          pan: {
            mode: 'keyframes',
            track: {
              timeBase: COMPOSITION_TIME_BASE,
              interpolation: 'linear',
              keyframes: [{ tick: seconds(6), value: 1.1 }],
            },
          },
        },
      }])],
    }
    expect(issueCodes(invalidAudio)).toEqual(expect.arrayContaining(['animatable-value', 'keyframe-value']))
  })

  it('exports Linear masks and primary animation while gating the active visual keyframe budget exactly', () => {
    const linearMask: CompositionVideoMask = {
      id: 'linear-mask',
      shape: 'linear',
      rotationDegrees: { mode: 'constant', value: 35 },
      x: { mode: 'constant', value: 0.5 },
      y: { mode: 'constant', value: 0.5 },
      width: { mode: 'constant', value: 0.8 },
      height: { mode: 'constant', value: 0.8 },
      feather: 0,
      inverted: false,
    }
    const primary = videoTrack('primary', [videoClip('primary-clip')])
    const linear: Composition = {
      ...emptyComposition(),
      tracks: [videoTrack('overlay', [{ ...videoClip('overlay-clip'), masks: [linearMask] }]), primary],
    }
    expect(validateComposition(linear)).toEqual([])
    expect(compositionRenderUnavailableReason(linear)).toBeNull()
    const wireTrack = buildCompositionRenderRequest(linear).composition.tracks[0]!
    if (wireTrack.kind !== 'video') throw new Error('expected video track')
    expect(wireTrack.clips[0]!.effects[0]).toMatchObject({
      kind: 'linear_mask', rotationDegrees: { mode: 'constant', value: 35 },
    })

    const primaryAnimated: Composition = {
      ...emptyComposition(),
      tracks: [{ ...primary, clips: [{ ...primary.clips[0]!, animation: { x: keyframedConstant(0, 1) } }] }],
    }
    expect(compositionRenderUnavailableReason(primaryAnimated)).toBeNull()
    const primaryWire = buildCompositionRenderRequest(primaryAnimated).composition.tracks[0]!
    if (primaryWire.kind !== 'video') throw new Error('expected primary video track')
    expect(primaryWire.clips[0]!.transform.x).toEqual(keyframedConstant(0, 1))

    const primaryOpacity: Composition = {
      ...emptyComposition(),
      tracks: [{ ...primary, clips: [{
        ...primary.clips[0]!,
        opacity: 0,
        animation: { opacity: keyframedConstant(0, 1) },
        chromaKey: { enabled: true, color: '#00ff00', similarity: 0.1, softness: 0.05, spill: 0 },
        masks: [linearMask],
        blendMode: 'multiply',
      }] }],
    }
    expect(compositionRenderUnavailableReason(primaryOpacity)).toBeNull()
    const primaryOpacityWire = buildCompositionRenderRequest(primaryOpacity).composition.tracks[0]!
    if (primaryOpacityWire.kind !== 'video') throw new Error('expected primary video track')
    expect(primaryOpacityWire.clips[0]!.opacity).toEqual(keyframedConstant(0, 1))
    expect(primaryOpacityWire.clips[0]!.effects.map((effect) => effect.kind)).toEqual(['chroma_key', 'linear_mask'])
    expect(primaryOpacityWire.clips[0]!.blendMode).toBe('multiply')

    const animatedMasks: CompositionVideoMask[] = Array.from({ length: 15 }, (_, index) => ({
      id: `budget-mask-${index}`,
      shape: 'rectangle',
      x: keyframedConstant(0.5),
      y: keyframedConstant(0.5),
      width: keyframedConstant(0.8),
      height: keyframedConstant(0.8),
      feather: 0,
      inverted: false,
    }))
    const budgeted: Composition = {
      ...emptyComposition(),
      tracks: [
        videoTrack('budget-overlay', [{
          ...videoClip('budget-overlay-clip'),
          animation: {
            x: keyframedConstant(0),
            y: keyframedConstant(0),
            scaleX: keyframedConstant(1),
            scaleY: keyframedConstant(1),
            rotationDegrees: keyframedConstant(0),
            opacity: keyframedConstant(1),
          },
          masks: animatedMasks,
        }]),
        primary,
      ],
    }
    expect(validateComposition(budgeted)).toEqual([])
    expect(compositionRenderUnavailableReason(budgeted)).toContain(String(MAX_ACTIVE_VISUAL_KEYFRAMES))

    const animatedAudioClips = Array.from({ length: 33 }, (_, index): AudioClip => ({
      ...audioClip(`budget-audio-${index}`, index * 32, 0, 32),
      audioAnimation: {
        gain: keyframedConstant(1),
        pan: keyframedConstant(0),
      },
    }))
    const audioBudgeted: Composition = {
      ...emptyComposition(),
      tracks: [primary, audioTrack('budget-audio-track', animatedAudioClips)],
    }
    expect(issueCodes(audioBudgeted)).toContain('audio-keyframe-budget')
    expect(compositionRenderUnavailableReason(audioBudgeted)).toContain(String(MAX_ACTIVE_AUDIO_KEYFRAMES))
    expect(validateComposition({
      ...audioBudgeted,
      tracks: [primary, { ...audioBudgeted.tracks[1]!, muted: true }],
    })).toEqual([])

    const overlayAudioAutomation: Composition = {
      ...emptyComposition(),
      tracks: [
        videoTrack('audio-overlay', [{
          ...videoClip('audio-overlay-clip'),
          audioAnimation: { gain: keyframedConstant(1, 2) },
        }]),
        primary,
      ],
    }
    expect(compositionRenderUnavailableReason(overlayAudioAutomation)).toContain('только на основной видеодорожке')
  })

  it('accepts optical flow only for active slow-motion video and migrates ordinary clips to duplicate', () => {
    const slowClip: VideoClip = {
      ...videoClip('slow-motion', 0, 0, seconds(4)),
      speed: 0.5,
      frameInterpolation: 'optical_flow',
    }
    const active: Composition = { ...emptyComposition(), tracks: [videoTrack('slow-track', [slowClip])] }
    expect(validateComposition(active)).toEqual([])
    expect(compositionRenderUnavailableReason(active)).toBeNull()
    const wire = buildCompositionRenderRequest(active).composition.tracks[0]!
    expect(wire.kind === 'video' ? wire.clips[0]?.frameInterpolation : null).toBe('optical_flow')

    const normalSpeed: Composition = {
      ...active,
      tracks: [{ ...active.tracks[0]!, clips: [{ ...slowClip, speed: 1 }] }] as VideoTrack[],
    }
    expect(issueCodes(normalSpeed)).toContain('optical-flow-speed')
    expect(compositionRenderUnavailableReason(normalSpeed)).toContain('меньше 1x')

    const hidden: Composition = {
      ...active,
      tracks: [{ ...(active.tracks[0] as VideoTrack), hidden: true }],
    }
    expect(issueCodes(hidden)).toContain('optical-flow-active')
    expect(compositionRenderUnavailableReason(hidden)).toContain('видимой video-дорожке')

    const raw = JSON.parse(JSON.stringify(active)) as {
      tracks: Array<{ clips: Array<Record<string, unknown>> }>
    }
    delete raw.tracks[0]!.clips[0]!.frameInterpolation
    expect(normalizeComposition(raw).tracks[0]!.clips[0]).toMatchObject({ frameInterpolation: 'duplicate' })
  })

  it('retains imported effect timing through normalization and the render payload', () => {
    const timing = { mode: 'filmcraft_source' as const, holdFilters: true, reverseWhileFrozen: false }
    const composition: Composition = { ...emptyComposition(), tracks: [videoTrack('timed', [{ ...videoClip(), effectTiming: timing }])] }
    expect(validateComposition(composition)).toEqual([])
    const normalized = normalizeComposition(JSON.parse(JSON.stringify(composition)))
    expect(buildCompositionRenderRequest(normalized).composition.tracks[0]).toMatchObject({ clips: [{ effectTiming: timing }] })
    expect(issueCodes({ ...composition, tracks: [videoTrack('timed', [{ ...videoClip(), effectTiming: { ...timing, holdFilters: 'yes' } } as unknown as VideoClip])] })).toContain('effect-timing')
  })

  it('validates playback modes, freeze audio/optical constraints, and transition endpoints', () => {
    const reverse = videoClip('reverse-clip')
    const reverseComposition: Composition = {
      ...emptyComposition(),
      tracks: [videoTrack('reverse-track', [{ ...reverse, playbackMode: { mode: 'reverse' } }])],
    }
    expect(validateComposition(reverseComposition)).toEqual([])
    expect(buildCompositionRenderRequest(reverseComposition).composition.tracks[0]).toMatchObject({
      kind: 'video',
      clips: [{ playbackMode: { mode: 'reverse' } }],
    })

    const frozen: VideoClip = {
      ...videoClip('freeze-clip'),
      sourceAudioEnabled: false,
      playbackMode: { mode: 'freeze', sourceTick: seconds(2) },
    }
    const freezeComposition: Composition = {
      ...emptyComposition(),
      tracks: [videoTrack('freeze-track', [frozen])],
    }
    expect(validateComposition(freezeComposition)).toEqual([])
    expect(buildCompositionRenderRequest(freezeComposition).composition.tracks[0]).toMatchObject({
      kind: 'video',
      clips: [{ sourceAudioEnabled: false, playbackMode: { mode: 'freeze', sourceTick: seconds(2) } }],
    })

    expect(issueCodes({
      ...freezeComposition,
      tracks: [videoTrack('freeze-track', [{ ...frozen, sourceAudioEnabled: true }])],
    })).toContain('freeze-source-audio')
    expect(issueCodes({
      ...freezeComposition,
      tracks: [videoTrack('freeze-track', [{ ...frozen, playbackMode: { mode: 'freeze', sourceTick: seconds(5) } }])],
    })).toContain('freeze-source-tick')
    expect(issueCodes({
      ...freezeComposition,
      tracks: [videoTrack('freeze-track', [{ ...frozen, speed: 0.5, frameInterpolation: 'optical_flow' }])],
    })).toContain('freeze-optical-flow')

    const transitioned: Composition = {
      ...emptyComposition(),
      tracks: [{
        ...videoTrack('playback-transition', [
          { ...videoClip('transition-from', 0, 0, seconds(4)), playbackMode: { mode: 'reverse' } },
          videoClip('transition-to', seconds(4), seconds(2), seconds(6)),
        ]),
        transitions: [{
          id: 'transition-playback',
          fromClipId: 'transition-from',
          toClipId: 'transition-to',
          durationTicks: seconds(1),
          kind: 'dissolve',
        }],
      }],
    }
    expect(compositionRenderUnavailableReason(transitioned)).toContain('forward playback')
    expect(compositionTransitionUnavailableReason(
      transitioned,
      'playback-transition',
      (transitioned.tracks[0] as VideoTrack).transitions![0]!,
    )).toContain('forward playback')
  })

  it('migrates, validates, and emits the exact deterministic stabilization contract', () => {
    const stabilized: VideoClip = {
      ...videoClip('stabilized-clip'),
      stabilization: { mode: 'deshake', radiusX: 32, radiusY: 64 },
    }
    const composition: Composition = {
      ...emptyComposition(),
      tracks: [videoTrack('stabilized-track', [stabilized])],
    }

    expect(validateComposition(composition)).toEqual([])
    expect(buildCompositionRenderRequest(composition).composition.tracks[0]).toMatchObject({
      kind: 'video',
      clips: [{ stabilization: { mode: 'deshake', radiusX: 32, radiusY: 64 } }],
    })

    const raw = JSON.parse(JSON.stringify(composition)) as {
      tracks: Array<{ clips: Array<Record<string, unknown>> }>
    }
    delete raw.tracks[0]!.clips[0]!.stabilization
    expect(normalizeComposition(raw).tracks[0]!.clips[0]).toMatchObject({ stabilization: { mode: 'disabled' } })

    expect(issueCodes({
      ...composition,
      tracks: [videoTrack('stabilized-track', [{
        ...stabilized,
        stabilization: { mode: 'deshake', radiusX: 24, radiusY: 64 },
      } as unknown as VideoClip])],
    })).toContain('stabilization-radius')
    expect(issueCodes({
      ...composition,
      tracks: [videoTrack('stabilized-track', [{
        ...stabilized,
        stabilization: { mode: 'disabled', futureRadius: 48 },
      } as unknown as VideoClip])],
    })).toContain('stabilization-field')
    expect(issueCodes({
      ...composition,
      tracks: [videoTrack('stabilized-track', [stabilized], true)],
    })).toContain('stabilization-active')
    expect(issueCodes({
      ...composition,
      tracks: [videoTrack('stabilized-track', [{
        ...stabilized,
        sourceAudioEnabled: false,
        playbackMode: { mode: 'freeze', sourceTick: seconds(2) },
      }])],
    })).toContain('freeze-stabilization')
  })
})

describe('immutable composition commands', () => {
  it('slips source media without changing the timeline range and clamps to source bounds', () => {
    let composition = createComposition(canvas, { [videoSource.id]: videoSource })
    composition = addTrack(composition, videoTrack('video', [videoClip('slip', seconds(4), seconds(2), seconds(7))]))
    const slipped = slipClip(composition, 'slip', seconds(3))
    const clip = slipped.tracks[0]!.clips[0]!
    expect(clip.timelineStartTicks).toBe(seconds(4))
    expect(clip).toMatchObject({ sourceInTicks: seconds(5), sourceOutTicks: seconds(10) })
    const clamped = slipClip(slipped, 'slip', seconds(99)).tracks[0]!.clips[0]!
    expect(clamped).toMatchObject({ sourceInTicks: seconds(15), sourceOutTicks: seconds(20) })
  })

  it('adds tracks and clips without mutating frozen input state', () => {
    const before = deepFreeze(emptyComposition())
    const withTrack = addTrack(before, videoTrack('video-a'))
    const after = addClip(withTrack, 'video-a', videoClip())

    expect(before.tracks).toEqual([])
    expect(withTrack.tracks[0]!.clips).toEqual([])
    expect(after.tracks[0]!.clips.map((clip) => clip.id)).toEqual(['video-clip'])
    expect(after).not.toBe(withTrack)
    expect(after.tracks[0]).not.toBe(withTrack.tracks[0])
  })

  it('moves clips across same-kind tracks and rejects incompatible or overlapping moves', () => {
    let composition = emptyComposition()
    composition = addTrack(composition, videoTrack('video-a'))
    composition = addTrack(composition, videoTrack('video-b'))
    composition = addTrack(composition, audioTrack('audio-a'))
    composition = addClip(composition, 'video-a', videoClip('moving'))
    composition = addClip(composition, 'video-b', videoClip('stationary', seconds(7), 0, seconds(2)))

    const moved = moveClip(composition, 'moving', 'video-b', 0)
    expect(moved.tracks[0]!.clips).toHaveLength(0)
    expect(moved.tracks[1]!.clips.map((clip) => clip.id)).toEqual(['moving', 'stationary'])
    expect(() => moveClip(composition, 'moving', 'audio-a', 0)).toThrow(CompositionCommandError)
    expect(() => moveClip(composition, 'moving', 'video-b', seconds(6))).toThrow(CompositionValidationError)
  })

  it('trims source clips by moving source boundaries with composition boundaries', () => {
    let composition = emptyComposition()
    composition = addTrack(composition, videoTrack('video-a'))
    composition = addClip(composition, 'video-a', videoClip('clip', seconds(2), seconds(1), seconds(6)))

    const trimmed = trimClip(composition, 'clip', seconds(3), seconds(6))
    const clip = trimmed.tracks[0]!.clips[0] as VideoClip

    expect(clip.timelineStartTicks).toBe(seconds(3))
    expect(clip.sourceInTicks).toBe(seconds(2))
    expect(clip.sourceOutTicks).toBe(seconds(5))
    expect((composition.tracks[0]!.clips[0] as VideoClip).sourceInTicks).toBe(seconds(1))
  })

  it('splits source and timed text clips while preserving stable left ids', () => {
    let sourceComposition = emptyComposition()
    sourceComposition = addTrack(sourceComposition, videoTrack('video-a'))
    sourceComposition = addClip(
      sourceComposition,
      'video-a',
      {
        ...videoClip('left-id', seconds(2), seconds(1), seconds(6)),
        animation: {
          x: {
            mode: 'keyframes',
            track: {
              timeBase: COMPOSITION_TIME_BASE,
              interpolation: 'linear',
              keyframes: [
                { tick: 0, value: 0 },
                { tick: seconds(2), value: 20 },
                { tick: seconds(4), value: 40 },
              ],
            },
          },
        },
      },
    )
    const splitSource = splitClip(sourceComposition, 'left-id', seconds(4), 'right-id')
    const [left, right] = splitSource.tracks[0]!.clips as readonly VideoClip[]

    expect(left).toMatchObject({ id: 'left-id', sourceInTicks: seconds(1), sourceOutTicks: seconds(3) })
    expect(right).toMatchObject({ id: 'right-id', timelineStartTicks: seconds(4), sourceInTicks: seconds(3) })
    for (const at of [0, seconds(0.25), seconds(1), seconds(1.75)]) {
      expect(sampleAnimatableValue(left.animation!.x!, at)).toBe(at / COMPOSITION_TIME_BASE * 10)
      expect(sampleAnimatableValue(right.animation!.x!, at)).toBe((at / COMPOSITION_TIME_BASE + 2) * 10)
    }

    let textComposition = emptyComposition()
    textComposition = addTrack(textComposition, textTrack('text-a'))
    textComposition = addClip(textComposition, 'text-a', textClip('title', seconds(1), seconds(4)))
    const splitText = splitClip(textComposition, 'title', seconds(3), 'title-right')
    expect(splitText.tracks[0]!.clips).toEqual([
      expect.objectContaining({ id: 'title', durationTicks: seconds(2) }),
      expect.objectContaining({ id: 'title-right', timelineStartTicks: seconds(3), durationTicks: seconds(2) }),
    ])
  })

  it('duplicates, deletes, and reorders deterministically', () => {
    let composition = emptyComposition()
    composition = addTrack(composition, videoTrack('top'))
    composition = addTrack(composition, videoTrack('bottom'))
    composition = addClip(composition, 'top', videoClip('original'))
    composition = duplicateClip(composition, 'original', { id: 'copy' })

    expect(composition.tracks[0]!.clips.map((clip) => clip.id)).toEqual(['original', 'copy'])
    composition = deleteClip(composition, 'original')
    expect(composition.tracks[0]!.clips.map((clip) => clip.id)).toEqual(['copy'])
    composition = reorderTrack(composition, 'bottom', 0)
    expect(composition.tracks.map((track) => track.id)).toEqual(['bottom', 'top'])
  })

  it('preserves optical-flow authoring through trim, split, and duplicate commands', () => {
    let composition = emptyComposition()
    composition = addTrack(composition, videoTrack('slow-track'))
    composition = addClip(composition, 'slow-track', {
      ...videoClip('slow-source-clip', 0, 0, seconds(4)),
      speed: 0.5,
      frameInterpolation: 'optical_flow',
    })

    const trimmed = trimClip(composition, 'slow-source-clip', seconds(1), seconds(7))
    expect(trimmed.tracks[0]!.clips[0]).toMatchObject({
      frameInterpolation: 'optical_flow',
      sourceInTicks: seconds(0.5),
      sourceOutTicks: seconds(3.5),
    })
    const split = splitClip(composition, 'slow-source-clip', seconds(4), 'slow-right')
    expect(split.tracks[0]!.clips).toEqual([
      expect.objectContaining({ id: 'slow-source-clip', frameInterpolation: 'optical_flow', sourceOutTicks: seconds(2) }),
      expect.objectContaining({ id: 'slow-right', frameInterpolation: 'optical_flow', sourceInTicks: seconds(2) }),
    ])
    const duplicated = duplicateClip(composition, 'slow-source-clip', { id: 'slow-copy' })
    expect(duplicated.tracks[0]!.clips[1]).toMatchObject({ frameInterpolation: 'optical_flow' })
  })

  it('preserves stabilization through trim, split, and duplicate commands', () => {
    let composition = emptyComposition()
    composition = addTrack(composition, videoTrack('stabilized-track'))
    composition = addClip(composition, 'stabilized-track', {
      ...videoClip('stabilized-source-clip', 0, 0, seconds(8)),
      stabilization: { mode: 'deshake', radiusX: 16, radiusY: 48 },
    })

    expect(trimClip(composition, 'stabilized-source-clip', seconds(1), seconds(7)).tracks[0]!.clips[0])
      .toMatchObject({ stabilization: { mode: 'deshake', radiusX: 16, radiusY: 48 } })
    expect(splitClip(composition, 'stabilized-source-clip', seconds(4), 'stabilized-right').tracks[0]!.clips)
      .toEqual([
        expect.objectContaining({ id: 'stabilized-source-clip', stabilization: { mode: 'deshake', radiusX: 16, radiusY: 48 } }),
        expect.objectContaining({ id: 'stabilized-right', stabilization: { mode: 'deshake', radiusX: 16, radiusY: 48 } }),
      ])
    expect(duplicateClip(composition, 'stabilized-source-clip', { id: 'stabilized-copy' }).tracks[0]!.clips[1])
      .toMatchObject({ stabilization: { mode: 'deshake', radiusX: 16, radiusY: 48 } })
  })

  it('clones and clip-locally slices audio and mask automation without mutating sibling tracks', () => {
    const animated = (from: number, middle: number, to: number): CompositionAnimatableValue => ({
      mode: 'keyframes',
      track: {
        timeBase: COMPOSITION_TIME_BASE,
        interpolation: 'linear',
        keyframes: [
          { tick: 0, value: from },
          { tick: seconds(4), value: middle },
          { tick: seconds(8), value: to },
        ],
      },
    })
    const mask: CompositionVideoMask = {
      id: 'animated-mask',
      shape: 'ellipse',
      x: animated(0.2, 0.5, 0.8),
      y: { mode: 'constant', value: 0.5 },
      width: { mode: 'constant', value: 0.8 },
      height: { mode: 'constant', value: 0.8 },
      feather: 0,
      inverted: false,
    }
    const composition = deepFreeze<Composition>({
      ...emptyComposition(),
      tracks: [
        videoTrack('automation-video-track', [{
          ...videoClip('automation-video', 0, 0, seconds(8)),
          audioAnimation: { gain: animated(0.25, 1, 0.5) },
          masks: [mask],
        }]),
        audioTrack('automation-audio-track', [{
          ...audioClip('automation-audio', 0, 0, seconds(8)),
          audioAnimation: { pan: animated(-1, 0, 1) },
        }]),
      ],
    })

    const trimmed = trimClip(composition, 'automation-video', seconds(2), seconds(6))
    const trimmedVideo = trimmed.tracks[0]!.clips[0] as VideoClip
    expect(trimmedVideo.audioAnimation?.gain).toMatchObject({
      track: { keyframes: [{ tick: -seconds(2), value: 0.25 }, { tick: seconds(2), value: 1 }, { tick: seconds(6), value: 0.5 }] },
    })
    expect(trimmedVideo.masks?.[0]?.x).toMatchObject({
      track: { keyframes: [{ tick: -seconds(2), value: 0.2 }, { tick: seconds(2), value: 0.5 }, { tick: seconds(6), value: 0.8 }] },
    })
    expect(trimmed.tracks[1]).toBe(composition.tracks[1])

    const split = splitClip(composition, 'automation-audio', seconds(4), 'automation-audio-right')
    const splitAudio = (split.tracks[1] as AudioTrack).clips
    for (const at of [0, seconds(0.5), seconds(2), seconds(3.75)]) {
      expect(sampleAnimatableValue(splitAudio[0]!.audioAnimation!.pan!, at)).toBe(-1 + at / seconds(4))
      expect(sampleAnimatableValue(splitAudio[1]!.audioAnimation!.pan!, at)).toBe(at / seconds(4))
    }
    expect(split.tracks[0]).toBe(composition.tracks[0])

    const duplicated = duplicateClip(composition, 'automation-video', {
      id: 'automation-video-copy',
      timelineStartTicks: seconds(8),
    })
    const videoClips = (duplicated.tracks[0] as VideoTrack).clips
    expect(videoClips[1]!.audioAnimation).toEqual(videoClips[0]!.audioAnimation)
    expect(videoClips[1]!.audioAnimation).not.toBe(videoClips[0]!.audioAnimation)
    expect(videoClips[1]!.masks?.[0]).not.toBe(videoClips[0]!.masks?.[0])
  })

  it('preserves playback modes and uses mode-aware source ranges through trim, split, and duplicate', () => {
    let reverseComposition = emptyComposition()
    reverseComposition = addTrack(reverseComposition, videoTrack('reverse-track'))
    reverseComposition = addClip(reverseComposition, 'reverse-track', {
      ...videoClip('reverse-source-clip', 0, seconds(2), seconds(10)),
      playbackMode: { mode: 'reverse' },
    })
    const reverseTrimmed = trimClip(reverseComposition, 'reverse-source-clip', seconds(2), seconds(6))
    expect(reverseTrimmed.tracks[0]!.clips[0]).toMatchObject({
      playbackMode: { mode: 'reverse' },
      sourceInTicks: seconds(4),
      sourceOutTicks: seconds(8),
    })
    const reverseSplit = splitClip(reverseComposition, 'reverse-source-clip', seconds(4), 'reverse-right')
    expect(reverseSplit.tracks[0]!.clips).toEqual([
      expect.objectContaining({ id: 'reverse-source-clip', sourceInTicks: seconds(6), sourceOutTicks: seconds(10), playbackMode: { mode: 'reverse' } }),
      expect.objectContaining({ id: 'reverse-right', sourceInTicks: seconds(2), sourceOutTicks: seconds(6), playbackMode: { mode: 'reverse' } }),
    ])

    let freezeComposition = emptyComposition()
    freezeComposition = addTrack(freezeComposition, videoTrack('freeze-track'))
    freezeComposition = addClip(freezeComposition, 'freeze-track', {
      ...videoClip('freeze-source-clip', 0, 0, seconds(8)),
      sourceAudioEnabled: false,
      playbackMode: { mode: 'freeze', sourceTick: seconds(3) },
    })
    const freezeTrimmed = trimClip(freezeComposition, 'freeze-source-clip', seconds(1), seconds(6))
    expect(freezeTrimmed.tracks[0]!.clips[0]).toMatchObject({
      playbackMode: { mode: 'freeze', sourceTick: seconds(3) },
      sourceInTicks: 0,
      sourceOutTicks: seconds(5),
    })
    const freezeSplit = splitClip(freezeComposition, 'freeze-source-clip', seconds(4), 'freeze-right')
    expect(freezeSplit.tracks[0]!.clips).toEqual([
      expect.objectContaining({ id: 'freeze-source-clip', sourceInTicks: 0, sourceOutTicks: seconds(4), playbackMode: { mode: 'freeze', sourceTick: seconds(3) } }),
      expect.objectContaining({ id: 'freeze-right', sourceInTicks: 0, sourceOutTicks: seconds(4), playbackMode: { mode: 'freeze', sourceTick: seconds(3) } }),
    ])
    const freezeDuplicated = duplicateClip(freezeComposition, 'freeze-source-clip', { id: 'freeze-copy' })
    expect(freezeDuplicated.tracks[0]!.clips[1]).toMatchObject({
      playbackMode: { mode: 'freeze', sourceTick: seconds(3) },
      sourceAudioEnabled: false,
    })
  })

  it('moves a complete transition pair to one destination and refuses separated or occupied endpoints atomically', () => {
    const transition: CompositionTransition = { id: 'moving-transition', fromClipId: 'clip-a', toClipId: 'clip-b', durationTicks: seconds(1), kind: 'dissolve' }
    const primary = videoTrack('primary', [videoClip('clip-a', 0, 0, seconds(4)), videoClip('clip-b', seconds(4), seconds(2), seconds(6))])
    const destination = videoTrack('destination', [])
    const document = upsertTransition({ ...emptyComposition(), tracks: [primary, destination] }, 'primary', transition)
    const before = JSON.stringify(document)
    const placements = [{ clipId: 'clip-a', trackId: 'destination' }, { clipId: 'clip-b', trackId: 'destination' }]
    const moved = moveClipPlacements(document, placements, 'clip-a', seconds(2))
    expect((moved.tracks[0] as VideoTrack).transitions).toEqual([])
    expect((moved.tracks[1] as VideoTrack).transitions).toEqual([transition])
    expect(moved.tracks[1]!.clips.map((clip) => clip.timelineStartTicks)).toEqual([seconds(2), seconds(6)])
    expect(compositionTransitionUnavailableReason(moved, 'destination', transition)).toBeNull()
    expect(() => moveClipPlacements(document, [placements[0]!], 'clip-a', 0)).toThrow('together')
    const occupied = { ...document, tracks: [document.tracks[0]!, videoTrack('destination', [videoClip('obstacle', seconds(2), 0, seconds(4))])] }
    expect(() => moveClipPlacements(occupied, placements, 'clip-a', seconds(2))).toThrow()
    expect(JSON.stringify(document)).toBe(before)
  })

  it('authors exact-handle adjacent transitions on primary and overlay tracks and cleans references on delete', () => {
    const transition: CompositionTransition = {
      id: 'transition-a-b',
      fromClipId: 'clip-a',
      toClipId: 'clip-b',
      durationTicks: seconds(1),
      kind: 'dissolve',
    }
    const primary: VideoTrack = {
      ...videoTrack('primary', [
        videoClip('clip-a', 0, 0, seconds(4)),
        videoClip('clip-b', seconds(4), seconds(2), seconds(6)),
      ]),
      transitions: [],
    }
    const composition: Composition = { ...emptyComposition(), tracks: [primary] }
    const transitioned = upsertTransition(composition, primary.id, transition)
    expect((transitioned.tracks[0] as VideoTrack).transitions).toEqual([transition])

    const deleted = deleteClip(transitioned, 'clip-b')
    expect((deleted.tracks[0] as VideoTrack).transitions).toEqual([])

    const withoutHead: Composition = {
      ...composition,
      tracks: [{ ...primary, clips: [primary.clips[0]!, { ...primary.clips[1]!, sourceInTicks: 0 }] }],
    }
    expect(() => upsertTransition(withoutHead, primary.id, transition)).toThrow(
      expect.objectContaining({ code: 'transition-conflict' }),
    )

    const overlayTransition = { ...transition, id: 'overlay-transition', fromClipId: 'overlay-a', toClipId: 'overlay-b' }
    const overlay = {
      ...videoTrack('overlay', [
        videoClip('overlay-a', 0, 0, seconds(4)),
        videoClip('overlay-b', seconds(4), seconds(2), seconds(6)),
      ]),
      transitions: [],
    }
    const layered: Composition = { ...emptyComposition(), tracks: [overlay, primary] }
    expect(compositionTransitionUnavailableReason(layered, overlay.id, overlayTransition)).toBeNull()
    const overlayTransitioned = upsertTransition(layered, overlay.id, overlayTransition)
    expect((overlayTransitioned.tracks[0] as VideoTrack).transitions).toEqual([overlayTransition])
    expect(compositionRenderUnavailableReason(overlayTransitioned)).toBeNull()
  })

  it('honours locked tracks', () => {
    const locked: VideoTrack = { ...videoTrack('locked', [videoClip()]), locked: true }
    const composition: Composition = { ...emptyComposition(), tracks: [locked] }

    expect(() => deleteClip(composition, 'video-clip')).toThrow(
      expect.objectContaining({ code: 'track-locked' }),
    )
  })
})

describe('snapping helpers', () => {
  it('collects deterministic visible targets and excludes the moving clip', () => {
    const composition: Composition = {
      ...emptyComposition(),
      tracks: [
        videoTrack('video-a', [videoClip('moving')]),
        videoTrack('video-b', [videoClip('target', seconds(8), 0, seconds(2))]),
        imageTrack('hidden', [imageClip('hidden-clip', seconds(20))], true),
      ],
    }
    const targets = collectSnapTargets(composition, {
      playheadTicks: seconds(12),
      excludeClipId: 'moving',
    })

    expect(targets).toEqual([
      { tick: 0, kind: 'zero' },
      { tick: seconds(8), kind: 'clip-start', clipId: 'target' },
      { tick: seconds(10), kind: 'clip-end', clipId: 'target' },
      { tick: seconds(12), kind: 'playhead' },
    ])
  })

  it('snaps a scalar or either moving clip edge within threshold', () => {
    const targets = [
      { tick: seconds(8), kind: 'clip-start' as const, clipId: 'target' },
      { tick: seconds(12), kind: 'playhead' as const },
    ]

    expect(snapTick(seconds(12) - 10, targets, 20)).toMatchObject({
      valueTicks: seconds(12),
      snapped: true,
      deltaTicks: 10,
    })
    expect(snapClipStart(seconds(5) + 100_000, seconds(3), targets, 200_000)).toMatchObject({
      timelineStartTicks: seconds(5),
      snapped: true,
      deltaTicks: -100_000,
      edge: 'end',
    })
    expect(snapTick(seconds(7), targets, 100)).toEqual({
      valueTicks: seconds(7),
      snapped: false,
      deltaTicks: 0,
    })
  })
})

describe('composition render payload', () => {
  it('preserves supported ordered image masks and rejects unsupported shapes', () => {
    const constant = (value: number) => ({ mode: 'constant' as const, value })
    const effects = [
      {kind:'mask' as const,shape:'ellipse' as const,x:constant(0),y:constant(0),width:constant(20),height:constant(10),rotationDegrees:constant(0),feather:0,inverted:false},
      {kind:'linear_mask' as const,x:constant(0),y:constant(0),rotationDegrees:constant(45),feather:0,inverted:true},
    ]
    const base = populatedComposition()
    const value: Composition = {...base,tracks:base.tracks.map(track=>track.kind==='image'?{...track,clips:track.clips.map(clip=>({...clip,effects}))}:track)}
    const restored = normalizeComposition(JSON.parse(JSON.stringify(value)))
    const wire=buildCompositionRenderRequest(restored).composition.tracks.find(track=>track.kind==='image')!
    expect(wire.clips[0]).toMatchObject({effects})
    const unsupported=JSON.parse(JSON.stringify(value))
    unsupported.tracks.find((track:{kind:string})=>track.kind==='image').clips[0].effects[0].shape='polygon'
    expect(()=>buildCompositionRenderRequest(unsupported)).toThrow('Неподдерживаемые clip effects')
  })

  it('retains authored image effects through render JSON', () => {
    const effects = [{ kind: 'gamma_correction' as const, parameters: { gamma: 20 } }]
    const value = populatedComposition()
    const document: Composition = { ...value, tracks: value.tracks.map(track => track.kind === 'image'
      ? { ...track, clips: track.clips.map(clip => ({ ...clip, effects })) } : track) }
    const restored = normalizeComposition(JSON.parse(JSON.stringify(document)))
    expect(restored.tracks.find(track => track.kind === 'image')!.clips[0]).toMatchObject({ effects })
    const wire = JSON.parse(JSON.stringify(buildCompositionRenderRequest(document))).composition
    const image = wire.tracks.find((track: {kind: string}) => track.kind === 'image')
    expect(image.clips[0].effects).toEqual(effects)
    const old = buildCompositionRenderRequest(value).composition.tracks.find(track => track.kind === 'image')!
    expect(old.clips[0]).not.toHaveProperty('effects')
    const unknown = { ...document, tracks: document.tracks.map(track => track.kind === 'image' ? { ...track, clips: track.clips.map(clip => ({ ...clip, effects: [{ kind: 'unknown_effect' }] })) } : track) } as unknown as Composition
    expect(() => buildCompositionRenderRequest(unknown)).toThrow('Неподдерживаемые clip effects')
  })

  it('preserves disabled flags on all wire clip kinds without deleting authored content', () => {
    const value = populatedComposition()
    const document = { ...value, tracks: value.tracks.map((track) => ({ ...track, clips: track.clips.map((clip) => ({ ...clip, enabled: track.kind === 'video' })) })) } as Composition
    const wire = buildCompositionRenderRequest(document).composition
    expect(wire.tracks.map((track) => track.clips[0]!.enabled)).toEqual([false, false, true, false])
    expect(document.tracks.reduce((sum, track) => sum + track.clips.length, 0)).toBe(4)
    expect(buildCompositionRenderRequest(value).composition.tracks.every((track) => track.clips[0]!.enabled)).toBe(true)
  })

  it('omits render transitions with a disabled endpoint while retaining the authored transition', () => {
    const primary: VideoTrack = { ...videoTrack('primary', [videoClip('a', 0, 0, seconds(4)), { ...videoClip('b', seconds(4), seconds(2), seconds(6)), enabled: false }]), transitions: [{ id: 'fade', fromClipId: 'a', toClipId: 'b', durationTicks: seconds(1), kind: 'dissolve' }] }
    const document = { ...emptyComposition(), tracks: [primary] }
    const wire = buildCompositionRenderRequest(document).composition.tracks[0]!
    expect(wire.kind).toBe('video')
    if (wire.kind === 'video') expect(wire.transitions).toEqual([])
    expect(primary.transitions).toHaveLength(1)
    expect(wire.clips).toHaveLength(2)
  })

  it('uses the v1 endpoint/contract and strips paths, URLs, and framework metadata', () => {
    const clean = populatedComposition()
    const taintedSource = {
      ...clean.sources[videoSource.id]!,
      path: '/private/source.mp4',
      url: 'file:///private/source.mp4',
      fps: 30,
      vcodec: 'h264',
      acodec: 'aac',
    }
    const firstTrack = clean.tracks[0]!
    const taintedTrack = {
      ...firstTrack,
      selected: true,
      clips: firstTrack.clips.map((clip) => ({ ...clip, filesystemPath: '/tmp/title.txt', selected: true })),
    } as unknown as CompositionTrack
    const tainted = {
      ...clean,
      sources: { ...clean.sources, [videoSource.id]: taintedSource },
      tracks: [taintedTrack, ...clean.tracks.slice(1)],
      selectedClipId: 'text-clip',
      markers: [{ id: 'render-marker', tick: 1, label: 'Do not send marker', origin: 'auto_beat' }],
      multicamGroups: [{
        id: 'render-multicam',
        name: 'Authoring only',
        timelineStartTicks: 0,
        durationTicks: seconds(5),
        videoTrackId: 'video-track',
        angles: [
          { id: 'render-angle-a', label: 'A', sourceId: videoSource.id, sourceTickAtGroupStart: 0 },
          { id: 'render-angle-b', label: 'B', sourceId: silentVideoSource.id, sourceTickAtGroupStart: 0 },
        ],
        switches: [{
          id: 'render-switch',
          clipId: 'render-multicam-output',
          timelineTick: 0,
          angleId: 'render-angle-a',
        }],
      }],
    } as Composition

    const payload: CompositionRenderRequest = buildCompositionRenderRequest(tainted, {
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'high',
    })
    const serialized = JSON.stringify(payload)

    expect(COMPOSITION_RENDER_ENDPOINT).toBe('/api/compositions/render')
    expect(payload.schemaVersion).toBe(COMPOSITION_SCHEMA_VERSION)
    expect(payload.composition.schemaVersion).toBe(COMPOSITION_SCHEMA_VERSION)
    expect(payload.composition.timeBase).toBe(COMPOSITION_TIME_BASE)
    expect(payload.composition.canvas).toEqual({
      width: 1920,
      height: 1080,
      fpsMilli: 30_000,
      background: { red: 0, green: 0, blue: 0, alpha: 1 },
      backgroundMode: 'color',
      backgroundBlur: 24,
    })
    expect(payload.output).toEqual({ profile: { container: 'mp4', codec: 'h264' }, qualityTier: 'high' })
    expect(payload.composition.sources[videoSource.id]).toEqual(videoSource)
    const video = payload.composition.tracks.find((track) => track.kind === 'video')!
    expect(video).toMatchObject({ hidden: false, muted: false, locked: false, transitions: [] })
    expect(video.clips[0]).toMatchObject({
      placement: { timelineStartTick: 0, sourceInTick: 0, sourceOutTick: seconds(5), speed: 1 },
      transform: {
        x: { mode: 'constant', value: 0 },
        y: { mode: 'constant', value: 0 },
        scaleX: { mode: 'constant', value: 1 },
        scaleY: { mode: 'constant', value: 1 },
      },
      sourceAudioEnabled: true,
      audioGain: { mode: 'constant', value: 1 },
      audioPan: { mode: 'constant', value: 0 },
    })
    const image = payload.composition.tracks.find((track) => track.kind === 'image')!
    expect(image.clips[0]).toMatchObject({
      transform: {
        x: { mode: 'constant', value: 1500 },
        y: { mode: 'constant', value: 32 },
        scaleX: { mode: 'constant', value: 0.4 },
        scaleY: { mode: 'constant', value: 0.4 },
      },
      blendMode: 'normal',
    })
    const audio = payload.composition.tracks.find((track) => track.kind === 'audio')!
    expect(audio).toMatchObject({ muted: false, solo: false })
    expect(audio.clips[0]).toMatchObject({
      gain: { mode: 'constant', value: 1 },
      pan: { mode: 'constant', value: 0 },
      fadeInTicks: 0,
      fadeOutTicks: 0,
      crossfadeInTicks: 0,
    })
    expect(serialized).not.toContain('filesystemPath')
    expect(serialized).not.toContain('file:///')
    expect(serialized).not.toContain('/private/source.mp4')
    expect(serialized).not.toContain('selectedClipId')
    expect(serialized).not.toContain('"selected"')
    expect(buildCompositionWireDocument(tainted).metadata?.markers).toEqual(compositionMarkers(tainted))
    expect(compositionMarkers(tainted)[0]?.id).toBe('render-marker')
    expect(payload.composition).not.toHaveProperty('metadata')
    expect(serialized).not.toContain('render-marker')
    expect(serialized).not.toContain('render-multicam')
    expect(serialized).not.toContain('multicamGroups')
    expect(serialized).not.toContain('vcodec')
    expect(serialized).not.toContain('acodec')
  })

  it('emits the exact static parity wire for speed, chroma, transitions, source audio, and audio mix', () => {
    const transition: CompositionTransition = {
      id: 'transition-primary',
      fromClipId: 'primary-a',
      toClipId: 'primary-b',
      durationTicks: seconds(1),
      kind: 'wipe_left',
    }
    const primary: VideoTrack = {
      ...videoTrack('primary', [
        {
          ...videoClip('primary-a', 0, 0, seconds(4)),
          sourceAudioEnabled: false,
          audioGain: 0.75,
          audioPan: -0.25,
        },
        {
          ...videoClip('primary-b', seconds(4), seconds(2), seconds(6)),
          audioPan: 0.2,
        },
      ]),
      muted: true,
      transitions: [transition],
    }
    const overlay: VideoTrack = videoTrack('overlay', [{
      ...videoClip('overlay-clip', seconds(1), 0, seconds(2)),
      sourceId: silentVideoSource.id,
      speed: 2,
      transform: { x: 40, y: -20, width: 640, height: 360, fit: 'contain' },
      opacity: 0.65,
      rotationDegrees: 25,
      blendMode: 'difference',
      chromaKey: {
        enabled: true,
        color: '#00ff0080',
        similarity: 0.3,
        softness: 0.12,
        spill: 0.45,
      },
      sourceAudioEnabled: false,
      audioGain: 0.6,
      audioPan: 0.4,
    }])
    const mixedAudio: AudioTrack = {
      ...audioTrack('mixed-audio', [{
        ...audioClip('mixed-audio-clip', 0, 0, seconds(8)),
        speed: 2,
        gain: 1.25,
        pan: 0.5,
        fadeInTicks: seconds(0.3),
        fadeOutTicks: seconds(0.4),
      }]),
      solo: true,
    }
    const composition: Composition = { ...emptyComposition(), tracks: [overlay, primary, mixedAudio] }

    expect(compositionTransitionUnavailableReason(composition, primary.id, transition)).toBeNull()
    const payload = buildCompositionRenderRequest(composition).composition
    const wireOverlay = payload.tracks[0]!
    const wirePrimary = payload.tracks[1]!
    const wireAudio = payload.tracks[2]!

    expect(wireOverlay).toEqual({
      id: 'overlay',
      kind: 'video',
      name: 'overlay',
      hidden: false,
      muted: false,
      locked: false,
      transitions: [],
      clips: [{
        id: 'overlay-clip',
        sourceId: 'source-silent',
        placement: { timelineStartTick: seconds(1), sourceInTick: 0, sourceOutTick: seconds(2), speed: 2 },
        transform: {
          x: { mode: 'constant', value: 40 },
          y: { mode: 'constant', value: -20 },
          scaleX: { mode: 'constant', value: 0.5 },
          scaleY: { mode: 'constant', value: 0.5 },
          rotationDegrees: { mode: 'constant', value: 25 },
          anchorX: 0.5,
          anchorY: 0.5,
        },
        opacity: { mode: 'constant', value: 0.65 },
        blendMode: 'difference',
        effects: [{
          kind: 'chroma_key',
          color: { red: 0, green: 1, blue: 0, alpha: 128 / 255 },
          similarity: 0.3,
          softness: 0.12,
          spill: 0.45,
        }],
        sourceAudioEnabled: false,
        audioGain: { mode: 'constant', value: 0.6 },
        audioPan: { mode: 'constant', value: 0.4 },
        frameInterpolation: 'duplicate',
        playbackMode: { mode: 'forward' },
        stabilization: { mode: 'disabled' },
        enabled: true,
      }],
    })
    expect(wirePrimary).toMatchObject({
      kind: 'video',
      muted: true,
      transitions: [transition],
      clips: [
        {
          sourceAudioEnabled: false,
          audioGain: { mode: 'constant', value: 0.75 },
          audioPan: { mode: 'constant', value: -0.25 },
          transform: {
            x: { mode: 'constant', value: 0 },
            y: { mode: 'constant', value: 0 },
            scaleX: { mode: 'constant', value: 1 },
            scaleY: { mode: 'constant', value: 1 },
          },
        },
        { sourceAudioEnabled: true, audioPan: { mode: 'constant', value: 0.2 } },
      ],
    })
    expect(wireAudio).toMatchObject({
      kind: 'audio',
      muted: false,
      solo: true,
      clips: [{
        placement: { timelineStartTick: 0, sourceInTick: 0, sourceOutTick: seconds(8), speed: 2 },
        gain: { mode: 'constant', value: 1.25 },
        pan: { mode: 'constant', value: 0.5 },
        fadeInTicks: seconds(0.3),
        fadeOutTicks: seconds(0.4),
      }],
    })
  })

  it('emits authored primary-video and audio-track automation as exact AnimatableValue wire data', () => {
    const primary = videoTrack('audio-wire-primary', [{
      ...videoClip('audio-wire-video'),
      audioGain: 0.9,
      audioPan: 0.1,
      audioAnimation: {
        gain: {
          mode: 'keyframes',
          track: {
            timeBase: 1_000,
            interpolation: 'ease_in_out_cubic',
            keyframes: [{ tick: 0, value: 0.25 }, { tick: 5_000, value: 1.5 }],
          },
        },
        pan: { mode: 'constant', value: -0.35 },
      },
    }])
    const audio = audioTrack('audio-wire-track', [{
      ...audioClip('audio-wire-clip'),
      gain: 0.8,
      pan: 0.2,
      audioAnimation: {
        gain: { mode: 'constant', value: 1.25 },
        pan: {
          mode: 'keyframes',
          track: {
            timeBase: COMPOSITION_TIME_BASE,
            interpolation: 'ease_out',
            keyframes: [{ tick: 0, value: -1 }, { tick: seconds(5), value: 1 }],
          },
        },
      },
    }])
    const composition: Composition = { ...emptyComposition(), tracks: [primary, audio] }

    expect(compositionRenderUnavailableReason(composition)).toBeNull()
    const wire = buildCompositionRenderRequest(composition).composition.tracks
    const wireVideo = wire[0] as Extract<WireCompositionTrack, { kind: 'video' }>
    const wireAudio = wire[1] as Extract<WireCompositionTrack, { kind: 'audio' }>
    expect(wireVideo.clips[0]).toMatchObject({
      audioGain: {
        mode: 'keyframes',
        track: {
          timeBase: 1_000,
          interpolation: 'ease_in_out',
          keyframes: [{ tick: 0, value: 0.25 }, { tick: 5_000, value: 1.5 }],
        },
      },
      audioPan: { mode: 'constant', value: -0.35 },
    })
    expect(wireAudio.clips[0]).toMatchObject({
      gain: { mode: 'constant', value: 1.25 },
      pan: {
        mode: 'keyframes',
        track: {
          timeBase: COMPOSITION_TIME_BASE,
          interpolation: 'ease_out',
          keyframes: [{ tick: 0, value: -1 }, { tick: seconds(5), value: 1 }],
        },
      },
    })
  })

  it('maps visual keyframes and ordered chroma-then-mask effects to the exact backend wire', () => {
    const primary = videoTrack('wire-primary', [videoClip('wire-primary-clip')])
    const overlayClip: VideoClip = {
      ...videoClip('wire-overlay-clip', seconds(1), 0, seconds(4)),
      sourceId: silentVideoSource.id,
      sourceAudioEnabled: false,
      transform: { x: 10, y: 20, width: 640, height: 360, fit: 'contain' },
      opacity: 0.8,
      chromaKey: {
        enabled: true,
        color: '#00ff00',
        similarity: 0.3,
        softness: 0.1,
        spill: 0.2,
      },
      animation: {
        x: {
          mode: 'keyframes',
          track: {
            timeBase: 1_000,
            interpolation: 'ease_in_out_cubic',
            keyframes: [{ tick: 0, value: 10 }, { tick: 2_000, value: 110 }],
          },
        },
        opacity: {
          mode: 'keyframes',
          track: {
            timeBase: COMPOSITION_TIME_BASE,
            interpolation: 'ease_out',
            keyframes: [{ tick: 0, value: 0.2 }, { tick: seconds(4), value: 0.8 }],
          },
        },
      },
      masks: [
        {
          id: 'wire-mask-rectangle',
          shape: 'rectangle',
          x: {
            mode: 'keyframes',
            track: {
              timeBase: 1_000,
              interpolation: 'hold',
              keyframes: [{ tick: 0, value: 0.4 }, { tick: 1_000, value: 0.6 }],
            },
          },
          y: { mode: 'constant', value: 0.5 },
          width: { mode: 'constant', value: 0.8 },
        height: { mode: 'constant', value: 0.6 },
        rotationDegrees: { mode: 'constant', value: 0 },
        feather: 0.2,
          inverted: false,
        },
        {
          id: 'wire-mask-ellipse',
          shape: 'ellipse',
          x: { mode: 'constant', value: 0.5 },
          y: { mode: 'constant', value: 0.5 },
          width: { mode: 'constant', value: 0.4 },
        height: { mode: 'constant', value: 0.4 },
        rotationDegrees: { mode: 'constant', value: 0 },
        feather: 0,
          inverted: true,
        },
      ],
    }
    const composition: Composition = {
      ...emptyComposition(),
      tracks: [videoTrack('wire-overlay', [overlayClip]), primary],
    }

    expect(compositionRenderUnavailableReason(composition)).toBeNull()
    const wire = buildCompositionRenderRequest(composition).composition.tracks
      .find((track): track is Extract<WireCompositionTrack, { kind: 'video' }> => track.kind === 'video' && track.id === 'wire-overlay')!
    const clip = wire.clips[0]!

    expect(clip.transform).toEqual({
      x: {
        mode: 'keyframes',
        track: {
          timeBase: 1_000,
          interpolation: 'ease_in_out',
          keyframes: [{ tick: 0, value: 10 }, { tick: 2_000, value: 110 }],
        },
      },
      y: { mode: 'constant', value: 20 },
      scaleX: { mode: 'constant', value: 0.5 },
      scaleY: { mode: 'constant', value: 0.5 },
      rotationDegrees: { mode: 'constant', value: 0 },
      anchorX: 0.5,
      anchorY: 0.5,
    })
    expect(clip.opacity).toEqual(overlayClip.animation!.opacity)
    expect(clip.effects).toEqual([
      {
        kind: 'chroma_key',
        color: { red: 0, green: 1, blue: 0, alpha: 1 },
        similarity: 0.3,
        softness: 0.1,
        spill: 0.2,
      },
      {
        kind: 'mask',
        shape: 'rectangle',
        x: {
          mode: 'keyframes',
          track: {
            timeBase: 1_000,
            interpolation: 'hold',
            keyframes: [{ tick: 0, value: 0.4 }, { tick: 1_000, value: 0.6 }],
          },
        },
        y: { mode: 'constant', value: 0.5 },
        width: { mode: 'constant', value: 0.8 },
        height: { mode: 'constant', value: 0.6 },
        rotationDegrees: { mode: 'constant', value: 0 },
        feather: 0.2,
        inverted: false,
      },
      {
        kind: 'mask',
        shape: 'ellipse',
        x: { mode: 'constant', value: 0.5 },
        y: { mode: 'constant', value: 0.5 },
        width: { mode: 'constant', value: 0.4 },
        height: { mode: 'constant', value: 0.4 },
        rotationDegrees: { mode: 'constant', value: 0 },
        feather: 0,
        inverted: true,
      },
    ])
    expect(clip.effects[1]).not.toHaveProperty('id')
    expect(clip.effects[2]).not.toHaveProperty('id')
  })

  it('fails closed with precise reasons for genuinely unsupported render layouts and authored data', () => {
    const clean = populatedComposition()
    const primaryIndex = clean.tracks.findIndex((track) => track.kind === 'video')
    const primary = clean.tracks[primaryIndex] as VideoTrack
    const lowestImage: Composition = {
      ...clean,
      tracks: [...clean.tracks.slice(0, primaryIndex + 1), imageTrack('lower-image', [imageClip('lower-image-clip')]), ...clean.tracks.slice(primaryIndex + 1)],
    }
    expect(compositionRenderUnavailableReason(lowestImage)).toBeNull()

    const gapped: Composition = {
      ...clean,
      tracks: clean.tracks.map((track) => track.id === primary.id
        ? { ...track, clips: [{ ...track.clips[0]!, timelineStartTicks: 1 }] }
        : track) as CompositionTrack[],
    }
    expect(compositionRenderUnavailableReason(gapped)).toBeNull()

    const transformedPrimary: Composition = {
      ...clean,
      tracks: clean.tracks.map((track) => track.id === primary.id
        ? { ...track, clips: [{ ...track.clips[0]!, rotationDegrees: 1 }] }
        : track) as CompositionTrack[],
    }
    expect(compositionRenderUnavailableReason(transformedPrimary)).toBeNull()

    const keyframed = {
      ...clean,
      tracks: clean.tracks.map((track) => track.id === 'image-track'
        ? { ...track, clips: track.clips.map((candidate) => ({ ...candidate, futureOpacity: { mode: 'keyframes', keyframes: [] } })) }
        : track),
    } as Composition
    expect(compositionRenderUnavailableReason(keyframed)).toContain('Keyframes')

    const masked = {
      ...clean,
      tracks: clean.tracks.map((track) => track.id === 'image-track'
        ? { ...track, clips: track.clips.map((candidate) => ({ ...candidate, masks: [{ kind: 'rectangle' }] })) }
        : track),
    } as Composition
    expect(compositionRenderUnavailableReason(masked)).toContain('Masks')
  })

  it('explains render-v1 authoring limitations without deleting hidden layers', () => {
    const populated = populatedComposition()
    expect(compositionRenderUnavailableReason(populated)).toBeNull()

    const renderable: Composition = {
      ...populated,
      tracks: populated.tracks.map((track) =>
        track.kind === 'image' || track.kind === 'text' ? { ...track, hidden: true } : track,
      ),
    }
    expect(compositionRenderUnavailableReason(renderable)).toBeNull()
    expect(renderable.tracks.filter((track) => track.kind === 'image' || track.kind === 'text')).toHaveLength(2)
  })

  it('sorts source registry keys while preserving authored track order', () => {
    const clean = populatedComposition()
    const reversedSources = Object.fromEntries(Object.entries(clean.sources).reverse())
    const reorderedTracks = [clean.tracks[1]!, clean.tracks[0]!, clean.tracks[2]!, clean.tracks[3]!]
    const payload = buildCompositionRenderRequest({
      ...clean,
      sources: reversedSources,
      tracks: reorderedTracks,
    })

    expect(Object.keys(payload.composition.sources)).toEqual([...Object.keys(clean.sources)].sort())
    expect(payload.composition.tracks.map((track) => track.id)).toEqual(reorderedTracks.map((track) => track.id))
  })

  it('emits all twelve canonical delivery profile wires without legacy output fields', () => {
    const composition = populatedComposition()
    expect(COMPOSITION_DELIVERY_PROFILE_OPTIONS.map(({ profile, capabilityId }) => ({ profile, capabilityId }))).toEqual([
      { profile: { container: 'mp4', codec: 'h264' }, capabilityId: null },
      { profile: { container: 'mp4', codec: 'h265' }, capabilityId: 'composition-mp4-h265' },
      { profile: { container: 'webm', codec: 'vp9' }, capabilityId: 'composition-webm-vp9' },
      { profile: { container: 'webm', codec: 'av1' }, capabilityId: 'composition-webm-av1' },
      { profile: { container: 'mov', profile: 'proxy' }, capabilityId: 'composition-mov-prores' },
      { profile: { container: 'mov', profile: 'lt' }, capabilityId: 'composition-mov-prores' },
      { profile: { container: 'mov', profile: 'standard' }, capabilityId: 'composition-mov-prores' },
      { profile: { container: 'mov', profile: 'hq' }, capabilityId: 'composition-mov-prores' },
      { profile: { container: 'audio', codec: 'mp3' }, capabilityId: 'composition-audio-mp3' },
      { profile: { container: 'audio', codec: 'wav' }, capabilityId: 'composition-audio-wav' },
      { profile: { container: 'audio', codec: 'aac' }, capabilityId: 'composition-audio-aac' },
      { profile: { container: 'audio', codec: 'flac' }, capabilityId: 'composition-audio-flac' },
    ])
    for (const option of COMPOSITION_DELIVERY_PROFILE_OPTIONS) {
      const output = buildCompositionRenderRequest(composition, {
        profile: option.profile,
        qualityTier: 'compact',
      }).output
      expect(output).toEqual({ profile: option.profile, qualityTier: 'compact' })
      expect(Object.keys(output).sort()).toEqual(['profile', 'qualityTier'])
      expect(output).not.toHaveProperty('format')
      expect(output).not.toHaveProperty('codec')
    }
  })

  it('migrates absent and legacy output state to the unchanged MP4 H.264 default', () => {
    expect(normalizeCompositionRenderOutput(undefined)).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
    })
    expect(normalizeCompositionRenderOutput({ qualityTier: 'high' })).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'high',
    })
    expect(normalizeCompositionRenderOutput({
      format: 'mp4',
      codec: 'h264',
      qualityTier: 'compact',
    })).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'compact',
    })
    expect(normalizeCompositionRenderOutput({
      profile: { container: 'mov', profile: 'hq' },
      qualityTier: 'high',
    })).toEqual({
      profile: { container: 'mov', profile: 'hq' },
      qualityTier: 'high',
    })
    expect(normalizeCompositionRenderOutput({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'high',
      videoBitrateKbps: 18_000,
    })).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'high',
      videoBitrateKbps: 18_000,
    })
  })

  it('carries a target file size through the payload and rejects conflicting knobs', () => {
    expect(buildCompositionRenderRequest(populatedComposition(), {
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
      targetSizeBytes: 25_000_000,
    }).output).toMatchObject({ targetSizeBytes: 25_000_000 })
    expect(() => buildCompositionRenderRequest(populatedComposition(), {
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
      targetSizeBytes: 25_000_000,
      videoBitrateKbps: 12_000,
    })).toThrow(CompositionPayloadError)
    expect(() => buildCompositionRenderRequest(populatedComposition(), {
      profile: { container: 'audio', codec: 'mp3' },
      qualityTier: 'medium',
      targetSizeBytes: 25_000_000,
    })).toThrow(CompositionPayloadError)
    expect(() => buildCompositionRenderRequest(populatedComposition(), {
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
      targetSizeBytes: 0,
    })).toThrow(CompositionPayloadError)
    expect(normalizeCompositionRenderOutput({
      profile: { container: 'webm', codec: 'vp9' },
      qualityTier: 'compact',
      targetSizeBytes: 8_000_000,
    })).toEqual({
      profile: { container: 'webm', codec: 'vp9' },
      qualityTier: 'compact',
      targetSizeBytes: 8_000_000,
    })
    // Persisted state claiming both keeps the explicit bitrate and drops the target.
    expect(normalizeCompositionRenderOutput({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
      videoBitrateKbps: 12_000,
      targetSizeBytes: 8_000_000,
    })).toEqual({
      profile: { container: 'mp4', codec: 'h264' },
      qualityTier: 'medium',
      videoBitrateKbps: 12_000,
    })
  })

  it('mirrors the backend size-to-bitrate arithmetic', () => {
    // 400_000 bytes over 4 s is 800 Kbps total; the muxed audio stream keeps 192.
    expect(targetVideoBitrateKbps(400_000, 4)).toBe(608)
    // Floored, never rounded up, so the render leans under the target.
    expect(targetVideoBitrateKbps(1_096_499, 4)).toBe(2_000)
    expect(targetVideoBitrateKbps(1_097_000, 4)).toBe(2_002)
    // A budget the audio stream alone consumes leaves nothing for video.
    expect(targetVideoBitrateKbps(96_000, 4)).toBe(0)
    expect(targetVideoBitrateKbps(1_000_000, 0)).toBe(0)
  })

  it('rejects an empty timeline and unsupported output at runtime', () => {
    expect(() => buildCompositionRenderRequest(emptyComposition())).toThrow(CompositionPayloadError)
    expect(() =>
      buildCompositionRenderRequest(populatedComposition(), {
        profile: { container: 'webm', codec: 'h264' },
        qualityTier: 'medium',
      } as unknown as Parameters<typeof buildCompositionRenderRequest>[1]),
    ).toThrow(CompositionPayloadError)
    expect(buildCompositionRenderRequest(populatedComposition(), {
      profile: { container: 'webm', codec: 'vp9' },
      qualityTier: 'medium',
      videoBitrateKbps: 12_000,
    }).output).toMatchObject({ videoBitrateKbps: 12_000 })
    expect(() => buildCompositionRenderRequest(populatedComposition(), {
      profile: { container: 'audio', codec: 'mp3' },
      qualityTier: 'medium',
      videoBitrateKbps: 12_000,
    })).toThrow(CompositionPayloadError)
    expect(() =>
      buildCompositionRenderRequest(populatedComposition(), {
        profile: { container: 'mp4', codec: 'h264' },
        qualityTier: 'medium',
        format: 'mp4',
      } as unknown as Parameters<typeof buildCompositionRenderRequest>[1]),
    ).toThrow(CompositionPayloadError)
    expect(() =>
      buildCompositionRenderRequest(populatedComposition(), {
        profile: { container: 'mp4', codec: 'h264' },
        qualityTier: 'medium',
        range: { startTicks: 800_000, endTicks: 200_000 },
      }),
    ).toThrow(CompositionPayloadError)
  })
})

// Compile-time coverage for every track/clip member used by the public unions.
void ([] as CompositionClip[])
void ([] as CompositionTrack[])

it('duplicates independent style effects, mask tracks and per-key easing metadata', () => {
  const base = populatedComposition()
  const source: VideoClip = { ...videoClip(), videoEffects: [{ preset: 'blur', intensity: 0.5 }], masks: [{ id: 'copy-mask', shape: 'ellipse', x: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 0, value: 0.5, interpolation: 'ease_in' }] } }, y: { mode: 'constant', value: 0.5 }, width: { mode: 'constant', value: 0.5 }, height: { mode: 'constant', value: 0.5 }, feather: 0, inverted: false }] }
  const document = { ...base, tracks: base.tracks.map((track) => track.id === 'video-track' ? videoTrack('video-track', [source]) : track) }
  const before = JSON.stringify(document)
  const result = duplicateClip(document, source.id, { id: 'isolated-copy', timelineStartTicks: seconds(6), escapeLink: true })
  const copy = (result.tracks.find((track) => track.id === 'video-track') as VideoTrack).clips.find((clip) => clip.id === 'isolated-copy')!
  expect(copy.videoEffects).toEqual(source.videoEffects)
  expect(copy.videoEffects).not.toBe(source.videoEffects)
  expect(copy.videoEffects![0]).not.toBe(source.videoEffects![0])
  expect(copy.masks).toEqual(source.masks)
  expect(copy.masks![0]).not.toBe(source.masks![0])
  expect(copy.masks![0]!.x).not.toBe(source.masks![0]!.x)
  expect(copy.masks![0]!.x).toMatchObject({ track: { keyframes: [{ interpolation: 'ease_in' }] } })
  expect(JSON.stringify(document)).toBe(before)
})

it('preserves signed and trailing authored keys when moving a clip resnaps its extent', () => {
  const document = emptyComposition()
  const clip: VideoClip = { ...videoClip('resnap-keys', 0, 0, 1_020_000), animation: { x: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'ease_in', extrapolation: { before: 'loop', after: 'offset' }, keyframes: [{ tick: -500, value: 10, interpolation: 'hold' }, { tick: 1500, value: 20 }] } } } }
  const before = JSON.stringify(clip)
  const moved = withClipGridStart(document, clip, 33_333, true)
  if (moved.kind !== 'video') throw new Error('video required')
  expect(moved.timelineStartTicks).not.toBe(clip.timelineStartTicks)
  expect(moved.timelineDurationTicks).toBeDefined()
  expect(moved.timelineDurationTicks).not.toBe(1_020_000)
  expect(moved.animation).toEqual(clip.animation)
  expect(JSON.stringify(clip)).toBe(before)
})


describe('authored DSP chain persistence', () => {
  it('retains ordered slots through JSON and clones payload parameter maps', () => {
    const params = { gain: -6 }
    const effects = [{ id: 'amplify', params }, { id: 'invert', bypass: true }]
    const composition = { ...emptyComposition(), tracks: [audioTrack('audio', [{ ...audioClip(), dspEffects: effects }])] }
    expect(validateComposition(JSON.parse(JSON.stringify(composition)))).toEqual([])
    const request = buildCompositionRenderRequest(composition)
    expect(request.composition.tracks[0]).toMatchObject({ clips: [{ dspEffects: effects }] })
    params.gain = 12
    expect(request.composition.tracks[0]).toMatchObject({ clips: [{ dspEffects: [{ params: { gain: -6 } }, { bypass: true }] }] })
  })
  it('rejects malformed chains and preserves legacy omission', () => {
    const make = (effects: unknown) => ({ ...emptyComposition(), tracks: [audioTrack('audio', [{ ...audioClip(), dspEffects: effects } as AudioClip])] })
    for (const invalid of [null, {}, Array(17).fill({ id: 'amplify' }), [{ id: 'amplify', params: { gain: NaN } }], [{ id: 'amplify', bypass: 1 }], [{ id: 'amplify', extra: true }]]) {
      expect(issueCodes(make(invalid))).toContain('audio-dsp')
    }
    expect(validateComposition({ ...emptyComposition(), tracks: [audioTrack('audio', [audioClip()])] })).toEqual([])
  })
})

it('retains and clones authored native DSP curves through JSON and payload', () => {
  const curve = { mode: 'constant' as const, value: -6 }
  const composition = { ...emptyComposition(), tracks: [audioTrack('audio', [{ ...audioClip(), dspEffects: [{ id: 'amplify', animation: { gain: curve } }] }])] }
  expect(validateComposition(JSON.parse(JSON.stringify(composition)))).toEqual([])
  const request = buildCompositionRenderRequest(composition)
  curve.value = 3
  expect(request.composition.tracks[0]).toMatchObject({ clips: [{ dspEffects: [{ animation: { gain: { mode: 'constant', value: -6 } } }] }] })
  const invalid = { ...emptyComposition(), tracks: [audioTrack('audio', [{ ...audioClip(), dspEffects: [{ id: 'amplify', animation: { gain: { mode: 'constant', value: NaN } } }] }])] }
  expect(validateComposition(invalid).length).toBeGreaterThan(0)
})

it('rebases DSP curves on audio split while retaining off-clip endpoint handles', () => {
  const curve = { mode: 'keyframes' as const, track: { timeBase: 1000000, interpolation: 'linear' as const, keyframes: [{ tick: 0, value: -6 }, { tick: 4000000, value: 6 }] } }
  const clip = { ...audioClip(), dspEffects: [{ id: 'amplify', animation: { gain: curve } }] }
  const composition = { ...emptyComposition(), tracks: [audioTrack('audio', [clip])] }
  const split = splitClip(composition, clip.id, 2000000, 'right')
  const track = split.tracks[0] as AudioTrack
  expect(track.clips[0]?.dspEffects?.[0]?.animation?.gain).toEqual(curve)
  expect(track.clips[1]?.dspEffects?.[0]?.animation?.gain).toMatchObject({ track: { keyframes: [{ tick: -2000000, value: -6 }, { tick: 2000000, value: 6 }] } })
  expect(validateComposition(split)).toEqual([])
  expect(curve.track.keyframes[0]?.tick).toBe(0)
})

it('preserves native image effect curves across clip splitting',()=>{
 const animation={mode:'keyframes' as const,track:{timeBase:1000000,interpolation:'linear' as const,keyframes:[{tick:0,value:10},{tick:2000000,value:20}]}}
 const source=populatedComposition()
 const value:Composition={...source,tracks:source.tracks.map(track=>track.kind==='image'?{...track,clips:track.clips.map(clip=>({...clip,effects:[{kind:'gamma_correction' as const,parameters:{},animation}]}))}:track)}
 const result=splitClip(value,'image-clip',seconds(2),'image-right')
 const image=result.tracks.find(track=>track.kind==='image')!
 const left=image.clips[0]!.effects![0]!,right=image.clips[1]!.effects![0]!
 if(left.kind!=='gamma_correction'||right.kind!=='gamma_correction')throw new Error('Gamma expected')
 expect(right.animation).toMatchObject({track:{keyframes:[{tick:-1000000,value:10},{tick:1000000,value:20}]}})
 for(const time of [0,250000,1000000,1500000])expect(sampleAnimatableValue(right.animation!,time)).toBe(sampleAnimatableValue(animation,time+1000000))
 expect(left.animation).toEqual(animation)
 expect(left.animation).not.toBe(animation)
 expect(animation.track.keyframes).toEqual([{tick:0,value:10},{tick:2000000,value:20}])
 buildCompositionRenderRequest(result)
})

it('keeps Auto Reframe animation continuous through split, trim and duplicate delivery payloads',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}})
 const effect={kind:'auto_reframe' as const,parameters:{preset:2,aspect:1,zoom:175,offset:[1.123456789012,-.75] as const},animation:{zoom:make(100,400),offsetX:make(1.123456789012,2.123456789012),offsetY:make(-.75,.25)}}
 const clip:ImageClip={...imageClip(),effects:[effect]}
 const source:Composition={...emptyComposition(),tracks:[imageTrack('reframe',[clip])]}
 const before=JSON.stringify(source)
 const split=splitClip(source,clip.id,seconds(2),'reframe-right')
 const trimmed=trimClip(source,clip.id,seconds(2),seconds(4))
 const duplicated=duplicateClip(source,clip.id,{id:'reframe-copy',timelineStartTicks:seconds(5),escapeLink:true})
 for(const [document,id,offset] of [[split,'reframe-right',1000000],[trimmed,clip.id,1000000],[duplicated,'reframe-copy',0]] as const){
  expect(validateComposition(document)).toEqual([])
  const track=document.tracks[0] as ImageTrack
  const selected=track.clips.find(value=>value.id===id)!.effects![0]!
  if(selected.kind!=='auto_reframe')throw new Error('Auto Reframe required')
  expect(selected.parameters).toEqual(effect.parameters)
  expect(selected.parameters).not.toBe(effect.parameters)
  for(const channel of ['zoom','offsetX','offsetY'] as const){
   const actual=selected.animation![channel]!
   for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(actual,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12)
   expect(actual).not.toBe(effect.animation[channel])
  }
  const payload=buildCompositionRenderRequest(document)
  expect(JSON.stringify(payload)).toContain('1.123456789012')
 }
 expect(JSON.stringify(source)).toBe(before)
})

it('keeps Warp Stabilizer animation continuous through split, trim and duplicate delivery payloads',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}})
 const effect={kind:'warp_stabilizer' as const,parameters:{result:1,method:3,framing:2,preserveScale:true,detailed:true,smoothness:75},animation:{smoothness:make(0,1000),maxScale:make(100,300),actionSafe:make(0,100),additionalScale:make(50,400),cropLess:make(0,100)}}
 const clip:ImageClip={...imageClip(),effects:[effect]}
 const source:Composition={...emptyComposition(),tracks:[imageTrack('reframe',[clip])]}
 const before=JSON.stringify(source)
 const split=splitClip(source,clip.id,seconds(2),'reframe-right')
 const trimmed=trimClip(source,clip.id,seconds(2),seconds(4))
 const duplicated=duplicateClip(source,clip.id,{id:'reframe-copy',timelineStartTicks:seconds(5),escapeLink:true})
 for(const [document,id,offset] of [[split,'reframe-right',1000000],[trimmed,clip.id,1000000],[duplicated,'reframe-copy',0]] as const){
  expect(validateComposition(document)).toEqual([])
  const track=document.tracks[0] as ImageTrack
  const selected=track.clips.find(value=>value.id===id)!.effects![0]!
  if(selected.kind!=='warp_stabilizer')throw new Error('Warp Stabilizer required')
  expect(selected.parameters).toEqual(effect.parameters)
  expect(selected.parameters).not.toBe(effect.parameters)
  for(const channel of ['smoothness','maxScale','actionSafe','additionalScale','cropLess'] as const){
   const actual=selected.animation![channel]!
   for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(actual,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12)
   expect(actual).not.toBe(effect.animation[channel])
  }
  const payload=buildCompositionRenderRequest(document)
  expect(JSON.stringify(payload)).toContain('"preserveScale":true');expect(JSON.stringify(payload)).toContain('"detailed":true')
 }
 expect(JSON.stringify(source)).toBe(before)
})

it('keeps Light Leaks animation continuous through split, trim and duplicate delivery payloads',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}})
 const effect={kind:'light_leaks' as const,parameters:{mode:2,c1:[1,0.45,0.1,1] as const,c2:[1,0.15,0.35,1] as const},animation:{intensity:make(0,100),scale:make(10,1000),direction:make(-180,180),speed:make(0,20),seed:make(0,10000),c1Red:make(0,1),c1Green:make(0,1),c1Blue:make(0,1),c1Alpha:make(0,1),c2Red:make(0,1),c2Green:make(0,1),c2Blue:make(0,1),c2Alpha:make(0,1)}}
 const clip:ImageClip={...imageClip(),effects:[effect]}
 const source:Composition={...emptyComposition(),tracks:[imageTrack('reframe',[clip])]}
 const before=JSON.stringify(source)
 const split=splitClip(source,clip.id,seconds(2),'reframe-right')
 const trimmed=trimClip(source,clip.id,seconds(2),seconds(4))
 const duplicated=duplicateClip(source,clip.id,{id:'reframe-copy',timelineStartTicks:seconds(5),escapeLink:true})
 for(const [document,id,offset] of [[split,'reframe-right',1000000],[trimmed,clip.id,1000000],[duplicated,'reframe-copy',0]] as const){
  expect(validateComposition(document)).toEqual([])
  const track=document.tracks[0] as ImageTrack
  const selected=track.clips.find(value=>value.id===id)!.effects![0]!
  if(selected.kind!=='light_leaks')throw new Error('Light Leaks required')
  expect(selected.parameters).toEqual(effect.parameters)
  expect(selected.parameters).not.toBe(effect.parameters)
  for(const channel of ['intensity','scale','direction','speed','seed','c1Red','c1Green','c1Blue','c1Alpha','c2Red','c2Green','c2Blue','c2Alpha'] as const){
   const actual=selected.animation![channel]!
   for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(actual,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12)
   expect(actual).not.toBe(effect.animation[channel])
  }
  const payload=buildCompositionRenderRequest(document)
  expect(JSON.stringify(payload)).toContain('"mode":2');expect(JSON.stringify(payload)).toContain('"c2":[1,0.15,0.35,1]')
 }
 expect(JSON.stringify(source)).toBe(before)
})

it('keeps VR Projection animation continuous through split, trim and duplicate delivery payloads',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}})
 const effect={kind:'vr_projection' as const,parameters:{frameLayout:2,tilt:12.5,pan:-37.25,roll:91.75,fov:270,stretch:false},animation:{tilt:make(-90,90),pan:make(-180,180),roll:make(-720,720),fov:make(10,360)}}
 const clip:ImageClip={...imageClip(),effects:[effect]}
 const source:Composition={...emptyComposition(),tracks:[imageTrack('reframe',[clip])]}
 const before=JSON.stringify(source)
 const split=splitClip(source,clip.id,seconds(2),'reframe-right')
 const trimmed=trimClip(source,clip.id,seconds(2),seconds(4))
 const duplicated=duplicateClip(source,clip.id,{id:'reframe-copy',timelineStartTicks:seconds(5),escapeLink:true})
 for(const [document,id,offset] of [[split,'reframe-right',1000000],[trimmed,clip.id,1000000],[duplicated,'reframe-copy',0]] as const){
  expect(validateComposition(document)).toEqual([])
  const track=document.tracks[0] as ImageTrack
  const selected=track.clips.find(value=>value.id===id)!.effects![0]!
  if(selected.kind!=='vr_projection')throw new Error('VR Projection required')
  expect(selected.parameters).toEqual(effect.parameters)
  expect(selected.parameters).not.toBe(effect.parameters)
  for(const channel of ['tilt','pan','roll','fov'] as const){
   const actual=selected.animation![channel]!
   for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(actual,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12)
   expect(actual).not.toBe(effect.animation[channel])
  }
  const payload=buildCompositionRenderRequest(document)
  expect(JSON.stringify(payload)).toContain('"frameLayout":2');expect(JSON.stringify(payload)).toContain('"stretch":false')
 }
 expect(JSON.stringify(source)).toBe(before)
})

it('keeps VR Rotate Sphere animation continuous through split, trim and duplicate delivery payloads',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}})
 const effect={kind:'vr_rotate_sphere' as const,parameters:{frameLayout:2,tilt:12.5,pan:-37.25,roll:91.75},animation:{tilt:make(-90,90),pan:make(-180,180),roll:make(-720,720)}}
 const clip:ImageClip={...imageClip(),effects:[effect]}
 const source:Composition={...emptyComposition(),tracks:[imageTrack('reframe',[clip])]}
 const before=JSON.stringify(source)
 const split=splitClip(source,clip.id,seconds(2),'reframe-right')
 const trimmed=trimClip(source,clip.id,seconds(2),seconds(4))
 const duplicated=duplicateClip(source,clip.id,{id:'reframe-copy',timelineStartTicks:seconds(5),escapeLink:true})
 for(const [document,id,offset] of [[split,'reframe-right',1000000],[trimmed,clip.id,1000000],[duplicated,'reframe-copy',0]] as const){
  expect(validateComposition(document)).toEqual([])
  const track=document.tracks[0] as ImageTrack
  const selected=track.clips.find(value=>value.id===id)!.effects![0]!
  if(selected.kind!=='vr_rotate_sphere')throw new Error('VR Rotate Sphere required')
  expect(selected.parameters).toEqual(effect.parameters)
  expect(selected.parameters).not.toBe(effect.parameters)
  for(const channel of ['tilt','pan','roll'] as const){
   const actual=selected.animation![channel]!
   for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(actual,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12)
   expect(actual).not.toBe(effect.animation[channel])
  }
  const payload=buildCompositionRenderRequest(document)
  expect(JSON.stringify(payload)).toContain('"frameLayout":2');expect(JSON.stringify(payload)).not.toContain('"fov"')
 }
 expect(JSON.stringify(source)).toBe(before)
})

for(const[kind,channel]of [['vr_blur','blurriness'],['vr_sharpen','amount']]as const)it(`keeps ${kind} scalar animation continuous through split trim and duplicate`,()=>{
 const curve={mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:0},{tick:3250,value:500}]}};
 const effect=kind==='vr_blur'?{kind,parameters:{frameLayout:2,blurriness:17},animation:curve}:{kind,parameters:{frameLayout:2,amount:17},animation:curve};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('filter',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='vr_blur'&&selected.kind!=='vr_sharpen')throw new Error('VR filter required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);expect(selected.animation).not.toBe(curve);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation!,tick)).toBeCloseTo(sampleAnimatableValue(curve,tick+offset),12);const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"frameLayout":2');expect(payload).toContain(`"${channel}":17`);}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps VR De-Noise scalar animation and static modes through split trim and duplicate',()=>{
 const curve={mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:0},{tick:3250,value:100}]}};
 const effect={kind:'vr_denoise' as const,parameters:{frameLayout:2,amount:17,noiseType:1,showNoise:true},animation:curve};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('denoise',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='vr_denoise')throw new Error('VR De-Noise required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);expect(selected.animation).not.toBe(curve);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation!,tick)).toBeCloseTo(sampleAnimatableValue(curve,tick+offset),12);const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"frameLayout":2');expect(payload).toContain('"noiseType":1');expect(payload).toContain('"showNoise":true');}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps six VR chromatic curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'vr_chromatic_aberrations' as const,parameters:{frameLayout:2,inverse:true,centerY:-47.75},animation:{red:make(-100,100),green:make(-50,50),blue:make(25,-25),centerX:make(-180,180),centerY:make(-90,90),falloff:make(0,100)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='vr_chromatic_aberrations')throw new Error('VR Chromatic required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['red','green','blue','centerX','centerY','falloff']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"frameLayout":2');expect(payload).toContain('"inverse":true');}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps twenty VR gradient curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'vr_color_gradients' as const,parameters:{frameLayout:2,mode:4,c2:[.25,.5,.4,1] as const,p3Lat:-47.75},animation:{c1R:make(0,1),c1G:make(0,1),c1B:make(0,1),c1A:make(0,1),c2R:make(0,1),c2G:make(0,1),c2B:make(0,1),c2A:make(0,1),c3R:make(0,1),c3G:make(0,1),c3B:make(0,1),c3A:make(0,1),p1Lon:make(-180,180),p1Lat:make(-90,90),p2Lon:make(-180,180),p2Lat:make(-90,90),p3Lon:make(-180,180),p3Lat:make(-90,90),blend:make(1,100),opacity:make(0,100)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='vr_color_gradients')throw new Error('VR Gradients required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['c1R','c1G','c1B','c1A','c2R','c2G','c2B','c2A','c3R','c3G','c3B','c3A','p1Lon','p1Lat','p2Lon','p2Lat','p3Lon','p3Lat','blend','opacity']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"frameLayout":2');expect(payload).toContain('"mode":4');}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps six VR glitch curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'vr_digital_glitch' as const,parameters:{frameLayout:2,seed:17.75},animation:{amplitude:make(0,100),distortion:make(1,100),rate:make(0,60),color:make(0,100),scanlines:make(0,100),seed:make(0,10000)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='vr_digital_glitch')throw new Error('VR Glitch required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['amplitude','distortion','rate','color','scanlines','seed']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"frameLayout":2');expect(payload).toContain('"seed":17.75');}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps six VR fractal curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'vr_fractal_noise' as const,parameters:{frameLayout:2,fractalType:2,mode:4,invert:true,evolution:123.75},animation:{contrast:make(0,1000),brightness:make(-200,200),scale:make(10,10000),complexity:make(1,10),evolution:make(-720,720),opacity:make(0,100)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='vr_fractal_noise')throw new Error('VR Fractal required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['contrast','brightness','scale','complexity','evolution','opacity']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"frameLayout":2');expect(payload).toContain('"invert":true');}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps eight VR glow curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'vr_glow' as const,parameters:{frameLayout:2,useTint:true,tint:[.2,.7,.4,.25] as const},animation:{threshold:make(0,100),radius:make(0,1000),brightness:make(0,10),saturation:make(0,4),tintR:make(0,1),tintG:make(0,1),tintB:make(0,1),tintA:make(0,1)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='vr_glow')throw new Error('VR Glow required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['threshold','radius','brightness','saturation','tintR','tintG','tintB','tintA']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"frameLayout":2');expect(payload).toContain('"useTint":true');}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps five VR plane curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'vr_plane_to_sphere' as const,parameters:{frameLayout:2,roll:123.75},animation:{scale:make(1,180),pan:make(-180,180),tilt:make(-90,90),roll:make(-720,720),feather:make(0,100)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='vr_plane_to_sphere')throw new Error('VR Plane required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['scale','pan','tilt','roll','feather']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"frameLayout":2');expect(payload).toContain('"roll":123.75');}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps eight Simple Text curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'simple_text' as const,parameters:{text:'Привет\nעולם',position:[null,12.25] as const,font:1,alignment:2,color:[.2,.7,.4,.5] as const},animation:{positionX:make(-100,100),positionY:make(-50,50),size:make(1,1000),colorR:make(0,1),colorG:make(.1,.9),colorB:make(.2,.8),colorA:make(.3,.7),bgOpacity:make(0,100)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='simple_text')throw new Error('Simple Text required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['positionX','positionY','size','colorR','colorG','colorB','colorA','bgOpacity']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"font":1');expect(payload).toContain('"position":[null,12.25]');}
 expect(JSON.stringify(source)).toBe(before);
})

it('retains authored burn-in metadata in render payload without sharing maps',()=>{
 const metadata={sequenceName:'Seq Привет',clipNames:{clip:'Clip'},sourceNames:{source:'A001.mov'},dropFrame:true,sequenceStartFrame:107892};
 const plain={...emptyComposition(),tracks:[imageTrack('metadata',[imageClip()])]};const source={...plain,metadata:{...metadata,markers:[{id:'editor-marker',tick:1,label:'Cut'}]}};const before=JSON.stringify(source);const wire=buildCompositionRenderRequest(source).composition;
 expect(buildCompositionWireDocument(source).metadata?.markers).toEqual(source.metadata.markers);
 expect(wire.metadata).toEqual(metadata);expect(wire.metadata).not.toBe(metadata);expect(wire.metadata!.clipNames).not.toBe(metadata.clipNames);expect(wire.metadata!.sourceNames).not.toBe(metadata.sourceNames);expect(JSON.stringify(source)).toBe(before);
 expect(buildCompositionRenderRequest(plain).composition).not.toHaveProperty('metadata');
 expect(()=>buildCompositionRenderRequest({...plain,metadata:{sequenceStartFrame:0.5}})).toThrow('safe integer');
})

it('keeps eight Metadata Burn-in curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'metadata_burnin' as const,parameters:{prefix:'FILE Привет',position:[null,12.25] as const,source:5,alignment:6,color:[.2,.7,.4,.5] as const},animation:{positionX:make(-100,100),positionY:make(-50,50),size:make(1,100),colorR:make(0,1),colorG:make(.1,.9),colorB:make(.2,.8),colorA:make(.3,.7),opacity:make(0,100)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='metadata_burnin')throw new Error('Metadata Burn-in required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['positionX','positionY','size','colorR','colorG','colorB','colorA','opacity']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"source":5');expect(payload).toContain('"position":[null,12.25]');}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps Timecode curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'timecode' as const,parameters:{position:[null,12.25] as const},animation:{positionX:make(-100,100),positionY:make(-50,50),size:make(1,100),opacity:make(0,100)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='timecode')throw new Error('Timecode required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['positionX','positionY','size','opacity']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"kind":"timecode"');expect(payload).toContain('"position":[null,12.25]');}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps Clip Name curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'clip_name' as const,parameters:{position:[null,12.25] as const},animation:{positionX:make(-100,100),positionY:make(-50,50),size:make(1,100)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='clip_name')throw new Error('Clip Name required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['positionX','positionY','size']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"kind":"clip_name"');expect(payload).toContain('"position":[null,12.25]');}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps eleven Write-on curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'write_on' as const,parameters:{brush:[null,12.25] as const,style:2,color:[.2,.7,.4,.5] as const},animation:{brushX:make(-100,100),brushY:make(-50,50),size:make(0,500),colorR:make(0,1),colorG:make(.1,.9),colorB:make(.2,.8),colorA:make(.3,.7),hardness:make(0,100),opacity:make(0,100),strokeLength:make(0,30),spacing:make(.001,1)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='write_on')throw new Error('Write-on required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['brushX','brushY','size','colorR','colorG','colorB','colorA','hardness','opacity','strokeLength','spacing']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"style":2');expect(payload).toContain('"brush":[null,12.25]');}
 expect(JSON.stringify(source)).toBe(before);
})

it('keeps twenty-one Lightning curves and static choices through split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:3250,value:b}]}});
 const effect={kind:'lightning' as const,parameters:{start:[null,12.25] as const,end:[128,null] as const,mode:4,outside:[.2,.7,.4,.5] as const},animation:{startX:make(-100,100),startY:make(-50,50),endX:make(10,200),endY:make(20,100),segments:make(1,100),amplitude:make(0,100),detail:make(0,10),detailAmplitude:make(0,1),branching:make(0,1),speed:make(0,100),width:make(0,200),core:make(0,1),seed:make(0,10000),outsideR:make(0,1),outsideG:make(0,1),outsideB:make(0,1),outsideA:make(0,1),insideR:make(0,1),insideG:make(0,1),insideB:make(0,1),insideA:make(0,1)}};
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('chroma',[clip])]};const before=JSON.stringify(source);
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='lightning')throw new Error('Lightning required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters);for(const channel of ['startX','startY','endX','endY','segments','amplitude','detail','detailAmplitude','branching','speed','width','core','seed','outsideR','outsideG','outsideB','outsideA','insideR','insideG','insideB','insideA']as const){expect(selected.animation![channel]).not.toBe(effect.animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(effect.animation[channel],tick+offset),12);}const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"mode":4');expect(payload).toContain('"start":[null,12.25]');}
 expect(JSON.stringify(source)).toBe(before);
})

it('preserves all GraphicShape curves and authored path across split trim and duplicate',()=>{
 const make=(a:number,b:number)=>({mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:-250,value:a},{tick:2250,value:b}]}})
 const channels=['sizeX','sizeY','positionX','positionY','anchorX','anchorY','fillR','fillG','fillB','fillA','strokeR','strokeG','strokeB','strokeA','stroke2R','stroke2G','stroke2B','stroke2A','backgroundR','backgroundG','backgroundB','backgroundA','shadowR','shadowG','shadowB','shadowA','sides','cornerRadius','strokeWidth','stroke2Width','backgroundOpacity','backgroundSize','backgroundRadius','shadowOpacity','shadowAngle','shadowDistance','shadowSize','shadowBlur','scale','scaleWidth','rotation','opacity']as const
 const animation=Object.fromEntries(channels.map(channel=>[channel,channel==='sides'?make(3,64):make(0.25,0.75)]))
 const effect={kind:'graphic_shape' as const,parameters:{shape:3,points:[[0,0],[12,0],[0,12]]as const,uniformScale:false,stroke:true,strokeType:2,position:[1.23456789012345,24]as const},animation}
 const clip:ImageClip={...imageClip(),effects:[effect]};const source:Composition={...emptyComposition(),tracks:[imageTrack('graphics',[clip])]};const before=JSON.stringify(source)
 for(const[document,id,offset]of [[splitClip(source,clip.id,seconds(2),'right'),'right',1000000],[trimClip(source,clip.id,seconds(2),seconds(4)),clip.id,1000000],[duplicateClip(source,clip.id,{id:'copy',timelineStartTicks:seconds(5),escapeLink:true}),'copy',0]]as const){
  expect(validateComposition(document)).toEqual([]);const selected=(document.tracks[0]as ImageTrack).clips.find(c=>c.id===id)!.effects![0]!;if(selected.kind!=='graphic_shape')throw new Error('GraphicShape required');expect(selected.parameters).toEqual(effect.parameters);expect(selected.parameters).not.toBe(effect.parameters)
  for(const channel of channels){expect(selected.animation![channel]).not.toBe(animation[channel]);for(const tick of [0,250000,500000,1000000,2000000])expect(sampleAnimatableValue(selected.animation![channel]!,tick)).toBeCloseTo(sampleAnimatableValue(animation[channel]!,tick+offset),12)}
  const payload=JSON.stringify(buildCompositionRenderRequest(document));expect(payload).toContain('"points":[[0,0],[12,0],[0,12]]');expect(payload).toContain('"uniformScale":false')
 }
 expect(JSON.stringify(source)).toBe(before)
})

describe('Filmcraft authored keyframe clock preservation', () => {
  it('retains a 254016000000 Hz curve through normalization and render payloads', () => {
    const rate = 254_016_000_000
    const curve: CompositionAnimatableValue = {
      mode: 'keyframes',
      track: { timeBase: rate, interpolation: 'linear', keyframes: [
        { tick: rate + 1, value: 0.2 }, { tick: 2 * rate - 1, value: 0.8 },
      ] },
    }
    const composition: Composition = { ...emptyComposition(), tracks: [
      videoTrack('precise-video', [{ ...videoClip(), animation: { opacity: curve } }]),
    ] }
    expect(validateComposition(composition)).toEqual([])
    const restored = normalizeComposition(JSON.parse(JSON.stringify(composition)))
    expect(restored.tracks[0]).toMatchObject({ clips: [{ animation: { opacity: curve } }] })
    expect(buildCompositionRenderRequest(restored).composition.tracks[0]).toMatchObject({ clips: [{ opacity: curve }] })
    expect(sampleAnimatableValue(curve, rate + 1, rate)).toBe(0.2)
    expect(sampleAnimatableValue(curve, rate * 1.5, rate)).toBe(0.5)
    for (const timeBase of [0, Number.MAX_SAFE_INTEGER + 1]) {
      const invalid = JSON.parse(JSON.stringify(composition))
      invalid.tracks[0].clips[0].animation.opacity.track.timeBase = timeBase
      expect(issueCodes(invalid)).toContain('keyframe-time-base')
    }
  })
})

it('retains Lumetri split-tone alpha through public video effect export', () => {
  const curve = {mode: 'constant' as const, value: 0.375}
  const effect = {kind: 'lumetri' as const, parameters: {
    creative: {shadowAlpha: 0.25, highlightAlpha: 0.75},
    animation: {creative: {shadowAlpha: curve, highlightAlpha: curve}},
  }}
  const composition = normalizeComposition({...emptyComposition(), tracks: [{...videoTrack('split-tone', []), clips: [{...videoClip(), effects: [effect]}]}]})
  expect(validateComposition(composition)).toEqual([])
  const restored = normalizeComposition(JSON.parse(JSON.stringify(composition)))
  expect(restored.tracks[0]).toMatchObject({clips: [{effects: [effect]}]})
  expect(buildCompositionRenderRequest(restored).composition.tracks[0]).toMatchObject({clips:[{effects:[effect]}]})
})


it('retains imported Motion curves while public export explicitly refuses them', () => {
  const constant = (value: number) => ({mode: 'constant' as const, value})
  const filmcraftMotion = {enabled: true, scaleToFrame: false, parameters: {
    position: [constant(1.25), constant(-2.75)] as const,
    anchor: [constant(-4), constant(12)] as const,
    scale: constant(73.25), scaleWidth: constant(123.75), rotation: constant(31.5),
    uniformScale: false, antiFlicker: constant(0),
  }}
  const composition: Composition = {...emptyComposition(), tracks: [videoTrack('motion', [{...videoClip(), filmcraftMotion, effectTiming: {mode: 'filmcraft_source', holdFilters: false, reverseWhileFrozen: false}}])]}
  const restored = normalizeComposition(JSON.parse(JSON.stringify(composition)))
  expect(restored.tracks[0]).toMatchObject({clips: [{filmcraftMotion}]})
  expect(compositionRenderUnavailableReason(restored)).toContain('Motion Filmcraft')
  expect(() => buildCompositionRenderRequest(restored)).toThrow(CompositionPayloadError)
})

it('preserves overlong audio fades through payload, split and trim',()=>{
 const c={...emptyComposition(),tracks:[audioTrack('long-fades',[{...audioClip('sound',0,0,seconds(1)),fadeInTicks:seconds(2),fadeOutTicks:seconds(3)}])]}
 expect(buildCompositionRenderRequest(c).composition.tracks[0]).toMatchObject({clips:[{fadeInTicks:seconds(2),fadeOutTicks:seconds(3)}]})
 const parts=splitClip(c,'sound',seconds(0.5),'tail').tracks[0]!.clips
 expect(parts[0]).toMatchObject({fadeInTicks:seconds(2),fadeOutTicks:0});expect(parts[1]).toMatchObject({fadeInTicks:0,fadeOutTicks:seconds(3)})
 expect(trimClip(c,'sound',seconds(0.25),seconds(0.75)).tracks[0]!.clips[0]).toMatchObject({fadeInTicks:seconds(2),fadeOutTicks:seconds(3)})
})

it('preserves original normalized audio volume keys through payload and source-window editing',()=>{
 const keys=[{at:0,value:0,ease:'linear'},{at:1,value:1,ease:[0.42,-0.5,0.58,1] as const}]
 const c={...emptyComposition(),tracks:[audioTrack('ride',[{...audioClip('sound',0,0,seconds(1)),gain:0.9,concatGainKeys:keys}])]}
 expect(buildCompositionRenderRequest(c).composition.tracks[0]).toMatchObject({clips:[{concatGainKeys:keys,gain:{mode:'constant',value:0.9}}]})
 const parts=splitClip(c,'sound',seconds(0.5),'tail').tracks[0]!.clips
 expect(parts[0]).toMatchObject({concatGainKeys:[{at:0,value:0},{at:1,value:0.3125,derived:true}]})
 expect(parts[1]).toMatchObject({concatGainKeys:[{at:0,value:0.3125,derived:true},{at:1,value:1}]})
 const copy=duplicateClip(c,'sound',{id:'copy'}).tracks[0]!.clips[1]!
 expect('concatGainKeys'in copy&&copy.concatGainKeys).not.toBe(keys)
 const curve={mode:'constant' as const,value:1}
 expect(()=>buildCompositionRenderRequest({...c,tracks:[audioTrack('invalid',[{...audioClip(),concatGainKeys:keys,audioAnimation:{gain:curve}}])]})).toThrow()
})

it('retains absolute audio stream selection through payload and clip editing',()=>{
 const c={...emptyComposition(),tracks:[audioTrack('streams',[{...audioClip('sound',0,0,seconds(1)),audioStream:2}])]}
 expect(buildCompositionRenderRequest(c).composition.tracks[0]).toMatchObject({clips:[{audioStream:2}]})
 expect(splitClip(c,'sound',seconds(0.5),'tail').tracks[0]!.clips).toMatchObject([{audioStream:2},{audioStream:2}])
 expect(trimClip(c,'sound',seconds(0.1),seconds(0.9)).tracks[0]!.clips[0]).toMatchObject({audioStream:2})
 for(const audioStream of [-1,1.5,NaN,4294967296])expect(()=>buildCompositionRenderRequest({...c,tracks:[audioTrack('bad',[{...audioClip(),audioStream}])]})).toThrow()
})

it('preserves source-pose animation in ordinary video/image payloads, copies and cut windows',()=>{
 const animation:import('./types').WireConcatClipAnimation={durationTicks:seconds(1),keys:{opacity:[{at:0,value:0},{at:1,value:1}]},rest:{scaleX:1,scaleY:1,x:0,y:0,rotationDegrees:0,opacity:0},stretch:[1,1],canvas:[1920,1080]}
 for(const kind of ['video','image']as const){
  const c={...emptyComposition(),tracks:kind==='video'?[videoTrack('picture',[{...videoClip('picture',0,0,seconds(1)),concatAnimation:animation}])]:[imageTrack('picture',[{...imageClip('picture',0,seconds(1)),concatAnimation:animation}])]}
  const payload=buildCompositionRenderRequest(c);expect(payload.composition.tracks[0]).toMatchObject({clips:[{transform:{concatAnimation:animation}}]})
  const parts=splitClip(c,'picture',seconds(0.5),'tail').tracks[0]!.clips
  expect(parts[0]).toMatchObject({concatAnimation:{durationTicks:seconds(0.5),keys:{opacity:[{at:0,value:0},{at:1,value:0.5}]}}})
  expect(parts[1]).toMatchObject({concatAnimation:{durationTicks:seconds(0.5),keys:{opacity:[{at:0,value:0.5},{at:1,value:1}]}}})
  expect(trimClip(c,'picture',seconds(0.25),seconds(0.75)).tracks[0]!.clips[0]).toMatchObject({concatAnimation:{durationTicks:seconds(0.5),keys:{opacity:[{at:0,value:0.25},{at:1,value:0.75}]}}})
  const copy=duplicateClip(c,'picture',{id:'copy'}).tracks[0]!.clips[1]!
  expect('concatAnimation'in copy&&copy.concatAnimation).not.toBe(animation)
  expect(()=>buildCompositionRenderRequest({...c,tracks:kind==='video'?[videoTrack('picture',[{...videoClip(),concatAnimation:{...animation,durationTicks:0}}])]:[imageTrack('picture',[{...imageClip(),concatAnimation:{...animation,durationTicks:0}}])]})).toThrow()
 }
})

it('preserves portable per-clip Concat video cutout identity in render payloads',()=>{
 const binding={subject:'person' as const,sourceHash:'a'.repeat(64),modelHash:'88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',runtimeHash:'b'.repeat(64)}
 const document={...emptyComposition(),tracks:[videoTrack('video',[{...videoClip(),concatCutout:binding}])]}
 expect(buildCompositionRenderRequest(document).composition.tracks[0]).toMatchObject({clips:[{concatCutout:binding}]})
 expect(()=>buildCompositionRenderRequest({...document,tracks:[videoTrack('video',[{...videoClip(),concatCutout:{...binding,runtimeHash:'broken'}}])]})).toThrow()
})

it('preserves each supported video cutout family and refuses incompatible subjects',()=>{
 const person='88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',selfie='3241ac4ad8aa35bdaf33946776db29f7c283a413aa0b0dacb9483594b4531aad',object='60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a'
 const request=(subject:'auto'|'person'|'object',modelHash:string)=>buildCompositionRenderRequest({...emptyComposition(),tracks:[videoTrack('video',[{...videoClip(),concatCutout:{subject,modelHash,sourceHash:'a'.repeat(64),runtimeHash:'b'.repeat(64)}}])]})
 for(const model of [person,selfie,object])expect(request('auto',model).composition.tracks[0]).toMatchObject({clips:[{concatCutout:{subject:'auto',modelHash:model}}]})
 for(const model of [person,selfie])expect(request('person',model).composition.tracks[0]).toMatchObject({clips:[{concatCutout:{subject:'person',modelHash:model}}]})
 expect(request('object',object).composition.tracks[0]).toMatchObject({clips:[{concatCutout:{subject:'object',modelHash:object}}]})
 for(const model of [person,selfie])expect(()=>request('object',model)).toThrow()
 expect(()=>request('person',object)).toThrow()
 expect(()=>request('auto','f'.repeat(64))).toThrow()
})

it('preserves bounded per-clip video softness while retaining legacy bindings',()=>{
 const binding={subject:'person' as const,sourceHash:'a'.repeat(64),modelHash:'88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',runtimeHash:'b'.repeat(64)}
 for(const feather of [undefined,0,0.01,0.1]){const cutout={...binding,...(feather===undefined?{}:{feather})};expect(buildCompositionRenderRequest({...emptyComposition(),tracks:[videoTrack('video',[{...videoClip(),concatCutout:cutout}])]}).composition.tracks[0]).toMatchObject({clips:[{concatCutout:cutout}]})}
 for(const feather of [-0.001,0.101,Number.NaN,Number.POSITIVE_INFINITY])expect(()=>buildCompositionRenderRequest({...emptyComposition(),tracks:[videoTrack('video',[{...videoClip(),concatCutout:{...binding,feather}}])]})).toThrow()
})

it('preserves source-bound temporal smart regions and rejects mismatched tracks',()=>{
 const base=registerSource(emptyComposition(),{id:'region-video',kind:'video',durationTicks:seconds(20),width:256,height:256,hasAudio:false})
 const snapshot={sourceId:videoSource.id,maskSourceId:'region-video',sourceHash:'a'.repeat(64),maskHash:'b'.repeat(64),maskFileHash:'c'.repeat(64),encoderHash:'d'.repeat(64),decoderHash:'e'.repeat(64),runtimeHash:'f'.repeat(64),promptHash:'0'.repeat(64),points:[[0.25,0.5] as [number,number]],width:1920,height:1080,frames:600,frameRate:{numerator:30,denominator:1},seedFrame:0,trackedFrames:600,forwardLostAt:null,backwardLostAt:null}
 const binding={subject:'person' as const,sourceHash:'a'.repeat(64),modelHash:'88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',runtimeHash:'b'.repeat(64),correction:{custom:true,feather:0,strokes:[{tool:'smartBrush' as const,size:0.1,points:snapshot.points,at:0}]},regionTracks:[{stroke:0,snapshot}]}
 const payload=(value:typeof binding)=>buildCompositionRenderRequest({...base,tracks:[videoTrack('video',[{...videoClip(),concatCutout:value}])]})
 expect(payload(binding).composition.tracks[0]).toMatchObject({clips:[{concatCutout:binding}]})
 for(const changed of [{...binding,regionTracks:[]},{...binding,regionTracks:[...binding.regionTracks,...binding.regionTracks]},{...binding,regionTracks:[{stroke:0,snapshot:{...snapshot,sourceHash:'c'.repeat(64)}}]},{...binding,regionTracks:[{stroke:0,snapshot:{...snapshot,sourceId:'other'}}]}])expect(()=>payload(changed)).toThrow()
})

it('preserves VFR smart provenance and validates the seed by held source PTS',()=>{
 const base=registerSource(emptyComposition(),{id:'vfr-region',kind:'video',durationTicks:seconds(.5),width:256,height:256,hasAudio:false})
 const region={sourceId:videoSource.id,maskSourceId:'vfr-region',sourceHash:'a'.repeat(64),maskHash:'b'.repeat(64),maskFileHash:'c'.repeat(64),encoderHash:'d'.repeat(64),decoderHash:'e'.repeat(64),runtimeHash:'f'.repeat(64),promptHash:'0'.repeat(64),points:[[.25,.5] as [number,number]],width:1920,height:1080,frames:3,frameRate:{numerator:10,denominator:1},frameTimesUs:[0,100000,400000,500000],seedFrame:1,trackedFrames:3,forwardLostAt:null,backwardLostAt:null}
 const binding={subject:'person' as const,sourceHash:'a'.repeat(64),modelHash:'88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',runtimeHash:'b'.repeat(64),correction:{custom:true,feather:0,strokes:[{tool:'smartBrush' as const,size:.1,points:region.points,at:.25}]},regionTracks:[{stroke:0,snapshot:region}]}
 const payload=(value:typeof binding)=>buildCompositionRenderRequest({...base,tracks:[videoTrack('video',[{...videoClip(),concatCutout:value}])]})
 expect(payload(binding).composition.tracks[0]).toMatchObject({clips:[{concatCutout:binding}]})
 expect(()=>payload({...binding,correction:{...binding.correction,strokes:[{...binding.correction.strokes[0]!,at:.4}]}})).toThrow()
})


it('validates persisted subtitle lane roles without requiring stale advisory ids to exist', () => {
  const document = emptyComposition()
  for (const ids of ['track', null, [42], ['bad id'], ['lane', 'lane'], Array.from({length: MAX_COMPOSITION_TRACKS + 1}, (_, i) => `lane-${i}`)]) {
    expect(validateComposition({...document, metadata: {captionTextTracks: ids}} as unknown as Composition)
      .some(issue => issue.code === 'caption-text-tracks')).toBe(true)
  }
  expect(validateComposition({...document, metadata: {captionTextTracks: ['previous-lane']}})).toEqual([])
})
