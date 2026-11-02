// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftVideoOptions, type FilmcraftVideoOptionsResult } from '$lib/api'
import Controls from './FilmcraftVideoOptionsControls.svelte'
vi.mock('$lib/api', () => ({ editFilmcraftVideoOptions: vi.fn() }))
const props = { document: 'exact', sequenceId: '18446744073709551615', clipId: '9007199254740993', token: 'owner', selection: ['9007199254740993'], timeFrames: '7', timeTicks: '9007199254740999' }
const result = { documentText: 'changed', videoOptionsEdit: true, videoOptionsClipCount: 0, originalSchema: 12, sequences: [], media: [] } as FilmcraftVideoOptionsResult
let component: ReturnType<typeof mount> | undefined
const button = (text: string) => [...document.querySelectorAll('button')].find(b => b.textContent === text)!
async function choose(label: string, value: string) { const select = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; select.value = value; select.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
it.each([['Применить стоп-кадр', 'clip.frameHoldOptions', { enabled: true, holdFilters: false, holdOn: 'in' }], ['Применить параметры полей', 'clip.fieldOptions', { processing: 'none', reverseFieldDominance: false }], ['Применить интерполяцию', 'clip.setTimeInterpolation', { mode: 'frameSampling' }], ['Вписать в кадр', 'clip.fitToFrame', {}], ['Заполнить кадр', 'clip.fillFrame', {}]] as const)('runs %s once with exact video context and adopts no-op marker', async (label, command, options) => {
  vi.mocked(editFilmcraftVideoOptions).mockResolvedValue(result); const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); button(label).click(); await tick(); await tick()
  expect(editFilmcraftVideoOptions).toHaveBeenCalledWith('owner', command, { document: props.document, sequence: props.sequenceId, clips: props.selection, ...options, context: { activeSequence: props.sequenceId, selection: props.selection, playhead: props.timeTicks } }, expect.any(AbortSignal))
  expect(onedit).toHaveBeenCalledExactlyOnceWith(result, { document: props.document, sequenceId: props.sequenceId, clipId: props.clipId, token: props.token, timelineSelection: props.selection, timeFrames: props.timeFrames, timeTicks: props.timeTicks })
})
it('retains exact hold ticks and resets incompatible time format when switching to source timecode', async () => {
  vi.mocked(editFilmcraftVideoOptions).mockResolvedValue(result); component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn() } }); await tick()
  await choose('Режим стоп-кадра Filmcraft', 'sequenceTime'); await choose('Формат времени стоп-кадра Filmcraft', 'frame'); await choose('Режим стоп-кадра Filmcraft', 'sourceTimecode')
  const value = document.querySelector<HTMLInputElement>('[aria-label="Время стоп-кадра Filmcraft"]')!; value.value = '9007199254740997'; value.dispatchEvent(new Event('input', { bubbles: true })); await tick(); button('Применить стоп-кадр').click(); await tick()
  expect(editFilmcraftVideoOptions).toHaveBeenCalledWith('owner', 'clip.frameHoldOptions', expect.objectContaining({ time: '9007199254740997', holdOn: 'sourceTimecode' }), expect.any(AbortSignal))
})
it('rejects a stale edit after selection changes', async () => {
  let resolve!: (result: FilmcraftVideoOptionsResult) => void
  vi.mocked(editFilmcraftVideoOptions).mockImplementation(() => new Promise(done => { resolve = done })); const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); button('Вписать в кадр').click(); await tick(); props.selection.push('9007199254740995'); resolve(result); await tick(); await tick(); expect(onedit).not.toHaveBeenCalled(); props.selection.pop()
})
