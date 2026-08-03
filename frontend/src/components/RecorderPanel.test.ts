import { createApp, nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  start: vi.fn(), pause: vi.fn(), resume: vi.fn(), stop: vi.fn(), cancel: vi.fn(), retry: vi.fn(),
  capabilities: { supported: true, reason: null as string | null, screen: true, camera: true },
  state: { phase: 'idle', elapsedSeconds: 0, error: '', warnings: [] as string[], previewStream: null as MediaStream | null, savedName: '', canRetrySave: false, hasPendingCapture: false },
}))

vi.mock('../recording-store', () => ({
  recordingCapabilities: mocks.capabilities, recordingState: mocks.state,
  startRecording: mocks.start, pauseRecording: mocks.pause, resumeRecording: mocks.resume,
  stopRecording: mocks.stop, cancelRecording: mocks.cancel,
  retrySavingRecording: mocks.retry,
}))

import RecorderPanel from './RecorderPanel.vue'

describe('RecorderPanel', () => {
  let host: HTMLDivElement
  let app: ReturnType<typeof createApp> | null
  beforeEach(() => {
    host = document.createElement('div'); document.body.append(host); app = null
    Object.assign(mocks.capabilities, { supported: true, reason: null, screen: true, camera: true })
    Object.assign(mocks.state, { phase: 'idle', elapsedSeconds: 0, error: '', warnings: [], previewStream: null, savedName: '', canRetrySave: false, hasPendingCapture: false })
    vi.clearAllMocks()
  })
  afterEach(() => { app?.unmount(); host.remove() })

  async function mount() { app = createApp(RecorderPanel); app.mount(host); await nextTick() }
  const button = (name: string) => [...host.querySelectorAll<HTMLButtonElement>('button')].find(item => item.textContent?.includes(name))!

  it('does not request permissions on mount and starts only from the user action', async () => {
    await mount()
    expect(mocks.start).not.toHaveBeenCalled()
    button('Начать запись').click()
    expect(mocks.start).toHaveBeenCalledWith({
      screen: true, camera: false, microphone: true, systemAudio: true,
      cameraCorner: 'bottom-right', cameraScale: .3, fps: 30,
    })
  })

  it('requires a visual source and exposes unsupported reasons', async () => {
    mocks.capabilities.screen = false; mocks.capabilities.camera = false
    mocks.capabilities.supported = false; mocks.capabilities.reason = 'MediaRecorder отсутствует'
    await mount()
    expect(button('Начать запись').disabled).toBe(true)
    expect(host.textContent).toContain('Захват экрана недоступен')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('MediaRecorder отсутствует')
  })

  it('offers accessible pause, stop and confirmed discard controls', async () => {
    mocks.state.phase = 'recording'; mocks.state.elapsedSeconds = 65
    await mount()
    expect(host.querySelector('[role="status"]')?.textContent).toContain('01:05')
    const pause = button('Пауза'); expect(pause.getAttribute('aria-pressed')).toBe('false'); pause.click()
    button('Завершить и сохранить').click(); button('Отменить запись').click(); await nextTick()
    expect(mocks.pause).toHaveBeenCalled(); expect(mocks.stop).toHaveBeenCalled()
    expect(host.querySelector('[role="alertdialog"]')).not.toBeNull()
    expect(mocks.cancel).not.toHaveBeenCalled()
    button('Удалить запись').click(); expect(mocks.cancel).toHaveBeenCalled()
  })

  it('announces source warnings, errors and saved timeline completion', async () => {
    mocks.state.phase = 'done'; mocks.state.savedName = 'Запись 1.webm'; mocks.state.warnings = ['Системный звук не предоставлен']
    await mount()
    expect(host.textContent).toContain('Системный звук не предоставлен')
    expect(host.textContent).toContain('Запись «Запись 1.webm» сохранена')
  })

  it('stops all capture resources when the panel unmounts', async () => {
    mocks.state.phase = 'recording'
    await mount()
    app?.unmount(); app = null
    expect(mocks.cancel).toHaveBeenCalledOnce()
  })

  it('keeps Start disabled and permits confirmed discard after permanent save failure', async () => {
    Object.assign(mocks.state, { phase: 'error', error: 'Нет места', canRetrySave: true, hasPendingCapture: true })
    await mount()
    expect(button('Начать запись').disabled).toBe(true)
    button('Удалить несохранённую').click(); await nextTick()
    expect(host.querySelector('[role="alertdialog"]')).not.toBeNull()
    button('Удалить запись').click()
    expect(mocks.cancel).toHaveBeenCalledOnce()
  })
})
