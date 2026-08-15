import { AnnotationModel, renderAnnotations } from './annotations.js'
import type { AnnotationReader } from './annotations.js'
import type {
  CaptureCompleteHandler,
  CaptureErrorHandler,
  CaptureInputStreams,
  CaptureListener,
  CaptureOptions,
  CaptureRuntime,
  CaptureSession,
  CaptureSnapshot,
  ExtendedDisplayMediaOptions,
  PreparedCaptureStream,
  RecorderPort,
} from './types.js'

const MIME_TYPES = [
  'video/webm;codecs=vp9,opus',
  'video/webm;codecs=vp8,opus',
  'video/webm',
] as const

export const INITIAL_CAPTURE_SNAPSHOT: CaptureSnapshot = {
  phase: 'idle',
  countdownSeconds: 0,
  elapsedSeconds: 0,
  devices: { microphones: [], cameras: [] },
  devicesLoading: false,
  deviceError: null,
  hasSystemAudio: null,
  file: null,
  previewUrl: null,
  previewCanvas: null,
  error: null,
}

interface CaptureControllerOptions {
  runtime?: CaptureRuntime
  onCapture?: CaptureCompleteHandler
  onError?: CaptureErrorHandler
  annotations?: AnnotationModel
  output?: RecordingOutputProfile
  formatError?: (error: unknown) => string
  unsupportedMessage?: string
  sourceAdapter?: RecordingSourceAdapter
}

export interface RecordingSourceAdapter {
  acquire(options: CaptureOptions): Promise<MediaStream>
  prepare(display: MediaStream, options: CaptureOptions): Promise<PreparedCaptureStream>
}

export interface RecordingOutputProfile {
  readonly mimeTypes: readonly string[]
  readonly fallbackMimeType: string
  readonly filenamePrefix: string
  readonly fallbackExtension: string
  readonly extensions?: Readonly<Record<string, string>>
}

const VIDEO_OUTPUT: RecordingOutputProfile = {
  mimeTypes: MIME_TYPES,
  fallbackMimeType: 'video/webm',
  filenamePrefix: 'capture',
  fallbackExtension: 'webm',
}

interface GeneratedTracks {
  tracks: MediaStreamTrack[]
  previewCanvas?: HTMLCanvasElement
  cleanup(): void | Promise<void>
}

export function buildDisplayMediaOptions(options: CaptureOptions): ExtendedDisplayMediaOptions {
  return {
    video: { displaySurface: options.displaySurface },
    audio: options.includeSystemAudio,
    preferCurrentTab: options.displaySurface === 'browser',
    selfBrowserSurface: 'exclude',
    surfaceSwitching: 'include',
    systemAudio: options.includeSystemAudio ? 'include' : 'exclude',
  }
}

export function buildUserMediaOptions(options: CaptureOptions): MediaStreamConstraints | null {
  if (!options.includeMicrophone && !options.includeWebcam) return null

  return {
    audio: options.includeMicrophone
      ? options.microphoneDeviceId
        ? { deviceId: { exact: options.microphoneDeviceId } }
        : true
      : false,
    video: options.includeWebcam
      ? options.webcamDeviceId
        ? { deviceId: { exact: options.webcamDeviceId } }
        : { facingMode: 'user' }
      : false,
  }
}

export function formatCaptureError(error: unknown): string {
  const name = error instanceof Error ? error.name : ''

  switch (name) {
    case 'NotAllowedError':
    case 'PermissionDeniedError':
    case 'SecurityError':
      return 'Доступ к экрану, камере или микрофону не разрешён.'
    case 'NotFoundError':
    case 'DevicesNotFoundError':
      return 'Запрошенная камера или микрофон не найдены.'
    case 'NotReadableError':
    case 'TrackStartError':
      return 'Источник уже используется другим приложением или временно недоступен.'
    case 'AbortError':
      return 'Выбор источника отменён.'
    case 'OverconstrainedError':
      return 'Выбранное устройство больше недоступно. Обновите список устройств.'
    default:
      return error instanceof Error && error.message
        ? error.message
        : 'Не удалось начать запись. Проверьте разрешения браузера.'
  }
}

