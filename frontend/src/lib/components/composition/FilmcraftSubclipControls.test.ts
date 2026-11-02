// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftSubclip, getFilmcraftSubclipDefaults, type FilmcraftSubclipDefaults, type FilmcraftSubclipResult } from '$lib/api'
import Controls from './FilmcraftSubclipControls.svelte'
vi.mock('$lib/api', () => ({ editFilmcraftSubclip: vi.fn(), getFilmcraftSubclipDefaults: vi.fn() }))
const parent = '9007199254740993'; const child = '9007199254740997'; const created = '18446744073709551614'
const props = { document: 'exact', sequenceId: '18446744073709551615', clipId: '9007199254740995', token: 'owner', sourceItem: parent, sourcePlayhead: '333', selection: ['9007199254740995'], selected: [parent], items: [{ id: parent, name: 'Parent', kind: 'Media' as const }, { id: child, name: 'Existing subclip', kind: 'Subclip' as const, subclip: { parentId: parent, startTicks: '100', endTicks: '200', restrictTrims: false } }] }
const result: FilmcraftSubclipResult = { documentText: 'changed', originalSchema: 12, sequences: [], media: [], subclipEdit: { itemId: created, parentId: parent, created: true, converted: false, bindingSourceId: null }, subclipProjectSelection: [created] }
const defaults: FilmcraftSubclipDefaults = { name: 'Parent.Subclip', parentId: parent, startTicks: '9007199254740997', endTicks: '9007199254740999', startFrame: '17', endFrame: '21', fps: 25, frameRate: { num: 25, den: 1 }, restrictTrims: true }
let component: ReturnType<typeof mount> | undefined
const button = (label: string) => [...document.querySelectorAll('button')].find(b => b.textContent === label)!
async function select(label: string, value: string) { const e = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; e.value = value; e.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
async function write(label: string, value: string) { const e = document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!; e.value = value; e.dispatchEvent(new Event('input', { bubbles: true })); await tick() }
async function check(label: string) { document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!.click(); await tick() }
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
it.each(['automatic', 'source', 'project', 'timeline'] as const)('creates from %s with exact target/context and bounded adoption fence', async origin => {
  vi.mocked(editFilmcraftSubclip).mockResolvedValue(result); const onedit = vi.fn().mockReturnValue(true)
  component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); await select('Источник подклипа Filmcraft', origin); if (origin === 'project') await select('Элемент подклипа Filmcraft', child)
  button('Создать подклип').click(); await tick(); await vi.waitFor(() => expect(onedit).toHaveBeenCalledOnce())
  expect(editFilmcraftSubclip).toHaveBeenCalledWith('owner', 'clip.makeSubclip', { document: 'exact', ...(origin === 'source' ? { item: parent } : origin === 'project' ? { item: child } : origin === 'timeline' ? { clip: props.clipId } : {}), restrictTrims: true, context: { activeSequence: props.sequenceId, sourceItem: parent, selection: props.selection, projectSelection: props.selected, stillImageDuration: 5, stillImageUnit: 'seconds' } }, expect.any(AbortSignal))
  expect(onedit).toHaveBeenCalledWith(result, { document: 'exact', sequenceId: props.sequenceId, clipId: props.clipId, token: 'owner', sourceItem: parent, sourcePlayhead: '333', timelineSelection: props.selection, projectSelection: props.selected })
})
it('loads defaults in native code without a document edit, then sends only requested frame overrides', async () => {
  vi.mocked(getFilmcraftSubclipDefaults).mockResolvedValue(defaults); vi.mocked(editFilmcraftSubclip).mockResolvedValue(result); const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); await select('Единицы границ подклипа Filmcraft', 'frames'); button('Показать исходные параметры подклипа').click(); await tick()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Параметры подклипа Filmcraft"]')?.textContent).toContain(defaults.startTicks)); expect(onedit).not.toHaveBeenCalled(); expect(editFilmcraftSubclip).not.toHaveBeenCalled()
  await check('Изменить начало подклипа Filmcraft'); button('Создать подклип').click(); await tick()
  expect(editFilmcraftSubclip).toHaveBeenCalledWith('owner', 'clip.makeSubclip', expect.objectContaining({ startFrame: '17' }), expect.any(AbortSignal)); expect(vi.mocked(editFilmcraftSubclip).mock.calls[0][2]).not.toHaveProperty('endFrame')
})
it('keeps untouched edit fields absent and supports explicit master conversion', async () => {
  vi.mocked(editFilmcraftSubclip).mockResolvedValue({ ...result, subclipEdit: { ...result.subclipEdit, itemId: child, created: false, converted: true, bindingSourceId: parent }, subclipProjectSelection: props.selected })
  component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn() } }); await tick(); await select('Действие подклипа Filmcraft', 'edit'); await select('Элемент подклипа Filmcraft', child)
  expect(document.querySelector('[aria-label="Границы подклипа Filmcraft"]')?.textContent).toContain('100 — 200')
  await check('Мастер-клип из подклипа Filmcraft'); button('Применить подклип').click(); await tick()
  const input = vi.mocked(editFilmcraftSubclip).mock.calls[0][2]; expect(input).toMatchObject({ item: child, convertToMaster: true }); for (const key of ['name', 'start', 'end', 'restrictTrims']) expect(input).not.toHaveProperty(key)
})
it('preserves whitespace names and refuses out-of-range explicit ticks locally', async () => {
  vi.mocked(editFilmcraftSubclip).mockResolvedValue(result); component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn() } }); await tick(); await check('Автоматическое имя подклипа Filmcraft'); await write('Имя подклипа Filmcraft', '   '); await check('Изменить начало подклипа Filmcraft'); await write('Начало подклипа Filmcraft', '9223372036854775808')
  expect(button('Создать подклип').disabled).toBe(true); await write('Начало подклипа Filmcraft', '9007199254740997'); button('Создать подклип').click(); await tick()
  expect(editFilmcraftSubclip).toHaveBeenCalledWith('owner', 'clip.makeSubclip', expect.objectContaining({ name: '   ', start: '9007199254740997' }), expect.any(AbortSignal))
})
it('discards stale defaults after target changes', async () => {
  let resolve!: (value: FilmcraftSubclipDefaults) => void; vi.mocked(getFilmcraftSubclipDefaults).mockImplementation(() => new Promise(accept => { resolve = accept }))
  component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn() } }); await tick(); button('Показать исходные параметры подклипа').click(); await tick(); await select('Источник подклипа Filmcraft', 'timeline'); resolve(defaults); await tick(); await tick(); expect(document.querySelector('[aria-label="Параметры подклипа Filmcraft"]')).toBeNull(); expect(getFilmcraftSubclipDefaults).toHaveBeenCalledTimes(1)
})
it('aborts a pending authoring observation on unmount without adopting or replaying it', async () => {
  let resolve!: (value: FilmcraftSubclipResult) => void; vi.mocked(editFilmcraftSubclip).mockImplementation(() => new Promise(accept => { resolve = accept })); const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); button('Создать подклип').click(); await tick(); const signal = vi.mocked(editFilmcraftSubclip).mock.calls[0][3]!
  await unmount(component); component = undefined; expect(signal.aborted).toBe(true); resolve(result); await tick(); expect(onedit).not.toHaveBeenCalled(); expect(editFilmcraftSubclip).toHaveBeenCalledTimes(1)
})

it('can reload native defaults to repair an invalid boundary draft', async () => {
  vi.mocked(getFilmcraftSubclipDefaults).mockResolvedValue(defaults)
  component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn() } }); await tick(); await check('Изменить начало подклипа Filmcraft'); await write('Начало подклипа Filmcraft', '9223372036854775808')
  expect(button('Создать подклип').disabled).toBe(true); expect(button('Показать исходные параметры подклипа').disabled).toBe(false)
  button('Показать исходные параметры подклипа').click(); await tick(); await vi.waitFor(() => expect(document.querySelector<HTMLInputElement>('[aria-label="Начало подклипа Filmcraft"]')!.value).toBe(defaults.startTicks)); expect(button('Создать подклип').disabled).toBe(false)
})
