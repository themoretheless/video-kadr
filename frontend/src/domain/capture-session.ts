export type CaptureCompositionMode = 'screen_with_camera' | 'screen_only' | 'camera_only'
export type CameraCorner = 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right'
export type CaptureInsertAt = 'playhead' | 'timeline_end'
export type CaptureFps = 15 | 24 | 30

export interface CaptureRequest {
  screen: boolean
  camera: boolean
  microphone: boolean
  systemAudio: boolean
  composition: { mode: CaptureCompositionMode; cameraCorner: CameraCorner; cameraScale: number }
  fps: CaptureFps
  insertAt: CaptureInsertAt
}

export interface ValidatedCaptureOptions extends CaptureRequest {
  readonly contract: 'capture-session-v1'
  readonly maxDurationMs: number
  readonly maxBytes: number
  readonly chunkTimesliceMs: number
}

export const CAPTURE_LIMITS = Object.freeze({
  maxDurationMs: 30 * 60 * 1000,
  maxBytes: 1024 * 1024 * 1024,
  chunkTimesliceMs: 1000,
  minCameraScale: 0.15,
  maxCameraScale: 0.4,
})

export const CAPTURE_MIME_CANDIDATES = Object.freeze([
  'video/webm;codecs=vp9,opus',
  'video/webm;codecs=vp8,opus',
  'video/webm;codecs=vp9',
  'video/webm;codecs=vp8',
  'video/webm',
] as const)

export type CaptureErrorCode =
  | 'invalid_request' | 'unsupported' | 'permission_denied' | 'permission_cancelled'
  | 'device_ended' | 'recorder_failed' | 'storage_failed' | 'limit_reached'
  | 'invalid_transition' | 'empty_recording'

export class CaptureSessionError extends Error {
  constructor(public readonly code: CaptureErrorCode, message: string, options?: ErrorOptions) {
    super(message, options)
    this.name = 'CaptureSessionError'
  }
}

const FPS = new Set<number>([15, 24, 30])
const CORNERS = new Set<CameraCorner>(['top_left', 'top_right', 'bottom_left', 'bottom_right'])

export function validateCaptureRequest(input: CaptureRequest): ValidatedCaptureOptions {
  if (!input || typeof input !== 'object') throw new CaptureSessionError('invalid_request', 'Capture request is required')
  if (!input.screen && !input.camera) throw new CaptureSessionError('invalid_request', 'Select screen or camera')
  if (input.systemAudio && !input.screen) throw new CaptureSessionError('invalid_request', 'System audio requires screen capture')
  if (input.composition?.mode === 'screen_with_camera' && (!input.screen || !input.camera)) {
    throw new CaptureSessionError('invalid_request', 'Screen with camera requires both sources')
  }
  if (input.composition?.mode === 'screen_only' && (!input.screen || input.camera)) {
    throw new CaptureSessionError('invalid_request', 'Screen-only composition requires only screen video')
  }
  if (input.composition?.mode === 'camera_only' && (!input.camera || input.screen)) {
    throw new CaptureSessionError('invalid_request', 'Camera-only composition requires only camera video')
  }
  if (!CORNERS.has(input.composition?.cameraCorner)) throw new CaptureSessionError('invalid_request', 'Invalid camera corner')
  if (!Number.isFinite(input.composition?.cameraScale)
    || input.composition.cameraScale < CAPTURE_LIMITS.minCameraScale
    || input.composition.cameraScale > CAPTURE_LIMITS.maxCameraScale) {
    throw new CaptureSessionError('invalid_request', 'Invalid camera scale')
  }
  if (!FPS.has(input.fps)) throw new CaptureSessionError('invalid_request', 'Unsupported capture FPS')
  if (!['playhead', 'timeline_end'].includes(input.insertAt)) throw new CaptureSessionError('invalid_request', 'Invalid insertion target')
  return Object.freeze({
    ...structuredClone(input), contract: 'capture-session-v1' as const,
    maxDurationMs: CAPTURE_LIMITS.maxDurationMs, maxBytes: CAPTURE_LIMITS.maxBytes,
    chunkTimesliceMs: CAPTURE_LIMITS.chunkTimesliceMs,
  })
}

export type CapturePhase = 'idle' | 'requesting_display' | 'requesting_user_media' | 'previewing'
  | 'recording' | 'paused' | 'stopping' | 'finalizing' | 'saved' | 'failed' | 'cancelled'

export interface CaptureSessionState {
  phase: CapturePhase
  generation: number
  error?: CaptureSessionError
}

