import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftVideoOptions } from './api'
afterEach(() => vi.unstubAllGlobals())
const document = '{"id":18446744073709551615}'
const input = { document, sequence: '18446744073709551615', clips: ['9007199254740993'], enabled: true, holdOn: 'sourceTimecode', time: '9007199254740997', context: { activeSequence: '18446744073709551615', selection: ['9007199254740993'], playhead: '9007199254740999' } }
const manifest = { documentText: document, originalSchema: 12, sequences: [], media: [] }
it.each([['clip.frameHoldOptions', { clips: [{ clip: input.clips[0], hold: '9007199254740997' }] }, 1], ['clip.fieldOptions', { clips: 2 }, 2], ['clip.setTimeInterpolation', { clips: 1, mode: 'opticalFlow' }, 1], ['clip.fitToFrame', { clips: [] }, 0], ['clip.fillFrame', { clips: [{ clip: input.clips[0], scale: 150 }] }, 1]] as const)('imports exact accepted %s document once', async (command, result, count) => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ videoOptionsEdit: true, documentText: document, result }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  expect(await editFilmcraftVideoOptions('owner', command, input)).toEqual({ ...manifest, videoOptionsEdit: true, videoOptionsClipCount: count })
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toMatchObject({ command, params: input }); expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ document })
})
it.each([{ clips: -1 }, { clips: '1' }, { clips: [{ clip: Number(input.clips[0]), hold: '1' }] }, { clips: [{ clip: input.clips[0], hold: '-1' }] }])('refuses invalid hold report before import', async result => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ videoOptionsEdit: true, documentText: document, result }))); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftVideoOptions('owner', 'clip.frameHoldOptions', input)).rejects.toThrow(); expect(fetcher).toHaveBeenCalledTimes(1)
})
it('does not import an aborted response', async () => {
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify({ videoOptionsEdit: true, documentText: document, result: { clips: [] } })) }); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftVideoOptions('owner', 'clip.frameHoldOptions', input, controller.signal)).rejects.toThrow('Aborted'); expect(fetcher).toHaveBeenCalledTimes(1)
})
