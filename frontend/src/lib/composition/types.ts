import type { MulticamGroup } from './multicam'
import { MAX_SPEED_RAMP_POINTS, speedRampTimelineDurationTicks } from './speedRamp'

export { MAX_SPEED_RAMP_POINTS }

export const COMPOSITION_SCHEMA_VERSION = 1 as const
export const COMPOSITION_TIME_BASE = 1_000_000 as const

export const MAX_COMPOSITION_TRACKS = 16
export const MAX_COMPOSITION_CLIPS = 512
export const MAX_COMPOSITION_SOURCES = 32
export const MAX_VISIBLE_VISUAL_TRACKS = 8
export const MAX_COMPOSITION_DURATION_TICKS = 24 * 60 * 60 * COMPOSITION_TIME_BASE
export const MAX_TEXT_LENGTH = 512
export const MAX_KEYFRAMES_PER_VALUE = 32
export const MAX_ACTIVE_VISUAL_KEYFRAMES = 2_048
export const MAX_ACTIVE_AUDIO_KEYFRAMES = 2_048
export const MAX_ACTIVE_SPEED_RAMP_SEGMENTS = 256

export type Tick = number
export type StableId = string
export type SourceKind = 'video' | 'audio' | 'image'
export type TrackKind = SourceKind | 'text'
export type VisualFit = 'contain' | 'cover'
export type CompositionBlendMode = 'normal' | 'multiply' | 'screen' | 'overlay' | 'darken' | 'lighten' | 'difference' | 'addition'
export type CompositionTransitionKind = 'dissolve' | 'fade_black' | 'wipe_left' | 'wipe_right' | 'slide_left' | 'slide_right'
export type CompositionFrameInterpolation = 'duplicate' | 'optical_flow'
export type CompositionStabilizationRadius = 16 | 32 | 48 | 64
export type CompositionStabilization =
  | { readonly mode: 'disabled' }
  | {
      readonly mode: 'deshake'
      readonly radiusX: CompositionStabilizationRadius
      readonly radiusY: CompositionStabilizationRadius
    }
export type CompositionPlaybackMode =
  | { readonly mode: 'forward' }
  | { readonly mode: 'reverse' }
  | { readonly mode: 'freeze'; readonly sourceTick: Tick }
export type CompositionInterpolation = 'hold' | 'linear' | 'ease_in' | 'ease_out' | 'ease_in_out' | 'ease_in_out_cubic'
export type CompositionVisualProperty = 'x' | 'y' | 'scaleX' | 'scaleY' | 'rotationDegrees' | 'opacity'
export type CompositionAudioProperty = 'gain' | 'pan'
export type CompositionMaskProperty = 'x' | 'y' | 'width' | 'height'
export type CompositionMaskShape = 'rectangle' | 'ellipse' | 'linear'
export type CompositionSpeedRampInterpolation = 'hold' | 'linear'
export type CompositionSpeedRampAudioPolicy = 'preserve_pitch' | 'mute'

export interface CompositionSpeedRampPoint {
  readonly sourceProgressTick: Tick
  readonly speed: number
}

export interface CompositionSpeedRamp {
  readonly interpolation: CompositionSpeedRampInterpolation
  readonly points: readonly CompositionSpeedRampPoint[]
  readonly audioPolicy?: CompositionSpeedRampAudioPolicy
}

export const COMPOSITION_BLEND_MODES: readonly CompositionBlendMode[] = [
  'normal', 'multiply', 'screen', 'overlay', 'darken', 'lighten', 'difference', 'addition',
]
export const COMPOSITION_TRANSITION_KINDS: readonly CompositionTransitionKind[] = [
  'dissolve', 'fade_black', 'wipe_left', 'wipe_right', 'slide_left', 'slide_right',
]
export const COMPOSITION_INTERPOLATIONS: readonly Exclude<CompositionInterpolation, 'ease_in_out_cubic'>[] = [
  'hold', 'linear', 'ease_in', 'ease_out', 'ease_in_out',
]
export const COMPOSITION_VISUAL_PROPERTIES: readonly CompositionVisualProperty[] = [
  'x', 'y', 'scaleX', 'scaleY', 'rotationDegrees', 'opacity',
]
export const COMPOSITION_AUDIO_PROPERTIES: readonly CompositionAudioProperty[] = ['gain', 'pan']
export const COMPOSITION_MASK_PROPERTIES: readonly CompositionMaskProperty[] = ['x', 'y', 'width', 'height']
export const COMPOSITION_STABILIZATION_RADII: readonly CompositionStabilizationRadius[] = [16, 32, 48, 64]

