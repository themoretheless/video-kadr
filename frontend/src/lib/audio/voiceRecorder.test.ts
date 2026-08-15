import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { DEFAULT_VOICEOVER_DSP } from './dsp.js'
import {
  browserProcessingStillEnabled,
  buildMicrophoneConstraints,
  formatVoiceoverError,
  VoiceoverController,
} from './voiceRecorder.js'
import type { AudioGraphPort, AudioRecorderPort, VoiceoverOptions, VoiceoverRuntime } from './types.js'

class FakeMicTrack {
  stopped = false
  private readonly ended = new Set<EventListenerOrEventListenerObject>()
  stop(): void { this.stopped = true }
  addEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    if (type === 'ended') this.ended.add(listener)
  }
  removeEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    if (type === 'ended') this.ended.delete(listener)
  }
}

function stream(track: FakeMicTrack): MediaStream {
  return {
    getAudioTracks: () => [track],
    getTracks: () => [track],
  } as unknown as MediaStream
}

class FakeAudioRecorder implements AudioRecorderPort {
  readonly mimeType = 'audio/webm;codecs=opus'
  state: RecordingState = 'inactive'
  ondataavailable: ((event: BlobEvent) => void) | null = null
  onerror: ((event: ErrorEvent) => void) | null = null
  onstop: ((event: Event) => void) | null = null
  start(): void { this.state = 'recording' }
  pause(): void { this.state = 'paused' }
  resume(): void { this.state = 'recording' }
  stop(): void {
    this.state = 'inactive'
    this.ondataavailable?.({ data: new Blob(['voice']) } as BlobEvent)
    this.onstop?.(new Event('stop'))
  }
}

const OPTIONS: VoiceoverOptions = {
  microphoneDeviceId: 'mic-main',
  countdownSeconds: 3,
  dsp: { ...DEFAULT_VOICEOVER_DSP, inputGainDb: 3 },
}

function harness(): {
  runtime: VoiceoverRuntime
  track: FakeMicTrack
  recorder: FakeAudioRecorder
  graph: AudioGraphPort
  getUserMedia: ReturnType<typeof vi.fn>
  createGraph: ReturnType<typeof vi.fn>
  graphCleanup: ReturnType<typeof vi.fn>
  revokeObjectURL: ReturnType<typeof vi.fn>
} {
  const track = new FakeMicTrack()
  const input = stream(track)
  const output = stream(new FakeMicTrack())
  const recorder = new FakeAudioRecorder()
  const graphCleanup = vi.fn()
  const graph: AudioGraphPort = {
    output,
    readLevel: vi.fn(() => ({ rms: 0.25, peak: 0.6 })),
    cleanup: graphCleanup,
  }
  const getUserMedia = vi.fn(async () => input)
  const createGraph = vi.fn(async () => graph)
  const revokeObjectURL = vi.fn()
  const runtime: VoiceoverRuntime = {
    isSupported: () => true,
    getUserMedia,
    enumerateDevices: vi.fn(async () => []),
    createGraph,
    createRecorder: vi.fn(() => recorder),
    isMimeTypeSupported: (mime) => mime.includes('webm'),
    createObjectURL: vi.fn(() => 'blob:voiceover'),
    revokeObjectURL,
    now: () => Date.now(),
    setTimeout: (callback, delay) => setTimeout(callback, delay),
    clearTimeout: (timer) => clearTimeout(timer),
    setInterval: (callback, delay) => setInterval(callback, delay),
    clearInterval: (timer) => clearInterval(timer),
  }
  return { runtime, track, recorder, graph, getUserMedia, createGraph, graphCleanup, revokeObjectURL }
}

