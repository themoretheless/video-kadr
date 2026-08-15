import {
  COMPOSITION_SCHEMA_VERSION,
  type Composition,
  type CompositionQualityTier,
  type CompositionRenderOutput,
  type CompositionRenderRequest,
  type CompositionSource,
  type CompositionSourceRegistry,
  type CompositionTrack,
  type AudioClip,
  type VideoClip,
  type VisualClip,
  type WireAnimatableValue,
  type WireComposition,
  type WireCompositionTrack,
  type WireClipPlacement,
  type WireRgba,
  type WireTransform,
  type WireVideoEffect,
} from './types'
import { audioPropertyValue, normalizeInterpolation, visualPropertyValue } from './keyframes'
import {
  assertValidComposition,
  compositionClipCount,
  compositionRenderUnavailableReason,
  primaryCompositionVideoTrack,
} from './validation'

export const COMPOSITION_RENDER_ENDPOINT = '/api/compositions/render' as const

export const DEFAULT_COMPOSITION_RENDER_OUTPUT: CompositionRenderOutput = {
  profile: { container: 'mp4', codec: 'h264' },
  qualityTier: 'medium',
}

export class CompositionPayloadError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'CompositionPayloadError'
  }
}

/**
 * Build the strict render wire document by copying only contract fields. Even
 * if runtime state was polluted with `path`, `url`, or framework metadata,
 * those values cannot cross the API boundary.
 */
export function buildCompositionRenderRequest(
  composition: Composition,
  output: CompositionRenderOutput = DEFAULT_COMPOSITION_RENDER_OUTPUT,
): CompositionRenderRequest {
  assertValidComposition(composition)
  if (compositionClipCount(composition) === 0) {
    throw new CompositionPayloadError('Cannot render an empty composition')
  }
  const unavailableReason = compositionRenderUnavailableReason(composition)
  if (unavailableReason) throw new CompositionPayloadError(unavailableReason)
  validateOutput(output)

  const primaryTrackId = primaryCompositionVideoTrack(composition)?.id

  const wire: WireComposition = {
    schemaVersion: COMPOSITION_SCHEMA_VERSION,
    timeBase: composition.timeBase,
    canvas: {
      width: composition.canvas.width,
      height: composition.canvas.height,
      fpsMilli: Math.round(composition.canvas.fps * 1_000),
      background: colorToRgba(composition.canvas.backgroundColor),
    },
    sources: copySources(composition.sources),
    tracks: composition.tracks.map((track) => copyTrack(track, composition, track.id === primaryTrackId)),
  }
  return {
    schemaVersion: COMPOSITION_SCHEMA_VERSION,
    composition: wire,
    output: {
      profile: { ...output.profile },
      qualityTier: output.qualityTier,
    },
  }
}

/** Alias matching existing `build*Payload` call sites. */
export const buildCompositionRenderPayload = buildCompositionRenderRequest

function copySources(sources: CompositionSourceRegistry): CompositionSourceRegistry {
  return Object.fromEntries(
    Object.keys(sources)
      .sort()
      .map((id) => {
        const source = sources[id]!
        const wireSource: CompositionSource = {
          id: source.id,
          kind: source.kind,
          durationTicks: source.durationTicks,
          width: source.width,
          height: source.height,
          hasAudio: source.hasAudio,
        }
        return [id, wireSource]
      }),
  )
}

