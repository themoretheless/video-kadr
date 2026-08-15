import { afterEach, describe, expect, it, vi } from 'vitest'
import type { AudioDecoderPort, DecodedAudioLike } from './waveform'
import {
  LocalWaveformCache,
  localMediaUrl,
  readResponseBounded,
} from './waveformCache'

afterEach(() => vi.unstubAllGlobals())

function decoder(): { port: AudioDecoderPort; decode: ReturnType<typeof vi.fn>; close: ReturnType<typeof vi.fn> } {
  const decoded: DecodedAudioLike = {
    duration: 1,
    sampleRate: 2,
    numberOfChannels: 1,
    length: 2,
    getChannelData: () => new Float32Array([-1, 1]),
  }
  const decode = vi.fn(async (data: ArrayBuffer): Promise<DecodedAudioLike> => {
    void data
    return decoded
  })
  const close = vi.fn(async () => undefined)
  return { port: { decodeAudioData: decode, close }, decode, close }
}

describe('local waveform cache', () => {
  it('coalesces concurrent loads and retains only bounded summaries', async () => {
    const audio = decoder()
    const fetcher = vi.fn(async () => new Response(new Uint8Array([1, 2, 3])))
    const cache = new LocalWaveformCache(fetcher, () => audio.port, 2, 16)

    const first = cache.load('/files/sources/source.wav')
    const second = cache.load('/files/sources/source.wav')

    expect(first).toBe(second)
    await expect(first).resolves.toMatchObject({ durationSeconds: 1, sampleRate: 2 })
    expect(fetcher).toHaveBeenCalledTimes(1)
    expect(audio.decode).toHaveBeenCalledTimes(1)
    expect(audio.close).toHaveBeenCalledTimes(1)
  })

  it('evicts the oldest entry and retries failed entries', async () => {
    const audio = decoder()
    const fetcher = vi
      .fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>()
      .mockRejectedValueOnce(new Error('offline'))
      .mockResolvedValue(new Response(new Uint8Array([1])))
    const cache = new LocalWaveformCache(fetcher, () => audio.port, 1, 16)

    await expect(cache.load('/files/sources/a.wav')).rejects.toThrow('offline')
    await expect(cache.load('/files/sources/a.wav')).resolves.toBeDefined()
    await cache.load('/files/sources/b.wav')
    await cache.load('/files/sources/a.wav')
    expect(fetcher).toHaveBeenCalledTimes(4)
  })

  it('rejects oversized streams before decode', async () => {
    const response = new Response(new Uint8Array([1, 2, 3, 4]))
    await expect(readResponseBounded(response, 3)).rejects.toThrow('слишком большой')

    const declared = new Response(new Uint8Array([1]), { headers: { 'content-length': '999' } })
    await expect(readResponseBounded(declared, 3)).rejects.toThrow('слишком большой')
  })

  it('accepts only same-origin source/output file routes', () => {
    vi.stubGlobal('location', { origin: 'https://editor.test' })
    expect(localMediaUrl('https://editor.test/files/outputs/render.mp3?download=1')).toBe('/files/outputs/render.mp3?download=1')
    expect(() => localMediaUrl('https://evil.test/files/sources/a.wav')).toThrow('только для локального')
    expect(() => localMediaUrl('/api/private')).toThrow('только для локального')
    expect(() => localMediaUrl('/files/sources/nested/a.wav')).toThrow('только для локального')
  })
})
