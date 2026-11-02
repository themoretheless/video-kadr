// @ts-expect-error Tests need the Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect, it } from 'vitest'
import ClipMediaReplacement from './ClipMediaReplacement.svelte'
import { addMediaInfoToComposition, compositionState, resetCompositionForTests, setCompositionStorageForTests, undoComposition } from '$lib/state/composition.svelte'
it('replaces the selected clip through the inspector and restores it with one Undo', async () => {
  setCompositionStorageForTests(null)
  resetCompositionForTests()
  const first = addMediaInfoToComposition({ id: 'old', url: '/old.mov', filename: 'old.mov', mediaType: 'video', duration: 4, width: 16, height: 16, acodec: 'pcm_s16le' }, { pairAudio: false })
  addMediaInfoToComposition({ id: 'new', url: '/new.mov', filename: 'new.mov', mediaType: 'video', duration: 8, width: 16, height: 16, acodec: 'pcm_s16le' }, { pairAudio: false })
  const before = JSON.stringify(compositionState.document), depth = compositionState.history.past.length
  const target = document.createElement('div'); document.body.append(target)
  const component = mount(ClipMediaReplacement, { target, props: { clipId: first } })
  try {
    await tick()
    const select = target.querySelector<HTMLSelectElement>('select')!
    expect([...select.options].map(o => o.value)).toContain('new')
    select.value = 'new'; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const button = target.querySelector<HTMLButtonElement>('button')!
    expect(button.disabled, button.title).toBe(false)
    button.click(); await tick()
    expect(compositionState.document.tracks.flatMap((t): readonly import('$lib/composition/types').CompositionClip[] => t.clips).find(c => c.id === first)).toMatchObject({ sourceId: 'new' })
    expect(compositionState.history.past).toHaveLength(depth + 1)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  } finally { await unmount(component); target.remove() }
})

import { state as libraryState } from '$lib/state/store.svelte'
import { redoComposition, replaceCompositionClipFromLibrary } from '$lib/state/composition.svelte'
import type { MediaEntry } from '$lib/types'
const entry: MediaEntry = { id: 'library-only', kind: 'source', url: '/files/sources/library.mov', filename: 'library.mov', mediaType: 'video', duration: 8, width: 16, height: 16, acodec: 'pcm_s16le', createdAt: 1 }
it('registers a library-only file and its binding with replacement in one Undo/Redo', async () => {
  setCompositionStorageForTests(null); resetCompositionForTests()
  const first = addMediaInfoToComposition({ id: 'old', url: '/old.mov', filename: 'old.mov', mediaType: 'video', duration: 4, width: 16, height: 16, acodec: 'pcm_s16le' }, { pairAudio: false })
  const oldLibrary = libraryState.library; libraryState.library = [entry]
  const before = JSON.stringify(compositionState.document), oldMedia = JSON.stringify(compositionState.media), depth = compositionState.history.past.length
  const target = document.createElement('div'); document.body.append(target)
  const component = mount(ClipMediaReplacement, { target, props: { clipId: first } })
  try {
    await tick()
    expect(compositionState.document.sources[entry.id]).toBeUndefined()
    const select = target.querySelector<HTMLSelectElement>('select')!
    select.value = entry.id; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    const button = target.querySelector<HTMLButtonElement>('button')!
    expect(button.disabled, button.title).toBe(false)
    button.click(); await tick()
    expect(compositionState.document.tracks.flatMap((t): readonly import('$lib/composition/types').CompositionClip[] => t.clips)).toHaveLength(1)
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ id: first, sourceId: entry.id })
    expect(compositionState.media[entry.id]).toMatchObject({ url: entry.url, filename: entry.filename })
    expect(compositionState.history.past).toHaveLength(depth + 1)
    const replaced = JSON.stringify(compositionState.document)
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
    expect(JSON.stringify(compositionState.media)).toBe(oldMedia)
    redoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(replaced)
    expect(compositionState.media[entry.id]?.url).toBe(entry.url)
  } finally { await unmount(component); target.remove(); libraryState.library = oldLibrary }
})
it('failed library replacement leaves no source, binding or Undo entry', () => {
  setCompositionStorageForTests(null); resetCompositionForTests()
  const first = addMediaInfoToComposition({ id: 'old', url: '/old.mov', filename: 'old.mov', mediaType: 'video', duration: 4, width: 16, height: 16, acodec: 'pcm_s16le' }, { pairAudio: false })
  const before = JSON.stringify(compositionState.document), media = JSON.stringify(compositionState.media), depth = compositionState.history.past.length
  for (const candidate of [{ ...entry, duration: 1 }, { ...entry, kind: 'output' as const }, { ...entry, url: '' }]) expect(() => replaceCompositionClipFromLibrary(first, candidate)).toThrow()
  expect(JSON.stringify(compositionState.document)).toBe(before)
  expect(JSON.stringify(compositionState.media)).toBe(media)
  expect(compositionState.history.past).toHaveLength(depth)
})

