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
  registerSource,
  reorderTrack,
  snapClipStart,
  snapTick,
  slipClip,
  splitClip,
  trimClip,
  upsertTransition,
} from './commands'
import {
  COMPOSITION_RENDER_ENDPOINT,
  CompositionPayloadError,
  buildCompositionRenderRequest,
  normalizeCompositionRenderOutput,
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
    expect(migratedAudio.clips[0]).toMatchObject({ speed: 1, pan: 0, fadeInTicks: 0, fadeOutTicks: 0 })
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

  it('validates keyframe ordering, count, clip-local duration, values, and masks', () => {
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
      'keyframe-duration',
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
    expect(issueCodes(invalidAudio)).toEqual(expect.arrayContaining(['animatable-value', 'keyframe-value', 'keyframe-duration']))
  })

  it('gates Linear masks, primary animation, and the active visual keyframe budget exactly', () => {
    const linearMask: CompositionVideoMask = {
      id: 'linear-mask',
      shape: 'linear',
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
    expect(compositionRenderUnavailableReason(linear)).toContain('Linear mask')

    const primaryAnimated: Composition = {
      ...emptyComposition(),
      tracks: [{ ...primary, clips: [{ ...primary.clips[0]!, animation: { x: keyframedConstant(0, 1) } }] }],
    }
    expect(compositionRenderUnavailableReason(primaryAnimated)).toContain('neutral constant')

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
    expect(left.animation?.x).toMatchObject({ track: { keyframes: [{ tick: 0, value: 0 }, { tick: seconds(2), value: 20 }] } })
    expect(right.animation?.x).toMatchObject({ track: { keyframes: [{ tick: 0, value: 20 }, { tick: seconds(2), value: 40 }] } })

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
      track: { keyframes: [{ tick: 0, value: 0.625 }, { tick: seconds(2), value: 1 }, { tick: seconds(4), value: 0.75 }] },
    })
    expect(trimmedVideo.masks?.[0]?.x).toMatchObject({
      track: { keyframes: [{ tick: 0, value: 0.35 }, { tick: seconds(2), value: 0.5 }, { tick: seconds(4), value: 0.65 }] },
    })
    expect(trimmed.tracks[1]).toBe(composition.tracks[1])

    const split = splitClip(composition, 'automation-audio', seconds(4), 'automation-audio-right')
    const splitAudio = (split.tracks[1] as AudioTrack).clips
    expect(splitAudio[0]!.audioAnimation?.pan).toMatchObject({
      track: { keyframes: [{ tick: 0, value: -1 }, { tick: seconds(4), value: 0 }] },
    })
    expect(splitAudio[1]!.audioAnimation?.pan).toMatchObject({
      track: { keyframes: [{ tick: 0, value: 0 }, { tick: seconds(4), value: 1 }] },
    })
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

  it('authors only exact-handle adjacent primary transitions and cleans references on delete', () => {
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
    })
    expect(serialized).not.toContain('filesystemPath')
    expect(serialized).not.toContain('file:///')
    expect(serialized).not.toContain('/private/source.mp4')
    expect(serialized).not.toContain('selectedClipId')
    expect(serialized).not.toContain('"selected"')
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
    expect(compositionRenderUnavailableReason(lowestImage)).toContain('Нижняя видимая')

    const gapped: Composition = {
      ...clean,
      tracks: clean.tracks.map((track) => track.id === primary.id
        ? { ...track, clips: [{ ...track.clips[0]!, timelineStartTicks: 1 }] }
        : track) as CompositionTrack[],
    }
    expect(compositionRenderUnavailableReason(gapped)).toContain('не содержать gaps')

    const transformedPrimary: Composition = {
      ...clean,
      tracks: clean.tracks.map((track) => track.id === primary.id
        ? { ...track, clips: [{ ...track.clips[0]!, rotationDegrees: 1 }] }
        : track) as CompositionTrack[],
    }
    expect(compositionRenderUnavailableReason(transformedPrimary)).toContain('neutral constant transform')

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

  it('emits all eight canonical delivery profile wires without legacy output fields', () => {
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
  })

  it('rejects an empty timeline and unsupported output at runtime', () => {
    expect(() => buildCompositionRenderRequest(emptyComposition())).toThrow(CompositionPayloadError)
    expect(() =>
      buildCompositionRenderRequest(populatedComposition(), {
        profile: { container: 'webm', codec: 'h264' },
        qualityTier: 'medium',
      } as unknown as Parameters<typeof buildCompositionRenderRequest>[1]),
    ).toThrow(CompositionPayloadError)
    expect(() =>
      buildCompositionRenderRequest(populatedComposition(), {
        profile: { container: 'mp4', codec: 'h264' },
        qualityTier: 'medium',
        format: 'mp4',
      } as unknown as Parameters<typeof buildCompositionRenderRequest>[1]),
    ).toThrow(CompositionPayloadError)
  })
})

// Compile-time coverage for every track/clip member used by the public unions.
void ([] as CompositionClip[])
void ([] as CompositionTrack[])
