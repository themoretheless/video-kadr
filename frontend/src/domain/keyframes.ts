// Keyframe sampling and track algebra for the motion feature.
//
// The semantics here mirror `backend/src/domain/keyframes.rs` exactly: what the
// user scrubs in the preview has to be what the render produces, so three
// details of the backend contract are reproduced literally.
//   1. The domain stores millisecond ticks, so every time is quantized before
//      it is compared or interpolated. Two keyframes that round to the same
//      tick are one keyframe (the backend rejects the duplicate outright).
//   2. Interpolation belongs to the track, not to the point: the wire
//      conversion reads `keyframes[0].interp` and ignores the rest.
//   3. Outside the first and last keyframe a track holds its endpoint value.
//
// The speed-ramp duration model mirrors `render/graph/motion.rs`, including its
// documented simplification that a `smooth` ramp is integrated as a linear one.

import type { Interpolation, Keyframe, KeyframeTrack } from '../types'

/** `domain::keyframes::OUTPUT_TIME_BASE`. */
export const KEYFRAME_TIME_BASE = 1_000
/** Contract section 0: any keyframe track is at most 64 points. */
export const MAX_KEYFRAMES = 64

/** `domain::motion` ranges. Zoom below 1 and above 8 is rejected on export. */
export const MIN_ZOOM = 1
export const MAX_ZOOM = 8
export const MAX_PAN = 1
export const MAX_ROTATION = 360
export const MIN_RAMP_SPEED = 0.25
export const MAX_RAMP_SPEED = 4

/** Values closer than this to each other are the same value. */
const EPSILON = 1e-6

export function clamp(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min
  return Math.max(min, Math.min(max, value))
}

/** Millisecond tick of a time in seconds, matching the wire conversion. */
export function toTick(seconds: number): number {
  if (!Number.isFinite(seconds)) return 0
  return Math.round(Math.max(0, seconds) * KEYFRAME_TIME_BASE)
}

/** A time snapped onto the tick grid, so no two points can collide later. */
export function quantizeTime(seconds: number): number {
  return toTick(seconds) / KEYFRAME_TIME_BASE
}

/** The whole track's interpolation. An empty track interpolates linearly. */
export function trackInterpolation(track: KeyframeTrack): Interpolation {
  return track.length ? track[0].interp : 'linear'
}

/** Eased progress inside a segment, identical to `sample_tick` in Rust. */
export function easeProgress(progress: number, interp: Interpolation): number {
  if (interp === 'hold') return 0
  if (interp === 'linear') return progress
  return progress < 0.5 ? 4 * progress ** 3 : 1 - (-2 * progress + 2) ** 3 / 2
}

/**
 * Value of a sorted track at `seconds`, or `fallback` when the parameter is
 * unused. Ticks, not seconds, drive the comparison so the preview steps at the
 * same instants the render does.
 */
export function sampleTrack(track: KeyframeTrack, seconds: number, fallback: number): number {
  if (!track.length) return fallback
  const tick = toTick(seconds)
  const first = track[0]
  if (tick <= toTick(first.t)) return first.v
  const last = track[track.length - 1]
  if (tick >= toTick(last.t)) return last.v

  const interp = trackInterpolation(track)
  for (let index = 1; index < track.length; index += 1) {
    const right = track[index]
    const rightTick = toTick(right.t)
    if (tick >= rightTick) continue
    const left = track[index - 1]
    const leftTick = toTick(left.t)
    const span = rightTick - leftTick
    if (span <= 0) return right.v
    return left.v + (right.v - left.v) * easeProgress((tick - leftTick) / span, interp)
  }
  return last.v
}

/** `steps + 1` samples across `[0, duration]`, for the curve preview. */
export function sampleTrackCurve(
  track: KeyframeTrack,
  duration: number,
  steps: number,
  fallback: number,
): number[] {
  const span = Number.isFinite(duration) && duration > 0 ? duration : 0
  const count = Math.max(1, Math.min(512, Math.round(steps)))
  const out: number[] = []
  for (let index = 0; index <= count; index += 1) {
    out.push(sampleTrack(track, (span * index) / count, fallback))
  }
  return out
}

