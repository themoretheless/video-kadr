// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { renameFilmcraftClip, startFilmcraftMaskTrackingJob, readFilmcraftTrackingJob, cancelFilmcraftTrackingJob, importFilmcraftTrackingSnapshot, editFilmcraftAnimation, setFilmcraftEffectParameter, applyFilmcraftEffect, listFilmcraftEffects, editFilmcraftEffectRack, listFilmcraftExportQueue, editFilmcraftMaskMonitor, trackFilmcraftMask, getFilmcraftParameters, editFilmcraftParameter, editFilmcraftMask, createFilmcraftTrimSession, commandFilmcraftTrimSession, readFilmcraftTrimSession, closeFilmcraftTrimSession, trimFilmcraftToPlayhead, trimFilmcraftEditPoints, getFilmcraftNearestEditPoints, getFilmcraftTrimMonitor, applyFilmcraftSelectionTransitions, setFilmcraftTransition, getFilmcraftTransitions, applyFilmcraftTransition, importFilmcraftProject, listFilmcraftVideoJobs, exportFilmcraftAudio, exportFilmcraftFrame, trimFilmcraftProject, slipFilmcraftProject, stretchFilmcraftProject, rollFilmcraftProject, slideFilmcraftProject, deleteFilmcraftClips, closeFilmcraftGap, razorFilmcraftProject, removeFilmcraftRange, copyFilmcraftClips, pasteFilmcraftClips } from '$lib/api'
import FilmcraftProjectPanel from './FilmcraftProjectPanel.svelte'
import type { MediaEntry } from '$lib/types'
import type { FilmcraftImportResult } from '$lib/api'
// Vitest resolves the public Svelte entry to SSR; interaction effects need
// the client untrack implementation used by the actual browser component.
vi.mock('svelte', async importOriginal => {
  const actual = await importOriginal<typeof import('svelte')>()
  // @ts-expect-error The explicit client entry has no public declaration file.
  const client = await import('../../../../node_modules/svelte/src/index-client.js')
  return { ...actual, untrack: client.untrack }
})
vi.mock('$lib/api', () => ({ renameFilmcraftClip: vi.fn(), startFilmcraftMaskTrackingJob: vi.fn(), readFilmcraftTrackingJob: vi.fn(), cancelFilmcraftTrackingJob: vi.fn(), importFilmcraftTrackingSnapshot: vi.fn(), editFilmcraftAnimation: vi.fn(), setFilmcraftEffectParameter: vi.fn(), applyFilmcraftEffect: vi.fn(), listFilmcraftEffects: vi.fn(), editFilmcraftEffectRack: vi.fn(), editFilmcraftMaskMonitor: vi.fn(), trackFilmcraftMask: vi.fn(), editFilmcraftMask: vi.fn(), getFilmcraftParameters: vi.fn(), editFilmcraftParameter: vi.fn(), getFilmcraftPreviewFrame: vi.fn(), createFilmcraftTrimSession: vi.fn(), commandFilmcraftTrimSession: vi.fn(), readFilmcraftTrimSession: vi.fn(), closeFilmcraftTrimSession: vi.fn(), trimFilmcraftToPlayhead: vi.fn(), trimFilmcraftEditPoints: vi.fn(), getFilmcraftNearestEditPoints: vi.fn(), getFilmcraftTrimMonitor: vi.fn(), applyFilmcraftSelectionTransitions: vi.fn(), setFilmcraftTransition: vi.fn(), getFilmcraftTransitions: vi.fn(), applyFilmcraftTransition: vi.fn(), importFilmcraftProject: vi.fn(), exportFilmcraftAudio: vi.fn(), exportFilmcraftFrame: vi.fn(), startFilmcraftVideoJob: vi.fn(), getFilmcraftVideoJob: vi.fn(), cancelFilmcraftVideoJob: vi.fn(), retryFilmcraftVideoJob: vi.fn(), downloadFilmcraftVideo: vi.fn(), listFilmcraftExportQueue: vi.fn().mockResolvedValue({ running: false, items: [] }), editFilmcraftExportQueue: vi.fn(), listFilmcraftVideoJobs: vi.fn(), trimFilmcraftProject: vi.fn(), slipFilmcraftProject: vi.fn(), stretchFilmcraftProject: vi.fn(), rollFilmcraftProject: vi.fn(), slideFilmcraftProject: vi.fn(), deleteFilmcraftClips: vi.fn(), closeFilmcraftGap: vi.fn(), razorFilmcraftProject: vi.fn(), removeFilmcraftRange: vi.fn(), copyFilmcraftClips: vi.fn(), pasteFilmcraftClips: vi.fn() }))
beforeEach(() => { vi.mocked(listFilmcraftExportQueue).mockResolvedValue({ running: false, items: [] }) })
let component: ReturnType<typeof mount> | undefined
const sourceText = '{"id":18446744073709551615}'
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.restoreAllMocks(); vi.resetAllMocks(); document.body.innerHTML = '' })
async function open(text = sourceText, group = false, emptyTarget = false, library: MediaEntry[] = [{ id: 'asset', kind: 'source', filename: 'pattern.wav', url: '' } as MediaEntry]) {
  vi.mocked(listFilmcraftVideoJobs).mockResolvedValue({ jobs: [], nextCursor: null })
  vi.mocked(importFilmcraftProject).mockResolvedValue({ documentText: sourceText, originalSchema: 12, sequences: [{ id: '18446744073709551615', name: 'Scene', sampleRate: 48000, durationSeconds: 2, settings: {}, clips: [{ id: '9007199254740995', name: 'Shot', linkId: group ? '18446744073709551615' : null, trackId: '3', trackKind: 'video_tracks', locked: false, startTicks: '9007199254740993', durationTicks: '254016000000' }, { id: '9007199254740997', name: 'Right shot', trackId: '3', trackKind: 'video_tracks', locked: false, startTicks: '9007453270740993', durationTicks: '254016000000' }, ...(group ? [{ id: '9007199254740999', name: 'Sound partner', linkId: '18446744073709551615', trackId: '4', trackKind: 'audio_tracks', locked: false, startTicks: '9007199254740993', durationTicks: '254016000000' }] : [])] }, ...(emptyTarget ? [{ id: '9007199254740990', name: 'Empty target', sampleRate: 48000, durationSeconds: 0, settings: {}, clips: [] }] : [])], media: [{ id: '9007199254740993', name: 'Wave', path: '/original/pattern.wav', offline: false }] })
  component = mount(FilmcraftProjectPanel, { target: document.body, props: { token: 'owner', library } })
  const file = new File([text], 'project.json', { type: 'application/json' })
  Object.defineProperty(file, 'text', { value: async () => text })
  const input = document.querySelector<HTMLInputElement>('input[type=file]')!
  Object.defineProperty(input, 'files', { configurable: true, value: [file] })
  input.dispatchEvent(new Event('change', { bubbles: true }))
  await vi.waitFor(() => expect(document.querySelector('button')).not.toBeNull()); await tick()
}
it('opens source text, binds unique WAV and exports selected range without integer loss', async () => {
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:export')
  const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  vi.mocked(exportFilmcraftAudio).mockResolvedValue(new Blob(['WAV']))
  await open()
  document.querySelector('button')!.click()
  await vi.waitFor(() => expect(document.querySelector('a')?.getAttribute('href')).toBe('blob:export'))
  expect(exportFilmcraftAudio).toHaveBeenCalledWith({ document: sourceText, sequenceId: '18446744073709551615', startSample: 0, frames: 96000, channels: 2, bits: 24, format: 'wav', sources: { '9007199254740993': 'asset' } }, 'owner', expect.any(AbortSignal))
  await unmount(component!); component = undefined
  expect(revoke).toHaveBeenCalledWith('blob:export')
})
it('shows export failures and never offers a stale download', async () => {
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('Источник не найден'))
  await open(); document.querySelector('button')!.click()
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('Источник не найден'))
  expect(document.querySelector('a')).toBeNull()
})

it('downloads the preserved document and a portable session with exact source IDs', async () => {
  const blobs: Blob[] = []
  vi.spyOn(URL, 'createObjectURL').mockImplementation((blob) => { blobs.push(blob as Blob); return `blob:saved-${blobs.length}` })
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
  await open()
  const button = (label: string) => [...document.querySelectorAll('button')].find((item) => item.textContent === label)!
  button('Скачать проект').click()
  button('Сохранить рабочую сессию').click()
  expect(await blobs[0]!.text()).toBe(sourceText)
  const saved = JSON.parse(await blobs[1]!.text())
  expect(saved.documentText).toBe(sourceText)
  expect(saved.sequenceId).toBe('18446744073709551615')
  expect(saved.sources).toEqual({ '9007199254740993': 'asset' })
  expect(saved).toMatchObject({ start: 0, end: 2, audioFormat: 'wav', bits: 24, channels: 2 })
})

it('reopens session settings and exposes unavailable bindings for replacement', async () => {
  vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:restored')
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  vi.mocked(exportFilmcraftAudio).mockResolvedValue(new Blob(['AIFF']))
  await open(JSON.stringify({ format: 'video-kadr.filmcraft-session', version: 1, documentText: sourceText, sequenceId: '18446744073709551615', sources: { '9007199254740993': 'previous-library-id' }, start: 0.25, end: 1.5, audioFormat: 'aiff', bits: 16, channels: 1 }))
  expect(importFilmcraftProject).toHaveBeenCalledWith(sourceText, expect.any(AbortSignal))
  expect(document.body.textContent).toContain('Файл недоступен — выберите замену')
  document.querySelector('button')!.click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenCalled())
  expect(exportFilmcraftAudio).toHaveBeenCalledWith(expect.objectContaining({ sequenceId: '18446744073709551615', startSample: 12000, frames: 60000, format: 'aiff', bits: 16, channels: 1, sources: { '9007199254740993': 'previous-library-id' } }), 'owner', expect.any(AbortSignal))
})

it('trims exact clip IDs and restores source text with undo/redo; failure keeps edits', async () => {
  await open()
  const original = vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const loaded = await original
  const editedText = '{"id":18446744073709551615,"edited":true}'
  vi.mocked(trimFilmcraftProject).mockResolvedValue({ ...loaded, documentText: editedText, appliedDeltaTicks: '10584000000' })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const button = (label: string) => [...document.querySelectorAll('button')].find((item) => item.textContent === label)!
  button('Обрезать').click()
  await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
  expect(trimFilmcraftProject).toHaveBeenCalledWith({ document: sourceText, sequenceId: '18446744073709551615', clipId: '9007199254740995', edge: 'out', mode: 'regular', deltaFrames: '1', linked: true }, expect.any(AbortSignal))
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('export fixture'))
  button('Отменить правку').click(); await tick(); button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(button('Повторить правку').disabled).toBe(false))
  button('Повторить правку').click(); await tick()
  vi.mocked(trimFilmcraftProject).mockRejectedValue(new Error('trim refused'))
  const mode = document.querySelector<HTMLSelectElement>('[aria-label="Режим обрезки"]')!
  mode.value = 'ripple'; mode.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  button('Обрезать').click()
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('trim refused'))
  expect(trimFilmcraftProject).toHaveBeenLastCalledWith(expect.objectContaining({ mode: 'ripple', document: editedText }), expect.any(AbortSignal))
  button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: editedText }), 'owner', expect.any(AbortSignal)))
})

it('shifts only the chosen source clip and offers undo', async () => {
 await open()
 const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
 vi.mocked(slipFilmcraftProject).mockResolvedValue({ ...loaded, documentText: '{"slipped":true}', appliedDeltaTicks: '10584000000' })
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll('button')].find((item) => item.textContent === label)!
 button('Сдвинуть исходный участок').click()
 await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
 expect(slipFilmcraftProject).toHaveBeenCalledWith({ document: sourceText, sequenceId: '18446744073709551615', clipId: '9007199254740995', deltaFrames: '1' }, expect.any(AbortSignal))
 expect(trimFilmcraftProject).not.toHaveBeenCalled()
})

it('stretches the selected edge with exact IDs and commits to history', async () => {
 await open()
 const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
 vi.mocked(stretchFilmcraftProject).mockResolvedValue({ ...loaded, documentText: '{"stretched":true}', appliedSpeed: 0.8 })
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll('button')].find((item) => item.textContent === label)!
 button('Растянуть со сменой скорости').click()
 await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
 expect(stretchFilmcraftProject).toHaveBeenCalledWith({ document: sourceText, sequenceId: '18446744073709551615', clipId: '9007199254740995', edge: 'out', deltaFrames: '1' }, expect.any(AbortSignal))
 expect(trimFilmcraftProject).not.toHaveBeenCalled(); expect(slipFilmcraftProject).not.toHaveBeenCalled()
})

it('finds the exact adjacent right clip above the JS integer limit and rolls its cut', async () => {
 await open()
 const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
 vi.mocked(rollFilmcraftProject).mockResolvedValue({ ...loaded, documentText: '{"rolled":true}', appliedDeltaTicks: '10584000000' })
 const left = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 left.value = '9007199254740995'; left.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const right = document.querySelector<HTMLSelectElement>('[aria-label="Правый клип стыка"]')!
 expect([...right.options].map((option) => option.value)).toEqual(['', '9007199254740997'])
 right.value = '9007199254740997'; right.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll('button')].find((item) => item.textContent === label)!
 button('Сдвинуть стык').click()
 await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
 expect(rollFilmcraftProject).toHaveBeenCalledWith({ document: sourceText, sequenceId: '18446744073709551615', leftClipId: '9007199254740995', rightClipId: '9007199254740997', deltaFrames: '1' }, expect.any(AbortSignal))
})

