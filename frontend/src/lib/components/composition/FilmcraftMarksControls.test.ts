// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftSource, type FilmcraftSourcePlacementResult, navigateFilmcraft, type FilmcraftNavigationResult, editFilmcraftMarks, type FilmcraftMarkEditResult } from '$lib/api'
import Controls from './FilmcraftMarksControls.svelte'
vi.mock('$lib/api', () => ({ editFilmcraftSource: vi.fn(), navigateFilmcraft: vi.fn(), editFilmcraftMarks: vi.fn() }))
const props = { document: 'exact', sequenceId: '18446744073709551615', clipId: '9007199254740995', token: 'owner', items: [{ id: '9007199254740993', name: 'Source', kind: 'Media' as const }], timeFrames: '9', timeTicks: '9007199254740997' }
const result: FilmcraftMarkEditResult = { documentText: 'changed', originalSchema: 12, sequences: [], media: [], markEdit: true }
let component: ReturnType<typeof mount> | undefined
const button = (label: string) => [...document.querySelectorAll('button')].find(b => b.textContent === label)!
async function select(label: string, value: string) { const element = document.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!; element.value = value; element.dispatchEvent(new Event('change', { bubbles: true })); await tick() }
async function position(value: string) { const element = document.querySelector<HTMLInputElement>('[aria-label="Позиция точки Filmcraft"]')!; element.value = value; element.dispatchEvent(new Event('input', { bubbles: true })); await tick() }
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
it.each([['Поставить In', 'markers.markIn'], ['Поставить Out', 'markers.markOut'], ['Видео In', 'markers.markSplitVideoIn'], ['Видео Out', 'markers.markSplitVideoOut'], ['Аудио In', 'markers.markSplitAudioIn'], ['Аудио Out', 'markers.markSplitAudioOut']] as const)('dispatches %s with exact tick and document/clock fences', async (label, command) => {
  vi.mocked(editFilmcraftMarks).mockResolvedValue(result); const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); await position('-9007199254740997'); button(label).click(); await tick()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(result, { document: props.document, sequenceId: props.sequenceId, clipId: props.clipId, token: 'owner', timeFrames: '9', timeTicks: props.timeTicks }))
  expect(editFilmcraftMarks).toHaveBeenCalledWith('owner', command, { document: 'exact', target: 'program', time: '-9007199254740997', context: { activeSequence: props.sequenceId, sourceItem: null } }, expect.any(AbortSignal))
})
it.each([['Очистить In', { in: null }], ['Очистить Out', { out: null }], ['Очистить In/Out', { in: null, out: null }]] as const)('clears only requested source sides (%s)', async (label, marks) => {
  vi.mocked(editFilmcraftMarks).mockResolvedValue(result)
  component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn() } }); await tick(); await select('Монитор точек Filmcraft', 'source')
  expect(button(label).disabled).toBe(true)
  await select('Исходник точек Filmcraft', props.items[0].id); button(label).click(); await tick()
  expect(editFilmcraftMarks).toHaveBeenCalledWith('owner', 'project.setMarks', { document: 'exact', item: props.items[0].id, ...marks, context: { activeSequence: props.sequenceId, sourceItem: props.items[0].id } }, expect.any(AbortSignal))
})
it.each([['frames', 'frame', '9007199254740997'], ['seconds', 'seconds', 1.25], ['timecode', 'timecode', '+15']] as const)('sends %s positions for native resolution', async (unit, key, value) => {
  vi.mocked(editFilmcraftMarks).mockResolvedValue(result); component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn() } }); await tick()
  await select('Единицы точки Filmcraft', unit); await position(String(value)); button('Поставить In').click(); await tick()
  expect(editFilmcraftMarks).toHaveBeenCalledWith('owner', 'markers.markIn', expect.objectContaining({ [key]: value }), expect.any(AbortSignal))
})
it('rejects out-of-range tick input locally', async () => {
  component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn() } }); await tick(); await position('9223372036854775808'); expect(button('Поставить In').disabled).toBe(true); expect(button('Очистить In').disabled).toBe(false); expect(editFilmcraftMarks).not.toHaveBeenCalled()
})
it('refuses a late response after the target changes and never replays', async () => {
  let resolve!: (value: FilmcraftMarkEditResult) => void; vi.mocked(editFilmcraftMarks).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); button('Поставить In').click(); await tick()
  await position('17'); resolve(result); await tick(); await tick(); expect(onedit).not.toHaveBeenCalled(); expect(editFilmcraftMarks).toHaveBeenCalledTimes(1)
})
it('aborts observation on unmount without adopting or replaying a late mutation', async () => {
  let resolve!: (value: FilmcraftMarkEditResult) => void; vi.mocked(editFilmcraftMarks).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit } }); await tick(); button('Поставить In').click(); await tick(); const signal = vi.mocked(editFilmcraftMarks).mock.calls[0][3]!
  await unmount(component); component = undefined; expect(signal.aborted).toBe(true); resolve(result); await tick(); expect(onedit).not.toHaveBeenCalled(); expect(editFilmcraftMarks).toHaveBeenCalledTimes(1)
})

