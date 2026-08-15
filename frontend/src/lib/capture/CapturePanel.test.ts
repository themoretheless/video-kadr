// @ts-expect-error Vitest's default SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { afterEach, describe, expect, it, vi } from 'vitest'
import CapturePanel from './CapturePanel.svelte'
import type { CaptureRuntime, RecorderPort } from './types.js'

class ComponentTrack {
  readonly kind: 'audio' | 'video'
  stopped = false
  private readonly endedListeners = new Set<EventListenerOrEventListenerObject>()

  constructor(kind: 'audio' | 'video') {
    this.kind = kind
  }

  stop(): void {
    this.stopped = true
  }

  addEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    if (type === 'ended') this.endedListeners.add(listener)
  }

  removeEventListener(type: string, listener: EventListenerOrEventListenerObject): void {
    if (type === 'ended') this.endedListeners.delete(listener)
  }
}

function componentStream(tracks: ComponentTrack[]): MediaStream {
  return {
    getTracks: () => tracks,
    getVideoTracks: () => tracks.filter((track) => track.kind === 'video'),
    getAudioTracks: () => tracks.filter((track) => track.kind === 'audio'),
  } as unknown as MediaStream
}

class ComponentRecorder implements RecorderPort {
  readonly mimeType = 'video/webm'
  state: RecordingState = 'inactive'
  ondataavailable: ((event: BlobEvent) => void) | null = null
  onerror: ((event: ErrorEvent) => void) | null = null
  onstop: ((event: Event) => void) | null = null

  start(): void {
    this.state = 'recording'
  }

  pause(): void {
    this.state = 'paused'
  }

  resume(): void {
    this.state = 'recording'
  }

  stop(): void {
    this.state = 'inactive'
    this.ondataavailable?.({ data: new Blob(['captured']) } as BlobEvent)
    this.onstop?.(new Event('stop'))
  }
}

