import { describe, expect, it } from 'vitest'
import {
  computeHistogram,
  computeVectorscope,
  computeWaveform,
  gradeFrame,
  gradeTransfer,
  gradeTransferTables,
  isNeutralGrade,
  lumaOf,
  sampleDimensions,
  type FrameSample,
  type ScopeGrade,
} from './scopes'

/** Synthetic frame: `pixel(x, y)` returns the RGB triple for that position. */
function frameOf(
  width: number,
  height: number,
  pixel: (x: number, y: number) => [number, number, number],
): FrameSample {
  const data = new Uint8ClampedArray(width * height * 4)
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const at = (y * width + x) * 4
      const [r, g, b] = pixel(x, y)
      data[at] = r
      data[at + 1] = g
      data[at + 2] = b
      data[at + 3] = 255
    }
  }
  return { width, height, data }
}

const solid = (r: number, g: number, b: number) => () => [r, g, b] as [number, number, number]

function neutralGrade(): ScopeGrade {
  return {
    temperature: 0,
    tint: 0,
    exposure: 0,
    highlights: 0,
    shadows: 0,
    lift: { r: 0, g: 0, b: 0 },
    gamma: { r: 1, g: 1, b: 1 },
    gain: { r: 1, g: 1, b: 1 },
  }
}

describe('sampleDimensions', () => {
  it('fits the source inside the cap and keeps the aspect ratio', () => {
    expect(sampleDimensions(1920, 1080, 320)).toEqual({ width: 320, height: 180 })
    expect(sampleDimensions(1080, 1920, 320)).toEqual({ width: 320, height: 569 })
  })

  it('never upscales and never returns a zero side', () => {
    expect(sampleDimensions(64, 36, 320)).toEqual({ width: 64, height: 36 })
    expect(sampleDimensions(0, 0, 320).width).toBeGreaterThan(0)
    expect(sampleDimensions(Number.NaN, Number.NaN, 320).height).toBeGreaterThan(0)
  })
})

describe('computeHistogram', () => {
  it('puts a flat black frame entirely in the first bin', () => {
    const histogram = computeHistogram(frameOf(8, 4, solid(0, 0, 0)))
    expect(histogram.red[0]).toBe(32)
    expect(histogram.luma[0]).toBe(32)
    expect(histogram.peak).toBe(32)
    expect(histogram.red[255]).toBe(0)
  })

  it('separates the channels of a pure primary', () => {
    const histogram = computeHistogram(frameOf(4, 4, solid(255, 0, 0)))
    expect(histogram.red[255]).toBe(16)
    expect(histogram.green[0]).toBe(16)
    expect(histogram.blue[0]).toBe(16)
    // Rec.709 luma of pure red is 0.2126.
    expect(histogram.luma[Math.round(lumaOf(255, 0, 0))]).toBe(16)
  })

  it('spreads a horizontal ramp over every bin', () => {
    const histogram = computeHistogram(frameOf(256, 1, (x) => [x, x, x]))
    for (let bin = 0; bin < 256; bin++) expect(histogram.luma[bin]).toBe(1)
  })

  it('ignores pixels the buffer cannot actually address', () => {
    const short: FrameSample = { width: 4, height: 4, data: new Uint8ClampedArray(8) }
    expect(computeHistogram(short).peak).toBe(2)
  })
})

describe('computeWaveform', () => {
  it('maps a vertical split onto two column groups', () => {
    // Left half black, right half white.
    const frame = frameOf(16, 4, (x) => (x < 8 ? [0, 0, 0] : [255, 255, 255]))
    const waveform = computeWaveform(frame, 16, 128)
    expect(waveform.luma[0 * 128 + 0]).toBe(4)
    expect(waveform.luma[15 * 128 + 127]).toBe(4)
    expect(waveform.luma[0 * 128 + 127]).toBe(0)
  })

  it('parades each channel independently', () => {
    const waveform = computeWaveform(frameOf(4, 4, solid(255, 0, 0)), 4, 128)
    expect(waveform.red[0 * 128 + 127]).toBe(4)
    expect(waveform.green[0 * 128 + 0]).toBe(4)
    expect(waveform.blue[0 * 128 + 0]).toBe(4)
  })
})

describe('computeVectorscope', () => {
  it('collapses a neutral frame onto the centre', () => {
    const scope = computeVectorscope(frameOf(8, 8, solid(128, 128, 128)), 65)
    const centre = 32 * 65 + 32
    expect(scope.bins[centre]).toBe(64)
    expect(scope.peak).toBe(64)
  })

  it('pushes pure blue and pure red to opposite sides of the centre', () => {
    const blue = computeVectorscope(frameOf(2, 2, solid(0, 0, 255)), 65)
    const red = computeVectorscope(frameOf(2, 2, solid(255, 0, 0)), 65)
    const at = (scope: { size: number; bins: Uint32Array }) => {
      const index = scope.bins.indexOf(4)
      return { column: index % scope.size, row: Math.floor(index / scope.size) }
    }
    // Blue is +U (right of centre), red is +V (above centre).
    expect(at(blue).column).toBeGreaterThan(32)
    expect(at(red).row).toBeLessThan(32)
    expect(at(red).column).toBeLessThan(32)
  })
})