export interface CompositionKeyframe {
  readonly tick: Tick
  readonly value: number
}

export interface CompositionKeyframeTrack {
  readonly timeBase: number
  readonly interpolation: CompositionInterpolation
  readonly keyframes: readonly CompositionKeyframe[]
}

export type CompositionAnimatableValue =
  | { readonly mode: 'constant'; readonly value: number }
  | { readonly mode: 'keyframes'; readonly track: CompositionKeyframeTrack }

export type CompositionVisualAnimation = Readonly<Partial<Record<CompositionVisualProperty, CompositionAnimatableValue>>>
/** Optional authoring automation; omitted documents retain their static v1 gain/pan fields. */
export type CompositionAudioAnimation = Readonly<Partial<Record<CompositionAudioProperty, CompositionAnimatableValue>>>

/**
 * Untrusted authoring metadata for an id-addressed backend source. The backend
 * probes every source again before rendering; no path or URL is part of this
 * document.
 */
export interface CompositionSource {
  readonly id: StableId
  readonly kind: SourceKind
  readonly durationTicks: Tick
  readonly width: number
  readonly height: number
  readonly hasAudio: boolean
  /** Additive local probe metadata; the strict render wire deliberately omits it. */
  readonly fps?: number | null
  readonly vcodec?: string | null
  readonly acodec?: string | null
}

export type CompositionSourceRegistry = Readonly<Record<StableId, CompositionSource>>

export interface CompositionCanvas {
  readonly width: number
  readonly height: number
  readonly fps: number
  readonly backgroundColor: string
}

export interface VisualTransform {
  readonly x: number
  readonly y: number
  readonly width: number
  readonly height: number
  readonly fit: VisualFit
}

export interface SourceClipTiming {
  readonly sourceId: StableId
  readonly timelineStartTicks: Tick
  readonly sourceInTicks: Tick
  readonly sourceOutTicks: Tick
  /** Static placement speed. Missing in older v1 documents means 1x. */
  readonly speed?: number
  /** Optional source-progress speed curve; omission preserves constant-speed v1 behavior. */
  readonly speedRamp?: CompositionSpeedRamp
}

export interface CompositionChromaKey {
  readonly enabled: boolean
  readonly color: string
  readonly similarity: number
  readonly softness: number
  readonly spill: number
}

export interface CompositionVideoMask {
  readonly id: StableId
  readonly shape: CompositionMaskShape
  readonly x: CompositionAnimatableValue
  readonly y: CompositionAnimatableValue
  readonly width: CompositionAnimatableValue
  readonly height: CompositionAnimatableValue
  readonly feather: number
  readonly inverted: boolean
}

export interface VideoClip extends SourceClipTiming {
  readonly id: StableId
  readonly kind: 'video'
  readonly transform: VisualTransform
  readonly opacity: number
  readonly rotationDegrees?: number
  readonly blendMode?: CompositionBlendMode
  readonly chromaKey?: CompositionChromaKey
  readonly masks?: readonly CompositionVideoMask[]
  readonly animation?: CompositionVisualAnimation
  readonly sourceAudioEnabled: boolean
  readonly audioGain: number
  /** Static pan for embedded source audio. Missing in older v1 documents means centered. */
  readonly audioPan?: number
  /** Timeline-local automation for primary embedded audio; omission uses static audioGain/audioPan. */
  readonly audioAnimation?: CompositionAudioAnimation
  /** Missing in older v1 documents means ordinary duplicate/drop frame pacing. */
  readonly frameInterpolation?: CompositionFrameInterpolation
  /** Missing in older v1 documents means ordinary forward playback. */
  readonly playbackMode?: CompositionPlaybackMode
  /** Missing in older v1 documents means stabilization is disabled. */
  readonly stabilization?: CompositionStabilization
}

export interface AudioClip extends SourceClipTiming {
  readonly id: StableId
  readonly kind: 'audio'
  readonly gain: number
  readonly pan?: number
  /** Timeline-local automation; omission uses static gain/pan. */
  readonly audioAnimation?: CompositionAudioAnimation
  readonly fadeInTicks?: Tick
  readonly fadeOutTicks?: Tick
}