it('slides one clip with exact IDs and commits to undo history', async () => {
 await open()
 const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
 vi.mocked(slideFilmcraftProject).mockResolvedValue({ ...loaded, documentText: '{"slid":true}', appliedDeltaTicks: '10584000000' })
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll('button')].find((item) => item.textContent === label)!
 button('Переместить с подрезкой соседей').click()
 await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
 expect(slideFilmcraftProject).toHaveBeenCalledWith({ document: sourceText, sequenceId: '18446744073709551615', clipId: '9007199254740995', deltaFrames: '1' }, expect.any(AbortSignal))
 expect(trimFilmcraftProject).not.toHaveBeenCalled(); expect(slipFilmcraftProject).not.toHaveBeenCalled()
})

it('deletes the chosen clip, clears stale selection and restores the document with undo', async () => {
 await open()
 const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
 const sequences = loaded.sequences.map((sequence) => ({ ...sequence, clips: sequence.clips?.filter((clip) => clip.id !== '9007199254740995') }))
 vi.mocked(deleteFilmcraftClips).mockResolvedValue({ ...loaded, sequences, documentText: '{"deleted":true}', removed: 1, closedSpans: [] })
 const select = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 select.value = '9007199254740995'; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll('button')].find((item) => item.textContent === label)!
 button('Удалить клип со сдвигом').click()
 await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
 expect(deleteFilmcraftClips).toHaveBeenCalledWith({ document: sourceText, sequenceId: '18446744073709551615', clipIds: ['9007199254740995'], ripple: true }, expect.any(AbortSignal))
 expect(select.value).toBe(''); expect(button('Удалить клип с зазором').disabled).toBe(true)
 button('Отменить правку').click(); await tick()
 expect([...select.options].some((option) => option.value === '9007199254740995')).toBe(true)
 select.value = '9007199254740995'; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 vi.mocked(deleteFilmcraftClips).mockRejectedValue(new Error('locked'))
 button('Удалить клип с зазором').click()
 await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('locked'))
 expect(deleteFilmcraftClips).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText, ripple: false }), expect.any(AbortSignal))
 expect(select.value).toBe('9007199254740995')
})

it('combines explicit multi-selection and exact linked partners, with an opt-out', async () => {
 await open(sourceText, true)
 vi.mocked(deleteFilmcraftClips).mockRejectedValue(new Error('keep selection for test'))
 const left = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 left.value = '9007199254740995'; left.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const extras = document.querySelector<HTMLSelectElement>('[aria-label="Дополнительные клипы"]')!
 for (const option of extras.options) option.selected = option.value === '9007199254740997'
 extras.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll('button')].find((item) => item.textContent === label)!
 button('Удалить клип с зазором').click()
 await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('keep selection for test'))
 expect(deleteFilmcraftClips).toHaveBeenLastCalledWith(expect.objectContaining({ clipIds: ['9007199254740995', '9007199254740997', '9007199254740999'], ripple: false }), expect.any(AbortSignal))
 const linked = document.querySelector<HTMLInputElement>('[aria-label="Связанные клипы"]')!
 linked.checked = false; linked.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 button('Удалить клип со сдвигом').click()
 await vi.waitFor(() => expect(deleteFilmcraftClips).toHaveBeenCalledTimes(2))
 expect(deleteFilmcraftClips).toHaveBeenLastCalledWith(expect.objectContaining({ clipIds: ['9007199254740995', '9007199254740997'], ripple: true }), expect.any(AbortSignal))
})

it('closes a gap at an absolute frame position on the selected track with undo and refusal retention', async () => {
 await open()
 const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
 vi.mocked(closeFilmcraftGap).mockRejectedValueOnce(new Error('occupied sync track')).mockResolvedValueOnce({ ...loaded, documentText: '{"closedGap":true}' })
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true }))
 const position = document.querySelector<HTMLInputElement>('[aria-label="Позиция правки, кадров"]')!
 position.value = '120'; position.dispatchEvent(new Event('input', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent === label)!
 button('Закрыть зазор на дорожке').click()
 await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('occupied sync track'))
 expect(button('Отменить правку').disabled).toBe(true)
 button('Закрыть зазор на дорожке').click()
 await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
 expect(closeFilmcraftGap).toHaveBeenLastCalledWith({ document: sourceText, sequenceId: '18446744073709551615', trackId: '3', timeFrames: '120' }, expect.any(AbortSignal))
 button('Отменить правку').click(); await tick()
 expect(button('Повторить правку').disabled).toBe(false)
})

it('razors all unlocked tracks without selection and explicitly expands selected links', async () => {
 await open(sourceText, true)
 const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
 vi.mocked(razorFilmcraftProject).mockResolvedValueOnce({ ...loaded, createdClipIds: [] }).mockResolvedValue({ ...loaded, documentText: '{"razored":true}', createdClipIds: ['18446744073709551614'] })
 const position = document.querySelector<HTMLInputElement>('[aria-label="Позиция правки, кадров"]')!
 position.value = '72'; position.dispatchEvent(new Event('input', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent === label)!
 expect(button('Разрезать выбранные клипы').disabled).toBe(true)
 button('Разрезать все дорожки').click()
 await vi.waitFor(() => expect(razorFilmcraftProject).toHaveBeenCalledWith({ document: sourceText, sequenceId: '18446744073709551615', trackIds: [], timeFrames: '72' }, expect.any(AbortSignal)))
 await vi.waitFor(() => expect(button('Разрезать все дорожки').disabled).toBe(false))
 expect(button('Отменить правку').disabled).toBe(true)
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const extras = document.querySelector<HTMLSelectElement>('[aria-label="Дополнительные клипы"]')!
 for (const option of extras.options) option.selected = option.value === '9007199254740997'
 extras.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 button('Разрезать выбранные клипы').click()
 await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
 expect(razorFilmcraftProject).toHaveBeenLastCalledWith({ document: sourceText, sequenceId: '18446744073709551615', clipIds: ['9007199254740995', '9007199254740997', '9007199254740999'], timeFrames: '72' }, expect.any(AbortSignal))
 button('Отменить правку').click(); await tick()
 expect(button('Повторить правку').disabled).toBe(false)
})

it('keeps selection and history when a razor command fails', async () => {
 await open()
 vi.mocked(razorFilmcraftProject).mockRejectedValue(new Error('razor ID overflow'))
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent === label)!
 button('Разрезать выбранные клипы').click()
 await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('razor ID overflow'))
 expect(clip.value).toBe('9007199254740995'); expect(button('Отменить правку').disabled).toBe(true)
})

it('removes a frame range on deduplicated selected tracks, preserves failed edits and supports undo', async () => {
 await open(sourceText, true)
 const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
 vi.mocked(removeFilmcraftRange).mockResolvedValueOnce({ ...loaded, documentText: '{"lifted":true}', removedClipIds: ['18446744073709551613'] }).mockRejectedValueOnce(new Error('range ID overflow'))
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const extras = document.querySelector<HTMLSelectElement>('[aria-label="Дополнительные клипы"]')!
 for (const option of extras.options) option.selected = option.value === '9007199254740997'
 extras.dispatchEvent(new Event('change', { bubbles: true }))
 const position = document.querySelector<HTMLInputElement>('[aria-label="Позиция правки, кадров"]')!
 position.value = '48'; position.dispatchEvent(new Event('input', { bubbles: true }))
 const duration = document.querySelector<HTMLInputElement>('[aria-label="Длительность диапазона, кадров"]')!
 duration.value = '17'; duration.dispatchEvent(new Event('input', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent === label)!
 button('Удалить диапазон с зазором').click()
 await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
 expect(removeFilmcraftRange).toHaveBeenLastCalledWith({ document: sourceText, sequenceId: '18446744073709551615', trackIds: ['3', '4'], startFrames: '48', durationFrames: '17', ripple: false }, expect.any(AbortSignal))
 const linked = document.querySelector<HTMLInputElement>('[aria-label="Связанные клипы"]')!
 linked.checked = false; linked.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 button('Удалить диапазон со сдвигом').click()
 await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('range ID overflow'))
 expect(removeFilmcraftRange).toHaveBeenLastCalledWith({ document: '{"lifted":true}', sequenceId: '18446744073709551615', trackIds: ['3'], startFrames: '48', durationFrames: '17', ripple: true }, expect.any(AbortSignal))
 expect(clip.value).toBe('9007199254740995')
 duration.value = '-1'; duration.dispatchEvent(new Event('input', { bubbles: true })); await tick()
 expect(button('Удалить диапазон со сдвигом').disabled).toBe(true)
 button('Отменить правку').click(); await tick(); expect(button('Повторить правку').disabled).toBe(false)
})

it('copies opaque linked selection and pastes fresh IDs with marker settings and undo', async () => {
 await open(sourceText, true)
 const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
 const clipboardText = '{"id":18446744073709551615,"tick":9007199254740993}'
 vi.mocked(copyFilmcraftClips).mockResolvedValueOnce({ clipboardText, clipCount: 2, markerCount: 1 }).mockRejectedValueOnce(new Error('copy refused'))
 const newIds = ['18446744073709551613', '18446744073709551614']
 const clips = loaded.sequences[0]!.clips!
 vi.mocked(pasteFilmcraftClips).mockResolvedValueOnce({ ...loaded, documentText: '{"pasted":true}', sequences: [{ ...loaded.sequences[0]!, clips: [...clips, { ...clips[0]!, id: newIds[0]! }, { ...clips[0]!, id: newIds[1]! }] }], selectedClipIds: newIds, playheadTicks: '9007199254740993', playheadFrames: '849600' }).mockRejectedValueOnce(new Error('paste refused'))
 const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent === label)!
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true }))
 for (const name of ['Копировать маркеры секвенции', 'Сдвигать маркеры при вставке']) { const input = document.querySelector<HTMLInputElement>(`[aria-label="${name}"]`)!; input.checked = true; input.dispatchEvent(new Event('change', { bubbles: true })) }
 const position = document.querySelector<HTMLInputElement>('[aria-label="Позиция правки, кадров"]')!
 position.value = '72'; position.dispatchEvent(new Event('input', { bubbles: true })); await tick()
 expect(button('Вставить со сдвигом').disabled).toBe(true)
 button('Копировать выбранные клипы').click()
 await vi.waitFor(() => expect(button('Вставить со сдвигом').disabled).toBe(false))
 expect(copyFilmcraftClips).toHaveBeenLastCalledWith({ document: sourceText, sequenceId: '18446744073709551615', clipIds: ['9007199254740995', '9007199254740999'], linked: true, includeMarkers: true }, expect.any(AbortSignal))
 expect(button('Отменить правку').disabled).toBe(true)
 button('Копировать выбранные клипы').click(); await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('copy refused'))
 button('Вставить со сдвигом').click(); await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
 expect(pasteFilmcraftClips).toHaveBeenLastCalledWith({ document: sourceText, sequenceId: '18446744073709551615', clipboardText, insert: true, includeMarkers: true, rippleMarkers: true, timeFrames: '72' }, expect.any(AbortSignal))
 expect(clip.value).toBe(newIds[0]); expect([...document.querySelector<HTMLSelectElement>('[aria-label="Дополнительные клипы"]')!.selectedOptions].map((option) => option.value)).toEqual([newIds[1]])
 button('Вставить с перезаписью').click(); await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('paste refused'))
 expect(pasteFilmcraftClips).toHaveBeenLastCalledWith(expect.objectContaining({ document: '{"pasted":true}', insert: false, clipboardText, timeTicks: '9007199254740993' }), expect.any(AbortSignal))
 expect(clip.value).toBe(newIds[0]); button('Отменить правку').click(); await tick(); expect(button('Повторить правку').disabled).toBe(false)
 expect(clip.value).toBe(''); expect(button('Копировать выбранные клипы').disabled).toBe(true)
})

it('retains clipboard across sequence changes and can paste into an empty sequence', async () => {
 await open(sourceText, false, true)
 const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
 const clipboardText = '{"clipboardId":18446744073709551615}'
 vi.mocked(copyFilmcraftClips).mockResolvedValue({ clipboardText, clipCount: 1, markerCount: 0 })
 vi.mocked(pasteFilmcraftClips).mockResolvedValue({ ...loaded, documentText: '{"targetPasted":true}', sequences: [loaded.sequences[0]!, { ...loaded.sequences[1]!, clips: [{ ...loaded.sequences[0]!.clips![0]!, id: '18446744073709551613' }] }], selectedClipIds: ['18446744073709551613'], playheadTicks: '254016000000', playheadFrames: '24' })
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
 clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent === label)!
 button('Копировать выбранные клипы').click(); await vi.waitFor(() => expect(button('Вставить с перезаписью').disabled).toBe(false))
 const sequence = document.querySelector<HTMLSelectElement>('[aria-label="Секвенция Filmcraft"]')!
 sequence.value = '9007199254740990'; sequence.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 expect(document.querySelector('[aria-label="Клип Filmcraft"]')).toBeNull()
 expect(button('Вставить с перезаписью').disabled).toBe(false)
 button('Вставить с перезаписью').click(); await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
 expect(pasteFilmcraftClips).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText, sequenceId: '9007199254740990', clipboardText, insert: false, timeFrames: '0' }), expect.any(AbortSignal))
 expect(document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!.value).toBe('18446744073709551613')
 const file = new File([sourceText], 'replacement.json', { type: 'application/json' }); Object.defineProperty(file, 'text', { value: async () => sourceText })
 const input = document.querySelector<HTMLInputElement>('input[type=file]')!; Object.defineProperty(input, 'files', { configurable: true, value: [file] })
 vi.mocked(importFilmcraftProject).mockRejectedValueOnce(new Error('replacement refused'))
 input.dispatchEvent(new Event('change', { bubbles: true })); await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('replacement refused'))
 expect(button('Вставить с перезаписью').disabled).toBe(false)
 input.dispatchEvent(new Event('change', { bubbles: true })); await vi.waitFor(() => expect(button('Вставить с перезаписью').disabled).toBe(true))
 expect(button('Отменить правку').disabled).toBe(true)

})

