import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftMarks } from './api'
afterEach(() => vi.unstubAllGlobals())
const document = '{"id":18446744073709551615}'
const manifest = { documentText: document, originalSchema: 12, sequences: [], media: [] }
it('retains exact ticks and absence/null semantics, imports exact text once, and keeps accepted no-op marker', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ documentText: document, markEdit: true }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  const input = { document, item: '18446744073709551615', in: '-9223372036854775808', out: null }
  expect(await editFilmcraftMarks('owner', 'project.setMarks', input)).toEqual({ ...manifest, markEdit: true })
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toMatchObject({ command: 'project.setMarks', params: input })
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ document })
  expect(fetcher).toHaveBeenCalledTimes(2)
})
it('refuses a reply without an accepted edit marker before import', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ documentText: document }))); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftMarks('owner', 'markers.markIn', { document })).rejects.toThrow('accepted edit marker'); expect(fetcher).toHaveBeenCalledTimes(1)
})
it('never imports a late observation after abort', async () => {
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify({ documentText: document, markEdit: true })) }); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftMarks('owner', 'markers.markSplitVideoOut', { document }, controller.signal)).rejects.toThrow('Aborted'); expect(fetcher).toHaveBeenCalledTimes(1)
})