export type CaptureSessionEvent =
  | { type: 'start'; generation: number; needsDisplay: boolean }
  | { type: 'display_acquired'; generation: number; needsUserMedia: boolean }
  | { type: 'user_media_acquired'; generation: number }
  | { type: 'preview_ready'; generation: number }
  | { type: 'record'; generation: number }
  | { type: 'pause'; generation: number }
  | { type: 'resume'; generation: number }
  | { type: 'stop'; generation: number }
  | { type: 'recorder_stopped'; generation: number }
  | { type: 'saved'; generation: number }
  | { type: 'fail'; generation: number; error: CaptureSessionError }
  | { type: 'cancel'; generation: number }
  | { type: 'reset'; generation: number }

export const initialCaptureSessionState = (): CaptureSessionState => ({ phase: 'idle', generation: 0 })

export function transitionCaptureSession(state: CaptureSessionState, event: CaptureSessionEvent): CaptureSessionState {
  if (!Number.isSafeInteger(event.generation) || event.generation < 0) throw new CaptureSessionError('invalid_transition', 'Invalid capture generation')
  if (event.type === 'start') {
    if (!['idle', 'saved', 'failed', 'cancelled'].includes(state.phase) || event.generation <= state.generation) {
      throw new CaptureSessionError('invalid_transition', `Cannot start from ${state.phase}`)
    }
    return { phase: event.needsDisplay ? 'requesting_display' : 'requesting_user_media', generation: event.generation }
  }
  // Late callbacks from an older session are intentionally inert.
  if (event.generation !== state.generation) return state
  if (event.type === 'fail') {
    if (['idle', 'saved', 'failed', 'cancelled'].includes(state.phase)) throw new CaptureSessionError('invalid_transition', `Cannot fail from ${state.phase}`)
    return { phase: 'failed', generation: state.generation, error: event.error }
  }
  if (event.type === 'cancel') {
    if (!['requesting_display', 'requesting_user_media', 'previewing', 'recording', 'paused', 'stopping'].includes(state.phase)) throw new CaptureSessionError('invalid_transition', `Cannot cancel from ${state.phase}`)
    return { phase: 'cancelled', generation: state.generation }
  }
  const next = (() => {
    if (state.phase === 'requesting_display' && event.type === 'display_acquired') return event.needsUserMedia ? 'requesting_user_media' : 'previewing'
    if (state.phase === 'requesting_user_media' && event.type === 'user_media_acquired') return 'previewing'
    if (state.phase === 'previewing' && event.type === 'preview_ready') return 'previewing'
    if (state.phase === 'previewing' && event.type === 'record') return 'recording'
    if (state.phase === 'recording' && event.type === 'pause') return 'paused'
    if (state.phase === 'paused' && event.type === 'resume') return 'recording'
    if (state.phase === 'recording' && event.type === 'stop') return 'stopping'
    if (state.phase === 'paused' && event.type === 'stop') return 'stopping'
    if (state.phase === 'stopping' && event.type === 'recorder_stopped') return 'finalizing'
    if (state.phase === 'finalizing' && event.type === 'saved') return 'saved'
    if (['saved', 'failed', 'cancelled'].includes(state.phase) && event.type === 'reset') return 'idle'
    return null
  })()
  if (!next) throw new CaptureSessionError('invalid_transition', `${event.type} is invalid from ${state.phase}`)
  return { phase: next, generation: state.generation }
}

export interface Size { width: number; height: number }
export interface Rect extends Size { x: number; y: number }

function validSize(size: Size): void {
  if (!Number.isFinite(size.width) || !Number.isFinite(size.height) || size.width <= 0 || size.height <= 0) {
    throw new CaptureSessionError('invalid_request', 'Invalid capture geometry')
  }
}

export function containRect(source: Size, destination: Size): Rect {
  validSize(source); validSize(destination)
  const widthLimited = destination.width / source.width <= destination.height / source.height
  const width = widthLimited ? destination.width : source.width * (destination.height / source.height)
  const height = widthLimited ? source.height * (destination.width / source.width) : destination.height
  return { x: (destination.width - width) / 2, y: (destination.height - height) / 2, width, height }
}

export function cameraPipRect(canvas: Size, camera: Size, corner: CameraCorner, scale: number, marginRatio = 0.025): Rect {
  validSize(canvas); validSize(camera)
  if (!CORNERS.has(corner) || !Number.isFinite(scale) || scale < CAPTURE_LIMITS.minCameraScale || scale > CAPTURE_LIMITS.maxCameraScale
    || !Number.isFinite(marginRatio) || marginRatio < 0 || marginRatio > 0.1) {
    throw new CaptureSessionError('invalid_request', 'Invalid PiP geometry')
  }
  const max = { width: canvas.width * scale, height: canvas.height * scale }
  const fitted = containRect(camera, max)
  const margin = Math.min(canvas.width, canvas.height) * marginRatio
  const left = corner.endsWith('left')
  const top = corner.startsWith('top')
  return {
    x: left ? margin : canvas.width - margin - fitted.width,
    y: top ? margin : canvas.height - margin - fitted.height,
    width: fitted.width, height: fitted.height,
  }
}
