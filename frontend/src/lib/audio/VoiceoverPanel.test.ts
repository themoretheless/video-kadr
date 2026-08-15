// @ts-expect-error Vitest's default SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { afterEach, describe, expect, it, vi } from 'vitest'
import VoiceoverPanel from './VoiceoverPanel.svelte'
import type { AudioGraphPort, AudioRecorderPort, VoiceoverRuntime } from './types.js'

class PanelTrack {
  stopped = false
  private readonly listeners = new Set<EventListenerOrEventListenerObject>()
  stop(): void { this.stopped = true }
  addEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    if (type === 'ended') this.listeners.add(listener)
  }
  removeEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    if (type === 'ended') this.listeners.delete(listener)
  }
}

function panelStream(track: PanelTrack): MediaStream {
  return {
    getAudioTracks: () => [track],
    getTracks: () => [track],
  } as unknown as MediaStream
}

class PanelRecorder implements AudioRecorderPort {
  readonly mimeType = 'audio/webm'
  state: RecordingState = 'inactive'
  ondataavailable: ((event: BlobEvent) => void) | null = null
  onerror: ((event: ErrorEvent) => void) | null = null
  onstop: ((event: Event) => void) | null = null
  start(): void { this.state = 'recording' }
  pause(): void { this.state = 'paused' }
  resume(): void { this.state = 'recording' }
  stop(): void {
    this.state = 'inactive'
    this.ondataavailable?.({ data: new Blob(['voiceover']) } as BlobEvent)
    this.onstop?.(new Event('stop'))
  }
}

function button(root: HTMLElement, text: string): HTMLButtonElement {
  const found = [...root.querySelectorAll('button')].find((candidate) => candidate.textContent?.trim() === text)
  if (!(found instanceof HTMLButtonElement)) throw new Error(`Button not found: ${text}`)
  return found
}

async function settle(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
  await Promise.resolve()
  await tick()
}

afterEach(() => {
  vi.restoreAllMocks()
  document.body.innerHTML = ''
})

describe('VoiceoverPanel', () => {
  it('selects a microphone, meters DSP output, controls recording, and emits a File', async () => {
    const inputTrack = new PanelTrack()
    const outputTrack = new PanelTrack()
    const input = panelStream(inputTrack)
    const output = panelStream(outputTrack)
    const recorder = new PanelRecorder()
    const graphCleanup = vi.fn()
    const graph: AudioGraphPort = {
      output,
      readLevel: () => ({ rms: 0.3, peak: 0.7 }),
      cleanup: graphCleanup,
    }
    const createGraph = vi.fn(async () => graph)
    const revokeObjectURL = vi.fn()
    const microphone = {
      deviceId: 'mic-usb',
      groupId: 'group-1',
      kind: 'audioinput',
      label: 'USB Mic',
      toJSON: () => ({}),
    } as MediaDeviceInfo
    const runtime: VoiceoverRuntime = {
      isSupported: () => true,
      getUserMedia: vi.fn(async () => input),
      enumerateDevices: vi.fn(async () => [microphone]),
      createGraph,
      createRecorder: vi.fn(() => recorder),
      isMimeTypeSupported: () => true,
      createObjectURL: vi.fn(() => 'blob:voiceover-panel'),
      revokeObjectURL,
      now: () => Date.now(),
      setTimeout: (callback, delay) => setTimeout(callback, delay),
      clearTimeout: (timer) => clearTimeout(timer),
      setInterval: (callback, delay) => setInterval(callback, delay),
      clearInterval: (timer) => clearInterval(timer),
    }
    const oncapture = vi.fn()
    const target = document.createElement('div')
    document.body.append(target)
    const voiceoverEvents: File[] = []
    const captureEvents: File[] = []
    target.addEventListener('voiceover', (event) => voiceoverEvents.push((event as CustomEvent<File>).detail))
    target.addEventListener('capture', (event) => captureEvents.push((event as CustomEvent<File>).detail))
    const component = mount(VoiceoverPanel, { target, props: { runtime, countdownSeconds: 0, oncapture } })
    await settle()

    const select = target.querySelector('select')
    if (!(select instanceof HTMLSelectElement)) throw new Error('Microphone select not found')
    select.value = 'mic-usb'
    select.dispatchEvent(new Event('change', { bubbles: true }))

    const gain = target.querySelector('input[type="range"][min="-24"]')
    if (!(gain instanceof HTMLInputElement)) throw new Error('Gain control not found')
    gain.value = '5'
    gain.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()

    button(target, 'Начать запись').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Идёт запись голоса'))
    expect(target.querySelector('meter')?.getAttribute('value')).toBe('0.7')
    expect(vi.mocked(runtime.getUserMedia)).toHaveBeenCalledWith(expect.objectContaining({
      audio: expect.objectContaining({ deviceId: { exact: 'mic-usb' }, noiseSuppression: false }),
    }))
    expect(createGraph).toHaveBeenCalledWith(input, expect.objectContaining({ inputGainDb: 5 }))

    button(target, 'Пауза').click()
    await tick()
    expect(target.textContent).toContain('Запись приостановлена')
    button(target, 'Продолжить').click()
    button(target, 'Завершить').click()
    await settle()

    expect(oncapture).toHaveBeenCalledOnce()
    const file = oncapture.mock.calls[0]?.[0] as File
    expect(file).toBeInstanceOf(File)
    expect(file.type).toBe('audio/webm')
    expect(voiceoverEvents).toEqual([file])
    expect(captureEvents).toEqual([file])
    expect(target.querySelector('audio[aria-label="Предпросмотр голосовой записи"]')).not.toBeNull()
    expect(inputTrack.stopped).toBe(true)
    expect(graphCleanup).toHaveBeenCalledOnce()

    await unmount(component)
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:voiceover-panel')
  })

  it('shows a fail-closed unsupported-browser error without requesting permission', async () => {
    const runtime: VoiceoverRuntime = {
      isSupported: () => false,
      getUserMedia: vi.fn(),
      enumerateDevices: vi.fn(async () => []),
      createGraph: vi.fn(),
      createRecorder: vi.fn(),
      isMimeTypeSupported: () => false,
      createObjectURL: vi.fn(),
      revokeObjectURL: vi.fn(),
      now: () => Date.now(),
      setTimeout: (callback, delay) => setTimeout(callback, delay),
      clearTimeout: (timer) => clearTimeout(timer),
      setInterval: (callback, delay) => setInterval(callback, delay),
      clearInterval: (timer) => clearInterval(timer),
    }
    const target = document.createElement('div')
    document.body.append(target)
    const component = mount(VoiceoverPanel, { target, props: { runtime, countdownSeconds: 0 } })
    await settle()
    button(target, 'Начать запись').click()
    await vi.waitFor(() => expect(target.querySelector('[role="alert"]')?.textContent).toContain('не поддерживает'))

    expect(target.querySelector('[role="alert"]')?.textContent).toContain('не поддерживает')
    expect(vi.mocked(runtime.getUserMedia)).not.toHaveBeenCalled()
    await unmount(component)
  })
})
