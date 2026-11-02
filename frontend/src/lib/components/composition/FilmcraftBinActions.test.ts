// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { duplicateFilmcraftProjectItems, editFilmcraftBins, type FilmcraftImportResult } from '$lib/api'
import Controls from './FilmcraftBinActions.svelte'
vi.mock('$lib/api', () => ({ duplicateFilmcraftProjectItems: vi.fn(), editFilmcraftBins: vi.fn() }))
const props = { document: 'exact', sequenceId: '18446744073709551615', clipId: '', token: 'owner', items: [{ id: '9007199254740993', name: 'One', kind: 'Media' as const }, { id: '9007199254740995', name: 'Two', kind: 'Graphic' as const }], bins: [{ id: '0', name: 'Root', parentId: null, depth: 0 }, { id: '9007199254740997', name: 'Nested', parentId: '0', depth: 1 }] }
const result = { documentText: 'changed', originalSchema: 12, sequences: [], media: [] } as FilmcraftImportResult
let component: ReturnType<typeof mount> | undefined
const button = (label: string) => [...document.querySelectorAll('button')].find(b => b.textContent === label)!
async function selectItems(ids: string[]) {
  const select = document.querySelector<HTMLSelectElement>('[aria-label="Элементы для организации Filmcraft"]')!
  for (const option of select.options) option.selected = ids.includes(option.value)
  select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
}
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
it.each([['Создать папку', 'file.newBin'], ['Создать папку из выделения', 'file.newBinFromSelection'], ['Переместить в папку', 'project.moveToBin']] as const)('dispatches %s with exact IDs and one fenced adoption', async (label, command) => {
  vi.mocked(editFilmcraftBins).mockResolvedValue(result)
  const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick()
  await selectItems(props.items.map(i => i.id))
  const target = document.querySelector<HTMLSelectElement>('[aria-label="Папка назначения Filmcraft"]')!
  target.value = '9007199254740997'; target.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button(label).click(); await tick()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(result, { document: props.document, sequenceId: props.sequenceId, clipId: '', token: props.token, projectSelection: props.items.map(i => i.id) }))
  expect(editFilmcraftBins).toHaveBeenCalledWith('owner', command, { document: 'exact', ...(command !== 'file.newBin' ? { items: props.items.map(i => i.id) } : {}), ...(command === 'file.newBin' ? { parent: '9007199254740997' } : command === 'project.moveToBin' ? { bin: '9007199254740997' } : {}) }, expect.any(AbortSignal))
})
it('allows explicit empty names and root creation when automatic naming is off', async () => {
  vi.mocked(editFilmcraftBins).mockResolvedValue(result)
  component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn() } }); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Автоматическое имя папки Filmcraft"]')!.click(); await tick()
  button('Создать папку').click(); await tick()
  expect(editFilmcraftBins).toHaveBeenCalledWith('owner', 'file.newBin', { document: 'exact', parent: null, name: '' }, expect.any(AbortSignal))
})
it('refuses a late result after target change without replay', async () => {
  let resolve!: (value: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftBins).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick()
  button('Создать папку').click(); await tick()
  const target = document.querySelector<HTMLSelectElement>('[aria-label="Папка назначения Filmcraft"]')!
  target.value = '9007199254740997'; target.dispatchEvent(new Event('change', { bubbles: true }))
  resolve(result); await tick(); await tick()
  expect(onedit).not.toHaveBeenCalled(); expect(editFilmcraftBins).toHaveBeenCalledTimes(1)
})
it('aborts observation on unmount and never adopts a late response', async () => {
  let resolve!: (value: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftBins).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick()
  button('Создать папку').click(); await tick()
  const signal = vi.mocked(editFilmcraftBins).mock.calls[0][3]!
  await unmount(component!); component = undefined; expect(signal.aborted).toBe(true)
  resolve(result); await tick(); expect(onedit).not.toHaveBeenCalled()
})
it('reports parent refusal and server errors without applying changes', async () => {
  vi.mocked(editFilmcraftBins).mockResolvedValueOnce(result).mockRejectedValueOnce(new Error('no such bin'))
  const onedit = vi.fn(() => false); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick()
  button('Создать папку').click(); await tick()
  await vi.waitFor(() => expect(document.body.textContent).toContain('контекст изменился'))
  button('Создать папку').click(); await tick()
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('no such bin'))
  expect(onedit).toHaveBeenCalledTimes(1); expect(editFilmcraftBins).toHaveBeenCalledTimes(2)
})
it('duplicates the exact current selection through one fenced project edit', async () => {
  const copied = { ...result, duplicatedItemIds: ['18446744073709551615'], duplicateSources: [{ id: '18446744073709551615', sourceId: props.items[0].id }] }
  vi.mocked(duplicateFilmcraftProjectItems).mockResolvedValue(copied); const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); await selectItems(props.items.map(i => i.id))
  button('Дублировать элементы проекта').click(); await tick()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(copied, { document: props.document, sequenceId: props.sequenceId, clipId: '', token: props.token, projectSelection: props.items.map(i => i.id) }))
  expect(duplicateFilmcraftProjectItems).toHaveBeenCalledWith('owner', { document: props.document, context: { projectSelection: props.items.map(i => i.id) } }, expect.any(AbortSignal))
  expect(editFilmcraftBins).not.toHaveBeenCalled()
})
it('rejects a late duplicate response after selection changes', async () => {
  let resolve!: (value: typeof result & { duplicatedItemIds: string[]; duplicateSources: Array<{ id: string; sourceId: string }> }) => void
  vi.mocked(duplicateFilmcraftProjectItems).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); await selectItems([props.items[0].id])
  button('Дублировать элементы проекта').click(); await tick(); await selectItems([props.items[1].id])
  resolve({ ...result, duplicatedItemIds: [], duplicateSources: [] }); await tick(); await tick(); expect(onedit).not.toHaveBeenCalled(); expect(duplicateFilmcraftProjectItems).toHaveBeenCalledTimes(1)
})
it('aborts duplicate observation on unmount without adopting or replaying', async () => {
  let resolve!: (value: typeof result & { duplicatedItemIds: string[]; duplicateSources: Array<{ id: string; sourceId: string }> }) => void
  vi.mocked(duplicateFilmcraftProjectItems).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); await selectItems([props.items[0].id])
  button('Дублировать элементы проекта').click(); await tick(); const signal = vi.mocked(duplicateFilmcraftProjectItems).mock.calls[0][2]!
  await unmount(component!); component = undefined; expect(signal.aborted).toBe(true)
  resolve({ ...result, duplicatedItemIds: [], duplicateSources: [] }); await tick(); expect(onedit).not.toHaveBeenCalled()
})
it('reports duplicate refusal and server failure without changing selection', async () => {
  vi.mocked(duplicateFilmcraftProjectItems).mockResolvedValueOnce({ ...result, duplicatedItemIds: [], duplicateSources: [] }).mockRejectedValueOnce(new Error('new nesting cycle'))
  const onedit = vi.fn(() => false); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); await selectItems([props.items[0].id])
  button('Дублировать элементы проекта').click(); await tick(); await vi.waitFor(() => expect(document.body.textContent).toContain('контекст изменился'))
  button('Дублировать элементы проекта').click(); await tick(); await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('new nesting cycle'))
  expect(onedit).toHaveBeenCalledTimes(1); expect(duplicateFilmcraftProjectItems).toHaveBeenCalledTimes(2)
})