it('shows source and sequence marks from imported metadata without parsing the project document', async () => {
  const marks = { in: '9007199254740997', out: null, split: { videoIn: null, videoOut: null, audioIn: '-9007199254740997', audioOut: null } }
  component = mount(Controls, { target: document.body, props: { ...props, marks, items: [{ ...props.items[0], sourceMarks: { ...marks, in: '17' } }], onedit: vi.fn() } }); await tick()
  expect(document.querySelector('[aria-label="Текущие точки Filmcraft"]')?.textContent).toContain(marks.in)
  await select('Монитор точек Filmcraft', 'source'); await select('Исходник точек Filmcraft', props.items[0].id)
  expect(document.querySelector('[aria-label="Текущие точки Filmcraft"]')?.textContent).toContain('17')
  expect(document.querySelector('[aria-label="Текущие точки Filmcraft"]')?.textContent).toContain(marks.split.audioIn)
})

it.each([['К In таймлайна', 'markers.goToIn'], ['К Out таймлайна', 'markers.goToOut'], ['К видео In', 'markers.goToSplitVideoIn'], ['К видео Out', 'markers.goToSplitVideoOut'], ['К аудио In', 'markers.goToSplitAudioIn'], ['К аудио Out', 'markers.goToSplitAudioOut']] as const)('navigates %s without invoking document edits', async (label, command) => {
  const context = { activeSequence: props.sequenceId, sourceItem: null, sourcePlayhead: '0', playhead: '9007199254740997', playheadFrames: '12', selection: [props.clipId], projectSelection: [] }
  vi.mocked(navigateFilmcraft).mockResolvedValue({ context, events: [] }); const oncontext = vi.fn().mockReturnValue(true); const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, selection: [props.clipId], onedit, oncontext } }); await tick(); button(label).click(); await tick()
  await vi.waitFor(() => expect(oncontext).toHaveBeenCalledOnce())
  expect(navigateFilmcraft).toHaveBeenCalledWith('owner', command, expect.objectContaining({ document: 'exact', target: 'program', context: { activeSequence: props.sequenceId, sourceItem: null, sourcePlayhead: '0', playhead: props.timeTicks, selection: [props.clipId], projectSelection: [], linkedSelection: true, selectionFollowsPlayhead: false, stillImageDuration: 5, stillImageUnit: 'seconds' } }), expect.any(AbortSignal))
  expect(onedit).not.toHaveBeenCalled(); expect(editFilmcraftMarks).not.toHaveBeenCalled()
})
it('opens a source and seeks it with exact raw ticks', async () => {
  const context = { activeSequence: props.sequenceId, sourceItem: props.items[0].id, sourcePlayhead: '0', playhead: '0', playheadFrames: '0', selection: [], projectSelection: [] }
  vi.mocked(navigateFilmcraft).mockResolvedValue({ context, events: [] }); const oncontext = vi.fn().mockReturnValue(true)
  component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn(), oncontext } }); await tick(); await select('Монитор точек Filmcraft', 'source'); await select('Исходник точек Filmcraft', props.items[0].id)
  button('Открыть исходник').click(); await tick(); await vi.waitFor(() => expect(oncontext).toHaveBeenCalledOnce())
  expect(navigateFilmcraft).toHaveBeenLastCalledWith('owner', 'source.open', expect.objectContaining({ item: props.items[0].id }), expect.any(AbortSignal))
  const seek = document.querySelector<HTMLInputElement>('[aria-label="Позиция перехода исходника Filmcraft"]')!; seek.value = '9007199254740997'; seek.dispatchEvent(new Event('input', { bubbles: true })); await tick(); button('Перейти в исходнике').click(); await tick()
  await vi.waitFor(() => expect(navigateFilmcraft).toHaveBeenCalledTimes(2))
  expect(navigateFilmcraft).toHaveBeenLastCalledWith('owner', 'source.setPlayhead', expect.objectContaining({ time: '9007199254740997' }), expect.any(AbortSignal))
})
it('discards late navigation after local scope changes and does not create a document edit', async () => {
  let resolve!: (result: FilmcraftNavigationResult) => void; vi.mocked(navigateFilmcraft).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const oncontext = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn(), oncontext } }); await tick(); button('К In таймлайна').click(); await tick()
  await position('17'); resolve({ context: { activeSequence: props.sequenceId, sourceItem: null, sourcePlayhead: '0', playhead: '0', playheadFrames: '0', selection: [], projectSelection: [] }, events: [] }); await tick(); await tick()
  expect(oncontext).not.toHaveBeenCalled(); expect(navigateFilmcraft).toHaveBeenCalledTimes(1)
})
it('aborts pending navigation on unmount and never adopts its late context', async () => {
  let resolve!: (value: FilmcraftNavigationResult) => void; vi.mocked(navigateFilmcraft).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const oncontext = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, onedit: vi.fn(), oncontext } }); await tick(); button('К In таймлайна').click(); await tick()
  const signal = vi.mocked(navigateFilmcraft).mock.calls[0][3]!
  await unmount(component); component = undefined; expect(signal.aborted).toBe(true)
  resolve({ context: { activeSequence: props.sequenceId, sourceItem: null, sourcePlayhead: '0', playhead: '0', playheadFrames: '0', selection: [], projectSelection: [] }, events: [] }); await tick()
  expect(oncontext).not.toHaveBeenCalled(); expect(navigateFilmcraft).toHaveBeenCalledTimes(1)
})

