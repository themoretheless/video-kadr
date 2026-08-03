import { beforeEach, describe, expect, it, vi } from 'vitest'
import { BrowserCaptureError, BrowserCaptureSession, captureCapabilities, startBrowserCapture } from './browser-capture'
import { validateCaptureRequest } from './domain/capture-session'

const options = (value: Partial<Parameters<typeof validateCaptureRequest>[0]>) => validateCaptureRequest({
  screen: true, camera: true, microphone: false, systemAudio: false,
  composition: { mode: 'screen_with_camera', cameraCorner: 'bottom_right', cameraScale: .25 },
  fps: 30, insertAt: 'timeline_end', ...value,
  ...(value.composition ? { composition: value.composition } : {}),
} as Parameters<typeof validateCaptureRequest>[0])

class Track extends EventTarget {
  stops = 0
  constructor(readonly kind: 'video' | 'audio', private readonly settings: MediaTrackSettings = {}) { super() }
  stop() { this.stops++ }
  getSettings() { return this.settings }
}
class Stream {
  constructor(readonly tracks: Track[] = []) {}
  getTracks() { return this.tracks }
  getVideoTracks() { return this.tracks.filter(track => track.kind === 'video') }
  getAudioTracks() { return this.tracks.filter(track => track.kind === 'audio') }
}
class Recorder {
  static isTypeSupported = vi.fn(() => true)
  static last: Recorder | null = null
  state: RecordingState = 'inactive'
  ondataavailable: ((event: { data: Blob }) => void) | null = null
  onstop: (() => void) | null = null
  onerror: (() => void) | null = null
  constructor(readonly stream: MediaStream, readonly options: MediaRecorderOptions) { Recorder.last = this }
  start() { this.state = 'recording' }
  pause() { this.state = 'paused' }
  resume() { this.state = 'recording' }
  stop() { this.ondataavailable?.({ data: new Blob(['webm']) }); this.state = 'inactive'; this.onstop?.() }
}

const displayVideo = new Track('video', { width: 1920, height: 1080, frameRate: 30 })
const displayAudio = new Track('audio')
const cameraVideo = new Track('video', { width: 640, height: 480 })
const microphone = new Track('audio')
const display = new Stream([displayVideo, displayAudio])
const canvasVideo = new Track('video', { width: 1920, height: 1080 })
const mixedAudio = new Track('audio')
const drawImage = vi.fn()

beforeEach(() => {
  vi.restoreAllMocks()
  for (const track of [displayVideo, displayAudio, cameraVideo, microphone, canvasVideo, mixedAudio]) track.stops = 0
  Object.defineProperty(window, 'isSecureContext', { configurable: true, value: true })
  Object.defineProperty(globalThis, 'MediaStream', { configurable: true, value: Stream })
  Object.defineProperty(globalThis, 'MediaRecorder', { configurable: true, value: Recorder })
  Object.defineProperty(globalThis, 'AudioContext', { configurable: true, value: class {
    state: AudioContextState = 'running'
    resume() { this.state = 'running'; return Promise.resolve() }
    createMediaStreamDestination() { return { stream: new Stream([mixedAudio]) } }
    createMediaStreamSource() { return { connect: vi.fn() } }
    createDynamicsCompressor() { return { threshold: { value: 0 }, knee: { value: 0 }, ratio: { value: 0 }, attack: { value: 0 }, release: { value: 0 }, connect: vi.fn() } }
    close() { return Promise.resolve() }
  } })
  Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: {
    getDisplayMedia: vi.fn(async () => display), getUserMedia: vi.fn(async (constraints: MediaStreamConstraints) => constraints.video ? new Stream([cameraVideo]) : new Stream([microphone])),
  } })
  Object.defineProperty(navigator, 'storage', { configurable: true, value: {} })
  vi.spyOn(HTMLMediaElement.prototype, 'play').mockResolvedValue()
  Object.defineProperty(HTMLMediaElement.prototype, 'srcObject', { configurable: true, writable: true, value: null })
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({ drawImage, fillRect: vi.fn(), fillStyle: '' } as unknown as CanvasRenderingContext2D)
  Object.defineProperty(HTMLCanvasElement.prototype, 'captureStream', { configurable: true, value: vi.fn(() => new Stream([canvasVideo])) })
  vi.stubGlobal('requestAnimationFrame', vi.fn(() => 1))
  vi.stubGlobal('cancelAnimationFrame', vi.fn())
})