it('continues paste from exact native ticks and preserves manually entered frame strings', async () => {
 await open()
 const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
 vi.mocked(copyFilmcraftClips).mockResolvedValue({ clipboardText: '{"opaque":18446744073709551615}', clipCount: 1, markerCount: 0 })
 vi.mocked(pasteFilmcraftClips).mockResolvedValue({ ...loaded, documentText: '{"pasted":true}', selectedClipIds: ['9007199254740995'], playheadTicks: '9007199254740993', playheadFrames: '849600' })
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!; clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>('button')].find((item) => item.textContent === label)!
 button('Копировать выбранные клипы').click(); await vi.waitFor(() => expect(button('Вставить со сдвигом').disabled).toBe(false))
 const position = document.querySelector<HTMLInputElement>('[aria-label="Позиция правки, кадров"]')!
 position.value = '72'; position.dispatchEvent(new Event('input', { bubbles: true })); await tick()
 button('Вставить со сдвигом').click(); await vi.waitFor(() => expect(position.value).toBe('849600'))
 expect(pasteFilmcraftClips).toHaveBeenLastCalledWith(expect.objectContaining({ timeFrames: '72' }), expect.any(AbortSignal))
 button('Вставить со сдвигом').click(); await vi.waitFor(() => expect(pasteFilmcraftClips).toHaveBeenCalledTimes(2)); await vi.waitFor(() => expect(button('Вставить со сдвигом').disabled).toBe(false))
 const continued = vi.mocked(pasteFilmcraftClips).mock.calls[1]![0]
 expect(continued).toHaveProperty('timeTicks', '9007199254740993'); expect(continued).not.toHaveProperty('timeFrames')
 position.value = '9007199254740993'; position.dispatchEvent(new Event('input', { bubbles: true })); await tick()
 vi.mocked(pasteFilmcraftClips).mockRejectedValueOnce(new Error('frame clock overflow'))
 button('Вставить со сдвигом').click(); await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toBe('frame clock overflow'))
 const manual = vi.mocked(pasteFilmcraftClips).mock.calls[2]![0]; expect(manual).toHaveProperty('timeFrames', '9007199254740993'); expect(manual).not.toHaveProperty('timeTicks')
 position.value = '9223372036854775808'; position.dispatchEvent(new Event('input', { bubbles: true })); await tick(); expect(button('Вставить со сдвигом').disabled).toBe(true)
 position.value = '1.5'; position.dispatchEvent(new Event('input', { bubbles: true })); await tick(); expect(button('Вставить со сдвигом').disabled).toBe(true)
})

it('loads transitions lazily, applies typed parameters and restores the previous document', async () => {
 await open()
 expect(getFilmcraftTransitions).not.toHaveBeenCalled()
 vi.mocked(getFilmcraftTransitions).mockResolvedValue([{ definition: { id: 'push', name: 'Push', kind: 'VideoTransition', category: ['Video Transitions', 'Slide'], params: [
 { id: 'direction', label: 'Direction', kind: { Choice: ['North', 'East'] }, default: { Choice: 0 } },
 { id: 'amount', label: 'Amount', kind: { Float: {} }, default: { Float: 50 } },
 { id: 'center', label: 'Center', kind: 'Point', default: { Vec2: { x: null, y: null } } },
 { id: 'color', label: 'Color', kind: 'Color', default: { Color: [0, 0, 0, 1] } },
 { id: 'flag', label: 'Flag', kind: 'Bool', default: { Bool: false } },
 ] }, instance: {} }])
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!; clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 const details = document.querySelector<HTMLDetailsElement>('.transition-controls')!; details.open = true; details.dispatchEvent(new Event('toggle')); await vi.waitFor(() => expect(document.querySelector('[aria-label="Переход Filmcraft"]')).not.toBeNull())
 const set = (label: string, value: string) => { const input = document.querySelector<HTMLInputElement | HTMLSelectElement>(`[aria-label="${label}"]`)!; input.value = value; input.dispatchEvent(new Event(input.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true })) }
 set('Параметр перехода: Direction', '1'); set('Параметр перехода: Amount', '125'); set('Параметр перехода: Center', '12.5, -4'); set('Параметр перехода: Color', '#11223380')
 const flag = document.querySelector<HTMLInputElement>('[aria-label="Параметр перехода: Flag"]')!; flag.checked = true; flag.dispatchEvent(new Event('change', { bubbles: true })); set('Длительность перехода', '31'); await tick()
 const edited = structuredClone(await vi.mocked(importFilmcraftProject).mock.results[0]!.value) as FilmcraftImportResult
 edited.documentText = '{"transition":9007199254740993}'
 vi.mocked(applyFilmcraftTransition).mockResolvedValue({ ...edited, createdTransitionId: '9007199254740993' })
 const button = (text: string) => [...document.querySelectorAll('button')].find((button) => button.textContent === text)!
 button('Применить переход').click(); await vi.waitFor(() => expect(applyFilmcraftTransition).toHaveBeenCalled())
 expect(applyFilmcraftTransition).toHaveBeenCalledWith({ document: sourceText, sequenceId: '18446744073709551615', kind: 'video', effect: 'push', frames: '31', clipId: '9007199254740995', edge: 'out', timeFrames: '0', params: { direction: 1, amount: 125, center: [12.5, -4], color: '#11223380', flag: true }, reverse: false }, expect.any(AbortSignal))
 await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false)); button('Отменить правку').click(); await tick(); button('Применить переход').click(); await vi.waitFor(() => expect(applyFilmcraftTransition).toHaveBeenCalledTimes(2)); expect(vi.mocked(applyFilmcraftTransition).mock.calls[1]![0].document).toBe(sourceText)
})

it('retries catalog loading and applies audio defaults without serializing untouched auto values', async () => {
 await open(sourceText, true)
 vi.mocked(getFilmcraftTransitions).mockRejectedValueOnce(new Error('Каталог недоступен')).mockResolvedValueOnce([
 { definition: { id: 'cross_dissolve', name: 'Cross Dissolve', kind: 'VideoTransition', category: [], params: [] }, instance: {} },
 { definition: { id: 'constant_power', name: 'Constant Power', kind: 'AudioTransition', category: [], params: [] }, instance: {} },
 ])
 const details = document.querySelector<HTMLDetailsElement>('.transition-controls')!; details.open = true; details.dispatchEvent(new Event('toggle'))
 await vi.waitFor(() => expect(document.body.textContent).toContain('Каталог недоступен'))
 const button = (text: string) => [...document.querySelectorAll('button')].find((button) => button.textContent === text)!
 button('Повторить загрузку').click(); await vi.waitFor(() => expect(document.querySelector('[aria-label="Переход Filmcraft"]')).not.toBeNull())
 const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!; clip.value = '9007199254740999'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
 expect(document.querySelector<HTMLSelectElement>('[aria-label="Переход Filmcraft"]')!.value).toBe('constant_power')
 vi.mocked(applyFilmcraftTransition).mockRejectedValue(new Error('Дорожка заблокирована'))
 button('Применить переход').click(); await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('Дорожка заблокирована'))
 expect(vi.mocked(applyFilmcraftTransition).mock.calls[0]![0]).toMatchObject({ kind: 'audio', effect: 'constant_power', clipId: '9007199254740999', params: {} })
 expect(button('Отменить правку').disabled).toBe(true)
 const duration = document.querySelector<HTMLInputElement>('[aria-label="Длительность перехода"]')!; duration.value = 'invalid'; duration.dispatchEvent(new Event('input', { bubbles: true })); await tick(); expect(button('Применить переход').disabled).toBe(true)
})

it('edits an existing transition with exact ID, resets defaults and restores values on undo', async () => {
 await open()
 const initial = structuredClone(await vi.mocked(importFilmcraftProject).mock.results[0]!.value) as FilmcraftImportResult
 initial.sequences[0]!.transitions = [{ id: '18446744073709551615', trackId: '3', trackKind: 'video_tracks', effect: 'push', reverse: true, startTicks: '9007199254740993', durationTicks: '254016000000', paramsText: '{"direction":{"value":{"Choice":1},"keyframes":[]}}' }]
 vi.mocked(importFilmcraftProject).mockResolvedValue(initial)
 const file = new File([sourceText], 'again.json');Object.defineProperty(file, 'text', { value: async () => sourceText });const fileInput = document.querySelector<HTMLInputElement>('input[type=file]')!;Object.defineProperty(fileInput, 'files', { configurable: true, value: [file] });fileInput.dispatchEvent(new Event('change', { bubbles: true }));await tick();await vi.waitFor(() => expect(document.querySelector('[aria-label="Созданный переход Filmcraft"] option[value="18446744073709551615"]')).not.toBeNull())
 vi.mocked(getFilmcraftTransitions).mockResolvedValue([{ definition: { id: 'push', name: 'Push', kind: 'VideoTransition', category: [], params: [{ id: 'direction', label: 'Direction', kind: { Choice: ['North', 'East'] }, default: { Choice: 0 } }] }, instance: {} }])
 const details=document.querySelector<HTMLDetailsElement>('.transition-controls')!;details.open=true;details.dispatchEvent(new Event('toggle'));await vi.waitFor(() => expect(document.querySelector('[aria-label="Переход Filmcraft"]')).not.toBeNull())
 const select=document.querySelector<HTMLSelectElement>('[aria-label="Созданный переход Filmcraft"]')!;select.value='18446744073709551615';select.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 expect(document.querySelector('[aria-label="Переход Filmcraft"]')).toBeNull();expect(document.querySelector<HTMLSelectElement>('[aria-label="Параметр перехода: Direction"]')!.value).toBe('1');expect(document.querySelector<HTMLInputElement>('[aria-label="Обратный переход"]')!.checked).toBe(true)
 const edited=structuredClone(initial);edited.documentText='{"edited":true}';edited.sequences[0]!.transitions![0]!.paramsText='{"direction":{"value":{"Choice":0},"keyframes":[]}}';vi.mocked(setFilmcraftTransition).mockResolvedValue(edited)
 const direction=document.querySelector<HTMLSelectElement>('[aria-label="Параметр перехода: Direction"]')!;direction.value='0';direction.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const button=(text:string)=>[...document.querySelectorAll('button')].find((button)=>button.textContent===text)!
 button('Сохранить переход').click();await vi.waitFor(()=>expect(setFilmcraftTransition).toHaveBeenCalled());expect(setFilmcraftTransition).toHaveBeenCalledWith({document:sourceText,sequenceId:'18446744073709551615',transitionId:'18446744073709551615',params:{direction:0},reverse:true},expect.any(AbortSignal));await vi.waitFor(()=>expect(button('Отменить правку').disabled).toBe(false));button('Отменить правку').click();await tick();expect(document.querySelector<HTMLSelectElement>('[aria-label="Параметр перехода: Direction"]')!.value).toBe('1')
 button('Сбросить параметры перехода').click();await vi.waitFor(()=>expect(setFilmcraftTransition).toHaveBeenCalledTimes(2));expect(vi.mocked(setFilmcraftTransition).mock.calls[1]![0]).toEqual({document:sourceText,sequenceId:'18446744073709551615',transitionId:'18446744073709551615',reset:true})
})

it('targets selected media tracks at an exact playhead without forcing a clip edge', async () => {
 await open(sourceText, true)
 vi.mocked(getFilmcraftTransitions).mockResolvedValue([{ definition: { id: 'cross_dissolve', name: 'Cross Dissolve', kind: 'VideoTransition', category: [], params: [] }, instance: {} }, { definition: { id: 'constant_power', name: 'Constant Power', kind: 'AudioTransition', category: [], params: [] }, instance: {} }])
 const clip=document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!;clip.value='9007199254740995';clip.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const details=document.querySelector<HTMLDetailsElement>('.transition-controls')!;details.open=true;details.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(document.querySelector('[aria-label="Место перехода"]')).not.toBeNull());await tick()
 const place=document.querySelector<HTMLSelectElement>('[aria-label="Место перехода"]')!;place.value='playhead';place.dispatchEvent(new Event('change',{bubbles:true}));await tick();const time=document.querySelector<HTMLInputElement>('[aria-label="Позиция правки, кадров"]')!;time.value='9007199254740993';time.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 const apply=[...document.querySelectorAll('button')].find((b)=>b.textContent==='Применить переход')!;vi.mocked(applyFilmcraftTransition).mockRejectedValue(new Error('Стык не найден'));apply.click();await vi.waitFor(()=>expect(applyFilmcraftTransition).toHaveBeenCalled());expect(vi.mocked(applyFilmcraftTransition).mock.calls[0]![0]).toEqual({document:sourceText,sequenceId:'18446744073709551615',kind:'video',effect:'cross_dissolve',frames:'24',targetedTrackIds:['3'],timeFrames:'9007199254740993',params:{},reverse:false});await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('Стык не найден'));expect([...document.querySelectorAll('button')].find((b)=>b.textContent==='Отменить правку')!.disabled).toBe(true)
 clip.value='9007199254740999';clip.dispatchEvent(new Event('change',{bubbles:true}));await tick();apply.click();await vi.waitFor(()=>expect(applyFilmcraftTransition).toHaveBeenCalledTimes(2));expect(vi.mocked(applyFilmcraftTransition).mock.calls[1]![0]).toMatchObject({kind:'audio',effect:'constant_power',targetedTrackIds:['4']});expect(vi.mocked(applyFilmcraftTransition).mock.calls[1]![0]).not.toHaveProperty('clipId');expect(vi.mocked(applyFilmcraftTransition).mock.calls[1]![0]).not.toHaveProperty('edge')
})

