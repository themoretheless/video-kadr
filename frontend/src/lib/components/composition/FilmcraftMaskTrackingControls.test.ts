// @ts-expect-error Tests require the Svelte client entry under Vitest SSR resolution.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, expect, it, vi } from 'vitest'
import { startFilmcraftMaskTrackingJob, readFilmcraftTrackingJob, cancelFilmcraftTrackingJob, importFilmcraftTrackingSnapshot, type FilmcraftTrackingJob, trackFilmcraftMask, type FilmcraftMaskTrackingResult } from '$lib/api'
import Controls from './FilmcraftMaskTrackingControls.svelte'
import ContextHarness from './fixtures/FilmcraftTrackingContextHarness.svelte'
import { writable } from 'svelte/store'
vi.mock('$lib/api', () => ({ startFilmcraftMaskTrackingJob: vi.fn(), readFilmcraftTrackingJob: vi.fn(), cancelFilmcraftTrackingJob: vi.fn(), importFilmcraftTrackingSnapshot: vi.fn(), trackFilmcraftMask: vi.fn() }))
let component: ReturnType<typeof mount> | undefined
const anonymousFence = { document: 'opaque original', sequenceId: '18446744073709551615', clipId: '18446744073709551614', token: null, sourceBindings: { '1': 'movie' }, timeFrames: '0', timeTicks: '9007199254740993' }
const address = { sequenceId: '18446744073709551615', clipId: '18446744073709551614', effectIndex: 2, effectKind: 'opacity', maskIndex: 0, parameter: 'path' as const }
function result(documentText: string, sourceTicks: string, more = true, status: 'tracked' | 'lost' | 'boundary' = 'tracked'): FilmcraftMaskTrackingResult { return { documentText, originalSchema: 12, sequences: [], media: [], tracking: { status, sourceTicks, more, inliers: 12 } } }
afterEach(async () => { if (component) await unmount(component); component = undefined; vi.resetAllMocks(); document.body.innerHTML = '' })
async function open(sources: Record<string, string> = { '1': 'movie' }) { const onedit = vi.fn(), onbusy = vi.fn(); component = mount(Controls, { target: document.body, props: { document: 'opaque original', address, sources, timeTicks: '9007199254740993', timeFrames: '0', token: null as string | null, onedit, onbusy } }); await tick(); return { onedit, onbusy } }
const button = (label: string) => [...document.querySelectorAll('button')].find(b => b.textContent === label)!
it('tracks a range using only native exact cursor strings and publishes one accepted document', async () => {
  const { onedit, onbusy } = await open()
  const first = result('tracked one', '9007219841940993'), last = result('tracked two', '9007240629140993', false)
  vi.mocked(trackFilmcraftMask).mockResolvedValueOnce(first).mockResolvedValueOnce(last)
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(last, anonymousFence))
  expect(trackFilmcraftMask).toHaveBeenNthCalledWith(1, { document: 'opaque original', address, sources: { '1': 'movie' }, timeTicks: '9007199254740993', direction: 'forward' }, null, expect.any(AbortSignal))
  expect(trackFilmcraftMask).toHaveBeenNthCalledWith(2, { document: 'tracked one', address, sources: { '1': 'movie' }, timeTicks: '9007199254740993', cursorTicks: first.tracking.sourceTicks, direction: 'forward' }, null, expect.any(AbortSignal))
  expect(onbusy.mock.calls.map(c => c[0])).toEqual([true, false])
})
it('keeps successful steps when tracking is lost', async () => {
  const { onedit } = await open()
  const accepted = result('tracked one', '9007219841940993')
  vi.mocked(trackFilmcraftMask).mockResolvedValueOnce(accepted).mockResolvedValueOnce(result('tracked one', accepted.tracking.sourceTicks, false, 'lost'))
  button('Трекинг маски Filmcraft назад').click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(accepted, anonymousFence))
  expect(document.querySelector('[role=status]')?.textContent).toContain('потерян после 1')
})
it.each(['lost', 'boundary'] as const)('keeps the document untouched on initial %s', async status => {
  const { onedit, onbusy } = await open()
  vi.mocked(trackFilmcraftMask).mockResolvedValue(result('opaque original', '9007199254740993', false, status))
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(onbusy).toHaveBeenLastCalledWith(false))
  expect(onedit).not.toHaveBeenCalled()
})
function pending() { vi.mocked(trackFilmcraftMask).mockImplementationOnce((_input, _token, signal) => new Promise((_resolve, reject) => signal?.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError')), { once: true }))) }
it('stops the active step and saves only the successful prefix', async () => {
  const { onedit } = await open(); const accepted = result('tracked one', '9007219841940993')
  vi.mocked(trackFilmcraftMask).mockResolvedValueOnce(accepted); pending()
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(trackFilmcraftMask).toHaveBeenCalledTimes(2))
  button('Остановить трекинг маски Filmcraft').click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(accepted, anonymousFence))
  expect(document.querySelector('[role=status]')?.textContent).toContain('сохранено 1')
})
it('discards the pending tracking batch when unmounted', async () => {
  const { onedit } = await open(); vi.mocked(trackFilmcraftMask).mockResolvedValueOnce(result('tracked one', '9007219841940993')); pending()
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(trackFilmcraftMask).toHaveBeenCalledTimes(2))
  await unmount(component!); component = undefined; await tick(); await tick()
  expect(onedit).not.toHaveBeenCalled()
  expect(vi.mocked(trackFilmcraftMask).mock.calls[1]![2]?.aborted).toBe(true)
})
it('rejects invalid range lengths before any request', async () => {
  const { onedit } = await open(); const input = document.querySelector<HTMLInputElement>('[aria-label="Кадров трекинга Filmcraft"]')!
  input.value = '4096'; input.dispatchEvent(new Event('input', { bubbles: true })); await tick()
  button('Трекинг маски Filmcraft вперёд').click(); await tick()
  expect(trackFilmcraftMask).not.toHaveBeenCalled(); expect(onedit).not.toHaveBeenCalled()
  expect(document.querySelector('[role=alert]')?.textContent).toContain('4095')
})

