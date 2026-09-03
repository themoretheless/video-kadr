// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Composition } from '$lib/composition/types.js'
import type { BrowserTrackingFrames } from '$lib/motion/browserFrameSampler.js'
import type { PointTrackResult } from '$lib/motion/tracker.js'
import CompositionTrackerPanel from './CompositionTrackerPanel.svelte'

const motionMocks = vi.hoisted(() => ({
  sampleFrames: vi.fn(),
  trackPoints: vi.fn(),
}))

vi.mock('$lib/motion/browserFrameSampler.js', async (importOriginal) => ({
  ...await importOriginal<typeof import('$lib/motion/browserFrameSampler.js')>(),
  sampleBrowserVideoLumaFrames: motionMocks.sampleFrames,
}))

vi.mock('$lib/motion/tracker.js', async (importOriginal) => ({
  ...await importOriginal<typeof import('$lib/motion/tracker.js')>(),
  trackPointSequence: motionMocks.trackPoints,
}))

const second = 1_000_000

function compositionFixture(): Composition {
  return {
    schemaVersion: 1,
    timeBase: second,
    canvas: { width: 1280, height: 720, fps: 30, backgroundColor: '#000000' },
    sources: {
      'tracking-source': {
        id: 'tracking-source',
        kind: 'video',
        durationTicks: 2 * second,
        width: 1920,
        height: 1080,
        hasAudio: true,
      },
      'tracked-overlay': {
        id: 'tracked-overlay',
        kind: 'image',
        durationTicks: 0,
        width: 320,
        height: 180,
        hasAudio: false,
      },
    },
    tracks: [
      {
        id: 'overlay-track',
        kind: 'image',
        name: 'Tracked overlay',
        locked: false,
        hidden: false,
        clips: [{
          id: 'overlay-clip',
          kind: 'image',
          sourceId: 'tracked-overlay',
          timelineStartTicks: 0,
          durationTicks: 2 * second,
          transform: { x: 100, y: 50, width: 320, height: 180, fit: 'contain' },
          opacity: 0.8,
          animation: { opacity: { mode: 'constant', value: 0.65 } },
        }],
      },
      {
        id: 'primary-track',
        kind: 'video',
        name: 'Primary',
        locked: false,
        hidden: false,
        muted: false,
        transitions: [],
        clips: [{
          id: 'primary-clip',
          kind: 'video',
          sourceId: 'tracking-source',
          timelineStartTicks: 0,
          sourceInTicks: 0,
          sourceOutTicks: 2 * second,
          transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' },
          opacity: 1,
          sourceAudioEnabled: true,
          audioGain: 1,
        }],
      },
    ],
  }
}

function sampledFrames(): BrowserTrackingFrames {
  const frame = { width: 12, height: 12, data: new Uint8Array(12 * 12).fill(128) }
  return {
    frames: [frame, { ...frame, data: new Uint8Array(frame.data) }],
    width: 12,
    height: 12,
    sampleToSourceScaleX: 1,
    sampleToSourceScaleY: 1,
  }
}

function completedTrack(): PointTrackResult {
  return {
    status: 'completed',
    points: [
      { frameIndex: 0, x: 6, y: 6, confidence: 1 },
      { frameIndex: 1, x: 8, y: 7, confidence: 0.9 },
    ],
  }
}

let target: HTMLDivElement

beforeEach(() => {
  target = document.createElement('div')
  document.body.append(target)
  motionMocks.sampleFrames.mockReset()
  motionMocks.trackPoints.mockReset()
  vi.stubGlobal('ImageData', class ImageData {
    constructor(
      readonly data: Uint8ClampedArray,
      readonly width: number,
      readonly height: number,
    ) {}
  })
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    putImageData: vi.fn(),
    strokeRect: vi.fn(),
    strokeStyle: '',
    lineWidth: 1,
  } as unknown as CanvasRenderingContext2D)
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  target.remove()
})

function button(text: string): HTMLButtonElement {
  const result = [...target.querySelectorAll('button')].find((candidate) => candidate.textContent?.includes(text))
  if (!result) throw new Error(`Missing button: ${text}`)
  return result
}

