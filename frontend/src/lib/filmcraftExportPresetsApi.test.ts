import { afterEach, expect, it, vi } from 'vitest'
import { ApiError, editFilmcraftExportPresets, listFilmcraftExportPresets } from './api'

afterEach(() => vi.unstubAllGlobals())

it('authenticates filtered lists and preserves preset files as JSON bodies', async () => {
  const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify({ presets: [], default: 'default' }), { status: 200 }))
  vi.stubGlobal('fetch', fetch)
  const controller = new AbortController()
  await listFilmcraftExportPresets('owner', { query: 'Review & 720p', category: 'User', favorites: true }, controller.signal)
  const [url, options] = fetch.mock.calls[0]
  const query = new URL(String(url), 'http://localhost').searchParams
  expect(query.get('query')).toBe('Review & 720p')
  expect(query.get('favorites')).toBe('true')
  expect(options.headers.Authorization).toBe('Bearer owner')
  expect(options.signal).toBe(controller.signal)
  const file = { format: 'filmcraft.export-presets' as const, version: 1, presets: [{ name: 'Review', category: 'User', description: '', settings: { frameSize: [1280, 720] } }], favorites: [] }
  fetch.mockResolvedValueOnce(new Response(JSON.stringify({ imported: ['Review'] }), { status: 200 }))
  await editFilmcraftExportPresets('owner', { action: 'import', file }, controller.signal)
  expect(fetch.mock.calls[1][1].method).toBe('POST')
  expect(JSON.parse(fetch.mock.calls[1][1].body)).toEqual({ action: 'import', file })
  expect(fetch.mock.calls[1][1].headers.Authorization).toBe('Bearer owner')
})

it('surfaces owner authorization failures', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: 'Unauthorized', code: 'unauthorized' }), { status: 401 })))
  await expect(editFilmcraftExportPresets('expired', { action: 'get', name: 'Review' })).rejects.toBeInstanceOf(ApiError)
})