// --- track algebra (every helper returns a new array) ---

/** Sorted by tick, one point per tick (last wins), capped at 64 points. */
export function normalizeTrack(track: KeyframeTrack): KeyframeTrack {
  const byTick = new Map<number, Keyframe>()
  for (const point of track) {
    if (!Number.isFinite(point.t) || !Number.isFinite(point.v)) continue
    const t = quantizeTime(point.t)
    byTick.set(toTick(t), { t, v: point.v, interp: point.interp })
  }
  return [...byTick.entries()]
    .sort((left, right) => left[0] - right[0])
    .slice(0, MAX_KEYFRAMES)
    .map(([, point]) => point)
}

/**
 * Add a point, or replace the one already sitting on that tick. The track's own
 * interpolation wins so the emitted payload cannot imply a per-point easing the
 * backend would silently drop.
 */
export function putKeyframe(track: KeyframeTrack, t: number, v: number): KeyframeTrack {
  const interp = trackInterpolation(track)
  const next = normalizeTrack([...track, { t: quantizeTime(t), v, interp }])
  return next.map((point) => ({ ...point, interp }))
}

export function removeKeyframe(track: KeyframeTrack, index: number): KeyframeTrack {
  if (index < 0 || index >= track.length) return track
  return track.filter((_, position) => position !== index)
}

/**
 * Move one point in time and value. The moved point keeps its identity: when it
 * lands on a neighbour's tick it is nudged by one tick instead of swallowing it,
 * which is what makes a drag across a neighbour feel continuous.
 */
export function moveKeyframe(
  track: KeyframeTrack,
  index: number,
  t: number,
  v: number,
): KeyframeTrack {
  const current = track[index]
  if (!current) return track
  const taken = new Set(track.map((point, position) => (position === index ? -1 : toTick(point.t))))
  let tick = toTick(t)
  while (taken.has(tick)) tick += 1
  const moved: Keyframe = { t: tick / KEYFRAME_TIME_BASE, v, interp: current.interp }
  const next = track.map((point, position) => (position === index ? moved : point))
  return normalizeTrack(next)
}

/** Index of `moved` after a re-sort, so the editor can keep it selected. */
export function indexOfTime(track: KeyframeTrack, t: number): number {
  const tick = toTick(t)
  return track.findIndex((point) => toTick(point.t) === tick)
}

export function setTrackInterpolation(
  track: KeyframeTrack,
  interp: Interpolation,
): KeyframeTrack {
  return track.map((point) => ({ ...point, interp }))
}

/** Value range to plot, always containing the track and the neutral value. */
export function trackBounds(
  track: KeyframeTrack,
  neutral: number,
  min: number,
  max: number,
): { min: number; max: number } {
  let low = neutral
  let high = neutral
  for (const point of track) {
    low = Math.min(low, point.v)
    high = Math.max(high, point.v)
  }
  const padding = Math.max((high - low) * 0.15, (max - min) * 0.05)
  return { min: Math.max(min, low - padding), max: Math.min(max, high + padding) }
}

// --- speed ramps ---

/**
 * Output duration of `sourceDuration` seconds of source under a ramp track,
 * using the same piecewise integral `render/graph/motion.rs` builds:
 * a constant region contributes `span / speed`, a linear one the exact
 * `log(s(b)/s(a)) / k`. A `smooth` ramp is integrated as linear because the
 * backend compiles it that way.
 */
