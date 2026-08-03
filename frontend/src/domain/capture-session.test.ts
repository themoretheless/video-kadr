import { describe, expect, it } from 'vitest'
import {
  cameraPipRect, CAPTURE_LIMITS, CAPTURE_MIME_CANDIDATES, CaptureSessionError,
  containRect, initialCaptureSessionState, transitionCaptureSession, validateCaptureRequest,
  type CaptureRequest,
} from './capture-session'

const request = (patch: Partial<CaptureRequest> = {}): CaptureRequest => ({
  screen: true, camera: true, microphone: true, systemAudio: true,
  composition: { mode: 'screen_with_camera', cameraCorner: 'bottom_right', cameraScale: .25 },
  fps: 30, insertAt: 'playhead', ...patch,
})

describe('capture request contract', () => {
  it('normalizes an immutable bounded v1 contract', () => {
    const value = validateCaptureRequest(request())
    expect(value).toMatchObject({ contract: 'capture-session-v1', maxDurationMs: 1_800_000, maxBytes: 1_073_741_824, chunkTimesliceMs: 1000 })
    expect(Object.isFrozen(value)).toBe(true)
    expect(CAPTURE_MIME_CANDIDATES[0]).toBe('video/webm;codecs=vp9,opus')
  })

  it.each([
    [{ screen: false, camera: false }, 'Select screen'],
    [{ screen: false, camera: true, systemAudio: true, composition: { mode: 'camera_only', cameraCorner: 'top_left', cameraScale: .2 } }, 'System audio'],
    [{ camera: false }, 'requires both'],
    [{ camera: false, composition: { mode: 'camera_only', cameraCorner: 'top_left', cameraScale: .2 } }, 'only camera'],
    [{ camera: true, composition: { mode: 'screen_only', cameraCorner: 'top_left', cameraScale: .2 } }, 'only screen'],
    [{ screen: true, camera: true, composition: { mode: 'camera_only', cameraCorner: 'top_left', cameraScale: .2 } }, 'only camera'],
    [{ composition: { mode: 'screen_with_camera', cameraCorner: 'top_left', cameraScale: .14 } }, 'camera scale'],
    [{ fps: 60 }, 'FPS'],
    [{ insertAt: 'middle' }, 'insertion'],
  ])('rejects invalid request %#', (patch, message) => {
    expect(() => validateCaptureRequest(request(patch as Partial<CaptureRequest>))).toThrow(message)
  })

  it('uses typed errors', () => {
    expect(() => validateCaptureRequest(request({ screen: false, camera: false }))).toThrow(expect.objectContaining({ name: 'CaptureSessionError', code: 'invalid_request' }))
  })

  it('represents strict screen-only capture', () => {
    expect(validateCaptureRequest(request({ camera: false, microphone: false, composition: { mode: 'screen_only', cameraCorner: 'top_right', cameraScale: .2 } }))).toMatchObject({ screen: true, camera: false, composition: { mode: 'screen_only' } })
  })
})

describe('capture state machine', () => {
  it('runs the screen+devices happy path', () => {
    let state = transitionCaptureSession(initialCaptureSessionState(), { type: 'start', generation: 1, needsDisplay: true })
    state = transitionCaptureSession(state, { type: 'display_acquired', generation: 1, needsUserMedia: true })
    state = transitionCaptureSession(state, { type: 'user_media_acquired', generation: 1 })
    state = transitionCaptureSession(state, { type: 'record', generation: 1 })
    state = transitionCaptureSession(state, { type: 'stop', generation: 1 })
    state = transitionCaptureSession(state, { type: 'recorder_stopped', generation: 1 })
    state = transitionCaptureSession(state, { type: 'saved', generation: 1 })
    expect(state).toEqual({ phase: 'saved', generation: 1 })
    expect(transitionCaptureSession(state, { type: 'reset', generation: 1 })).toEqual({ phase: 'idle', generation: 1 })
  })

  it('supports camera-only and cancellation', () => {
    let state = transitionCaptureSession(initialCaptureSessionState(), { type: 'start', generation: 2, needsDisplay: false })
    state = transitionCaptureSession(state, { type: 'user_media_acquired', generation: 2 })
    expect(transitionCaptureSession(state, { type: 'cancel', generation: 2 }).phase).toBe('cancelled')
  })

  it('pauses, resumes and can stop or cancel while paused', () => {
    let state = transitionCaptureSession(initialCaptureSessionState(), { type: 'start', generation: 4, needsDisplay: false })
    state = transitionCaptureSession(state, { type: 'user_media_acquired', generation: 4 })
    state = transitionCaptureSession(state, { type: 'record', generation: 4 })
    state = transitionCaptureSession(state, { type: 'pause', generation: 4 })
    expect(state.phase).toBe('paused')
    expect(transitionCaptureSession(state, { type: 'cancel', generation: 4 }).phase).toBe('cancelled')
    state = transitionCaptureSession(state, { type: 'resume', generation: 4 })
    expect(state.phase).toBe('recording')
    state = transitionCaptureSession(state, { type: 'pause', generation: 4 })
    expect(transitionCaptureSession(state, { type: 'stop', generation: 4 }).phase).toBe('stopping')
  })

  it('fences stale callbacks and rejects illegal current-generation transitions', () => {
    const state = transitionCaptureSession(initialCaptureSessionState(), { type: 'start', generation: 3, needsDisplay: true })
    expect(transitionCaptureSession(state, { type: 'display_acquired', generation: 2, needsUserMedia: false })).toBe(state)
    expect(() => transitionCaptureSession(state, { type: 'record', generation: 3 })).toThrow(expect.objectContaining({ code: 'invalid_transition' }))
    expect(() => transitionCaptureSession(state, { type: 'start', generation: 3, needsDisplay: true })).toThrow(CaptureSessionError)
  })

  it('stores typed terminal failure and cannot overwrite it', () => {
    const requesting = transitionCaptureSession(initialCaptureSessionState(), { type: 'start', generation: 1, needsDisplay: true })
    const error = new CaptureSessionError('permission_denied', 'Display denied')
    const failed = transitionCaptureSession(requesting, { type: 'fail', generation: 1, error })
    expect(failed).toEqual({ phase: 'failed', generation: 1, error })
    expect(() => transitionCaptureSession(failed, { type: 'fail', generation: 1, error })).toThrow('Cannot fail')
  })
})

describe('capture composition geometry', () => {
  it('contains landscape and portrait sources without stretching', () => {
    expect(containRect({ width: 1920, height: 1080 }, { width: 1000, height: 1000 })).toEqual({ x: 0, y: 218.75, width: 1000, height: 562.5 })
    expect(containRect({ width: 1080, height: 1920 }, { width: 1920, height: 1080 })).toEqual({ x: 656.25, y: 0, width: 607.5, height: 1080 })
  })

  it.each([
    ['top_left', 27, 27], ['top_right', 1413, 27], ['bottom_left', 27, 783], ['bottom_right', 1413, 783],
  ] as const)('places 16:9 PiP in %s with stable safe margin', (corner, x, y) => {
    expect(cameraPipRect({ width: 1920, height: 1080 }, { width: 1280, height: 720 }, corner, .25)).toEqual({ x, y, width: 480, height: 270 })
  })

  it('rejects non-finite, empty and out-of-policy geometry', () => {
    expect(() => containRect({ width: 0, height: 1 }, { width: 1, height: 1 })).toThrow('geometry')
    expect(() => cameraPipRect({ width: 100, height: 100 }, { width: 1, height: 1 }, 'top_left', CAPTURE_LIMITS.maxCameraScale + .01)).toThrow('PiP')
  })
})
