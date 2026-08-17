// Insta360 / action-cam handling: 360 reframing, stabilization and lens
// correction. Foundation ships state, serialization and validated restore; the
// spatial agent adds the reframing UI on top.

import { reactive } from 'vue'
import { indexOfTime, MAX_KEYFRAMES, putKeyframe, quantizeTime, sampleTrack } from '../domain/keyframes'
import type {
  InputProjection,
  KeyframeTrack,
  LensCorrection,
  OutputProjection,
  Reframe360Spec,
  StabilizeMode,
  StabilizeSpec,
} from '../types'
import {
  boolOr,
  clampInt,
  clampNumber,
  cloneTrack,
  enumOr,
  isRecord,
  sanitizeKeyframeTrack,
} from './validation'

const INPUT_PROJECTIONS: readonly InputProjection[] = ['equirect', 'fisheye', 'dfisheye']
const OUTPUT_PROJECTIONS: readonly OutputProjection[] = [
  'flat',
  'equirect',
  'fisheye',
  'stereographic',
  'pannini',
]
const STABILIZE_MODES: readonly StabilizeMode[] = ['off', 'fast', 'precise']

const MIN_OUTPUT_SIZE = 16
const MAX_OUTPUT_SIZE = 7680

/** Pitch past the poles turns the frame upside down; a camera never goes there. */
export const MAX_PITCH = 90

/**
 * `enabled` is UI-only: the wire has no such flag, it simply omits `reframe360`
 * when reframing is off. Keeping the settings around lets the user toggle the
 * feature without losing the yaw/pitch they dialled in.
 *
 * `view` is UI-only too: it is the live camera the viewport drags, and it
 * becomes keyframes only when the user presses "поставить ключ", or a single
 * implicit keyframe at t=0 while no track is animated.
 */
export interface Reframe360State extends Reframe360Spec {
  enabled: boolean
  fov: KeyframeTrack
  yaw: KeyframeTrack
  pitch: KeyframeTrack
  roll: KeyframeTrack
  view: ReframeView
}

/** The live camera, in degrees. Not a wire shape. */
export interface ReframeView {
  yaw: number
  pitch: number
  roll: number
  fov: number
}

export interface SpatialState {
  reframe360: Reframe360State
  stabilize: StabilizeSpec
  lensCorrection: LensCorrection
}

/** The four animatable reframe axes, in the order the panel lists them. */
export const REFRAME_AXES = ['yaw', 'pitch', 'roll', 'fov'] as const
export type ReframeAxis = (typeof REFRAME_AXES)[number]

export function defaultReframeView(): ReframeView {
  return { yaw: 0, pitch: 0, roll: 0, fov: 90 }
}

export function defaultReframe360(): Reframe360State {
  return {
    enabled: false,
    inputProjection: 'equirect',
    outputProjection: 'flat',
    fov: [],
    yaw: [],
    pitch: [],
    roll: [],
    outputWidth: 1920,
    outputHeight: 1080,
    horizonLock: true,
    view: defaultReframeView(),
  }
}

export function defaultStabilize(): StabilizeSpec {
  return { mode: 'off', smoothing: 10, zoom: 0, horizonLock: false }
}

export function defaultLensCorrection(): LensCorrection {
  return { k1: 0, k2: 0 }
}

function defaults(): SpatialState {
  return {
    reframe360: defaultReframe360(),
    stabilize: defaultStabilize(),
    lensCorrection: defaultLensCorrection(),
  }
}

export const spatialState = reactive<SpatialState>(defaults())

export function resetSpatial(): void {
  spatialState.reframe360 = defaultReframe360()
  spatialState.stabilize = defaultStabilize()
  spatialState.lensCorrection = defaultLensCorrection()
}

