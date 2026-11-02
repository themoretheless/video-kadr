import wasmUrl from './color-preview.wasm?url'
import type { NativeColor,NativeColorTrack } from '../api'

interface ColorAbi {
  color_begin(): void
  color_push(byte: number): number
  color_track_prepare(id: number): number
  color_track_drop(id: number): void
  color_track_at(id: number,time: number): number
  color_packed(): number
  color_output_len(): number
  color_output_byte(index: number): number
}
let compiled: Promise<WebAssembly.Module> | undefined
async function module(): Promise<WebAssembly.Module> {
  compiled ??= fetch(wasmUrl).then(async response => {
    if (!response.ok) throw new Error(`Color preview WASM loading failed (${response.status})`)
    return WebAssembly.compile(await response.arrayBuffer())
  }).catch(error => { compiled = undefined;throw error })
  return compiled
}
let shared: Promise<ColorAbi> | undefined
let nextId = 1
const released = new FinalizationRegistry<{ api: ColorAbi;id: number }>(value => value.api.color_track_drop(value.id))
async function sharedApi(): Promise<ColorAbi> {
  shared ??= module().then(compiled => WebAssembly.instantiate(compiled)).then(instance => instance.exports as unknown as ColorAbi).catch(error => { shared = undefined;throw error })
  return shared
}
export type PreparedColorSampler = ((timeUs: number) => NativeColor) & { dispose(): void }
function nativeError(api: ColorAbi): Error {
  const length = api.color_output_len()
  if (length > 2 * 1024 * 1024) return new Error('Invalid native color error response')
  return new Error(new TextDecoder().decode(Uint8Array.from({ length },(_,index) => api.color_output_byte(index))))
}
/** Resident tracks share one instance; samples and disposal are synchronous. */
export async function prepareColorPreview(track: NativeColorTrack,fallback: NativeColor): Promise<PreparedColorSampler> {
  const api = await sharedApi()
  if (nextId > 0xffffffff) throw new Error('Native color track identifiers exhausted')
  const id = nextId++
  api.color_begin()
  const bytes = new TextEncoder().encode(JSON.stringify({ track,default: fallback,times_us: [] }))
  if (bytes.length > 2 * 1024 * 1024) throw new Error('Color preview track exceeds native upload limit')
  for (const byte of bytes) if (api.color_push(byte)) throw new Error('Color preview upload refused')
  if (api.color_track_prepare(id)) throw nativeError(api)
  let disposed = false
  const sample = ((timeUs: number) => {
    if (disposed) throw new Error('Color preview track has been disposed')
    if (api.color_track_at(id,timeUs)) throw nativeError(api)
    const packed = api.color_packed() >>> 0
    return { r: packed >>> 24,g: (packed >>> 16) & 255,b: (packed >>> 8) & 255,a: packed & 255 }
  }) as PreparedColorSampler
  sample.dispose = () => {
    if (disposed) return
    disposed = true
    api.color_track_drop(id)
    released.unregister(sample)
  }
  released.register(sample,{ api,id },sample)
  return sample
}
