import { afterEach, expect, it, vi } from 'vitest'
import { modifyFilmcraftMedia } from './api'
afterEach(() => vi.unstubAllGlobals())
const document = '{"id":18446744073709551615}'
const input = { document, items: ['9007199254740993'], context: { activeSequence: null, selection: [], projectSelection: ['9007199254740993'] } }
const id = '18446744073709551614'
const manifest = { documentText: document, originalSchema: 12, sequences: [], media: [], projectItems: [{ id, name: 'Mono', kind: 'Media' }] }
it('validates mono provenance and imports exact document text once', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ modifyEdit: true, documentText: document, result: { items: [id] }, context: { projectSelection: [id] }, copies: [{ id, sourceId: input.items[0] }] }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  expect(await modifyFilmcraftMedia('owner', 'clip.breakoutToMono', input)).toEqual({ ...manifest, modifyEdit: true, modificationCount: 1, modifiedProjectSelection: [id], duplicateSources: [{ id, sourceId: input.items[0] }] })
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toMatchObject({ command: 'clip.breakoutToMono', params: input }); expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ document })
})
it.each([['clip.modifyTimecode', { item: input.items[0], startTimecode: '9223372036854775807' }], ['clip.audioChannels', { clips: [id], channels: [1, 0] }], ['clip.audioChannels', { items: [{ item: input.items[0], format: 'Mono', clips: [[1], [0]] }] }]] as const)('keeps %s project selection separate from its result', async (command, result) => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ modifyEdit: true, documentText: document, result, context: { projectSelection: input.context.projectSelection }, copies: [] }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  expect(await modifyFilmcraftMedia('owner', command, input)).toEqual({ ...manifest, modifyEdit: true, modificationCount: 1 })
})
it.each([{ copies: [{ id, sourceId: '7' }], context: { projectSelection: [id] } }, { copies: [], context: { projectSelection: [id] } }, { copies: [{ id, sourceId: input.items[0] }], context: { projectSelection: [] } }])('refuses inconsistent mono provenance before import', async overrides => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ modifyEdit: true, documentText: document, result: { items: [id] }, ...overrides }))); vi.stubGlobal('fetch', fetcher)
  await expect(modifyFilmcraftMedia('owner', 'clip.breakoutToMono', input)).rejects.toThrow(); expect(fetcher).toHaveBeenCalledTimes(1)
})
it('does not import an aborted response', async () => {
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify({ modifyEdit: true })) }); vi.stubGlobal('fetch', fetcher)
  await expect(modifyFilmcraftMedia('owner', 'clip.breakoutToMono', input, controller.signal)).rejects.toThrow('Aborted'); expect(fetcher).toHaveBeenCalledTimes(1)
})
