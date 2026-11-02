// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { renameFilmcraftBin, renameFilmcraftClip, type FilmcraftImportResult } from '$lib/api'
import Controls from './FilmcraftRenameControls.svelte'
vi.mock('$lib/api', () => ({ renameFilmcraftBin: vi.fn(), renameFilmcraftClip: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
const props = { document: 'exact source', sequenceId: '18446744073709551615', clipId: '9007199254740993', token: 'owner', name: 'Shot' }
const result = { documentText: 'renamed', sequences: [], media: [], originalSchema: 12 } as FilmcraftImportResult
async function submit(name: string) {
  const input = document.querySelector('input')!
  input.value = name; input.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  document.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await tick()
}
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
it('adopts one exact snapshot with a parent fence and allows empty names', async () => {
  vi.mocked(renameFilmcraftClip).mockResolvedValue(result)
  const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, onedit } })
  await tick(); await submit('')
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(result, { document: props.document, sequenceId: props.sequenceId, clipId: props.clipId, token: props.token }))
  expect(renameFilmcraftClip).toHaveBeenCalledWith('owner', { document: props.document, sequence: props.sequenceId, clip: props.clipId, name: '' }, expect.any(AbortSignal))
})
it('reports parent refusal without reporting a saved name', async () => {
  vi.mocked(renameFilmcraftClip).mockResolvedValue(result)
  component = mount(Controls, { target: document.body, props: { ...props, onedit: () => false } })
  await tick(); await submit('new')
  await vi.waitFor(() => expect(document.body.textContent).toContain('контекст изменился'))
  expect(document.body.textContent).not.toContain('Имя сохранено')
})
it('aborts observation on unmount and never adopts a late result', async () => {
  let resolve!: (value: FilmcraftImportResult) => void
  vi.mocked(renameFilmcraftClip).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, onedit } })
  await tick(); await submit('new')
  const signal = vi.mocked(renameFilmcraftClip).mock.calls[0][2]!
  await unmount(component!); component = undefined
  expect(signal.aborted).toBe(true)
  resolve(result); await tick()
  expect(onedit).not.toHaveBeenCalled()
})
it('refuses anonymous submission and exposes errors without replay', async () => {
  const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, token: null, onedit } })
  await tick(); await submit('new'); expect(renameFilmcraftClip).not.toHaveBeenCalled()
  await unmount(component!); component = undefined
  vi.mocked(renameFilmcraftClip).mockRejectedValue(new Error('No such clip'))
  component = mount(Controls, { target: document.body, props: { ...props, onedit } })
  await tick(); await submit('new')
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('No such clip'))
  expect(renameFilmcraftClip).toHaveBeenCalledTimes(1); expect(onedit).not.toHaveBeenCalled()
})
it('uses the item target and adds it to the adoption fence', async () => {
  vi.mocked(renameFilmcraftClip).mockResolvedValue(result)
  const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, itemId: '18446744073709551614', onedit } })
  await tick(); await submit('медиа 🎬')
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(result, { document: props.document, sequenceId: props.sequenceId, clipId: props.clipId, token: props.token, itemId: '18446744073709551614' }))
  expect(renameFilmcraftClip).toHaveBeenCalledWith('owner', { document: props.document, item: '18446744073709551614', name: 'медиа 🎬' }, expect.any(AbortSignal))
  expect(document.querySelector('[aria-label="Имя элемента проекта Filmcraft"]')).not.toBeNull()
})
it('uses the bin command and parent fence without rewriting Unicode whitespace', async () => {
  vi.mocked(renameFilmcraftBin).mockResolvedValue(result)
  const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, binId: '18446744073709551615', onedit } })
  await tick(); await submit('\u0085 Папка 🎬 \u2003')
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(result, { document: props.document, sequenceId: props.sequenceId, clipId: props.clipId, token: props.token, binId: '18446744073709551615' }))
  expect(renameFilmcraftBin).toHaveBeenCalledWith('owner', { document: props.document, bin: '18446744073709551615', name: '\u0085 Папка 🎬 \u2003' }, expect.any(AbortSignal))
  expect(renameFilmcraftClip).not.toHaveBeenCalled()
  expect(document.querySelector('[aria-label="Имя папки Filmcraft"]')).not.toBeNull()
})
