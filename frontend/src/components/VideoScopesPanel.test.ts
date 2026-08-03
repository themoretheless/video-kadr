import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { VideoScopesResult } from '../domain/video-scopes'
import { defaultVideoScopesPreferences, VIDEO_SCOPES_PREFERENCES_KEY } from '../video-scopes/preferences'
import type { VideoScopesPresentationStatus } from '../video-scopes/presentation'
import VideoScopesPanel from './VideoScopesPanel.vue'

let app: App<Element> | null = null
let host: HTMLElement | null = null
let contextSpy: ReturnType<typeof vi.spyOn> | null = null
let rectSpy: ReturnType<typeof vi.spyOn> | null = null
let context: CanvasRenderingContext2D | null = null
const disconnect = vi.fn()

function fixtureResult(): VideoScopesResult {
  const histogram = new Uint32Array(4 * 256)
  histogram[32] = 255
  histogram[256 + 80] = 255
  histogram[512 + 120] = 255
  histogram[768 + 200] = 255
  const waveform = new Uint32Array(2 * 256)
  waveform[255 - 32] = 255
  waveform[256 + 255 - 180] = 255
  const parade = new Uint32Array(3 * 2 * 256)
  parade[255 - 80] = 255
  parade[2 * 256 + 255 - 120] = 255
  parade[4 * 256 + 255 - 200] = 255
  const vectorscope = new Uint32Array(256 * 256)
  vectorscope[128 * 256 + 220] = 255
  return {
    schemaVersion: 1,
    descriptor: 'straight-rgba8-encoded-srgb',
    sourceWidth: 2,
    sourceHeight: 1,
    stride: 1,
    sampledColumns: 2,
    sampledRows: 1,
    sampledPixels: 2,
    alphaWeight: 510,
    histogram,
    waveform,
    parade,
    vectorscope,
  }
}

function fakeContext(): CanvasRenderingContext2D {
  return {
    beginPath: vi.fn(),
    clearRect: vi.fn(),
    createImageData: (width: number, height: number) => new ImageData(width, height),
    drawImage: vi.fn(),
    fillRect: vi.fn(),
    fillText: vi.fn(),
    lineTo: vi.fn(),
    moveTo: vi.fn(),
    putImageData: vi.fn(),
    restore: vi.fn(),
    save: vi.fn(),
    stroke: vi.fn(),
    fillStyle: '',
    font: '',
    lineWidth: 1,
    strokeStyle: '',
    textAlign: 'start',
    textBaseline: 'alphabetic',
  } as unknown as CanvasRenderingContext2D
}

beforeEach(() => {
  localStorage.clear()
  disconnect.mockClear()
  vi.stubGlobal('ResizeObserver', class {
    observe = vi.fn()
    unobserve = vi.fn()
    disconnect = disconnect
  })
  context = fakeContext()
  contextSpy = vi.spyOn(HTMLCanvasElement.prototype, 'getContext')
    .mockReturnValue(context)
  rectSpy = vi.spyOn(HTMLCanvasElement.prototype, 'getBoundingClientRect')
    .mockReturnValue({
      left: 0, top: 0, right: 320, bottom: 180,
      x: 0, y: 0, width: 320, height: 180,
      toJSON: () => ({}),
    })
})

afterEach(() => {
  app?.unmount()
  host?.remove()
  contextSpy?.mockRestore()
  rectSpy?.mockRestore()
  vi.unstubAllGlobals()
  app = null
  host = null
  contextSpy = null
  rectSpy = null
  context = null
})

function mountPanel() {
  const preferences = ref(defaultVideoScopesPreferences())
  const status = ref<VideoScopesPresentationStatus>({
    kind: 'exact',
    detail: 'Точный остановленный кадр',
    mediaTime: 1.25,
    sourceMode: 'proxy',
  })
  const Root = defineComponent({
    setup: () => () => h(VideoScopesPanel, {
      result: fixtureResult(),
      status: status.value,
      preferences: preferences.value,
      'onUpdate:preferences': value => { preferences.value = value },
    }),
  })
  host = document.createElement('div')
  document.body.append(host)
  app = createApp(Root)
  app.mount(host)
  return { element: host, preferences, status }
}

