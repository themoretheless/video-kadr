import { afterEach, expect, it, vi } from 'vitest'
import { startFilmcraftMaskTrackingJob, readFilmcraftTrackingJob, cancelFilmcraftTrackingJob, importFilmcraftTrackingSnapshot, type FilmcraftTrackingJob } from './api'
afterEach(() => vi.unstubAllGlobals())
const id = '9007199254740993'
const text = '{"id":18446744073709551615,"tracked":true}'
const instance = '11111111-1111-4111-8111-111111111111'
const row = (extra: Partial<FilmcraftTrackingJob> = {}): FilmcraftTrackingJob => ({ id, instance, kind: 'maskTracking', done: 0, total: 12, finished: false, status: '', result: null, ...extra })
const input = { document: text, sequence: '18446744073709551615', clip: '9007199254740995', effect: 2, mask: 0, frames: 12, direction: 'backward' as const, sources: { '1': 'private-source' } }
it.each(['timeTicks', 'timeFrames'] as const)('starts one native background task preserving exact %s', async clock => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ job: id, status: row() }))); vi.stubGlobal('fetch', fetcher)
  const started = await startFilmcraftMaskTrackingJob('owner', { ...input, ...(clock === 'timeTicks' ? { timeTicks: '9007199254740997' } : { timeFrames: '9007199254740997' }) })
  expect(started.id).toBe(id); expect(fetcher).toHaveBeenCalledTimes(1)
  const init = fetcher.mock.calls[0]![1] as RequestInit
  expect(new Headers(init.headers).get('authorization')).toBe('Bearer owner')
  expect(JSON.parse(init.body as string)).toEqual({ command: 'masks.track', params: { ...input, wait: false, [clock === 'timeTicks' ? 'time' : 'frame']: '9007199254740997' } })
  expect(init.signal).toBeUndefined()
})
it('fetches and reimports an exact authorized terminal snapshot without using a rounded object', async () => {
  const job = row({ done: 2, finished: true, result: { error: 'lost track' }, documentText: text })
  const manifest = { documentText: text, originalSchema: 12, sequences: [], media: [] }
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify([{ ...job, document: { id: 1 } }]))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  const controller = new AbortController()
  expect(await importFilmcraftTrackingSnapshot(await readFilmcraftTrackingJob('owner', { id, instance }, true, controller.signal), controller.signal)).toEqual(manifest)
  const init = fetcher.mock.calls[0]![1] as RequestInit
  expect(new Headers(init.headers).get('authorization')).toBe('Bearer owner')
  expect(JSON.parse(init.body as string)).toEqual({ command: 'jobs.list', params: { job: id, instance, snapshot: true } })
  expect(fetcher.mock.calls[1]![0]).toBe('/api/compositions/filmcraft/import')
  expect(JSON.parse((fetcher.mock.calls[1]![1] as RequestInit).body as string)).toEqual({ document: text })
  expect((fetcher.mock.calls[1]![1] as RequestInit).signal).toBe(controller.signal)
})
it('cancels by exact job ID without discarding retained work', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response('null')); vi.stubGlobal('fetch', fetcher)
  await cancelFilmcraftTrackingJob('owner', id)
  expect(JSON.parse((fetcher.mock.calls[0]![1] as RequestInit).body as string)).toEqual({ command: 'jobs.cancel', params: { job: id } })
})
it.each([
  { job: 9007199254740992, status: row() },
  { job: '18446744073709551616', status: row({ id: '18446744073709551616' }) },
  { job: 1, status: row({ id: 2 }) },
  { job: 1, status: { ...row({ id: 1 }), kind: 'export' } },
  { job: 1, status: row({ id: 1, done: -1 }) },
  { job: 1, status: row({ id: 1, total: 0 }) },
  { job: 1, status: row({ id: 1, finished: true, result: null }) },
])('refuses malformed/unsafe job admission without replay: %j', async response => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(response))); vi.stubGlobal('fetch', fetcher)
  await expect(startFilmcraftMaskTrackingJob('owner', { ...input, timeTicks: '0' })).rejects.toThrow()
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it.each([[[]], [[row({ id: 2 })]], [[row({ finished: true, result: { frames: 0 } })]]])('refuses missing, mismatched or textless snapshots', async response => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(response))); vi.stubGlobal('fetch', fetcher)
  await expect(readFilmcraftTrackingJob('owner', id, true)).rejects.toThrow()
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it('does not import a running task or an aborted observation', async () => {
  const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher)
  await expect(importFilmcraftTrackingSnapshot(row({ documentText: text }))).rejects.toThrow('still running')
  const controller = new AbortController(); controller.abort()
  await expect(importFilmcraftTrackingSnapshot(row({ finished: true, result: { frames: 0 }, documentText: text }), controller.signal)).rejects.toThrow()
  expect(fetcher).not.toHaveBeenCalled()
})

it.each([NaN, Infinity, 0, -1, 1.5])('refuses invalid frame counts before admitting a server task: %s', async frames => {
  const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher)
  await expect(startFilmcraftMaskTrackingJob('owner', { ...input, frames, timeTicks: '0' })).rejects.toThrow('frame count')
  expect(fetcher).not.toHaveBeenCalled()
})

it('refuses a different task instance even when a restarted server reuses its numeric ID', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify([row({ instance: '22222222-2222-4222-8222-222222222222', finished: true, result: { frames: 0 }, documentText: text })]))); vi.stubGlobal('fetch', fetcher)
  await expect(readFilmcraftTrackingJob('owner', { id, instance }, true)).rejects.toThrow('instance changed')
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it('cancels a precise task instance so a reused numeric ID cannot cancel new work', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response('null')); vi.stubGlobal('fetch', fetcher)
  await cancelFilmcraftTrackingJob('owner', { id, instance })
  expect(JSON.parse((fetcher.mock.calls[0]![1] as RequestInit).body as string)).toEqual({ command: 'jobs.cancel', params: { job: id, instance } })
})