export interface ImageClip {
  readonly id: StableId
  readonly kind: 'image'
  readonly sourceId: StableId
  readonly timelineStartTicks: Tick
  readonly durationTicks: Tick
  readonly transform: VisualTransform
  readonly opacity: number
  readonly rotationDegrees?: number
  readonly blendMode?: CompositionBlendMode
  readonly animation?: CompositionVisualAnimation
}

export interface TextStyle {
  readonly fontSizePx: number
  readonly color: string
  readonly backgroundColor?: string
  readonly align: 'left' | 'center' | 'right'
  readonly fontFamily?: 'Noto Sans' | 'Arial Unicode MS' | 'DejaVu Sans' | 'Arial'
  readonly strokeColor?: string
  readonly strokeWidthPx?: number
  readonly shadowColor?: string
  readonly shadowX?: number
  readonly shadowY?: number
}

export interface TextClip {
  readonly id: StableId
  readonly kind: 'text'
  readonly timelineStartTicks: Tick
  readonly durationTicks: Tick
  readonly text: string
  readonly x: number
  readonly y: number
  readonly opacity: number
  readonly rotationDegrees?: number
  readonly style: TextStyle
  readonly animation?: CompositionVisualAnimation
}

export type CompositionClip = VideoClip | AudioClip | ImageClip | TextClip
export type VisualClip = VideoClip | ImageClip | TextClip

interface TrackBase<K extends TrackKind, C extends CompositionClip> {
  readonly id: StableId
  readonly kind: K
  readonly name: string
  readonly locked: boolean
  readonly clips: readonly C[]
}

export interface VideoTrack extends TrackBase<'video', VideoClip> {
  readonly hidden: boolean
  readonly muted: boolean
  readonly transitions?: readonly CompositionTransition[]
}

export interface AudioTrack extends TrackBase<'audio', AudioClip> {
  readonly muted: boolean
  readonly solo?: boolean
}

export interface ImageTrack extends TrackBase<'image', ImageClip> {
  readonly hidden: boolean
}

export interface TextTrack extends TrackBase<'text', TextClip> {
  readonly hidden: boolean
}

export type CompositionTrack = VideoTrack | AudioTrack | ImageTrack | TextTrack
export type VisualTrack = VideoTrack | ImageTrack | TextTrack

export interface CompositionTransition {
  readonly id: StableId
  readonly fromClipId: StableId
  readonly toClipId: StableId
  readonly durationTicks: Tick
  readonly kind: CompositionTransitionKind
}

/** Top-to-bottom track order: the lowest array index is the highest visual layer. */
export interface Composition {
  readonly schemaVersion: typeof COMPOSITION_SCHEMA_VERSION
  readonly timeBase: typeof COMPOSITION_TIME_BASE
  readonly canvas: CompositionCanvas
  readonly sources: CompositionSourceRegistry
  readonly tracks: readonly CompositionTrack[]
  /** Authoring-only live switching metadata; the render payload drops it. */
  readonly multicamGroups?: readonly MulticamGroup[]
}

export type CompositionQualityTier = 'high' | 'medium' | 'compact'

export type CompositionDeliveryProfile =
  | { readonly container: 'mp4'; readonly codec: 'h264' | 'h265' }
  | { readonly container: 'webm'; readonly codec: 'vp9' | 'av1' }
  | { readonly container: 'mov'; readonly profile: 'proxy' | 'lt' | 'standard' | 'hq' }

export type CompositionDeliveryProfileId =
  | 'mp4-h264'
  | 'mp4-h265'
  | 'webm-vp9'
  | 'webm-av1'
  | 'mov-prores-proxy'
  | 'mov-prores-lt'
  | 'mov-prores-standard'
  | 'mov-prores-hq'

export interface CompositionDeliveryProfileOption {
  readonly id: CompositionDeliveryProfileId
  readonly label: string
  readonly extension: 'mp4' | 'webm' | 'mov'
  readonly videoCodec: string
  readonly audioCodec: string
  readonly capabilityId: string | null
  readonly profile: CompositionDeliveryProfile
}

