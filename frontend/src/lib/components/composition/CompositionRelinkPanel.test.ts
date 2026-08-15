// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Composition, CompositionSource } from '$lib/composition/types.js'
import CompositionRelinkPanel from './CompositionRelinkPanel.svelte'

const second = 1_000_000
const missing: CompositionSource = {
  id: 'missing-camera',
  kind: 'video',
  durationTicks: 10 * second,
  width: 1920,
  height: 1080,
  hasAudio: true,
}
const compatible: CompositionSource = {
  id: 'new-camera',
  kind: 'video',
  durationTicks: 20 * second,
  width: 3840,
  height: 2160,
  hasAudio: true,
}

function compositionFixture(): Composition {
  return {
    schemaVersion: 1,
    timeBase: second,
    canvas: { width: 1920, height: 1080, fps: 30, backgroundColor: '#000000' },
    sources: { [missing.id]: missing },
    tracks: [{
      id: 'video-track',
      kind: 'video',
      name: 'Video',
      locked: false,
      hidden: false,
      muted: false,
      transitions: [],
      clips: [{
        id: 'missing-clip',
        kind: 'video',
        sourceId: missing.id,
        timelineStartTicks: 0,
        sourceInTicks: second,
        sourceOutTicks: 8 * second,
        transform: { x: 0, y: 0, width: 1920, height: 1080, fit: 'contain' },
        opacity: 1,
        sourceAudioEnabled: true,
        audioGain: 1,
      }],
    }],
  }
}

let target: HTMLDivElement

beforeEach(() => {
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(() => target.remove())

describe('CompositionRelinkPanel', () => {
  it('shows only compatible candidates and invokes the exact replacement', async () => {
    const onrelink = vi.fn()
    const component = mount(CompositionRelinkPanel, {
      target,
      props: {
        document: compositionFixture(),
        missingSourceIds: [missing.id],
        candidates: [
          { source: compatible, label: 'Camera replacement' },
          { source: { ...compatible, id: 'short-camera', durationTicks: 7 * second }, label: 'Too short' },
          { source: { ...compatible, id: 'silent-camera', hasAudio: false }, label: 'Silent' },
        ],
        onrelink,
      },
    })
    await tick()

    expect(target.textContent).toContain('минимум 8.000 с · с аудио')
    expect(target.textContent).toContain('Camera replacement')
    expect(target.textContent).not.toContain('Too short')
    expect(target.textContent).not.toContain('Silent')
    const select = target.querySelector<HTMLSelectElement>(`select[aria-label="Замена для ${missing.id}"]`)!
    select.value = compatible.id
    select.dispatchEvent(new Event('input', { bubbles: true }))
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    const button = [...target.querySelectorAll('button')].find((candidate) => candidate.textContent?.includes('Перепривязать'))!
    button.click()
    await vi.waitFor(() => expect(onrelink).toHaveBeenCalledWith(missing.id, compatible))
    await tick()

    expect(target.textContent).toContain('Источник missing-camera заменён')
    await unmount(component)
  })

  it('renders an honest empty state when no local file can cover the edit', async () => {
    const component = mount(CompositionRelinkPanel, {
      target,
      props: {
        document: compositionFixture(),
        missingSourceIds: [missing.id],
        candidates: [],
        onrelink: vi.fn(),
      },
    })
    await tick()

    expect(target.textContent).toContain('нет файла нужного типа, длительности и аудио')
    expect(target.querySelector<HTMLButtonElement>('button')?.disabled).toBe(true)
    await unmount(component)
  })
})