it('applies a configured batch to linked selection as one undo step and rejects invalid durations', async () => {
 await open(sourceText,true)
 vi.mocked(getFilmcraftTransitions).mockResolvedValue([{definition:{id:'cross_dissolve',name:'Cross Dissolve',kind:'VideoTransition',category:[],params:[]},instance:{}},{definition:{id:'push',name:'Push',kind:'VideoTransition',category:[],params:[]},instance:{}},{definition:{id:'constant_power',name:'Constant Power',kind:'AudioTransition',category:[],params:[]},instance:{}}])
 const clip=document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!;clip.value='9007199254740995';clip.dispatchEvent(new Event('change',{bubbles:true}));await tick();const details=document.querySelector<HTMLDetailsElement>('.transition-controls')!;details.open=true;details.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(document.querySelector('[aria-label="Пакетный видеопереход"]')).not.toBeNull());await tick()
 const video=document.querySelector<HTMLSelectElement>('[aria-label="Пакетный видеопереход"]')!;video.value='push';video.dispatchEvent(new Event('change',{bubbles:true}));const duration=document.querySelector<HTMLInputElement>('[aria-label="Длительность пакетного аудиоперехода"]')!;const unit=document.querySelector<HTMLSelectElement>('[aria-label="Единицы пакетного аудиоперехода"]')!;unit.value='frames';unit.dispatchEvent(new Event('change',{bubbles:true}));duration.value='12';duration.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 const initial=structuredClone(await vi.mocked(importFilmcraftProject).mock.results[0]!.value) as FilmcraftImportResult;const edited={...initial,documentText:'{"batch":true}',createdTransitionIds:['18446744073709551615','9007199254740993']};vi.mocked(applyFilmcraftSelectionTransitions).mockResolvedValue(edited)
 const button=(text:string)=>[...document.querySelectorAll('button')].find((button)=>button.textContent===text)!
 button('Применить к выделенным').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Создано переходов: 2'));expect(applyFilmcraftSelectionTransitions).toHaveBeenCalledWith({document:sourceText,sequenceId:'18446744073709551615',clipIds:['9007199254740995','9007199254740999'],videoEffect:'push',audioEffect:'constant_power',videoFrames:'30',audioFrames:'12'},expect.any(AbortSignal))
 button('Отменить правку').click();await tick();expect(button('Отменить правку').disabled).toBe(true);expect(document.body.textContent).not.toContain('Создано переходов:');vi.mocked(applyFilmcraftSelectionTransitions).mockRejectedValue(new Error('Нет свободного стыка'));button('Применить к выделенным').click();await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('Нет свободного стыка'));expect(vi.mocked(applyFilmcraftSelectionTransitions).mock.calls[1]![0].document).toBe(sourceText);expect(button('Отменить правку').disabled).toBe(true)
 duration.value='invalid';duration.dispatchEvent(new Event('input',{bubbles:true}));await tick();expect(button('Применить к выделенным').disabled).toBe(true)
})

it('reopens and saves transition preferences, sending seconds and fractional frames to the native clock', async () => {
 const transitionSettings={video:{effect:'push',duration:{value:'0.5',unit:'seconds'}},audio:{effect:'constant_power',duration:{value:'1.25',unit:'seconds'}},single:{value:'1.5',unit:'frames'}}
 const session={format:'video-kadr.filmcraft-session',version:1,documentText:sourceText,sequenceId:'18446744073709551615',sources:{},start:0,end:1,audioFormat:'wav',bits:24,channels:2,transitionSettings}
 const blobs:Blob[]=[];vi.spyOn(URL,'createObjectURL').mockImplementation((blob)=>{blobs.push(blob as Blob);return 'blob:saved'});vi.spyOn(URL,'revokeObjectURL').mockImplementation(()=>{});vi.spyOn(HTMLAnchorElement.prototype,'click').mockImplementation(()=>{})
 await open(JSON.stringify(session),true)
 vi.mocked(getFilmcraftTransitions).mockResolvedValue([{definition:{id:'cross_dissolve',name:'Cross Dissolve',kind:'VideoTransition',category:[],params:[]},instance:{}},{definition:{id:'push',name:'Push',kind:'VideoTransition',category:[],params:[]},instance:{}},{definition:{id:'constant_power',name:'Constant Power',kind:'AudioTransition',category:[],params:[]},instance:{}}])
 const clip=document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!;clip.value='9007199254740995';clip.dispatchEvent(new Event('change',{bubbles:true}));await tick();const details=document.querySelector<HTMLDetailsElement>('.transition-controls')!;details.open=true;details.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(document.querySelector('[aria-label="Пакетный видеопереход"]')).not.toBeNull());await tick()
 expect(document.querySelector<HTMLSelectElement>('[aria-label="Пакетный видеопереход"]')!.value).toBe('push');expect(document.querySelector<HTMLSelectElement>('[aria-label="Единицы пакетного видеоперехода"]')!.value).toBe('seconds');expect(document.querySelector<HTMLInputElement>('[aria-label="Длительность пакетного аудиоперехода"]')!.value).toBe('1.25')
 const initial=await vi.mocked(importFilmcraftProject).mock.results[0]!.value as FilmcraftImportResult;vi.mocked(applyFilmcraftSelectionTransitions).mockResolvedValue({...initial,documentText:'{"seconds":true}',createdTransitionIds:['9007199254740993']});const button=(text:string)=>[...document.querySelectorAll('button')].find((button)=>button.textContent===text)!
 button('Применить к выделенным').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Создано переходов: 1'));expect(vi.mocked(applyFilmcraftSelectionTransitions).mock.calls[0]![0]).toMatchObject({videoDuration:{value:0.5,unit:'seconds'},audioDuration:{value:1.25,unit:'seconds'}});expect(vi.mocked(applyFilmcraftSelectionTransitions).mock.calls[0]![0]).not.toHaveProperty('videoFrames')
 vi.mocked(applyFilmcraftTransition).mockRejectedValue(new Error('Проверка дробных кадров'));button('Применить переход').click();await vi.waitFor(()=>expect(applyFilmcraftTransition).toHaveBeenCalled());expect(vi.mocked(applyFilmcraftTransition).mock.calls[0]![0]).toMatchObject({duration:{value:1.5,unit:'frames'}});expect(vi.mocked(applyFilmcraftTransition).mock.calls[0]![0]).not.toHaveProperty('frames')
 await vi.waitFor(()=>expect(button('Сохранить рабочую сессию').disabled).toBe(false));button('Сохранить рабочую сессию').click();await vi.waitFor(()=>expect(blobs.length).toBeGreaterThan(0));const saved=JSON.parse(await blobs.at(-1)!.text());expect(saved.transitionSettings).toEqual(transitionSettings);expect(saved.documentText).toBe('{"seconds":true}')
})

it('applies ordered edit points with duplicates and clears them on sequence changes', async () => {
 await open(sourceText,true)
 vi.mocked(getFilmcraftTransitions).mockResolvedValue([{definition:{id:'cross_dissolve',name:'Cross Dissolve',kind:'VideoTransition',category:[],params:[]},instance:{}},{definition:{id:'constant_power',name:'Constant Power',kind:'AudioTransition',category:[],params:[]},instance:{}}])
 const clip=document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!;clip.value='9007199254740995';clip.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const details=document.querySelector<HTMLDetailsElement>('.transition-controls')!;details.open=true;details.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(document.querySelector('[aria-label="Пакетный видеопереход"]')).not.toBeNull())
 const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===text)!
 button('Добавить точку монтажа').click();await tick();button('Добавить точку монтажа').click();await tick()
 expect(document.querySelectorAll('[aria-label="Выбранные точки монтажа"] li')).toHaveLength(2)
 const initial=structuredClone(await vi.mocked(importFilmcraftProject).mock.results[0]!.value) as FilmcraftImportResult
 vi.mocked(applyFilmcraftSelectionTransitions).mockResolvedValue({...initial,documentText:'{"points":true}',createdTransitionIds:['18446744073709551615']})
 button('Применить к точкам монтажа').click();await vi.waitFor(()=>expect(applyFilmcraftSelectionTransitions).toHaveBeenCalled())
 expect(applyFilmcraftSelectionTransitions).toHaveBeenLastCalledWith(expect.objectContaining({clipIds:[],editPoints:[{clipId:'9007199254740995',edge:'out'},{clipId:'9007199254740995',edge:'out'}]}),expect.any(AbortSignal))
 await vi.waitFor(()=>expect(document.body.textContent).toContain('Создано переходов: 1'));button('Отменить правку').click();await tick()
 vi.mocked(applyFilmcraftSelectionTransitions).mockRejectedValue(new Error('Нет доступных точек'));button('Применить к точкам монтажа').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Нет доступных точек'))
 expect(document.querySelectorAll('[aria-label="Выбранные точки монтажа"] li')).toHaveLength(2)
 expect(applyFilmcraftSelectionTransitions).toHaveBeenLastCalledWith(expect.objectContaining({document:initial.documentText}),expect.any(AbortSignal))
 document.querySelector<HTMLButtonElement>('[aria-label="Удалить точку монтажа 1"]')!.click();await tick();expect(document.querySelectorAll('[aria-label="Выбранные точки монтажа"] li')).toHaveLength(1)
 const seq=document.querySelector<HTMLSelectElement>('[aria-label="Секвенция Filmcraft"]')!;seq.dispatchEvent(new Event('change',{bubbles:true}));await tick();expect(document.querySelector('[aria-label="Выбранные точки монтажа"]')).toBeNull()
})

it('loads exact trim-monitor frames lazily and discards late responses after clearing points', async () => {
 await open(sourceText,true)
 vi.mocked(getFilmcraftTransitions).mockResolvedValue([{definition:{id:'cross_dissolve',name:'Cross Dissolve',kind:'VideoTransition',category:[],params:[]},instance:{}}])
 const clip=document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!;clip.value='9007199254740995';clip.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const transitions=document.querySelector<HTMLDetailsElement>('.transition-controls')!;transitions.open=true;transitions.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(document.querySelector('[aria-label="Пакетный видеопереход"]')).not.toBeNull())
 const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===text)!
 button('Добавить точку монтажа').click();await tick();expect(getFilmcraftTrimMonitor).not.toHaveBeenCalled()
 const frame={clip:'9007199254740995',item:'18446744073709551615',name:'Точный исходник',track:'V1',video:true,time:'9007199254740993',mediaTime:'-9007199254740993',mediaFrame:'9007199254740993',sourceIn:'0',sourceOut:'9223372036854775806'}
 const result={editTime:'9007199254740993',rollPair:null,outgoing:frame,incoming:null}
 let resolve!:(value:typeof result)=>void
 vi.mocked(getFilmcraftTrimMonitor).mockImplementationOnce(()=>new Promise(r=>{resolve=r}))
 const details=document.querySelector<HTMLDetailsElement>('.trim-monitor')!;details.open=true;details.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(getFilmcraftTrimMonitor).toHaveBeenCalled())
 const [input,signal]=vi.mocked(getFilmcraftTrimMonitor).mock.calls[0]!;expect(input).toEqual({document:sourceText,sequenceId:'18446744073709551615',clipId:'9007199254740995',edge:'out',includeFrames:true,sources:{'9007199254740993':'asset'}})
 button('Очистить точки монтажа').click();await tick();expect(signal!.aborted).toBe(true);resolve(result);await tick();expect(document.body.textContent).not.toContain('Точный исходник')
 vi.mocked(getFilmcraftTrimMonitor).mockRejectedValueOnce(new Error('Стык не найден'));button('Добавить точку монтажа').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Стык не найден'))
 vi.mocked(getFilmcraftTrimMonitor).mockResolvedValue(result);button('Повторить загрузку стыка').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Точный исходник'))
 expect(document.body.textContent).toContain('Кадр исходника: 9007199254740993');expect(document.body.textContent).toContain('Позиция исходника: -9007199254740993');expect(document.body.textContent).toContain('Клипа с этой стороны нет')
 vi.mocked(getFilmcraftTrimMonitor).mockResolvedValueOnce({...result,outgoing:{...frame,frameUnavailable:'video frame decode failed'}})
 details.open=false;details.dispatchEvent(new Event('toggle'));await tick();details.open=true;details.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(button('Повторить загрузку кадров')).toBeDefined())
 const count=vi.mocked(getFilmcraftTrimMonitor).mock.calls.length;button('Повторить загрузку кадров').click();await vi.waitFor(()=>expect(getFilmcraftTrimMonitor).toHaveBeenCalledTimes(count+1));await vi.waitFor(()=>expect(button('Повторить загрузку кадров')).toBeUndefined())
 details.open=false;details.dispatchEvent(new Event('toggle'));await tick()
})

