import { afterEach, expect, it, vi } from 'vitest'
import { editFilmcraftEffectRack } from './api'
afterEach(() => vi.unstubAllGlobals())
const input = { command: 'effects.reset' as const, document: '{"id":18446744073709551615}', sequence: '18446744073709551615', clip: '9007199254740993', index: 2 }
it('reimports the exact returned source text and authenticates the rack command', async () => {
  const text = '{"id":18446744073709551615,"reset":true}'
  const manifest = { documentText: text, originalSchema: 12, sequences: [], media: [] }
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ document: { rounded: 1 }, documentText: text }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest)))
  vi.stubGlobal('fetch', fetcher)
  const controller = new AbortController()
  expect(await editFilmcraftEffectRack('owner', input, controller.signal)).toEqual(manifest)
  expect(fetcher.mock.calls[0]![0]).toBe('/api/compositions/filmcraft/command')
  const first = fetcher.mock.calls[0]![1] as RequestInit
  expect(new Headers(first.headers).get('authorization')).toBe('Bearer owner')
  expect(JSON.parse(first.body as string)).toEqual({ command: 'effects.reset', params: { document: input.document, sequence: input.sequence, clip: input.clip, index: 2 } })
  expect(fetcher.mock.calls[1]![0]).toBe('/api/compositions/filmcraft/import')
  expect(JSON.parse((fetcher.mock.calls[1]![1] as RequestInit).body as string)).toEqual({ document: text })
  expect((fetcher.mock.calls[1]![1] as RequestInit).signal).toBe(controller.signal)
})
it('refuses responses without an exact source snapshot', async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response('{}')); vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftEffectRack('owner', input)).rejects.toThrow('source snapshot')
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it('stops before reimport when the edit request loses its context', async () => {
  const controller = new AbortController()
  const fetcher = vi.fn().mockImplementation(() => { controller.abort(); return Promise.resolve(new Response('{"documentText":"changed"}')) })
  vi.stubGlobal('fetch', fetcher)
  await expect(editFilmcraftEffectRack('owner', input, controller.signal)).rejects.toThrow()
  expect(fetcher).toHaveBeenCalledTimes(1)
})
it('applies a source effect and reimports exact documentText', async () => {
  const { applyFilmcraftEffect } = await import('./api')
  const text = '{"id":18446744073709551615,"effectAdded":true}'
  const manifest = { documentText: text, originalSchema: 12, sequences: [], media: [] }
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ documentText: text }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest)))
  vi.stubGlobal('fetch', fetcher)
  const input = { document: '{"source":true}', sequence: '18446744073709551615', clip: '9007199254740993', effect: 'gaussian_blur' }
  expect(await applyFilmcraftEffect('owner', input)).toEqual(manifest)
  expect(JSON.parse((fetcher.mock.calls[0]![1] as RequestInit).body as string)).toEqual({ command: 'effects.apply', params: input })
  expect(JSON.parse((fetcher.mock.calls[1]![1] as RequestInit).body as string)).toEqual({ document: text })
})
it('reads the source effect catalog by audio/video kind with owner authentication', async () => {
  const { listFilmcraftEffects } = await import('./api')
  const fetcher = vi.fn().mockResolvedValue(new Response('[]')); vi.stubGlobal('fetch', fetcher)
  await listFilmcraftEffects('owner', 'Audio')
  const init = fetcher.mock.calls[0]![1] as RequestInit
  expect(new Headers(init.headers).get('authorization')).toBe('Bearer owner')
  expect(JSON.parse(init.body as string)).toEqual({ command: 'effects.list', params: { kind: 'Audio' } })
})

it.each([
  [{ Float: 2.5 }, 2.5], [{ Vec2: { x: 10, y: -20 } }, [10, -20]],
  [{ Color: [0.2, 0.3, 0.4, 1] }, [0.2, 0.3, 0.4, 1]],
  [{ Bool: false }, false], [{ Choice: 2 }, 2], [{ Text: 'hello' }, 'hello'],
  [{ Curve: [[0, 0], [1, 1]] }, [[0, 0], [1, 1]]],
  [{ Path: { vertices: [], closed: true } }, { vertices: [], closed: true }],
] as const)('forwards typed source values without converting exact cursor ticks: %j', async (value, raw) => {
  const { setFilmcraftEffectParameter } = await import('./api')
  const text = '{"id":18446744073709551615}'
  const manifest = { documentText: text, originalSchema: 12, sequences: [], media: [] }
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ documentText: text }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest)))
  vi.stubGlobal('fetch', fetcher)
  const controller = new AbortController()
  await setFilmcraftEffectParameter('owner', { document: text, sequence: '18446744073709551615', clip: '9007199254740993', effect: 2, mask: 1, param: 'value', value: structuredClone(value) as import('./api').FilmcraftParameterValue, timeTicks: '9007199254740995' }, controller.signal)
  const init = fetcher.mock.calls[0]![1] as RequestInit
  expect(new Headers(init.headers).get('authorization')).toBe('Bearer owner')
  expect(JSON.parse(init.body as string)).toEqual({ command: 'effects.setParam', params: { document: text, sequence: '18446744073709551615', clip: '9007199254740993', effect: 2, mask: 1, param: 'value', value: raw, time: '9007199254740995' } })
  expect(JSON.parse((fetcher.mock.calls[1]![1] as RequestInit).body as string)).toEqual({ document: text })
})
it('refuses auto point coordinates rather than silently replacing them for source setParam', async () => {
  const { setFilmcraftEffectParameter } = await import('./api')
  const fetcher = vi.fn(); vi.stubGlobal('fetch', fetcher)
  await expect(setFilmcraftEffectParameter('owner', { document: '{}', sequence: '21', clip: '29', effect: 0, param: 'position', value: { Vec2: { x: null, y: 1 } }, timeFrames: '0' })).rejects.toThrow('auto')
  expect(fetcher).not.toHaveBeenCalled()
})
it.each(['effects.toggleAnimation', 'effects.addKeyframe'] as const)('reimports %s with the exact source timeline cursor', async command => {
  const { editFilmcraftAnimation } = await import('./api')
  const text = '{"id":18446744073709551615,"animation":true}'
  const manifest = { documentText: text, originalSchema: 12, sequences: [], media: [] }
  const fetcher = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ documentText: text }))).mockResolvedValueOnce(new Response(JSON.stringify(manifest)))
  vi.stubGlobal('fetch', fetcher)
  expect(await editFilmcraftAnimation('owner', { command, document: '{}', sequence: '18446744073709551615', clip: '9007199254740993', effect: 2, mask: 0, param: 'path', playhead: '9007199254740995' })).toEqual(manifest)
  const init = fetcher.mock.calls[0]![1] as RequestInit
  expect(new Headers(init.headers).get('authorization')).toBe('Bearer owner')
  expect(JSON.parse(init.body as string)).toEqual({ command, params: { document: '{}', sequence: '18446744073709551615', clip: '9007199254740993', effect: 2, mask: 0, param: 'path', context: { playhead: '9007199254740995' } } })
  expect(JSON.parse((fetcher.mock.calls[1]![1] as RequestInit).body as string)).toEqual({ document: text })
})
