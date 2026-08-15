import { createVoiceoverGraph } from './dsp.js'
import { createCaptureRuntime } from '$lib/capture/mediaRecorder.js'
import type {
  AudioGraphPort,
  AudioRecorderPort,
  VoiceoverCompleteHandler,
  VoiceoverErrorHandler,
  VoiceoverListener,
  VoiceoverOptions,
  VoiceoverRuntime,
  VoiceoverSession,
  VoiceoverSnapshot,
} from './types.js'

const MIME_TYPES = [
  'audio/webm;codecs=opus',
  'audio/webm',
  'audio/ogg;codecs=opus',
  'audio/mp4;codecs=mp4a.40.2',
  'audio/mp4',
] as const

const INITIAL_SNAPSHOT: VoiceoverSnapshot = {
  phase: 'idle',
  permission: 'prompt',
  countdownSeconds: 0,
  elapsedSeconds: 0,
  level: { rms: 0, peak: 0 },
  microphones: [],
  devicesLoading: false,
  deviceError: null,
  file: null,
  previewUrl: null,
  error: null,
}

interface VoiceoverControllerOptions {
  runtime?: VoiceoverRuntime
  onCapture?: VoiceoverCompleteHandler
  onError?: VoiceoverErrorHandler
}

export function buildMicrophoneConstraints(deviceId?: string): MediaStreamConstraints {
  return {
    audio: {
      ...(deviceId ? { deviceId: { exact: deviceId } } : {}),
      channelCount: 1,
      echoCancellation: false,
      noiseSuppression: false,
      autoGainControl: false,
    },
    video: false,
  }
}

export function formatVoiceoverError(error: unknown): string {
  const name = error instanceof Error ? error.name : ''
  switch (name) {
    case 'NotAllowedError':
    case 'PermissionDeniedError':
    case 'SecurityError':
      return 'Доступ к микрофону не разрешён. Разрешите его в настройках браузера и попробуйте снова.'
    case 'NotFoundError':
    case 'DevicesNotFoundError':
      return 'Микрофон не найден. Подключите устройство и обновите список.'
    case 'NotReadableError':
    case 'TrackStartError':
      return 'Микрофон уже используется другим приложением или временно недоступен.'
    case 'OverconstrainedError':
      return 'Выбранный микрофон больше недоступен. Выберите другое устройство.'
    case 'AbortError':
      return 'Запрос микрофона отменён.'
    default:
      return error instanceof Error && error.message
        ? error.message
        : 'Не удалось записать голос. Проверьте микрофон и разрешения браузера.'
  }
}

export function formatVoiceoverElapsed(totalSeconds: number): string {
  const safe = Math.max(0, Math.floor(totalSeconds))
  const hours = Math.floor(safe / 3600)
  const minutes = Math.floor((safe % 3600) / 60)
  const seconds = safe % 60
  const mm = String(minutes).padStart(2, '0')
  const ss = String(seconds).padStart(2, '0')
  return hours ? `${String(hours).padStart(2, '0')}:${mm}:${ss}` : `${mm}:${ss}`
}

export function browserProcessingStillEnabled(settings: MediaTrackSettings): string[] {
  return [
    ...(settings.noiseSuppression === true ? ['noise suppression'] : []),
    ...(settings.autoGainControl === true ? ['automatic gain control'] : []),
    ...(settings.echoCancellation === true ? ['echo cancellation'] : []),
  ]
}

function audioContextSupported(): boolean {
  if (typeof window === 'undefined') return false
  const audioWindow = window as typeof window & { webkitAudioContext?: typeof AudioContext }
  return Boolean(window.AudioContext ?? audioWindow.webkitAudioContext)
}

export function createVoiceoverRuntime(): VoiceoverRuntime {
  const captureRuntime = createCaptureRuntime()
  return {
    ...captureRuntime,
    isSupported: () =>
      typeof navigator !== 'undefined' &&
      typeof navigator.mediaDevices?.getUserMedia === 'function' &&
      typeof MediaRecorder !== 'undefined' &&
      audioContextSupported(),
    createGraph: createVoiceoverGraph,
  }
}

function stopStream(stream: MediaStream | null): void {
  stream?.getTracks().forEach((track) => track.stop())
}