export function formatElapsed(totalSeconds: number): string {
  const safeSeconds = Math.max(0, Math.floor(totalSeconds))
  const hours = Math.floor(safeSeconds / 3600)
  const minutes = Math.floor((safeSeconds % 3600) / 60)
  const seconds = safeSeconds % 60
  const minuteText = String(minutes).padStart(2, '0')
  const secondText = String(seconds).padStart(2, '0')
  return hours > 0 ? `${String(hours).padStart(2, '0')}:${minuteText}:${secondText}` : `${minuteText}:${secondText}`
}

function stopStream(stream: MediaStream | null): void {
  stream?.getTracks().forEach((track) => track.stop())
}

function waitForVideo(video: HTMLVideoElement): Promise<void> {
  video.muted = true
  video.playsInline = true
  return video.play().then(() => undefined)
}

async function composeVideo(
  display: MediaStream,
  webcam: MediaStream | null,
  annotations: AnnotationReader,
): Promise<GeneratedTracks> {
  const displayTrack = display.getVideoTracks()[0]
  if (!displayTrack) throw new Error('Выбранный источник не содержит видеодорожку.')

  if (typeof document === 'undefined') {
    throw new Error('Запись аннотаций поверх экрана не поддерживается в этой среде.')
  }

  const canvas = document.createElement('canvas')
  if (typeof canvas.captureStream !== 'function' || typeof requestAnimationFrame !== 'function') {
    throw new Error('Этот браузер не поддерживает честную запись аннотаций поверх экрана.')
  }

  const context = canvas.getContext('2d')
  if (!context) throw new Error('Браузер не смог создать видеокомпозитор.')

  const displayVideo = document.createElement('video')
  const webcamVideo = webcam?.getVideoTracks().length ? document.createElement('video') : null
  displayVideo.srcObject = display
  if (webcamVideo) webcamVideo.srcObject = webcam

  try {
    await Promise.all([waitForVideo(displayVideo), ...(webcamVideo ? [waitForVideo(webcamVideo)] : [])])
  } catch {
    displayVideo.srcObject = null
    if (webcamVideo) webcamVideo.srcObject = null
    throw new Error('Браузер не смог запустить предпросмотр для записи.')
  }

  const displaySettings = displayTrack.getSettings()
  const webcamSettings = webcam?.getVideoTracks()[0]?.getSettings()
  canvas.width = Math.max(1, displaySettings.width ?? displayVideo.videoWidth ?? 1280)
  canvas.height = Math.max(1, displaySettings.height ?? displayVideo.videoHeight ?? 720)

  let frameRequest = 0
  const drawFrame = (): void => {
    context.drawImage(displayVideo, 0, 0, canvas.width, canvas.height)

    if (webcamVideo) {
      const insetWidth = Math.max(160, Math.round(canvas.width * 0.23))
      const webcamWidth = webcamSettings?.width ?? webcamVideo.videoWidth ?? 16
      const webcamHeight = webcamSettings?.height ?? webcamVideo.videoHeight ?? 9
      const insetHeight = Math.round(insetWidth * (webcamHeight / webcamWidth))
      const margin = Math.max(12, Math.round(canvas.width * 0.015))
      const x = canvas.width - insetWidth - margin
      const y = canvas.height - insetHeight - margin

      context.save()
      context.shadowBlur = Math.max(8, Math.round(canvas.width * 0.008))
      context.shadowColor = 'rgba(0, 0, 0, 0.45)'
      context.drawImage(webcamVideo, x, y, insetWidth, insetHeight)
      context.restore()
    }
    renderAnnotations(context, annotations.getSnapshot(), canvas.width, canvas.height)
    frameRequest = requestAnimationFrame(drawFrame)
  }
  drawFrame()

  const output = canvas.captureStream(displaySettings.frameRate ?? 30)
  const outputTrack = output.getVideoTracks()[0]
  if (!outputTrack) {
    cancelAnimationFrame(frameRequest)
    displayVideo.srcObject = null
    if (webcamVideo) webcamVideo.srcObject = null
    throw new Error('Браузер не смог создать видеодорожку записи.')
  }

  return {
    tracks: [outputTrack],
    previewCanvas: canvas,
    cleanup() {
      cancelAnimationFrame(frameRequest)
      output.getTracks().forEach((track) => track.stop())
      displayVideo.pause()
      webcamVideo?.pause()
      displayVideo.srcObject = null
      if (webcamVideo) webcamVideo.srcObject = null
    },
  }
}

