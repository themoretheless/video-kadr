import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  buildDisplayMediaOptions,
  buildUserMediaOptions,
  CaptureController,
  createCaptureRuntime,
  formatCaptureError,
  formatElapsed,
} from './mediaRecorder.js'
import { AnnotationModel } from './annotations.js'
import type {
  CaptureOptions,
  CaptureRuntime,
  PreparedCaptureStream,
  RecorderPort,
} from './types.js'

class FakeTrack {
  readonly kind: 'audio' | 'video'
  stopped = false
  private readonly listeners = new Set<EventListenerOrEventListenerObject>()

  constructor(kind: 'audio' | 'video') {
    this.kind = kind
  }

  stop(): void {
    this.stopped = true
  }

  getSettings(): MediaTrackSettings {
    return this.kind === 'video' ? { width: 1280, height: 720, frameRate: 30 } : {}
  }

  addEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    if (type === 'ended') this.listeners.add(listener)
  }

  removeEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    if (type === 'ended') this.listeners.delete(listener)
  }

  end(): void {
    const event = new Event('ended')
    this.listeners.forEach((listener) => {
      if (typeof listener === 'function') listener(event)
      else listener.handleEvent(event)
    })
  }
}

function fakeStream(tracks: FakeTrack[]): MediaStream {
  return {
    getTracks: () => tracks,
    getVideoTracks: () => tracks.filter((track) => track.kind === 'video'),
    getAudioTracks: () => tracks.filter((track) => track.kind === 'audio'),
  } as unknown as MediaStream
}

class FakeRecorder implements RecorderPort {
  readonly mimeType: string
  state: RecordingState = 'inactive'
  ondataavailable: ((event: BlobEvent) => void) | null = null
  onerror: ((event: ErrorEvent) => void) | null = null
  onstop: ((event: Event) => void) | null = null

  constructor(mimeType = 'video/webm;codecs=vp8,opus') {
    this.mimeType = mimeType
  }

  start(): void {
    this.state = 'recording'
  }

  pause(): void {
    this.state = 'paused'
  }

  resume(): void {
    this.state = 'recording'
  }

  stop(): void {
    this.state = 'inactive'
    this.ondataavailable?.({ data: new Blob(['video-data'], { type: this.mimeType }) } as BlobEvent)
    this.onstop?.(new Event('stop'))
  }
}

const DEFAULT_OPTIONS: CaptureOptions = {
  displaySurface: 'monitor',
  includeSystemAudio: true,
  includeMicrophone: false,
  microphoneDeviceId: undefined,
  includeWebcam: false,
  webcamDeviceId: undefined,
  countdownSeconds: 3,
}

function makeRuntime(display: MediaStream, user: MediaStream | null = null): {
  runtime: CaptureRuntime
  recorder: FakeRecorder
  preparedCleanup: ReturnType<typeof vi.fn>
  getDisplayMedia: ReturnType<typeof vi.fn>
  getUserMedia: ReturnType<typeof vi.fn>
  prepareStream: ReturnType<typeof vi.fn>
  createObjectURL: ReturnType<typeof vi.fn>
  revokeObjectURL: ReturnType<typeof vi.fn>
  previewCanvas: HTMLCanvasElement
} {
  const recorder = new FakeRecorder()
  const previewCanvas = document.createElement('canvas')
  const preparedCleanup = vi.fn()
  const getDisplayMedia = vi.fn(async () => display)
  const getUserMedia = vi.fn(async () => user ?? fakeStream([]))
  const prepareStream = vi.fn(async ({ display: selectedDisplay }: { display: MediaStream }): Promise<PreparedCaptureStream> => ({
    stream: selectedDisplay,
    previewCanvas,
    cleanup: preparedCleanup,
  }))
  const createObjectURL = vi.fn(() => 'blob:capture-preview')
  const revokeObjectURL = vi.fn()

  const runtime: CaptureRuntime = {
    isSupported: () => true,
    getDisplayMedia,
    getUserMedia,
    enumerateDevices: vi.fn(async () => []),
    prepareStream,
    createRecorder: vi.fn(() => recorder),
    isMimeTypeSupported: (mimeType) => mimeType.includes('vp8'),
    createObjectURL,
    revokeObjectURL,
    now: () => Date.now(),
    setTimeout: (callback, delayMs) => setTimeout(callback, delayMs),
    clearTimeout: (timer) => clearTimeout(timer),
    setInterval: (callback, delayMs) => setInterval(callback, delayMs),
    clearInterval: (timer) => clearInterval(timer),
  }

  return {
    runtime,
    recorder,
    preparedCleanup,
    getDisplayMedia,
    getUserMedia,
    prepareStream,
    createObjectURL,
    revokeObjectURL,
    previewCanvas,
  }
}

