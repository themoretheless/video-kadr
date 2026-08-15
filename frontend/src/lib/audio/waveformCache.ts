import {
  DEFAULT_WAVEFORM_BUCKETS,
  decodeWaveform,
  type AudioDecoderFactory,
  type WaveformSummary,
} from './waveform'

export const DEFAULT_WAVEFORM_CACHE_ENTRIES = 24
export const DEFAULT_WAVEFORM_DECODE_BYTES = 128 * 1024 * 1024

export type WaveformFetchPort = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>

/**
 * Small LRU-like cache for same-origin media waveforms. It stores only bounded
 * summaries, not decoded PCM, and coalesces concurrent requests for one URL.
 */
export class LocalWaveformCache {
  private readonly entries = new Map<string, Promise<WaveformSummary>>()

  constructor(
    private readonly fetcher: WaveformFetchPort = (input, init) => globalThis.fetch(input, init),
    private readonly decoderFactory?: AudioDecoderFactory,
    private readonly maxEntries = DEFAULT_WAVEFORM_CACHE_ENTRIES,
    private readonly maxBytes = DEFAULT_WAVEFORM_DECODE_BYTES,
  ) {
    if (!Number.isSafeInteger(maxEntries) || maxEntries <= 0) throw new Error('Waveform cache size must be positive')
    if (!Number.isSafeInteger(maxBytes) || maxBytes <= 0) throw new Error('Waveform decode limit must be positive')
  }

  load(url: string): Promise<WaveformSummary> {
    const safeUrl = localMediaUrl(url)
    const cached = this.entries.get(safeUrl)
    if (cached) {
      this.entries.delete(safeUrl)
      this.entries.set(safeUrl, cached)
      return cached
    }

    while (this.entries.size >= this.maxEntries) {
      const oldest = this.entries.keys().next().value as string | undefined
      if (oldest === undefined) break
      this.entries.delete(oldest)
    }
    const pending = this.fetchAndDecode(safeUrl).catch((error: unknown) => {
      if (this.entries.get(safeUrl) === pending) this.entries.delete(safeUrl)
      throw error
    })
    this.entries.set(safeUrl, pending)
    return pending
  }

  clear(): void {
    this.entries.clear()
  }

  private async fetchAndDecode(url: string): Promise<WaveformSummary> {
    const response = await this.fetcher(url, { credentials: 'same-origin' })
    if (!response.ok) throw new Error(`Не удалось загрузить аудио для формы волны (${response.status})`)
    const bytes = await readResponseBounded(response, this.maxBytes)
    return decodeWaveform(bytes, this.decoderFactory, DEFAULT_WAVEFORM_BUCKETS)
  }
}

export async function readResponseBounded(response: Response, maxBytes: number): Promise<ArrayBuffer> {
  const declaredLength = Number(response.headers.get('content-length'))
  if (Number.isFinite(declaredLength) && declaredLength > maxBytes) {
    throw new Error('Медиафайл слишком большой для локальной формы волны')
  }
  if (!response.body) {
    const buffer = await response.arrayBuffer()
    if (buffer.byteLength > maxBytes) throw new Error('Медиафайл слишком большой для локальной формы волны')
    return buffer
  }

  const reader = response.body.getReader()
  const chunks: Uint8Array[] = []
  let length = 0
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      if (!value?.byteLength) continue
      length += value.byteLength
      if (length > maxBytes) {
        await reader.cancel('waveform byte limit exceeded').catch(() => undefined)
        throw new Error('Медиафайл слишком большой для локальной формы волны')
      }
      chunks.push(value)
    }
  } finally {
    reader.releaseLock()
  }
  const combined = new Uint8Array(length)
  let offset = 0
  for (const chunk of chunks) {
    combined.set(chunk, offset)
    offset += chunk.byteLength
  }
  return combined.buffer
}

export function localMediaUrl(value: string): string {
  if (typeof value !== 'string' || value.length > 2_048 || value.includes('\0')) {
    throw new Error('Некорректный URL медиафайла')
  }
  const origin = globalThis.location?.origin ?? 'http://local.invalid'
  const parsed = new URL(value, origin)
  if (parsed.origin !== origin || !/^\/files\/(?:sources|outputs)\/[^/]+$/.test(parsed.pathname)) {
    throw new Error('Форма волны доступна только для локального медиафайла')
  }
  return `${parsed.pathname}${parsed.search}`
}

export const localWaveformCache = new LocalWaveformCache()