describe('voiceover constraints and errors', () => {
  it('requests only the selected raw microphone without browser enhancement DSP', () => {
    expect(buildMicrophoneConstraints('mic-2')).toEqual({
      audio: {
        deviceId: { exact: 'mic-2' },
        channelCount: 1,
        echoCancellation: false,
        noiseSuppression: false,
        autoGainControl: false,
      },
      video: false,
    })
  })

  it('explains permission and missing-device failures', () => {
    expect(formatVoiceoverError(new DOMException('denied', 'NotAllowedError'))).toContain('не разрешён')
    expect(formatVoiceoverError(new DOMException('missing', 'NotFoundError'))).toContain('не найден')
  })

  it('detects browser processing that would make the raw-DSP promise dishonest', () => {
    expect(browserProcessingStillEnabled({ noiseSuppression: true, autoGainControl: false, echoCancellation: true }))
      .toEqual(['noise suppression', 'echo cancellation'])
  })
})

describe('VoiceoverController', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-08-14T12:00:00.000Z'))
  })
  afterEach(() => vi.useRealTimers())

  it('meters the processed graph and records with countdown, pause, resume, and cleanup', async () => {
    const test = harness()
    const onCapture = vi.fn()
    const controller = new VoiceoverController({ runtime: test.runtime, onCapture })
    const started = controller.start(OPTIONS)
    await vi.advanceTimersByTimeAsync(0)

    expect(controller.getSnapshot()).toMatchObject({
      phase: 'countdown',
      permission: 'granted',
      countdownSeconds: 3,
      level: { rms: 0.25, peak: 0.6 },
    })
    expect(test.getUserMedia).toHaveBeenCalledWith(buildMicrophoneConstraints('mic-main'))
    expect(test.createGraph).toHaveBeenCalledWith(expect.anything(), OPTIONS.dsp)

    await vi.advanceTimersByTimeAsync(3000)
    await started
    expect(controller.getSnapshot().phase).toBe('recording')
    await vi.advanceTimersByTimeAsync(2100)
    expect(controller.getSnapshot().elapsedSeconds).toBe(2)

    controller.pause()
    await vi.advanceTimersByTimeAsync(4000)
    expect(controller.getSnapshot().elapsedSeconds).toBe(2)
    controller.resume()
    await vi.advanceTimersByTimeAsync(1000)
    expect(controller.getSnapshot().elapsedSeconds).toBe(3)

    controller.stop()
    await Promise.resolve()
    const result = controller.getSnapshot()
    expect(result).toMatchObject({ phase: 'completed', previewUrl: 'blob:voiceover', level: { rms: 0, peak: 0 } })
    expect(result.file).toBeInstanceOf(File)
    expect(result.file?.type).toBe('audio/webm')
    expect(result.file?.name).toMatch(/^voiceover-.*\.webm$/)
    expect(onCapture).toHaveBeenCalledWith(result.file)
    expect(test.track.stopped).toBe(true)
    expect(test.graphCleanup).toHaveBeenCalledOnce()

    controller.dispose()
    expect(test.revokeObjectURL).toHaveBeenCalledWith('blob:voiceover')
  })

  it('maps denied permission and remains retryable', async () => {
    const test = harness()
    test.getUserMedia.mockRejectedValueOnce(new DOMException('denied', 'NotAllowedError'))
    const onError = vi.fn()
    const controller = new VoiceoverController({ runtime: test.runtime, onError })

    await controller.start({ ...OPTIONS, countdownSeconds: 0 })
    expect(controller.getSnapshot()).toMatchObject({ phase: 'error', permission: 'denied' })
    expect(controller.getSnapshot().error).toContain('настройках браузера')
    expect(onError).toHaveBeenCalledOnce()
  })

  it('cancels a pending permission request and stops a late stream', async () => {
    const test = harness()
    let resolveInput: ((input: MediaStream) => void) | undefined
    test.getUserMedia.mockImplementationOnce(() => new Promise((resolve) => { resolveInput = resolve }))
    const controller = new VoiceoverController({ runtime: test.runtime })
    const started = controller.start(OPTIONS)
    controller.stop()
    expect(controller.getSnapshot().phase).toBe('idle')

    resolveInput?.(stream(test.track))
    await started
    expect(test.track.stopped).toBe(true)
    expect(test.createGraph).not.toHaveBeenCalled()
  })
})
