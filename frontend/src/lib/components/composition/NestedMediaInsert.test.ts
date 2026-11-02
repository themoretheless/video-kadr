// @ts-expect-error Tests need the Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import * as api from '$lib/api'
import NestedMediaInsert from './NestedMediaInsert.svelte'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { state as library } from '$lib/state/store.svelte'
import fixture from '../../../../../backend/tests/fixtures/concat-merge-preview.json'
vi.mock('$lib/api', async importOriginal => ({ ...await importOriginal<typeof api>(), evaluateNestedFrame: vi.fn(async () => ({})) }))
it('inserts a library source through actual controls, validates, and restores source/clip/output through Undo', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.createdTimeline) as unknown as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'; nestedComposition.renderedOutputs = { 'new-timeline': 'old', root: 'keep' }
  const old = library.library
  library.library = [{ id: 'insert-source', kind: 'source', filename: 'video.mov', url: '/files/sources/video.mov', mediaType: 'video', duration: 10, width: 4, height: 4, acodec: 'pcm_s16le', createdAt: 0 }]
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedMediaInsert, { target })
  try {
    await tick()
    const choose = (label: string, value: string) => { const select = target.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; select.value = value; select.dispatchEvent(new Event('change', { bubbles: true })) }
    choose('Медиа для нативного таймлайна', 'insert-source'); choose('Дорожка для нового клипа', 'video-1'); await tick()
    const audio = target.querySelector<HTMLInputElement>('[aria-label="Использовать звук нового видео"]')!
    audio.checked = false; audio.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const button = target.querySelector<HTMLButtonElement>('button')!
    expect(button.disabled, button.title).toBe(false); button.click()
    await vi.waitFor(() => expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[0]!.clips).toHaveLength(1))
    expect(nestedComposition.document!.compositions['new-timeline']!.content.sources['insert-source']).toMatchObject({ kind: 'video', hasAudio: true, durationTicks: 10000000 })
    expect(nestedComposition.document!.compositions['new-timeline']!.content.tracks[0]!.clips[0]).toMatchObject({ sourceAudioEnabled: false, placement: { sourceInTick: 0, sourceOutTick: 10000000 } })
    expect(api.evaluateNestedFrame).toHaveBeenCalled(); expect(nestedHistory.past).toHaveLength(1)
    expect(nestedComposition.renderedOutputs).toEqual({ root: 'keep' })
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
    expect(nestedComposition.renderedOutputs).toEqual({ 'new-timeline': 'old', root: 'keep' })
  } finally { await unmount(app); target.remove(); library.library = old; closeNestedComposition() }
})

it('automatically chooses lanes and commits video plus separate sound as one undoable edit', async () => {
  closeNestedComposition()
  const document = structuredClone(fixture.createdTimeline) as unknown as api.NativeNestedProject
  nestedComposition.document = document; nestedComposition.activeCompositionId = 'new-timeline'
  const old = library.library
  library.library = [{ id: 'source', kind: 'source', filename: 'video.mov', url: '/files/sources/video.mov', mediaType: 'video', duration: 10, width: 4, height: 4, acodec: 'pcm_s16le', createdAt: 0 }]
  const target = globalThis.document.createElement('div'); globalThis.document.body.append(target)
  const app = mount(NestedMediaInsert, { target })
  try {
    await tick()
    const source = target.querySelector<HTMLSelectElement>('[aria-label="Медиа для нативного таймлайна"]')!
    source.value = 'source'; source.dispatchEvent(new Event('change', { bubbles: true }))
    const automatic = target.querySelector<HTMLInputElement>('[aria-label="Автоматический выбор дорожки"]')!
    automatic.checked = true; automatic.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    expect(target.querySelector<HTMLSelectElement>('[aria-label="Дорожка для нового клипа"]')!.disabled).toBe(true)
    const button = target.querySelector<HTMLButtonElement>('button')!
    expect(button.disabled, button.title).toBe(false); button.click()
    await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
    const tracks = nestedComposition.document!.compositions['new-timeline']!.content.tracks
    expect(tracks[1]!.clips).toHaveLength(1); expect(tracks[3]!.clips).toHaveLength(1)
    expect(tracks[1]!.clips[0]).toMatchObject({ sourceAudioEnabled: false, placement: { sourceInTick: 0, sourceOutTick: 10000000 } })
    expect('placement' in tracks[1]!.clips[0]! && tracks[1]!.clips[0].placement).toEqual('placement' in tracks[3]!.clips[0]! && tracks[3]!.clips[0].placement)
    undoNestedComposition(); expect(nestedComposition.document).toEqual(document)
  } finally { await unmount(app); target.remove(); library.library = old; closeNestedComposition() }
})