export class VoiceoverController implements VoiceoverSession {
  private readonly runtime: VoiceoverRuntime
  private readonly onCapture?: VoiceoverCompleteHandler
  private readonly onError?: VoiceoverErrorHandler
  private snapshot = INITIAL_SNAPSHOT
  private readonly listeners = new Set<VoiceoverListener>()
  private generation = 0
  private inputStream: MediaStream | null = null
  private inputTrack: MediaStreamTrack | null = null
  private readonly inputEnded = (): void => {
    if (this.snapshot.phase === 'recording' || this.snapshot.phase === 'paused') this.stop()
    else if (this.snapshot.phase === 'countdown') this.fail(new Error('Микрофон отключён до начала записи.'))
  }
  private graph: AudioGraphPort | null = null
  private recorder: AudioRecorderPort | null = null
  private chunks: Blob[] = []
  private countdownTimer: ReturnType<typeof setTimeout> | null = null
  private countdownResolve: (() => void) | null = null
  private elapsedTimer: ReturnType<typeof setInterval> | null = null
  private meterTimer: ReturnType<typeof setInterval> | null = null
  private startedAt = 0
  private pausedAt = 0
  private pausedDuration = 0
  private disposed = false

  constructor(options: VoiceoverControllerOptions = {}) {
    this.runtime = options.runtime ?? createVoiceoverRuntime()
    this.onCapture = options.onCapture
    this.onError = options.onError
  }

  getSnapshot(): VoiceoverSnapshot {
    return {
      ...this.snapshot,
      level: { ...this.snapshot.level },
      microphones: [...this.snapshot.microphones],
    }
  }

