export const VIDEO_SCOPES_SCHEMA_VERSION = 1 as const
export const VIDEO_SCOPES_BINS = 256 as const
export const VIDEO_SCOPES_TRACE_WIDTH = 512 as const
export const VIDEO_SCOPES_PAUSED_SAMPLE_CAP = 262_144 as const
export const VIDEO_SCOPES_LIVE_SAMPLE_CAP = 65_536 as const

export type VideoScopeKind = 'histogram' | 'waveform' | 'parade' | 'vectorscope'
export type VideoScopesRate = 'paused' | 'live'

export interface VideoScopesFrame {
  /** Straight (not premultiplied) RGBA8 in encoded sRGB. */
  rgba: Uint8ClampedArray
  width: number
  height: number
}

export interface VideoScopesRequest extends VideoScopesFrame {
  scopes: readonly VideoScopeKind[]
  rate: VideoScopesRate
}

export interface VideoScopesResult {
  schemaVersion: typeof VIDEO_SCOPES_SCHEMA_VERSION
  descriptor: 'straight-rgba8-encoded-srgb'
  sourceWidth: number
  sourceHeight: number
  stride: number
  sampledColumns: number
  sampledRows: number
  sampledPixels: number
  alphaWeight: number
  /** Four 256-bin planes in the fixed order Y', R, G, B. */
  histogram?: Uint32Array
  /** Fixed-width row-major density: x * 256 + inverted Y' bin. */
  waveform?: Uint32Array
  /** Three fixed-width planes R, G, B; each x * 256 + inverted channel bin. */
  parade?: Uint32Array
  /** Row-major density: (0.5 + Cr) x, (0.5 - Cb) y. */
  vectorscope?: Uint32Array
}

export interface VideoScopesAnalysisControl {
  isCancelled?: () => boolean
  /** Worker supplies a macrotask yield so cancel messages can be observed. */
  yieldEvery32Rows?: () => Promise<void>
}

export class VideoScopesCancelledError extends Error {
  constructor() {
    super('video scopes analysis cancelled')
    this.name = 'VideoScopesCancelledError'
  }
}

function bin(value: number): number {
  return Math.min(255, Math.max(0, Math.floor(value * 256)))
}

function uniqueScopes(scopes: readonly VideoScopeKind[]): Set<VideoScopeKind> {
  const valid = new Set<VideoScopeKind>(['histogram', 'waveform', 'parade', 'vectorscope'])
  const selected = new Set<VideoScopeKind>()
  for (const scope of scopes) {
    if (!valid.has(scope)) throw new RangeError(`unknown video scope: ${scope}`)
    selected.add(scope)
  }
  return selected
}

function validate(request: VideoScopesRequest): void {
  if (!Number.isSafeInteger(request.width) || request.width <= 0
    || !Number.isSafeInteger(request.height) || request.height <= 0) {
    throw new RangeError('video scope dimensions must be positive integers')
  }
  if (request.rgba.length !== request.width * request.height * 4) {
    throw new RangeError('RGBA byte length does not match dimensions')
  }
}

/**
 * Deterministic reference analyzer. Sampling uses one square stride in both
 * axes: ceil(sqrt(pixelCount/cap)). Every contribution is weighted by the
 * source alpha byte (0..255), keeping all result planes integer and additive.
 */
export async function analyzeVideoScopes(
  request: VideoScopesRequest,
  control: VideoScopesAnalysisControl = {},
): Promise<VideoScopesResult> {
  validate(request)
  const selected = uniqueScopes(request.scopes)
  const cap = request.rate === 'live'
    ? VIDEO_SCOPES_LIVE_SAMPLE_CAP
    : VIDEO_SCOPES_PAUSED_SAMPLE_CAP
  let stride = Math.max(1, Math.ceil(Math.sqrt((request.width * request.height) / cap)))
  const sampleCount = (dimension: number, step: number) =>
    Math.max(0, Math.ceil((dimension - Math.floor(step / 2)) / step))
  while (sampleCount(request.width, stride) * sampleCount(request.height, stride) > cap) stride++
  const offset = Math.floor(stride / 2)
  const sampledColumns = sampleCount(request.width, stride)
  const sampledRows = sampleCount(request.height, stride)
  const histogram = selected.has('histogram') ? new Uint32Array(4 * 256) : undefined
  const waveform = selected.has('waveform') ? new Uint32Array(VIDEO_SCOPES_TRACE_WIDTH * 256) : undefined
  const parade = selected.has('parade') ? new Uint32Array(3 * VIDEO_SCOPES_TRACE_WIDTH * 256) : undefined
  const vectorscope = selected.has('vectorscope') ? new Uint32Array(256 * 256) : undefined
  let sampledPixels = 0
  let alphaWeight = 0

  for (let sampledY = 0, y = offset; y < request.height; sampledY++, y += stride) {
    if ((sampledY & 31) === 0) {
      if (control.isCancelled?.()) throw new VideoScopesCancelledError()
      if (sampledY !== 0 && control.yieldEvery32Rows) await control.yieldEvery32Rows()
      if (control.isCancelled?.()) throw new VideoScopesCancelledError()
    }
    for (let x = offset; x < request.width; x += stride) {
      const offset = (y * request.width + x) * 4
      const redByte = request.rgba[offset]!
      const greenByte = request.rgba[offset + 1]!
      const blueByte = request.rgba[offset + 2]!
      const weight = request.rgba[offset + 3]!
      const red = redByte / 255
      const green = greenByte / 255
      const blue = blueByte / 255
      const luma = 0.2126 * red + 0.7152 * green + 0.0722 * blue
      const lumaBin = bin(luma)
      const redBin = bin(red)
      const greenBin = bin(green)
      const blueBin = bin(blue)
      const traceX = Math.min(511, Math.floor(x * VIDEO_SCOPES_TRACE_WIDTH / request.width))
      sampledPixels++
      alphaWeight += weight

      if (histogram) {
        histogram[lumaBin]! += weight
        histogram[256 + redBin]! += weight
        histogram[512 + greenBin]! += weight
        histogram[768 + blueBin]! += weight
      }
      if (waveform) waveform[traceX * 256 + (255 - lumaBin)]! += weight
      if (parade) {
        const plane = VIDEO_SCOPES_TRACE_WIDTH * 256
        parade[traceX * 256 + (255 - redBin)]! += weight
        parade[plane + traceX * 256 + (255 - greenBin)]! += weight
        parade[2 * plane + traceX * 256 + (255 - blueBin)]! += weight
      }
      if (vectorscope) {
        const cb = -0.114572 * red - 0.385428 * green + 0.5 * blue
        const cr = 0.5 * red - 0.454153 * green - 0.045847 * blue
        const vectorX = bin(0.5 + cr)
        const vectorY = bin(0.5 - cb)
        vectorscope[vectorY * 256 + vectorX]! += weight
      }
    }
  }

  return {
    schemaVersion: VIDEO_SCOPES_SCHEMA_VERSION,
    descriptor: 'straight-rgba8-encoded-srgb',
    sourceWidth: request.width,
    sourceHeight: request.height,
    stride,
    sampledColumns,
    sampledRows,
    sampledPixels,
    alphaWeight,
    histogram,
    waveform,
    parade,
    vectorscope,
  }
}

export function videoScopesTransferBuffers(result: VideoScopesResult): ArrayBuffer[] {
  return [result.histogram, result.waveform, result.parade, result.vectorscope]
    .filter((plane): plane is Uint32Array => plane !== undefined)
    .map(plane => plane.buffer as ArrayBuffer)
}
