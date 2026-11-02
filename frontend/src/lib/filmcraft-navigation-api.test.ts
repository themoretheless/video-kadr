import { afterEach, expect, it, vi } from 'vitest'
import { navigateFilmcraft } from './api'
afterEach(() => vi.unstubAllGlobals())
const id = '18446744073709551615'
const input = { document: '{"id":18446744073709551615}', context: { activeSequence: id, sourceItem: null, sourcePlayhead: '0', playhead: '0', selection: ['9007199254740993'], projectSelection: [id] } }
const context = { ...input.context, sourceItem: id, sourcePlayhead: '9007199254740997', playheadFrames: '0' }
const reply = { result: null, context, events: [{ type: 'openSource', item: id }] }
it('returns exact source context and event from one request with no document import or replay', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(reply))); vi.stubGlobal('fetch', fetcher)
  expect(await navigateFilmcraft('owner', 'source.open', { ...input, item: id })).toEqual({ context, events: reply.events })
  expect(fetcher).toHaveBeenCalledTimes(1)
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toMatchObject({ command: 'source.open', params: { ...input, item: id } })
})
it.each([
  { context: { ...context, sourcePlayhead: Number(context.sourcePlayhead) } }, { context: { ...context, playheadFrames: '01' } }, { context: { ...context, playhead: '-0' } }, { context: { ...context, sourcePlayhead: '9223372036854775808' } }, { context: { ...context, sourceItem: '18446744073709551616' } }, { context: { ...context, activeSequence: null } }, { context: { ...context, projectSelection: [] } }, { context: { ...context, selection: [] } }, { events: [] }, { events: [{ type: 'openSource', item: '42' }] }, { result: { time: '0' } }
])('rejects malformed or inconsistent navigation metadata (%j)', async patch => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ ...reply, ...patch }))); vi.stubGlobal('fetch', fetcher)
  await expect(navigateFilmcraft('owner', 'source.open', input)).rejects.toThrow(); expect(fetcher).toHaveBeenCalledTimes(1)
})
it('checks the source seek result against the authoritative clock', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ result: { time: '17' }, context, events: [] }))))
  await expect(navigateFilmcraft('owner', 'source.setPlayhead', { ...input, context: { ...input.context, sourceItem: id } })).rejects.toThrow('seek result')
})
it('does not return late metadata after observation abort', async () => {
  const controller = new AbortController(); const fetcher = vi.fn().mockImplementation(async () => { controller.abort(); return new Response(JSON.stringify(reply)) }); vi.stubGlobal('fetch', fetcher)
  await expect(navigateFilmcraft('owner', 'source.open', input, controller.signal)).rejects.toThrow('Aborted'); expect(fetcher).toHaveBeenCalledTimes(1)
})

it('matches an exact source frame using one context request and no import', async () => {
  const response = { result: { item: id }, context, events: reply.events }
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify(response))); vi.stubGlobal('fetch', fetcher)
  expect(await navigateFilmcraft('owner', 'sequence.matchFrame', input)).toEqual({ context, events: reply.events }); expect(fetcher).toHaveBeenCalledTimes(1)
})
it.each(['clip.revealInProject', 'sequence.revealInProject'] as const)('returns %s project selection and reveal event without touching the Source Monitor', async command => {
  const target = '9007199254740995'; const fixed = { ...input.context, projectSelection: [target], playheadFrames: '0' }
  const events = [{ type: 'revealInProject', item: target }]
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ result: { item: target }, context: fixed, events }))); vi.stubGlobal('fetch', fetcher)
  expect(await navigateFilmcraft('owner', command, input)).toEqual({ context: fixed, events }); expect(fetcher).toHaveBeenCalledTimes(1)
})
it.each([{ result: { item: '7' } }, { context: { ...context, playhead: '1' } }, { context: { ...context, selection: [] } }, { events: [{ type: 'revealInProject', item: id }] }])('rejects inconsistent Match Frame metadata (%j)', async patch => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ result: { item: id }, context, events: reply.events, ...patch }))); vi.stubGlobal('fetch', fetcher)
  await expect(navigateFilmcraft('owner', 'sequence.matchFrame', input)).rejects.toThrow(); expect(fetcher).toHaveBeenCalledTimes(1)
})