export const COMPOSITION_DELIVERY_PROFILE_OPTIONS: readonly CompositionDeliveryProfileOption[] = [
  {
    id: 'mp4-h264',
    label: 'MP4 · H.264 / AAC',
    extension: 'mp4',
    videoCodec: 'H.264',
    audioCodec: 'AAC',
    capabilityId: null,
    profile: { container: 'mp4', codec: 'h264' },
  },
  {
    id: 'mp4-h265',
    label: 'MP4 · H.265 / AAC',
    extension: 'mp4',
    videoCodec: 'H.265',
    audioCodec: 'AAC',
    capabilityId: 'composition-mp4-h265',
    profile: { container: 'mp4', codec: 'h265' },
  },
  {
    id: 'webm-vp9',
    label: 'WebM · VP9 / Opus',
    extension: 'webm',
    videoCodec: 'VP9',
    audioCodec: 'Opus',
    capabilityId: 'composition-webm-vp9',
    profile: { container: 'webm', codec: 'vp9' },
  },
  {
    id: 'webm-av1',
    label: 'WebM · AV1 / Opus',
    extension: 'webm',
    videoCodec: 'AV1',
    audioCodec: 'Opus',
    capabilityId: 'composition-webm-av1',
    profile: { container: 'webm', codec: 'av1' },
  },
  {
    id: 'mov-prores-proxy',
    label: 'MOV · ProRes Proxy / PCM',
    extension: 'mov',
    videoCodec: 'ProRes Proxy',
    audioCodec: 'PCM',
    capabilityId: 'composition-mov-prores',
    profile: { container: 'mov', profile: 'proxy' },
  },
  {
    id: 'mov-prores-lt',
    label: 'MOV · ProRes LT / PCM',
    extension: 'mov',
    videoCodec: 'ProRes LT',
    audioCodec: 'PCM',
    capabilityId: 'composition-mov-prores',
    profile: { container: 'mov', profile: 'lt' },
  },
  {
    id: 'mov-prores-standard',
    label: 'MOV · ProRes Standard / PCM',
    extension: 'mov',
    videoCodec: 'ProRes Standard',
    audioCodec: 'PCM',
    capabilityId: 'composition-mov-prores',
    profile: { container: 'mov', profile: 'standard' },
  },
  {
    id: 'mov-prores-hq',
    label: 'MOV · ProRes HQ / PCM',
    extension: 'mov',
    videoCodec: 'ProRes HQ',
    audioCodec: 'PCM',
    capabilityId: 'composition-mov-prores',
    profile: { container: 'mov', profile: 'hq' },
  },
]

export interface CompositionRenderOutput {
  readonly profile: CompositionDeliveryProfile
  readonly qualityTier: CompositionQualityTier
}

export function compositionDeliveryProfileOption(
  profile: CompositionDeliveryProfile,
): CompositionDeliveryProfileOption {
  const option = COMPOSITION_DELIVERY_PROFILE_OPTIONS.find((candidate) => (
    candidate.profile.container === profile.container &&
    ('codec' in candidate.profile
      ? 'codec' in profile && candidate.profile.codec === profile.codec
      : 'profile' in profile && candidate.profile.profile === profile.profile)
  ))
  if (!option) throw new Error('Unsupported composition delivery profile')
  return option
}

export interface WireRgba {
  readonly red: number
  readonly green: number
  readonly blue: number
  readonly alpha: number
}

export type WireAnimatableValue = CompositionAnimatableValue

export interface WireTransform {
  readonly x: WireAnimatableValue
  readonly y: WireAnimatableValue
  readonly scaleX: WireAnimatableValue
  readonly scaleY: WireAnimatableValue
  readonly rotationDegrees: WireAnimatableValue
  readonly anchorX: number
  readonly anchorY: number
}

export interface WireClipPlacement {
  readonly timelineStartTick: Tick
  readonly sourceInTick: Tick
  readonly sourceOutTick: Tick
  readonly speed: number
  readonly speedRamp?: CompositionSpeedRamp
}

export interface WireCanvas {
  readonly width: number
  readonly height: number
  readonly fpsMilli: number
  readonly background: WireRgba
}

export interface WireVideoClip {
  readonly id: StableId
  readonly sourceId: StableId
  readonly placement: WireClipPlacement
  readonly transform: WireTransform
  readonly opacity: WireAnimatableValue
  readonly blendMode: CompositionBlendMode
  readonly effects: readonly WireVideoEffect[]
  readonly sourceAudioEnabled: boolean
  readonly audioGain: WireAnimatableValue
  readonly audioPan: WireAnimatableValue
  readonly frameInterpolation: CompositionFrameInterpolation
  readonly playbackMode: CompositionPlaybackMode
  readonly stabilization: CompositionStabilization
  readonly enabled: boolean
}