it('selects nearest points by exact position and targets, clears clip selection and strips point kinds from transition requests', async () => {
 await open(sourceText,true)
 vi.mocked(getFilmcraftTransitions).mockResolvedValue([{definition:{id:'cross_dissolve',name:'Cross Dissolve',kind:'VideoTransition',category:[],params:[]},instance:{}}])
 const clip=document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!;clip.value='9007199254740995';clip.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const transitions=document.querySelector<HTMLDetailsElement>('.transition-controls')!;transitions.open=true;transitions.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(document.querySelector('[aria-label="Режим ближайших точек"]')).not.toBeNull())
 const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===text)!
 const kind=document.querySelector<HTMLSelectElement>('[aria-label="Режим ближайших точек"]')!;kind.value='rippleOut';kind.dispatchEvent(new Event('change',{bubbles:true}))
 const target=document.querySelector<HTMLSelectElement>('[aria-label="Дорожки ближайших точек"]')!;target.options[1]!.selected=true;target.dispatchEvent(new Event('change',{bubbles:true}))
 const position=document.querySelector<HTMLInputElement>('[aria-label="Позиция правки, кадров"]')!;position.value='72';position.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 vi.mocked(getFilmcraftNearestEditPoints).mockResolvedValue({editPoints:[{clip:'9007199254740999',out:true,kind:'ripple'}]})
 button('Выбрать ближайшие точки').click();await vi.waitFor(()=>expect(getFilmcraftNearestEditPoints).toHaveBeenCalled());await vi.waitFor(()=>expect(document.querySelectorAll('[aria-label="Выбранные точки монтажа"] li')).toHaveLength(1))
 expect(getFilmcraftNearestEditPoints).toHaveBeenLastCalledWith({document:sourceText,sequenceId:'18446744073709551615',kind:'rippleOut',targetedTrackIds:['4'],timeFrames:'72'},expect.any(AbortSignal))
 expect(clip.value).toBe('');expect(button('Отменить правку').disabled).toBe(true)
 const initial=structuredClone(await vi.mocked(importFilmcraftProject).mock.results[0]!.value) as FilmcraftImportResult
 vi.mocked(applyFilmcraftSelectionTransitions).mockResolvedValue({...initial,documentText:'{"nearest":true}',createdTransitionIds:['18446744073709551615']})
 button('Применить к точкам монтажа').click();await vi.waitFor(()=>expect(applyFilmcraftSelectionTransitions).toHaveBeenCalled())
 expect(applyFilmcraftSelectionTransitions).toHaveBeenLastCalledWith(expect.objectContaining({clipIds:[],editPoints:[{clipId:'9007199254740999',edge:'out'}]}),expect.any(AbortSignal));await vi.waitFor(()=>expect(button('Выбрать ближайшие точки').disabled).toBe(false))
 vi.mocked(getFilmcraftNearestEditPoints).mockRejectedValue(new Error('Поиск недоступен'));button('Выбрать ближайшие точки').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Поиск недоступен'));expect(document.querySelectorAll('[aria-label="Выбранные точки монтажа"] li')).toHaveLength(1)
 vi.mocked(getFilmcraftNearestEditPoints).mockResolvedValue({editPoints:[]});button('Выбрать ближайшие точки').click();await vi.waitFor(()=>expect(document.querySelector('[aria-label="Выбранные точки монтажа"]')).toBeNull())
})

it('adopts partially refused trim-point changes, preserves exact requests and supports undo', async () => {
 await open(sourceText,true)
 vi.mocked(getFilmcraftTransitions).mockResolvedValue([{definition:{id:'cross_dissolve',name:'Cross Dissolve',kind:'VideoTransition',category:[],params:[]},instance:{}}])
 const clip=document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!;clip.value='9007199254740995';clip.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const transitions=document.querySelector<HTMLDetailsElement>('.transition-controls')!;transitions.open=true;transitions.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(document.querySelector('[aria-label="Режим ближайших точек"]')).not.toBeNull())
 const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===text)!
 button('Добавить точку монтажа').click();await tick();button('Сменить тип подрезки точек').click();await tick()
 const delta=document.querySelector<HTMLInputElement>('[aria-label="Сдвиг точек монтажа, кадров"]')!;delta.value='-12';delta.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 const initial=structuredClone(await vi.mocked(importFilmcraftProject).mock.results[0]!.value) as FilmcraftImportResult
 const partial={...initial,documentText:'{"partial":true}',shift:{outShift:'0',inShift:'0'},shiftFrames:{outShift:'0',inShift:'0'},appliedTicks:null,refusal:'Поздняя точка заблокирована'}
 vi.mocked(trimFilmcraftEditPoints).mockResolvedValue(partial)
 button('Подрезать выбранные точки').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Поздняя точка заблокирована'))
 expect(trimFilmcraftEditPoints).toHaveBeenLastCalledWith({document:sourceText,sequenceId:'18446744073709551615',points:[{clip:'9007199254740995',out:true,kind:'ripple'}],linked:true,deltaFrames:'-12',shift:{outShift:'0',inShift:'0'}},expect.any(AbortSignal))
 expect(button('Отменить правку').disabled).toBe(false)
 vi.mocked(trimFilmcraftEditPoints).mockResolvedValue({...partial,shift:{outShift:'9007199254740993',inShift:'-9007199254740993'},shiftFrames:{outShift:'9007199254740993',inShift:'-9007199254740993'},appliedTicks:['0'],refusal:null});button('Подрезать выбранные точки').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Обработано точек: 1'))
 expect(trimFilmcraftEditPoints).toHaveBeenLastCalledWith(expect.objectContaining({document:'{"partial":true}'}),expect.any(AbortSignal))
 expect(document.querySelector('[aria-label="Счётчики сдвига точек"]')!.textContent).toContain('9007199254740993');
 button('Отменить правку').click();await tick();expect(document.body.textContent).not.toContain('Обработано точек:');expect(document.querySelector('[aria-label="Счётчики сдвига точек"]')!.textContent).toContain('-9007199254740993')
 button('Сменить тип подрезки точек').click();await tick();vi.mocked(trimFilmcraftEditPoints).mockResolvedValue({...initial,shift:{outShift:'0',inShift:'0'},shiftFrames:{outShift:'0',inShift:'0'},appliedTicks:[],refusal:null});button('Подрезать выбранные точки').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Обработано точек: 0'))
 expect(trimFilmcraftEditPoints).toHaveBeenLastCalledWith(expect.objectContaining({document:sourceText,points:[{clip:'9007199254740995',out:true,kind:'roll'}],shift:{outShift:'9007199254740993',inShift:'-9007199254740993'}}),expect.any(AbortSignal))
 delta.value='1.5';delta.dispatchEvent(new Event('input',{bubbles:true}));await tick();expect(button('Подрезать выбранные точки').disabled).toBe(true)
 const playhead=document.querySelector<HTMLInputElement>('[aria-label="Позиция правки, кадров"]')!;playhead.value='9007199254740993';playhead.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 expect(button('Подрезать точки до курсора').disabled).toBe(false)
 vi.mocked(trimFilmcraftEditPoints).mockResolvedValue({...initial,documentText:'{"extended":true}',shift:{outShift:'-1',inShift:'-1'},shiftFrames:{outShift:'-1',inShift:'-1'},appliedTicks:['-1'],refusal:null})
 button('Подрезать точки до курсора').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Обработано точек: 1'))
 const extendedRequest=vi.mocked(trimFilmcraftEditPoints).mock.calls.at(-1)![0];expect(extendedRequest).toMatchObject({playheadFrames:'9007199254740993',points:[{clip:'9007199254740995',out:true,kind:'roll'}]});expect(extendedRequest).not.toHaveProperty('deltaFrames');expect(extendedRequest).not.toHaveProperty('deltaTicks')
 expect(button('Отменить правку').disabled).toBe(false);button('Отменить правку').click();await tick()
 playhead.value='1.5';playhead.dispatchEvent(new Event('input',{bubbles:true}));await tick();expect(button('Подрезать точки до курсора').disabled).toBe(true)

})

it('adopts Q/W edits, continues exact returned ticks and preserves undo on refusal', async () => {
 await open(sourceText,true)
 vi.mocked(getFilmcraftTransitions).mockResolvedValue([{definition:{id:'cross_dissolve',name:'Cross Dissolve',kind:'VideoTransition',category:[],params:[]},instance:{}}])
 const clip=document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!;clip.value='9007199254740995';clip.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const transitions=document.querySelector<HTMLDetailsElement>('.transition-controls')!;transitions.open=true;transitions.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(document.querySelector('[aria-label="Режим ближайших точек"]')).not.toBeNull())
 const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===text)!
 const initial=structuredClone(await vi.mocked(importFilmcraftProject).mock.results[0]!.value) as FilmcraftImportResult
 vi.mocked(trimFilmcraftToPlayhead).mockResolvedValue({...initial,documentText:'{"qw":true}',rangeTicks:['9007199254740993','9007199254740994'],playheadTicks:'9007199254740993',playheadFrames:'851020',trackIds:['3']})
 const position=document.querySelector<HTMLInputElement>('[aria-label="Позиция правки, кадров"]')!;position.value='9007199254740993';position.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 button('Удалить до курсора со сдвигом').click();await vi.waitFor(()=>expect(position.value).toBe('851020'))
 expect(trimFilmcraftToPlayhead).toHaveBeenLastCalledWith(expect.objectContaining({document:sourceText,side:'previous',ripple:true,linked:true,timeFrames:'9007199254740993',targetedTrackIds:[]}),expect.any(AbortSignal))
 vi.mocked(trimFilmcraftToPlayhead).mockRejectedValue(new Error('Нет клипа под курсором'));button('Удалить после курсора с зазором').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('Нет клипа под курсором'))
 const request=vi.mocked(trimFilmcraftToPlayhead).mock.calls.at(-1)![0];expect(request).toMatchObject({document:'{"qw":true}',side:'next',ripple:false,timeTicks:'9007199254740993'});expect(request).not.toHaveProperty('timeFrames')
 expect(button('Отменить правку').disabled).toBe(false);button('Отменить правку').click();await tick();expect(document.body.textContent).not.toContain('Нет клипа под курсором')
})

it('integrates dynamic previews as one host undo step after session close', async () => {
 vi.spyOn(globalThis,'setInterval').mockImplementation(()=>0 as unknown as ReturnType<typeof setInterval>)
 await open(sourceText,true)
 vi.mocked(getFilmcraftTransitions).mockResolvedValue([{definition:{id:'cross_dissolve',name:'Cross Dissolve',kind:'VideoTransition',category:[],params:[]},instance:{}}])
 const clip=document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!;clip.value='9007199254740995';clip.dispatchEvent(new Event('change',{bubbles:true}));await tick()
 const transitions=document.querySelector<HTMLDetailsElement>('.transition-controls')!;transitions.open=true;transitions.dispatchEvent(new Event('toggle'));await vi.waitFor(()=>expect(document.querySelector('[aria-label="Режим ближайших точек"]')).not.toBeNull())
 const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===text)!
 button('Добавить точку монтажа').click();await tick()
 const initial=structuredClone(await vi.mocked(importFilmcraftProject).mock.results[0]!.value) as FilmcraftImportResult
 const result=(revision:string,dynamic:boolean,documentText=sourceText,committed=false)=>({...initial,documentText,sessionId:'session',revision,shift:{outShift:'0',inShift:'0'},shiftFrames:{outShift:'0',inShift:'0'},status:{playing:dynamic,playheadTicks:'762048000000',playheadFrames:'72',dynamic:dynamic?{offsetFrames:'2',speed:1,atLimit:false}:null,around:null},commit:committed?{committed:true,label:'Dynamic Trim',offsetTicks:'2',offsetFrames:'2'}:null})
 vi.mocked(createFilmcraftTrimSession).mockResolvedValue(result('0',false));vi.mocked(commandFilmcraftTrimSession).mockResolvedValueOnce(result('1',true,'{"live":1}')).mockResolvedValueOnce(result('2',true,'{"live":2}')).mockResolvedValueOnce(result('3',false,'{"live":2}',true));vi.mocked(readFilmcraftTrimSession).mockResolvedValue(result('3',false,'{"live":2}'));vi.mocked(closeFilmcraftTrimSession).mockResolvedValue({closed:true})
 const position=document.querySelector<HTMLInputElement>('[aria-label="Позиция правки, кадров"]')!;position.value='72';position.dispatchEvent(new Event('input',{bubbles:true}));await tick()
 button('Вперёд (L)').click();await vi.waitFor(()=>expect(commandFilmcraftTrimSession).toHaveBeenCalledTimes(1));await vi.waitFor(()=>expect(button('Вперёд (L)').disabled).toBe(false));expect(createFilmcraftTrimSession).toHaveBeenCalledWith('owner',expect.objectContaining({playheadFrames:'72',points:[{clip:'9007199254740995',out:true,kind:'trim'}]}),undefined)
 expect(button('Подрезать выбранные точки').disabled).toBe(true);button('Вперёд (L)').click();await vi.waitFor(()=>expect(commandFilmcraftTrimSession).toHaveBeenCalledTimes(2));await vi.waitFor(()=>expect(button('Стоп (K)').disabled).toBe(false));button('Стоп (K)').click();await vi.waitFor(()=>expect(commandFilmcraftTrimSession).toHaveBeenCalledTimes(3));await vi.waitFor(()=>expect(button('Закрыть монтажную сессию').disabled).toBe(false));button('Закрыть монтажную сессию').click();await vi.waitFor(()=>expect(button('Отменить правку').disabled).toBe(false));button('Отменить правку').click();await tick();expect(button('Отменить правку').disabled).toBe(true);expect(button('Повторить правку').disabled).toBe(false)
})