it.each([['frames', 'frame', '9007199254740997'], ['seconds', 'seconds', 1.25]] as const)('resolves source seek %s in native code', async (unit, key, value) => {
  vi.mocked(navigateFilmcraft).mockResolvedValue({ context: { activeSequence: props.sequenceId, sourceItem: props.items[0].id, sourcePlayhead: '0', playhead: '0', playheadFrames: '0', selection: [], projectSelection: [] }, events: [] })
  component = mount(Controls, { target: document.body, props: { ...props, item: props.items[0].id, onedit: vi.fn(), oncontext: vi.fn().mockReturnValue(true) } }); await tick(); await select('Монитор точек Filmcraft', 'source'); await select('Единицы перехода исходника Filmcraft', unit)
  const seek = document.querySelector<HTMLInputElement>('[aria-label="Позиция перехода исходника Filmcraft"]')!; seek.value = String(value); seek.dispatchEvent(new Event('input', { bubbles: true })); await tick(); button('Перейти в исходнике').click(); await tick()
  expect(navigateFilmcraft).toHaveBeenCalledWith('owner', 'source.setPlayhead', expect.objectContaining({ [key]: value }), expect.any(AbortSignal))
})

it.each([['Вставить исходник', 'source.insert'], ['Записать исходник поверх', 'source.overwrite']] as const)('dispatches %s with source patching and one fenced authoring adoption', async (label, command) => {
  const tracks = [{ id: '9007199254740999', name: 'V1', kind: 'video_tracks' as const, locked: false }, { id: '9007199254740997', name: 'A1', kind: 'audio_tracks' as const, locked: false }]
  const edited: FilmcraftSourcePlacementResult = { ...result, sourceEdit: true, insertedClipIds: ['18446744073709551614'], sourceEditContext: { activeSequence: props.sequenceId, sourceItem: props.items[0].id, sourcePlayhead: '333', playhead: '9007199254740997', playheadFrames: '51', selection: ['18446744073709551614'] } }
  vi.mocked(editFilmcraftSource).mockResolvedValue(edited); const onedit = vi.fn().mockReturnValue(true)
  component = mount(Controls, { target: document.body, props: { ...props, tracks, item: props.items[0].id, sourcePlayhead: '333', selection: [props.clipId], onedit, oncontext: vi.fn() } }); await tick(); await select('Монитор точек Filmcraft', 'source')
  document.querySelector<HTMLInputElement>('[aria-label="Аудио исходника Filmcraft"]')!.click(); document.querySelector<HTMLInputElement>('[aria-label="Секвенция как клипы Filmcraft"]')!.click(); document.querySelector<HTMLInputElement>('[aria-label="Сдвиг маркеров при вставке Filmcraft"]')!.click(); await tick(); await select('Масштабирование исходника Filmcraft', 'setToFrameSize')
  button(label).click(); await tick(); await vi.waitFor(() => expect(onedit).toHaveBeenCalledOnce())
  expect(editFilmcraftSource).toHaveBeenCalledWith('owner', command, { document: 'exact', context: { activeSequence: props.sequenceId, sourceItem: props.items[0].id, sourcePlayhead: '333', playhead: props.timeTicks, targeting: { videoDest: tracks[0].id, audioDest: null }, sequencesAsClips: true, rippleSequenceMarkers: true, selectionFollowsPlayhead: false, linkedSelection: true, defaultMediaScaling: 'setToFrameSize', stillImageDuration: 5, stillImageUnit: 'seconds' } }, expect.any(AbortSignal))
  expect(onedit).toHaveBeenCalledWith(edited, expect.objectContaining({ sourceItem: props.items[0].id, sourcePlayhead: '333', timelineSelection: [props.clipId], projectSelection: [] }))
})
it('keeps the document when source patching fails and never retries the mutation', async () => {
  vi.mocked(editFilmcraftSource).mockRejectedValue(new Error('placement track is locked')); const onedit = vi.fn()
  component = mount(Controls, { target: document.body, props: { ...props, item: props.items[0].id, onedit, oncontext: vi.fn() } }); await tick(); await select('Монитор точек Filmcraft', 'source'); button('Вставить исходник').click(); await tick()
  await vi.waitFor(() => expect(document.querySelector('[role="alert"]')?.textContent).toContain('locked')); expect(onedit).not.toHaveBeenCalled(); expect(editFilmcraftSource).toHaveBeenCalledTimes(1)
})
it('rejects a source insertion reply after the control preferences change', async () => {
  let resolve!: (value: FilmcraftSourcePlacementResult) => void; vi.mocked(editFilmcraftSource).mockImplementation(() => new Promise(accept => { resolve = accept }))
  const onedit = vi.fn(); component = mount(Controls, { target: document.body, props: { ...props, item: props.items[0].id, onedit, oncontext: vi.fn() } }); await tick(); await select('Монитор точек Filmcraft', 'source'); button('Вставить исходник').click(); await tick(); await select('Масштабирование исходника Filmcraft', 'scaleToFrameSize')
  resolve({ ...result, sourceEdit: true, insertedClipIds: [], sourceEditContext: { activeSequence: props.sequenceId, sourceItem: props.items[0].id, sourcePlayhead: '0', playhead: '0', playheadFrames: '0', selection: [] } }); await tick(); await tick(); expect(onedit).not.toHaveBeenCalled(); expect(editFilmcraftSource).toHaveBeenCalledTimes(1)
})

it.each(['program', 'source'] as const)('marks the current %s clock without re-entering its tick value', async target => {
  vi.mocked(editFilmcraftMarks).mockResolvedValue(result)
  component = mount(Controls, { target: document.body, props: { ...props, item: props.items[0].id, sourcePlayhead: '333', onedit: vi.fn(), oncontext: vi.fn() } }); await tick(); await select('Монитор точек Filmcraft', target)
  document.querySelector<HTMLInputElement>('[aria-label="По позиции монитора Filmcraft"]')!.click(); await tick(); await position('not used')
  button('Поставить In').click(); await tick()
  expect(editFilmcraftMarks).toHaveBeenCalledWith('owner', 'markers.markIn', expect.objectContaining({ time: target === 'source' ? '333' : props.timeTicks }), expect.any(AbortSignal))
})
