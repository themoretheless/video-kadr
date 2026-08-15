// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
  addMediaInfoToComposition,
  compositionMulticamGroups,
  compositionState,
  resetCompositionForTests,
  setCompositionPlayhead,
  toggleCompositionTrackFlag,
} from '$lib/state/composition.svelte.js'
import MulticamPanel from './MulticamPanel.svelte'

let target: HTMLDivElement

beforeEach(() => {
  resetCompositionForTests()
  for (const id of ['multicam-a', 'multicam-b']) {
    addMediaInfoToComposition({
      id,
      url: `/files/sources/${id}.mp4`,
      filename: `${id}.mp4`,
      mediaType: 'video',
      duration: 4,
      width: 1280,
      height: 720,
      acodec: 'aac',
    })
  }
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(() => target?.remove())

function button(text: string): HTMLButtonElement {
  const result = [...target.querySelectorAll('button')].find((candidate) => candidate.textContent?.includes(text))
  if (!result) throw new Error(`Missing button: ${text}`)
  return result
}

function input(label: string): HTMLInputElement {
  const result = target.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`)
  if (!result) throw new Error(`Missing input: ${label}`)
  return result
}

function change(control: HTMLInputElement, value: string): void {
  control.value = value
  control.dispatchEvent(new Event('input', { bubbles: true }))
  control.dispatchEvent(new Event('change', { bubbles: true }))
}

function waveform(shift: number) {
  const values = Array.from({ length: 100 }, (_, index) => {
    const sourceIndex = index - shift
    return sourceIndex === 25 ? 1 : sourceIndex === 26 ? 0.6 : sourceIndex === 62 ? 0.75 : 0.01
  })
  return {
    durationSeconds: 1,
    sampleRate: 48_000,
    buckets: values.map((value) => ({ min: -value, max: value, rms: value })),
  }
}

describe('MulticamPanel', () => {
  it('syncs selected angles, creates tracks, records an exact EDL switch, and exposes lock errors', async () => {
    const cache = {
      load: vi.fn(async (url: string) => waveform(url.includes('multicam-b') ? 4 : 0)),
    }
    const component = mount(MulticamPanel, { target, props: { waveformCache: cache } })
    await tick()

    input('Выбрать angle multicam-a.mp4').click()
    input('Выбрать angle multicam-b.mp4').click()
    await tick()
    expect(input('Label angle multicam-a')).toBeTruthy()
    expect(input('Offset angle multicam-b')).toBeTruthy()

    button('Синхронизировать по waveform').click()
    await vi.waitFor(() => expect(target.textContent).toContain('Waveform sync готов'))
    expect(cache.load).toHaveBeenCalledTimes(2)
    change(input('Название multicam group'), 'Interview')
    button('Создать multicam group').click()
    await tick()

    let group = compositionMulticamGroups()[0]!
    expect(group).toMatchObject({ name: 'Interview' })
    expect(group.angles).toHaveLength(2)
    expect(compositionState.document.tracks.find((track) => track.id === group.videoTrackId)).toMatchObject({
      kind: 'video',
      clips: [expect.objectContaining({ id: group.switches[0]!.clipId })],
    })
    expect(target.querySelectorAll('.multicam-angle')).toHaveLength(2)

    setCompositionPlayhead(1_000_000)
    await tick()
    const secondAngle = group.angles[1]!
    target.querySelector<HTMLButtonElement>(`button[aria-label="Переключить на angle ${secondAngle.label}"]`)!.click()
    await tick()
    group = compositionMulticamGroups()[0]!
    expect(group.switches.map((change) => ({ tick: change.timelineTick, angleId: change.angleId }))).toEqual([
      { tick: 0, angleId: group.angles[0]!.id },
      { tick: 1_000_000, angleId: secondAngle.id },
    ])
    expect(target.querySelectorAll('.multicam-edl tbody tr')).toHaveLength(2)

    toggleCompositionTrackFlag(group.videoTrackId, 'locked')
    await tick()
    expect(target.textContent).toContain('Program video track заблокирована')
    expect([...target.querySelectorAll<HTMLButtonElement>('.multicam-angle')].every((candidate) => candidate.disabled)).toBe(true)

    await unmount(component)
  })
})
