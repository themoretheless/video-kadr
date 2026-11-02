import { afterEach, expect, it, vi } from 'vitest'
import { duplicateFilmcraftProjectItems } from './api'
afterEach(() => vi.unstubAllGlobals())
const id = '18446744073709551614'
const document = '{"id":18446744073709551615}'
const input = { document, context: { projectSelection: ['9007199254740993'] } }
const manifest = { documentText: document, originalSchema: 12, sequences: [], media: [], projectItems: [{ id: input.context.projectSelection[0], name: 'Original', kind: 'Media' }, { id, name: 'Copy', kind: 'Media' }] }
it('imports exact text and returns authoritative copied selection without replay', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ result: { items: [id] }, context: { projectSelection: [id] }, copies: [{ id, sourceId: input.context.projectSelection[0] }], documentText: document, document: { id: 0 } }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  const controller = new AbortController()
  expect(await duplicateFilmcraftProjectItems('owner', input, controller.signal)).toEqual({ ...manifest, duplicatedItemIds: [id], duplicateSources: [{ id, sourceId: input.context.projectSelection[0] }] })
  const init = fetcher.mock.calls[0][1] as RequestInit
  expect(new Headers(init.headers).get('authorization')).toBe('Bearer owner'); expect(init.signal).toBe(controller.signal)
  expect(JSON.parse(init.body as string)).toEqual({ command: 'edit.duplicate', params: input })
  expect(JSON.parse((fetcher.mock.calls[1][1] as RequestInit).body as string)).toEqual({ document })
  expect(fetcher).toHaveBeenCalledTimes(2)
})
it.each([
  { result: { items: [Number(id)] }, context: { projectSelection: [id] }, copies: [{ id, sourceId: input.context.projectSelection[0] }], documentText: document },
  { result: { items: ['18446744073709551616'] }, context: { projectSelection: ['18446744073709551616'] }, documentText: document },
  { result: { items: [id, id] }, context: { projectSelection: [id, id] }, documentText: document },
  { result: { items: [id] }, context: { projectSelection: [] }, copies: [], documentText: document },
  { result: { items: [id] }, context: { projectSelection: [id] }, document: {} }
])('rejects invalid duplicate replies before import', async raw => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(raw))); vi.stubGlobal('fetch', fetcher)
  await expect(duplicateFilmcraftProjectItems('owner', input)).rejects.toThrow()
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it('refuses copied IDs missing from the imported manifest', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ result: { items: [id] }, context: { projectSelection: [id] }, copies: [{ id, sourceId: input.context.projectSelection[0] }], documentText: document }))).mockResolvedValueOnce(new Response(JSON.stringify({ ...manifest, projectItems: [] }))); vi.stubGlobal('fetch', fetcher)
  await expect(duplicateFilmcraftProjectItems('owner', input)).rejects.toThrow('omitted copied items')
  expect(fetcher).toHaveBeenCalledTimes(2)
})
it('retains an accepted no-op edit marker for source Undo semantics', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ result: { items: [] }, context: { projectSelection: [] }, copies: [], documentText: document }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  expect((await duplicateFilmcraftProjectItems('owner', input)).duplicatedItemIds).toEqual([])
})
it('does not import a reply after observation abort', async () => {
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify({ result: { items: [] }, context: { projectSelection: [] }, copies: [], documentText: document })) }); vi.stubGlobal('fetch', fetcher)
  await expect(duplicateFilmcraftProjectItems('owner', input, controller.signal)).rejects.toThrow('Aborted')
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it.each([
  { copies: [] },
  { copies: [{ id, sourceId: id }] },
  { copies: [{ id: '9007199254740992', sourceId: input.context.projectSelection[0] }] },
  { copies: [{ id, sourceId: '9007199254740994' }] }
])('refuses missing or inconsistent copy provenance before import', async ({ copies }) => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ result: { items: [id] }, context: { projectSelection: [id] }, copies, documentText: document }))); vi.stubGlobal('fetch', fetcher)
  await expect(duplicateFilmcraftProjectItems('owner', input)).rejects.toThrow('Duplicate')
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it('preserves evolving copy-of-copy provenance in allocation order', async () => {
  const first = '18446744073709551613', second = id, source = input.context.projectSelection[0]
  const copies = [{ id: first, sourceId: source }, { id: second, sourceId: first }]
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ result: { items: [first, second] }, context: { projectSelection: [first, second] }, copies, documentText: document }))).mockResolvedValueOnce(new Response(JSON.stringify({ ...manifest, projectItems: [{ id: source, name: 'Original', kind: 'Media' }, { id: first, name: 'Original Copy', kind: 'Media' }, { id: second, name: 'Original Copy Copy', kind: 'Media' }] }))); vi.stubGlobal('fetch', fetcher)
  const result = await duplicateFilmcraftProjectItems('owner', { document, context: { projectSelection: [source, first] } })
  expect(result.duplicateSources).toEqual(copies); expect(result.duplicatedItemIds).toEqual([first, second])
})
