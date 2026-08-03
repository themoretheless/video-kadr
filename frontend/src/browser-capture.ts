import { cameraPipRect, CaptureSessionError, containRect, type ValidatedCaptureOptions } from './domain/capture-session'
import { BrowserCaptureSink } from './browser-capture-sink'
export { CaptureSessionError as BrowserCaptureError } from './domain/capture-session'

export type BrowserCaptureMode = 'screen' | 'camera' | 'screen_camera'
export type CameraCorner = 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right'
export interface BrowserCaptureOptions {
  mode: BrowserCaptureMode
  microphone: boolean
  systemAudio: boolean
  fps?: number
  cameraCorner?: CameraCorner
  cameraScale?: number
}
export interface BrowserCaptureResult {
  file: File
  mimeType: string
  durationMs: number
  durationSeconds: number
  width: number
  height: number
  hasAudio: boolean
  stoppedBy: 'user' | 'display_ended'
  warnings: string[]
}
export interface CaptureOptions { screen: boolean; camera: boolean; microphone: boolean; systemAudio: boolean; cameraCorner: 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right'; cameraScale: number; fps: 15 | 24 | 30 }
export type CaptureResult = BrowserCaptureResult

const MAX_CAPTURE_BYTES = 512 * 1024 * 1024
const MIME_CANDIDATES = ['video/webm;codecs=vp9,opus', 'video/webm;codecs=vp8,opus', 'video/webm']

export interface BrowserCaptureCapabilities { supported: boolean; reason: string | null; mimeType: string | null; screen: boolean; camera: boolean }
export function browserCaptureCapabilities(): BrowserCaptureCapabilities {
  const screen = Boolean(typeof navigator !== 'undefined' && navigator.mediaDevices?.getDisplayMedia)
  const camera = Boolean(typeof navigator !== 'undefined' && navigator.mediaDevices?.getUserMedia)
  if (typeof window === 'undefined' || !window.isSecureContext) return { supported: false, reason: 'Запись доступна только в безопасном контексте HTTPS', mimeType: null, screen, camera }
  if ((!screen && !camera) || typeof MediaRecorder === 'undefined') return { supported: false, reason: 'Браузер не поддерживает запись экрана или камеры', mimeType: null, screen, camera }
  const mimeType = MIME_CANDIDATES.find(type => MediaRecorder.isTypeSupported(type)) ?? null
  return mimeType ? { supported: true, reason: null, mimeType, screen, camera } : { supported: false, reason: 'Нет поддерживаемого WebM MediaRecorder', mimeType: null, screen, camera }
}
export const captureCapabilities = browserCaptureCapabilities

function permissionError(error: unknown, requested: 'display' | 'camera' | 'microphone'): CaptureSessionError {
  const name = error instanceof DOMException ? error.name : error instanceof Error ? error.name : ''
  if (name === 'NotFoundError' || name === 'OverconstrainedError') return new CaptureSessionError('device_ended', 'Запрошенное устройство не найдено', { cause: error })
  const code = name === 'AbortError' ? 'permission_cancelled' : 'permission_denied'
  return new CaptureSessionError(code, requested === 'display' ? 'Доступ к экрану не разрешён' : requested === 'camera' ? 'Доступ к камере не разрешён' : 'Доступ к микрофону не разрешён', { cause: error })
}

function stopStream(stream: MediaStream | null | undefined): void { stream?.getTracks().forEach(track => track.stop()) }

export class BrowserCaptureSession {
  readonly completion: Promise<BrowserCaptureResult>
  private completeResolve!: (result: BrowserCaptureResult) => void
  private completeReject!: (error: unknown) => void
  private display: MediaStream | null = null
  private camera: MediaStream | null = null
  private microphone: MediaStream | null = null
  private output: MediaStream | null = null
  private recorder: MediaRecorder | null = null
  private audioContext: AudioContext | null = null
  private animation = 0
  private sink: BrowserCaptureSink | null = null
  private bytes = 0
  private startedAt = 0
  private stopping: Promise<BrowserCaptureResult> | null = null
  private disposed = false
  private width = 0
  private height = 0
  private mimeType = ''
  private displayEnded?: () => void
  private cameraEnded?: () => void
  private microphoneEnded?: () => void
  private maxBytes = MAX_CAPTURE_BYTES
  private maxDurationMs = 30 * 60 * 1000
  private timesliceMs = 1000
  private limitReached = false
  private recorderFailed = false
  private sinkFailure: CaptureSessionError | null = null
  private durationTimer = 0
  private pausedAt = 0
  private pausedDurationMs = 0
  private stoppedBy: 'user' | 'display_ended' = 'user'
  readonly warnings: string[] = []
  onWarning: ((warning: string) => void) | null = null
  state: 'recording' | 'paused' | 'stopping' | 'stopped' = 'recording'
  get previewStream(): MediaStream | null { return this.output }