export interface WireAudioClip {
  readonly id: StableId
  readonly sourceId: StableId
  readonly placement: WireClipPlacement
  readonly gain: WireAnimatableValue
  readonly pan: WireAnimatableValue
  readonly fadeInTicks: Tick
  readonly fadeOutTicks: Tick
  readonly enabled: boolean
}

export interface WireImageClip {
  readonly id: StableId
  readonly sourceId: StableId
  readonly timelineStartTick: Tick
  readonly durationTicks: Tick
  readonly transform: WireTransform
  readonly opacity: WireAnimatableValue
  readonly blendMode: CompositionBlendMode
  readonly enabled: boolean
}

export interface WireTextStyle {
  readonly fontFamily: string
  readonly fontSize: number
  readonly color: WireRgba
  readonly background: WireRgba
  readonly stroke: WireRgba
  readonly strokeWidth: number
  readonly shadow: WireRgba
  readonly shadowX: number
  readonly shadowY: number
}

export interface WireTextClip {
  readonly id: StableId
  readonly timelineStartTick: Tick
  readonly timelineEndTick: Tick
  readonly text: string
  readonly style: WireTextStyle
  readonly transform: WireTransform
  readonly opacity: WireAnimatableValue
  readonly enabled: boolean
}

export type WireVideoEffect =
  | {
      readonly kind: 'chroma_key'
      readonly color: WireRgba
      readonly similarity: number
      readonly softness: number
      readonly spill: number
    }
  | {
      readonly kind: 'mask'
      readonly shape: CompositionMaskShape
      readonly x: WireAnimatableValue
      readonly y: WireAnimatableValue
      readonly width: WireAnimatableValue
      readonly height: WireAnimatableValue
      readonly feather: number
      readonly inverted: boolean
    }

export interface WireClipTransition {
  readonly id: StableId
  readonly fromClipId: StableId
  readonly toClipId: StableId
  readonly durationTicks: Tick
  readonly kind: CompositionTransitionKind
}

export type WireCompositionTrack =
  | {
      readonly kind: 'video'
      readonly id: StableId
      readonly name: string
      readonly hidden: boolean
      readonly muted: boolean
      readonly locked: boolean
      readonly clips: readonly WireVideoClip[]
      readonly transitions: readonly WireClipTransition[]
    }
  | {
      readonly kind: 'audio'
      readonly id: StableId
      readonly name: string
      readonly muted: boolean
      readonly solo: boolean
      readonly locked: boolean
      readonly clips: readonly WireAudioClip[]
    }
  | {
      readonly kind: 'image'
      readonly id: StableId
      readonly name: string
      readonly hidden: boolean
      readonly locked: boolean
      readonly clips: readonly WireImageClip[]
    }
  | {
      readonly kind: 'text'
      readonly id: StableId
      readonly name: string
      readonly hidden: boolean
      readonly locked: boolean
      readonly clips: readonly WireTextClip[]
    }

/** Exact backend render wire shape; authoring state stays intentionally simpler. */
export interface WireComposition {
  readonly schemaVersion: typeof COMPOSITION_SCHEMA_VERSION
  readonly timeBase: typeof COMPOSITION_TIME_BASE
  readonly canvas: WireCanvas
  readonly sources: CompositionSourceRegistry
  readonly tracks: readonly WireCompositionTrack[]
}

export interface CompositionRenderRequest {
  readonly schemaVersion: typeof COMPOSITION_SCHEMA_VERSION
  readonly composition: WireComposition
  readonly output: CompositionRenderOutput
}

export function isVisualTrack(track: CompositionTrack): track is VisualTrack {
  return track.kind !== 'audio'
}

export function clipDurationTicks(clip: CompositionClip): Tick {
  return clip.kind === 'video' || clip.kind === 'audio'
    ? clip.speedRamp
      ? speedRampTimelineDurationTicks(
          clip.sourceOutTicks - clip.sourceInTicks,
          clipSpeed(clip),
          clip.speedRamp,
        )
      : Math.round((clip.sourceOutTicks - clip.sourceInTicks) / clipSpeed(clip))
    : clip.durationTicks
}

export function clipEndTicks(clip: CompositionClip): Tick {
  return clip.timelineStartTicks + clipDurationTicks(clip)
}

export function clipSpeed(clip: VideoClip | AudioClip): number {
  return clip.speed ?? 1
}
