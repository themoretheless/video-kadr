import { createApp, nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  state: { library: [] as Array<Record<string, unknown>>, playerTime: 0, playerPlaying: false, playerSeeking: false },
  timelineState: { revision: 0, document: { multicamGroups: [], media: [] } as Record<string, unknown> },
  multicamState: {
    groups: [] as Array<Record<string, unknown>>,
    activeGroupId: null as string | null,
    playing: false,
    busy: false,
    status: '',
    error: '',
  },
  create: vi.fn(),
  switchAngle: vi.fn(),
  resync: vi.fn(),
  selectGroup: vi.fn(),
  sources: { value: [] as Array<Record<string, unknown>> },
  timeBase: { value: 1_000 },
}))

vi.mock('../store', () => ({ state: mocks.state, timelineState: mocks.timelineState }))
vi.mock('../multicam', () => ({
  multicamState: mocks.multicamState,
  createMulticam: mocks.create,
  switchMulticamAngle: mocks.switchAngle,
  resyncMulticam: mocks.resync,
  selectMulticamGroup: mocks.selectGroup,
  projectMulticamSources: mocks.sources,
  multicamTimeBase: mocks.timeBase,
  resolveMulticamAngleSource: vi.fn().mockRejectedValue(new Error('offline')),
}))

describe('MulticamPanel', () => {
  let host: HTMLElement
  let app: ReturnType<typeof createApp> | null

  async function mount(): Promise<void> {
    app?.unmount()
    host?.remove()
    const Component = (await import('./MulticamPanel.vue')).default
    host = document.createElement('div')
    document.body.append(host)
    app = createApp(Component)
    app.mount(host)
    await nextTick()
  }

  beforeEach(async () => {
    vi.clearAllMocks()
    mocks.state.library = [
      { id: 'cam-a', kind: 'source', mediaKind: 'video', filename: 'Camera A.mp4', availability: 'ready' },
      { id: 'cam-b', kind: 'source', mediaKind: 'video', filename: 'Camera B.mp4', availability: 'ready' },
      { id: 'audio', kind: 'source', mediaKind: 'audio', filename: 'Audio.wav', availability: 'ready' },
      { id: 'offline', kind: 'source', mediaKind: 'video', filename: 'Offline.mp4', availability: 'offline' },
    ]
    mocks.sources.value = [
      { id: 'cam-a', name: 'Camera A.mp4', durationTicks: 10_000, fps: 25, availability: 'ready' },
      { id: 'cam-b', name: 'Camera B.mp4', durationTicks: 10_000, fps: 25, availability: 'ready' },
      { id: 'offline', name: 'Offline.mp4', durationTicks: 10_000, fps: 25, availability: 'offline' },
    ]
    Object.assign(mocks.multicamState, {
      groups: [], activeGroupId: null, playing: false, busy: false, status: '', error: '',
    })
    await mount()
  })

  afterEach(() => {
    app?.unmount()
    host?.remove()
    app = null
  })

  it('creates from two ready videos with an explicit sync mode and reference', async () => {
    expect(host.textContent).toContain('Camera A.mp4')
    expect(host.textContent).toContain('Camera B.mp4')
    expect(host.textContent).not.toContain('Audio.wav')
    expect(host.textContent).toContain('Offline.mp4 · недоступен')
    const submit = [...host.querySelectorAll<HTMLButtonElement>('button')]
      .find(button => button.textContent?.includes('Создать multicam'))!
    expect(submit.disabled).toBe(true)

    const checks = [...host.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].filter(input => !input.disabled)
    for (const checkbox of checks) {
      checkbox.checked = true
      checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    }
    const timecode = host.querySelector<HTMLInputElement>('input[value="timecode"]')!
    timecode.click()
    await nextTick()
    const reference = host.querySelector<HTMLSelectElement>('select[required]')!
    reference.value = 'cam-b'
    reference.dispatchEvent(new Event('change', { bubbles: true }))
    await nextTick()
    expect(submit.disabled).toBe(false)
    host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await vi.waitFor(() => expect(mocks.create).toHaveBeenCalledOnce())
    expect(mocks.create).toHaveBeenCalledWith(expect.objectContaining({
      name: 'Multicam 1', mediaIds: ['cam-a', 'cam-b'], syncMode: 'timecode', referenceMediaId: 'cam-b',
      timecodeAnchors: {
        'cam-a': { startFrame: 0, rate: { numerator: 25, denominator: 1 }, dropFrame: false },
        'cam-b': { startFrame: 0, rate: { numerator: 25, denominator: 1 }, dropFrame: false },
      },
    }))
  })

  it('exposes angle state without relying on colour and switches by click or digit', async () => {
    mocks.multicamState.groups = [{
      id: 'group-1', name: 'Concert', activeAngleId: 'angle-a', referenceMediaId: 'cam-a',
      angles: [
        { id: 'angle-a', mediaId: 'cam-a', name: 'Wide', availability: 'ready' },
        { id: 'angle-b', mediaId: 'cam-b', name: 'Close', availability: 'ready' },
        { id: 'angle-c', mediaId: 'cam-c', name: 'Offline', availability: 'offline' },
      ],
    }]
    mocks.multicamState.activeGroupId = 'group-1'
    mocks.multicamState.playing = true
    await mount()

    const buttons = [...host.querySelectorAll<HTMLButtonElement>('.angle-button')]
    expect(buttons).toHaveLength(3)
    expect(buttons[0]!.getAttribute('aria-pressed')).toBe('true')
    expect(buttons[0]!.getAttribute('aria-label')).toContain('эфирный ракурс')
    expect(buttons[1]!.getAttribute('aria-keyshortcuts')).toBe('2')
    expect(buttons[2]!.disabled).toBe(true)
    expect(buttons[2]!.getAttribute('aria-label')).toContain('недоступен')

    buttons[1]!.click()
    await vi.waitFor(() => expect(mocks.switchAngle).toHaveBeenCalledWith(
      'group-1', 'angle-b', { live: true },
    ))
    mocks.switchAngle.mockClear()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: '2', bubbles: true }))
    await vi.waitFor(() => expect(mocks.switchAngle).toHaveBeenCalledWith(
      'group-1', 'angle-b', { live: true },
    ))
  })

  it('suppresses digit shortcuts in form fields and focuses actionable failures', async () => {
    mocks.multicamState.groups = [{
      id: 'group-1', name: 'Interview', activeAngleId: 'angle-a', referenceMediaId: 'cam-a',
      angles: [
        { id: 'angle-a', mediaId: 'cam-a', name: 'A', availability: 'ready' },
        { id: 'angle-b', mediaId: 'cam-b', name: 'B', availability: 'ready' },
      ],
    }]
    mocks.multicamState.activeGroupId = 'group-1'
    await mount()
    const input = host.querySelector<HTMLInputElement>('input[type="text"]')!
    input.dispatchEvent(new KeyboardEvent('keydown', { key: '2', bubbles: true }))
    expect(mocks.switchAngle).not.toHaveBeenCalled()

    mocks.create.mockRejectedValueOnce(new Error('Не удалось синхронизировать Camera B'))
    const checks = [...host.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].filter(input => !input.disabled)
    for (const checkbox of checks) {
      checkbox.checked = true
      checkbox.dispatchEvent(new Event('change', { bubbles: true }))
    }
    await nextTick()
    host.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await vi.waitFor(() => expect(host.querySelector('[role="alert"]')?.textContent).toContain('Camera B'))
    expect(document.activeElement).toBe(host.querySelector('[role="alert"]'))
  })

  it('announces sync progress and disables unavailable work', async () => {
    mocks.multicamState.busy = true
    mocks.multicamState.status = 'Анализ звука: Camera B, 50%'
    await mount()
    expect(host.querySelector('.multicam-panel')?.getAttribute('aria-busy')).toBe('true')
    expect(host.querySelector('[role="status"]')?.textContent).toContain('50%')
    expect(host.querySelector<HTMLButtonElement>('button[type="submit"]')?.disabled).toBe(true)
    expect([...host.querySelectorAll<HTMLInputElement>('input')].every(input => input.disabled)).toBe(true)
  })
})
