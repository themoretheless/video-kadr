import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftSource } from './api'
afterEach(() => vi.unstubAllGlobals())
const source = '9007199254740993'; const sequence = '18446744073709551615'; const clip = '18446744073709551614'
const document = '{"id":18446744073709551615}'
const input = { document, context: { activeSequence: sequence, sourceItem: source, sourcePlayhead: '17', playheadFrames: '5', targeting: { videoDest: '9007199254740995', audioDest: null }, sequencesAsClips: true } }
const context = { activeSequence: sequence, sourceItem: source, sourcePlayhead: '17', playhead: '9007199254740997', playheadFrames: '51', selection: [clip] }
const manifest = { documentText: document, originalSchema: 12, projectItems: [{ id: source, name: 'Source', kind: 'Media' }], sequences: [{ id: sequence, name: 'Scene', sampleRate: 48000, durationSeconds: 2, settings: {}, clips: [{ id: clip }] }], media: [] }
const reply = { sourceEdit: true, documentText: document, result: { clips: [clip] }, context }
it.each(['source.insert', 'source.overwrite'] as const)('imports exact %s text once with authoritative selection and clock', async command => {
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(reply))).mockResolvedValueOnce(new Response(JSON.stringify(manifest))); vi.stubGlobal('fetch', fetcher)
  expect(await editFilmcraftSource('owner', command, input)).toEqual({ ...manifest, sourceEdit: true, insertedClipIds: [clip], sourceEditContext: context })
  expect(fetcher).toHaveBeenCalledTimes(2); expect(JSON.parse(fetcher.mock.calls[0][1].body)).toMatchObject({ command, params: input }); expect(JSON.parse(fetcher.mock.calls[1][1].body)).toEqual({ document })
})
it.each([{ sourceEdit: false }, { result: { clips: [] } }, { result: { clips: [clip, clip] } }, { result: { clips: [Number(clip)] } }, { context: { ...context, activeSequence: source } }, { context: { ...context, sourceItem: sequence } }, { context: { ...context, sourcePlayhead: '18' } }, { context: { ...context, playhead: '-0' } }, { context: { ...context, playheadFrames: Number(context.playheadFrames) } }])('refuses malformed source edit metadata before import (%j)', async patch => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ ...reply, ...patch }))); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftSource('owner', 'source.insert', input)).rejects.toThrow(); expect(fetcher).toHaveBeenCalledTimes(1)
})
it.each([{ ...manifest, projectItems: [] }, { ...manifest, sequences: [] }])('rejects imported snapshots that omit the active source or sequence', async project => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(reply))).mockResolvedValueOnce(new Response(JSON.stringify(project))))
  await expect(editFilmcraftSource('owner', 'source.insert', input)).rejects.toThrow('identities')
})
it('does not import an aborted authoring reply or replay it', async () => {
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify(reply)) }); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftSource('owner', 'source.insert', input, controller.signal)).rejects.toThrow('Aborted'); expect(fetcher).toHaveBeenCalledTimes(1)
})
it('preserves declared placed IDs when a later same-track overwrite removed one of them', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(reply))).mockResolvedValueOnce(new Response(JSON.stringify({ ...manifest, sequences: [{ ...manifest.sequences[0], clips: [] }] })))); expect((await editFilmcraftSource('owner', 'source.overwrite', input)).insertedClipIds).toEqual([clip])
})
