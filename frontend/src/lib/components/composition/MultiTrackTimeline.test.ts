// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { COMPOSITION_TIME_BASE } from '$lib/composition/types.js'
import { LocalWaveformCache } from '$lib/audio/waveformCache.js'
import {
  addMediaInfoToComposition,
  compositionState,
  moveCompositionClip,
  resetCompositionForTests,
  setCompositionPlayhead,
  setCompositionTransition,
} from '$lib/state/composition.svelte.js'
import MultiTrackTimeline from './MultiTrackTimeline.svelte'

let target: HTMLDivElement

beforeEach(() => {
  vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('decode unavailable in test')))
  resetCompositionForTests()
  addMediaInfoToComposition({
    id: 'timeline-video',
    url: '/files/sources/timeline.mp4',
    filename: 'timeline.mp4',
    mediaType: 'video',
    duration: 8,
    width: 1280,
    height: 720,
    acodec: 'aac',
  })
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(() => {
  vi.unstubAllGlobals()
  target?.remove()
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

describe('MultiTrackTimeline', () => {
  it('renders authored tracks and invokes split plus track controls', async () => {
    const component = mount(MultiTrackTimeline, { target })
    await tick()

    expect(target.textContent).toContain('timeline.mp4')
    expect(target.querySelectorAll('.composition-clip')).toHaveLength(1)

    setCompositionPlayhead(3 * COMPOSITION_TIME_BASE)
    button('Разрезать').click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips).toHaveLength(2)
    expect(target.querySelectorAll('.composition-clip')).toHaveLength(2)

    const videoTrack = compositionState.document.tracks[0]!
    const [from, to] = videoTrack.clips
    setCompositionTransition(videoTrack.id, from!.id, to!.id, 'dissolve', 500_000)
    await tick()
    expect(target.querySelector<HTMLButtonElement>('.composition-transition-handle')?.getAttribute('aria-label')).toContain('dissolve')

    const sound = target.querySelector<HTMLButtonElement>('button[title="Звук"]')!
    sound.click()
    await tick()
    expect(compositionState.document.tracks[0]).toMatchObject({ kind: 'video', muted: true })

    const firstClip = compositionState.document.tracks[0]!.clips[0]!
    const startBefore = firstClip.timelineStartTicks
    const trimStart = target.querySelector<HTMLButtonElement>('.composition-trim-handle.left')!
    trimStart.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }))
    await tick()
    expect(compositionState.document.tracks[0]!.clips[0]!.timelineStartTicks).toBe(
      startBefore + Math.round(COMPOSITION_TIME_BASE / compositionState.document.canvas.fps),
    )

    await unmount(component)
  })

  it('renders an audio waveform tile and degrades gracefully when local decoding fails', async () => {
    addMediaInfoToComposition({
      id: 'timeline-audio',
      url: '/files/sources/timeline.wav',
      filename: 'timeline.wav',
      mediaType: 'audio',
      duration: 6,
      width: 0,
      height: 0,
      acodec: 'pcm_s16le',
    })
    const waveformCache = new LocalWaveformCache(vi.fn().mockRejectedValue(new Error('decode unavailable in test')))
    const component = mount(MultiTrackTimeline, { target, props: { waveformCache } })
    await tick()
    const waveform = target.querySelector<SVGElement>('svg.audio-waveform')
    expect(waveform).not.toBeNull()
    expect(waveform?.getAttribute('aria-label')).toContain('timeline.wav')
    await vi.waitFor(() => {
      expect(waveform?.getAttribute('aria-label')).toContain('недоступна')
    })

    const solo = target.querySelector<HTMLButtonElement>('button[title="Solo"]')!
    solo.click()
    await tick()
    expect(compositionState.document.tracks.find((track) => track.kind === 'audio')).toMatchObject({ solo: true })

    await unmount(component)
  })

  it('renders editable ruler markers and invokes one-shot magnet plus ripple delete', async () => {
    const component = mount(MultiTrackTimeline, { target })
    setCompositionPlayhead(2 * COMPOSITION_TIME_BASE)
    await tick()
    button('+ Marker').click()
    await tick()

    expect(target.querySelectorAll('.composition-marker-handle')).toHaveLength(1)
    const label = target.querySelector<HTMLInputElement>('input[aria-label^="Метка marker"]')!
    change(label, 'Drop')
    const markerTime = target.querySelector<HTMLInputElement>('input[aria-label^="Время marker"]')!
    change(markerTime, '1.5')
    change(target.querySelector<HTMLInputElement>('input[type="color"]')!, '#22c55e')
    await tick()
    expect(target.querySelector('.composition-marker-handle')?.getAttribute('aria-label')).toContain('Drop, 1.5s')
    expect(target.querySelector<HTMLElement>('.composition-marker-handle')?.style.getPropertyValue('--marker-color')).toBe('#22c55e')
    setCompositionPlayhead(0)
    target.querySelector<HTMLButtonElement>('.composition-marker-handle')!.click()
    expect(compositionState.transport.playheadTicks).toBe(1.5 * COMPOSITION_TIME_BASE)

    setCompositionPlayhead(3 * COMPOSITION_TIME_BASE)
    button('Разрезать').click()
    await tick()
    const track = compositionState.document.tracks[0]!
    const right = track.clips[1]!
    moveCompositionClip(right.id, track.id, 5 * COMPOSITION_TIME_BASE, false)
    await tick()
    target.querySelector<HTMLButtonElement>('button[aria-label^="Собрать gaps"]')!.click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips[1]!.timelineStartTicks).toBe(3 * COMPOSITION_TIME_BASE)
    button('Ripple delete').click()
    await tick()
    expect(compositionState.document.tracks[0]!.clips).toHaveLength(1)
    target.querySelector<HTMLButtonElement>('button[aria-label="Удалить marker Drop"]')!.click()
    await tick()
    expect(target.querySelectorAll('.composition-marker-handle')).toHaveLength(0)

    await unmount(component)
  })
})
