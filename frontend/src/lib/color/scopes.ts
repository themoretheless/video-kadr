export const SCOPE_RESOLUTION = 256
export const WAVEFORM_HEIGHT = 128
export const MAX_SCOPE_PIXELS = 512 * 512

export interface ColorHistogram {
  readonly red: Uint32Array
  readonly green: Uint32Array
  readonly blue: Uint32Array
  readonly luma: Uint32Array
  readonly maximum: number
}

export interface ColorScopeAnalysis {
  readonly histogram: ColorHistogram
  readonly waveform: Uint32Array
  readonly waveformMaximum: number
  readonly vectorscope: Uint32Array
  readonly vectorscopeMaximum: number
  readonly sampledPixels: number
}

/** Deterministic Rec.709-ish scope analysis over a bounded RGBA frame. */
export function analyzeScopeFrame(data: Uint8ClampedArray, width: number, height: number): ColorScopeAnalysis {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0) {
    throw new Error('Scope frame dimensions are invalid')
  }
  const pixels = width * height
  if (!Number.isSafeInteger(pixels) || pixels > MAX_SCOPE_PIXELS || data.length !== pixels * 4) {
    throw new Error('Scope frame exceeds the analysis limit')
  }

  const red = new Uint32Array(SCOPE_RESOLUTION)
  const green = new Uint32Array(SCOPE_RESOLUTION)
  const blue = new Uint32Array(SCOPE_RESOLUTION)
  const luma = new Uint32Array(SCOPE_RESOLUTION)
  const waveform = new Uint32Array(SCOPE_RESOLUTION * WAVEFORM_HEIGHT)
  const vectorscope = new Uint32Array(SCOPE_RESOLUTION * SCOPE_RESOLUTION)
  let sampledPixels = 0
  let histogramMaximum = 0
  let waveformMaximum = 0
  let vectorscopeMaximum = 0

  for (let pixel = 0; pixel < pixels; pixel += 1) {
    const offset = pixel * 4
    if (data[offset + 3] === 0) continue
    const r = data[offset]!
    const g = data[offset + 1]!
    const b = data[offset + 2]!
    const y = clampByte(Math.round(0.2126 * r + 0.7152 * g + 0.0722 * b))
    histogramMaximum = Math.max(histogramMaximum, ++red[r]!, ++green[g]!, ++blue[b]!, ++luma[y]!)

    const sourceX = pixel % width
    const waveformX = Math.min(SCOPE_RESOLUTION - 1, Math.floor(sourceX * SCOPE_RESOLUTION / width))
    const waveformY = Math.min(WAVEFORM_HEIGHT - 1, Math.floor((255 - y) * WAVEFORM_HEIGHT / 256))
    const waveformIndex = waveformY * SCOPE_RESOLUTION + waveformX
    waveformMaximum = Math.max(waveformMaximum, ++waveform[waveformIndex]!)

    const cb = clampByte(Math.round(128 - 0.114572 * r - 0.385428 * g + 0.5 * b))
    const cr = clampByte(Math.round(128 + 0.5 * r - 0.454153 * g - 0.045847 * b))
    const vectorIndex = (255 - cb) * SCOPE_RESOLUTION + cr
    vectorscopeMaximum = Math.max(vectorscopeMaximum, ++vectorscope[vectorIndex]!)
    sampledPixels += 1
  }

  return {
    histogram: { red, green, blue, luma, maximum: histogramMaximum },
    waveform,
    waveformMaximum,
    vectorscope,
    vectorscopeMaximum,
    sampledPixels,
  }
}

function clampByte(value: number): number {
  return Math.max(0, Math.min(255, value))
}
