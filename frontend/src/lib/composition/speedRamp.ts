import type {
  CompositionSpeedRamp,
  CompositionSpeedRampPoint,
  CompositionSpeedRampInterpolation,
} from './types'

export const MIN_SPEED_RAMP_SPEED = 0.05
export const MAX_SPEED_RAMP_SPEED = 16
export const MIN_SPEED_RAMP_SEGMENT_TICKS = 1_000
export const MAX_SPEED_RAMP_CLIP_TICKS = 2 * 60 * 60 * 1_000_000
export const MAX_SPEED_RAMP_POINTS = 32

export interface CompositionSpeedRampSegment {
  readonly sourceStartTick: number
  readonly sourceEndTick: number
  readonly timelineStartTick: number
  readonly timelineEndTick: number
  readonly startSpeed: number
  readonly endSpeed: number
  readonly interpolation: CompositionSpeedRampInterpolation
}

export interface SlicedCompositionSpeedRamp {
  readonly speed: number
  readonly speedRamp: CompositionSpeedRamp
}

/** Exact frontend mirror of the backend cumulative reciprocal-speed integral. */
export function compositionSpeedRampSegments(
  sourceSpanTicks: number,
  baselineSpeed: number,
  speedRamp: CompositionSpeedRamp,
): readonly CompositionSpeedRampSegment[] {
  assertSafeTick(sourceSpanTicks, 'source span')
  if (sourceSpanTicks <= 0 || sourceSpanTicks > MAX_SPEED_RAMP_CLIP_TICKS) {
    throw new Error('Speed ramp source span выходит за пределы 2 часов')
  }
  assertSpeed(baselineSpeed, 'baseline speed')
  if (speedRamp.interpolation !== 'hold' && speedRamp.interpolation !== 'linear') {
    throw new Error('Speed ramp interpolation должна быть hold или linear')
  }
  if (speedRamp.audioPolicy !== undefined && speedRamp.audioPolicy !== 'preserve_pitch' && speedRamp.audioPolicy !== 'mute') {
    throw new Error('Speed ramp audio policy должна быть preserve_pitch или mute')
  }
  if (!Array.isArray(speedRamp.points) || speedRamp.points.length < 2 || speedRamp.points.length > MAX_SPEED_RAMP_POINTS) {
    throw new Error(`Speed ramp должна содержать 2..${MAX_SPEED_RAMP_POINTS} points`)
  }
  const first = speedRamp.points[0]
  const last = speedRamp.points.at(-1)
  if (!first || first.sourceProgressTick !== 0 || first.speed !== baselineSpeed) {
    throw new Error('Первая speed ramp point должна быть (0, clip.speed)')
  }
  if (!last || last.sourceProgressTick !== sourceSpanTicks) {
    throw new Error('Последняя speed ramp point должна совпадать с source span')
  }

  let cumulativeOutput = 0
  let previousOutputTick = 0
  const segments: CompositionSpeedRampSegment[] = []
  for (let index = 0; index < speedRamp.points.length; index += 1) {
    const point = speedRamp.points[index]!
    assertSafeTick(point.sourceProgressTick, `point ${index} tick`)
    assertSpeed(point.speed, `point ${index} speed`)
    if (index === 0) continue
    const start = speedRamp.points[index - 1]!
    const sourceTicks = point.sourceProgressTick - start.sourceProgressTick
    if (!Number.isSafeInteger(sourceTicks) || sourceTicks < MIN_SPEED_RAMP_SEGMENT_TICKS) {
      throw new Error(`Speed ramp source segments должны быть не короче ${MIN_SPEED_RAMP_SEGMENT_TICKS} ticks`)
    }
    const outputTicks = segmentIntegralTicks(sourceTicks, start.speed, point.speed, speedRamp.interpolation)
    cumulativeOutput += outputTicks
    if (!Number.isFinite(cumulativeOutput) || cumulativeOutput > MAX_SPEED_RAMP_CLIP_TICKS) {
      throw new Error('Speed ramp output duration выходит за пределы 2 часов')
    }
    const timelineEndTick = Math.round(cumulativeOutput)
    if (timelineEndTick - previousOutputTick < MIN_SPEED_RAMP_SEGMENT_TICKS) {
      throw new Error(`Speed ramp timeline segments должны быть не короче ${MIN_SPEED_RAMP_SEGMENT_TICKS} ticks`)
    }
    segments.push({
      sourceStartTick: start.sourceProgressTick,
      sourceEndTick: point.sourceProgressTick,
      timelineStartTick: previousOutputTick,
      timelineEndTick,
      startSpeed: start.speed,
      endSpeed: point.speed,
      interpolation: speedRamp.interpolation,
    })
    previousOutputTick = timelineEndTick
  }
  return segments
}

