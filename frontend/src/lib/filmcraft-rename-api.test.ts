import { afterEach, expect, it, vi } from 'vitest'
import { renameFilmcraftClip } from './api'
afterEach(() => vi.unstubAllGlobals())
const document = '{"id":18446744073709551615}'
const input = { document, sequence: '18446744073709551615', clip: '9007199254740993', name: '' }
it('imports exact returned text instead of the rounded parsed snapshot', async () => {
  const text = '{"id":18446744073709551615,"name":""}'
  const manifest = { documentText: text, sequences: [], media: [], originalSchema: 12 }
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ document: { id: 18446744073709552000 }, documentText: text }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest)))
  vi.stubGlobal('fetch', fetcher)
  const controller = new AbortController()
  expect(await renameFilmcraftClip('owner', input, controller.signal)).toEqual(manifest)
  const init = fetcher.mock.calls[0][1] as RequestInit
  expect(new Headers(init.headers).get('authorization')).toBe('Bearer owner')
  expect(JSON.parse(init.body as string)).toEqual({ command: 'clip.rename', params: input })
  expect(JSON.parse((fetcher.mock.calls[1][1] as RequestInit).body as string)).toEqual({ document: text })
  expect((fetcher.mock.calls[1][1] as RequestInit).signal).toBe(controller.signal)
})
it('refuses missing exact text without importing or replaying', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ document: {} })))
  vi.stubGlobal('fetch', fetcher)
  await expect(renameFilmcraftClip('owner', input)).rejects.toThrow('no source snapshot')
  expect(fetcher).toHaveBeenCalledTimes(1)
})