describe('browser capture adapter', () => {
  it('reports capabilities and composes screen, camera, system audio and microphone', async () => {
    expect(captureCapabilities()).toMatchObject({ supported: true, mimeType: 'video/webm;codecs=vp9,opus' })
    const session = await startBrowserCapture(options({ screen: true, camera: true, microphone: true, systemAudio: true }))
    expect(navigator.mediaDevices.getDisplayMedia).toHaveBeenCalledWith({ video: true, audio: true })
    expect(navigator.mediaDevices.getUserMedia).toHaveBeenNthCalledWith(1, { video: true, audio: false })
    expect(navigator.mediaDevices.getUserMedia).toHaveBeenNthCalledWith(2, { video: false, audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true } })
    expect(drawImage).toHaveBeenCalledTimes(2)
    expect(session.previewStream?.getTracks()).toHaveLength(2)
    session.pause(); expect(session.state).toBe('paused'); session.resume()
    const first = session.stop(), second = session.stop()
    expect(second).toBe(first)
    await expect(first).resolves.toMatchObject({ width: 1920, height: 1080, hasAudio: true, stoppedBy: 'user', warnings: [] })
    for (const track of [displayVideo, displayAudio, cameraVideo, microphone, canvasVideo, mixedAudio]) expect(track.stops).toBe(1)
  })

  it('maps denied display permission and cleans already acquired tracks', async () => {
    vi.mocked(navigator.mediaDevices.getDisplayMedia).mockRejectedValueOnce(new DOMException('no', 'NotAllowedError'))
    await expect(startBrowserCapture(options({ composition: { mode: 'screen_with_camera', cameraCorner: 'top_left', cameraScale: .2 }, fps: 24 })))
      .rejects.toMatchObject({ code: 'permission_denied' } satisfies Partial<BrowserCaptureError>)
  })

  it('finishes exactly once when browser sharing ends and warns about absent system audio', async () => {
    vi.mocked(navigator.mediaDevices.getDisplayMedia).mockResolvedValueOnce(new Stream([displayVideo]) as unknown as MediaStream)
    const session = await startBrowserCapture(options({ systemAudio: true, composition: { mode: 'screen_with_camera', cameraCorner: 'top_left', cameraScale: .2 }, fps: 15 }))
    displayVideo.dispatchEvent(new Event('ended'))
    const result = await session.stop()
    expect(result).toMatchObject({ stoppedBy: 'display_ended', hasAudio: false, warnings: ['system_audio_unavailable'] })
    expect(displayVideo.stops).toBe(1)
  })

  it('routes recorder errors through the same terminal cleanup promise', async () => {
    const session = await startBrowserCapture(options({ screen: true, camera: true }))
    Recorder.last!.onerror?.()
    await expect(session.completion).rejects.toMatchObject({ code: 'recorder_failed' })
    for (const track of [displayVideo, displayAudio, cameraVideo, canvasVideo]) expect(track.stops).toBe(1)
  })

  it('auto-stops camera-only capture when its sole visual source ends', async () => {
    const session = await startBrowserCapture(options({
      screen: false, camera: true,
      composition: { mode: 'camera_only', cameraCorner: 'bottom_right', cameraScale: .25 },
    }))
    cameraVideo.dispatchEvent(new Event('ended'))
    await expect(session.completion).resolves.toMatchObject({ stoppedBy: 'user' })
    expect(cameraVideo.stops).toBe(1)
  })

  it('reports active media duration without counting a paused interval', async () => {
    let now = 0
    vi.spyOn(performance, 'now').mockImplementation(() => now)
    const session = await startBrowserCapture(options({ screen: true, camera: true }))
    now = 100; session.pause()
    now = 1_100; session.resume()
    now = 2_100
    await expect(session.stop()).resolves.toMatchObject({ durationMs: 1_100, durationSeconds: 1.1 })
  })

  it('routes byte-limit termination through finalization and cleanup', async () => {
    const session = await BrowserCaptureSession.start({ mode: 'screen', microphone: false, systemAudio: false, maxBytes: 2 })
    Recorder.last!.ondataavailable?.({ data: new Blob(['too large']) })
    await expect(session.completion).rejects.toMatchObject({ code: 'limit_reached' })
    expect(displayVideo.stops).toBe(1)
  })

  it('routes duration-limit termination through finalization and cleanup', async () => {
    vi.useFakeTimers()
    try {
      const session = await BrowserCaptureSession.start({ mode: 'screen', microphone: false, systemAudio: false, maxDurationMs: 10 })
      const completion = expect(session.completion).rejects.toMatchObject({ code: 'limit_reached' })
      await vi.advanceTimersByTimeAsync(10)
      await completion
      expect(displayVideo.stops).toBe(1)
    } finally { vi.useRealTimers() }
  })

  it('stops immediately when the bounded memory sink overflows', async () => {
    const session = await BrowserCaptureSession.start({ mode: 'screen', microphone: false, systemAudio: false })
    const failure = expect(session.completion).rejects.toMatchObject({ code: 'limit_reached' })
    Recorder.last!.ondataavailable?.({ data: { size: 129 * 1024 * 1024 } as Blob })
    await failure
    expect(displayVideo.stops).toBe(1)
  })

  it('stops immediately when an OPFS chunk write fails', async () => {
    const writable = { write: vi.fn(async () => { throw new Error('quota') }), close: vi.fn(async () => undefined), abort: vi.fn(async () => undefined) }
    const handle = { createWritable: vi.fn(async () => writable), getFile: vi.fn() }
    const directory = { getFileHandle: vi.fn(async () => handle), removeEntry: vi.fn(async () => undefined) }
    const root = { getDirectoryHandle: vi.fn(async () => directory) }
    Object.defineProperty(navigator, 'storage', { configurable: true, value: { getDirectory: vi.fn(async () => root) } })
    const session = await BrowserCaptureSession.start({ mode: 'screen', microphone: false, systemAudio: false })
    const failure = expect(session.completion).rejects.toMatchObject({ code: 'storage_failed' })
    Recorder.last!.ondataavailable?.({ data: new Blob(['chunk']) })
    await failure
    expect(displayVideo.stops).toBe(1)
  })
})