it('commits source scalar parameter edits into the project Undo and Redo history', async () => {
  await open()
  const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const editedText = '{"id":18446744073709551615,"sourceParameterEdited":true}'
  vi.mocked(editFilmcraftParameter).mockResolvedValue({ ...loaded, documentText: editedText })
  vi.mocked(getFilmcraftParameters).mockResolvedValue({ locked: false, timelineTicks: '0', parameterTicks: '9007199254740993', frameTicks: '9007199254740993', generatorTicks: '0', effects: [{ index: 0, kind: 'gamma_correction', enabled: true, parameters: [{ name: 'gamma', value: { Float: 1 }, sampled: 1, keys: [] }] }] })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const summary = [...document.querySelectorAll('summary')].find(node => node.textContent === 'Ключи параметров Filmcraft')!
  const details = summary.parentElement as HTMLDetailsElement
  details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  const button = (label: string) => [...document.querySelectorAll('button')].find(node => node.textContent === label)!
  await vi.waitFor(() => expect(button('Ключ Filmcraft в позиции курсора')).toBeTruthy())
  button('Ключ Filmcraft в позиции курсора').click()
  await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('fixture export'))
  button('Отменить правку').click(); await tick(); button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(button('Повторить правку').disabled).toBe(false))
  button('Повторить правку').click(); await tick(); button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: editedText }), 'owner', expect.any(AbortSignal)))
})
it('commits a mask creation into source project Undo and Redo', async () => {
  await open()
  const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const editedText = '{"id":18446744073709551615,"maskCreated":true}'
  vi.mocked(editFilmcraftMask).mockResolvedValue({ ...loaded, documentText: editedText })
  vi.mocked(getFilmcraftParameters).mockResolvedValue({ locked: false, timelineTicks: '0', parameterTicks: '9007199254740993', frameTicks: '9007199254740993', generatorTicks: '0', effects: [{ index: 0, kind: 'gamma_correction', enabled: true, parameters: [], masks: [] }] })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const summary = [...document.querySelectorAll('summary')].find(node => node.textContent === 'Ключи параметров Filmcraft')!
  const details = summary.parentElement as HTMLDetailsElement
  details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  const button = (label: string) => [...document.querySelectorAll('button')].find(node => node.textContent === label)!
  await vi.waitFor(() => expect(button('Создать маску Filmcraft')).toBeTruthy())
  button('Создать маску Filmcraft').click()
  await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('fixture export'))
  button('Отменить правку').click(); await tick(); button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(button('Повторить правку').disabled).toBe(false))
  button('Повторить правку').click(); await tick(); button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: editedText }), 'owner', expect.any(AbortSignal)))
})

it('accepts a tracked mask range as one source-project Undo and Redo operation', async () => {
  await open()
  const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const editedText = '{"id":18446744073709551615,"maskTracked":true}'
  vi.mocked(startFilmcraftMaskTrackingJob).mockResolvedValue({ id: 1, instance: '11111111-1111-4111-8111-111111111111', kind: 'maskTracking', done: 0, total: 1, finished: false, status: '', result: null })
  vi.mocked(readFilmcraftTrackingJob).mockResolvedValue({ id: 1, instance: '11111111-1111-4111-8111-111111111111', kind: 'maskTracking', done: 1, total: 1, finished: true, status: 'Tracked', result: { frames: 1 }, documentText: editedText })
  vi.mocked(importFilmcraftTrackingSnapshot).mockResolvedValue({ ...loaded, documentText: editedText })
  vi.mocked(getFilmcraftParameters).mockResolvedValue({ locked: false, timelineTicks: '0', parameterTicks: '9007199254740993', frameTicks: '9007199254740993', generatorTicks: '0', effects: [{ index: 0, kind: 'opacity', enabled: true, parameters: [], masks: [{ index: 0, name: 'Tracked', parameters: [{ name: 'path', value: { Path: { vertices: [] } }, sampled: null, keys: [] }] }] }] })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const summary = [...document.querySelectorAll('summary')].find(node => node.textContent === 'Ключи параметров Filmcraft')!
  const details = summary.parentElement as HTMLDetailsElement
  details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Маска параметров Filmcraft"]')).toBeTruthy())
  const mask = document.querySelector<HTMLSelectElement>('[aria-label="Маска параметров Filmcraft"]')!
  mask.value = '0'; mask.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const button = (label: string) => [...document.querySelectorAll('button')].find(node => node.textContent === label)!
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
  expect(startFilmcraftMaskTrackingJob).toHaveBeenCalledExactlyOnceWith('owner', expect.objectContaining({ sources: { '9007199254740993': 'asset' }, mask: 0, effect: 0 }))
  expect(trackFilmcraftMask).not.toHaveBeenCalled()
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('fixture export'))
  button('Отменить правку').click(); await tick(); button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(button('Повторить правку').disabled).toBe(false))
  button('Повторить правку').click(); await tick(); button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: editedText }), 'owner', expect.any(AbortSignal)))
})

it('keeps the mask monitor open after a pen edit and records the accepted document once', async () => {
  await open()
  const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const editedText = '{"id":18446744073709551615,"penEdited":true}'
  const overlay = { canvas: [100, 100] as [number, number], matrix: [1, 0, 0, 1, 0, 0] as [number, number, number, number, number, number], sampleTicks: '9007199254740993', locked: false, visible: true, editable: true, closed: false, svgPath: '', vertices: [] }
  vi.mocked(editFilmcraftMaskMonitor).mockImplementation(async input => ({ overlay, project: input.operation.kind === 'read' ? null : { ...loaded, documentText: editedText } }))
  vi.mocked(getFilmcraftParameters).mockResolvedValue({ locked: false, timelineTicks: '0', parameterTicks: '9007199254740993', frameTicks: '9007199254740993', generatorTicks: '0', effects: [{ index: 0, kind: 'opacity', enabled: true, parameters: [], masks: [{ index: 0, name: 'Draw', parameters: [{ name: 'path', value: { Path: { vertices: [] } }, sampled: null, keys: [] }] }] }] })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const details = (text: string) => [...document.querySelectorAll('summary')].find(node => node.textContent === text)!.parentElement as HTMLDetailsElement
  const parent = details('Ключи параметров Filmcraft'); parent.open = true; parent.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Маска параметров Filmcraft"]')).toBeTruthy())
  const mask = document.querySelector<HTMLSelectElement>('[aria-label="Маска параметров Filmcraft"]')!
  mask.value = '0'; mask.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const monitor = details('Маска в мониторе Filmcraft'); monitor.open = true; monitor.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.querySelector('svg[aria-label="Контур маски Filmcraft"]')).toBeTruthy())
  const tool = document.querySelector<HTMLSelectElement>('[aria-label="Инструмент маски в мониторе Filmcraft"]')!
  tool.value = 'pen'; tool.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const svg = document.querySelector<SVGSVGElement>('svg[aria-label="Контур маски Filmcraft"]')!
  Object.defineProperty(svg, 'getBoundingClientRect', { value: () => ({ left: 0, top: 0, width: 200, height: 200 }) })
  for (const [x, y] of [[40, 40], [120, 40], [80, 140]]) {
    svg.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 1, button: 0, clientX: x, clientY: y }))
    svg.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, button: 0, clientX: x, clientY: y }))
    await tick()
  }
  svg.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  const button = (label: string) => [...document.querySelectorAll('button')].find(node => node.textContent === label)!
  await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
  await vi.waitFor(() => expect(getFilmcraftParameters).toHaveBeenCalledTimes(2))
  expect(details('Маска в мониторе Filmcraft')).toBe(monitor)
  expect(monitor.open).toBe(true)
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.filter(call => call[0].operation.kind === 'penDraft' && call[0].operation.closed)).toHaveLength(1)
  button('Отменить правку').click(); await tick()
  expect(button('Отменить правку').disabled).toBe(true)
})

it('creates a mask in the monitor from an empty effect and keeps selection through Undo and Redo', async () => {
  await open()
  const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const editedText = '{"id":18446744073709551615,"penEdited":true}'
  const overlay = { canvas: [100, 100] as [number, number], matrix: [1, 0, 0, 1, 0, 0] as [number, number, number, number, number, number], sampleTicks: '9007199254740993', locked: false, visible: true, editable: true, closed: false, svgPath: '', vertices: [] }
  vi.mocked(editFilmcraftMaskMonitor).mockImplementation(async input => ({ overlay: { ...overlay, ...(input.createMask ? { createdMaskIndex: 0 } : {}) }, project: input.operation.kind === 'read' ? null : { ...loaded, documentText: editedText } }))
  vi.mocked(getFilmcraftParameters).mockImplementation(async input => ({ locked: false, timelineTicks: '0', parameterTicks: '9007199254740993', frameTicks: '9007199254740993', generatorTicks: '0', effects: [{ index: 0, kind: 'opacity', maskable: true, enabled: true, parameters: [], masks: input.document === editedText ? [{ index: 0, name: 'Mask (1)', parameters: [{ name: 'path', value: { Path: { vertices: [] } }, sampled: null, keys: [] }] }] : [] }] }))
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const details = (text: string) => [...document.querySelectorAll('summary')].find(node => node.textContent === text)!.parentElement as HTMLDetailsElement
  const parent = details('Ключи параметров Filmcraft'); parent.open = true; parent.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect([...document.querySelectorAll('summary')].some(node => node.textContent === 'Создать маску в мониторе Filmcraft')).toBe(true))
  const monitor = details('Создать маску в мониторе Filmcraft'); monitor.open = true; monitor.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.querySelector('svg[aria-label="Контур маски Filmcraft"]')).toBeTruthy())
  const tool = document.querySelector<HTMLSelectElement>('[aria-label="Инструмент маски в мониторе Filmcraft"]')!
  expect(tool.value).toBe('pen')
  const svg = document.querySelector<SVGSVGElement>('svg[aria-label="Контур маски Filmcraft"]')!
  Object.defineProperty(svg, 'getBoundingClientRect', { value: () => ({ left: 0, top: 0, width: 200, height: 200 }) })
  for (const [x, y] of [[40, 40], [120, 40], [80, 140]]) {
    svg.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, pointerId: 1, button: 0, clientX: x, clientY: y }))
    svg.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 1, button: 0, clientX: x, clientY: y }))
    await tick()
  }
  svg.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  const button = (label: string) => [...document.querySelectorAll('button')].find(node => node.textContent === label)!
  await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
  await vi.waitFor(() => expect(getFilmcraftParameters).toHaveBeenCalledTimes(2))
  expect(details('Маска в мониторе Filmcraft')).toBe(monitor)
  expect(monitor.open).toBe(true)
  expect(vi.mocked(editFilmcraftMaskMonitor).mock.calls.filter(call => call[0].operation.kind === 'penDraft' && call[0].operation.closed)).toHaveLength(1)
  button('Отменить правку').click(); await tick()
  expect(button('Отменить правку').disabled).toBe(true)
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('export fixture'))
  button('Экспортировать звук').click(); await tick()
  await vi.waitFor(() => expect(vi.mocked(exportFilmcraftAudio).mock.calls.at(-1)![0].document).toBe(loaded.documentText))
  await vi.waitFor(() => expect(button('Повторить правку').disabled).toBe(false))
  button('Повторить правку').click(); await tick()
  button('Экспортировать звук').click(); await tick()
  await vi.waitFor(() => expect(vi.mocked(exportFilmcraftAudio).mock.calls.at(-1)![0].document).toBe(editedText))
})


