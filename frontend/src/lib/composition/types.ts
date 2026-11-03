import type {TransitionTimeline} from './transitionTimeline'
import type {CaptionTimestamp} from './captionTime'
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
export const MAX_COLOR_KEYFRAMES_PER_VALUE = 4096
export const MAX_ACTIVE_VISUAL_KEYFRAMES = 2_048
export const MAX_ACTIVE_AUDIO_KEYFRAMES = 2_048
export const MAX_ACTIVE_SPEED_RAMP_SEGMENTS = 256

export type Tick = number
export type StableId = string
export type SourceKind = 'video' | 'audio' | 'image'
export type TrackKind = SourceKind | 'text' | 'color' | 'graphic'
export type VisualFit = 'contain' | 'cover'
export type CompositionBlendMode = 'normal' | 'multiply' | 'screen' | 'overlay' | 'darken' | 'lighten' | 'difference' | 'addition'
export type CompositionTransitionKind =
  | 'concat_dissolve' | 'concat_fade_black' | 'concat_fade_white'
  | 'procedural' | 'dissolve' | 'fade_black' | 'film_dissolve' | 'additive_dissolve' | 'fade_white'
  | 'wipe_left' | 'wipe_right' | 'wipe_up' | 'wipe_down'
  | 'smooth_left' | 'smooth_right' | 'smooth_up' | 'smooth_down'
  | 'slide_left' | 'slide_right' | 'slide_up' | 'slide_down'
  | 'circle_open' | 'circle_close'
  | 'wipe_top_left' | 'wipe_top_right' | 'wipe_bottom_left' | 'wipe_bottom_right'
  | 'vertical_open' | 'vertical_close' | 'horizontal_open' | 'horizontal_close'
export type CompositionFrameInterpolation = 'duplicate' | 'optical_flow'
export type CompositionStabilizationRadius = 16 | 32 | 48 | 64
export type CompositionVidstabShakiness = 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10
export type CompositionVidstabAccuracy = 1 | 5 | 10 | 15
export type CompositionVidstabSmoothing = 1 | 5 | 10 | 15 | 20 | 30
export type CompositionStabilization =
  | { readonly mode: 'disabled' }
  | {
      readonly mode: 'deshake'
      readonly radiusX: CompositionStabilizationRadius
      readonly radiusY: CompositionStabilizationRadius
    }
  | {
      readonly mode: 'vidstab'
      readonly shakiness: CompositionVidstabShakiness
      readonly accuracy: CompositionVidstabAccuracy
      readonly smoothing: CompositionVidstabSmoothing
    }
export type CompositionPlaybackMode =
  | { readonly mode: 'forward' }
  | { readonly mode: 'reverse' }
  | { readonly mode: 'freeze'; readonly sourceTick: Tick }
export type CompositionSimpleInterpolation = 'hold' | 'linear' | 'ease_in' | 'ease_out' | 'ease_in_out' | 'ease_in_out_cubic' | 'bounce_in' | 'bounce_out' | 'bounce_in_out'
export interface CompositionElasticInterpolation {
  readonly kind: 'elastic'
  readonly dir: 'in' | 'out' | 'in_out'
  readonly amplitude: number
  readonly period: number
}
export interface CompositionBezierInterpolation {
  readonly kind: 'bezier'
  readonly p1: readonly [number, number]
  readonly p2: readonly [number, number]
}
export type CompositionInterpolation = CompositionSimpleInterpolation | CompositionElasticInterpolation | CompositionBezierInterpolation

export type CompositionVisualProperty = 'x' | 'y' | 'scaleX' | 'scaleY' | 'rotationDegrees' | 'opacity'
export type CompositionAudioProperty = 'gain' | 'pan'
export type CompositionMaskProperty = 'x' | 'y' | 'width' | 'height' | 'rotationDegrees'
export type CompositionMaskShape = 'rectangle' | 'ellipse' | 'linear'
export type CompositionSpeedRampInterpolation = 'hold' | 'linear'
export type CompositionSpeedRampAudioPolicy = 'preserve_pitch' | 'mute'
export type CompositionVoiceEffect = 'none' | 'deep' | 'high' | 'chipmunk' | 'echo' | 'robot'
export interface CompositionAudioDucking {
  readonly thresholdDb: number
  readonly ratio: number
  readonly attackMs: number
  readonly releaseMs: number
}

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
  'concat_dissolve', 'concat_fade_black', 'concat_fade_white',
  'procedural', 'dissolve', 'fade_black', 'film_dissolve', 'additive_dissolve', 'fade_white',
  'wipe_left', 'wipe_right', 'wipe_up', 'wipe_down',
  'smooth_left', 'smooth_right', 'smooth_up', 'smooth_down',
  'slide_left', 'slide_right', 'slide_up', 'slide_down',
  'circle_open', 'circle_close',
  'wipe_top_left', 'wipe_top_right', 'wipe_bottom_left', 'wipe_bottom_right',
  'vertical_open', 'vertical_close', 'horizontal_open', 'horizontal_close',
]
export const COMPOSITION_INTERPOLATIONS: readonly Exclude<CompositionSimpleInterpolation, 'ease_in_out_cubic'>[] = [
  'hold', 'linear', 'ease_in', 'ease_out', 'ease_in_out', 'bounce_in', 'bounce_out', 'bounce_in_out',
]
export const COMPOSITION_VISUAL_PROPERTIES: readonly CompositionVisualProperty[] = [
  'x', 'y', 'scaleX', 'scaleY', 'rotationDegrees', 'opacity',
]
export const COMPOSITION_AUDIO_PROPERTIES: readonly CompositionAudioProperty[] = ['gain', 'pan']
export const COMPOSITION_VOICE_EFFECTS: readonly CompositionVoiceEffect[] = ['none', 'deep', 'high', 'chipmunk', 'echo', 'robot']
export const COMPOSITION_MASK_PROPERTIES: readonly CompositionMaskProperty[] = ['x', 'y', 'width', 'height', 'rotationDegrees']
export const COMPOSITION_STABILIZATION_RADII: readonly CompositionStabilizationRadius[] = [16, 32, 48, 64]
export const COMPOSITION_VIDSTAB_SHAKINESS: readonly CompositionVidstabShakiness[] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
export const COMPOSITION_VIDSTAB_ACCURACY: readonly CompositionVidstabAccuracy[] = [1, 5, 10, 15]
export const COMPOSITION_VIDSTAB_SMOOTHING: readonly CompositionVidstabSmoothing[] = [1, 5, 10, 15, 20, 30]

export interface CompositionTangent {
  readonly x: number
  readonly y: number
  readonly mode: 'auto' | 'free'
}
export interface CompositionKeyframeTangents {
  readonly incoming: CompositionTangent
  readonly outgoing: CompositionTangent
  readonly continuity: 'smooth' | 'broken'
}
export interface CompositionKeyframe {
  readonly tangents?: CompositionKeyframeTangents
  readonly interpolation?: CompositionInterpolation
  readonly tick: Tick
  readonly value: number
}

export type CompositionExtrapolationMode = 'hold' | 'continue' | 'loop' | 'ping_pong' | 'offset'
export interface CompositionExtrapolation { readonly before: CompositionExtrapolationMode; readonly after: CompositionExtrapolationMode }

export interface CompositionKeyframeTrack {
  readonly sampleClock?: import('./keyframeClock').KeyframeSampleClock
  readonly extrapolation?: CompositionExtrapolation
  readonly timeBase: number
  readonly interpolation: CompositionInterpolation
  readonly keyframes: readonly CompositionKeyframe[]
}

export type CompositionAnimatableValue =
  | { readonly mode: 'constant'; readonly value: number }
  | { readonly mode: 'keyframes'; readonly track: CompositionKeyframeTrack }

export interface CompositionColorKeyframe extends Omit<CompositionKeyframe,'value'> {
  readonly value: string
}
export type CompositionAnimatableColor =
  | { readonly mode: 'constant';readonly value: string }
  | { readonly mode: 'keyframes';readonly track: Omit<CompositionKeyframeTrack,'keyframes'|'sampleClock'> & { readonly keyframes: readonly CompositionColorKeyframe[] } }

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
  readonly audioChannels?: number
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
  /** Exact rate when a project uses fractional frames; fps remains its numeric value. */
  readonly fpsRate?: { readonly numerator: number; readonly denominator: number }
  readonly backgroundColor: string
  /** Missing in older projects means a solid color. */
  readonly backgroundMode?: 'color' | 'blur' | 'checker'
  /** Gaussian sigma used by the source-derived blur background. */
  readonly backgroundBlur?: number
}

export interface VisualTransform {
  readonly x: number
  readonly y: number
  readonly width: number
  readonly height: number
  readonly fit: VisualFit
}

