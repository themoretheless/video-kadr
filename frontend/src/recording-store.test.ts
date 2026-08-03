import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  start: vi.fn(), upload: vi.fn(),
  importState: { importError: '', importing: false },
}))

vi.mock('./browser-capture', () => ({
  BrowserCaptureSession: class {},
  browserCaptureCapabilities: () => ({ supported: true, reason: null, mimeType: 'video/webm' }),
  startBrowserCapture: mocks.start,
}))
vi.mock('./store', () => ({ doUpload: mocks.upload, state: mocks.importState }))

describe('recording store', () => {
  beforeEach(() => {
    vi.resetModules()
    vi.clearAllMocks()
    mocks.importState.importError = ''
    mocks.importState.importing = false
    mocks.upload.mockResolvedValue(true)
    Object.defineProperty(navigator, 'mediaDevices', { configurable: true, value: { getDisplayMedia: vi.fn(), getUserMedia: vi.fn() } })
  })

  async function setup() {
    const result = { file: new File(['webm'], 'recording.webm', { type: 'video/webm' }), warnings: [], durationSeconds: 1 }
    let complete!: (value: typeof result) => void
    const completion = new Promise<typeof result>(resolve => { complete = resolve })
    const session = { previewStream: new MediaStream(), warnings: [], state: 'recording', onWarning: null as ((warning: string) => void) | null, completion,
      pause: vi.fn(), resume: vi.fn(), stop: vi.fn(async () => { complete(result); return result }), cancel: vi.fn(), releaseStagedFile: vi.fn(async () => undefined) }
    mocks.start.mockResolvedValue(session)
    const store = await import('./recording-store')
    return { store, session }
  }

  it('validates, captures, persists and inserts through the existing upload pipeline', async () => {
    const { store, session } = await setup()
    await store.startRecording({ screen: true, camera: false, microphone: true, systemAudio: true, cameraCorner: 'bottom-right', cameraScale: .3, fps: 30 })
    expect(mocks.start).toHaveBeenCalledWith(expect.objectContaining({ contract: 'capture-session-v1', composition: expect.objectContaining({ mode: 'screen_only' }) }))
    expect(store.recordingState.phase).toBe('recording')
    await store.stopRecording()
    await vi.waitFor(() => expect(store.recordingState.phase).toBe('done'))
    expect(session.stop).toHaveBeenCalledOnce()
    expect(mocks.upload).toHaveBeenCalledWith(expect.objectContaining({ name: 'recording.webm' }))
    expect(store.recordingState).toMatchObject({ phase: 'done', savedName: 'recording.webm' })
  })

  it('pauses, resumes and uses the same stop path when browser sharing ends', async () => {
    const { store, session } = await setup()
    await store.startRecording({ screen: true, camera: true, microphone: false, systemAudio: false, cameraCorner: 'top-left', cameraScale: .2, fps: 15 })
    store.pauseRecording(); expect(session.pause).toHaveBeenCalledOnce(); expect(store.recordingState.phase).toBe('paused')
    store.resumeRecording(); expect(session.resume).toHaveBeenCalledOnce()
    await session.stop()
    await vi.waitFor(() => expect(store.recordingState.phase).toBe('done'))
    expect(session.stop).toHaveBeenCalledOnce()
  })

  it('cancels exactly once and fences a late permission result', async () => {
    const { store, session } = await setup()
    let resolve!: (value: typeof session) => void
    mocks.start.mockReturnValueOnce(new Promise(value => { resolve = value }))
    const pending = store.startRecording({ screen: false, camera: true, microphone: true, systemAudio: false, cameraCorner: 'bottom-right', cameraScale: .3, fps: 30 })
    await store.cancelRecording(); resolve(session); await pending
    expect(session.cancel).toHaveBeenCalledOnce()
    expect(store.recordingState.phase).toBe('idle')
  })

  it('retains a failed save for retry and discards it explicitly back to idle', async () => {
    const { store, session } = await setup()
    mocks.upload.mockResolvedValueOnce(false)
    await store.startRecording({ screen: true, camera: false, microphone: false, systemAudio: false, cameraCorner: 'bottom-right', cameraScale: .3, fps: 30 })
    await store.stopRecording()
    await vi.waitFor(() => expect(store.recordingState.phase).toBe('error'))
    expect(store.recordingState).toMatchObject({ canRetrySave: true, hasPendingCapture: true })
    await store.cancelRecording()
    expect(session.cancel).toHaveBeenCalledOnce()
    expect(store.recordingState).toMatchObject({ phase: 'idle', canRetrySave: false, hasPendingCapture: false })
  })
})