it('clears mask selection through native body click without adding source Undo history', async () => {
  await open()
  const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const overlay = { canvas: [100, 100] as [number, number], matrix: [1, 0, 0, 1, 0, 0] as [number, number, number, number, number, number], sampleTicks: '9007199254740993', locked: false, visible: true, editable: true, closed: true, svgPath: '', vertices: [] }
  vi.mocked(editFilmcraftMaskMonitor).mockImplementation(async input => ({ overlay: { ...overlay, ...(input.operation.kind === 'bodyClick' ? { deselectMask: true } : {}) }, project: null }))
  vi.mocked(getFilmcraftParameters).mockResolvedValue({ locked: false, timelineTicks: '0', parameterTicks: '9007199254740993', frameTicks: '9007199254740993', generatorTicks: '0', effects: [{ index: 0, kind: 'opacity', enabled: true, parameters: [], masks: [{ index: 0, name: 'Draw', parameters: [{ name: 'path', value: { Path: { vertices: [] } }, sampled: null, keys: [] }] }] }] })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const details = (text: string) => [...document.querySelectorAll('summary')].find(node => node.textContent === text)!.parentElement as HTMLDetailsElement
  const parent = details('Ключи параметров Filmcraft'); parent.open = true; parent.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Маска параметров Filmcraft"]')).toBeTruthy())
  const mask = document.querySelector<HTMLSelectElement>('[aria-label="Маска параметров Filmcraft"]')!
  mask.value = '0'; mask.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const monitor = details('Маска в мониторе Filmcraft'); monitor.open = true; monitor.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.querySelector('svg[aria-label="Контур маски Filmcraft"]')).toBeTruthy())
  const svg = document.querySelector<SVGSVGElement>('svg[aria-label="Контур маски Filmcraft"]')!
  Object.defineProperty(svg, 'getBoundingClientRect', { value: () => ({ left: 0, top: 0, width: 200, height: 200 }) })
  for (const type of ['pointerdown', 'pointerup']) svg.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId: 1, button: 0, clientX: 20, clientY: 100 }))
  await vi.waitFor(() => expect(mask.value).toBe('-1'))
  expect(details('Создать маску в мониторе Filmcraft')).toBe(monitor)
  expect(monitor.open).toBe(false)
  const button = (label: string) => [...document.querySelectorAll('button')].find(node => node.textContent === label)!
  expect(button('Отменить правку').disabled).toBe(true)
  expect(button('Повторить правку').disabled).toBe(true)
  expect(getFilmcraftParameters).toHaveBeenCalledTimes(1)
  button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: loaded.documentText }), 'owner', expect.any(AbortSignal)))
})

it('creates an asymmetric custom mask in one source Undo and Redo step', async () => {
  await open()
  const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const editedText = '{"id":18446744073709551615,"maskCreated":true}'
  vi.mocked(editFilmcraftMask).mockResolvedValue({ ...loaded, documentText: editedText })
  vi.mocked(getFilmcraftParameters).mockResolvedValue({ locked: false, timelineTicks: '0', parameterTicks: '9007199254740993', frameTicks: '9007199254740993', generatorTicks: '0', effects: [{ index: 0, kind: 'gamma_correction', enabled: true, parameters: [], masks: [] }] })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const summary = [...document.querySelectorAll('summary')].find(node => node.textContent === 'Ключи параметров Filmcraft')!
  const details = summary.parentElement as HTMLDetailsElement
  details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  const button = (label: string) => [...document.querySelectorAll('button')].find(node => node.textContent === label)!
  await vi.waitFor(() => expect(button('Создать маску Filmcraft')).toBeTruthy())
  const shape = document.querySelector<HTMLSelectElement>('[aria-label="Форма новой маски Filmcraft"]')!
  shape.value = 'path'; shape.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const number = async (label: string, value: string) => { const field = document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!; field.value = value; field.dispatchEvent(new Event('input', { bubbles: true })); await tick() }
  await number('Вершина 1 X Filmcraft', '5')
  button('Добавить t_in вершины 1').click(); button('Добавить t_out вершины 1').click(); await tick()
  await number('Вершина 1 t_in X Filmcraft', '-2'); await number('Вершина 1 t_in Y Filmcraft', '1')
  await number('Вершина 1 t_out X Filmcraft', '12'); await number('Вершина 1 t_out Y Filmcraft', '-3')
  expect(button('Отменить правку').disabled).toBe(true)
  expect(editFilmcraftMask).not.toHaveBeenCalled()
  button('Создать маску Filmcraft').click()
  await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
  expect(editFilmcraftMask).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ command: { kind: 'create', name: 'Маска', shape: { kind: 'path', path: { closed: true, vertices: [{ p: { x: 5, y: 0 }, t_in: { x: -2, y: 1 }, t_out: { x: 12, y: -3 } }, { p: { x: 100, y: 0 } }, { p: { x: 0, y: 100 } }] } } } }), expect.any(AbortSignal))
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('fixture export'))
  button('Отменить правку').click(); await tick(); button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(button('Повторить правку').disabled).toBe(false))
  button('Повторить правку').click(); await tick(); button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: editedText }), 'owner', expect.any(AbortSignal)))
})


it('binds a library still image by media type and keeps tracking in one Undo step', async () => {
  await open(sourceText, false, false, [{ id: 'still', kind: 'source', filename: 'opaque-image', mediaType: 'image', url: '' } as MediaEntry])
  const binding = document.querySelector<HTMLSelectElement>('[aria-label="Файл для Wave"]')!
  expect([...binding.options].some(option => option.value === 'still')).toBe(true)
  binding.value = 'still'; binding.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const editedText = '{"id":18446744073709551615,"maskTracked":true}'
  vi.mocked(startFilmcraftMaskTrackingJob).mockResolvedValue({ id: 1, instance: '11111111-1111-4111-8111-111111111111', kind: 'maskTracking', done: 0, total: 1, finished: false, status: '', result: null })
  vi.mocked(readFilmcraftTrackingJob).mockResolvedValue({ id: 1, instance: '11111111-1111-4111-8111-111111111111', kind: 'maskTracking', done: 1, total: 1, finished: true, status: 'Tracked', result: { frames: 1 }, documentText: editedText })
  vi.mocked(importFilmcraftTrackingSnapshot).mockResolvedValue({ ...loaded, documentText: editedText })
  vi.mocked(getFilmcraftParameters).mockResolvedValue({ locked: false, timelineTicks: '0', parameterTicks: '9007199254740993', frameTicks: '9007199254740993', generatorTicks: '0', effects: [{ index: 0, kind: 'opacity', enabled: true, parameters: [], masks: [{ index: 0, name: 'Tracked', parameters: [{ name: 'path', value: { Path: { vertices: [] } }, sampled: null, keys: [] }] }] }] })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const summary = [...document.querySelectorAll('summary')].find(node => node.textContent === 'Ключи параметров Filmcraft')!
  const details = summary.parentElement as HTMLDetailsElement
  details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Маска параметров Filmcraft"]')).toBeTruthy())
  const mask = document.querySelector<HTMLSelectElement>('[aria-label="Маска параметров Filmcraft"]')!
  mask.value = '0'; mask.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const button = (label: string) => [...document.querySelectorAll('button')].find(node => node.textContent === label)!
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
  expect(startFilmcraftMaskTrackingJob).toHaveBeenCalledExactlyOnceWith('owner', expect.objectContaining({ sources: { '9007199254740993': 'still' }, mask: 0, effect: 0, frames: 12 }))
  expect(trackFilmcraftMask).not.toHaveBeenCalled()
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('fixture export'))
  button('Отменить правку').click(); await tick(); button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(button('Повторить правку').disabled).toBe(false))
  button('Повторить правку').click(); await tick(); button('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: editedText }), 'owner', expect.any(AbortSignal)))
})

it('relinks an uploaded UUID filename using its preserved original basename', async () => {
  await open(sourceText, false, false, [{ id: 'original-upload', kind: 'source', filename: '11111111-1111-4111-8111-111111111111.wav', originalFilename: 'pattern.wav', mediaType: 'audio', url: '' } as MediaEntry])
  const binding = document.querySelector<HTMLSelectElement>('[aria-label="Файл для Wave"]')!
  expect(binding.value).toBe('original-upload')
})

it('exports the selected source frame as a downloadable PNG with exact identifiers', async () => {
  const png = new Blob([new Uint8Array([137, 80, 78, 71])], { type: 'image/png' })
  vi.mocked(exportFilmcraftFrame).mockResolvedValue(png)
  const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:frame')
  vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
  const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) { expect(this.download).toBe('frame.png') })
  await open()
  ;[...document.querySelectorAll('button')].find(button => button.textContent === 'Экспортировать кадр PNG')!.click()
  await vi.waitFor(() => expect(exportFilmcraftFrame).toHaveBeenCalled())
  expect(vi.mocked(exportFilmcraftFrame).mock.calls[0][0]).toMatchObject({ document: sourceText, sequenceId: '18446744073709551615', timeFrames: '0', sources: { '9007199254740993': 'asset' } })
  await vi.waitFor(() => expect(click).toHaveBeenCalled())
  expect(create).toHaveBeenCalledWith(png)
})

it('binds a non-WAV library audio file into the audiovisual export job', async () => {
  const { startFilmcraftVideoJob, getFilmcraftVideoJob } = await import('$lib/api')
  vi.mocked(startFilmcraftVideoJob).mockResolvedValue({ jobId: 'av-job', mode: 'video_audio', container: 'mkv' })
  vi.mocked(getFilmcraftVideoJob).mockResolvedValue({ id: 'av-job', status: 'done', result: { id: '12345678-1234-1234-1234-123456789abc' } })
  await open(sourceText, false, false, [{ id: 'flac', kind: 'source', filename: 'signal.flac', mediaType: 'audio', url: '' } as MediaEntry])
  const select = document.querySelector<HTMLSelectElement>('select[aria-label="Файл для Wave"]')!
  expect([...select.options].some(option => option.value === 'flac')).toBe(true)
  select.value = 'flac'; select.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  document.querySelector<HTMLInputElement>('[aria-label="Добавить звук в видеоэкспорт Filmcraft"]')!.click(); await tick()
  ;[...document.querySelectorAll('button')].find(button => button.textContent === 'Экспортировать видео со звуком')!.click()
  await vi.waitFor(() => expect(startFilmcraftVideoJob).toHaveBeenCalledWith('owner', expect.objectContaining({ document: sourceText, withAudio: true, sources: { '9007199254740993': 'flac' } }), expect.any(AbortSignal)))
})
async function openRackControls(withParameter = false) {
  vi.mocked(getFilmcraftParameters).mockResolvedValue({ locked: false, timelineTicks: '0', parameterTicks: '0', frameTicks: '0', generatorTicks: '0', effects: [{ index: 0, kind: 'gamma_correction', enabled: true, parameters: withParameter ? [{ name: 'gamma', value: { Float: 1 }, sampled: 1, keys: [] }] : [] }] })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const details = [...document.querySelectorAll('summary')].find(node => node.textContent === 'Ключи параметров Filmcraft')!.parentElement as HTMLDetailsElement
  details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Сбросить эффект Filmcraft"]')).toBeTruthy())
}
function rackButton(label: string) { return document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)! }
function projectButton(label: string) { return [...document.querySelectorAll('button')].find(node => node.textContent === label)! }
it.each(['effects.reset', 'effects.toggleEnabled', 'effects.remove'] as const)('commits %s into project Undo and Redo with exact source text', async command => {
  await open(); const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const changedText = '{"id":18446744073709551615,"rackEdited":true}'
  vi.mocked(editFilmcraftEffectRack).mockResolvedValue({ ...loaded, documentText: changedText })
  await openRackControls()
  rackButton(command === 'effects.reset' ? 'Сбросить эффект Filmcraft' : command === 'effects.remove' ? 'Удалить эффект Filmcraft' : 'Переключить эффект Filmcraft').click()
  await vi.waitFor(() => expect(projectButton('Отменить правку').disabled).toBe(false))
  expect(editFilmcraftEffectRack).toHaveBeenCalledWith('owner', { document: sourceText, sequence: '18446744073709551615', clip: '9007199254740995', index: 0, command }, expect.any(AbortSignal))
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('export fixture'))
  projectButton('Отменить правку').click(); await tick(); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(projectButton('Повторить правку').disabled).toBe(false))
  projectButton('Повторить правку').click(); await tick(); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: changedText }), 'owner', expect.any(AbortSignal)))
})
it('fences rack adoption when Undo happens before the queued response continuation', async () => {
  await open(); const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const trimmedText = '{"id":18446744073709551615,"trimmed":true}'
  vi.mocked(trimFilmcraftProject).mockResolvedValue({ ...loaded, documentText: trimmedText, appliedDeltaTicks: '1' })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!; clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  projectButton('Обрезать').click(); await vi.waitFor(() => expect(projectButton('Отменить правку').disabled).toBe(false))
  let resolve!: (result: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftEffectRack).mockImplementation(() => new Promise(done => resolve = done))
  await openRackControls(); rackButton('Сбросить эффект Filmcraft').click()
  await vi.waitFor(() => expect(editFilmcraftEffectRack).toHaveBeenCalledTimes(1))
  resolve({ ...loaded, documentText: '{"staleRack":true}' })
  projectButton('Отменить правку').click()
  await tick(); await tick()
  expect(projectButton('Отменить правку').disabled).toBe(true)
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('export fixture')); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
})
it('fences rack adoption when clip selection changes before the queued response continuation', async () => {
  await open(); const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  let resolve!: (result: FilmcraftImportResult) => void
  vi.mocked(editFilmcraftEffectRack).mockImplementation(() => new Promise(done => resolve = done))
  await openRackControls(); rackButton('Сбросить эффект Filmcraft').click()
  await vi.waitFor(() => expect(editFilmcraftEffectRack).toHaveBeenCalledTimes(1))
  resolve({ ...loaded, documentText: '{"staleRack":true}' })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!; clip.value = '9007199254740997'; clip.dispatchEvent(new Event('change', { bubbles: true }))
  await tick(); await tick()
  expect(projectButton('Отменить правку').disabled).toBe(true)
})
function catalogOption(id: string, kind: 'Video' | 'Audio') { return { id, name: id, kind, category: [], folder: null, path: '', params: [], badges: { accelerated: true, float32: true, yuv: false } } }
async function openEffectPicker() { await vi.waitFor(() => expect(projectButton('Добавить эффект…')).toBeTruthy()); projectButton('Добавить эффект…').click(); await tick(); await vi.waitFor(() => expect(document.querySelector<HTMLSelectElement>('[aria-label="Новый эффект Filmcraft"]')!.options.length).toBeGreaterThan(0)) }
it.each(['Video', 'Audio'] as const)('adds a %s effect to the selected source clip with Undo and Redo', async kind => {
  await open(sourceText, kind === 'Audio')
  const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const changedText = '{"id":18446744073709551615,"effectAdded":true}'
  const effect = kind === 'Audio' ? 'volume_a' : 'gaussian_blur'
  vi.mocked(listFilmcraftEffects).mockResolvedValue([catalogOption(effect, kind)])
  vi.mocked(applyFilmcraftEffect).mockResolvedValue({ ...loaded, documentText: changedText })
  await openRackControls()
  const clipId = kind === 'Audio' ? '9007199254740999' : '9007199254740995'
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!; clip.value = clipId; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  await openEffectPicker()
  expect(listFilmcraftEffects).toHaveBeenLastCalledWith('owner', kind, expect.any(AbortSignal))
  rackButton('Применить новый эффект Filmcraft').click()
  await vi.waitFor(() => expect(projectButton('Отменить правку').disabled).toBe(false))
  expect(applyFilmcraftEffect).toHaveBeenCalledWith('owner', { document: sourceText, sequence: '18446744073709551615', clip: clipId, effect }, expect.any(AbortSignal))
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('export fixture'))
  projectButton('Отменить правку').click(); await tick(); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(projectButton('Повторить правку').disabled).toBe(false))
  projectButton('Повторить правку').click(); await tick(); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: changedText }), 'owner', expect.any(AbortSignal)))
})
it('refuses a queued add-effect response after project Undo', async () => {
  await open(); const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  vi.mocked(trimFilmcraftProject).mockResolvedValue({ ...loaded, documentText: '{"trimmed":true}', appliedDeltaTicks: '1' })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!; clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  projectButton('Обрезать').click(); await vi.waitFor(() => expect(projectButton('Отменить правку').disabled).toBe(false))
  let resolve!: (project: FilmcraftImportResult) => void
  vi.mocked(applyFilmcraftEffect).mockImplementation(() => new Promise(done => resolve = done))
  vi.mocked(listFilmcraftEffects).mockResolvedValue([catalogOption('gaussian_blur', 'Video')])
  await openRackControls(); await openEffectPicker(); rackButton('Применить новый эффект Filmcraft').click()
  await vi.waitFor(() => expect(applyFilmcraftEffect).toHaveBeenCalledTimes(1))
  resolve({ ...loaded, documentText: '{"staleAdd":true}' }); projectButton('Отменить правку').click()
  await tick(); await tick(); expect(projectButton('Отменить правку').disabled).toBe(true)
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('export fixture')); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
})