function copyTrack(track: CompositionTrack, composition: Composition, primary: boolean): WireCompositionTrack {
  switch (track.kind) {
    case 'video':
      return {
        id: track.id,
        kind: 'video',
        name: track.name,
        locked: track.locked,
        hidden: track.hidden,
        muted: track.muted,
        clips: track.clips.map((clip) => ({
          id: clip.id,
          sourceId: clip.sourceId,
          placement: copyPlacement(clip),
          transform: primary ? defaultTransform() : transformToWire(composition, clip),
          opacity: primary ? constant(1) : visualValueToWire(composition, clip, 'opacity'),
          blendMode: clip.blendMode ?? 'normal',
          effects: [...chromaEffects(clip), ...maskEffects(clip)],
          sourceAudioEnabled: clip.sourceAudioEnabled,
          audioGain: copyAnimatable(audioPropertyValue(clip, 'gain')),
          audioPan: copyAnimatable(audioPropertyValue(clip, 'pan')),
          frameInterpolation: clip.frameInterpolation ?? 'duplicate',
          playbackMode: clip.playbackMode?.mode === 'freeze'
            ? { mode: 'freeze', sourceTick: clip.playbackMode.sourceTick }
            : { mode: clip.playbackMode?.mode ?? 'forward' },
          stabilization: clip.stabilization?.mode === 'deshake'
            ? {
                mode: 'deshake',
                radiusX: clip.stabilization.radiusX,
                radiusY: clip.stabilization.radiusY,
              }
            : { mode: 'disabled' },
          enabled: true,
        })),
        transitions: (track.transitions ?? []).map((transition) => ({ ...transition })),
      }
    case 'audio':
      return {
        id: track.id,
        kind: 'audio',
        name: track.name,
        locked: track.locked,
        muted: track.muted,
        solo: track.solo ?? false,
        clips: track.clips.map((clip) => ({
          id: clip.id,
          sourceId: clip.sourceId,
          placement: copyPlacement(clip),
          gain: copyAnimatable(audioPropertyValue(clip, 'gain')),
          pan: copyAnimatable(audioPropertyValue(clip, 'pan')),
          fadeInTicks: clip.fadeInTicks ?? 0,
          fadeOutTicks: clip.fadeOutTicks ?? 0,
          enabled: true,
        })),
      }
    case 'image':
      return {
        id: track.id,
        kind: 'image',
        name: track.name,
        locked: track.locked,
        hidden: track.hidden,
        clips: track.clips.map((clip) => ({
          id: clip.id,
          sourceId: clip.sourceId,
          timelineStartTick: clip.timelineStartTicks,
          durationTicks: clip.durationTicks,
          transform: transformToWire(composition, clip),
          opacity: visualValueToWire(composition, clip, 'opacity'),
          blendMode: clip.blendMode ?? 'normal',
          enabled: true,
        })),
      }
    case 'text':
      return {
        id: track.id,
        kind: 'text',
        name: track.name,
        locked: track.locked,
        hidden: track.hidden,
        clips: track.clips.map((clip) => ({
          id: clip.id,
          timelineStartTick: clip.timelineStartTicks,
          timelineEndTick: clip.timelineStartTicks + clip.durationTicks,
          text: clip.text,
          style: {
            fontFamily: clip.style.fontFamily ?? 'Noto Sans',
            fontSize: clip.style.fontSizePx,
            color: colorToRgba(clip.style.color),
            background: colorToRgba(clip.style.backgroundColor ?? '#00000000'),
            stroke: colorToRgba(clip.style.strokeColor ?? '#00000000'),
            strokeWidth: clip.style.strokeWidthPx ?? 0,
            shadow: colorToRgba(clip.style.shadowColor ?? '#00000000'),
            shadowX: clip.style.shadowX ?? 0,
            shadowY: clip.style.shadowY ?? 0,
          },
          transform: transformToWire(composition, clip),
          opacity: visualValueToWire(composition, clip, 'opacity'),
          enabled: true,
        })),
      }
  }
}

function copyPlacement(clip: VideoClip | AudioClip): WireClipPlacement {
  return {
    timelineStartTick: clip.timelineStartTicks,
    sourceInTick: clip.sourceInTicks,
    sourceOutTick: clip.sourceOutTicks,
    speed: clip.speed ?? 1,
    ...(clip.speedRamp ? {
      speedRamp: {
        interpolation: clip.speedRamp.interpolation,
        points: clip.speedRamp.points.map((point) => ({ ...point })),
        audioPolicy: clip.speedRamp.audioPolicy ?? 'preserve_pitch',
      },
    } : {}),
  }
}

function constant(value: number): WireAnimatableValue {
  return { mode: 'constant', value }
}

function defaultTransform(): WireTransform {
  return {
    x: constant(0),
    y: constant(0),
    scaleX: constant(1),
    scaleY: constant(1),
    rotationDegrees: constant(0),
    anchorX: 0.5,
    anchorY: 0.5,
  }
}

function transformToWire(composition: Composition, clip: VisualClip): WireTransform {
  return {
    ...defaultTransform(),
    x: visualValueToWire(composition, clip, 'x'),
    y: visualValueToWire(composition, clip, 'y'),
    scaleX: visualValueToWire(composition, clip, 'scaleX'),
    scaleY: visualValueToWire(composition, clip, 'scaleY'),
    rotationDegrees: visualValueToWire(composition, clip, 'rotationDegrees'),
  }
}

