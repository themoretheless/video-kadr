import { afterEach, expect, it, vi } from 'vitest'
import { renderCompositionAudioPreview, type NativeNestedProject } from '../api'
const project = { schemaVersion: 1, root: 'root', compositions: {} } as NativeNestedProject
afterEach(() => vi.unstubAllGlobals())
function response(patch: Record<string, string> = {}, samples = [0.25, -0.5]) {
  const bytes = new ArrayBuffer(samples.length * 4)
  samples.forEach((value, i) => new DataView(bytes).setFloat32(i * 4, value, true))
  return new Response(bytes, { headers: { 'content-type': 'application/octet-stream',
    'x-audio-rate': '48000', 'x-audio-channels': '2', 'x-audio-frames': '1', 'x-audio-start-sample': '47', ...patch } })
}
it('preserves exact sample coordinates, authorization and PCM signs', async () => {
  const fetch = vi.fn().mockResolvedValue(response()); vi.stubGlobal('fetch', fetch)
  const abort = new AbortController()
  expect(Array.from(await renderCompositionAudioPreview(project, 47, 1, 'session', abort.signal, 'root'))).toEqual([0.25, -0.5])
  const [url, options] = fetch.mock.calls[0]
  expect(url).toBe('/api/compositions/audio-preview')
  expect(options.signal).toBe(abort.signal)
  expect(options.headers.Authorization).toBe('Bearer session')
  expect(JSON.parse(options.body)).toMatchObject({ startSample: 47, frames: 1, compositionId: 'root' })
})
it('rejects wrong clocks, incomplete PCM and non-finite samples', async () => {
  for (const value of [response({ 'x-audio-start-sample': '48' }), response({}, [1]), response({}, [NaN, 0])]) {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(value))
    await expect(renderCompositionAudioPreview(project, 47, 1)).rejects.toThrow()
  }
})
it('rejects unbounded requests and preserves cancellation', async () => {
  const fetch = vi.fn(); vi.stubGlobal('fetch', fetch)
  await expect(renderCompositionAudioPreview(project, 0, 96001)).rejects.toThrow(RangeError)
  expect(fetch).not.toHaveBeenCalled()
  const abort = new AbortController(); abort.abort()
  const error = new DOMException('cancelled', 'AbortError'); fetch.mockRejectedValue(error)
  await expect(renderCompositionAudioPreview(project, 47, 1, undefined, abort.signal)).rejects.toBe(error)
})
