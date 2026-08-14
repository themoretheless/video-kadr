export const COMPOSITION_SCHEMA_VERSION = 1 as const
export const COMPOSITION_TIME_BASE = 1_000_000 as const

export const MAX_COMPOSITION_TRACKS = 16
export const MAX_COMPOSITION_CLIPS = 512
export const MAX_COMPOSITION_SOURCES = 32
export const MAX_VISIBLE_VISUAL_TRACKS = 8
export const MAX_COMPOSITION_DURATION_TICKS = 24 * 60 * 60 * COMPOSITION_TIME_BASE
export const MAX_TEXT_LENGTH = 512

export type Tick = number
export type StableId = string
export type SourceKind = 'video' | 'audio' | 'image'
export type TrackKind = SourceKind | 'text'
export type VisualFit = 'contain' | 'cover'

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
}

export interface VideoClip extends SourceClipTiming {
  readonly id: StableId
  readonly kind: 'video'
  readonly transform: VisualTransform
  readonly opacity: number
  readonly sourceAudioEnabled: boolean
  readonly audioGain: number
}

export interface AudioClip extends SourceClipTiming {
  readonly id: StableId
  readonly kind: 'audio'
  readonly gain: number
}

export interface ImageClip {
  readonly id: StableId
  readonly kind: 'image'
  readonly sourceId: StableId
  readonly timelineStartTicks: Tick
  readonly durationTicks: Tick
  readonly transform: VisualTransform
  readonly opacity: number
}

export interface TextStyle {
  readonly fontSizePx: number
  readonly color: string
  readonly backgroundColor?: string
  readonly align: 'left' | 'center' | 'right'
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
  readonly style: TextStyle
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
}

export interface AudioTrack extends TrackBase<'audio', AudioClip> {
  readonly muted: boolean
}

export interface ImageTrack extends TrackBase<'image', ImageClip> {
  readonly hidden: boolean
}

export interface TextTrack extends TrackBase<'text', TextClip> {
  readonly hidden: boolean
}

export type CompositionTrack = VideoTrack | AudioTrack | ImageTrack | TextTrack
export type VisualTrack = VideoTrack | ImageTrack | TextTrack

/** Top-to-bottom track order: the lowest array index is the highest visual layer. */
export interface Composition {
  readonly schemaVersion: typeof COMPOSITION_SCHEMA_VERSION
  readonly timeBase: typeof COMPOSITION_TIME_BASE
  readonly canvas: CompositionCanvas
  readonly sources: CompositionSourceRegistry
  readonly tracks: readonly CompositionTrack[]
}

export type CompositionQualityTier = 'high' | 'medium' | 'compact'

export interface CompositionRenderOutput {
  readonly format: 'mp4'
  readonly codec: 'h264'
  readonly qualityTier: CompositionQualityTier
}

export interface WireComposition {
  readonly timeBase: typeof COMPOSITION_TIME_BASE
  readonly canvas: CompositionCanvas
  readonly sources: CompositionSourceRegistry
  readonly tracks: readonly CompositionTrack[]
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
    ? clip.sourceOutTicks - clip.sourceInTicks
    : clip.durationTicks
}

export function clipEndTicks(clip: CompositionClip): Tick {
  return clip.timelineStartTicks + clipDurationTicks(clip)
}
