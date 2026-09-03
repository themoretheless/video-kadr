import { sampleVisualProperty } from '../composition/keyframes'
import { videoSourceTickAtTimelineTick } from '../composition/playback'
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
  readonly sourceCanvasTransforms: readonly SourceCanvasTransform[]
  readonly targetOriginX: number
  readonly targetOriginY: number
}

export interface SourceCanvasTransform {
  readonly centerX: number
  readonly centerY: number
  readonly scaleX: number
  readonly scaleY: number
  readonly rotationRadians: number
  readonly sourceCenterX: number
  readonly sourceCenterY: number
}

/**
 * Build the deterministic bridge between browser frame sampling and the
 * composition timeline. It tracks any visible ordinary video clip, including
 * reverse playback and Hold/Linear speed ramps. Deshake/freeze change the rendered
 * spatial/temporal domain and therefore fail closed here. Transitions are
 * safe because the tracking range remains inside the clip's nominal timeline
 * interval; source handles outside that interval are never sampled.
 */
export function buildCompositionPointTrackingPlan(
  composition: Composition,
  sourceClipId: string,
  targetClipId: string,
  range: CompositionTrackingRange = {},
): CompositionPointTrackingPlan {
  const sourceClip = findTrackableVideoClip(composition, sourceClipId)
  if (!sourceClip) throw new Error('Tracking source должен быть видимым video clip')
  const targetClip = findVisualClip(composition, targetClipId)
  if (!targetClip || targetClip.id === sourceClip.id) {
    throw new Error('Выберите отдельный visual overlay для привязки к tracking point')
  }
  assertTrackableSource(sourceClip)

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

  const sampleSourceSeconds = sampleTimelineTicks.map((timelineTick) =>
    videoSourceTickAtTimelineTick(sourceClip, timelineTick) / COMPOSITION_TIME_BASE,
  )
  const sourcePixelToCanvasScale = Math.min(
    composition.canvas.width / source.width,
    composition.canvas.height / source.height,
  )
  const sourceCanvasTransforms = sampleTimelineTicks.map((timelineTick): SourceCanvasTransform => {
    const localTick = timelineTick - sourceClip.timelineStartTicks
    return {
      centerX: composition.canvas.width / 2 + sampleVisualProperty(composition, sourceClip, 'x', localTick),
      centerY: composition.canvas.height / 2 + sampleVisualProperty(composition, sourceClip, 'y', localTick),
      scaleX: sampleVisualProperty(composition, sourceClip, 'scaleX', localTick),
      scaleY: sampleVisualProperty(composition, sourceClip, 'scaleY', localTick),
      rotationRadians: sampleVisualProperty(composition, sourceClip, 'rotationDegrees', localTick) * Math.PI / 180,
      sourceCenterX: source.width / 2,
      sourceCenterY: source.height / 2,
    }
  })
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
    sourceCanvasTransforms,
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
  const canvasPoints = result.points.map((point, index) => {
    const transform = plan.sourceCanvasTransforms[index]
    if (!transform) throw new Error('Tracking plan не содержит canvas transform для sample frame')
    const dx = point.x * sampleToSourceScale.x - transform.sourceCenterX
    const dy = point.y * sampleToSourceScale.y - transform.sourceCenterY
    const cosine = Math.cos(transform.rotationRadians)
    const sine = Math.sin(transform.rotationRadians)
    return {
      ...point,
      x: transform.centerX + dx * transform.scaleX * cosine - dy * transform.scaleY * sine,
      y: transform.centerY + dx * transform.scaleX * sine + dy * transform.scaleY * cosine,
    }
  })
  const tracks = trackedPointsToPositionKeyframes(
    canvasPoints,
    plan.sampleFps,
    COMPOSITION_TIME_BASE,
    tolerancePixels,
  )
  const initialX = canvasPoints[0]!.x
  const initialY = canvasPoints[0]!.y
  return {
    ...existing,
    x: {
      mode: 'keyframes',
      track: translateTrack(
        tracks.x,
        plan.targetLocalStartTicks,
        plan.targetOriginX,
        initialX,
        1,
      ),
    },
    y: {
      mode: 'keyframes',
      track: translateTrack(
        tracks.y,
        plan.targetLocalStartTicks,
        plan.targetOriginY,
        initialY,
        1,
      ),
    },
  }
}

function assertTrackableSource(clip: VideoClip): void {
  if (clip.playbackMode?.mode === 'freeze') {
    throw new Error('Freeze source не содержит temporal motion для tracking')
  }
  if ((clip.stabilization?.mode ?? 'disabled') !== 'disabled') {
    throw new Error('Отключите stabilization на tracking source: она меняет координаты экспорта')
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

function findTrackableVideoClip(composition: Composition, id: string): VideoClip | undefined {
  for (const track of composition.tracks) {
    if (track.kind !== 'video' || track.hidden) continue
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