  constructor() {
    this.completion = new Promise((resolve, reject) => { this.completeResolve = resolve; this.completeReject = reject })
  }

  static async start(options: BrowserCaptureOptions & { maxBytes?: number; maxDurationMs?: number; timesliceMs?: number }): Promise<BrowserCaptureSession> {
    const capabilities = browserCaptureCapabilities()
    if (!capabilities.supported || !capabilities.mimeType) throw new CaptureSessionError('unsupported', capabilities.reason ?? 'Запись не поддерживается')
    if (options.mode !== 'camera' && !capabilities.screen) throw new CaptureSessionError('unsupported', 'Захват экрана недоступен')
    if ((options.mode !== 'screen' || options.microphone) && !capabilities.camera) throw new CaptureSessionError('unsupported', 'Камера или микрофон недоступны')
    const session = new BrowserCaptureSession()
    session.maxBytes = options.maxBytes ?? session.maxBytes
    session.maxDurationMs = options.maxDurationMs ?? session.maxDurationMs
    session.timesliceMs = options.timesliceMs ?? session.timesliceMs
    session.mimeType = capabilities.mimeType
    try { await session.acquire(options); return session } catch (error) { await session.sink?.abort(); session.cleanup(); throw error }
  }

  private async acquire(options: BrowserCaptureOptions): Promise<void> {
    const screen = options.mode !== 'camera'
    const camera = options.mode !== 'screen'
    if (screen) {
      try { this.display = await navigator.mediaDevices.getDisplayMedia({ video: true, audio: options.systemAudio }) }
      catch (error) { throw permissionError(error, 'display') }
    }
    if (camera) {
      try { this.camera = await navigator.mediaDevices.getUserMedia({ video: true, audio: false }) }
      catch (error) { throw permissionError(error, 'camera') }
    }
    if (options.microphone) {
      try { this.microphone = await navigator.mediaDevices.getUserMedia({ video: false, audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true } }) }
      catch (error) { throw permissionError(error, 'microphone') }
    }
    const primary = (this.display ?? this.camera)!.getVideoTracks()[0]
    if (!primary) throw new CaptureSessionError('device_ended', 'Источник не содержит видеодорожку')
    const settings = primary.getSettings()
    this.width = Math.max(1, settings.width ?? 1280); this.height = Math.max(1, settings.height ?? 720)
    const fps = Math.max(1, Math.min(60, options.fps ?? settings.frameRate ?? 30))
    const canvas = document.createElement('canvas'); canvas.width = this.width; canvas.height = this.height
    const context = canvas.getContext('2d'); if (!context) throw new CaptureSessionError('unsupported', 'Canvas 2D недоступен')
    const screenVideo = this.display ? await this.video(this.display) : null
    const cameraVideo = this.camera?.getVideoTracks().length ? await this.video(this.camera) : null
    const draw = () => {
      if (this.disposed) return
      const base = screenVideo ?? cameraVideo
      if (base) {
        context.fillStyle = '#000'; context.fillRect(0, 0, this.width, this.height)
        const source = { width: base.videoWidth || this.width, height: base.videoHeight || this.height }
        const rect = containRect(source, { width: this.width, height: this.height })
        context.drawImage(base, rect.x, rect.y, rect.width, rect.height)
      }
      if (screenVideo && cameraVideo) {
        const rect = cameraPipRect(
          { width: this.width, height: this.height },
          { width: cameraVideo.videoWidth || 16, height: cameraVideo.videoHeight || 9 },
          (options.cameraCorner ?? 'bottom-right').replace('-', '_') as 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right',
          options.cameraScale ?? .25,
        )
        context.drawImage(cameraVideo, rect.x, rect.y, rect.width, rect.height)
      }
      this.animation = requestAnimationFrame(draw)
    }
    draw()
    const canvasStream = canvas.captureStream(fps)
    const tracks = [...canvasStream.getVideoTracks()]
    const audioTracks = [...(options.systemAudio ? this.display?.getAudioTracks() ?? [] : []), ...(options.microphone ? this.microphone?.getAudioTracks() ?? [] : [])]
    if (audioTracks.length) {
      this.audioContext = new AudioContext()
      if (this.audioContext.state === 'suspended') await this.audioContext.resume()
      if (this.audioContext.state !== 'running') throw new CaptureSessionError('recorder_failed', 'Аудиоконтекст не запущен; запись звука невозможна')
      const destination = this.audioContext.createMediaStreamDestination()
      const limiter = this.audioContext.createDynamicsCompressor()
      limiter.threshold.value = -6; limiter.knee.value = 6; limiter.ratio.value = 12
      limiter.attack.value = .003; limiter.release.value = .25; limiter.connect(destination)
      for (const track of audioTracks) this.audioContext.createMediaStreamSource(new MediaStream([track])).connect(limiter)
      tracks.push(...destination.stream.getAudioTracks())
    }
    this.sink = await BrowserCaptureSink.create()
    this.sink.onFailure = error => { this.sinkFailure = error; void this.stop().catch(() => undefined) }
    this.output = new MediaStream(tracks)
    this.recorder = new MediaRecorder(this.output, { mimeType: this.mimeType })
    this.recorder.ondataavailable = event => {
      if (!event.data.size || this.disposed) return
      this.bytes += event.data.size
      if (this.bytes > this.maxBytes) { this.limitReached = true; void this.stop().catch(() => undefined); return }
      this.sink?.append(event.data)
    }
    this.recorder.onerror = () => { this.recorderFailed = true; void this.stop().catch(() => undefined) }
    const displayTrack = this.display?.getVideoTracks()[0]
    const cameraTrack = this.camera?.getVideoTracks()[0]
    const microphoneTrack = this.microphone?.getAudioTracks()[0]
    if (options.systemAudio && !this.display?.getAudioTracks().length) this.warnings.push('system_audio_unavailable')
    if (displayTrack) { this.displayEnded = () => { this.stoppedBy = 'display_ended'; void this.stop().catch(() => undefined) }; displayTrack.addEventListener('ended', this.displayEnded, { once: true }) }
    if (cameraTrack) {
      this.cameraEnded = () => {
        if (!this.display) { void this.stop().catch(() => undefined); return }
        this.warnings.push('camera_unavailable'); this.onWarning?.('camera_unavailable')
      }
      cameraTrack.addEventListener('ended', this.cameraEnded, { once: true })
    }
    if (microphoneTrack) {
      this.microphoneEnded = () => { this.warnings.push('microphone_unavailable'); this.onWarning?.('microphone_unavailable') }
      microphoneTrack.addEventListener('ended', this.microphoneEnded, { once: true })
    }
    this.startedAt = performance.now(); this.recorder.start(this.timesliceMs)
    this.durationTimer = window.setTimeout(() => { this.limitReached = true; void this.stop().catch(() => undefined) }, this.maxDurationMs)
  }