it.each(['document', 'token', 'sources'] as const)('discards a partial tracking batch when %s changes', async field => {
  const onedit = vi.fn(), onbusy = vi.fn()
  const initial = { document: 'opaque original', address, sources: { '1': 'movie' }, timeFrames: '0', token: null as string | null, onedit, onbusy }
  const context = writable(initial)
  component = mount(ContextHarness, { target: document.body, props: { context } }); await tick()
  vi.mocked(trackFilmcraftMask).mockResolvedValueOnce(result('tracked one', '9007219841940993')); pending()
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(trackFilmcraftMask).toHaveBeenCalledTimes(2))
  context.set({ ...initial, ...(field === 'sources' ? { sources: { '1': 'replacement' } } : { [field]: 'changed' }) }); await tick()
  await vi.waitFor(() => expect(onbusy).toHaveBeenLastCalledWith(false))
  expect(onedit).not.toHaveBeenCalled()
  expect(vi.mocked(trackFilmcraftMask).mock.calls[1]![2]?.aborted).toBe(true)
})

it('tracks an embedded generator with empty library bindings and one history publication', async () => {
  const { onedit } = await open({})
  const accepted = result('generator tracked', '9007219841940993', false)
  vi.mocked(trackFilmcraftMask).mockResolvedValue(accepted)
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(accepted, { ...anonymousFence, sourceBindings: {} }))
  expect(trackFilmcraftMask).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ sources: {}, direction: 'forward', document: 'opaque original' }), null, expect.any(AbortSignal))
})
function job(done = 0, finished = false, extra: Partial<FilmcraftTrackingJob> = {}): FilmcraftTrackingJob { return { id: '9007199254740993', instance: '11111111-1111-4111-8111-111111111111', kind: 'maskTracking', done, total: 3, finished, status: '', result: finished ? { frames: done } : null, ...extra } }
const nativeFence = { ...anonymousFence, token: 'owner' }
async function openNative() {
  const onedit = vi.fn(), onbusy = vi.fn()
  component = mount(Controls, { target: document.body, props: { document: 'opaque original', address, sources: { '1': 'movie' }, timeTicks: '9007199254740993', timeFrames: '0', token: 'owner', onedit, onbusy } })
  vi.mocked(startFilmcraftMaskTrackingJob).mockResolvedValue(job())
  vi.mocked(cancelFilmcraftTrackingJob).mockResolvedValue(null)
  await tick(); return { onedit, onbusy }
}
it('observes native background progress and reimports one final fenced snapshot', async () => {
  const { onedit, onbusy } = await openNative()
  let finish!: (row: FilmcraftTrackingJob) => void
  vi.mocked(readFilmcraftTrackingJob).mockResolvedValueOnce(job(1)).mockImplementationOnce(() => new Promise(resolve => finish = resolve)).mockResolvedValueOnce(job(3, true, { documentText: 'tracked exact' }))
  const accepted = result('tracked exact', '1'); vi.mocked(importFilmcraftTrackingSnapshot).mockResolvedValue(accepted)
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(readFilmcraftTrackingJob).toHaveBeenCalledTimes(2))
  const progress = document.querySelector<HTMLProgressElement>('progress')!; expect(progress.max).toBe(3); expect(progress.value).toBe(1)
  finish(job(3, true))
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(accepted, nativeFence))
  expect(startFilmcraftMaskTrackingJob).toHaveBeenCalledExactlyOnceWith('owner', { document: 'opaque original', sequence: address.sequenceId, clip: address.clipId, effect: 2, mask: 0, frames: 12, sources: { '1': 'movie' }, direction: 'forward', timeTicks: '9007199254740993' })
  expect(readFilmcraftTrackingJob).toHaveBeenLastCalledWith('owner', expect.objectContaining({ id: '9007199254740993', instance: '11111111-1111-4111-8111-111111111111' }), true, expect.any(AbortSignal))
  expect(trackFilmcraftMask).not.toHaveBeenCalled(); expect(onbusy.mock.calls.map(c => c[0])).toEqual([true, false])
})
it('cancels the server job without aborting observation and commits the completed prefix once', async () => {
  const { onedit } = await openNative()
  let finish!: (row: FilmcraftTrackingJob) => void
  vi.mocked(readFilmcraftTrackingJob).mockImplementationOnce(() => new Promise(resolve => finish = resolve)).mockResolvedValueOnce(job(2, true, { documentText: 'partial exact' }))
  const accepted = result('partial exact', '1'); vi.mocked(importFilmcraftTrackingSnapshot).mockResolvedValue(accepted)
  button('Трекинг маски Filmcraft назад').click(); await vi.waitFor(() => expect(readFilmcraftTrackingJob).toHaveBeenCalledTimes(1))
  const signal = vi.mocked(readFilmcraftTrackingJob).mock.calls[0]![3]!
  button('Остановить трекинг маски Filmcraft').click()
  await vi.waitFor(() => expect(cancelFilmcraftTrackingJob).toHaveBeenCalledExactlyOnceWith('owner', expect.objectContaining({ id: '9007199254740993', instance: '11111111-1111-4111-8111-111111111111' })))
  expect(signal.aborted).toBe(false)
  finish(job(2, true))
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(accepted, nativeFence))
  expect(document.querySelector('[role=status]')?.textContent).toContain('сохранено 2')
})
it('remembers Stop during admission and cancels the actual returned job without replaying start', async () => {
  const { onedit } = await openNative(); let admit!: (row: FilmcraftTrackingJob) => void
  vi.mocked(startFilmcraftMaskTrackingJob).mockImplementation(() => new Promise(resolve => admit = resolve))
  vi.mocked(readFilmcraftTrackingJob).mockResolvedValue(job(0, true, { documentText: 'opaque original' }))
  vi.mocked(importFilmcraftTrackingSnapshot).mockResolvedValue(result('opaque original', '0'))
  button('Трекинг маски Filmcraft вперёд').click(); await vi.waitFor(() => expect(startFilmcraftMaskTrackingJob).toHaveBeenCalledTimes(1))
  button('Остановить трекинг маски Filmcraft').click(); admit(job())
  await vi.waitFor(() => expect(cancelFilmcraftTrackingJob).toHaveBeenCalledTimes(1))
  await vi.waitFor(() => expect(button('Трекинг маски Filmcraft вперёд').disabled).toBe(false))
  expect(onedit).not.toHaveBeenCalled(); expect(startFilmcraftMaskTrackingJob).toHaveBeenCalledTimes(1)
})
it('cancels admission that completes after unmount instead of orphaning an accepted job', async () => {
  const { onedit, onbusy } = await openNative(); let admit!: (row: FilmcraftTrackingJob) => void
  vi.mocked(startFilmcraftMaskTrackingJob).mockImplementation(() => new Promise(resolve => admit = resolve))
  button('Трекинг маски Filmcraft вперёд').click(); await vi.waitFor(() => expect(startFilmcraftMaskTrackingJob).toHaveBeenCalledTimes(1))
  await unmount(component!); component = undefined; admit(job())
  await vi.waitFor(() => expect(cancelFilmcraftTrackingJob).toHaveBeenCalledExactlyOnceWith('owner', expect.objectContaining({ id: '9007199254740993', instance: '11111111-1111-4111-8111-111111111111' })))
  expect(onedit).not.toHaveBeenCalled(); expect(readFilmcraftTrackingJob).not.toHaveBeenCalled(); expect(onbusy.mock.calls.map(c => c[0])).toEqual([true, false])
})
it.each(['document', 'token', 'sources', 'address', 'timeFrames', 'timeTicks', 'disabled'] as const)('cancels native tracking and rejects adoption when %s changes', async field => {
  const onedit = vi.fn(), onbusy = vi.fn()
  const props = { document: 'opaque original', address, sources: { '1': 'movie' }, timeFrames: '0', timeTicks: '9007199254740993', token: 'owner', disabled: false, onedit, onbusy }
  const context = writable(props); component = mount(ContextHarness, { target: document.body, props: { context } }); await tick()
  vi.mocked(startFilmcraftMaskTrackingJob).mockResolvedValue(job()); vi.mocked(cancelFilmcraftTrackingJob).mockResolvedValue(null)
  vi.mocked(readFilmcraftTrackingJob).mockImplementation((_actor, _job, _snapshot, signal) => new Promise((_resolve, reject) => signal!.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError')), { once: true })))
  button('Трекинг маски Filmcraft вперёд').click(); await vi.waitFor(() => expect(readFilmcraftTrackingJob).toHaveBeenCalledTimes(1))
  const change = field === 'address' ? { address: { ...address, clipId: 'other-clip' } } : field === 'sources' ? { sources: { '1': 'other-source' } } : field === 'disabled' ? { disabled: true } : { [field]: 'changed' }
  context.set({ ...props, ...change }); await tick()
  await vi.waitFor(() => expect(cancelFilmcraftTrackingJob).toHaveBeenCalledExactlyOnceWith('owner', expect.objectContaining({ id: '9007199254740993', instance: '11111111-1111-4111-8111-111111111111' })))
  expect(onedit).not.toHaveBeenCalled(); expect(importFilmcraftTrackingSnapshot).not.toHaveBeenCalled(); expect(startFilmcraftMaskTrackingJob).toHaveBeenCalledTimes(1)
})
it('adopts the source seed and successful keys even when the terminal job reports a tracking error', async () => {
  const { onedit } = await openNative()
  const failed = job(1, true, { result: { error: 'lost track after 1 frame(s)' }, documentText: 'retained keys' })
  vi.mocked(startFilmcraftMaskTrackingJob).mockResolvedValue(failed); vi.mocked(readFilmcraftTrackingJob).mockResolvedValue(failed)
  const accepted = result('retained keys', '1'); vi.mocked(importFilmcraftTrackingSnapshot).mockResolvedValue(accepted)
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(onedit).toHaveBeenCalledExactlyOnceWith(accepted, nativeFence))
  expect(document.querySelector('[role=alert]')?.textContent).toContain('lost track'); expect(document.querySelector('[role=status]')?.textContent).toContain('потерян после 1')
})
it('keeps the current project when source revision verification refuses the final snapshot', async () => {
  const { onedit } = await openNative(); vi.mocked(startFilmcraftMaskTrackingJob).mockResolvedValue(job(1, true)); vi.mocked(readFilmcraftTrackingJob).mockRejectedValue(new Error('source changed'))
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('source changed'))
  expect(onedit).not.toHaveBeenCalled(); expect(importFilmcraftTrackingSnapshot).not.toHaveBeenCalled(); expect(trackFilmcraftMask).not.toHaveBeenCalled()
})