  subscribe(listener: VoiceoverListener): () => void {
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
        microphones: devices.filter((device) => device.kind === 'audioinput'),
      })
    } catch (error) {
      if (this.disposed) return
      this.publish({ devicesLoading: false, deviceError: formatVoiceoverError(error) })
    }
  }

  async start(options: VoiceoverOptions): Promise<void> {
    if (this.disposed || ['requesting', 'countdown', 'recording', 'paused', 'stopping'].includes(this.snapshot.phase)) return
    this.releasePreview()
    this.cleanupAudio()

    if (!this.runtime.isSupported()) {
      this.publish({ permission: 'unsupported' })
      this.fail(new Error('Этот браузер не поддерживает безопасную локальную запись с Web Audio и MediaRecorder.'))
      return
    }

    const currentGeneration = ++this.generation
    this.publish({
      phase: 'requesting',
      countdownSeconds: 0,
      elapsedSeconds: 0,
      level: { rms: 0, peak: 0 },
      file: null,
      error: null,
    })

    try {
      const input = await this.runtime.getUserMedia(buildMicrophoneConstraints(options.microphoneDeviceId))
      if (!this.isCurrent(currentGeneration)) {
        stopStream(input)
        return
      }
      this.inputStream = input
      this.inputTrack = input.getAudioTracks()[0] ?? null
      if (!this.inputTrack) throw new Error('Микрофон не предоставил аудиодорожку.')
      this.inputTrack.addEventListener('ended', this.inputEnded)
      const browserProcessing = browserProcessingStillEnabled(this.inputTrack.getSettings?.() ?? {})
      if (browserProcessing.length) {
        throw new Error(`Браузер не позволил отключить встроенную обработку: ${browserProcessing.join(', ')}.`)
      }
      this.publish({ permission: 'granted' })

      try {
        const devices = await this.runtime.enumerateDevices()
        if (this.isCurrent(currentGeneration)) {
          this.publish({ microphones: devices.filter((device) => device.kind === 'audioinput'), deviceError: null })
        }
      } catch {
        // Recording can proceed even if labels cannot be refreshed after permission.
      }

      const graph = await this.runtime.createGraph(input, options.dsp)
      if (!this.isCurrent(currentGeneration)) {
        void graph.cleanup()
        this.cleanupAudio()
        return
      }
      this.graph = graph
      this.startMeter()

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
      this.cleanupAudio()
      this.publish({ phase: 'idle', countdownSeconds: 0, elapsedSeconds: 0, level: { rms: 0, peak: 0 } })
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
    this.clearMeterTimer()
    if (this.recorder) {
      this.recorder.ondataavailable = null
      this.recorder.onerror = null
      this.recorder.onstop = null
      try {
        if (this.recorder.state !== 'inactive') this.recorder.stop()
      } catch {
        // Input and graph are still released below.
      }
    }
    this.recorder = null
    this.cleanupAudio()
    this.releasePreview()
    this.listeners.clear()
  }

  private publish(patch: Partial<VoiceoverSnapshot>): void {
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
    if (!this.isCurrent(generation) || !this.graph) return
    const mimeType = MIME_TYPES.find((candidate) => this.runtime.isMimeTypeSupported(candidate))
    this.chunks = []
    this.recorder = this.runtime.createRecorder(this.graph.output, mimeType ? { mimeType } : undefined)
    this.recorder.ondataavailable = (event) => {
      if (event.data.size > 0) this.chunks.push(event.data)
    }
    this.recorder.onerror = (event) => {
      this.fail(event.error instanceof Error ? event.error : new Error('Ошибка MediaRecorder.'))
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

  private startMeter(): void {
    this.clearMeterTimer()
    const update = (): void => {
      if (!this.graph) return
      this.publish({ level: this.graph.readLevel() })
    }
    update()
    this.meterTimer = this.runtime.setInterval(update, 50)
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
    this.clearMeterTimer()
    const recorderMime = this.recorder?.mimeType || 'audio/webm'
    if (this.recorder) {
      this.recorder.ondataavailable = null
      this.recorder.onerror = null
      this.recorder.onstop = null
    }
    this.recorder = null
    const timestamp = this.runtime.now()
    const baseMime = recorderMime.split(';')[0] || 'audio/webm'
    const extension = baseMime === 'audio/ogg' ? 'ogg' : baseMime === 'audio/mp4' ? 'm4a' : 'webm'
    const blob = new Blob(this.chunks, { type: recorderMime })
    const filename = `voiceover-${new Date(timestamp).toISOString().replace(/[:.]/g, '-')}.${extension}`
    const file = new File([blob], filename, { type: baseMime, lastModified: timestamp })
    const previewUrl = this.runtime.createObjectURL(file)
    this.chunks = []
    this.cleanupAudio()
    this.publish({ phase: 'completed', countdownSeconds: 0, level: { rms: 0, peak: 0 }, file, previewUrl })

    try {
      await this.onCapture?.(file)
    } catch (error) {
      const message = `Запись создана, но не передана дальше: ${formatVoiceoverError(error)}`
      this.publish({ error: message })
      this.onError?.(message)
    }
  }

  private fail(error: unknown): void {
    const message = formatVoiceoverError(error)
    const permission = error instanceof Error && ['NotAllowedError', 'PermissionDeniedError', 'SecurityError'].includes(error.name)
      ? 'denied' as const
      : this.snapshot.permission
    this.generation += 1
    this.clearCountdown()
    this.clearElapsedTimer()
    this.clearMeterTimer()
    if (this.recorder) {
      this.recorder.ondataavailable = null
      this.recorder.onerror = null
      this.recorder.onstop = null
      try {
        if (this.recorder.state !== 'inactive') this.recorder.stop()
      } catch {
        // Graph cleanup below is sufficient after a recorder failure.
      }
    }
    this.recorder = null
    this.chunks = []
    this.cleanupAudio()
    this.publish({ phase: 'error', permission, countdownSeconds: 0, level: { rms: 0, peak: 0 }, error: message })
    this.onError?.(message)
  }

  private cleanupAudio(): void {
    this.clearMeterTimer()
    this.inputTrack?.removeEventListener('ended', this.inputEnded)
    this.inputTrack = null
    if (this.graph) {
      try {
        void this.graph.cleanup()
      } catch {
        // The raw microphone track is still stopped below.
      }
    }
    this.graph = null
    stopStream(this.inputStream)
    this.inputStream = null
  }

  private releasePreview(): void {
    if (this.snapshot.previewUrl) this.runtime.revokeObjectURL(this.snapshot.previewUrl)
    this.snapshot = { ...this.snapshot, previewUrl: null, file: null }
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

  private clearMeterTimer(): void {
    if (this.meterTimer !== null) this.runtime.clearInterval(this.meterTimer)
    this.meterTimer = null
  }
}