  private video(stream: MediaStream): Promise<HTMLVideoElement> {
    const video = document.createElement('video'); video.muted = true; video.playsInline = true; video.srcObject = stream
    return video.play().then(() => video)
  }

  pause(): void { if (this.recorder?.state === 'recording') { this.recorder.pause(); this.pausedAt = performance.now(); this.state = 'paused' } }
  resume(): void { if (this.recorder?.state === 'paused') { this.pausedDurationMs += Math.max(0, performance.now() - this.pausedAt); this.pausedAt = 0; this.recorder.resume(); this.state = 'recording' } }
  async releaseStagedFile(): Promise<void> { await this.sink?.removeStage() }

  stop(): Promise<CaptureResult> {
    if (this.stopping) return this.stopping
    this.state = 'stopping'
    const recorder = this.recorder
    if (!recorder || recorder.state === 'inactive') {
      const error = new CaptureSessionError('recorder_failed', 'Запись уже остановлена')
      this.cleanup(); this.completeReject(error)
      return Promise.reject(error)
    }
    let resolveStop!: (result: CaptureResult) => void
    let rejectStop!: (error: unknown) => void
    this.stopping = new Promise<CaptureResult>((resolve, reject) => { resolveStop = resolve; rejectStop = reject })
    recorder.onstop = async () => {
        const openPause = this.pausedAt ? Math.max(0, performance.now() - this.pausedAt) : 0
        const durationMs = Math.max(0, performance.now() - this.startedAt - this.pausedDurationMs - openPause)
        const tooLarge = this.limitReached || this.bytes > this.maxBytes
        const hasAudio = Boolean(this.output?.getAudioTracks().length)
        const stamp = new Date().toISOString().replace(/[:.]/g, '-')
        try {
          if (this.recorderFailed) throw new CaptureSessionError('recorder_failed', 'MediaRecorder завершился с ошибкой')
          if (this.sinkFailure) throw this.sinkFailure
          if (tooLarge) throw new CaptureSessionError('limit_reached', 'Запись превысила установленный лимит')
          const file = await this.sink!.finish(`recording-${stamp}.webm`, this.mimeType)
          if (!file.size) throw new CaptureSessionError('empty_recording', 'Запись не содержит данных')
          this.cleanup(); this.state = 'stopped'
          const result = { file, mimeType: this.mimeType, durationMs, durationSeconds: durationMs / 1000, width: this.width, height: this.height, hasAudio, stoppedBy: this.stoppedBy, warnings: [...this.warnings] }
          this.completeResolve(result); resolveStop(result)
        } catch (error) { await this.sink?.abort(); this.cleanup(); this.completeReject(error); rejectStop(error) }
    }
    recorder.stop()
    return this.stopping
  }