export function speedRampTimelineDurationTicks(
  sourceSpanTicks: number,
  baselineSpeed: number,
  speedRamp: CompositionSpeedRamp,
): number {
  return compositionSpeedRampSegments(sourceSpanTicks, baselineSpeed, speedRamp).at(-1)!.timelineEndTick
}

export function minimumCompositionSpeed(
  baselineSpeed: number,
  speedRamp?: CompositionSpeedRamp,
): number {
  return speedRamp ? Math.min(...speedRamp.points.map((point) => point.speed)) : baselineSpeed
}

/** Map clip-local output time into presentation-order source progress, including bounded trim extensions. */
export function speedRampSourceProgressAtTimelineTick(
  sourceSpanTicks: number,
  baselineSpeed: number,
  speedRamp: CompositionSpeedRamp,
  timelineTick: number,
): number {
  const segments = compositionSpeedRampSegments(sourceSpanTicks, baselineSpeed, speedRamp)
  const duration = segments.at(-1)!.timelineEndTick
  if (timelineTick <= 0) return Math.round(timelineTick * segments[0]!.startSpeed)
  if (timelineTick >= duration) {
    return sourceSpanTicks + Math.round((timelineTick - duration) * segments.at(-1)!.endSpeed)
  }
  const segment = segments.find((candidate) => timelineTick < candidate.timelineEndTick) ?? segments.at(-1)!
  const timelineSpan = segment.timelineEndTick - segment.timelineStartTick
  const fraction = (timelineTick - segment.timelineStartTick) / timelineSpan
  const sourceFraction = inverseTimelineFraction(
    fraction,
    segment.startSpeed,
    segment.endSpeed,
    segment.interpolation,
  )
  return Math.round(
    segment.sourceStartTick + (segment.sourceEndTick - segment.sourceStartTick) * sourceFraction,
  )
}

export function speedAtSourceProgress(
  sourceSpanTicks: number,
  baselineSpeed: number,
  speedRamp: CompositionSpeedRamp,
  sourceProgressTick: number,
): number {
  compositionSpeedRampSegments(sourceSpanTicks, baselineSpeed, speedRamp)
  if (sourceProgressTick <= 0) return speedRamp.points[0]!.speed
  if (sourceProgressTick >= sourceSpanTicks) return speedRamp.points.at(-1)!.speed
  const index = speedRamp.points.findIndex((point) => sourceProgressTick < point.sourceProgressTick)
  const end = speedRamp.points[index]!
  const start = speedRamp.points[index - 1]!
  if (speedRamp.interpolation === 'hold' || end.speed === start.speed) return start.speed
  const fraction = (sourceProgressTick - start.sourceProgressTick) /
    (end.sourceProgressTick - start.sourceProgressTick)
  return start.speed + (end.speed - start.speed) * fraction
}

