import { afterEach, expect, it, vi } from 'vitest'
import { replaceFilmcraftClips } from './api'
afterEach(() => vi.unstubAllGlobals())
const document = '{"id":18446744073709551615}'
const input = { document, sequence: '18446744073709551615', clips: ['9007199254740993'], context: { activeSequence: '18446744073709551615', selection: ['9007199254740993'], projectSelection: [], sourceItem: '9007199254740995', sourcePlayhead: '9007199254740997', playhead: '9007199254740999', linkedSelection: true } }
const valid = { replacementEdit: true, documentText: document, result: { clips: input.clips, item: input.context.sourceItem, short: [{ clip: input.clips[0], shortByFrames: '9223372036854775807' }] } }
it('preserves exact identities, clocks and warnings through one native edit and exact import', async () => {
  const manifest = { documentText: document, originalSchema: 12, sequences: [], media: [] }
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(valid))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  expect(await replaceFilmcraftClips('owner', 'clip.replaceFromSourceMatchFrame', input)).toEqual({ ...manifest, replacementEdit: true, replacedClipIds: input.clips, replacementItemId: input.context.sourceItem, sourceInClamped: [], shortClips: valid.result.short })
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toMatchObject({ command: 'clip.replaceFromSourceMatchFrame', params: input })
  expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ document })
})
it.each([{ ...valid, replacementEdit: false }, { ...valid, result: { ...valid.result, clips: [] } }, { ...valid, result: { ...valid.result, item: '7' } }, { ...valid, result: { ...valid.result, short: [{ clip: '7', shortByFrames: '1' }] } }, { ...valid, result: { ...valid.result, short: [{ clip: input.clips[0], shortByFrames: '-1' }] } }, { ...valid, result: { ...valid.result, sourceInClamped: input.clips } }])('refuses inconsistent edit metadata before import', async reply => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(reply))); vi.stubGlobal('fetch', fetcher)
  await expect(replaceFilmcraftClips('owner', 'clip.replaceFromSource', input)).rejects.toThrow(); expect(fetcher).toHaveBeenCalledTimes(1)
})
it('does not import an aborted response', async () => {
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify(valid)) }); vi.stubGlobal('fetch', fetcher)
  await expect(replaceFilmcraftClips('owner', 'clip.replaceFromSource', input, controller.signal)).rejects.toThrow('Aborted'); expect(fetcher).toHaveBeenCalledTimes(1)
})
