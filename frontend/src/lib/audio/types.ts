export type VoiceoverPhase =
  | 'idle'
  | 'requesting'
  | 'countdown'
  | 'recording'
  | 'paused'
  | 'stopping'
  | 'completed'
  | 'error'

export type MicrophonePermission = 'prompt' | 'granted' | 'denied' | 'unsupported'

export interface VoiceoverDspOptions {
  inputGainDb: number
  highPassEnabled: boolean
  highPassHz: number
  compressorEnabled: boolean
  limiterEnabled: boolean
}

export interface VoiceoverOptions {
  microphoneDeviceId?: string
  countdownSeconds: number
  dsp: VoiceoverDspOptions
}

export interface AudioLevel {
  rms: number
  peak: number
}

export interface VoiceoverSnapshot {
  phase: VoiceoverPhase
  permission: MicrophonePermission
  countdownSeconds: number
  elapsedSeconds: number
  level: AudioLevel
  microphones: MediaDeviceInfo[]
  devicesLoading: boolean
  deviceError: string | null
  file: File | null
  previewUrl: string | null
  error: string | null
}

export type VoiceoverCompleteHandler = (file: File) => void | Promise<void>
export type VoiceoverErrorHandler = (message: string) => void
export type VoiceoverListener = (snapshot: VoiceoverSnapshot) => void

export interface AudioGraphPort {
  readonly output: MediaStream
  readLevel(): AudioLevel
  cleanup(): void | Promise<void>
}

export interface AudioRecorderPort {
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

export interface VoiceoverRuntime {
  isSupported(): boolean
  getUserMedia(options: MediaStreamConstraints): Promise<MediaStream>
  enumerateDevices(): Promise<MediaDeviceInfo[]>
  createGraph(input: MediaStream, options: VoiceoverDspOptions): Promise<AudioGraphPort>
  createRecorder(stream: MediaStream, options?: MediaRecorderOptions): AudioRecorderPort
  isMimeTypeSupported(mimeType: string): boolean
  createObjectURL(blob: Blob): string
  revokeObjectURL(url: string): void
  now(): number
  setTimeout(callback: () => void, delayMs: number): ReturnType<typeof setTimeout>
  clearTimeout(timer: ReturnType<typeof setTimeout>): void
  setInterval(callback: () => void, delayMs: number): ReturnType<typeof setInterval>
  clearInterval(timer: ReturnType<typeof setInterval>): void
}

export interface VoiceoverSession {
  getSnapshot(): VoiceoverSnapshot
  subscribe(listener: VoiceoverListener): () => void
  loadDevices(): Promise<void>
  start(options: VoiceoverOptions): Promise<void>
  pause(): void
  resume(): void
  stop(): void
  dispose(): void
}
