import { describe, expect, it } from 'vitest'
import { rgbaToLuma, trackPointSequence, type GrayFrame } from './tracker'

function patternedFrame(width: number, height: number, centerX: number, centerY: number): GrayFrame {
  const data = new Uint8Array(width * height)
  const pattern = [
    [10, 30, 80, 20, 5],
    [25, 120, 220, 70, 15],
    [60, 180, 255, 140, 40],
    [15, 90, 200, 50, 10],
    [5, 20, 70, 25, 0],
  ]
  for (let offsetY = -2; offsetY <= 2; offsetY += 1) {
    for (let offsetX = -2; offsetX <= 2; offsetX += 1) {
      data[(centerY + offsetY) * width + centerX + offsetX] = pattern[offsetY + 2]![offsetX + 2]!
    }
  }
  return { width, height, data }
}

describe('classical point tracker', () => {
  it('tracks deterministic integer translation and emits position keyframe samples', () => {
    const frames = [
      patternedFrame(32, 24, 10, 9),
      patternedFrame(32, 24, 12, 10),
      patternedFrame(32, 24, 14, 12),
    ]

    const result = trackPointSequence(frames, { x: 10, y: 9 }, {
      patchRadius: 2,
      searchRadius: 4,
      minimumConfidence: 0.7,
    })

    expect(result.status).toBe('completed')
    expect(result.points.map(({ x, y, frameIndex }) => ({ x, y, frameIndex }))).toEqual([
      { x: 10, y: 9, frameIndex: 0 },
      { x: 12, y: 10, frameIndex: 1 },
      { x: 14, y: 12, frameIndex: 2 },
    ])
    expect(result.points.every((point) => point.confidence >= 0.7)).toBe(true)
  })

  it('stops honestly when the template disappears', () => {
    const first = patternedFrame(24, 20, 8, 8)
    const blank: GrayFrame = { width: 24, height: 20, data: new Uint8Array(24 * 20) }

    const result = trackPointSequence([first, blank], { x: 8, y: 8 }, {
      patchRadius: 2,
      searchRadius: 4,
    })

    expect(result).toEqual({
      status: 'lost',
      points: [{ x: 8, y: 8, frameIndex: 0, confidence: 1 }],
      lostAtFrame: 1,
    })
  })

  it('converts RGBA to BT.601 luma without using canvas state', () => {
    const frame = rgbaToLuma(
      new Uint8ClampedArray([
        255, 0, 0, 255,
        0, 255, 0, 255,
        0, 0, 255, 255,
      ]),
      3,
      1,
    )
    expect([...frame.data]).toEqual([76, 150, 29])
  })

  it('rejects mismatched frames, edge patches, and excessive work', () => {
    const frame = patternedFrame(32, 24, 10, 9)
    expect(() => trackPointSequence([
      frame,
      { width: 16, height: 16, data: new Uint8Array(256) },
    ], { x: 10, y: 9 }, { patchRadius: 2, searchRadius: 4 })).toThrow('identical')
    expect(() => trackPointSequence([frame], { x: 0, y: 0 }, {
      patchRadius: 2,
      searchRadius: 4,
    })).toThrow('outside')
    const largeFrame = patternedFrame(128, 128, 64, 64)
    expect(() => trackPointSequence(Array.from({ length: 1_000 }, () => largeFrame), { x: 64, y: 64 }, {
      patchRadius: 10,
      searchRadius: 30,
    })).toThrow('operation budget')
  })
})