function change(control: HTMLInputElement, value: string): void {
  control.value = value
  control.dispatchEvent(new Event('input', { bubbles: true }))
  control.dispatchEvent(new Event('change', { bubbles: true }))
}

async function prepareAndSelectPoint(): Promise<void> {
  change(target.querySelector<HTMLInputElement>('input[aria-label="Конец tracking range"]')!, '0.2')
  motionMocks.sampleFrames.mockResolvedValueOnce(sampledFrames())
  button('Подготовить кадры').click()
  await vi.waitFor(() => expect(target.textContent).toContain('Подготовлено кадров: 2'))
  target.querySelector<HTMLButtonElement>('button[aria-label="Выбрать tracking point на первом кадре"]')!.click()
  await tick()
}

describe('CompositionTrackerPanel', () => {
  it('offers reverse and speed-ramp sources while excluding freeze playback', async () => {
    const base = compositionFixture()
    const primary = base.tracks[1]!
    if (primary.kind !== 'video') throw new Error('fixture')
    const clip = primary.clips[0]!
    const rampPoints = [
      { sourceProgressTick: 0, speed: 1 },
      { sourceProgressTick: second, speed: 2 },
      { sourceProgressTick: 2 * second, speed: 2 },
    ]
    const document: Composition = {
      ...base,
      tracks: [base.tracks[0]!, {
        ...primary,
        clips: [
          { ...clip, id: 'reverse-source', playbackMode: { mode: 'reverse' } },
          { ...clip, id: 'ramp-source', speedRamp: { interpolation: 'linear', points: rampPoints } },
          { ...clip, id: 'freeze-source', playbackMode: { mode: 'freeze', sourceTick: second } },
        ],
      }],
    }
    const component = mount(CompositionTrackerPanel, {
      target,
      props: {
        document,
        media: { 'tracking-source': { url: '/files/sources/tracking-source.mp4' } },
        onapply: vi.fn(),
      },
    })
    await tick()

    const options = [...target.querySelector<HTMLSelectElement>('select')!.options].map((option) => option.value)
    expect(options).toContain('reverse-source')
    expect(options).toContain('ramp-source')
    expect(options).not.toContain('freeze-source')
    await unmount(component)
  })

  it('offers a visible overlay video as a transformed tracking source', async () => {
    const base = compositionFixture()
    const primary = base.tracks[1]!
    const overlaySource = {
      id: 'camera-overlay-track', kind: 'video' as const, name: 'Camera overlay', locked: false, hidden: false, muted: true,
      transitions: [],
      clips: [{
        id: 'camera-overlay-clip', kind: 'video' as const, sourceId: 'tracking-source', timelineStartTicks: 0,
        sourceInTicks: 0, sourceOutTicks: 2 * second, speed: 1,
        transform: { x: 120, y: -40, width: 640, height: 360, fit: 'contain' as const },
        rotationDegrees: 15, opacity: 1, sourceAudioEnabled: false, audioGain: 1,
      }],
    }
    const component = mount(CompositionTrackerPanel, {
      target,
      props: {
        document: { ...base, tracks: [base.tracks[0]!, overlaySource, primary] },
        media: { 'tracking-source': { url: '/files/sources/tracking-source.mp4' } },
        onapply: vi.fn(),
      },
    })
    await tick()

    const sourceSelect = target.querySelector<HTMLSelectElement>('select')!
    expect([...sourceSelect.options].map((option) => option.value)).toContain('camera-overlay-clip')
    await unmount(component)
  })

  it('samples same-origin media, selects a point, and applies exact paired X/Y keyframes', async () => {
    const onapply = vi.fn()
    const component = mount(CompositionTrackerPanel, {
      target,
      props: {
        document: compositionFixture(),
        media: { 'tracking-source': { url: '/files/sources/tracking-source.mp4' } },
        onapply,
      },
    })
    await tick()

    expect(target.querySelector('details')?.getAttribute('aria-label')).toBe('Classical point tracking')
    await prepareAndSelectPoint()
    expect(motionMocks.sampleFrames).toHaveBeenCalledWith(
      '/files/sources/tracking-source.mp4',
      [0, 0.1],
      { signal: expect.any(AbortSignal) },
    )
    motionMocks.trackPoints.mockReturnValueOnce(completedTrack())
    button('Отследить и применить X/Y').click()
    await vi.waitFor(() => expect(onapply).toHaveBeenCalledTimes(1))

    const [clipId, animation] = onapply.mock.calls[0]!
    expect(clipId).toBe('overlay-clip')
    expect(animation.opacity).toEqual({ mode: 'constant', value: 0.65 })
    expect(animation.x).toEqual({
      mode: 'keyframes',
      track: {
        timeBase: second,
        interpolation: 'linear',
        keyframes: [{ tick: 0, value: 100 }, { tick: 100_000, value: 101.33333333333337 }],
      },
    })
    expect(animation.y).toEqual({
      mode: 'keyframes',
      track: {
        timeBase: second,
        interpolation: 'linear',
        keyframes: [{ tick: 0, value: 50 }, { tick: 100_000, value: 50.666666666666686 }],
      },
    })
    expect(animation.x.track.keyframes.map((keyframe: { tick: number }) => keyframe.tick))
      .toEqual(animation.y.track.keyframes.map((keyframe: { tick: number }) => keyframe.tick))
    await vi.waitFor(() => expect(target.textContent).toContain('minimum confidence 0.90'))
    await unmount(component)
  })

  it('reports a confidence loss without applying any mutation', async () => {
    const onapply = vi.fn()
    const component = mount(CompositionTrackerPanel, {
      target,
      props: {
        document: compositionFixture(),
        media: { 'tracking-source': { url: '/files/sources/tracking-source.mp4' } },
        onapply,
      },
    })
    await tick()
    await prepareAndSelectPoint()
    motionMocks.trackPoints.mockReturnValueOnce({
      status: 'lost',
      points: [{ frameIndex: 0, x: 6, y: 6, confidence: 1 }],
      lostAtFrame: 1,
    })

    button('Отследить и применить X/Y').click()
    await vi.waitFor(() => expect(target.querySelector('[role="alert"]')?.textContent).toContain('потерял точку'))
    expect(onapply).not.toHaveBeenCalled()
    await unmount(component)
  })

  it('aborts frame preparation on cancel and cleans up an active sampler on unmount', async () => {
    const signals: AbortSignal[] = []
    motionMocks.sampleFrames.mockImplementation(
      (_url: string, _seconds: readonly number[], options: { signal: AbortSignal }) => new Promise((_resolve, reject) => {
        signals.push(options.signal)
        options.signal.addEventListener(
          'abort',
          () => reject(new DOMException('Aborted', 'AbortError')),
          { once: true },
        )
      }),
    )
    const onapply = vi.fn()
    const component = mount(CompositionTrackerPanel, {
      target,
      props: {
        document: compositionFixture(),
        media: { 'tracking-source': { url: '/files/sources/tracking-source.mp4' } },
        onapply,
      },
    })
    await tick()
    change(target.querySelector<HTMLInputElement>('input[aria-label="Конец tracking range"]')!, '0.2')
    button('Подготовить кадры').click()
    await vi.waitFor(() => expect(signals).toHaveLength(1))
    const cancel = target.querySelector<HTMLButtonElement>('button[aria-label="Отменить подготовку tracking кадров"]')!
    expect(cancel).toBeTruthy()
    cancel.click()
    await vi.waitFor(() => expect(signals[0]?.aborted).toBe(true))
    await vi.waitFor(() => expect(button('Подготовить кадры').disabled).toBe(false))

    button('Подготовить кадры').click()
    await vi.waitFor(() => expect(signals).toHaveLength(2))
    await unmount(component)
    expect(signals[1]?.aborted).toBe(true)
    expect(onapply).not.toHaveBeenCalled()
  })
})
