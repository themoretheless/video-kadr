import { reactive } from 'vue'
import { browserCaptureCapabilities, BrowserCaptureSession, startBrowserCapture } from './browser-capture'
import { validateCaptureRequest, type CaptureFps } from './domain/capture-session'
import { doUpload, state } from './store'

export type RecordingPhase = 'idle' | 'requesting' | 'recording' | 'paused' | 'stopping' | 'saving' | 'done' | 'error'

export interface RecordingOptions {
  screen: boolean
  camera: boolean
  microphone: boolean
  systemAudio: boolean
  cameraCorner: 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right'
  cameraScale: number
  fps: number
}

const rawCapabilities = browserCaptureCapabilities()
const media = typeof navigator === 'undefined' ? undefined : navigator.mediaDevices
export const recordingCapabilities = reactive({
  supported: rawCapabilities.supported,
  reason: rawCapabilities.reason,
  screen: rawCapabilities.screen ?? Boolean(media?.getDisplayMedia),
  camera: rawCapabilities.camera ?? Boolean(media?.getUserMedia),
})

export const recordingState = reactive({
  phase: 'idle' as RecordingPhase,
  elapsedSeconds: 0,
  error: '',
  warnings: [] as string[],
  previewStream: null as MediaStream | null,
  savedName: '',
  canRetrySave: false,
  hasPendingCapture: false,
})

let session: BrowserCaptureSession | null = null
let generation = 0
let timer = 0
let recordedSeconds = 0
let runningSince = 0
let pendingCapture: { active: BrowserCaptureSession; result: Awaited<ReturnType<BrowserCaptureSession['stop']>>; generation: number } | null = null

function stopTimer(): void {
  if (timer) window.clearInterval(timer)
  timer = 0
}

function updateElapsed(): void {
  const running = recordingState.phase === 'recording' ? (performance.now() - runningSince) / 1000 : 0
  recordingState.elapsedSeconds = recordedSeconds + running
}

function beginTimer(): void {
  runningSince = performance.now()
  stopTimer()
  timer = window.setInterval(updateElapsed, 250)
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function captureWarning(value: string): string {
  if (value === 'system_audio_unavailable') return 'Браузер не предоставил системный звук; запись продолжается.'
  if (value === 'camera_unavailable') return 'Камера отключена; запись экрана продолжается.'
  if (value === 'microphone_unavailable') return 'Микрофон отключён; запись видео продолжается.'
  return value
}

async function waitForImportIdle(): Promise<void> {
  const deadline = performance.now() + 60_000
  while (state.importing) {
    if (performance.now() >= deadline) throw new Error('Другой импорт не завершился; повторите сохранение записи')
    await new Promise(resolve => window.setTimeout(resolve, 100))
  }
}

async function saveCapture(active: BrowserCaptureSession, result: Awaited<ReturnType<BrowserCaptureSession['stop']>>, current: number): Promise<void> {
  if (current !== generation) return
  pendingCapture = { active, result, generation: current }
  recordingState.hasPendingCapture = true
  recordingState.previewStream = null
  recordingState.warnings = result.warnings.map(captureWarning)
  recordingState.phase = 'saving'
  recordingState.canRetrySave = false
  try {
    await waitForImportIdle()
    const accepted = await doUpload(result.file)
    if (!accepted || state.importError) throw new Error(state.importError || 'Запись не была добавлена в медиатеку')
    await active.releaseStagedFile().catch(() => undefined)
    if (current !== generation) return
    pendingCapture = null; session = null; recordingState.hasPendingCapture = false
    recordingState.savedName = result.file.name
    recordingState.phase = 'done'
  } catch (error) {
    if (current !== generation) return
    recordingState.error = message(error)
    recordingState.canRetrySave = true
    recordingState.phase = 'error'
  }
}

export async function startRecording(options: RecordingOptions): Promise<void> {
  if (session || ['requesting', 'recording', 'paused', 'stopping', 'saving'].includes(recordingState.phase)) return
  const current = ++generation
  recordingState.phase = 'requesting'
  recordingState.error = ''
  recordingState.warnings = []
  recordingState.savedName = ''
  recordingState.canRetrySave = false
  recordingState.hasPendingCapture = false
  recordingState.elapsedSeconds = 0
  recordedSeconds = 0
  try {
    const composition = options.screen && options.camera ? 'screen_with_camera' : options.screen ? 'screen_only' : 'camera_only'
    const request = validateCaptureRequest({
      screen: options.screen,
      camera: options.camera,
      microphone: options.microphone,
      systemAudio: options.systemAudio,
      composition: {
        mode: composition,
        cameraCorner: options.cameraCorner.replace('-', '_') as 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right',
        cameraScale: options.cameraScale,
      },
      fps: options.fps as CaptureFps,
      insertAt: 'timeline_end',
    })
    const active = await startBrowserCapture(request)
    if (current !== generation) { active.cancel(); return }
    session = active
    session.onWarning = warning => { recordingState.warnings = [...recordingState.warnings, captureWarning(warning)] }
    void active.completion.then(
      result => saveCapture(active, result, current),
      error => {
        if (current !== generation) return
        stopTimer(); recordingState.previewStream = null; recordingState.error = message(error)
        recordingState.phase = 'error'; session = null
      },
    )
    recordingState.previewStream = active.previewStream
    recordingState.warnings = active.warnings.map(captureWarning)
    recordingState.phase = 'recording'
    beginTimer()
  } catch (error) {
    if (current !== generation) return
    recordingState.error = message(error)
    recordingState.phase = 'error'
    recordingState.previewStream = null
    stopTimer()
  }
}

export function pauseRecording(): void {
  if (!session || recordingState.phase !== 'recording') return
  updateElapsed()
  recordedSeconds = recordingState.elapsedSeconds
  stopTimer()
  session.pause()
  recordingState.phase = 'paused'
}

export function resumeRecording(): void {
  if (!session || recordingState.phase !== 'paused') return
  session.resume()
  recordingState.phase = 'recording'
  beginTimer()
}

export async function stopRecording(): Promise<void> {
  const active = session
  if (!active || !['recording', 'paused'].includes(recordingState.phase)) return
  if (recordingState.phase === 'recording') updateElapsed()
  stopTimer()
  recordingState.phase = 'stopping'
  void active.stop().catch(() => undefined)
}

export async function retrySavingRecording(): Promise<void> {
  if (!pendingCapture || !recordingState.canRetrySave) return
  recordingState.error = ''
  await saveCapture(pendingCapture.active, pendingCapture.result, pendingCapture.generation)
}

export async function cancelRecording(): Promise<void> {
  generation++
  stopTimer()
  const active = session
  session = null
  active?.cancel()
  pendingCapture = null
  recordingState.hasPendingCapture = false
  recordingState.previewStream = null
  recordingState.elapsedSeconds = 0
  recordedSeconds = 0
  recordingState.error = ''
  recordingState.warnings = []
  recordingState.canRetrySave = false
  recordingState.phase = 'idle'
}