describe('capture constraints and messages', () => {
  it('passes source hints and exact selected devices to browser APIs', () => {
    const options: CaptureOptions = {
      ...DEFAULT_OPTIONS,
      displaySurface: 'browser',
      microphoneDeviceId: 'mic-2',
      webcamDeviceId: 'cam-3',
      includeMicrophone: true,
      includeWebcam: true,
    }

    expect(buildDisplayMediaOptions(options)).toMatchObject({
      video: { displaySurface: 'browser' },
      audio: true,
      preferCurrentTab: true,
      systemAudio: 'include',
    })
    expect(buildUserMediaOptions(options)).toEqual({
      audio: { deviceId: { exact: 'mic-2' } },
      video: { deviceId: { exact: 'cam-3' } },
    })
  })

  it('formats timers and permission failures for the UI', () => {
    expect(formatElapsed(65)).toBe('01:05')
    expect(formatElapsed(3661)).toBe('01:01:01')
    expect(formatCaptureError(new DOMException('denied', 'NotAllowedError'))).toContain('не разрешён')
    expect(formatCaptureError(new DOMException('missing', 'NotFoundError'))).toContain('не найдены')
  })
})

describe('CaptureController', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-08-14T10:00:00.000Z'))
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('records display, system audio, microphone, and webcam with countdown and controls', async () => {
    const displayVideo = new FakeTrack('video')
    const systemAudio = new FakeTrack('audio')
    const microphone = new FakeTrack('audio')
    const webcam = new FakeTrack('video')
    const display = fakeStream([displayVideo, systemAudio])
    const user = fakeStream([microphone, webcam])
    const harness = makeRuntime(display, user)
    const annotations = new AnnotationModel()
    const onCapture = vi.fn()
    const controller = new CaptureController({ runtime: harness.runtime, onCapture, annotations })

    const start = controller.start({
      ...DEFAULT_OPTIONS,
      includeMicrophone: true,
      microphoneDeviceId: 'mic-main',
      includeWebcam: true,
      webcamDeviceId: 'cam-main',
    })
    await vi.advanceTimersByTimeAsync(0)

    expect(controller.getSnapshot()).toMatchObject({
      phase: 'countdown',
      countdownSeconds: 3,
      hasSystemAudio: true,
      previewCanvas: harness.previewCanvas,
    })
    expect(harness.getUserMedia).toHaveBeenCalledWith({
      audio: { deviceId: { exact: 'mic-main' } },
      video: { deviceId: { exact: 'cam-main' } },
    })
    expect(harness.prepareStream).toHaveBeenCalledWith(expect.objectContaining({ display, user }))

    await vi.advanceTimersByTimeAsync(3000)
    await start
    expect(controller.getSnapshot().phase).toBe('recording')
    expect(harness.recorder.state).toBe('recording')
    annotations.begin('arrow', { x: 0.1, y: 0.2 }, { color: '#ff0000', width: 4 })
    annotations.end({ x: 0.8, y: 0.7 })
    const preparedInputs = harness.prepareStream.mock.calls[0]?.[0]
    expect(preparedInputs.annotations.getSnapshot().strokes[0]?.tool).toBe('arrow')

    await vi.advanceTimersByTimeAsync(2250)
    expect(controller.getSnapshot().elapsedSeconds).toBe(2)
    controller.pause()
    expect(controller.getSnapshot().phase).toBe('paused')

    await vi.advanceTimersByTimeAsync(5000)
    expect(controller.getSnapshot().elapsedSeconds).toBe(2)
    controller.resume()
    await vi.advanceTimersByTimeAsync(1000)
    expect(controller.getSnapshot().elapsedSeconds).toBe(3)

    controller.stop()
    await Promise.resolve()
    const result = controller.getSnapshot()
    expect(result.phase).toBe('completed')
    expect(result.file).toBeInstanceOf(File)
    expect(result.file?.name).toMatch(/^capture-.*\.webm$/)
    expect(result.file?.type).toBe('video/webm')
    expect(result.previewUrl).toBe('blob:capture-preview')
    expect(result.previewCanvas).toBeNull()
    expect(onCapture).toHaveBeenCalledWith(result.file)
    expect(harness.preparedCleanup).toHaveBeenCalledOnce()
    expect([displayVideo, systemAudio, microphone, webcam].every((track) => track.stopped)).toBe(true)

    controller.dispose()
    expect(harness.revokeObjectURL).toHaveBeenCalledWith('blob:capture-preview')
  })

  it('maps permission errors and returns to an actionable error state', async () => {
    const display = fakeStream([new FakeTrack('video')])
    const harness = makeRuntime(display)
    const denied = new DOMException('denied', 'NotAllowedError')
    harness.getDisplayMedia.mockRejectedValueOnce(denied)
    const onError = vi.fn()
    const controller = new CaptureController({ runtime: harness.runtime, onError })

    await controller.start({ ...DEFAULT_OPTIONS, countdownSeconds: 0 })

    expect(controller.getSnapshot()).toMatchObject({ phase: 'error' })
    expect(controller.getSnapshot().error).toContain('не разрешён')
    expect(onError).toHaveBeenCalledWith(expect.stringContaining('не разрешён'))
  })

  it('cancels a pending picker and stops a stream that resolves afterwards', async () => {
    const track = new FakeTrack('video')
    const display = fakeStream([track])
    let resolveDisplay: ((stream: MediaStream) => void) | undefined
    const harness = makeRuntime(display)
    harness.getDisplayMedia.mockImplementationOnce(() => new Promise<MediaStream>((resolve) => {
      resolveDisplay = resolve
    }))
    const controller = new CaptureController({ runtime: harness.runtime })

    const start = controller.start(DEFAULT_OPTIONS)
    expect(controller.getSnapshot().phase).toBe('requesting')
    controller.stop()
    expect(controller.getSnapshot().phase).toBe('idle')

    resolveDisplay?.(display)
    await start
    expect(track.stopped).toBe(true)
    expect(harness.prepareStream).not.toHaveBeenCalled()
  })

  it('stops automatically when browser sharing ends', async () => {
    const displayTrack = new FakeTrack('video')
    const display = fakeStream([displayTrack])
    const harness = makeRuntime(display)
    const controller = new CaptureController({ runtime: harness.runtime })

    await controller.start({ ...DEFAULT_OPTIONS, countdownSeconds: 0 })
    expect(controller.getSnapshot().phase).toBe('recording')
    displayTrack.end()
    await Promise.resolve()
    expect(controller.getSnapshot().phase).toBe('completed')
  })

  it('fails closed when canvas capture cannot record annotations honestly', async () => {
    const descriptor = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, 'captureStream')
    Object.defineProperty(HTMLCanvasElement.prototype, 'captureStream', { configurable: true, value: undefined })
    const display = fakeStream([new FakeTrack('video')])
    const runtime = createCaptureRuntime()

    try {
      await expect(runtime.prepareStream({ display, user: null, annotations: new AnnotationModel() }))
        .rejects.toThrow('честную запись аннотаций')
    } finally {
      if (descriptor) Object.defineProperty(HTMLCanvasElement.prototype, 'captureStream', descriptor)
      else delete (HTMLCanvasElement.prototype as Partial<HTMLCanvasElement>).captureStream
    }
  })
})