export function sanitizeReframe360(value: unknown): Reframe360State {
  const source = isRecord(value) ? value : {}
  const base = defaultReframe360()
  return {
    enabled: boolOr(source.enabled, base.enabled),
    inputProjection: enumOr(source.inputProjection, INPUT_PROJECTIONS, base.inputProjection),
    outputProjection: enumOr(source.outputProjection, OUTPUT_PROJECTIONS, base.outputProjection),
    fov: sanitizeKeyframeTrack(source.fov, 1, 360),
    yaw: sanitizeKeyframeTrack(source.yaw, -360, 360),
    // The camera stops at the poles, so its keyframes do too.
    pitch: sanitizeKeyframeTrack(source.pitch, -MAX_PITCH, MAX_PITCH),
    roll: sanitizeKeyframeTrack(source.roll, -360, 360),
    // Even dimensions only: odd sizes break most encoders.
    outputWidth: evenSize(source.outputWidth, base.outputWidth),
    outputHeight: evenSize(source.outputHeight, base.outputHeight),
    horizonLock: boolOr(source.horizonLock, base.horizonLock),
    view: sanitizeReframeView(source.view),
  }
}

export function sanitizeReframeView(value: unknown): ReframeView {
  const source = isRecord(value) ? value : {}
  const base = defaultReframeView()
  return {
    yaw: wrapDegrees(clampNumber(source.yaw, -360, 360, base.yaw)),
    pitch: clampNumber(source.pitch, -MAX_PITCH, MAX_PITCH, base.pitch),
    roll: wrapDegrees(clampNumber(source.roll, -360, 360, base.roll)),
    fov: clampNumber(source.fov, 1, 360, base.fov),
  }
}

/** Fold an angle into -180..180, exactly like the `v360` stage does. */
export function wrapDegrees(value: number): number {
  if (!Number.isFinite(value)) return 0
  return ((((value + 180) % 360) + 360) % 360) - 180
}

function evenSize(value: unknown, fallback: number): number {
  const size = clampInt(value, MIN_OUTPUT_SIZE, MAX_OUTPUT_SIZE, fallback)
  return size - (size % 2)
}

export function sanitizeStabilize(value: unknown): StabilizeSpec {
  const source = isRecord(value) ? value : {}
  return {
    mode: enumOr(source.mode, STABILIZE_MODES, 'off'),
    smoothing: clampInt(source.smoothing, 1, 100, 10),
    zoom: clampNumber(source.zoom, 0, 20, 0),
    horizonLock: boolOr(source.horizonLock, false),
  }
}

export function sanitizeLensCorrection(value: unknown): LensCorrection {
  const source = isRecord(value) ? value : {}
  return {
    k1: clampNumber(source.k1, -1, 1, 0),
    k2: clampNumber(source.k2, -1, 1, 0),
  }
}

/**
 * Field of view only means something for the projections that sample a window
 * out of the sphere. A full equirectangular output ignores it, and the render
 * stage omits it from `v360` entirely, so the editor does too.
 */
export function defaultFov(projection: OutputProjection): number | null {
  if (projection === 'equirect') return null
  return projection === 'fisheye' ? 180 : 90
}

/** The value an axis holds when neither a keyframe nor the camera moved it. */
function axisDefault(state: Reframe360State, axis: ReframeAxis): number {
  return axis === 'fov' ? (defaultFov(state.outputProjection) ?? 0) : 0
}

/**
 * A track the render stage will actually read. An empty track falls back to the
 * live camera: `v360` takes the first keyframe as its static option, so a single
 * point at t=0 renders exactly the framing the viewport shows.
 */
function serializeAxis(state: Reframe360State, axis: ReframeAxis): KeyframeTrack | null {
  if (axis === 'fov' && defaultFov(state.outputProjection) === null) return null
  // Horizon lock cancels roll in the render stage; sending it would only make
  // the payload lie about what gets rendered.
  if (axis === 'roll' && state.horizonLock) return null
  const track = state[axis]
  if (track.length) return track
  const value = state.view[axis]
  if (Math.abs(value - axisDefault(state, axis)) < 1e-9) return null
  return [{ t: 0, v: value, interp: 'linear' }]
}

