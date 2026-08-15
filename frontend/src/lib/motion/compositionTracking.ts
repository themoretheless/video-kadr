import { sampleVisualProperty } from '../composition/keyframes'
import {
  clipEndTicks,
  COMPOSITION_TIME_BASE,
  type Composition,
  type CompositionKeyframeTrack,
  type CompositionVisualAnimation,
  type Tick,
  type VideoClip,
  type VisualClip,
} from '../composition/types'
import { primaryCompositionVideoTrack } from '../composition/validation'
import { trackedPointsToPositionKeyframes, type PositionKeyframeTracks } from './keyframes'
import type { PointTrackResult } from './tracker'

export const DEFAULT_COMPOSITION_TRACKING_FPS = 10
export const MAX_COMPOSITION_TRACKING_FPS = 30
export const MAX_COMPOSITION_TRACKING_SAMPLES = 300

export interface CompositionTrackingRange {
  readonly startTicks?: Tick
  readonly endTicks?: Tick
  readonly sampleFps?: number
}

export interface CompositionPointTrackingPlan {
  readonly sourceClipId: string
  readonly targetClipId: string
  readonly startTicks: Tick
  readonly endTicks: Tick
  readonly sampleFps: number
  readonly sampleTimelineTicks: readonly Tick[]
  readonly sampleSourceSeconds: readonly number[]
  readonly targetLocalStartTicks: Tick
  readonly sourcePixelToCanvasScale: number
  readonly targetOriginX: number
  readonly targetOriginY: number
}

/**
 * Build the deterministic bridge between browser frame sampling and the
 * composition timeline. The first slice intentionally tracks the primary,
 * ordinary forward clip: deshake/reverse/freeze and transition handles change
 * the rendered temporal/spatial domain and therefore fail closed here.
 */
export function buildCompositionPointTrackingPlan(
  composition: Composition,
  sourceClipId: string,
  targetClipId: string,
  range: CompositionTrackingRange = {},
): CompositionPointTrackingPlan {
  const primaryTrack = primaryCompositionVideoTrack(composition)
  if (!primaryTrack) throw new Error('Для tracking нужна первичная video-дорожка')
  const sourceClip = primaryTrack.clips.find((clip) => clip.id === sourceClipId)
  if (!sourceClip) throw new Error('Tracking source должен быть клипом первичной video-дорожки')
  const targetClip = findVisualClip(composition, targetClipId)
  if (!targetClip || targetClip.id === sourceClip.id) {
    throw new Error('Выберите отдельный visual overlay для привязки к tracking point')
  }
  assertTrackableSource(primaryTrack.transitions ?? [], sourceClip)

  const source = composition.sources[sourceClip.sourceId]
  if (!source || source.kind !== 'video' || source.width <= 0 || source.height <= 0) {
    throw new Error('Tracking source не содержит проверенного video raster')
  }
  const overlapStart = Math.max(sourceClip.timelineStartTicks, targetClip.timelineStartTicks)
  const overlapEnd = Math.min(clipEndTicks(sourceClip), clipEndTicks(targetClip))
  const startTicks = range.startTicks ?? overlapStart
  const endTicks = range.endTicks ?? overlapEnd
  if (
    !Number.isSafeInteger(startTicks) ||
    !Number.isSafeInteger(endTicks) ||
    startTicks < overlapStart ||
    endTicks > overlapEnd ||
    endTicks <= startTicks
  ) {
    throw new Error('Tracking range должен полностью лежать в пересечении source и target clips')
  }

  const sampleFps = range.sampleFps ?? Math.min(DEFAULT_COMPOSITION_TRACKING_FPS, Math.max(1, Math.floor(composition.canvas.fps)))
  if (!Number.isSafeInteger(sampleFps) || sampleFps < 1 || sampleFps > MAX_COMPOSITION_TRACKING_FPS) {
    throw new Error(`Tracking sample FPS должен быть целым числом 1..=${MAX_COMPOSITION_TRACKING_FPS}`)
  }
  const intervalTicks = Math.round(COMPOSITION_TIME_BASE / sampleFps)
  const sampleTimelineTicks: number[] = []
  for (let tick = startTicks; tick < endTicks; tick += intervalTicks) {
    sampleTimelineTicks.push(tick)
    if (sampleTimelineTicks.length > MAX_COMPOSITION_TRACKING_SAMPLES) {
      throw new Error(`Tracking range превышает лимит ${MAX_COMPOSITION_TRACKING_SAMPLES} кадров`)
    }
  }
  if (sampleTimelineTicks.length < 2) throw new Error('Tracking range должен содержать минимум два sample frame')

  const speed = sourceClip.speed ?? 1
  const sampleSourceSeconds = sampleTimelineTicks.map((timelineTick) => {
    const sourceTick = sourceClip.sourceInTicks + (timelineTick - sourceClip.timelineStartTicks) * speed
    return Math.min(sourceClip.sourceOutTicks - 1, sourceTick) / COMPOSITION_TIME_BASE
  })
  const sourcePixelToCanvasScale = Math.min(
    composition.canvas.width / source.width,
    composition.canvas.height / source.height,
  )
  const targetLocalStartTicks = startTicks - targetClip.timelineStartTicks

  return {
    sourceClipId,
    targetClipId,
    startTicks,
    endTicks,
    sampleFps,
    sampleTimelineTicks,
    sampleSourceSeconds,
    targetLocalStartTicks,
    sourcePixelToCanvasScale,
    targetOriginX: sampleVisualProperty(composition, targetClip, 'x', targetLocalStartTicks),
    targetOriginY: sampleVisualProperty(composition, targetClip, 'y', targetLocalStartTicks),
  }
}