it('does not report a document commit when the parent refuses its context fence', async () => {
  const { onedit } = await openNative(); onedit.mockReturnValue(false)
  const finished = job(3, true, { documentText: 'tracked' }); vi.mocked(startFilmcraftMaskTrackingJob).mockResolvedValue(finished); vi.mocked(readFilmcraftTrackingJob).mockResolvedValue(finished); vi.mocked(importFilmcraftTrackingSnapshot).mockResolvedValue(result('tracked', '1'))
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(document.querySelector('[role=status]')?.textContent).toContain('не применён'))
  expect(onedit).toHaveBeenCalledTimes(1); expect(cancelFilmcraftTrackingJob).not.toHaveBeenCalled()
})
it('cancels a known task when observation fails and preserves the current document', async () => {
  const { onedit, onbusy } = await openNative(); vi.mocked(readFilmcraftTrackingJob).mockRejectedValue(new Error('observer unavailable'))
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(onbusy).toHaveBeenLastCalledWith(false))
  expect(cancelFilmcraftTrackingJob).toHaveBeenCalledExactlyOnceWith('owner', expect.objectContaining({ id: '9007199254740993', instance: '11111111-1111-4111-8111-111111111111' }))
  expect(onedit).not.toHaveBeenCalled(); expect(document.querySelector('[role=alert]')?.textContent).toContain('observer unavailable')
})
it('clears previous actor diagnostics when the idle context changes', async () => {
  const onedit = vi.fn(), onbusy = vi.fn()
  const props = { document: 'opaque original', address, sources: { '1': 'movie' }, timeFrames: '0', token: 'owner', onedit, onbusy }
  const context = writable(props); component = mount(ContextHarness, { target: document.body, props: { context } }); await tick()
  vi.mocked(startFilmcraftMaskTrackingJob).mockRejectedValue(new Error('private source diagnostic'))
  button('Трекинг маски Filmcraft вперёд').click()
  await vi.waitFor(() => expect(document.querySelector('[role=alert]')?.textContent).toContain('private source diagnostic'))
  context.set({ ...props, token: 'other-owner' }); await tick()
  expect(document.querySelector('[role=alert]')).toBeNull(); expect(document.querySelector('[role=status]')).toBeNull(); expect(onedit).not.toHaveBeenCalled()
})