function serializeReframe360(state: Reframe360State): Reframe360Spec {
  const out: Reframe360Spec = {
    inputProjection: state.inputProjection,
    outputProjection: state.outputProjection,
    outputWidth: state.outputWidth,
    outputHeight: state.outputHeight,
    horizonLock: state.horizonLock,
  }
  for (const axis of REFRAME_AXES) {
    const track = serializeAxis(state, axis)
    if (track) out[axis] = track
  }
  return out
}

export function spatialPayload(): Record<string, unknown> {
  const payload: Record<string, unknown> = {}
  const reframe = sanitizeReframe360(spatialState.reframe360)
  if (reframe.enabled) payload.reframe360 = serializeReframe360(reframe)
  const stabilize = sanitizeStabilize(spatialState.stabilize)
  if (stabilize.mode !== 'off') payload.stabilize = { ...stabilize }
  const lens = sanitizeLensCorrection(spatialState.lensCorrection)
  if (lens.k1 !== 0 || lens.k2 !== 0) payload.lensCorrection = { ...lens }
  return payload
}

export function applySpatialSnapshot(raw: unknown): void {
  const source = isRecord(raw) ? raw : {}
  spatialState.reframe360 = sanitizeReframe360(source.reframe360)
  spatialState.stabilize = sanitizeStabilize(source.stabilize)
  spatialState.lensCorrection = sanitizeLensCorrection(source.lensCorrection)
}

export function cloneSpatialState(state: SpatialState): SpatialState {
  return {
    reframe360: {
      ...state.reframe360,
      fov: cloneTrack(state.reframe360.fov),
      yaw: cloneTrack(state.reframe360.yaw),
      pitch: cloneTrack(state.reframe360.pitch),
      roll: cloneTrack(state.reframe360.roll),
      view: { ...state.reframe360.view },
    },
    stabilize: { ...state.stabilize },
    lensCorrection: { ...state.lensCorrection },
  }
}

// --- reframe camera: drag-to-look, keyframes and sampling ---
// The viewport reads the live camera and writes through `setView`; the keyframe
// tracks are sampled with the shared `domain/keyframes` helpers, which mirror
// `domain::keyframes::KeyframeTrack::sample_tick` in Rust (millisecond ticks,
// one interpolation per track taken from its first point). What the viewport
// shows is therefore what the render stage computes.

/** Every axis sampled at `seconds`, with the live camera as the fallback. */
function sampleAxes(seconds: number): ReframeView {
  const reframe = spatialState.reframe360
  const camera = reframe.view
  return {
    yaw: sampleTrack(reframe.yaw, seconds, camera.yaw),
    pitch: sampleTrack(reframe.pitch, seconds, camera.pitch),
    roll: sampleTrack(reframe.roll, seconds, camera.roll),
    fov: sampleTrack(reframe.fov, seconds, camera.fov),
  }
}

/** The framing the render stage produces at `seconds`. */
export function viewAt(seconds: number): ReframeView {
  const view = sampleAxes(seconds)
  // Horizon lock cancels roll entirely in the render stage.
  return spatialState.reframe360.horizonLock ? { ...view, roll: 0 } : view
}

/** True while at least one axis is animated (two or more points). */
export function hasReframeAnimation(): boolean {
  return REFRAME_AXES.some((axis) => spatialState.reframe360[axis].length > 1)
}

export function hasReframeKeyframes(): boolean {
  return REFRAME_AXES.some((axis) => spatialState.reframe360[axis].length > 0)
}

/** Every time that carries a keyframe on any axis, sorted and de-duplicated. */
export function reframeKeyframeTimes(): number[] {
  const times = new Set<number>()
  for (const axis of REFRAME_AXES) {
    for (const point of spatialState.reframe360[axis]) times.add(point.t)
  }
  return [...times].sort((a, b) => a - b)
}