async function mixAudio(streams: MediaStream[]): Promise<GeneratedTracks> {
  const audioTracks = streams.flatMap((stream) => stream.getAudioTracks())
  if (audioTracks.length <= 1) return { tracks: audioTracks, cleanup() {} }

  if (typeof window === 'undefined') return { tracks: audioTracks, cleanup() {} }
  const audioWindow = window as typeof window & { webkitAudioContext?: typeof AudioContext }
  const AudioContextConstructor = window.AudioContext ?? audioWindow.webkitAudioContext
  if (!AudioContextConstructor) return { tracks: audioTracks, cleanup() {} }

  const audioContext = new AudioContextConstructor()
  const destination = audioContext.createMediaStreamDestination()
  const nodes = audioTracks.map((track) => {
    const source = audioContext.createMediaStreamSource(new MediaStream([track]))
    source.connect(destination)
    return source
  })
  if (audioContext.state === 'suspended') await audioContext.resume()

  return {
    tracks: destination.stream.getAudioTracks(),
    cleanup() {
      nodes.forEach((node) => node.disconnect())
      destination.stream.getTracks().forEach((track) => track.stop())
      void audioContext.close()
    },
  }
}

async function prepareBrowserStream(inputs: CaptureInputStreams): Promise<PreparedCaptureStream> {
  const webcamStream = inputs.user?.getVideoTracks().length ? inputs.user : null
  const video = await composeVideo(inputs.display, webcamStream, inputs.annotations)
  let audio: GeneratedTracks
  try {
    audio = await mixAudio([inputs.display, ...(inputs.user ? [inputs.user] : [])])
  } catch (error) {
    void video.cleanup()
    throw error
  }
  if (!video.previewCanvas) {
    void video.cleanup()
    void audio.cleanup()
    throw new Error('Браузер не создал live preview для аннотаций.')
  }
  let stream: MediaStream
  try {
    stream = new MediaStream([...video.tracks, ...audio.tracks])
  } catch (error) {
    void video.cleanup()
    void audio.cleanup()
    throw error
  }

  return {
    stream,
    previewCanvas: video.previewCanvas,
    cleanup() {
      void video.cleanup()
      void audio.cleanup()
    },
  }
}

export function createCaptureRuntime(): CaptureRuntime {
  return {
    isSupported: () =>
      typeof navigator !== 'undefined' &&
      typeof MediaRecorder !== 'undefined' &&
      typeof navigator.mediaDevices?.getDisplayMedia === 'function',
    getDisplayMedia: (options) => {
      if (!navigator.mediaDevices?.getDisplayMedia) {
        throw new Error('Запись экрана не поддерживается этим браузером.')
      }
      return navigator.mediaDevices.getDisplayMedia(options)
    },
    getUserMedia: (options) => {
      if (!navigator.mediaDevices?.getUserMedia) {
        throw new Error('Камера и микрофон не поддерживаются этим браузером.')
      }
      return navigator.mediaDevices.getUserMedia(options)
    },
    enumerateDevices: () => navigator.mediaDevices?.enumerateDevices?.() ?? Promise.resolve([]),
    prepareStream: prepareBrowserStream,
    createRecorder: (stream, options) => new MediaRecorder(stream, options),
    isMimeTypeSupported: (mimeType) => MediaRecorder.isTypeSupported(mimeType),
    createObjectURL: (blob) => URL.createObjectURL(blob),
    revokeObjectURL: (url) => URL.revokeObjectURL(url),
    now: () => Date.now(),
    setTimeout: (callback, delayMs) => setTimeout(callback, delayMs),
    clearTimeout: (timer) => clearTimeout(timer),
    setInterval: (callback, delayMs) => setInterval(callback, delayMs),
    clearInterval: (timer) => clearInterval(timer),
  }
}