function buttonByText(root: HTMLElement, text: string): HTMLButtonElement {
  const button = [...root.querySelectorAll('button')].find((candidate) => candidate.textContent?.trim() === text)
  if (!(button instanceof HTMLButtonElement)) throw new Error(`Button not found: ${text}`)
  return button
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

describe('CapturePanel', () => {
  it('exposes pause/resume/stop controls and emits the recorded File', async () => {
    const displayVideo = new ComponentTrack('video')
    const systemAudio = new ComponentTrack('audio')
    const display = componentStream([displayVideo, systemAudio])
    const recorder = new ComponentRecorder()
    const previewCanvas = document.createElement('canvas')
    previewCanvas.getBoundingClientRect = () => ({
      x: 0,
      y: 0,
      left: 0,
      top: 0,
      right: 640,
      bottom: 360,
      width: 640,
      height: 360,
      toJSON: () => ({}),
    })
    const revokeObjectURL = vi.fn()
    const requestFrame = vi.spyOn(window, 'requestAnimationFrame').mockReturnValue(42)
    const cancelFrame = vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => undefined)
    const runtime: CaptureRuntime = {
      isSupported: () => true,
      getDisplayMedia: vi.fn(async () => display),
      getUserMedia: vi.fn(async () => componentStream([])),
      enumerateDevices: vi.fn(async () => []),
      prepareStream: vi.fn(async () => ({ stream: display, previewCanvas, cleanup: vi.fn() })),
      createRecorder: vi.fn(() => recorder),
      isMimeTypeSupported: () => true,
      createObjectURL: vi.fn(() => 'blob:component-preview'),
      revokeObjectURL,
      now: () => Date.now(),
      setTimeout: (callback, delayMs) => setTimeout(callback, delayMs),
      clearTimeout: (timer) => clearTimeout(timer),
      setInterval: (callback, delayMs) => setInterval(callback, delayMs),
      clearInterval: (timer) => clearInterval(timer),
    }
    const oncapture = vi.fn()
    const target = document.createElement('div')
    document.body.append(target)
    const emitted: File[] = []
    target.addEventListener('capture', (event) => emitted.push((event as CustomEvent<File>).detail))
    const component = mount(CapturePanel, {
      target,
      props: { runtime, countdownSeconds: 0, oncapture },
    })
    await settle()

    buttonByText(target, 'Начать запись').click()
    await settle()
    expect(target.textContent).toContain('Идёт запись')
    expect(target.querySelector('.capture-live-stage canvas')).toBe(previewCanvas)
    expect(target.textContent).toContain('Прямоугольник')
    expect(target.textContent).toContain('Стрелка')

    previewCanvas.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 7, button: 0, clientX: 64, clientY: 72 }))
    previewCanvas.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 7, clientX: 320, clientY: 180 }))
    previewCanvas.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 7, clientX: 576, clientY: 288 }))
    const captureInputs = vi.mocked(runtime.prepareStream).mock.calls[0]?.[0]
    expect(captureInputs.annotations.getSnapshot().strokes[0]).toMatchObject({ tool: 'pen', color: '#ff3b30', width: 5 })

    const script = target.querySelector('.teleprompter-script textarea')
    if (!(script instanceof HTMLTextAreaElement)) throw new Error('Teleprompter textarea not found')
    script.value = 'Текст остаётся поверх preview'
    script.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    buttonByText(target, 'Запустить суфлёр').click()
    await tick()
    const promptOverlay = target.querySelector('.teleprompter-overlay')
    expect(promptOverlay?.textContent).toContain('Текст остаётся поверх preview')
    expect(previewCanvas.contains(promptOverlay)).toBe(false)
    buttonByText(target, 'Пауза суфлёра').click()
    expect(requestFrame).toHaveBeenCalled()
    expect(cancelFrame).toHaveBeenCalled()

    buttonByText(target, 'Пауза').click()
    await tick()
    expect(target.textContent).toContain('Запись приостановлена')

    buttonByText(target, 'Продолжить').click()
    buttonByText(target, 'Завершить').click()
    await settle()

    expect(oncapture).toHaveBeenCalledOnce()
    const file = oncapture.mock.calls[0]?.[0] as File
    expect(file).toBeInstanceOf(File)
    expect(file.type).toBe('video/webm')
    expect(emitted).toEqual([file])
    expect(target.querySelector('video[aria-label="Предпросмотр записи"]')).not.toBeNull()
    expect(displayVideo.stopped).toBe(true)
    expect(systemAudio.stopped).toBe(true)

    await unmount(component)
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:component-preview')
    requestFrame.mockRestore()
    cancelFrame.mockRestore()
  })

  it('renders an actionable unsupported-browser error', async () => {
    const runtime: CaptureRuntime = {
      isSupported: () => false,
      getDisplayMedia: vi.fn(),
      getUserMedia: vi.fn(),
      enumerateDevices: vi.fn(async () => []),
      prepareStream: vi.fn(),
      createRecorder: vi.fn(),
      isMimeTypeSupported: () => false,
      createObjectURL: vi.fn(),
      revokeObjectURL: vi.fn(),
      now: () => Date.now(),
      setTimeout: (callback, delayMs) => setTimeout(callback, delayMs),
      clearTimeout: (timer) => clearTimeout(timer),
      setInterval: (callback, delayMs) => setInterval(callback, delayMs),
      clearInterval: (timer) => clearInterval(timer),
    }
    const target = document.createElement('div')
    document.body.append(target)
    const component = mount(CapturePanel, { target, props: { runtime, countdownSeconds: 0 } })
    await settle()

    buttonByText(target, 'Начать запись').click()
    await settle()

    const alert = target.querySelector('[role="alert"]')
    expect(alert?.textContent).toContain('не поддерживается')
    expect(target.textContent).toContain('Chrome, Edge или Firefox')
    await unmount(component)
  })
})