/** Convert a completed raw-raster point track into paired target-local X/Y keyframes. */
export function pointTrackToTargetAnimation(
  plan: CompositionPointTrackingPlan,
  result: PointTrackResult,
  existing: CompositionVisualAnimation = {},
  tolerancePixels = 1.5,
  sampleToSourceScale: Readonly<{ x: number; y: number }> = { x: 1, y: 1 },
): CompositionVisualAnimation {
  if (result.status === 'lost') {
    throw new Error(`Tracking потерял точку на sample frame ${result.lostAtFrame ?? result.points.length}`)
  }
  if (result.points.length !== plan.sampleTimelineTicks.length) {
    throw new Error('Tracking result не совпадает с запланированным числом кадров')
  }
  if (
    !Number.isFinite(sampleToSourceScale.x) || sampleToSourceScale.x <= 0 ||
    !Number.isFinite(sampleToSourceScale.y) || sampleToSourceScale.y <= 0
  ) {
    throw new Error('Tracking raster scale должен быть положительным')
  }
  const tracks = trackedPointsToPositionKeyframes(
    result.points,
    plan.sampleFps,
    COMPOSITION_TIME_BASE,
    tolerancePixels,
  )
  const initialX = result.points[0]!.x
  const initialY = result.points[0]!.y
  return {
    ...existing,
    x: {
      mode: 'keyframes',
      track: translateTrack(
        tracks.x,
        plan.targetLocalStartTicks,
        plan.targetOriginX,
        initialX,
        plan.sourcePixelToCanvasScale * sampleToSourceScale.x,
      ),
    },
    y: {
      mode: 'keyframes',
      track: translateTrack(
        tracks.y,
        plan.targetLocalStartTicks,
        plan.targetOriginY,
        initialY,
        plan.sourcePixelToCanvasScale * sampleToSourceScale.y,
      ),
    },
  }
}

function assertTrackableSource(
  transitions: readonly { readonly fromClipId: string; readonly toClipId: string }[],
  clip: VideoClip,
): void {
  if ((clip.playbackMode?.mode ?? 'forward') !== 'forward') {
    throw new Error('Classical tracking пока поддерживает только forward playback')
  }
  if ((clip.stabilization?.mode ?? 'disabled') !== 'disabled') {
    throw new Error('Отключите stabilization на tracking source: она меняет координаты экспорта')
  }
  if ('speedRamp' in clip && clip.speedRamp != null) {
    throw new Error('Speed-ramp source требует отдельного temporal sampler')
  }
  if (transitions.some((transition) => transition.fromClipId === clip.id || transition.toClipId === clip.id)) {
    throw new Error('Tracking source с transition handles пока не поддерживается')
  }
}

function findVisualClip(composition: Composition, id: string): VisualClip | undefined {
  for (const track of composition.tracks) {
    if (track.kind === 'audio') continue
    const clip = track.clips.find((candidate) => candidate.id === id)
    if (clip) return clip
  }
  return undefined
}

function translateTrack(
  track: PositionKeyframeTracks['x'],
  tickOffset: number,
  targetOrigin: number,
  trackedOrigin: number,
  scale: number,
): CompositionKeyframeTrack {
  return {
    timeBase: track.timeBase,
    interpolation: track.interpolation,
    keyframes: track.keyframes.map((keyframe) => ({
      tick: tickOffset + keyframe.tick,
      value: targetOrigin + (keyframe.value - trackedOrigin) * scale,
    })),
  }
}