export class CaptureController implements CaptureSession {
  private readonly runtime: CaptureRuntime
  private readonly onCapture?: CaptureCompleteHandler
  private readonly onError?: CaptureErrorHandler
  private readonly annotations: AnnotationModel
  private readonly output: RecordingOutputProfile
  private readonly formatError: (error: unknown) => string
  private readonly unsupportedMessage: string
  private readonly sourceAdapter?: RecordingSourceAdapter
  private snapshot = INITIAL_CAPTURE_SNAPSHOT
  private readonly listeners = new Set<CaptureListener>()
  private generation = 0
  private displayStream: MediaStream | null = null
  private userStream: MediaStream | null = null
  private prepared: PreparedCaptureStream | null = null
  private recorder: RecorderPort | null = null
  private chunks: Blob[] = []
  private displayTrack: MediaStreamTrack | null = null
  private readonly displayEnded = (): void => this.stop()
  private countdownTimer: ReturnType<typeof setTimeout> | null = null
  private countdownResolve: (() => void) | null = null
  private elapsedTimer: ReturnType<typeof setInterval> | null = null
  private startedAt = 0
  private pausedAt = 0
  private pausedDuration = 0
  private disposed = false

  constructor(options: CaptureControllerOptions = {}) {
    this.runtime = options.runtime ?? createCaptureRuntime()
    this.onCapture = options.onCapture
    this.onError = options.onError
    this.annotations = options.annotations ?? new AnnotationModel()
    this.output = options.output ?? VIDEO_OUTPUT
    this.formatError = options.formatError ?? formatCaptureError
    this.unsupportedMessage = options.unsupportedMessage ?? 'Запись экрана не поддерживается этим браузером. Используйте актуальный Chrome, Edge или Firefox.'
    this.sourceAdapter = options.sourceAdapter
  }

  getSnapshot(): CaptureSnapshot {
    return {
      ...this.snapshot,
      devices: {
        microphones: [...this.snapshot.devices.microphones],
        cameras: [...this.snapshot.devices.cameras],
      },
    }
  }

  subscribe(listener: CaptureListener): () => void {
    this.listeners.add(listener)
    listener(this.getSnapshot())
    return () => this.listeners.delete(listener)
  }

  async loadDevices(): Promise<void> {
    if (this.disposed) return
    this.publish({ devicesLoading: true, deviceError: null })
    try {
      const devices = await this.runtime.enumerateDevices()
      if (this.disposed) return
      this.publish({
        devicesLoading: false,
        devices: {
          microphones: devices.filter((device) => device.kind === 'audioinput'),
          cameras: devices.filter((device) => device.kind === 'videoinput'),
        },
      })
    } catch (error) {
      if (this.disposed) return
      this.publish({ devicesLoading: false, deviceError: this.formatError(error) })
    }
  }

  async start(options: CaptureOptions): Promise<void> {
    if (this.disposed || ['requesting', 'countdown', 'recording', 'paused', 'stopping'].includes(this.snapshot.phase)) return
    this.releasePreview()
    this.cleanupMedia()
    this.annotations.clear()

    if (!this.runtime.isSupported()) {
      this.fail(new Error(this.unsupportedMessage))
      return
    }

    const currentGeneration = ++this.generation
    this.publish({
      phase: 'requesting',
      countdownSeconds: 0,
      elapsedSeconds: 0,
      hasSystemAudio: null,
      file: null,
      error: null,
    })

    try {
      const display = this.sourceAdapter
        ? await this.sourceAdapter.acquire(options)
        : await this.runtime.getDisplayMedia(buildDisplayMediaOptions(options))
      if (!this.isCurrent(currentGeneration)) {
        stopStream(display)
        return
      }
      this.displayStream = display
      this.displayTrack = display.getVideoTracks?.()[0] ?? null
      this.displayTrack?.addEventListener('ended', this.displayEnded)
      this.publish({ hasSystemAudio: options.includeSystemAudio ? display.getAudioTracks().length > 0 : null })

      const userOptions = buildUserMediaOptions(options)
      if (userOptions) {
        const user = await this.runtime.getUserMedia(userOptions)
        if (!this.isCurrent(currentGeneration)) {
          stopStream(user)
          this.cleanupMedia()
          return
        }
        this.userStream = user
      }

      const prepared = this.sourceAdapter
        ? await this.sourceAdapter.prepare(display, options)
        : await this.runtime.prepareStream({ display, user: this.userStream, annotations: this.annotations })
      if (!this.isCurrent(currentGeneration)) {
        void prepared.cleanup()
        this.cleanupMedia()
        return
      }
      this.prepared = prepared
      this.publish({ previewCanvas: prepared.previewCanvas })

      if (!(await this.runCountdown(currentGeneration, Math.max(0, Math.floor(options.countdownSeconds))))) return
      this.beginRecording(currentGeneration)
    } catch (error) {
      if (this.isCurrent(currentGeneration)) this.fail(error)
    }
  }

