// Keyframed transform (Ken Burns, animated reframe) and speed ramps.
// Foundation ships state, serialization and validated restore; the motion agent
// adds the keyframe editing UI on top.

import { reactive } from 'vue'
import {
  MAX_PAN,
  MAX_RAMP_SPEED,
  MAX_ROTATION,
  MAX_ZOOM,
  MIN_RAMP_SPEED,
  MIN_ZOOM,
  normalizeTrack,
  setTrackInterpolation,
} from '../domain/keyframes'
import type { Interpolation, KeyframeTrack, MotionSpec } from '../types'
import { cloneTrack, isRecord, sanitizeKeyframeTrack } from './validation'

/**
 * Ranges from `backend/src/domain/motion.rs`. They are narrower than the ones
 * the contract sketches for zoom: the backend rejects the whole request when a
 * zoom keyframe leaves 1..8, so the UI must never offer a value outside it.
 */
const ZOOM_MIN = MIN_ZOOM
const ZOOM_MAX = MAX_ZOOM
const PAN_LIMIT = MAX_PAN
const ROTATION_LIMIT = MAX_ROTATION
const SPEED_MIN = MIN_RAMP_SPEED
const SPEED_MAX = MAX_RAMP_SPEED

export interface MotionState {
  zoom: KeyframeTrack
  panX: KeyframeTrack
  panY: KeyframeTrack
  rotation: KeyframeTrack
  speedRamps: KeyframeTrack
}

/** The four tracks that make up the animated transform. */
export type MotionTrackKey = keyof MotionState

function defaults(): MotionState {
  return { zoom: [], panX: [], panY: [], rotation: [], speedRamps: [] }
}

export const motionState = reactive<MotionState>(defaults())

export function resetMotion(): void {
  motionState.zoom = []
  motionState.panX = []
  motionState.panY = []
  motionState.rotation = []
  motionState.speedRamps = []
}

/**
 * The shared sanitizer clamps and sorts; one thing is still left to do before a
 * track is safe to send. Times are snapped onto the millisecond grid the
 * backend stores, because two points that round to the same tick make the wire
 * conversion reject the whole request with `InvalidMotion`.
 *
 * The trailing `interp` values are deliberately left alone here: the backend
 * reads the first point's interpolation for the whole track and ignores the
 * rest, so a mixed track from an old snapshot renders predictably. The editor
 * writes one interpolation across a track so the UI never shows a mix.
 */
function sanitizeTrack(value: unknown, min: number, max: number): KeyframeTrack {
  return normalizeTrack(sanitizeKeyframeTrack(value, min, max))
}

function sanitizeMotionState(value: unknown): MotionState {
  const source = isRecord(value) ? value : {}
  return {
    zoom: sanitizeTrack(source.zoom, ZOOM_MIN, ZOOM_MAX),
    panX: sanitizeTrack(source.panX, -PAN_LIMIT, PAN_LIMIT),
    panY: sanitizeTrack(source.panY, -PAN_LIMIT, PAN_LIMIT),
    rotation: sanitizeTrack(source.rotation, -ROTATION_LIMIT, ROTATION_LIMIT),
    speedRamps: sanitizeTrack(source.speedRamps, SPEED_MIN, SPEED_MAX),
  }
}

/** Value range of a track, for the editors and for a programmatic write. */
export const MOTION_RANGES: Record<MotionTrackKey, { min: number; max: number; neutral: number }> =
  {
    zoom: { min: ZOOM_MIN, max: ZOOM_MAX, neutral: 1 },
    panX: { min: -PAN_LIMIT, max: PAN_LIMIT, neutral: 0 },
    panY: { min: -PAN_LIMIT, max: PAN_LIMIT, neutral: 0 },
    rotation: { min: -ROTATION_LIMIT, max: ROTATION_LIMIT, neutral: 0 },
    speedRamps: { min: SPEED_MIN, max: SPEED_MAX, neutral: 1 },
  }

/** Single write path for the panel: sanitize once, assign once. */
export function setMotionTrack(key: MotionTrackKey, track: KeyframeTrack): void {
  const range = MOTION_RANGES[key]
  motionState[key] = sanitizeTrack(track, range.min, range.max)
}

export function setMotionInterpolation(key: MotionTrackKey, interp: Interpolation): void {
  setMotionTrack(key, setTrackInterpolation(motionState[key], interp))
}

/** True when at least one track would reach the wire. */
export function motionActive(): boolean {
  return Object.keys(motionPayload()).length > 0
}

export function motionPayload(): Record<string, unknown> {
  const payload: Record<string, unknown> = {}
  const sanitized = sanitizeMotionState(motionState)
  const motion: MotionSpec = {}
  if (sanitized.zoom.length) motion.zoom = sanitized.zoom
  if (sanitized.panX.length) motion.panX = sanitized.panX
  if (sanitized.panY.length) motion.panY = sanitized.panY
  if (sanitized.rotation.length) motion.rotation = sanitized.rotation
  if (Object.keys(motion).length) payload.motion = motion
  if (sanitized.speedRamps.length) payload.speedRamps = sanitized.speedRamps
  return payload
}

/**
 * Restores both the nested `motion` object and the sibling `speedRamps` track,
 * accepting either the flat state shape or the wire shape.
 */
export function applyMotionSnapshot(raw: unknown): void {
  const source = isRecord(raw) ? raw : {}
  const nested = isRecord(source.motion) ? source.motion : source
  const sanitized = sanitizeMotionState({
    zoom: nested.zoom,
    panX: nested.panX,
    panY: nested.panY,
    rotation: nested.rotation,
    speedRamps: source.speedRamps,
  })
  motionState.zoom = sanitized.zoom
  motionState.panX = sanitized.panX
  motionState.panY = sanitized.panY
  motionState.rotation = sanitized.rotation
  motionState.speedRamps = sanitized.speedRamps
}

export function cloneMotionState(state: MotionState): MotionState {
  return {
    zoom: cloneTrack(state.zoom),
    panX: cloneTrack(state.panX),
    panY: cloneTrack(state.panY),
    rotation: cloneTrack(state.rotation),
    speedRamps: cloneTrack(state.speedRamps),
  }
}