export interface SourceClipTiming {
  /** Omission in older documents means enabled. */
  readonly locked?: boolean
  readonly enabled?: boolean
  readonly sourceId: StableId
  readonly timelineStartTicks: Tick
  readonly sourceInTicks: Tick
  readonly sourceOutTicks: Tick
  /** Authored extent independent of the source window; omission derives legacy timing. */
  readonly timelineDurationTicks?: Tick
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

export type CompositionVideoEffectPreset = 'blur' | 'pixelate' | 'vignette' | 'sharpen' | 'edge' | 'rgb_split' | 'posterize'

export interface CompositionVideoStyleEffect {
  readonly preset: CompositionVideoEffectPreset
  readonly intensity: number
}

export interface CompositionVideoMask {
  readonly id: StableId
  readonly shape: CompositionMaskShape
  readonly rotationDegrees?: CompositionAnimatableValue
  readonly x: CompositionAnimatableValue
  readonly y: CompositionAnimatableValue
  readonly width: CompositionAnimatableValue
  readonly height: CompositionAnimatableValue
  readonly feather: number
  readonly inverted: boolean
}

export interface CompositionPathPoint { readonly x: number; readonly y: number }
export interface CompositionPathNode {
  readonly id: StableId
  readonly point: CompositionPathPoint
  readonly inHandle: CompositionPathPoint
  readonly outHandle: CompositionPathPoint
  readonly segment: 'line' | 'cubic'
  readonly tangentMode: 'corner' | 'smooth' | 'auto'
}
export type CompositionPositionAnimation =
  | { readonly mode: 'xy'; readonly x: CompositionAnimatableValue; readonly y: CompositionAnimatableValue }
  | { readonly mode: 'path'; readonly path: { readonly nodes: readonly CompositionPathNode[] }; readonly progress: CompositionAnimatableValue }

export type CompositionEffectTiming = { readonly mode: 'filmcraft_source'; readonly holdFilters: boolean; readonly reverseWhileFrozen: boolean }

export interface CompositionFilmcraftMotion {
  readonly enabled: boolean
  readonly scaleToFrame: boolean
  readonly parameters: {
    readonly position: readonly [WireAnimatableValue, WireAnimatableValue] | null
    readonly anchor: readonly [WireAnimatableValue, WireAnimatableValue] | null
    readonly scale: WireAnimatableValue
    readonly scaleWidth: WireAnimatableValue
    readonly rotation: WireAnimatableValue
    readonly uniformScale: boolean
    readonly antiFlicker: WireAnimatableValue
  }
}

export interface ConcatVideoRegionTrack {readonly stroke:number;readonly snapshot:SamVideoRegionAttachment}
export interface ConcatVideoCacheOrigin {readonly projectId:string;readonly expectedRevision:number;readonly sourceId:string;readonly regionSources?:Readonly<Record<string,string>>}
export interface ConcatVideoBinding {readonly origin?:ConcatVideoCacheOrigin;readonly regionTracks?:readonly ConcatVideoRegionTrack[];readonly correction?:MaskCorrection;readonly feather?:number;readonly subject:'auto'|'person'|'object';readonly sourceHash:string;readonly modelHash:string;readonly runtimeHash:string}

export interface VideoClip extends SourceClipTiming {
  readonly concatCutout?:ConcatVideoBinding
  readonly concatAnimation?:WireConcatClipAnimation
  readonly audioStream?:number
  readonly concatGainKeys?:readonly WireConcatParamKey[]
  readonly dspEffects?:readonly CompositionDspEffect[]
  readonly audioFadeInTicks?:number
  readonly audioFadeOutTicks?:number
  readonly independentAudio?:boolean
  /** Ordered native effects, evaluated after legacy style effects and before masks. */
  readonly effects?: readonly WireVideoEffect[]
  readonly effectTiming?: CompositionEffectTiming
  readonly filmcraftMotion?: CompositionFilmcraftMotion
  readonly position?: CompositionPositionAnimation
  /** Authoring link; true requires structurally identical X/Y scale tracks. */
  readonly scaleLinked?: boolean
  readonly id: StableId
  readonly kind: 'video'
  readonly transform: VisualTransform
  readonly opacity: number
  readonly rotationDegrees?: number
  readonly blendMode?: CompositionBlendMode
  readonly chromaKey?: CompositionChromaKey
  readonly videoEffects?: readonly CompositionVideoStyleEffect[]
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

export const COMPOSITION_AUDIO_ROLES = ['dialogue', 'music', 'sfx', 'voiceover'] as const
export type CompositionAudioRole = typeof COMPOSITION_AUDIO_ROLES[number]
export interface CompositionRoleMix { readonly gainDb: number; readonly muted: boolean; readonly solo: boolean }

export interface CompositionDspEffect {
  readonly concatSourceKeys?:Readonly<Record<string,readonly WireConcatParamKey[]>>
  readonly animation?: Readonly<Record<string, CompositionAnimatableValue>>
  readonly id: string
  readonly params?: Readonly<Record<string, number>>
  readonly bypass?: boolean
}

export interface AudioClip extends SourceClipTiming {
  readonly detachedFrom?:string
  readonly concatDetachedOrder?:number
  readonly audioStream?:number
  readonly concatGainKeys?:readonly WireConcatParamKey[]
  readonly dspEffects?: readonly CompositionDspEffect[]
  /** Classification for role mixing; legacy omission means dialogue. */
  readonly role?: CompositionAudioRole
  readonly id: StableId
  readonly kind: 'audio'
  readonly gain: number
  readonly pan?: number
  /** Reverse the selected source interval without changing timeline duration. */
  readonly reversed?: boolean
  /** Timeline-local automation; omission uses static gain/pan. */
  readonly audioAnimation?: CompositionAudioAnimation
  readonly fadeInTicks?: Tick
  readonly fadeOutTicks?: Tick
  /** Deterministic export-time pitch preset; omission preserves legacy audio. */
  readonly voiceEffect?: CompositionVoiceEffect
  /** Additional duration-preserving pitch shift in semitones. */
  readonly pitchSemitones?: number
  /** Deterministic bass-to-treble tilt in dB. */
  readonly toneDb?: number
  /** Crossfade with the immediately preceding, touching clip on this audio track. */
  readonly crossfadeCurve?: 'constant_gain' | 'constant_power' | 'exponential_fade'
  readonly crossfadeInTicks?: Tick
  /** Compress this clip from the primary video's embedded-audio sidechain. */
  readonly ducking?: CompositionAudioDucking
}

export interface ImageClip {
  readonly concatSourceTiming?: {readonly sourceStart:number;readonly speed:number;readonly speedCurve?:readonly {readonly at:number;readonly speed:number}[]}
  readonly concatAnimation?:WireConcatClipAnimation
  readonly position?: CompositionPositionAnimation
  /** Authoring link; true requires structurally identical X/Y scale tracks. */
  readonly scaleLinked?: boolean
  readonly locked?: boolean
  readonly enabled?: boolean
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
  readonly effects?: readonly WireVideoEffect[]
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
  readonly colorAnimation?: CompositionAnimatableColor
  readonly position?: CompositionPositionAnimation
  /** Authoring link; true requires structurally identical X/Y scale tracks. */
  readonly scaleLinked?: boolean
  readonly locked?: boolean
  readonly enabled?: boolean
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

export interface ColorClip extends Omit<ImageClip, 'kind' | 'sourceId'> {
  readonly kind: 'color'
  readonly width: number
  readonly height: number
  readonly color: string
  readonly colorAnimation?: CompositionAnimatableColor
}

/** Source-free authored vector/text layers; backend owns evaluation and rasterization. */
export type GraphicContent =
  | (Omit<Extract<WireVideoEffect, { kind: 'graphic_shape' }>, 'kind'> & { readonly kind: 'shape' })
  | (Omit<Extract<WireVideoEffect, { kind: 'graphic_text' }>, 'kind'> & { readonly kind: 'text' })
/** Native responsive edge distances in graphic canvas pixels. */
export interface GraphicPin {
  readonly to: 'frame' | { readonly layer: number }
  readonly left: boolean
  readonly top: boolean
  readonly right: boolean
  readonly bottom: boolean
  readonly offsets: readonly [number, number, number, number]
}
export interface GraphicLayer {
  readonly originalParameters?: Readonly<Record<string,unknown>>
  readonly uid: number
  readonly enabled: boolean
  readonly content: GraphicContent
  readonly pin?: GraphicPin
}
export interface GraphicRoll {
  readonly mode: 'off' | 'roll' | 'crawlLeft' | 'crawlRight'
  readonly startOffScreen: boolean
  readonly endOffScreen: boolean
  readonly preroll: number
  readonly easeIn: number
  readonly easeOut: number
  readonly postroll: number
}
export interface GraphicResponsiveTime {
  readonly intro: number
  readonly outro: number
  readonly designIn: number
  readonly designDuration: number
}
export type GraphicTemplateMetadata = Omit<import('../api').NativeGraphicTemplate,'graphic'|'roll'|'responsiveTime'>
export interface GraphicClip extends Omit<ImageClip, 'kind' | 'sourceId' | 'blendMode'> {
  readonly kind: 'graphic'
  readonly width: number
  readonly height: number
  readonly animationOffsetUs?: number
  readonly graphic: { readonly layers: readonly GraphicLayer[] }
  readonly responsiveTime?: GraphicResponsiveTime
  readonly roll?: GraphicRoll
  readonly template?: GraphicTemplateMetadata
}
export type CompositionClip = VideoClip | AudioClip | ImageClip | TextClip | ColorClip | GraphicClip
export type VisualClip = VideoClip | ImageClip | TextClip | ColorClip | GraphicClip

interface TrackBase<K extends TrackKind, C extends CompositionClip> {
  /** Authoring state for global insertion; omitted means enabled (Filmcraft default). */
  readonly syncLock?: boolean
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

export interface ColorTrack extends TrackBase<'color', ColorClip> {
  readonly hidden: boolean
}

export interface GraphicTrack extends TrackBase<'graphic', GraphicClip> {
  readonly hidden: boolean
}

export type CompositionTrack = VideoTrack | AudioTrack | ImageTrack | TextTrack | ColorTrack | GraphicTrack
export type VisualTrack = VideoTrack | ImageTrack | TextTrack | ColorTrack | GraphicTrack

export interface CompositionTransition {
  readonly concatOverlap?:{readonly leadTicks:number;readonly durationTicks:number;readonly holdOutgoing?:boolean;readonly authoredDurationTicks?:number;readonly legacy?:'cross-fade'|'push'|'zoom'|'wipe-left'|'wipe-right'|'fade-black'|'fade-white'}
  readonly kernel?: import('./transitionCatalog').ProceduralTransitionSpec
  readonly reverse?: boolean
  readonly timeline?: TransitionTimeline
  readonly id: StableId
  readonly fromClipId: StableId | null
  readonly toClipId: StableId | null
  readonly durationTicks: Tick
  readonly kind: CompositionTransitionKind
}

/** Top-to-bottom track order: the lowest array index is the highest visual layer. */
export interface SamRegionAttachment {readonly sourceId:string;readonly maskSourceId:string;readonly sourceHash:string;readonly maskHash:string;readonly maskFileHash:string;readonly encoderHash:string;readonly decoderHash:string;readonly runtimeHash:string;readonly promptHash:string;readonly points:readonly [number,number][]}
export interface SamVideoRegionAttachment extends SamRegionAttachment {readonly width:number;readonly height:number;readonly frames:number;readonly frameRate:{readonly numerator:number;readonly denominator:number};readonly frameTimesUs?:readonly number[];readonly seedFrame:number;readonly trackedFrames:number;readonly forwardLostAt:number|null;readonly backwardLostAt:number|null}
export interface MaskStroke {tool:'brush'|'eraser'|'smartBrush'|'smartEraser';size:number;points:[number,number][];at?:number|null;region?:SamRegionAttachment}
export interface MaskCorrection {custom:boolean;strokes:MaskStroke[];feather:number}
export interface CutoutAttachment {readonly subject?:'auto'|'person'|'object';readonly sourceId:string;readonly maskSourceId:string;readonly sourceHash:string;readonly maskHash:string;readonly maskFileHash:string;readonly modelHash:string;readonly runtimeHash:string;readonly correction:MaskCorrection}
export interface VideoFrameCorrection {frame:number;correction:MaskCorrection}
export interface VideoMaskAttachment {readonly sourceId:string;readonly maskSourceId:string;readonly sourceHash:string;readonly maskHash:string;readonly maskFileHash:string;readonly width:number;readonly height:number;readonly maskWidth:number;readonly maskHeight:number;readonly frames:number;readonly frameRate:{readonly numerator:number;readonly denominator:number};readonly corrections:VideoFrameCorrection[];readonly trackedRegions?:readonly SamVideoRegionAttachment[]}
export interface MediaVersions {readonly originalSourceId:string;readonly copySourceId:string;readonly cutout?:CutoutAttachment;readonly videoCutout?:VideoMaskAttachment}
export function mediaVersionSourceIds(pair:MediaVersions|undefined):string[]{return pair?[...new Set([pair.originalSourceId,pair.copySourceId,...(pair.videoCutout?[pair.videoCutout.sourceId,pair.videoCutout.maskSourceId,...(pair.videoCutout.trackedRegions??[]).flatMap(r=>[r.sourceId,r.maskSourceId])]:[]),...(pair.cutout?[pair.cutout.sourceId,pair.cutout.maskSourceId,...pair.cutout.correction.strokes.flatMap(s=>s.region?[s.region.sourceId,s.region.maskSourceId]:[])]:[])])]:[]}
export interface CompositionMetadata {
  readonly markers?:readonly import('./markers').CompositionMarker[]
  /** Text tracks reserved for imported subtitle lanes. */
  readonly captionTextTracks?: readonly string[]
  readonly clipLinks?: Readonly<Record<string, readonly string[]>>
  /** Typed siblings representing one original Concat lane. */
  readonly concatLaneGroups?: Readonly<Record<string, readonly string[]>>
  readonly mediaVersions?: Readonly<Record<string, MediaVersions>>
  readonly sequenceName?:string
  readonly clipNames?:Readonly<Record<string,string>>
  readonly sourceNames?:Readonly<Record<string,string>>
  readonly dropFrame?:boolean
  readonly sequenceStartFrame?:number
}
export interface CaptionStyle {
  readonly font: string
  readonly size: number
  readonly color: string | readonly [number, number, number, number]
  readonly background: boolean
  readonly backgroundColor: string | readonly [number, number, number, number]
  readonly align: 'left' | 'center' | 'right'
  readonly anchor: 'top' | 'middle' | 'bottom'
  readonly margin: number
  readonly lineSpacing: number
  readonly outline: number
  readonly outlineColor: string | readonly [number, number, number, number]
}
export interface CaptionCue {
  readonly cueId?:string|null
  readonly id: string
  readonly timelineStartTick: CaptionTimestamp
  readonly timelineEndTick: CaptionTimestamp
  readonly text: string
  readonly speaker?: string | null
  readonly settings?: string
}
export interface CaptionTrack {
  readonly format?:'Subtitle'|'CEA-608'|'CEA-708'|'Teletext'
  readonly language?:string
  readonly syncLock?:boolean
  readonly vttBlocks?:readonly string[]
  readonly id: string
  readonly name: string
  readonly hidden?: boolean
  readonly locked?: boolean
  readonly style?: Partial<CaptionStyle>
  readonly clips: readonly CaptionCue[]
}
export interface Composition {
  readonly captionTracks?: readonly CaptionTrack[]
  readonly metadata?:CompositionMetadata
  readonly schemaVersion: typeof COMPOSITION_SCHEMA_VERSION
  readonly timeBase: typeof COMPOSITION_TIME_BASE
  readonly canvas: CompositionCanvas
  readonly sources: CompositionSourceRegistry
  readonly tracks: readonly CompositionTrack[]
  readonly audioRoles?: Readonly<Partial<Record<CompositionAudioRole, CompositionRoleMix>>>
  readonly links?: readonly CompositionLink[]
  /** Authoring-only live switching metadata; the render payload drops it. */
  readonly multicamGroups?: readonly MulticamGroup[]
  readonly shotReview?: { readonly threshold: number; readonly minShotTicks: number } | null
}

export interface CompositionLink { readonly id: string; readonly memberIds: readonly string[] }

export type CompositionQualityTier = 'high' | 'medium' | 'compact'

export type CompositionDeliveryProfile =
  | { readonly container: 'mp4'; readonly codec: 'h264' | 'h265' }
  | { readonly container: 'webm'; readonly codec: 'vp9' | 'av1' }
  | { readonly container: 'mov'; readonly profile: 'proxy' | 'lt' | 'standard' | 'hq' }
  | { readonly container: 'audio'; readonly codec: 'mp3' | 'wav' | 'aac' | 'flac' }

export type CompositionDeliveryProfileId =
  | 'mp4-h264'
  | 'mp4-h265'
  | 'webm-vp9'
  | 'webm-av1'
  | 'mov-prores-proxy'
  | 'mov-prores-lt'
  | 'mov-prores-standard'
  | 'mov-prores-hq'
  | 'audio-mp3'
  | 'audio-wav'
  | 'audio-aac'
  | 'audio-flac'

export interface CompositionDeliveryProfileOption {
  readonly id: CompositionDeliveryProfileId
  readonly label: string
  readonly extension: 'mp4' | 'webm' | 'mov' | 'mp3' | 'wav' | 'aac' | 'flac'
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
  {
    id: 'audio-mp3',
    label: 'Audio only · MP3 192k',
    extension: 'mp3',
    videoCodec: 'None',
    audioCodec: 'MP3',
    capabilityId: 'composition-audio-mp3',
    profile: { container: 'audio', codec: 'mp3' },
  },
  {
    id: 'audio-wav',
    label: 'Audio only · WAV PCM',
    extension: 'wav',
    videoCodec: 'None',
    audioCodec: 'PCM s16le',
    capabilityId: 'composition-audio-wav',
    profile: { container: 'audio', codec: 'wav' },
  },
  {
    id: 'audio-aac',
    label: 'Audio only · AAC 192k',
    extension: 'aac',
    videoCodec: 'None',
    audioCodec: 'AAC',
    capabilityId: 'composition-audio-aac',
    profile: { container: 'audio', codec: 'aac' },
  },
  {
    id: 'audio-flac',
    label: 'Audio only · FLAC lossless',
    extension: 'flac',
    videoCodec: 'None',
    audioCodec: 'FLAC',
    capabilityId: 'composition-audio-flac',
    profile: { container: 'audio', codec: 'flac' },
  },
]

export interface CompositionRenderOutput {
  readonly profile: CompositionDeliveryProfile
  readonly qualityTier: CompositionQualityTier
  readonly videoBitrateKbps?: number
  /** Final-file-size target; the server turns it into a video bitrate. */
  readonly targetSizeBytes?: number
  readonly range?: {
    readonly startTicks: Tick
    readonly endTicks: Tick
  }
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

export interface WireConcatClipAnimation {
 readonly durationTicks:number
 readonly keys:Readonly<Partial<Record<'scale'|'offsetX'|'offsetY'|'rotation'|'opacity',readonly WireConcatParamKey[]>>>
 readonly rest:{readonly scaleX:number;readonly scaleY:number;readonly x:number;readonly y:number;readonly rotationDegrees:number;readonly opacity:number}
 readonly stretch:readonly[number,number]
 readonly canvas:readonly[number,number]
}
export interface WireConcatPreparation {
 readonly crop:readonly[number,number,number,number]
 readonly canvas:readonly[number,number]
 readonly flipH?:boolean
 readonly flipV?:boolean
}
export interface WireTransform {
  readonly concatPreparation?:WireConcatPreparation
  readonly concatAnimation?:WireConcatClipAnimation
  readonly position?: CompositionPositionAnimation
  readonly x: WireAnimatableValue
  readonly y: WireAnimatableValue
  readonly scaleX: WireAnimatableValue
  readonly scaleY: WireAnimatableValue
  readonly rotationDegrees: WireAnimatableValue
  readonly anchorX: number
  readonly anchorY: number
}

export interface ConcatSourceDocumentClock {
  readonly start: number
  readonly duration: number
  readonly sourceStart: number
  readonly speed: number
  readonly speedCurve?: readonly {readonly at: number; readonly speed: number}[] | null
}
export interface ConcatSourceRetime {
  readonly documentClock?: ConcatSourceDocumentClock
  readonly points: readonly (readonly [number, number])[]
  readonly durationTicks: Tick
  readonly rateNum: number
  readonly rateDen: number
}

export interface WireClipPlacement {
  readonly concatSourceEof?: boolean
  readonly concatRetime?: ConcatSourceRetime
  readonly sourceHeadTimelineTick?: Tick
  readonly sourceTimelineOffsetTick?: Tick
  readonly timelineDurationTick?: Tick
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
  readonly fpsRate?: { readonly numerator: number; readonly denominator: number }
  readonly background: WireRgba
  readonly backgroundMode: 'color' | 'blur' | 'checker'
  readonly backgroundBlur: number
}

export interface WireVideoClip {
  readonly concatCutout?:ConcatVideoBinding
  readonly audioStream?:number
  readonly concatGainKeys?:readonly WireConcatParamKey[]
  readonly dspEffects?:readonly CompositionDspEffect[]
  readonly audioFadeInTicks?:number
  readonly audioFadeOutTicks?:number
  readonly independentAudio?:boolean
  readonly effectTiming?: CompositionEffectTiming
  readonly filmcraftMotion?: CompositionFilmcraftMotion
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
  readonly detachedFrom?:string
  readonly concatDetachedOrder?:number
  readonly audioStream?:number
  readonly concatGainKeys?:readonly WireConcatParamKey[]
  readonly dspEffects?: readonly CompositionDspEffect[]
  readonly roleGain?: number
  readonly id: StableId
  readonly sourceId: StableId
  readonly placement: WireClipPlacement
  readonly gain: WireAnimatableValue
  readonly pan: WireAnimatableValue
  readonly reversed: boolean
  readonly fadeInTicks: Tick
  readonly fadeOutTicks: Tick
  readonly voiceEffect: CompositionVoiceEffect
  readonly pitchSemitones: number
  readonly toneDb: number
  readonly crossfadeCurve: 'constant_gain' | 'constant_power' | 'exponential_fade'
  readonly crossfadeInTicks: Tick
  readonly ducking?: CompositionAudioDucking
  readonly enabled: boolean
}

export interface WireImageClip {
  readonly concatSourceTiming?: {readonly sourceStart:number;readonly speed:number;readonly speedCurve?:readonly {readonly at:number;readonly speed:number}[]}
  readonly id: StableId
  readonly sourceId: StableId
  readonly timelineStartTick: Tick
  readonly durationTicks: Tick
  readonly transform: WireTransform
  readonly opacity: WireAnimatableValue
  readonly blendMode: CompositionBlendMode
  readonly enabled: boolean
  readonly effects?: readonly WireVideoEffect[]
}

export interface WireTextStyle {
  readonly align?: 'left' | 'center' | 'right'
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

export interface WireConcatTitleStyle {
 readonly fontFamily?:string;readonly fontSize?:number;readonly fontWeight?:number;readonly italic?:boolean;readonly color?:string;readonly align?:'left'|'center'|'right';readonly opacity?:number
 readonly strokeWidth?:number;readonly strokeColor?:string;readonly shadow?:boolean;readonly background?:string;readonly backgroundRadius?:number;readonly backgroundPaddingX?:number;readonly backgroundPaddingY?:number
 readonly lineHeight?:number;readonly tracking?:number;readonly maxWidth?:number;readonly maxHeight?:number
}
export interface WireTextClip {
  readonly effects?:readonly WireVideoEffect[]
  readonly blendMode?:CompositionBlendMode
  readonly concatTitle?:WireConcatTitleStyle
  readonly id: StableId
  readonly timelineStartTick: Tick
  readonly timelineEndTick: Tick
  readonly text: string
  readonly style: WireTextStyle
  readonly colorAnimation?: import('../api').NativeColorTrack
  readonly transform: WireTransform
  readonly opacity: WireAnimatableValue
  readonly enabled: boolean
}

export interface WireColorWheel { readonly x?: number; readonly y?: number; readonly lightness?: number }
export interface WireCreativeGrade { readonly look?: number; readonly intensity?: number; readonly shadows?: WireColorWheel; readonly midtones?: WireColorWheel; readonly highlights?: WireColorWheel }
export type WireCreativeAnimation = Partial<Record<'intensity' | 'shadowsX' | 'shadowsY' | 'shadowsLightness' | 'midtonesX' | 'midtonesY' | 'midtonesLightness' | 'highlightsX' | 'highlightsY' | 'highlightsLightness', WireAnimatableValue>>
export interface WireColorLut { readonly format: 'cube' | 'three_dl'; readonly text: string; readonly intensity?: number }
export interface WireCreativeAdjust {readonly enabled?:boolean;readonly fadedFilm?:number;readonly vibrance?:number;readonly saturation?:number;readonly shadowRed?:number;readonly shadowGreen?:number;readonly shadowBlue?:number;readonly shadowAlpha?:number;readonly highlightRed?:number;readonly highlightGreen?:number;readonly highlightBlue?:number;readonly highlightAlpha?:number;}
export type WireAdjustChannel='fadedFilm'|'vibrance'|'saturation'|'shadowRed'|'shadowGreen'|'shadowBlue'|'shadowAlpha'|'highlightRed'|'highlightGreen'|'highlightBlue'|'highlightAlpha'
export type WireAdjustAnimation=Partial<Record<WireAdjustChannel,WireAnimatableValue>>
export interface WireBasicGrade {readonly enabled?:boolean;readonly temperature?:number;readonly tint?:number;readonly exposure?:number;readonly contrast?:number;readonly highlights?:number;readonly shadows?:number;readonly whites?:number;readonly blacks?:number;readonly saturation?:number;}
export type WireBasicChannel='temperature'|'tint'|'exposure'|'contrast'|'highlights'|'shadows'|'whites'|'blacks'|'saturation'
export type WireBasicAnimation=Partial<Record<WireBasicChannel,WireAnimatableValue>>
export interface WireColorVignette { readonly enabled?:boolean; readonly amount?:number; readonly midpoint?:number; readonly roundness?:number; readonly feather?:number }
export type WireVignetteChannel='amount'|'midpoint'|'roundness'|'feather'
export type WireVignetteAnimation=Partial<Record<WireVignetteChannel,WireAnimatableValue>>
export interface WireHslSecondary { readonly enabled?: boolean; readonly showMask?: number; readonly hue?: number; readonly hueRange?: number; readonly saturationMin?: number; readonly lightnessMin?: number; readonly lightnessMax?: number; readonly soften?: number; readonly denoise?: number; readonly blur?: number; readonly temperature?: number; readonly tint?: number; readonly saturation?: number; readonly hueShift?: number; }
export type WireHslChannel = 'hue' | 'hueRange' | 'saturationMin' | 'lightnessMin' | 'lightnessMax' | 'soften' | 'denoise' | 'blur' | 'temperature' | 'tint' | 'saturation' | 'hueShift'
export type WireHslAnimation = Partial<Record<WireHslChannel,WireAnimatableValue>>
export interface WireToneCurves { readonly master?: readonly (readonly [number, number])[]; readonly red?: readonly (readonly [number, number])[]; readonly green?: readonly (readonly [number, number])[]; readonly blue?: readonly (readonly [number, number])[]; readonly hueVsSat?: readonly (readonly [number, number])[]; readonly hueVsHue?: readonly (readonly [number, number])[]; readonly hueVsLuma?: readonly (readonly [number, number])[]; readonly lumaVsSat?: readonly (readonly [number, number])[]; readonly satVsSat?: readonly (readonly [number, number])[] }
export interface WireLumetriAnimation {readonly basic?:WireBasicAnimation;readonly creative?:WireAdjustAnimation;readonly vignette?:WireVignetteAnimation;readonly grade?:WireCreativeAnimation;readonly secondary?:WireHslAnimation;readonly sharpen?:WireAnimatableValue|null}
export interface WireLumetri {readonly inputLutLibrary?:string|null;readonly lookLutLibrary?:string|null;readonly inputLutBuiltin?:string|null;readonly lookLutBuiltin?:string|null;readonly animation?:WireLumetriAnimation|null;readonly enabled?:boolean;readonly basic?:WireBasicGrade;readonly creative?:WireCreativeAdjust;readonly vignette?:WireColorVignette;readonly grade?:WireCreativeGrade;readonly curves?:WireToneCurves;readonly secondary?:WireHslSecondary;readonly sharpen?:{readonly amount?:number};readonly curvesOn?:boolean;readonly wheelsOn?:boolean;readonly inputLut?:WireColorLut|null;readonly lookLut?:WireColorLut|null}
export interface WireVideoLimiter {readonly clipLevel?:number;readonly compression?:number;readonly axis?:number;readonly gamutWarning?:boolean;readonly warningColor?:readonly [number,number,number,number]}
export interface WireCineonConverter {readonly conversion?:number;readonly black10?:number;readonly white10?:number;readonly blackInternal?:number;readonly whiteInternal?:number;readonly gamma?:number;readonly rolloff?:number}
export type WireLightingLight = {readonly kind?:0|1|2|3;readonly color?:readonly [number,number,number,number];readonly center?:readonly [number|null,number|null]|null;readonly major?:number;readonly minor?:number;readonly angle?:number;readonly intensity?:number;readonly focus?:number}
export type WireLightingChannel = "l1Major"|"l1Minor"|"l1Angle"|"l1Intensity"|"l1Focus"|"l1ColorRed"|"l1ColorGreen"|"l1ColorBlue"|"l1ColorAlpha"|"l1CenterX"|"l1CenterY"|"l2Major"|"l2Minor"|"l2Angle"|"l2Intensity"|"l2Focus"|"l2ColorRed"|"l2ColorGreen"|"l2ColorBlue"|"l2ColorAlpha"|"l2CenterX"|"l2CenterY"|"l3Major"|"l3Minor"|"l3Angle"|"l3Intensity"|"l3Focus"|"l3ColorRed"|"l3ColorGreen"|"l3ColorBlue"|"l3ColorAlpha"|"l3CenterX"|"l3CenterY"|"l4Major"|"l4Minor"|"l4Angle"|"l4Intensity"|"l4Focus"|"l4ColorRed"|"l4ColorGreen"|"l4ColorBlue"|"l4ColorAlpha"|"l4CenterX"|"l4CenterY"|"l5Major"|"l5Minor"|"l5Angle"|"l5Intensity"|"l5Focus"|"l5ColorRed"|"l5ColorGreen"|"l5ColorBlue"|"l5ColorAlpha"|"l5CenterX"|"l5CenterY"|"ambientColorRed"|"ambientColorGreen"|"ambientColorBlue"|"ambientColorAlpha"|"ambient"|"gloss"|"material"|"exposure"|"bumpHeight"
export interface WireConcatParamKey { readonly derived?:boolean; readonly at:number;readonly value:number;readonly ease?:string|readonly [number,number,number,number] }

export type WireVideoEffect =
 | {readonly kind:"native_bypass";readonly effect:WireVideoEffect}
 | {readonly kind:"concat_bypass";readonly link:{readonly id:string;readonly enabled:false;readonly params?:Readonly<Record<string,number>>;readonly keys?:Readonly<Record<string,readonly WireConcatParamKey[]>>}}
 | {readonly kind:"proc_amp";readonly parameters:{readonly brightness?:number;readonly contrast?:number;readonly hue?:number;readonly saturation?:number};readonly animation?:Partial<Record<'brightness'|'contrast'|'hue'|'saturation',WireAnimatableValue>>}
 | {readonly kind:"gamma_correction";readonly parameters:{readonly gamma?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"posterize";readonly parameters:{readonly levels?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"color_invert";readonly parameters:{readonly channel?:number;readonly blend?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"alpha_adjust";readonly parameters:{readonly opacity?:number;readonly ignore?:boolean;readonly invert?:boolean;readonly maskOnly?:boolean};readonly animation?:WireAnimatableValue}
 | {readonly kind:"directional_blur";readonly parameters:{readonly direction?:number;readonly length?:number};readonly animation?:Partial<Record<"direction"|"length",WireAnimatableValue>>}
 | {readonly kind:"directional_blur_legacy";readonly parameters:{readonly direction?:number;readonly length?:number};readonly animation?:Partial<Record<"direction"|"length",WireAnimatableValue>>}
 | {readonly kind:"emboss";readonly parameters:{readonly direction?:number;readonly relief?:number;readonly contrast?:number;readonly blend?:number};readonly animation?:Readonly<Partial<Record<"direction"|"relief"|"contrast"|"blend",WireAnimatableValue>>>}
 | {readonly kind:"noise";readonly parameters:{readonly amount?:number;readonly color?:boolean;readonly clip?:boolean;readonly grainSize?:number};readonly animation?:Readonly<Partial<Record<"amount"|"grainSize",WireAnimatableValue>>>}
 | {readonly kind:"noise_legacy";readonly parameters:{readonly amount?:number;readonly color?:boolean;readonly clip?:boolean;readonly grainSize?:number};readonly animation?:Readonly<Partial<Record<"amount"|"grainSize",WireAnimatableValue>>>}
 | {readonly kind:"strobe";readonly parameters:{readonly color?:readonly [number,number,number,number];readonly blend?:number;readonly duration?:number;readonly period?:number};readonly animation?:Readonly<Partial<Record<"colorR"|"colorG"|"colorB"|"colorA"|"blend"|"duration"|"period",WireAnimatableValue>>>}
 | {readonly kind:"twirl";readonly parameters:{readonly angle?:number;readonly radius?:number;readonly center?:readonly [number|null,number|null]|null};readonly animation?:Readonly<Partial<Record<"angle"|"radius"|"centerX"|"centerY",WireAnimatableValue>>>}
 | {readonly kind:"twirl_legacy";readonly parameters:{readonly angle?:number;readonly radius?:number;readonly center?:readonly [number|null,number|null]|null};readonly animation?:Readonly<Partial<Record<"angle"|"radius"|"centerX"|"centerY",WireAnimatableValue>>>}
 | {readonly kind:"wave_warp";readonly parameters:{readonly height?:number;readonly width?:number;readonly direction?:number;readonly speed?:number};readonly animation?:Readonly<Partial<Record<"height"|"width"|"direction"|"speed",WireAnimatableValue>>>}
 | {readonly kind:"lens_distortion";readonly parameters:{readonly curvature?:number;readonly hDecentering?:number;readonly vDecentering?:number};readonly animation?:Readonly<Partial<Record<"curvature"|"hDecentering"|"vDecentering",WireAnimatableValue>>>}
 | {readonly kind:"grid";readonly parameters:{readonly size?:number;readonly border?:number;readonly color?:readonly [number,number,number,number];readonly opacity?:number};readonly animation?:Readonly<Partial<Record<"size"|"border"|"colorR"|"colorG"|"colorB"|"colorA"|"opacity",WireAnimatableValue>>>}
 | {readonly kind:"lens_flare";readonly parameters:{readonly center?:readonly [number|null,number|null];readonly brightness?:number;readonly blend?:number;readonly lensType?:number;readonly color?:readonly [number,number,number,number];readonly size?:number;readonly ghosts?:number};readonly animation?:Readonly<Partial<Record<"centerX"|"centerY"|"brightness"|"blend"|"size"|"ghosts"|"colorR"|"colorG"|"colorB"|"colorA",WireAnimatableValue>>>}
 | {readonly kind:"ellipse";readonly parameters:{readonly center?:readonly [number|null,number|null];readonly width?:number;readonly height?:number;readonly thickness?:number;readonly softness?:number;readonly inside?:readonly [number,number,number,number];readonly outside?:readonly [number,number,number,number];readonly composite?:boolean};readonly animation?:Readonly<Partial<Record<"centerX"|"centerY"|"width"|"height"|"thickness"|"softness"|"insideR"|"insideG"|"insideB"|"insideA"|"outsideR"|"outsideG"|"outsideB"|"outsideA",WireAnimatableValue>>>}
 | {readonly kind:"paint_bucket";readonly parameters:{readonly point?:readonly [number|null,number|null];readonly selector?:number;readonly tolerance?:number;readonly invert?:boolean;readonly color?:readonly [number,number,number,number];readonly opacity?:number;readonly mode?:number};readonly animation?:Readonly<Partial<Record<"pointX"|"pointY"|"tolerance"|"opacity"|"colorR"|"colorG"|"colorB"|"colorA",WireAnimatableValue>>>}
 | {readonly kind:"gradient";readonly parameters:{readonly start?:readonly [number|null,number|null];readonly end?:readonly [number|null,number|null];readonly startColor?:readonly [number,number,number,number];readonly endColor?:readonly [number,number,number,number];readonly shape?:number;readonly scatter?:number;readonly midpoint?:number;readonly blend?:number};readonly animation?:Readonly<Partial<Record<"startX"|"startY"|"endX"|"endY"|"startColorR"|"startColorG"|"startColorB"|"startColorA"|"endColorR"|"endColorG"|"endColorB"|"endColorA"|"scatter"|"midpoint"|"blend",WireAnimatableValue>>>}
 | {readonly kind:"brush_strokes";readonly parameters:{readonly angle?:number;readonly size?:number;readonly length?:number;readonly density?:number;readonly randomness?:number;readonly surface?:number;readonly blend?:number};readonly animation?:Readonly<Partial<Record<"angle"|"size"|"length"|"density"|"randomness"|"blend",WireAnimatableValue>>>}
 | {readonly kind:"alpha_glow";readonly parameters:{readonly glow?:number;readonly brightness?:number;readonly startColor?:readonly [number,number,number,number];readonly endColor?:readonly [number,number,number,number];readonly useEnd?:boolean;readonly fadeOut?:boolean};readonly animation?:Readonly<Partial<Record<"glow"|"brightness"|"startColorR"|"startColorG"|"startColorB"|"startColorA"|"endColorR"|"endColorG"|"endColorB"|"endColorA",WireAnimatableValue>>>}
 | {readonly kind:"block_dissolve";readonly parameters:{readonly completion?:number;readonly blockW?:number;readonly blockH?:number;readonly feather?:number;readonly soft?:boolean};readonly animation?:Readonly<Partial<Record<"completion"|"blockW"|"blockH"|"feather",WireAnimatableValue>>>}
 | {readonly kind:"graphic_text";readonly parameters:{readonly name?:string;readonly fill?:boolean;readonly fillColor?:readonly[number,number,number,number];readonly stroke?:boolean;readonly strokeColor?:readonly[number,number,number,number];readonly strokeWidth?:number;readonly strokeType?:number;readonly stroke2?:boolean;readonly stroke2Color?:readonly[number,number,number,number];readonly stroke2Width?:number;readonly stroke2Type?:number;readonly background?:boolean;readonly backgroundColor?:readonly[number,number,number,number];readonly backgroundOpacity?:number;readonly backgroundSize?:number;readonly backgroundRadius?:number;readonly shadow?:boolean;readonly shadowColor?:readonly[number,number,number,number];readonly shadowOpacity?:number;readonly shadowAngle?:number;readonly shadowDistance?:number;readonly shadowSize?:number;readonly shadowBlur?:number;readonly position?:readonly[number,number]|null;readonly anchor?:readonly[number,number];readonly scale?:number;readonly scaleWidth?:number;readonly uniformScale?:boolean;readonly rotation?:number;readonly opacity?:number;readonly text?:string;readonly font?:string;readonly fontStyle?:string;readonly size?:number;readonly align?:number;readonly tracking?:number;readonly kerning?:boolean;readonly ligatures?:boolean;readonly leading?:number;readonly baselineShift?:number;readonly fauxBold?:boolean;readonly fauxItalic?:boolean;readonly caps?:number;readonly underline?:boolean;readonly boxWidth?:number;readonly boxHeight?:number;readonly vertical?:boolean;readonly runs?:ReadonlyArray<Readonly<{start:number;end:number;style:Readonly<{font?:string;fontStyle?:string;size?:number;fill?:readonly[number,number,number,number];fauxBold?:boolean;fauxItalic?:boolean;underline?:boolean;tracking?:number;baselineShift?:number;caps?:number}>}>>;};readonly animation?:Readonly<Partial<Record<'size'|'tracking'|'leading'|'baselineShift'|'boxWidth'|'boxHeight'|'positionX'|'positionY'|'anchorX'|'anchorY'|'fillR'|'fillG'|'fillB'|'fillA'|'strokeR'|'strokeG'|'strokeB'|'strokeA'|'stroke2R'|'stroke2G'|'stroke2B'|'stroke2A'|'backgroundR'|'backgroundG'|'backgroundB'|'backgroundA'|'shadowR'|'shadowG'|'shadowB'|'shadowA'|'strokeWidth'|'stroke2Width'|'backgroundOpacity'|'backgroundSize'|'backgroundRadius'|'shadowOpacity'|'shadowAngle'|'shadowDistance'|'shadowSize'|'shadowBlur'|'scale'|'scaleWidth'|'rotation'|'opacity',WireAnimatableValue>>>;readonly sourceTextAnimation?:Readonly<{timeBase:number;keyframes:ReadonlyArray<Readonly<{tick:number;value:string}>>}>}
 | {readonly kind:"graphic_shape";readonly parameters:{readonly name?:string;readonly shape?:number;readonly size?:readonly[number,number];readonly position?:readonly[number,number]|null;readonly anchor?:readonly[number,number];readonly points?:readonly(readonly[number,number])[];readonly fill?:boolean;readonly fillColor?:readonly[number,number,number,number];readonly stroke?:boolean;readonly strokeColor?:readonly[number,number,number,number];readonly stroke2?:boolean;readonly stroke2Color?:readonly[number,number,number,number];readonly background?:boolean;readonly backgroundColor?:readonly[number,number,number,number];readonly shadow?:boolean;readonly shadowColor?:readonly[number,number,number,number];readonly uniformScale?:boolean;readonly strokeType?:number;readonly stroke2Type?:number;readonly sides?:number;readonly cornerRadius?:number;readonly strokeWidth?:number;readonly stroke2Width?:number;readonly backgroundOpacity?:number;readonly backgroundSize?:number;readonly backgroundRadius?:number;readonly shadowOpacity?:number;readonly shadowAngle?:number;readonly shadowDistance?:number;readonly shadowSize?:number;readonly shadowBlur?:number;readonly scale?:number;readonly scaleWidth?:number;readonly rotation?:number;readonly opacity?:number;};readonly animation?:Readonly<Partial<Record<'sizeX'|'sizeY'|'positionX'|'positionY'|'anchorX'|'anchorY'|'fillR'|'fillG'|'fillB'|'fillA'|'strokeR'|'strokeG'|'strokeB'|'strokeA'|'stroke2R'|'stroke2G'|'stroke2B'|'stroke2A'|'backgroundR'|'backgroundG'|'backgroundB'|'backgroundA'|'shadowR'|'shadowG'|'shadowB'|'shadowA'|'sides'|'cornerRadius'|'strokeWidth'|'stroke2Width'|'backgroundOpacity'|'backgroundSize'|'backgroundRadius'|'shadowOpacity'|'shadowAngle'|'shadowDistance'|'shadowSize'|'shadowBlur'|'scale'|'scaleWidth'|'rotation'|'opacity',WireAnimatableValue>>>}
 | {readonly kind:"track_matte";readonly parameters:{readonly matte?:number;readonly composite?:number;readonly reverse?:boolean}}
 | {readonly kind:"gradient_wipe";readonly parameters:{readonly completion?:number;readonly softness?:number;readonly layer?:number;readonly placement?:number;readonly invert?:boolean};readonly animation?:Readonly<Partial<Record<"completion"|"softness",WireAnimatableValue>>>}
 | {readonly kind:"linear_wipe";readonly parameters:{readonly completion?:number;readonly angle?:number;readonly feather?:number};readonly animation?:Readonly<Partial<Record<"completion"|"angle"|"feather",WireAnimatableValue>>>}
 | {readonly kind:"magnify";readonly parameters:{readonly shape?:number;readonly mode?:number;readonly center?:readonly [number|null,number|null]|null;readonly magnification?:number;readonly size?:number;readonly feather?:number;readonly opacity?:number};readonly animation?:Readonly<Partial<Record<"centerX"|"centerY"|"magnification"|"size"|"feather"|"opacity",WireAnimatableValue>>>}
 | {readonly kind:"spherize";readonly parameters:{readonly radius?:number;readonly center?:readonly [number|null,number|null]|null};readonly animation?:Readonly<Partial<Record<"radius"|"centerX"|"centerY",WireAnimatableValue>>>}
 | {readonly kind:"rgb_split";readonly parameters:{readonly mode?:0|1;readonly amount?:number;readonly angle?:number;readonly blend?:number;readonly center?:readonly [number|null,number|null]|null};readonly animation?:Readonly<Partial<Record<"amount"|"angle"|"centerX"|"centerY"|"blend",WireAnimatableValue>>>}
 | {readonly kind:"mirror";readonly parameters:{readonly angle?:number;readonly center?:readonly [number|null,number|null]|null};readonly animation?:Readonly<Partial<Record<"angle"|"centerX"|"centerY",WireAnimatableValue>>>}
 | {readonly kind:"offset";readonly parameters:{readonly blend?:number;readonly shift?:readonly [number|null,number|null]|null};readonly animation?:Readonly<Partial<Record<"blend"|"shiftX"|"shiftY",WireAnimatableValue>>>}
 | {readonly kind:"shrink";readonly parameters:{readonly from?:number;readonly to?:number;readonly center?:readonly [number|null,number|null]|null;readonly easing?:number};readonly animation?:Readonly<Partial<Record<"from"|"to"|"centerX"|"centerY",WireAnimatableValue>>>}
 | {readonly kind:"grow";readonly parameters:{readonly from?:number;readonly to?:number;readonly center?:readonly [number|null,number|null]|null;readonly easing?:number};readonly animation?:Readonly<Partial<Record<"from"|"to"|"centerX"|"centerY",WireAnimatableValue>>>}
 | {readonly kind:"spin";readonly parameters:{readonly amount?:number;readonly scale?:number;readonly center?:readonly [number|null,number|null]|null;readonly easing?:number};readonly animation?:Readonly<Partial<Record<"amount"|"scale"|"centerX"|"centerY",WireAnimatableValue>>>}
 | {readonly kind:"compound_blur";readonly parameters:{readonly layer?:number;readonly max?:number;readonly stretch?:boolean;readonly invert?:boolean};readonly animation?:WireAnimatableValue}
 | {readonly kind:"transform";readonly parameters:{readonly anchor?:readonly [number|null,number|null]|null;readonly position?:readonly [number|null,number|null]|null;readonly uniformScale?:boolean;readonly scaleHeight?:number;readonly scaleWidth?:number;readonly skew?:number;readonly skewAxis?:number;readonly rotation?:number;readonly opacity?:number;readonly shutterAngle?:number;};readonly animation?:Readonly<Partial<Record<"scaleHeight"|"scaleWidth"|"skew"|"skewAxis"|"rotation"|"opacity"|"shutterAngle"|"anchorX"|"anchorY"|"positionX"|"positionY",WireAnimatableValue>>>}
 | {readonly kind:"echo_glow";readonly parameters:{readonly threshold?:number;readonly echoes?:number;readonly radius?:number;readonly spread?:number;readonly intensity?:number;readonly decay?:number;readonly color?:readonly [number,number,number,number]};readonly animation?:Readonly<Partial<Record<"threshold"|"echoes"|"radius"|"spread"|"intensity"|"decay"|"colorRed"|"colorGreen"|"colorBlue"|"colorAlpha",WireAnimatableValue>>>}
 | {readonly kind:"wonder_glow";readonly parameters:{readonly threshold?:number;readonly saturation?:number;readonly radius?:number;readonly intensity?:number;readonly color?:readonly [number,number,number,number];readonly useColor?:boolean;readonly mode?:0|1};readonly animation?:Readonly<Partial<Record<"threshold"|"saturation"|"radius"|"intensity"|"colorRed"|"colorGreen"|"colorBlue"|"colorAlpha",WireAnimatableValue>>>}
 | {readonly kind:"glint";readonly parameters:{readonly threshold?:number;readonly colorize?:number;readonly length?:number;readonly intensity?:number;readonly color?:readonly [number,number,number,number];readonly rotation?:number;readonly rays?:0|1|2|3};readonly animation?:Readonly<Partial<Record<"rotation"|"threshold"|"colorize"|"length"|"intensity"|"colorRed"|"colorGreen"|"colorBlue"|"colorAlpha",WireAnimatableValue>>>}
 | {readonly kind:"vr_chromatic_aberrations";readonly parameters:{readonly frameLayout?:number;readonly red?:number;readonly green?:number;readonly blue?:number;readonly centerX?:number;readonly centerY?:number;readonly falloff?:number;readonly inverse?:boolean};readonly animation?:Readonly<Partial<Record<"red"|"green"|"blue"|"centerX"|"centerY"|"falloff",WireAnimatableValue>>>}
 | {readonly kind:"vr_color_gradients";readonly parameters:{readonly frameLayout?:number;readonly mode?:number;readonly c1?:readonly [number,number,number,number];readonly c2?:readonly [number,number,number,number];readonly c3?:readonly [number,number,number,number];readonly p1Lon?:number;readonly p1Lat?:number;readonly p2Lon?:number;readonly p2Lat?:number;readonly p3Lon?:number;readonly p3Lat?:number;readonly blend?:number;readonly opacity?:number;};readonly animation?:Readonly<Partial<Record<"c1R"|"c1G"|"c1B"|"c1A"|"c2R"|"c2G"|"c2B"|"c2A"|"c3R"|"c3G"|"c3B"|"c3A"|"p1Lon"|"p1Lat"|"p2Lon"|"p2Lat"|"p3Lon"|"p3Lat"|"blend"|"opacity",WireAnimatableValue>>>}
 | {readonly kind:"vr_digital_glitch";readonly parameters:{readonly frameLayout?:number;readonly amplitude?:number;readonly distortion?:number;readonly rate?:number;readonly color?:number;readonly scanlines?:number;readonly seed?:number;};readonly animation?:Readonly<Partial<Record<"amplitude"|"distortion"|"rate"|"color"|"scanlines"|"seed",WireAnimatableValue>>>}
 | {readonly kind:"metadata_burnin";readonly parameters:{readonly source?:number;readonly position?:readonly [number|null,number|null];readonly alignment?:number;readonly size?:number;readonly color?:readonly [number,number,number,number];readonly opacity?:number;readonly prefix?:string};readonly animation?:Readonly<Partial<Record<"positionX"|"positionY"|"size"|"colorR"|"colorG"|"colorB"|"colorA"|"opacity",WireAnimatableValue>>>}
 | {readonly kind:"lightning";readonly parameters:{readonly start?:readonly [number|null,number|null];readonly end?:readonly [number|null,number|null];readonly outside?:readonly [number,number,number,number];readonly inside?:readonly [number,number,number,number];readonly segments?:number;readonly amplitude?:number;readonly detail?:number;readonly detailAmplitude?:number;readonly branching?:number;readonly speed?:number;readonly width?:number;readonly core?:number;readonly seed?:number;readonly mode?:number};readonly animation?:Readonly<Partial<Record<"startX"|"startY"|"endX"|"endY"|"segments"|"amplitude"|"detail"|"detailAmplitude"|"branching"|"speed"|"width"|"core"|"seed"|"outsideR"|"outsideG"|"outsideB"|"outsideA"|"insideR"|"insideG"|"insideB"|"insideA",WireAnimatableValue>>>}
 | {readonly kind:"write_on";readonly parameters:{readonly brush?:readonly [number|null,number|null];readonly color?:readonly [number,number,number,number];readonly size?:number;readonly hardness?:number;readonly opacity?:number;readonly strokeLength?:number;readonly spacing?:number;readonly style?:number};readonly animation?:Readonly<Partial<Record<"brushX"|"brushY"|"colorR"|"colorG"|"colorB"|"colorA"|"size"|"hardness"|"opacity"|"strokeLength"|"spacing",WireAnimatableValue>>>}
 | {readonly kind:"timecode";readonly parameters:{readonly position?:readonly [number|null,number|null];readonly size?:number;readonly opacity?:number};readonly animation?:Readonly<Partial<Record<"positionX"|"positionY"|"size"|"opacity",WireAnimatableValue>>>}
 | {readonly kind:"clip_name";readonly parameters:{readonly position?:readonly [number|null,number|null];readonly size?:number};readonly animation?:Readonly<Partial<Record<"positionX"|"positionY"|"size",WireAnimatableValue>>>}
 | {readonly kind:"simple_text";readonly parameters:{readonly text?:string;readonly position?:readonly [number|null,number|null];readonly size?:number;readonly font?:number;readonly alignment?:number;readonly color?:readonly [number,number,number,number];readonly bgOpacity?:number};readonly animation?:Readonly<Partial<Record<"positionX"|"positionY"|"size"|"colorR"|"colorG"|"colorB"|"colorA"|"bgOpacity",WireAnimatableValue>>>}
 | {readonly kind:"vr_plane_to_sphere";readonly parameters:{readonly frameLayout?:number;readonly scale?:number;readonly pan?:number;readonly tilt?:number;readonly roll?:number;readonly feather?:number;};readonly animation?:Readonly<Partial<Record<"scale"|"pan"|"tilt"|"roll"|"feather",WireAnimatableValue>>>}
 | {readonly kind:"vr_glow";readonly parameters:{readonly frameLayout?:number;readonly useTint?:boolean;readonly tint?:readonly [number,number,number,number];readonly threshold?:number;readonly radius?:number;readonly brightness?:number;readonly saturation?:number;};readonly animation?:Readonly<Partial<Record<"threshold"|"radius"|"brightness"|"saturation"|"tintR"|"tintG"|"tintB"|"tintA",WireAnimatableValue>>>}
 | {readonly kind:"vr_fractal_noise";readonly parameters:{readonly frameLayout?:number;readonly fractalType?:number;readonly mode?:number;readonly invert?:boolean;readonly contrast?:number;readonly brightness?:number;readonly scale?:number;readonly complexity?:number;readonly evolution?:number;readonly opacity?:number;};readonly animation?:Readonly<Partial<Record<"contrast"|"brightness"|"scale"|"complexity"|"evolution"|"opacity",WireAnimatableValue>>>}
 | {readonly kind:"vr_denoise";readonly parameters:{readonly frameLayout?:number;readonly amount?:number;readonly noiseType?:number;readonly showNoise?:boolean};readonly animation?:WireAnimatableValue}
 | {readonly kind:"vr_blur";readonly parameters:{readonly frameLayout?:number;readonly blurriness?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"vr_sharpen";readonly parameters:{readonly frameLayout?:number;readonly amount?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"spherical_video";readonly parameters:WireSphericalVideo;readonly animation?:Readonly<Partial<Record<"tilt"|"pan"|"roll"|"fov",WireAnimatableValue>>>}
 | {readonly kind:"vr_projection";readonly parameters:{readonly frameLayout?:number;readonly tilt?:number;readonly pan?:number;readonly roll?:number;readonly fov?:number;readonly stretch?:boolean;};readonly animation?:Readonly<Partial<Record<"tilt"|"pan"|"roll"|"fov",WireAnimatableValue>>>}
 | {readonly kind:"vr_rotate_sphere";readonly parameters:{readonly frameLayout?:number;readonly tilt?:number;readonly pan?:number;readonly roll?:number;};readonly animation?:Readonly<Partial<Record<"tilt"|"pan"|"roll",WireAnimatableValue>>>}
 | {readonly kind:"light_leaks";readonly parameters:{readonly c1?:readonly [number,number,number,number];readonly c2?:readonly [number,number,number,number];readonly intensity?:number;readonly scale?:number;readonly direction?:number;readonly speed?:number;readonly seed?:number;readonly mode?:number};readonly animation?:Readonly<Partial<Record<"intensity"|"scale"|"direction"|"speed"|"seed"|"c1Red"|"c1Green"|"c1Blue"|"c1Alpha"|"c2Red"|"c2Green"|"c2Blue"|"c2Alpha",WireAnimatableValue>>>}
 | {readonly kind:"warp_stabilizer";readonly parameters:{readonly result?:number;readonly smoothness?:number;readonly method?:number;readonly preserveScale?:boolean;readonly framing?:number;readonly maxScale?:number;readonly actionSafe?:number;readonly additionalScale?:number;readonly detailed?:boolean;readonly cropLess?:number};readonly animation?:Readonly<Partial<Record<"smoothness"|"maxScale"|"actionSafe"|"additionalScale"|"cropLess",WireAnimatableValue>>>}
 | {readonly kind:"auto_reframe";readonly parameters:{readonly preset?:number;readonly aspect?:number;readonly zoom?:number;readonly offset?:readonly [number,number]};readonly animation?:Readonly<Partial<Record<"zoom"|"offsetX"|"offsetY",WireAnimatableValue>>>}
 | {readonly kind:"echo";readonly parameters:{readonly time?:number;readonly count?:number;readonly start?:number;readonly decay?:number;readonly operator?:number};readonly animation?:Readonly<Partial<Record<"time"|"count"|"start"|"decay",WireAnimatableValue>>>}
 | {readonly kind:"posterize_time";readonly parameters:{readonly rate?:number};readonly animation?:Readonly<Partial<Record<"rate",WireAnimatableValue>>>}
 | {readonly kind:"volumetric_rays";readonly parameters:{readonly center?:readonly [number|null,number|null]|null;readonly threshold?:number;readonly length?:number;readonly intensity?:number;readonly color?:readonly [number,number,number,number];readonly only?:boolean};readonly animation?:Readonly<Partial<Record<"centerX"|"centerY"|"threshold"|"length"|"intensity"|"colorRed"|"colorGreen"|"colorBlue"|"colorAlpha",WireAnimatableValue>>>}
 | {readonly kind:"edge_glow";readonly parameters:{readonly threshold?:number;readonly width?:number;readonly radius?:number;readonly intensity?:number;readonly color?:readonly [number,number,number,number];readonly only?:boolean};readonly animation?:Readonly<Partial<Record<"threshold"|"width"|"radius"|"intensity"|"colorRed"|"colorGreen"|"colorBlue"|"colorAlpha",WireAnimatableValue>>>}
 | {readonly kind:"logo_cutout";readonly parameters:{readonly background?:0|1|2;readonly color?:readonly [number,number,number,number];readonly threshold?:number;readonly softness?:number;readonly unmultiply?:boolean;readonly invert?:boolean};readonly animation?:Partial<Record<"threshold"|"softness"|"colorRed"|"colorGreen"|"colorBlue"|"colorAlpha",WireAnimatableValue>>}
 | {readonly kind:"vignette";readonly parameters:{readonly amount?:number;readonly midpoint?:number;readonly roundness?:number;readonly feather?:number;readonly color?:readonly [number,number,number,number]};readonly animation?:Readonly<Partial<Record<"amount"|"midpoint"|"roundness"|"feather"|"colorRed"|"colorGreen"|"colorBlue"|"colorAlpha",WireAnimatableValue>>>}
 | {readonly kind:"lighting_effects";readonly parameters:{readonly lights?:readonly WireLightingLight[];readonly ambientColor?:readonly [number,number,number,number];readonly ambient?:number;readonly gloss?:number;readonly material?:number;readonly exposure?:number;readonly bumpHeight?:number;readonly whiteHigh?:boolean};readonly animation?:Partial<Record<WireLightingChannel,WireAnimatableValue>>}
 | {readonly kind:"asc_cdl";readonly parameters:{readonly rSlope?:number;readonly rOffset?:number;readonly rPower?:number;readonly gSlope?:number;readonly gOffset?:number;readonly gPower?:number;readonly bSlope?:number;readonly bOffset?:number;readonly bPower?:number;readonly saturation?:number;};readonly animation?:Readonly<Partial<Record<"rSlope"|"rOffset"|"rPower"|"gSlope"|"gOffset"|"gPower"|"bSlope"|"bOffset"|"bPower"|"saturation",WireAnimatableValue>>>}
 | {readonly kind:"brightness_contrast";readonly parameters:{readonly brightness?:number;readonly contrast?:number};readonly animation?:Partial<Record<"brightness"|"contrast",WireAnimatableValue>>}
 | {readonly kind:"crop";readonly parameters:{readonly left?:number;readonly top?:number;readonly right?:number;readonly bottom?:number;readonly feather?:number;readonly zoom?:boolean};readonly animation?:Partial<Record<"left"|"top"|"right"|"bottom"|"feather",WireAnimatableValue>>}
 | {readonly kind:"unsharp_mask";readonly parameters:{readonly amount?:number;readonly radius?:number;readonly threshold?:number};readonly animation?:Partial<Record<"amount"|"radius"|"threshold",WireAnimatableValue>>}
 | {readonly kind:"bokeh_blur";readonly parameters:{readonly shape?:number;readonly amount?:number;readonly rotation?:number;readonly highlight?:number;readonly threshold?:number};readonly animation?:Readonly<Partial<Record<"amount"|"rotation"|"highlight"|"threshold",WireAnimatableValue>>>}
 | {readonly kind:"focus_blur";readonly parameters:{readonly shape?:number;readonly center?:readonly [number|null,number|null]|null;readonly angle?:number;readonly size?:number;readonly feather?:number;readonly amount?:number;readonly show?:boolean};readonly animation?:Readonly<Partial<Record<"amount"|"size"|"feather"|"angle"|"centerX"|"centerY",WireAnimatableValue>>>}
 | {readonly kind:"turbulent_displace";readonly parameters:{readonly displacement?:number;readonly amount?:number;readonly size?:number;readonly offset?:readonly [number|null,number|null]|null;readonly complexity?:number;readonly evolution?:number;readonly cycle?:boolean;readonly cycleRevs?:number;readonly seed?:number;readonly pinning?:number;readonly antialias?:number};readonly animation?:Readonly<Partial<Record<"amount"|"size"|"offsetX"|"offsetY"|"complexity"|"evolution"|"cycleRevs"|"seed",WireAnimatableValue>>>}
 | {readonly kind:"channel_blur";readonly parameters:{readonly red?:number;readonly green?:number;readonly blue?:number;readonly alpha?:number;readonly dimensions?:number;readonly repeatEdge?:boolean};readonly animation?:Readonly<Partial<Record<"red"|"green"|"blue"|"alpha",WireAnimatableValue>>>}
 | {readonly kind:"reduce_interlace_flicker";readonly parameters:{readonly softness?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"sharpen";readonly parameters:{readonly amount?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"edge_feather";readonly parameters:{readonly amount?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"auto_align";readonly parameters:{readonly horizontal?:number;readonly vertical?:number;readonly marginX?:number;readonly marginY?:number};readonly animation?:Readonly<Partial<Record<"marginX"|"marginY",WireAnimatableValue>>>}
 | {readonly kind:"clone";readonly parameters:{readonly columns?:number;readonly rows?:number;readonly gap?:number;readonly mirror?:boolean};readonly animation?:Readonly<Partial<Record<"columns"|"rows"|"gap",WireAnimatableValue>>>}
 | {readonly kind:"rounded_crop";readonly parameters:{readonly left?:number;readonly top?:number;readonly right?:number;readonly bottom?:number;readonly radius?:number;readonly feather?:number;readonly border?:number;readonly borderColor?:readonly [number,number,number,number]};readonly animation?:Readonly<Partial<Record<"left"|"top"|"right"|"bottom"|"radius"|"feather"|"border"|"borderR"|"borderG"|"borderB"|"borderA",WireAnimatableValue>>>}
 | {readonly kind:"spacer";readonly parameters:{readonly left?:number;readonly top?:number;readonly right?:number;readonly bottom?:number;readonly uniform?:boolean;readonly radius?:number;readonly fill?:boolean;readonly color?:readonly [number,number,number,number]};readonly animation?:Readonly<Partial<Record<"left"|"top"|"right"|"bottom"|"radius"|"colorR"|"colorG"|"colorB"|"colorA",WireAnimatableValue>>>}
 | {readonly kind:"camera_shake";readonly parameters:{readonly frequency?:number;readonly amount?:number;readonly rotation?:number;readonly zoom?:number;readonly complexity?:number;readonly motionBlur?:number;readonly seed?:number};readonly animation?:Readonly<Partial<Record<"frequency"|"amount"|"rotation"|"zoom"|"complexity"|"motionBlur"|"seed",WireAnimatableValue>>>}
 | {readonly kind:"wiggle";readonly parameters:{readonly frequency?:number;readonly amount?:number;readonly rotation?:number;readonly scale?:number;readonly dimensions?:number;readonly seed?:number};readonly animation?:Readonly<Partial<Record<"frequency"|"amount"|"rotation"|"scale"|"seed",WireAnimatableValue>>>}
 | {readonly kind:"move";readonly parameters:{readonly from?:readonly [number,number];readonly to?:readonly [number,number];readonly easing?:number;readonly motionBlur?:number};readonly animation?:Readonly<Partial<Record<"fromX"|"fromY"|"toX"|"toY"|"motionBlur",WireAnimatableValue>>>}
 | {readonly kind:"rotate_3d";readonly parameters:{readonly rotX?:number;readonly rotY?:number;readonly rotZ?:number;readonly perspective?:number;readonly z?:number;readonly hideBack?:boolean};readonly animation?:Readonly<Partial<Record<"rotX"|"rotY"|"rotZ"|"perspective"|"z",WireAnimatableValue>>>}
 | {readonly kind:"corner_pin";readonly parameters:{readonly upperLeft?:readonly [number|null,number|null]|null;readonly upperRight?:readonly [number|null,number|null]|null;readonly lowerLeft?:readonly [number|null,number|null]|null;readonly lowerRight?:readonly [number|null,number|null]|null};readonly animation?:Readonly<Partial<Record<"upperLeftX"|"upperLeftY"|"upperRightX"|"upperRightY"|"lowerLeftX"|"lowerLeftY"|"lowerRightX"|"lowerRightY",WireAnimatableValue>>>}
 | {readonly kind:"color_emboss";readonly parameters:{readonly direction?:number;readonly relief?:number;readonly contrast?:number;readonly blend?:number};readonly animation?:Readonly<Partial<Record<"direction"|"relief"|"contrast"|"blend",WireAnimatableValue>>>}
 | {readonly kind:"roughen_edges";readonly parameters:{readonly edgeType?:number;readonly edgeColor?:readonly [number,number,number,number];readonly border?:number;readonly sharpness?:number;readonly influence?:number;readonly scale?:number;readonly stretch?:number;readonly offset?:readonly [number,number];readonly complexity?:number;readonly evolution?:number;readonly seed?:number};readonly animation?:Readonly<Partial<Record<"border"|"sharpness"|"influence"|"scale"|"stretch"|"complexity"|"evolution"|"seed"|"offsetX"|"offsetY"|"edgeColorR"|"edgeColorG"|"edgeColorB"|"edgeColorA",WireAnimatableValue>>>}
 | {readonly kind:"long_shadow";readonly parameters:{readonly angle?:number;readonly length?:number;readonly color?:readonly [number,number,number,number];readonly opacity?:number;readonly fade?:boolean;readonly only?:boolean};readonly animation?:Readonly<Partial<Record<"angle"|"length"|"opacity"|"colorR"|"colorG"|"colorB"|"colorA",WireAnimatableValue>>>}
 | {readonly kind:"stroke";readonly parameters:{readonly color?:readonly [number,number,number,number];readonly width?:number;readonly position?:number;readonly opacity?:number;readonly only?:boolean};readonly animation?:Readonly<Partial<Record<"width"|"opacity"|"colorR"|"colorG"|"colorB"|"colorA",WireAnimatableValue>>>}
 | {readonly kind:"cell_pattern";readonly parameters:{readonly pattern?:number;readonly invert?:boolean;readonly contrast?:number;readonly overflow?:number;readonly disperse?:number;readonly size?:number;readonly offset?:readonly [number|null,number|null];readonly evolution?:number;readonly seed?:number};readonly animation?:Readonly<Partial<Record<"offsetX"|"offsetY"|"contrast"|"disperse"|"size"|"evolution"|"seed",WireAnimatableValue>>>}
 | {readonly kind:"checkerboard";readonly parameters:{readonly anchor?:readonly [number|null,number|null];readonly width?:number;readonly height?:number;readonly square?:boolean;readonly feather?:number;readonly color?:readonly [number,number,number,number];readonly opacity?:number;readonly mode?:number};readonly animation?:Readonly<Partial<Record<"anchorX"|"anchorY"|"width"|"height"|"feather"|"opacity"|"colorR"|"colorG"|"colorB"|"colorA",WireAnimatableValue>>>}
 | {readonly kind:"circle";readonly parameters:{readonly center?:readonly [number|null,number|null];readonly radius?:number;readonly color?:readonly [number,number,number,number];readonly opacity?:number};readonly animation?:Readonly<Partial<Record<"centerX"|"centerY"|"radius"|"colorR"|"colorG"|"colorB"|"colorA"|"opacity",WireAnimatableValue>>>}
 | {readonly kind:"ramp";readonly parameters:{readonly start?:readonly [number|null,number|null];readonly end?:readonly [number|null,number|null];readonly startColor?:readonly [number,number,number,number];readonly endColor?:readonly [number,number,number,number];readonly shape?:"linear"|"radial";readonly blend?:number};readonly animation?:Readonly<Partial<Record<"startX"|"startY"|"endX"|"endY"|"startColorR"|"startColorG"|"startColorB"|"startColorA"|"endColorR"|"endColorG"|"endColorB"|"endColorA"|"blend",WireAnimatableValue>>>}
 | {readonly kind:"four_color_gradient";readonly parameters:{readonly c1?:readonly [number,number,number,number];readonly c2?:readonly [number,number,number,number];readonly c3?:readonly [number,number,number,number];readonly c4?:readonly [number,number,number,number];readonly blend?:number;readonly opacity?:number};readonly animation?:Readonly<Partial<Record<"c1R"|"c1G"|"c1B"|"c1A"|"c2R"|"c2G"|"c2B"|"c2A"|"c3R"|"c3G"|"c3B"|"c3A"|"c4R"|"c4G"|"c4B"|"c4A"|"blend"|"opacity",WireAnimatableValue>>>}
 | {readonly kind:"bevel_alpha";readonly parameters:{readonly color?:readonly [number,number,number,number];readonly thickness?:number;readonly angle?:number;readonly intensity?:number};readonly animation?:Readonly<Partial<Record<"colorR"|"colorG"|"colorB"|"colorA"|"thickness"|"angle"|"intensity",WireAnimatableValue>>>}
 | {readonly kind:"drop_shadow";readonly parameters:{readonly color?:readonly [number,number,number,number];readonly opacity?:number;readonly direction?:number;readonly distance?:number;readonly softness?:number;readonly only?:boolean};readonly animation?:Readonly<Partial<Record<"colorR"|"colorG"|"colorB"|"colorA"|"opacity"|"direction"|"distance"|"softness",WireAnimatableValue>>>}
 | {readonly kind:"basic_3d";readonly parameters:{readonly swivel?:number;readonly tilt?:number;readonly distance?:number};readonly animation?:Readonly<Partial<Record<"swivel"|"tilt"|"distance",WireAnimatableValue>>>}
 | {readonly kind:"mosaic";readonly parameters:{readonly horizontal?:number;readonly vertical?:number;readonly softness?:number;readonly sharp?:boolean};readonly animation?:Readonly<Partial<Record<"horizontal"|"vertical"|"softness",WireAnimatableValue>>>}
 | {readonly kind:"mosaic_legacy";readonly parameters:{readonly horizontal?:number;readonly vertical?:number;readonly softness?:number;readonly sharp?:boolean};readonly animation?:Readonly<Partial<Record<"horizontal"|"vertical"|"softness",WireAnimatableValue>>>}
 | {readonly kind:"camera_blur";readonly parameters:{readonly percent?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"find_edges";readonly parameters:{readonly invert?:boolean;readonly blend?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"replicate";readonly parameters:{readonly count?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"median";readonly parameters:{readonly radius?:number};readonly animation?:WireAnimatableValue}
 | {readonly kind:"gaussian_blur";readonly parameters:{readonly blurriness?:number;readonly dimensions?:0|1|2;readonly repeatEdge?:boolean};readonly animation?:WireAnimatableValue}
 | {readonly kind:"gaussian_blur_legacy";readonly parameters:{readonly blurriness?:number;readonly dimensions?:0|1|2;readonly repeatEdge?:boolean};readonly animation?:WireAnimatableValue}
 | {readonly kind:"luma_key";readonly parameters:{readonly threshold?:number;readonly cutoff?:number};readonly animation?:Partial<Record<"threshold"|"cutoff",WireAnimatableValue>>}
 | {readonly kind:"channel_mix";readonly parameters:{readonly rr?:number;readonly rg?:number;readonly rb?:number;readonly rc?:number;readonly gr?:number;readonly gg?:number;readonly gb?:number;readonly gc?:number;readonly br?:number;readonly bg?:number;readonly bb?:number;readonly bc?:number;readonly monochrome?:boolean};readonly animation?:Partial<Record<"rr"|"rg"|"rb"|"rc"|"gr"|"gg"|"gb"|"gc"|"br"|"bg"|"bb"|"bc",WireAnimatableValue>>}
 | {readonly kind:"color_replace";readonly parameters:{readonly target?:readonly [number,number,number,number];readonly replace?:readonly [number,number,number,number];readonly similarity?:number;readonly solid?:boolean};readonly animation?:WireAnimatableValue;readonly colorAnimation?:Partial<Record<"targetR"|"targetG"|"targetB"|"targetA"|"replaceR"|"replaceG"|"replaceB"|"replaceA",WireAnimatableValue>>}
 | {readonly kind:"ultra_key";readonly parameters:{readonly keyColor?:readonly [number,number,number,number];readonly output?:0|1|2;readonly setting?:0|1|2|3;readonly transparency?:number;readonly highlight?:number;readonly shadow?:number;readonly tolerance?:number;readonly pedestal?:number;readonly choke?:number;readonly soften?:number;readonly spill?:number;readonly contrast?:number;readonly midPoint?:number;readonly desaturate?:number;readonly range?:number;readonly spillLuma?:number;readonly ccSaturation?:number;readonly ccHue?:number;readonly ccLuminance?:number;};readonly animation?:Partial<Record<"transparency"|"highlight"|"shadow"|"tolerance"|"pedestal"|"choke"|"soften"|"spill"|"contrast"|"midPoint"|"desaturate"|"range"|"spillLuma"|"ccSaturation"|"ccHue"|"ccLuminance",WireAnimatableValue>>;readonly colorAnimation?:Partial<Record<"r"|"g"|"b"|"a",WireAnimatableValue>>}
 | {readonly kind:"color_key";readonly parameters:{readonly color?:readonly [number,number,number,number];readonly tolerance?:number;readonly thin?:number;readonly feather?:number};readonly animation?:Partial<Record<"tolerance"|"thin"|"feather",WireAnimatableValue>>;readonly colorAnimation?:Partial<Record<"r"|"g"|"b"|"a",WireAnimatableValue>>}
 | {readonly kind:"color_pass";readonly parameters:{readonly color?:readonly [number,number,number,number];readonly similarity?:number;readonly reverse?:boolean};readonly animation?:WireAnimatableValue;readonly colorAnimation?:Partial<Record<"r"|"g"|"b"|"a",WireAnimatableValue>>}
 | {readonly kind:"change_to_color";readonly parameters:{readonly from?:readonly [number,number,number,number];readonly to?:readonly [number,number,number,number];readonly hueTol?:number;readonly softness?:number};readonly animation?:Partial<Record<"hueTol"|"softness",WireAnimatableValue>>;readonly colorAnimation?:Partial<Record<"fromR"|"fromG"|"fromB"|"fromA"|"toR"|"toG"|"toB"|"toA",WireAnimatableValue>>}
 | {readonly kind:"leave_color";readonly parameters:{readonly amount?:number;readonly color?:readonly [number,number,number,number];readonly tolerance?:number;readonly softness?:number};readonly animation?:Partial<Record<"amount"|"tolerance"|"softness",WireAnimatableValue>>;readonly colorAnimation?:Partial<Record<"r"|"g"|"b"|"a",WireAnimatableValue>>}
 | {readonly kind:"color_balance";readonly parameters:{readonly shadowR?:number;readonly shadowG?:number;readonly shadowB?:number;readonly midR?:number;readonly midG?:number;readonly midB?:number;readonly hiR?:number;readonly hiG?:number;readonly hiB?:number;readonly preserve?:boolean};readonly animation?:Partial<Record<"shadowR"|"shadowG"|"shadowB"|"midR"|"midG"|"midB"|"hiR"|"hiG"|"hiB",WireAnimatableValue>>}
 | {readonly kind:"color_tint";readonly parameters:{readonly black?:readonly [number,number,number,number];readonly white?:readonly [number,number,number,number];readonly amount?:number};readonly animation?:WireAnimatableValue;readonly colorAnimation?:Partial<Record<"blackR"|"blackG"|"blackB"|"blackA"|"whiteR"|"whiteG"|"whiteB"|"whiteA",WireAnimatableValue>>}
 | {readonly kind:"color_levels";readonly parameters:{readonly inBlack?:number;readonly inWhite?:number;readonly outBlack?:number;readonly outWhite?:number;readonly gamma?:number};readonly animation?:Partial<Readonly<Record<"inBlack"|"inWhite"|"outBlack"|"outWhite"|"gamma",WireAnimatableValue>>>}
 | {readonly kind:"color_extract";readonly parameters:{readonly black?:number;readonly white?:number;readonly softness?:number;readonly invert?:boolean};readonly animation?:Partial<Readonly<Record<"black"|"white"|"softness",WireAnimatableValue>>>}
 | {readonly kind:"horizontal_flip"}
 | {readonly kind:"vertical_flip"}
 | {readonly kind:"black_white"}
 | {readonly kind:"cineon_converter";readonly parameters:WireCineonConverter;readonly animation?:Partial<Record<'black10'|'white10'|'blackInternal'|'whiteInternal'|'gamma'|'rolloff',WireAnimatableValue>>}
 | {readonly kind:"video_limiter";readonly parameters:WireVideoLimiter;readonly animation?:Partial<Record<'red'|'green'|'blue'|'alpha',WireAnimatableValue>>}
 | {readonly kind:"lumetri";readonly parameters:WireLumetri}
  | {readonly kind:'concat_luma_key';readonly parameters:{readonly threshold?:number;readonly softness?:number;readonly invert?:number}}
  | {readonly kind:'concat_gaussian_blur';readonly parameters:{readonly radius?:number};readonly animation?:WireAnimatableValue;readonly concatKeys?:readonly WireConcatParamKey[]}
  | {readonly kind:'concat_motion_blur';readonly parameters:{readonly length?:number;readonly angle?:number};readonly concatKeys?:Readonly<Partial<Record<'length'|'angle',readonly WireConcatParamKey[]>>>}
  | {readonly kind:'concat_bloom_pulse';readonly parameters:{readonly amount?:number;readonly speed?:number};readonly concatKeys?:Readonly<Partial<Record<'amount'|'speed',readonly WireConcatParamKey[]>>>}
  | {readonly kind:'concat_zoom_blur';readonly parameters:{readonly amount?:number;readonly x?:number;readonly y?:number};readonly concatKeys?:Readonly<Partial<Record<'amount'|'x'|'y',readonly WireConcatParamKey[]>>>}
  | {readonly kind:'concat_film_grain';readonly parameters:{readonly amount?:number};readonly concatKeys?:readonly WireConcatParamKey[]}
  | {readonly kind:'concat_glow';readonly parameters:{readonly amount?:number};readonly concatKeys?:readonly WireConcatParamKey[]}
  | {readonly kind:'concat_halftone';readonly parameters:{readonly size?:number};readonly concatKeys?:readonly WireConcatParamKey[]}
  | {readonly kind:'concat_fisheye';readonly parameters:{readonly strength?:number};readonly concatKeys?:readonly WireConcatParamKey[]}
  | {readonly kind:'concat_chromatic_aberration';readonly parameters:{readonly shift?:number};readonly concatKeys?:readonly WireConcatParamKey[]}
  | {readonly kind:'concat_posterize';readonly parameters:{readonly levels?:number}}
  | {readonly kind:'concat_pixelate';readonly parameters:{readonly size?:number}}
  | {readonly kind:'concat_chroma_key';readonly parameters:{readonly color?:number;readonly similarity?:number;readonly softness?:number;readonly spill?:number};readonly concatKeys?:Readonly<Partial<Record<'color'|'similarity'|'softness'|'spill',readonly WireConcatParamKey[]>>>}
  | {readonly kind:'concat_saturation';readonly parameters:{readonly amount?:number};readonly animation?:WireAnimatableValue;readonly concatKeys?:readonly WireConcatParamKey[]}
  | {readonly kind:'concat_sharpen';readonly parameters:{readonly amount?:number}}
  | {readonly kind:'concat_bleach_bypass';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_vhs';readonly parameters:{readonly bleed?:number;readonly lines?:number}}
  | {readonly kind:'concat_falling_dust';readonly parameters:{readonly dust?:number;readonly leak?:number}}
  | {readonly kind:'concat_film';readonly parameters:{readonly grain?:number;readonly warmth?:number}}
  | {readonly kind:'concat_digicam';readonly parameters:{readonly flash?:number;readonly noise?:number}}
  | {readonly kind:'concat_mono';readonly parameters:{readonly colour?:number;readonly contrast?:number;readonly grain?:number}}
  | {readonly kind:'concat_midnight';readonly parameters:{readonly depth?:number;readonly grain?:number}}
  | {readonly kind:'concat_marquee';readonly parameters:{readonly lights?:number;readonly depth?:number}}
  | {readonly kind:'concat_retro_cartoon';readonly parameters:{readonly levels?:number;readonly ink?:number}}
  | {readonly kind:'concat_super8';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_autumn';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_neon';readonly parameters:{readonly glow?:number;readonly punch?:number}}
  | {readonly kind:'concat_pink_fuzz';readonly parameters:{readonly haze?:number;readonly blush?:number}}
  | {readonly kind:'concat_night';readonly parameters:{readonly lift?:number;readonly clarity?:number}}
  | {readonly kind:'concat_spooky';readonly parameters:{readonly dread?:number}}
  | {readonly kind:'concat_cool';readonly parameters:{readonly temperature?:number}}
  | {readonly kind:'concat_warm';readonly parameters:{readonly temperature?:number}}
  | {readonly kind:'concat_teal_orange';readonly parameters:{readonly amount?:number;readonly contrast?:number;readonly skin?:number}}
  | {readonly kind:'concat_noir';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_dreamy';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_high_contrast_mono';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_landscape_pop';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_natural';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_retro_70s';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_silver';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_tropical';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_vaporwave';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_winter_cool';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_classic_chrome';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_cyberpunk';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_delicious';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_rosy';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_sci_fi';readonly parameters:Readonly<Record<string,never>>}
  | {readonly kind:'concat_matcha';readonly parameters:{readonly amount?:number}}
  | {readonly kind:'concat_sage';readonly parameters:{readonly amount?:number}}
  | {readonly kind:'concat_ivory';readonly parameters:{readonly matte?:number;readonly warmth?:number}}
  | {readonly kind:'concat_milk';readonly parameters:{readonly matte?:number;readonly cool?:number}}
  | {readonly kind:'concat_bronze';readonly parameters:{readonly tone?:number;readonly contrast?:number}}
  | {readonly kind:'concat_cool_blue';readonly parameters:{readonly chill?:number;readonly contrast?:number}}
  | {readonly kind:'concat_fresh';readonly parameters:{readonly pop?:number;readonly brightness?:number}}
  | {readonly kind:'concat_vivid';readonly parameters:{readonly colour?:number;readonly punch?:number}}
  | {readonly kind:'concat_soft_light';readonly parameters:{readonly lift?:number;readonly warmth?:number}}
  | {readonly kind:'concat_moody';readonly parameters:{readonly depth?:number;readonly warmth?:number}}
  | {readonly kind:'concat_rain';readonly parameters:{readonly haze?:number;readonly chill?:number}}
  | {readonly kind:'concat_umber';readonly parameters:{readonly warmth?:number;readonly soften?:number}}
  | {readonly kind:'concat_fair';readonly parameters:{readonly brighten?:number;readonly even?:number}}
  | {readonly kind:'concat_peach';readonly parameters:{readonly glow?:number;readonly softness?:number}}
  | {readonly kind:'concat_polaroid';readonly parameters:{readonly fade?:number;readonly warmth?:number}}
  | {readonly kind:'concat_vintage';readonly parameters:{readonly age?:number;readonly tint?:number}}
  | {readonly kind:'concat_cloudless_sky';readonly parameters:{readonly sky?:number;readonly greens?:number}}
  | {readonly kind:'concat_lagoon';readonly parameters:{readonly water?:number;readonly brightness?:number}}
  | {readonly kind:'concat_golden_hour';readonly parameters:{readonly glow?:number;readonly warmth?:number}}
  | {readonly kind:'concat_daydream';readonly parameters:{readonly haze?:number;readonly warmth?:number}}
  | {readonly kind:'concat_clean';readonly parameters:{readonly amount?:number}}
  | {readonly kind:'concat_contrast_pop';readonly parameters:{readonly contrast?:number}}
  | {readonly kind:'concat_vignette';readonly parameters:{readonly strength?:number}}
  | {readonly kind:'concat_adjust';readonly parameters:Readonly<Partial<Record<'exposure'|'brightness'|'contrast'|'saturation'|'temperature'|'tint'|'shadows'|'highlights'|'whites'|'blacks'|'sharpen'|'fade'|'vignette',number>>&Partial<Record<'lift'|'gamma'|'gain',readonly[number,number,number]>>&Partial<Record<'luma'|'red'|'green'|'blue',readonly(readonly[number,number])[]>>>;readonly animation?:Readonly<Partial<Record<'lift.x'|'lift.y'|'lift.m'|'gamma.x'|'gamma.y'|'gamma.m'|'gain.x'|'gain.y'|'gain.m',WireAnimatableValue>>>;readonly concatKeys?:Readonly<Record<string,readonly WireConcatParamKey[]>>}
  | {readonly kind:'concat_curves';readonly parameters:Readonly<Partial<Record<'luma'|'red'|'green'|'blue',readonly (readonly[number,number])[]>>>}
  | {readonly kind:'concat_color_wheels';readonly parameters:Readonly<Partial<Record<'lift'|'gamma'|'gain',readonly [number,number,number]>>>;readonly animation?:Readonly<Partial<Record<'lift.x'|'lift.y'|'lift.m'|'gamma.x'|'gamma.y'|'gamma.m'|'gain.x'|'gain.y'|'gain.m',WireAnimatableValue>>>;readonly concatKeys?:Readonly<Record<string,readonly WireConcatParamKey[]>>}
  | {readonly kind:'concat_white_balance';readonly parameters:{readonly temperature?:number;readonly tint?:number};readonly animation?:Readonly<Partial<Record<'temperature'|'tint',WireAnimatableValue>>>;readonly concatKeys?:Readonly<Record<string,readonly WireConcatParamKey[]>>}
  | {readonly kind:'concat_vibrance';readonly parameters:{readonly amount?:number;readonly intensity?:number};readonly animation?:Readonly<Partial<Record<'amount'|'intensity',WireAnimatableValue>>>}
  | {readonly kind:'concat_exposure';readonly parameters:{readonly stops?:number};readonly animation?:WireAnimatableValue;readonly concatKeys?:readonly WireConcatParamKey[]}
  | {readonly kind:'concat_contrast';readonly parameters:{readonly amount?:number};readonly animation?:WireAnimatableValue;readonly concatKeys?:readonly WireConcatParamKey[]}
  | {readonly kind:'color_sharpen';readonly parameters:{readonly amount?:number};readonly animation?:WireAnimatableValue;readonly colorAnimation?:Partial<Record<"blackR"|"blackG"|"blackB"|"blackA"|"whiteR"|"whiteG"|"whiteB"|"whiteA",WireAnimatableValue>>}
  | {readonly kind:'creative_adjust';readonly parameters:WireCreativeAdjust;readonly animation?:WireAdjustAnimation}
  | {readonly kind:'basic_grade';readonly parameters:WireBasicGrade;readonly animation?:WireBasicAnimation}
  | {readonly kind:'color_vignette';readonly parameters:WireColorVignette;readonly animation?:WireVignetteAnimation}
  | {readonly kind:'hsl_secondary'; readonly parameters:WireHslSecondary; readonly animation?:WireHslAnimation}
  | { readonly kind: 'color_lut'; readonly lut: WireColorLut }
  | { readonly kind: 'creative_grade'; readonly parameters: WireCreativeGrade; readonly animation?: WireCreativeAnimation }
  | { readonly kind: 'tone_curves'; readonly curves: WireToneCurves }
  | {
      readonly kind: 'chroma_matte'
      readonly animation?: {
        readonly keyR?: WireAnimatableValue
        readonly keyG?: WireAnimatableValue
        readonly keyB?: WireAnimatableValue
        readonly balance?: WireAnimatableValue
        readonly clipBlack?: WireAnimatableValue
        readonly clipWhite?: WireAnimatableValue
        readonly despill?: WireAnimatableValue
        readonly feather?: WireAnimatableValue
        readonly shrink?: WireAnimatableValue
        readonly viewMatte?: WireAnimatableValue
      }
      readonly parameters: {
        readonly key: readonly [number, number, number]
        readonly balance: number
        readonly clipBlack: number
        readonly clipWhite: number
        readonly despill: number
        readonly feather: number
        readonly shrink: number
        readonly viewMatte: boolean
      }
    }
  | {
      readonly kind: 'chroma_key'
      readonly color: WireRgba
      readonly similarity: number
      readonly softness: number
      readonly spill: number
    }
  | {
      readonly kind: 'style'
      readonly preset: CompositionVideoEffectPreset
      readonly intensity: number
    }
  | {
      readonly kind: 'mask'
      readonly shape: CompositionMaskShape
      readonly x: WireAnimatableValue
      readonly y: WireAnimatableValue
      readonly width: WireAnimatableValue
      readonly height: WireAnimatableValue
      readonly rotationDegrees: WireAnimatableValue
      readonly feather: number
      readonly inverted: boolean
    }
  | {
      readonly kind: 'linear_mask'
      readonly x: WireAnimatableValue
      readonly y: WireAnimatableValue
      readonly rotationDegrees: WireAnimatableValue
      readonly feather: number
      readonly inverted: boolean
    }

export interface WireClipTransition {
  readonly concatOverlap?:{readonly leadTicks:number;readonly durationTicks:number;readonly holdOutgoing?:boolean;readonly authoredDurationTicks?:number;readonly legacy?:'cross-fade'|'push'|'zoom'|'wipe-left'|'wipe-right'|'fade-black'|'fade-white'}
  readonly reverse?: boolean
  readonly timeline?: TransitionTimeline
  readonly id: StableId
  readonly fromClipId: StableId | null
  readonly toClipId: StableId | null
  readonly durationTicks: Tick
  readonly kind: CompositionTransitionKind
}

export interface WireColorClip {
  readonly id: StableId
  readonly timelineStartTick: Tick
  readonly timelineEndTick: Tick
  readonly width: number
  readonly height: number
  readonly color: import('../api').NativeColor
  readonly colorAnimation?: import('../api').NativeColorTrack
  readonly transform: WireTransform
  readonly opacity: WireAnimatableValue
  readonly enabled: boolean
}

export interface WireConcatShapeStyle {
  readonly kind?:'square'|'circle'|'triangle'|'parallelogram'|'trapezoid'|'line'|'arrow'
  readonly size?:number;readonly fill?:string;readonly stroke?:string;readonly strokeWidth?:number
}
export interface WireGraphicClip extends Omit<WireColorClip, 'color' | 'colorAnimation'> {
  readonly concatTreatment?:{readonly strength:number;readonly fadeIn:number;readonly fadeOut:number;readonly durationTicks:number}
  readonly concatShape?:WireConcatShapeStyle
  readonly concatShapeBlend?:CompositionBlendMode
  readonly animationOffsetUs?: number
  readonly graphic: { readonly layers: readonly GraphicLayer[] }
  readonly responsiveTime?: GraphicResponsiveTime
  readonly roll?: GraphicRoll
  readonly template?: GraphicTemplateMetadata
  readonly effects: readonly WireVideoEffect[]
}

export type WireCompositionTrack =
  | { readonly kind: 'graphic'; readonly id: StableId; readonly name: string; readonly hidden: boolean; readonly locked: boolean; readonly clips: readonly WireGraphicClip[] }
  | { readonly kind: 'color'; readonly id: StableId; readonly name: string; readonly hidden: boolean; readonly locked: boolean; readonly clips: readonly WireColorClip[] }
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
  readonly captionTracks?: readonly CaptionTrack[]
  readonly metadata?:CompositionMetadata
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
  if ((clip.kind === 'video' || clip.kind === 'audio') && clip.timelineDurationTicks !== undefined) return clip.timelineDurationTicks
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

export function cutoutMaskSize(modelHash:string):number{return modelHash==='3241ac4ad8aa35bdaf33946776db29f7c283a413aa0b0dacb9483594b4531aad'?256:512}

export const CUTOUT_PERSON_MODEL_HASH='88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828'
/** Original RVM grid; invalid input cannot accidentally match a valid mask. */
export function cutoutMaskDimensions(modelHash:string,width:unknown,height:unknown):readonly [number,number]{
 if(typeof width!=='number'||typeof height!=='number'||!Number.isSafeInteger(width)||!Number.isSafeInteger(height)||width<=0||height<=0)return [0,0]
 if(modelHash!==CUTOUT_PERSON_MODEL_HASH){const size=cutoutMaskSize(modelHash);return [size,size]}
 const scale=512/Math.max(width,height),edge=(value:number)=>Math.max(64,Math.round(value*scale/16)*16)
 return [edge(width),edge(height)]
}

export interface WireSphericalVideo {
 readonly sourceLayout?:"packed"|"dual_video_streams"
 readonly input?: "equirectangular"|"dual_fisheye"
 readonly output?: "perspective"|"equirectangular"|"tiny_planet"
 readonly pan?:number;readonly tilt?:number;readonly roll?:number;readonly fov?:number
 readonly lenses?:readonly [WireFisheyeLens,WireFisheyeLens]
 readonly cameraProfile?:WireInsta360CameraProfile
 readonly orientation?:readonly {readonly timeSeconds:number;readonly quaternion:readonly [number,number,number,number]}[]
 readonly stabilizationMode?:"disabled"|"locked"|"smooth"|"horizon"
 readonly smoothSeconds?:number
 readonly stitching?:{readonly mode?:"feather"|"optical_flow";readonly searchRadius?:number;readonly strength?:number;readonly occlusionStrength?:number}
 readonly sourceFrameClock?:WireSourceFrameClock
 readonly rollingShutter?:WireRollingShutter
 readonly orientationOffsetSeconds?:number
}
export interface WireFisheyeLens {readonly center:readonly [number,number];readonly radius:readonly [number,number];readonly fov:number;readonly rotation:readonly [number,number,number]}
export interface WireInsta360OmniLens {
 readonly xi:number
 readonly focal:readonly [number,number]
 readonly principal:readonly [number,number]
 readonly eulerDegrees:readonly [number,number,number]
 readonly translation:readonly [number,number,number]
 readonly distortion:readonly [number,number,number,number,number,number,number,number,number,number,number,number,number]
 readonly lensType:number
 readonly angularPolynomial?:readonly [number,number,number,number]
}
export interface WireInsta360CameraProfile {
 readonly model:"insta360_v6_omni"|"insta360_legacy_polynomial"
 readonly referenceSize:readonly [number,number]
 readonly lenses:readonly [WireInsta360OmniLens,WireInsta360OmniLens]
 readonly flags:number
}

export interface WireSourceFrameClock {readonly numerator:number;readonly denominator:number;readonly sourceStartSeconds:number}
export interface WireCaptureTime {readonly timeSeconds:number;readonly captureSeconds:number}
export interface WireRollingShutter {readonly readoutSeconds:number;readonly referenceRow?:number;readonly scanDirections?:readonly ['top_to_bottom'|'bottom_to_top','top_to_bottom'|'bottom_to_top'];readonly captureClock?:readonly WireCaptureTime[]}