/** Keep a presentation-order source interval and rebase/interpolate its boundary points. */
export function sliceCompositionSpeedRamp(
  sourceSpanTicks: number,
  baselineSpeed: number,
  speedRamp: CompositionSpeedRamp,
  startProgressTick: number,
  endProgressTick: number,
): SlicedCompositionSpeedRamp {
  compositionSpeedRampSegments(sourceSpanTicks, baselineSpeed, speedRamp)
  if (
    !Number.isSafeInteger(startProgressTick) ||
    !Number.isSafeInteger(endProgressTick) ||
    endProgressTick <= startProgressTick
  ) {
    throw new Error('Speed ramp slice должен иметь положительный source-progress range')
  }
  const nextSpan = endProgressTick - startProgressTick
  const firstSpeed = speedAtSourceProgress(sourceSpanTicks, baselineSpeed, speedRamp, startProgressTick)
  const lastSpeed = speedAtSourceProgress(sourceSpanTicks, baselineSpeed, speedRamp, endProgressTick)
  const points: CompositionSpeedRampPoint[] = [
    { sourceProgressTick: 0, speed: firstSpeed },
    ...speedRamp.points
      .filter((point) => point.sourceProgressTick > startProgressTick && point.sourceProgressTick < endProgressTick)
      .map((point) => ({ sourceProgressTick: point.sourceProgressTick - startProgressTick, speed: point.speed })),
    { sourceProgressTick: nextSpan, speed: lastSpeed },
  ]
  const simplified = simplifyBoundaryPoints(points, speedRamp.interpolation)
  const sliced: CompositionSpeedRamp = {
    interpolation: speedRamp.interpolation,
    points: simplified,
    audioPolicy: speedRamp.audioPolicy ?? 'preserve_pitch',
  }
  compositionSpeedRampSegments(nextSpan, firstSpeed, sliced)
  return { speed: firstSpeed, speedRamp: sliced }
}

export function cloneCompositionSpeedRamp(speedRamp: CompositionSpeedRamp | undefined): CompositionSpeedRamp | undefined {
  return speedRamp
    ? {
        interpolation: speedRamp.interpolation,
        points: speedRamp.points.map((point) => ({ ...point })),
        ...(speedRamp.audioPolicy === undefined ? {} : { audioPolicy: speedRamp.audioPolicy }),
      }
    : undefined
}

function segmentIntegralTicks(
  sourceTicks: number,
  startSpeed: number,
  endSpeed: number,
  interpolation: CompositionSpeedRampInterpolation,
): number {
  if (interpolation === 'hold' || speedsEqual(startSpeed, endSpeed)) return sourceTicks / startSpeed
  return sourceTicks * Math.log(endSpeed / startSpeed) / (endSpeed - startSpeed)
}

function inverseTimelineFraction(
  timelineFraction: number,
  startSpeed: number,
  endSpeed: number,
  interpolation: CompositionSpeedRampInterpolation,
): number {
  if (interpolation === 'hold' || speedsEqual(startSpeed, endSpeed)) return timelineFraction
  const speed = startSpeed * Math.exp(timelineFraction * Math.log(endSpeed / startSpeed))
  return (speed - startSpeed) / (endSpeed - startSpeed)
}

function simplifyBoundaryPoints(
  value: readonly CompositionSpeedRampPoint[],
  interpolation: CompositionSpeedRampInterpolation,
): CompositionSpeedRampPoint[] {
  const points = value.map((point) => ({ ...point }))
  let index = 1
  while (index < points.length - 1) {
    const previous = points[index - 1]!
    const current = points[index]!
    const next = points[index + 1]!
    const redundant = interpolation === 'hold'
      ? current.speed === previous.speed
      : Math.abs(current.speed - (
          previous.speed +
          (next.speed - previous.speed) *
          ((current.sourceProgressTick - previous.sourceProgressTick) /
            (next.sourceProgressTick - previous.sourceProgressTick))
        )) <= Number.EPSILON * Math.max(1, Math.abs(current.speed))
    if (redundant) points.splice(index, 1)
    else index += 1
  }
  if (points.length > MAX_SPEED_RAMP_POINTS) {
    throw new Error(`Speed ramp slice превышает лимит ${MAX_SPEED_RAMP_POINTS} points`)
  }
  return points
}

function speedsEqual(left: number, right: number): boolean {
  return Math.abs(right - left) <= Number.EPSILON * Math.max(left, right)
}

function assertSafeTick(value: number, label: string): void {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error(`Speed ramp ${label} должен быть safe tick`)
}

function assertSpeed(value: number, label: string): void {
  if (!Number.isFinite(value) || value < MIN_SPEED_RAMP_SPEED || value > MAX_SPEED_RAMP_SPEED) {
    throw new Error(`Speed ramp ${label} должен быть ${MIN_SPEED_RAMP_SPEED}..${MAX_SPEED_RAMP_SPEED}`)
  }
}