describe('VideoScopesPanel', () => {
  it('renders four adaptive scope figures, exact provenance, and non-live numeric summaries', async () => {
    const { element } = mountPanel()
    await nextTick()
    await nextTick()

    expect(element.querySelectorAll('figure.video-scope-figure')).toHaveLength(4)
    expect(element.querySelectorAll('canvas[aria-hidden="true"]')).toHaveLength(4)
    expect(element.textContent).toContain('Пауза · exact')
    expect(element.textContent).toContain('Точный остановленный кадр · Proxy · 1.250 s')
    expect(element.textContent).toContain('RGB Parade')
    expect(element.textContent).toContain('Cb/Cr peak')
    expect(element.querySelectorAll('table.scope-summary')).toHaveLength(4)
    expect(element.querySelectorAll('canvas.is-vectorscope')).toHaveLength(1)
    expect(element.querySelector('[aria-live]')).toBeNull()
    expect(element.querySelector('section')?.getAttribute('aria-labelledby')).toBeTruthy()
  })

  it('uses native controls and persists only UI preferences', async () => {
    const { element, preferences } = mountPanel()
    const source = element.querySelector<HTMLInputElement>('input[type="radio"][value="source"]')!
    source.checked = true
    source.dispatchEvent(new Event('change', { bubbles: true }))
    await nextTick()
    expect(preferences.value.stage).toBe('source')

    const waveform = element.querySelector<HTMLInputElement>('input[type="checkbox"][value="waveform"]')!
    waveform.checked = false
    waveform.dispatchEvent(new Event('change', { bubbles: true }))
    await nextTick()
    expect(preferences.value.visibleScopes).not.toContain('waveform')
    expect(element.querySelectorAll('figure.video-scope-figure')).toHaveLength(3)

    const intensity = element.querySelector<HTMLInputElement>('input[aria-label="Яркость следа"]')!
    intensity.value = '1.75'
    intensity.dispatchEvent(new Event('input', { bubbles: true }))
    await nextTick()
    expect(preferences.value.intensity).toBe(1.75)
    expect(element.textContent).toContain('1.75×')

    const persisted = JSON.parse(localStorage.getItem(VIDEO_SCOPES_PREFERENCES_KEY)!)
    expect(persisted).toEqual(preferences.value)
    expect(persisted).not.toHaveProperty('edit')
    expect(persisted).not.toHaveProperty('result')
  })

  it('transposes x-major waveform storage into row-major canvas pixels', async () => {
    mountPanel()
    await nextTick()
    await nextTick()
    const calls = vi.mocked(context!.putImageData).mock.calls
    const waveform = calls.map(call => call[0]).find(image => image.width === 2 && image.height === 256)
    expect(waveform).toBeDefined()
    // x=0, level=32 is stored at x-major index 223; it must render at (0, 223).
    expect(waveform!.data[(223 * 2) * 4 + 3]).toBe(255)
    // x=1, level=180 is stored at x-major index 256+75; it must render at (1, 75).
    expect(waveform!.data[(75 * 2 + 1) * 4 + 3]).toBe(255)
    expect(waveform!.data[(223 * 2 + 1) * 4 + 3]).toBe(0)
  })

  it('exposes unavailable and last-exact states without announcing every frame', async () => {
    const { element, status } = mountPanel()
    status.value = { kind: 'last-exact', detail: 'Поставьте видео на паузу' }
    await nextTick()
    expect(element.textContent).toContain('Последний exact')
    expect(element.textContent).toContain('Поставьте видео на паузу')

    status.value = { kind: 'unavailable', detail: 'Точный post-effects кадр недоступен' }
    await nextTick()
    expect(element.textContent).toContain('Недоступно')
    expect(element.textContent).toContain('Точный post-effects кадр недоступен')
    expect(element.querySelector('[aria-live]')).toBeNull()
  })
})
