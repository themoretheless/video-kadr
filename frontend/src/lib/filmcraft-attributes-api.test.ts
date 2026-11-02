import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftAttributes } from './api'
afterEach(() => vi.unstubAllGlobals())
const document = '{"id":18446744073709551615}'
const input = { document, sequence: '18446744073709551615', clips: ['9007199254740993'], clipboardText: 'opaque', motion: true, opacity: false, timeRemapping: true, volume: true, channelVolume: true, panner: true, effects: ['brightness_contrast'], scaleTimes: false, context: { activeSequence: '18446744073709551615', selection: ['9007199254740993'], linkedSelection: false } }
it('imports exact snapshot text and preserves an accepted zero-change edit', async () => {
  const manifest = { documentText: document, originalSchema: 12, sequences: [], media: [] }
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ documentText: document, attributeEdit: true, result: { clips: 0 } }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  expect(await editFilmcraftAttributes('owner', 'edit.pasteAttributes', input)).toEqual({ ...manifest, attributeEdit: true, changedClipCount: 0 })
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toMatchObject({ command: 'edit.pasteAttributes', params: input })
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ document })
})
it.each([{}, { attributeEdit: true, result: { clips: -1 } }, { attributeEdit: true, result: { clips: '1' } }, { attributeEdit: true, result: { clips: 1.5 } }])('refuses invalid result before importing', async reply => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ documentText: document, ...reply }))); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftAttributes('owner', 'edit.removeAttributes', input)).rejects.toThrow('invalid edit result'); expect(fetcher).toHaveBeenCalledTimes(1)
})
it('never imports a response after abort', async () => {
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify({ documentText: document, attributeEdit: true, result: { clips: 1 } })) }); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftAttributes('owner', 'edit.pasteAttributes', input, controller.signal)).rejects.toThrow('Aborted'); expect(fetcher).toHaveBeenCalledTimes(1)
})