it('commits source setParam into exact project Undo and Redo snapshots', async () => {
  await open(); const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const changedText = '{"id":18446744073709551615,"parameterEdited":true}'
  vi.mocked(setFilmcraftEffectParameter).mockResolvedValue({ ...loaded, documentText: changedText })
  await openRackControls(true)
  projectButton('Применить значение в позиции курсора').click()
  await vi.waitFor(() => expect(projectButton('Отменить правку').disabled).toBe(false))
  expect(setFilmcraftEffectParameter).toHaveBeenCalledWith('owner', expect.objectContaining({ document: sourceText, sequence: '18446744073709551615', clip: '9007199254740995', effect: 0, param: 'gamma', value: { Float: 1 } }), expect.any(AbortSignal))
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('export fixture'))
  projectButton('Отменить правку').click(); await tick(); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(projectButton('Повторить правку').disabled).toBe(false))
  projectButton('Повторить правку').click(); await tick(); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: changedText }), 'owner', expect.any(AbortSignal)))
})
it('rejects a queued source setParam response when project Undo wins before continuation', async () => {
  await open(); const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  vi.mocked(trimFilmcraftProject).mockResolvedValue({ ...loaded, documentText: '{"trimmed":true}', appliedDeltaTicks: '1' })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!; clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  projectButton('Обрезать').click(); await vi.waitFor(() => expect(projectButton('Отменить правку').disabled).toBe(false))
  let resolve!: (result: FilmcraftImportResult) => void
  vi.mocked(setFilmcraftEffectParameter).mockImplementation(() => new Promise(done => resolve = done))
  await openRackControls(true); projectButton('Применить значение в позиции курсора').click()
  await vi.waitFor(() => expect(setFilmcraftEffectParameter).toHaveBeenCalledTimes(1))
  resolve({ ...loaded, documentText: '{"staleParameter":true}' }); projectButton('Отменить правку').click()
  await tick(); await tick(); expect(projectButton('Отменить правку').disabled).toBe(true)
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('export fixture')); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
})
it.each(['effects.toggleAnimation', 'effects.addKeyframe'] as const)('commits %s into exact Undo and Redo snapshots', async command => {
  await open(); const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const changedText = '{"id":18446744073709551615,"animationEdited":true}'
  vi.mocked(editFilmcraftAnimation).mockResolvedValue({ ...loaded, documentText: changedText })
  await openRackControls(true)
  projectButton(command === 'effects.toggleAnimation' ? 'Включить анимацию' : 'Добавить/удалить ключ в позиции курсора').click()
  await vi.waitFor(() => expect(projectButton('Отменить правку').disabled).toBe(false))
  expect(editFilmcraftAnimation).toHaveBeenCalledWith('owner', expect.objectContaining({ command, document: sourceText, sequence: '18446744073709551615', clip: '9007199254740995', effect: 0, param: 'gamma', playhead: '0' }), expect.any(AbortSignal))
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('export fixture'))
  projectButton('Отменить правку').click(); await tick(); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
  await vi.waitFor(() => expect(projectButton('Повторить правку').disabled).toBe(false))
  projectButton('Повторить правку').click(); await tick(); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: changedText }), 'owner', expect.any(AbortSignal)))
})
async function openNativeTrackingControls() {
  vi.mocked(getFilmcraftParameters).mockResolvedValue({ locked: false, timelineTicks: '0', parameterTicks: '0', frameTicks: '0', generatorTicks: '0', effects: [{ index: 0, kind: 'opacity', enabled: true, parameters: [], masks: [{ index: 0, name: 'Track', parameters: [{ name: 'path', value: { Path: { vertices: [] } }, sampled: null, keys: [] }] }] }] })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!; clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const details = [...document.querySelectorAll('summary')].find(node => node.textContent === 'Ключи параметров Filmcraft')!.parentElement as HTMLDetailsElement
  details.open = true; details.dispatchEvent(new Event('toggle')); await tick()
  await vi.waitFor(() => expect(document.querySelector('[aria-label="Маска параметров Filmcraft"]')).toBeTruthy())
  const mask = document.querySelector<HTMLSelectElement>('[aria-label="Маска параметров Filmcraft"]')!; mask.value = '0'; mask.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  vi.mocked(startFilmcraftMaskTrackingJob).mockResolvedValue({ id: 1, instance: '11111111-1111-4111-8111-111111111111', kind: 'maskTracking', done: 0, total: 2, finished: false, status: '', result: null })
  vi.mocked(readFilmcraftTrackingJob).mockResolvedValue({ id: 1, instance: '11111111-1111-4111-8111-111111111111', kind: 'maskTracking', done: 2, total: 2, finished: true, status: 'Tracked', result: { frames: 2 }, documentText: 'tracked' })
  vi.mocked(cancelFilmcraftTrackingJob).mockResolvedValue(null)
}
it('rejects a queued native tracking import when project Undo wins before continuation', async () => {
  await open(); const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!; clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  vi.mocked(trimFilmcraftProject).mockResolvedValue({ ...loaded, documentText: '{"trimmed":true}', appliedDeltaTicks: '1' })
  projectButton('Обрезать').click(); await vi.waitFor(() => expect(projectButton('Отменить правку').disabled).toBe(false))
  await openNativeTrackingControls()
  let finish!: (value: FilmcraftImportResult) => void
  vi.mocked(importFilmcraftTrackingSnapshot).mockImplementation(() => new Promise(resolve => finish = resolve))
  projectButton('Трекинг маски Filmcraft вперёд').click(); await vi.waitFor(() => expect(importFilmcraftTrackingSnapshot).toHaveBeenCalledTimes(1))
  finish({ ...loaded, documentText: '{"staleTracking":true}' }); projectButton('Отменить правку').click()
  await tick(); await tick(); expect(projectButton('Отменить правку').disabled).toBe(true)
  vi.mocked(exportFilmcraftAudio).mockRejectedValue(new Error('export fixture')); projectButton('Экспортировать звук').click()
  await vi.waitFor(() => expect(exportFilmcraftAudio).toHaveBeenLastCalledWith(expect.objectContaining({ document: sourceText }), 'owner', expect.any(AbortSignal)))
})
it('rejects a queued native tracking import after clip selection changes in the same turn', async () => {
  await open(); const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  await openNativeTrackingControls(); let finish!: (value: FilmcraftImportResult) => void
  vi.mocked(importFilmcraftTrackingSnapshot).mockImplementation(() => new Promise(resolve => finish = resolve))
  projectButton('Трекинг маски Filmcraft вперёд').click(); await vi.waitFor(() => expect(importFilmcraftTrackingSnapshot).toHaveBeenCalledTimes(1))
  finish({ ...loaded, documentText: '{"staleClipTracking":true}' })
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!; clip.value = '9007199254740997'; clip.dispatchEvent(new Event('change', { bubbles: true }))
  await tick(); await tick(); expect(projectButton('Отменить правку').disabled).toBe(true)
})
it.each(['source binding', 'clock'] as const)('rejects queued tracking adoption after the %s changes before child props update', async changed => {
  await open(sourceText, false, false, [{ id: 'asset', kind: 'source', filename: 'source.wav', mediaType: 'audio', url: '' }, { id: 'replacement', kind: 'source', filename: 'replacement.png', mediaType: 'image', url: '' }] as MediaEntry[])
  const loaded: FilmcraftImportResult = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  await openNativeTrackingControls(); let finish!: (value: FilmcraftImportResult) => void
  vi.mocked(importFilmcraftTrackingSnapshot).mockImplementation(() => new Promise(resolve => finish = resolve))
  projectButton('Трекинг маски Filmcraft вперёд').click(); await vi.waitFor(() => expect(importFilmcraftTrackingSnapshot).toHaveBeenCalledTimes(1))
  finish({ ...loaded, documentText: '{"staleBindingOrClock":true}' })
  if (changed === 'source binding') {
    const binding = document.querySelector<HTMLSelectElement>('[aria-label="Файл для Wave"]')!; binding.value = 'replacement'; binding.dispatchEvent(new Event('change', { bubbles: true }))
  } else {
    const cursor = document.querySelector<HTMLInputElement>('[aria-label="Позиция правки, кадров"]')!; cursor.value = '1'; cursor.dispatchEvent(new Event('input', { bubbles: true }))
  }
  await tick(); await tick(); expect(projectButton('Отменить правку').disabled).toBe(true)
})

async function submitClipRename() {
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740995'; clip.dispatchEvent(new Event('change', { bubbles: true })); await tick()
  const input = document.querySelector<HTMLInputElement>('[aria-label="Имя клипа Filmcraft"]')!
  input.value = 'Renamed'; input.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  input.closest('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await tick()
}
it('adopts clip rename as one undoable project edit', async () => {
  await open()
  const loaded: Awaited<ReturnType<typeof importFilmcraftProject>> = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  vi.mocked(renameFilmcraftClip).mockResolvedValue({ ...loaded, documentText: 'renamed', sequences: loaded.sequences.map(seq => ({ ...seq, clips: seq.clips?.map(clip => ({ ...clip, name: clip.id === '9007199254740995' ? 'Renamed' : clip.name })) })) })
  await submitClipRename()
  const button = (label: string) => [...document.querySelectorAll('button')].find(item => item.textContent === label)!
  await vi.waitFor(() => expect(button('Отменить правку').disabled).toBe(false))
  expect(document.querySelector<HTMLInputElement>('[aria-label="Имя клипа Filmcraft"]')!.value).toBe('Renamed')
  button('Отменить правку').click(); await tick()
  expect(document.querySelector<HTMLInputElement>('[aria-label="Имя клипа Filmcraft"]')!.value).toBe('Shot')
  expect(button('Отменить правку').disabled).toBe(true)
  button('Повторить правку').click(); await tick()
  expect(document.querySelector<HTMLInputElement>('[aria-label="Имя клипа Filmcraft"]')!.value).toBe('Renamed')
})
it('refuses a queued rename after changing clip selection', async () => {
  await open()
  const loaded: Awaited<ReturnType<typeof importFilmcraftProject>> = await vi.mocked(importFilmcraftProject).mock.results[0]!.value
  let resolve!: (result: typeof loaded) => void
  vi.mocked(renameFilmcraftClip).mockImplementation(() => new Promise(accept => { resolve = accept }))
  await submitClipRename()
  const clip = document.querySelector<HTMLSelectElement>('[aria-label="Клип Filmcraft"]')!
  clip.value = '9007199254740997'; clip.dispatchEvent(new Event('change', { bubbles: true }))
  resolve({ ...loaded, documentText: 'stale rename' }); await tick(); await tick()
  expect([...document.querySelectorAll('button')].find(item => item.textContent === 'Отменить правку')!.disabled).toBe(true)
  expect(document.querySelector<HTMLInputElement>('[aria-label="Имя клипа Filmcraft"]')!.value).toBe('Right shot')
})