function visualValueToWire(
  composition: Composition,
  clip: VisualClip,
  property: import('./types').CompositionVisualProperty,
): WireAnimatableValue {
  return copyAnimatable(visualPropertyValue(composition, clip, property))
}

function copyAnimatable(value: WireAnimatableValue): WireAnimatableValue {
  return value.mode === 'constant'
    ? { mode: 'constant', value: value.value }
    : {
        mode: 'keyframes',
        track: {
          timeBase: value.track.timeBase,
          interpolation: normalizeInterpolation(value.track.interpolation),
          keyframes: value.track.keyframes.map((keyframe) => ({ tick: keyframe.tick, value: keyframe.value })),
        },
      }
}

function chromaEffects(clip: VideoClip): WireVideoEffect[] {
  const chroma = clip.chromaKey
  return chroma?.enabled
    ? [{
        kind: 'chroma_key' as const,
        color: colorToRgba(chroma.color),
        similarity: chroma.similarity,
        softness: chroma.softness,
        spill: chroma.spill,
      }]
    : []
}

function maskEffects(clip: VideoClip): WireVideoEffect[] {
  return (clip.masks ?? []).map((mask) => ({
    kind: 'mask',
    shape: mask.shape,
    x: copyAnimatable(mask.x),
    y: copyAnimatable(mask.y),
    width: copyAnimatable(mask.width),
    height: copyAnimatable(mask.height),
    feather: mask.feather,
    inverted: mask.inverted,
  }))
}

function colorToRgba(color: string): WireRgba {
  const hex = color.slice(1)
  const alpha = hex.length === 8 ? Number.parseInt(hex.slice(6, 8), 16) / 255 : 1
  return {
    red: Number.parseInt(hex.slice(0, 2), 16) / 255,
    green: Number.parseInt(hex.slice(2, 4), 16) / 255,
    blue: Number.parseInt(hex.slice(4, 6), 16) / 255,
    alpha,
  }
}

function validateOutput(output: CompositionRenderOutput): void {
  if (
    !isRecord(output) ||
    !hasExactKeys(output, ['profile', 'qualityTier']) ||
    !isDeliveryProfile(output.profile) ||
    !isQualityTier(output.qualityTier)
  ) {
    throw new CompositionPayloadError('Composition output must use a canonical delivery profile and known quality tier')
  }
}

function isQualityTier(value: unknown): value is CompositionQualityTier {
  return value === 'high' || value === 'medium' || value === 'compact'
}

function isDeliveryProfile(value: unknown): value is CompositionRenderOutput['profile'] {
  if (!isRecord(value) || typeof value.container !== 'string') return false
  if (value.container === 'mp4') {
    return hasExactKeys(value, ['container', 'codec']) && (value.codec === 'h264' || value.codec === 'h265')
  }
  if (value.container === 'webm') {
    return hasExactKeys(value, ['container', 'codec']) && (value.codec === 'vp9' || value.codec === 'av1')
  }
  if (value.container === 'mov') {
    return hasExactKeys(value, ['container', 'profile']) &&
      (value.profile === 'proxy' || value.profile === 'lt' || value.profile === 'standard' || value.profile === 'hq')
  }
  return false
}

/** Tolerant persisted-state migration; API payload validation remains strict. */
export function normalizeCompositionRenderOutput(value: unknown): CompositionRenderOutput {
  const qualityTier = isRecord(value) && isQualityTier(value.qualityTier)
    ? value.qualityTier
    : DEFAULT_COMPOSITION_RENDER_OUTPUT.qualityTier
  if (isRecord(value) && isDeliveryProfile(value.profile)) {
    return { profile: { ...value.profile }, qualityTier }
  }
  if (
    isRecord(value) &&
    (value.format === undefined || value.format === 'mp4') &&
    (value.codec === undefined || value.codec === 'h264')
  ) {
    return { profile: { ...DEFAULT_COMPOSITION_RENDER_OUTPUT.profile }, qualityTier }
  }
  return {
    profile: { ...DEFAULT_COMPOSITION_RENDER_OUTPUT.profile },
    qualityTier: DEFAULT_COMPOSITION_RENDER_OUTPUT.qualityTier,
  }
}

function hasExactKeys(value: Record<string, unknown>, fields: readonly string[]): boolean {
  const keys = Object.keys(value)
  return keys.length === fields.length && fields.every((field) => Object.hasOwn(value, field))
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