/** Clamp the camera into the range each axis and projection allows. */
export function setView(view: Partial<ReframeView>): void {
  const reframe = spatialState.reframe360
  const [minFov, maxFov] = fovRange(reframe.outputProjection)
  const next = { ...reframe.view, ...view }
  reframe.view = {
    yaw: wrapDegrees(clampNumber(next.yaw, -360, 360, reframe.view.yaw)),
    pitch: clampNumber(next.pitch, -MAX_PITCH, MAX_PITCH, reframe.view.pitch),
    roll: wrapDegrees(clampNumber(next.roll, -360, 360, reframe.view.roll)),
    fov: clampNumber(next.fov, minFov, maxFov, reframe.view.fov),
  }
}

/**
 * Follow the animated tracks as the playhead moves. Without this the camera
 * would keep the last dragged framing while the render animated away from it.
 */
export function syncViewToPlayhead(seconds: number): void {
  if (!hasReframeKeyframes()) return
  setView(sampleAxes(seconds))
}

/** True when the camera no longer matches the keyframes at `seconds`. */
export function isViewOffKeyframes(seconds: number): boolean {
  if (!hasReframeKeyframes()) return false
  const sampled = sampleAxes(seconds)
  const camera = spatialState.reframe360.view
  return REFRAME_AXES.filter(
    (axis) => axis !== 'roll' || !spatialState.reframe360.horizonLock,
  ).some((axis) => Math.abs(sampled[axis] - camera[axis]) > 0.01)
}

/** Usable field of view per projection: past these the output is unreadable. */
export function fovRange(projection: OutputProjection): [number, number] {
  return projection === 'fisheye' ? [30, 360] : [10, 170]
}

/**
 * Write the live camera into every axis at `seconds`. All four axes move
 * together, like an Insta360 view keyframe: an axis keyframed alone would jump
 * while the others stayed static.
 */
export function setReframeKeyframe(seconds: number): boolean {
  const reframe = spatialState.reframe360
  const t = quantizeTime(seconds)
  const view = reframe.view
  const axes = REFRAME_AXES.filter((axis) => axis !== 'roll' || !reframe.horizonLock)
  // Refuse the whole set rather than keyframing some axes and not others.
  if (
    axes.some(
      (axis) => reframe[axis].length >= MAX_KEYFRAMES && indexOfTime(reframe[axis], t) < 0,
    )
  ) {
    return false
  }
  for (const axis of axes) {
    reframe[axis] = putKeyframe(reframe[axis], t, view[axis])
  }
  return true
}

export function removeReframeKeyframe(seconds: number): void {
  const reframe = spatialState.reframe360
  const t = quantizeTime(seconds)
  for (const axis of REFRAME_AXES) {
    reframe[axis] = reframe[axis].filter((point) => point.t !== t)
  }
}

/** Range an axis is allowed to take, for the per-axis keyframe editor. */
export function axisRange(axis: ReframeAxis): { min: number; max: number; neutral: number } {
  const reframe = spatialState.reframe360
  if (axis === 'fov') {
    const [min, max] = fovRange(reframe.outputProjection)
    return { min, max, neutral: defaultFov(reframe.outputProjection) ?? 90 }
  }
  if (axis === 'pitch') return { min: -MAX_PITCH, max: MAX_PITCH, neutral: 0 }
  return { min: -180, max: 180, neutral: 0 }
}

/** Accept an edited track from the keyframe editor, clamped into range. */
export function setReframeTrack(axis: ReframeAxis, track: unknown): void {
  const { min, max } = axisRange(axis)
  spatialState.reframe360[axis] = sanitizeKeyframeTrack(track, min, max)
}

/** Drop every keyframe and keep the camera where it currently looks. */
export function clearReframeKeyframes(seconds: number): void {
  const view = sampleAxes(seconds)
  const reframe = spatialState.reframe360
  for (const axis of REFRAME_AXES) reframe[axis] = []
  setView(view)
}

export function resetLensCorrection(): void {
  spatialState.lensCorrection = defaultLensCorrection()
}

/** True when the module would contribute anything to the wire. */
export function spatialActive(): boolean {
  return Object.keys(spatialPayload()).length > 0
}