  cancel(): void {
    const error = new CaptureSessionError('permission_cancelled', 'Запись отменена')
    void this.completion.catch(() => undefined)
    this.completeReject(error)
    if (this.recorder && this.recorder.state !== 'inactive') { this.recorder.onstop = null; this.recorder.stop() }
    void this.sink?.abort(); this.cleanup()
  }

  private cleanup(): void {
    if (this.disposed) return
    this.disposed = true
    if (this.animation) cancelAnimationFrame(this.animation)
    if (this.durationTimer) clearTimeout(this.durationTimer)
    const displayTrack = this.display?.getVideoTracks()[0]
    const cameraTrack = this.camera?.getVideoTracks()[0]
    const microphoneTrack = this.microphone?.getAudioTracks()[0]
    if (displayTrack && this.displayEnded) displayTrack.removeEventListener('ended', this.displayEnded)
    if (cameraTrack && this.cameraEnded) cameraTrack.removeEventListener('ended', this.cameraEnded)
    if (microphoneTrack && this.microphoneEnded) microphoneTrack.removeEventListener('ended', this.microphoneEnded)
    stopStream(this.output); stopStream(this.display); stopStream(this.camera); stopStream(this.microphone)
    void this.audioContext?.close().catch(() => undefined)
    this.display = this.camera = this.microphone = this.output = null; this.audioContext = null
  }
}

export async function startBrowserCapture(options: ValidatedCaptureOptions): Promise<BrowserCaptureSession> {
  if (!options.screen && !options.camera) throw new CaptureSessionError('invalid_request', 'Выберите экран или камеру')
  const mode: BrowserCaptureMode = options.screen && options.camera ? 'screen_camera' : options.screen ? 'screen' : 'camera'
  return BrowserCaptureSession.start({
    mode, microphone: options.microphone, systemAudio: options.systemAudio, fps: options.fps,
    cameraScale: options.composition.cameraScale, cameraCorner: options.composition.cameraCorner.replace('_', '-') as CameraCorner,
    maxBytes: options.maxBytes, maxDurationMs: options.maxDurationMs, timesliceMs: options.chunkTimesliceMs,
  })
}
