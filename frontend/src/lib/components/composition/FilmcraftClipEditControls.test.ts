// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftClips, type FilmcraftClipEditResult } from '$lib/api'
import Controls from './FilmcraftClipEditControls.svelte'
vi.mock('$lib/api', () => ({ editFilmcraftClips: vi.fn() }))
const props = { document: 'exact', sequenceId: '18446744073709551615', clipId: '9007199254740993', token: 'owner', selection: ['9007199254740993'], timeFrames: '7', timeTicks: '9007199254740999', linked: true }
const result = { documentText: 'changed', clipEdit: true, originalSchema: 12, sequences: [], media: [] } as FilmcraftClipEditResult
let component: ReturnType<typeof mount> | undefined
const button = (text: string) => [...document.querySelectorAll('button')].find(b => b.textContent === text)!
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
it.each([['Применить скорость', 'clip.speedDuration', { clips: props.selection, speed: 100, reverse: false, ripple: false }], ['Включить / выключить клипы', 'clip.enable', { clips: props.selection }], ['Связать / разорвать связь', 'clip.link', { clips: props.selection }], ['Сгруппировать клипы', 'clip.group', {}], ['Разгруппировать клипы', 'clip.ungroup', {}], ['Переключить масштаб по кадру', 'clip.scaleToFrameSize', {}], ['Стоп-кадр от курсора до конца', 'clip.frameHold', { time: props.timeTicks }], ['Вставить сегмент стоп-кадра', 'clip.insertFrameHoldSegment', { time: props.timeTicks, seconds: 2 }]] as const)('runs %s with exact documented targets and one fenced edit', async (label, command, options) => {
  vi.mocked(editFilmcraftClips).mockResolvedValue(result); const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); button(label).click(); await tick(); await tick()
  expect(editFilmcraftClips).toHaveBeenCalledWith('owner', command, { document: props.document, sequence: props.sequenceId, ...options, context: { activeSequence: props.sequenceId, selection: props.selection, linkedSelection: true, playhead: props.timeTicks } }, expect.any(AbortSignal))
  expect(onedit).toHaveBeenCalledExactlyOnceWith(result, { document: props.document, sequenceId: props.sequenceId, clipId: props.clipId, token: props.token, timelineSelection: props.selection, timeFrames: props.timeFrames, timeTicks: props.timeTicks })
})
it('allows native under-playhead fallback with no selected clips and sends an explicit position before hold duration', async () => {
  vi.mocked(editFilmcraftClips).mockResolvedValue(result); component = mount(Controls, { target: document.body, props: { ...props, selection: [], clipId: '', timeTicks: undefined, onedit: vi.fn() } }); await tick(); button('Вставить сегмент стоп-кадра').click(); await tick()
  expect(editFilmcraftClips).toHaveBeenCalledWith('owner', 'clip.insertFrameHoldSegment', expect.objectContaining({ frame: '7', seconds: 2, context: expect.objectContaining({ selection: [] }) }), expect.any(AbortSignal))
})
it('rejects a stale response after a selection change', async () => {
  let resolve!: (result: FilmcraftClipEditResult) => void; vi.mocked(editFilmcraftClips).mockImplementation(() => new Promise(done => { resolve = done })); const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); button('Применить скорость').click(); await tick(); props.selection.push('9007199254740995'); resolve(result); await tick(); await tick(); expect(onedit).not.toHaveBeenCalled(); props.selection.pop()
})
