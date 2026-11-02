import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftLabels, editFilmcraftBins, renameFilmcraftBin, renameFilmcraftClip } from './api'
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
it('renames an exact project item ID without a clip or sequence override', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ documentText: document }))).mockResolvedValueOnce(new Response(JSON.stringify({ documentText: document, sequences: [], media: [], originalSchema: 12 })))
  vi.stubGlobal('fetch', fetcher)
  await renameFilmcraftClip('owner', { document, item: '18446744073709551615', name: 'новое 🎬' })
  expect(JSON.parse((fetcher.mock.calls[0][1] as RequestInit).body as string)).toEqual({ command: 'clip.rename', params: { document, item: '18446744073709551615', name: 'новое 🎬' } })
})
it('renames an exact bin ID and imports the authoritative trimmed snapshot', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ result: { bin: '18446744073709551615', name: 'Папка' }, documentText: document }))).mockResolvedValueOnce(new Response(JSON.stringify({ documentText: document, sequences: [], media: [], originalSchema: 12 })))
  vi.stubGlobal('fetch', fetcher)
  await renameFilmcraftBin('owner', { document, bin: '18446744073709551615', name: '  Папка  ' })
  expect(JSON.parse((fetcher.mock.calls[0][1] as RequestInit).body as string)).toEqual({ command: 'project.renameBin', params: { document, bin: '18446744073709551615', name: '  Папка  ' } })
  expect(JSON.parse((fetcher.mock.calls[1][1] as RequestInit).body as string)).toEqual({ document })
})
it.each(['file.newBin', 'file.newBinFromSelection', 'project.moveToBin'] as const)('imports exact source text for %s without replay', async command => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ documentText: document, result: { moved: 2 } }))).mockResolvedValueOnce(new Response(JSON.stringify({ documentText: document, sequences: [], media: [], originalSchema: 12 })))
  vi.stubGlobal('fetch', fetcher)
  const input = { document, items: ['18446744073709551615', '9007199254740993'], bin: '9007199254740995', name: '' }
  await editFilmcraftBins('owner', command, input)
  expect(JSON.parse((fetcher.mock.calls[0][1] as RequestInit).body as string)).toEqual({ command, params: input })
  expect(JSON.parse((fetcher.mock.calls[1][1] as RequestInit).body as string)).toEqual({ document })
  expect(fetcher).toHaveBeenCalledTimes(2)
})
it.each(['edit.label', 'edit.label.magenta'] as const)('preserves selection presence and exact snapshots for %s', async command => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ documentText: document }))).mockResolvedValueOnce(new Response(JSON.stringify({ documentText: document, originalSchema: 12, sequences: [], media: [] })))
  vi.stubGlobal('fetch', fetcher)
  const input = { document, items: null, clips: [], label: 'rOsE', context: { projectSelection: ['18446744073709551615'], activeSequence: '9007199254740993' } }
  await editFilmcraftLabels('owner', command, input)
  expect(JSON.parse((fetcher.mock.calls[0][1] as RequestInit).body as string)).toEqual({ command, params: input })
  expect(JSON.parse((fetcher.mock.calls[1][1] as RequestInit).body as string)).toEqual({ document })
})