  pause(): void {
    if (this.snapshot.phase !== 'recording' || this.recorder?.state !== 'recording') return
    try {
      this.recorder.pause()
      this.pausedAt = this.runtime.now()
      this.refreshElapsed()
      this.publish({ phase: 'paused' })
    } catch (error) {
      this.fail(error)
    }
  }

  resume(): void {
    if (this.snapshot.phase !== 'paused' || this.recorder?.state !== 'paused') return
    try {
      this.recorder.resume()
      this.pausedDuration += Math.max(0, this.runtime.now() - this.pausedAt)
      this.pausedAt = 0
      this.publish({ phase: 'recording' })
    } catch (error) {
      this.fail(error)
    }
  }

  stop(): void {
    if (this.disposed) return

    if (this.snapshot.phase === 'requesting' || this.snapshot.phase === 'countdown') {
      this.generation += 1
      this.clearCountdown()
      this.cleanupMedia()
      this.publish({ phase: 'idle', countdownSeconds: 0, elapsedSeconds: 0, hasSystemAudio: null, previewCanvas: null })
      return
    }

    if (this.snapshot.phase !== 'recording' && this.snapshot.phase !== 'paused') return
    this.refreshElapsed()
    this.clearElapsedTimer()
    this.publish({ phase: 'stopping' })

    try {
      if (this.recorder?.state === 'inactive') void this.finishRecording()
      else this.recorder?.stop()
    } catch (error) {
      this.fail(error)
    }
  }

  dispose(): void {
    if (this.disposed) return
    this.disposed = true
    this.generation += 1
    this.clearCountdown()
    this.clearElapsedTimer()
    if (this.recorder) {
      this.recorder.ondataavailable = null
      this.recorder.onerror = null
      this.recorder.onstop = null
      try {
        if (this.recorder.state !== 'inactive') this.recorder.stop()
      } catch {
        // The source tracks are still stopped below.
      }
    }
    this.recorder = null
    this.cleanupMedia()
    this.releasePreview()
    this.listeners.clear()
  }

  private publish(patch: Partial<CaptureSnapshot>): void {
    if (this.disposed) return
    this.snapshot = { ...this.snapshot, ...patch }
    const next = this.getSnapshot()
    this.listeners.forEach((listener) => listener(next))
  }

  private isCurrent(generation: number): boolean {
    return !this.disposed && this.generation === generation
  }

  private async runCountdown(generation: number, seconds: number): Promise<boolean> {
    for (let remaining = seconds; remaining > 0; remaining -= 1) {
      if (!this.isCurrent(generation)) return false
      this.publish({ phase: 'countdown', countdownSeconds: remaining })
      await new Promise<void>((resolve) => {
        this.countdownResolve = resolve
        this.countdownTimer = this.runtime.setTimeout(resolve, 1000)
      })
      this.countdownTimer = null
      this.countdownResolve = null
    }
    return this.isCurrent(generation)
  }

