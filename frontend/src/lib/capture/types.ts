import type { AnnotationReader } from './annotations.js'

export type CapturePhase =
  | 'idle'
  | 'requesting'
  | 'countdown'
  | 'recording'
  | 'paused'
  | 'stopping'
  | 'completed'
  | 'error'

export type DisplaySourcePreference = 'monitor' | 'window' | 'browser'

export interface CaptureOptions {
  displaySurface: DisplaySourcePreference
  includeSystemAudio: boolean
  includeMicrophone: boolean
  microphoneDeviceId?: string
  includeWebcam: boolean
  webcamDeviceId?: string
  countdownSeconds: number
}

export interface CaptureDevices {
  microphones: MediaDeviceInfo[]
  cameras: MediaDeviceInfo[]
}

export interface CaptureSnapshot {
  phase: CapturePhase
  countdownSeconds: number
  elapsedSeconds: number
  devices: CaptureDevices
  devicesLoading: boolean
  deviceError: string | null
  hasSystemAudio: boolean | null
  file: File | null
  previewUrl: string | null
  previewCanvas: HTMLCanvasElement | null
  error: string | null
}

export type CaptureCompleteHandler = (file: File) => void | Promise<void>
export type CaptureErrorHandler = (message: string) => void
export type CaptureListener = (snapshot: CaptureSnapshot) => void

export interface CaptureInputStreams {
  display: MediaStream
  user: MediaStream | null
  annotations: AnnotationReader
}

export interface PreparedCaptureStream {
  stream: MediaStream
  previewCanvas: HTMLCanvasElement
  cleanup(): void | Promise<void>
}

export interface RecorderPort {
  readonly mimeType: string
  readonly state: RecordingState
  ondataavailable: ((event: BlobEvent) => void) | null
  onerror: ((event: ErrorEvent) => void) | null
  onstop: ((event: Event) => void) | null
  start(timeslice?: number): void
  pause(): void
  resume(): void
  stop(): void
}

export interface CaptureRuntime {
  isSupported(): boolean
  getDisplayMedia(options: DisplayMediaStreamOptions): Promise<MediaStream>
  getUserMedia(options: MediaStreamConstraints): Promise<MediaStream>
  enumerateDevices(): Promise<MediaDeviceInfo[]>
  prepareStream(inputs: CaptureInputStreams): Promise<PreparedCaptureStream>
  createRecorder(stream: MediaStream, options?: MediaRecorderOptions): RecorderPort
  isMimeTypeSupported(mimeType: string): boolean
  createObjectURL(blob: Blob): string
  revokeObjectURL(url: string): void
  now(): number
  setTimeout(callback: () => void, delayMs: number): ReturnType<typeof setTimeout>
  clearTimeout(timer: ReturnType<typeof setTimeout>): void
  setInterval(callback: () => void, delayMs: number): ReturnType<typeof setInterval>
  clearInterval(timer: ReturnType<typeof setInterval>): void
}

export interface CaptureSession {
  getSnapshot(): CaptureSnapshot
  subscribe(listener: CaptureListener): () => void
  loadDevices(): Promise<void>
  start(options: CaptureOptions): Promise<void>
  pause(): void
  resume(): void
  stop(): void
  dispose(): void
}

export interface ExtendedDisplayMediaOptions extends DisplayMediaStreamOptions {
  preferCurrentTab?: boolean
  selfBrowserSurface?: 'include' | 'exclude'
  surfaceSwitching?: 'include' | 'exclude'
  systemAudio?: 'include' | 'exclude'
}
