// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { selectFilmcraftSelection, selectFilmcraftLabelGroup, editFilmcraftLabels, FILMCRAFT_LABELS, type FilmcraftImportResult } from '$lib/api'
import Controls from './FilmcraftLabelControls.svelte'
vi.mock('$lib/api', async importOriginal => ({ ...await importOriginal<typeof import('$lib/api')>(), editFilmcraftLabels: vi.fn(), selectFilmcraftLabelGroup: vi.fn(), selectFilmcraftSelection: vi.fn() }))
const props = { document: 'exact', sequenceId: '18446744073709551615', clipId: '9007199254740993', token: 'owner', items: [{ id: '9007199254740995', name: 'One', kind: 'Media' as const, label: 'Iris' as const }], palette: [{ name: 'Violet' as const, rgb: [56, 14, 167] }] }
const result = { documentText: 'labelled', originalSchema: 12, sequences: [], media: [] } as FilmcraftImportResult
let component: ReturnType<typeof mount> | undefined
const button = (text: string) => [...document.querySelectorAll('button')].find(b => b.textContent === text)!
async function choose(name: string) { const input = document.querySelector<HTMLSelectElement>('[aria-label="Метка Filmcraft"]')!; input.value = name; input.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
it.each(FILMCRAFT_LABELS)('uses named %s command for a clip with exact source IDs', async label => {
  vi.mocked(editFilmcraftLabels).mockResolvedValue(result); const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); await choose(label)
  button('Метка для клипа').click(); await tick()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(result, { document: props.document, sequenceId: props.sequenceId, clipId: props.clipId, token: props.token, projectSelection: [], timelineSelection: [props.clipId] }))
  expect(editFilmcraftLabels).toHaveBeenCalledWith('owner', `edit.label.${label.toLowerCase()}`, { document: 'exact', sequence: props.sequenceId, clips: [props.clipId] }, expect.any(AbortSignal))
})
it('targets project items independently of the clip and uses native palette bytes', async () => {
  vi.mocked(editFilmcraftLabels).mockResolvedValue(result)
  component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn() } }); await tick()
  expect(document.querySelector<HTMLElement>('[aria-label="Цвет метки Filmcraft"]')!.style.backgroundColor.replaceAll(' ', '')).toBe('rgb(56,14,167)')
  const select = document.querySelector<HTMLSelectElement>('[aria-label="Элементы для меток Filmcraft"]')!
  select.options[0].selected = true; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Метка для элементов').click(); await tick()
  expect(editFilmcraftLabels).toHaveBeenCalledWith('owner', 'edit.label.violet', { document: 'exact', sequence: props.sequenceId, items: ['9007199254740995'] }, expect.any(AbortSignal))
})
it('rejects a late reply after changing color without replaying', async () => {
  let resolve!: (value: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftLabels).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick()
  button('Метка для клипа').click(); await tick(); await choose('Rose')
  resolve(result); await tick(); await tick()
  expect(onedit).not.toHaveBeenCalled(); expect(editFilmcraftLabels).toHaveBeenCalledTimes(1)
})
it('aborts observation on unmount and never adopts a late reply', async () => {
  let resolve!: (value: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftLabels).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick()
  button('Метка для клипа').click(); await tick(); const signal = vi.mocked(editFilmcraftLabels).mock.calls[0][3]!
  await unmount(component!); component = undefined; expect(signal.aborted).toBe(true)
  resolve(result); await tick(); expect(onedit).not.toHaveBeenCalled()
})
it('reports parent refusal and server errors without fallback', async () => {
  vi.mocked(editFilmcraftLabels).mockResolvedValueOnce(result).mockRejectedValueOnce(new Error('unknown label'))
  const onedit = vi.fn(() => false); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick()
  button('Метка для клипа').click(); await tick(); await vi.waitFor(() => expect(document.body.textContent).toContain('контекст изменился'))
  button('Метка для клипа').click(); await tick(); await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('unknown label'))
  expect(onedit).toHaveBeenCalledTimes(1); expect(editFilmcraftLabels).toHaveBeenCalledTimes(2)
})
it('queries timeline-priority selection without editing a document', async () => {
  vi.mocked(selectFilmcraftLabelGroup).mockResolvedValue({ clips: [props.clipId, '9007199254740997'] })
  const onedit = vi.fn(); const onselection = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, selected: ['9007199254740995'], onedit, onselection } }); await tick()
  button('Выбрать клипы по метке').click(); await tick()
  await vi.waitFor(() => expect(onselection).toHaveBeenCalledExactlyOnceWith({ clips: [props.clipId, '9007199254740997'] }, { document: props.document, sequenceId: props.sequenceId, clipId: props.clipId, token: props.token, projectSelection: ['9007199254740995'], timelineSelection: [props.clipId] }))
  expect(selectFilmcraftLabelGroup).toHaveBeenCalledWith('owner', { document: props.document, sequence: props.sequenceId, context: { selection: [props.clipId], projectSelection: ['9007199254740995'] } }, expect.any(AbortSignal))
  expect(onedit).not.toHaveBeenCalled(); expect(editFilmcraftLabels).not.toHaveBeenCalled()
})
it('selects project groups without requiring an active sequence', async () => {
  vi.mocked(selectFilmcraftLabelGroup).mockResolvedValue({ items: ['9007199254740995'] })
  const onselection = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, sequenceId: '', clipId: '', selected: ['9007199254740995'], onedit: vi.fn(), onselection } }); await tick()
  button('Выбрать элементы по метке').click(); await tick()
  await vi.waitFor(() => expect(onselection).toHaveBeenCalledTimes(1))
  expect(selectFilmcraftLabelGroup).toHaveBeenCalledWith('owner', { document: props.document, context: { selection: [], projectSelection: ['9007199254740995'] } }, expect.any(AbortSignal))
})
it('rejects a queued selection after color/context changes', async () => {
  let resolve!: (value: { clips: string[] }) => void
  vi.mocked(selectFilmcraftLabelGroup).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const onselection = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn(), onselection } }); await tick()
  button('Выбрать клипы по метке').click(); await tick(); await choose('Rose')
  resolve({ clips: [] }); await tick(); await tick(); expect(onselection).not.toHaveBeenCalled()
})
it.each([
  ['Выбрать клипы того же исходного элемента', 'edit.selectAllMatching', { clips: [props.clipId] }],
  ['Выбрать все элементы проекта', 'project.selectAll', { items: ['9007199254740995'] }],
  ['Снять выделение элементов проекта', 'project.deselectAll', { items: [] }]
] as const)('queries %s without label/document edits', async (text, command, result) => {
  vi.mocked(selectFilmcraftSelection).mockResolvedValue('clips' in result ? { clips: [...result.clips] } : { items: [...result.items] })
  const onselection = vi.fn(); const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, selected: ['9007199254740995'], onedit, onselection } }); await tick()
  button(text).click(); await tick()
  await vi.waitFor(() => expect(onselection).toHaveBeenCalledTimes(1))
  expect(selectFilmcraftSelection).toHaveBeenCalledWith('owner', command, { document: props.document, sequence: props.sequenceId, context: { selection: [props.clipId], projectSelection: ['9007199254740995'] } }, expect.any(AbortSignal))
  expect(onedit).not.toHaveBeenCalled(); expect(editFilmcraftLabels).not.toHaveBeenCalled(); expect(selectFilmcraftLabelGroup).not.toHaveBeenCalled()
})
it('can select all project items with no current selection or active sequence', async () => {
  vi.mocked(selectFilmcraftSelection).mockResolvedValue({ items: ['9007199254740995'] }); const onselection = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, sequenceId: '', clipId: '', onedit: vi.fn(), onselection } }); await tick()
  button('Выбрать все элементы проекта').click(); await tick()
  await vi.waitFor(() => expect(onselection).toHaveBeenCalledTimes(1))
  expect(selectFilmcraftSelection).toHaveBeenCalledWith('owner', 'project.selectAll', { document: props.document, context: { selection: [], projectSelection: [] } }, expect.any(AbortSignal))
})