import { vi } from 'vitest'
import * as api from '$lib/api'
import { updateCompositionClipOpacity } from '$lib/state/composition.svelte'
it.each([false, true])('uploads a chosen file and guards intervening edits (stale=%s)', async stale => {
  setCompositionStorageForTests(null); resetCompositionForTests()
  const first = addMediaInfoToComposition({ id: 'old', url: '/old.mov', filename: 'old.mov', mediaType: 'video', duration: 4, width: 16, height: 16, acodec: 'pcm_s16le' }, { pairAudio: false })
  const before = JSON.stringify(compositionState.document), depth = compositionState.history.past.length
  let finish!: (media: import('$lib/types').MediaInfo) => void
  const uploaded = { id: 'uploaded', url: '/files/sources/uploaded.mov', filename: 'uploaded.mov', mediaType: 'video' as const, duration: 8, width: 16, height: 16, acodec: 'pcm_s16le' }
  const spy = vi.spyOn(api, 'uploadFile').mockImplementation(() => new Promise(resolve => { finish = resolve }))
  vi.stubGlobal('fetch', vi.fn(async () => new Response('[]', { status: 200 })))
  const target = document.createElement('div'); document.body.append(target)
  const component = mount(ClipMediaReplacement, { target, props: { clipId: first } })
  try {
    await tick()
    const input = target.querySelector<HTMLInputElement>('input[type="file"]')!
    const file = new File(['synthetic media'], 'replacement.mov', { type: 'video/quicktime' })
    Object.defineProperty(input, 'files', { configurable: true, value: [file] })
    input.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    expect(spy).toHaveBeenCalledWith(file)
    expect(target.querySelector('fieldset')?.disabled).toBe(true)
    if (stale) updateCompositionClipOpacity(first, 0.5)
    finish(uploaded)
    await vi.waitFor(() => expect(target.querySelector('fieldset')?.disabled).toBe(false))
    if (stale) {
      expect(target.querySelector('[role="alert"]')?.textContent).toContain('Проект изменился')
      expect(compositionState.document.sources.uploaded).toBeUndefined()
      expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ sourceId: 'old', opacity: 0.5 })
    } else {
      expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ id: first, sourceId: 'uploaded' })
      expect(compositionState.media.uploaded?.url).toBe(uploaded.url)
      expect(compositionState.history.past).toHaveLength(depth + 1)
      undoComposition()
      expect(JSON.stringify(compositionState.document)).toBe(before)
      expect(compositionState.media.uploaded).toBeUndefined()
    }
  } finally { await unmount(component); target.remove(); spy.mockRestore(); vi.unstubAllGlobals() }
})
it('keeps the original while attaching a processed copy and toggles it with Undo', async () => {
  setCompositionStorageForTests(null); resetCompositionForTests()
  const first = addMediaInfoToComposition({ id: 'old', url: '/old.mov', filename: 'old.mov', mediaType: 'video', duration: 4, width: 16, height: 16, acodec: 'pcm_s16le' }, { pairAudio: false })
  addMediaInfoToComposition({ id: 'new', url: '/new.mov', filename: 'new.mov', mediaType: 'video', duration: 8, width: 16, height: 16, acodec: 'pcm_s16le' }, { pairAudio: false })
  const before = JSON.stringify(compositionState.document), depth = compositionState.history.past.length
  const target = document.createElement('div'); document.body.append(target)
  const component = mount(ClipMediaReplacement, { target, props: { clipId: first } })
  try {
    await tick()
    const retain = target.querySelector<HTMLInputElement>('[aria-label="Сохранить оригинал для сравнения"]')!
    retain.checked = true; retain.dispatchEvent(new Event('change', { bubbles: true }))
    const select = target.querySelector<HTMLSelectElement>('select')!
    select.value = 'new'; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    target.querySelector<HTMLButtonElement>('button')!.click(); await tick()
    expect(compositionState.document.metadata?.mediaVersions?.[first]).toEqual({ originalSourceId: 'old', copySourceId: 'new' })
    const toggle = target.querySelector<HTMLInputElement>('[aria-label="Использовать обработанную копию"]')!
    expect(toggle.checked).toBe(true)
    toggle.checked = false; toggle.dispatchEvent(new Event('change', { bubbles: true })); await tick()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ sourceId: 'old' })
    expect(compositionState.history.past).toHaveLength(depth + 2)
    undoComposition()
    expect(compositionState.document.tracks[0]!.clips[0]).toMatchObject({ sourceId: 'new' })
    undoComposition()
    expect(JSON.stringify(compositionState.document)).toBe(before)
  } finally { await unmount(component); target.remove() }
})