export function rampOutputDuration(track: KeyframeTrack, sourceDuration: number): number {
  const duration = Number.isFinite(sourceDuration) && sourceDuration > 0 ? sourceDuration : 0
  const points = normalizeTrack(track).map((point) => ({
    t: point.t,
    v: clamp(point.v, MIN_RAMP_SPEED, MAX_RAMP_SPEED),
  }))
  if (!points.length || duration <= 0) return duration

  const holds = trackInterpolation(track) === 'hold'
  const first = points[0]
  let output = constantOutput(0, Math.min(first.t, duration), first.v)
  for (let index = 1; index < points.length; index += 1) {
    const left = points[index - 1]
    const right = points[index]
    const span = right.t - left.t
    if (span <= 0 || left.t >= duration) continue
    const end = Math.min(right.t, duration)
    if (holds || Math.abs(right.v - left.v) <= EPSILON) {
      output += constantOutput(left.t, end, left.v)
      continue
    }
    const slope = (right.v - left.v) / span
    const speedAtEnd = left.v + slope * (end - left.t)
    output += Math.log(speedAtEnd / left.v) / slope
  }
  const last = points[points.length - 1]
  return output + constantOutput(last.t, duration, last.v)
}

function constantOutput(start: number, end: number, speed: number): number {
  if (end <= start || speed <= 0) return 0
  return (end - start) / speed
}

// --- Ken Burns geometry ---

/** Sampled transform at one instant of the output timeline. */
export interface MotionSample {
  zoom: number
  panX: number
  panY: number
  rotation: number
}

export interface MotionTracks {
  zoom: KeyframeTrack
  panX: KeyframeTrack
  panY: KeyframeTrack
  rotation: KeyframeTrack
}

export function sampleMotion(tracks: MotionTracks, seconds: number): MotionSample {
  return {
    zoom: clamp(sampleTrack(tracks.zoom, seconds, 1), MIN_ZOOM, MAX_ZOOM),
    panX: clamp(sampleTrack(tracks.panX, seconds, 0), -MAX_PAN, MAX_PAN),
    panY: clamp(sampleTrack(tracks.panY, seconds, 0), -MAX_PAN, MAX_PAN),
    rotation: clamp(sampleTrack(tracks.rotation, seconds, 0), -MAX_ROTATION, MAX_ROTATION),
  }
}

/**
 * Visible window over the source frame, normalized to 0..1 on both axes.
 *
 * `zoompan` places its window at `x = (iw*z - ow)/2 * (1 + panX)`; the zoomed
 * image is exactly `zoom` window widths across, so the window covers `1/zoom`
 * of the frame and its left edge sits at `(1 - size)/2 * (1 + panX)`. The zoom
 * is uniform, so the same fraction is taken from the height.
 */
export interface FrameWindow {
  x: number
  y: number
  size: number
}

export function windowFromZoomPan(zoom: number, panX: number, panY: number): FrameWindow {
  const size = 1 / clamp(zoom, MIN_ZOOM, MAX_ZOOM)
  const headroom = (1 - size) / 2
  return {
    x: headroom * (1 + clamp(panX, -MAX_PAN, MAX_PAN)),
    y: headroom * (1 + clamp(panY, -MAX_PAN, MAX_PAN)),
    size,
  }
}

/** Inverse of `windowFromZoomPan`. At zoom 1 there is no headroom to pan in. */
export function zoomPanFromWindow(frame: FrameWindow): MotionSample {
  const size = clamp(frame.size, 1 / MAX_ZOOM, 1)
  const zoom = clamp(1 / size, MIN_ZOOM, MAX_ZOOM)
  const headroom = (1 - 1 / zoom) / 2
  const pan = (value: number): number =>
    headroom <= EPSILON ? 0 : clamp(value / headroom - 1, -MAX_PAN, MAX_PAN)
  return { zoom, panX: pan(frame.x), panY: pan(frame.y), rotation: 0 }
}

/**
 * CSS equivalent of the render's rotate + `zoompan` window, for an element that
 * already has the source aspect ratio. The list applies right to left, so the
 * frame is rotated about its centre, scaled about its centre, and finally
 * shifted by the pan headroom, exactly like the filter chain.
 */
export function previewTransform(sample: MotionSample): string {
  const shift = ((sample.zoom - 1) / 2) * 100
  const x = (-shift * sample.panX).toFixed(3)
  const y = (-shift * sample.panY).toFixed(3)
  return `translate(${x}%, ${y}%) scale(${sample.zoom.toFixed(4)}) rotate(${sample.rotation.toFixed(3)}deg)`
}