describe('gradeTransfer', () => {
  it('is the identity for a neutral grade', () => {
    const [red, green, blue] = gradeTransfer(neutralGrade(), 256)
    for (const step of [0, 64, 128, 255]) {
      expect(red[step]).toBeCloseTo(step / 255, 6)
      expect(green[step]).toBeCloseTo(step / 255, 6)
      expect(blue[step]).toBeCloseTo(step / 255, 6)
    }
  })

  it('gains red and cuts blue as the image is warmed', () => {
    const grade = { ...neutralGrade(), temperature: 1 }
    const [red, , blue] = gradeTransfer(grade, 256)
    expect(red[128]).toBeGreaterThan(128 / 255)
    expect(blue[128]).toBeLessThan(128 / 255)
  })

  it('doubles the signal for one stop of exposure and clips at white', () => {
    const [red] = gradeTransfer({ ...neutralGrade(), exposure: 1 }, 256)
    expect(red[64]).toBeCloseTo((64 / 255) * 2, 3)
    expect(red[200]).toBe(1)
  })

  it('applies the wheel formula so gain clips and gamma bends the midtones', () => {
    const [red] = gradeTransfer(
      { ...neutralGrade(), gain: { r: 2, g: 1, b: 1 } },
      256,
    )
    expect(red[64]).toBeCloseTo((64 / 255) * 2, 3)
    expect(red[200]).toBe(1)

    const [, green] = gradeTransfer(
      { ...neutralGrade(), gamma: { r: 1, g: 2, b: 1 } },
      256,
    )
    // gamma 2 means out(x) = sqrt(x).
    expect(green[64]).toBeCloseTo(Math.sqrt(64 / 255), 3)
  })

  it('lifts the black point without moving white', () => {
    const [red] = gradeTransfer({ ...neutralGrade(), lift: { r: 0.1, g: 0, b: 0 } }, 256)
    expect(red[0]).toBeCloseTo(0.1, 6)
    expect(red[255]).toBe(1)
  })

  it('moves only the quarter tones for highlight and shadow recovery', () => {
    const [red] = gradeTransfer({ ...neutralGrade(), highlights: -1, shadows: 1 }, 5)
    expect(red[0]).toBe(0)
    expect(red[1]).toBeCloseTo(0.45, 6)
    expect(red[2]).toBeCloseTo(0.5, 6)
    expect(red[3]).toBeCloseTo(0.55, 6)
    expect(red[4]).toBe(1)
  })

  it('neutralises non-finite input instead of leaking NaN into a table', () => {
    const broken: ScopeGrade = {
      ...neutralGrade(),
      temperature: Number.NaN,
      exposure: Number.POSITIVE_INFINITY,
      gamma: { r: Number.NaN, g: 1, b: 1 },
    }
    for (const table of gradeTransfer(broken, 16)) {
      for (const value of table) expect(Number.isFinite(value)).toBe(true)
    }
  })
})

describe('gradeFrame', () => {
  it('returns the very same sample when nothing is graded', () => {
    const frame = frameOf(2, 2, solid(10, 20, 30))
    expect(gradeFrame(frame, neutralGrade())).toBe(frame)
    expect(isNeutralGrade(neutralGrade())).toBe(true)
  })

  it('grades a copy and leaves alpha and the source buffer alone', () => {
    const frame = frameOf(2, 2, solid(64, 64, 64))
    const graded = gradeFrame(frame, { ...neutralGrade(), exposure: 1 })
    expect(graded.data[0]).toBe(128)
    expect(graded.data[3]).toBe(255)
    expect(frame.data[0]).toBe(64)
  })

  it('feeds the scopes the graded pixels', () => {
    const frame = frameOf(4, 4, solid(64, 64, 64))
    const histogram = computeHistogram(gradeFrame(frame, { ...neutralGrade(), exposure: 1 }))
    expect(histogram.luma[128]).toBe(16)
  })
})

describe('gradeTransferTables', () => {
  it('emits one ramp per channel for feComponentTransfer', () => {
    const tables = gradeTransferTables(neutralGrade(), 5)
    expect(tables).toHaveLength(3)
    expect(tables[0]).toBe('0.0000 0.2500 0.5000 0.7500 1.0000')
  })

  it('tracks the grade it was built from', () => {
    const [red] = gradeTransferTables({ ...neutralGrade(), exposure: 1 }, 5)
    expect(red).toBe('0.0000 0.5000 1.0000 1.0000 1.0000')
  })
})
