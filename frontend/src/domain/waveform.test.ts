import { describe, expect, it } from 'vitest'
import { drawWaveform, extractPeaks, type DecodedAudioLike } from './waveform'

function buffer(channels: number[][], duration = 1): DecodedAudioLike {
  const length = channels[0]?.length ?? 0
  return {
    numberOfChannels: channels.length,
    length,
    duration,
    getChannelData: (channel) => Float32Array.from(channels[channel] ?? []),
  }
}

describe('extractPeaks', () => {
  it('reduces every sample of a bucket to its min and max', () => {
    const peaks = extractPeaks(buffer([[0, 0.5, -0.25, 1, -1, 0.1, 0, -0.4]]), 2)
    expect(peaks.buckets).toBe(2)
    expect(peaks.max[0]).toBe(1)
    expect(peaks.max[1]).toBeCloseTo(0.1, 6)
    expect([...peaks.min]).toEqual([-0.25, -1])
  })

  it('merges channels into one envelope', () => {
    const peaks = extractPeaks(
      buffer([
        [0.2, 0.2],
        [-0.9, 0.9],
      ]),
      1,
    )
    expect(peaks.max[0]).toBeCloseTo(0.9, 6)
    expect(peaks.min[0]).toBeCloseTo(-0.9, 6)
  })

  it('never returns fewer buckets than asked, even for a very short source', () => {
    const peaks = extractPeaks(buffer([[1, -1]]), 8)
    expect(peaks.buckets).toBe(8)
    expect(peaks.max).toHaveLength(8)
  })

  it('survives an empty, silent or non-finite source', () => {
    expect([...extractPeaks(buffer([]), 3).max]).toEqual([0, 0, 0])
    const broken = extractPeaks(buffer([[Number.NaN, Number.POSITIVE_INFINITY, 0.5]]), 1)
    expect(broken.max[0]).toBe(0.5)
    expect(broken.min[0]).toBe(0)
  })

  it('clamps a bogus bucket request and a bogus duration', () => {
    expect(extractPeaks(buffer([[1]]), Number.NaN).buckets).toBe(320)
    expect(extractPeaks(buffer([[1]]), -5).buckets).toBe(1)
    expect(extractPeaks(buffer([[1]], Number.NaN), 1).duration).toBe(0)
  })
})

describe('drawWaveform', () => {
  interface Call {
    x: number
    y: number
    width: number
    height: number
  }

  function canvasStub(): { canvas: HTMLCanvasElement; rects: Call[]; colors: string[] } {
    const rects: Call[] = []
    const colors: string[] = []
    const context = {
      fillStyle: '',
      clearRect: () => {},
      fillRect(x: number, y: number, width: number, height: number) {
        rects.push({ x, y, width, height })
        colors.push(this.fillStyle)
      },
    }
    const canvas = {
      width: 0,
      height: 0,
      clientWidth: 100,
      clientHeight: 40,
      getContext: () => context,
    }
    return { canvas: canvas as unknown as HTMLCanvasElement, rects, colors }
  }

  it('sizes the backing store and draws one column per bucket', () => {
    const { canvas, rects } = canvasStub()
    const peaks = extractPeaks(buffer([[1, -1, 0.5, -0.5]]), 2)
    expect(drawWaveform(canvas, peaks, { color: '#fff' })).toBe(true)
    expect(canvas.width).toBeGreaterThan(0)
    expect(rects).toHaveLength(2)
    expect(rects[0]!.height).toBeGreaterThan(rects[1]!.height)
  })

  it('paints the played part in the progress colour', () => {
    const { canvas, colors } = canvasStub()
    const peaks = extractPeaks(buffer([[1, -1, 1, -1]]), 4)
    drawWaveform(canvas, peaks, { color: '#aaa', progress: 0.5, progressColor: '#0f0' })
    expect(colors[0]).toBe('#0f0')
    expect(colors[3]).toBe('#aaa')
  })

  it('reports failure when the environment has no 2D context', () => {
    const canvas = { getContext: () => null } as unknown as HTMLCanvasElement
    expect(drawWaveform(canvas, null, { color: '#fff' })).toBe(false)
  })
})
