import { afterEach, expect, it, vi } from 'vitest'
import { deleteFilmcraftProjectItems } from './api'
afterEach(() => vi.unstubAllGlobals())
const id = '18446744073709551615'
const document = '{"id":18446744073709551615}'
const input = { document, items: [id], context: { activeSequence: id, selection: ['9007199254740993'], projectSelection: [id] } }
const reply = { result: { items: 1, bins: 0 }, removedItemIds: [id], removedBinIds: [], context: { activeSequence: null, selection: [], projectSelection: [] }, documentText: document }
const manifest = { documentText: document, originalSchema: 12, sequences: [], media: [], projectItems: [], projectBins: [] }
it('imports exact snapshot text once and retains accepted Clear marker and fixed context', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(reply))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  expect(await deleteFilmcraftProjectItems('owner', input)).toEqual({ ...manifest, deletedItemIds: [id], deletedBinIds: [], deletionContext: reply.context })
  expect(fetcher).toHaveBeenCalledTimes(2)
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toMatchObject({ command: 'project.delete', params: input })
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ document })
})
it.each([
  { result: { items: -1, bins: 0 } }, { result: { items: 0, bins: 0 } }, { removedItemIds: [Number(id)] }, { removedItemIds: [id, id] }, { removedBinIds: ['18446744073709551616'] }, { context: { ...reply.context, projectSelection: [id] } }, { context: { ...reply.context, activeSequence: '01' } }
])('refuses malformed metadata before import (%j)', async patch => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ ...reply, ...patch }))); vi.stubGlobal('fetch', fetcher)
  await expect(deleteFilmcraftProjectItems('owner', input)).rejects.toThrow()
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it.each([
  { ...manifest, projectItems: [{ id, name: 'Still exists', kind: 'Media' }] },
  { ...manifest, sequences: [{ id: '42', clips: [] }] }
])('rejects contradictions between fixed context and imported document (%j)', async project => {
  const contradictory = project.sequences.length ? { ...reply, context: { ...reply.context, activeSequence: id } } : reply
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(contradictory))).mockResolvedValueOnce(new Response(JSON.stringify(project))))
  await expect(deleteFilmcraftProjectItems('owner', input)).rejects.toThrow('contradicts')
})
it('does not import a late response after abort', async () => {
  const controller = new AbortController()
  const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify(reply)) }); vi.stubGlobal('fetch', fetcher)
  await expect(deleteFilmcraftProjectItems('owner', input, controller.signal)).rejects.toThrow('Aborted')
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it('preserves no-op Clear marker without equating source counts and actual deletions', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ ...reply, result: { items: 4, bins: 2 }, removedItemIds: [], removedBinIds: [] }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))))
  expect((await deleteFilmcraftProjectItems('owner', input)).deletedItemIds).toEqual([])
})
