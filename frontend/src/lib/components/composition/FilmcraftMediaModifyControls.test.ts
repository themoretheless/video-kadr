// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { modifyFilmcraftMedia, type FilmcraftMediaModifyResult } from '$lib/api'
import Controls from './FilmcraftMediaModifyControls.svelte'
vi.mock('$lib/api', () => ({ modifyFilmcraftMedia: vi.fn() }))
const props = { document: 'exact', sequenceId: '18446744073709551615', clipId: '9007199254740995', token: 'owner', selection: ['9007199254740995'], selected: ['9007199254740993'], items: [{ id: '9007199254740993', name: 'Source', kind: 'Media' as const }], sourceBindings: { '9007199254740993': 'asset' } }
const result = { documentText: 'changed', modifyEdit: true, modificationCount: 2, originalSchema: 12, sequences: [], media: [] } as FilmcraftMediaModifyResult
let component: ReturnType<typeof mount> | undefined
const button = (text: string) => [...document.querySelectorAll('button')].find(b => b.textContent === text)!
async function choose(label: string, value: string) { const select = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; select.value = value; select.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
it.each([['Применить каналы исходника', 'clip.audioChannels', { items: props.selected }], ['Создать моно-элементы', 'clip.breakoutToMono', { items: props.selected }], ['Применить каналы клипов', 'clip.audioChannels', { clips: props.selection, channels: [0, 1] }]] as const)('runs %s with separate exact selections and source authorization fence', async (label, command, options) => {
  vi.mocked(modifyFilmcraftMedia).mockResolvedValue(result); const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); button(label).click(); await tick(); await tick()
  expect(modifyFilmcraftMedia).toHaveBeenCalledWith('owner', command, { document: props.document, ...options, context: { activeSequence: props.sequenceId, selection: props.selection, projectSelection: props.selected } }, expect.any(AbortSignal))
  expect(onedit).toHaveBeenCalledExactlyOnceWith(result, { document: props.document, sequenceId: props.sequenceId, clipId: props.clipId, token: props.token, timelineSelection: props.selection, projectSelection: props.selected, sourceBindings: props.sourceBindings })
})
it('retains an exact frame value and allows reset without a replacement frame', async () => {
  vi.mocked(modifyFilmcraftMedia).mockResolvedValue(result); component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn() } }); await tick(); await choose('Исходник Modify Filmcraft', props.selected[0]); await choose('Правка таймкода Filmcraft', 'frame')
  const field = document.querySelector<HTMLInputElement>('[aria-label="Новый таймкод Filmcraft"]')!; field.value = '9007199254740997'; field.dispatchEvent(new Event('input', { bubbles: true })); await tick(); button('Применить таймкод').click(); await tick(); await tick()
  expect(modifyFilmcraftMedia).toHaveBeenLastCalledWith('owner', 'clip.modifyTimecode', expect.objectContaining({ item: props.selected[0], frame: '9007199254740997' }), expect.any(AbortSignal))
  await choose('Правка таймкода Filmcraft', 'reset'); button('Применить таймкод').click(); await tick()
  expect(modifyFilmcraftMedia).toHaveBeenLastCalledWith('owner', 'clip.modifyTimecode', expect.objectContaining({ reset: true }), expect.any(AbortSignal))
})
it('rejects late edits after a source binding changes', async () => {
  let resolve!: (result: FilmcraftMediaModifyResult) => void
  vi.mocked(modifyFilmcraftMedia).mockImplementation(() => new Promise(done => { resolve = done })); const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); button('Создать моно-элементы').click(); await tick(); props.sourceBindings[props.selected[0] as '9007199254740993'] = 'other'; resolve(result); await tick(); await tick(); expect(onedit).not.toHaveBeenCalled(); props.sourceBindings['9007199254740993'] = 'asset'
})
