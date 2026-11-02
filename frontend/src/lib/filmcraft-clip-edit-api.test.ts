import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftClips } from './api'
afterEach(() => vi.unstubAllGlobals())
const document = '{"id":18446744073709551615}'
const id = '18446744073709551614'
const input = { document, sequence: '18446744073709551615', context: { activeSequence: '18446744073709551615', selection: ['9007199254740993'], linkedSelection: true, playhead: '9007199254740997' } }
const manifest = { documentText: document, originalSchema: 12, sequences: [{ id: input.sequence, name: 'Scene', settings: {}, sampleRate: 48000, durationSeconds: 1, clips: [{ id }] }], media: [] }
it.each([['clip.frameHold', { clips: [id] }], ['clip.insertFrameHoldSegment', { clip: id, duration: '9007199254740997' }]] as const)('imports exact %s text and validates returned selection against the sequence', async (command, result) => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ clipEdit: true, documentText: document, result, context: { selection: [id] } }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  expect(await editFilmcraftClips('owner', command, input)).toEqual({ ...manifest, clipEdit: true, clipEditSelection: [id] })
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toMatchObject({ command, params: input }); expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ document })
})
it('keeps a non-structural no-op marker without adopting a new selection', async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ clipEdit: true, documentText: document, result: null, context: { selection: input.context.selection } }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  expect(await editFilmcraftClips('owner', 'clip.speedDuration', { ...input, speed: 100 })).toEqual({ ...manifest, clipEdit: true })
})
it.each([{ result: { clips: [id] }, context: { selection: [] } }, { result: { clips: [1] }, context: { selection: [id] } }])('refuses inconsistent hold metadata before import', async reply => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ clipEdit: true, documentText: document, ...reply }))); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftClips('owner', 'clip.frameHold', input)).rejects.toThrow(); expect(fetcher).toHaveBeenCalledTimes(1)
})
it('does not import a late response after abort', async () => {
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify({ clipEdit: true })) }); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftClips('owner', 'clip.enable', input, controller.signal)).rejects.toThrow('Aborted'); expect(fetcher).toHaveBeenCalledTimes(1)
})