  private beginRecording(generation: number): void {
    if (!this.isCurrent(generation) || !this.prepared) return
    const mimeType = this.output.mimeTypes.find((candidate) => this.runtime.isMimeTypeSupported(candidate))
    this.chunks = []
    this.recorder = this.runtime.createRecorder(this.prepared.stream, mimeType ? { mimeType } : undefined)
    this.recorder.ondataavailable = (event) => {
      if (event.data.size > 0) this.chunks.push(event.data)
    }
    this.recorder.onerror = (event) => {
      const recorderError = 'error' in event && event.error instanceof Error ? event.error : new Error('Ошибка MediaRecorder.')
      this.fail(recorderError)
    }
    this.recorder.onstop = () => {
      void this.finishRecording()
    }
    this.startedAt = this.runtime.now()
    this.pausedAt = 0
    this.pausedDuration = 0
    this.publish({ phase: 'recording', countdownSeconds: 0, elapsedSeconds: 0 })
    this.elapsedTimer = this.runtime.setInterval(() => this.refreshElapsed(), 250)
    this.recorder.start(1000)
  }

  private refreshElapsed(): void {
    if (!this.startedAt) return
    const end = this.pausedAt || this.runtime.now()
    const elapsed = Math.max(0, end - this.startedAt - this.pausedDuration)
    this.publish({ elapsedSeconds: Math.floor(elapsed / 1000) })
  }

  private async finishRecording(): Promise<void> {
    if (this.disposed || this.snapshot.phase === 'completed') return
    this.clearElapsedTimer()
    const recorderMime = this.recorder?.mimeType || this.output.fallbackMimeType
    if (this.recorder) {
      this.recorder.ondataavailable = null
      this.recorder.onerror = null
      this.recorder.onstop = null
    }
    this.recorder = null
    const timestamp = this.runtime.now()
    const mimeType = recorderMime.split(';')[0] || this.output.fallbackMimeType
    const extension = this.output.extensions?.[mimeType] ?? this.output.fallbackExtension
    const blob = new Blob(this.chunks, { type: recorderMime })
    const filename = `${this.output.filenamePrefix}-${new Date(timestamp).toISOString().replace(/[:.]/g, '-')}.${extension}`
    const file = new File([blob], filename, { type: mimeType, lastModified: timestamp })
    const previewUrl = this.runtime.createObjectURL(file)
    this.chunks = []
    this.cleanupMedia()
    this.publish({ phase: 'completed', countdownSeconds: 0, file, previewUrl, previewCanvas: null })

    try {
      await this.onCapture?.(file)
    } catch (error) {
      const message = `Запись создана, но не передана дальше: ${this.formatError(error)}`
      this.publish({ error: message })
      this.onError?.(message)
    }
  }

  private fail(error: unknown): void {
    const message = this.formatError(error)
    this.generation += 1
    this.clearCountdown()
    this.clearElapsedTimer()
    if (this.recorder) {
      this.recorder.ondataavailable = null
      this.recorder.onerror = null
      this.recorder.onstop = null
      try {
        if (this.recorder.state !== 'inactive') this.recorder.stop()
      } catch {
        // Source cleanup below is sufficient after a recorder failure.
      }
    }
    this.recorder = null
    this.chunks = []
    this.cleanupMedia()
    this.publish({ phase: 'error', countdownSeconds: 0, error: message, previewCanvas: null })
    this.onError?.(message)
  }

  private cleanupMedia(): void {
    this.displayTrack?.removeEventListener('ended', this.displayEnded)
    this.displayTrack = null
    if (this.prepared) {
      try {
        void this.prepared.cleanup()
      } catch {
        // Input tracks are still released below.
      }
    }
    this.prepared = null
    stopStream(this.displayStream)
    stopStream(this.userStream)
    this.displayStream = null
    this.userStream = null
  }

  private releasePreview(): void {
    if (this.snapshot.previewUrl) this.runtime.revokeObjectURL(this.snapshot.previewUrl)
    this.snapshot = { ...this.snapshot, previewUrl: null, previewCanvas: null, file: null }
  }

  private clearCountdown(): void {
    if (this.countdownTimer !== null) this.runtime.clearTimeout(this.countdownTimer)
    this.countdownTimer = null
    const resolve = this.countdownResolve
    this.countdownResolve = null
    resolve?.()
  }

  private clearElapsedTimer(): void {
    if (this.elapsedTimer !== null) this.runtime.clearInterval(this.elapsedTimer)
    this.elapsedTimer = null
  }
}
