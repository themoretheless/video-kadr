import { describe, expect, it } from 'vitest'
import { DECIMAL_MB, estimateExportSize, finalOutputDuration, resolveExportSizing, solveTargetVideoBitrate } from './export-size'

const video = { durationSeconds: 60, width: 1920, height: 1080, fps: 30, format: 'mp4', codec: 'h264' }

describe('size-v1 export estimator', () => {
  it('uses final trim, retained cuts, speed and multicam duration', () => {
    expect(finalOutputDuration(100, { trim: { start: 10, end: 50 }, segments: [{ start: 10, end: 20 }, { start: 30, end: 50 }], speed: 2 })).toEqual({ selectedSeconds: 30, outputSeconds: 15 })
    expect(finalOutputDuration(100, { trim: { start: 10, end: 50 }, segments: [{ start: 0, end: 20 }, { start: 30, end: 100 }] })).toEqual({ selectedSeconds: 30, outputSeconds: 30 })
    expect(finalOutputDuration(999, { multicamFlatten: { durationTicks: 900, timeBase: 90 }, speed: .5 }).outputSeconds).toBe(20)
    expect(finalOutputDuration(1, { timelineRender: { durationTicks: 6_000_000, timeBase: 1_000_000 }, trim: { start: 1, end: 5 } }).selectedSeconds).toBe(4)
  })

  it('resolves transformed and multicam output geometry canonically', () => {
    expect(resolveExportSizing({ duration: 10, width: 1920, height: 1080, fps: 30 }, {
      crop: { w: 1000, h: 500 }, scale: { w: 0, h: 250 }, rotate: 90, pad: '16:9', fps: 24,
    })).toMatchObject({ width: 889, height: 501, fps: 24 })
    expect(resolveExportSizing({ duration: 99, width: 1, height: 1 }, {
      multicamFlatten: { durationTicks: 500, timeBase: 100, target: { width: 1280, height: 720, fps: 25 } },
    })).toMatchObject({ width: 1280, height: 720, fps: 25, durationSeconds: 5 })
    expect(resolveExportSizing({ duration: 99, width: 1, height: 1 }, {
      timelineRender: { durationTicks: 5_000_000, timeBase: 1_000_000, target: { width: 854, height: 480, fps: 30 } },
    })).toMatchObject({ width: 854, height: 480, fps: 30, durationSeconds: 5 })
  })

  it('is monotonic by CRF and gives newer codecs a lower center at equal geometry', () => {
    const high = estimateExportSize({ ...video, crf: 18 })
    const compact = estimateExportSize({ ...video, crf: 28 })
    const h264Default = estimateExportSize({ ...video, crf: 23 })
    const h265 = estimateExportSize({ ...video, codec: 'h265', crf: 28 })
    expect(high.centerBytes).toBeGreaterThan(compact.centerBytes)
    expect(h265.centerBytes).toBeLessThan(h264Default.centerBytes)
    expect(compact.lowBytes).toBeLessThan(compact.centerBytes)
    expect(compact.highBytes).toBeGreaterThan(compact.centerBytes)
  })

  it('reserves audio, mux overhead and safety when solving a decimal-MB target', () => {
    const targetBytes = 50 * DECIMAL_MB
    const kbps = solveTargetVideoBitrate({ ...video, targetBytes })!
    expect(kbps).toBe(Math.round((targetBytes * 8 * .95 / 60 - 128_000) / 1000))
    const estimate = estimateExportSize({ ...video, targetBytes })
    expect(estimate.targetVideoBitrateKbps).toBe(kbps)
    expect(estimate.lowBytes).toBeLessThan(targetBytes)
    expect(estimate.highBytes).toBeGreaterThan(targetBytes)
    expect(solveTargetVideoBitrate({ ...video, muted: true, targetBytes })).toBeGreaterThan(kbps)
  })

  it('shares the high-pixel-rate lower-bound edge with Rust size-v1', () => {
    expect(() => solveTargetVideoBitrate({ durationSeconds: 60, width: 3840, height: 2160, fps: 60, format: 'mp4', codec: 'h264', targetBytes: 1_000_000 })).toThrow('bounds')
  })

  it('enforces the supported target matrix and browser bitrate ceiling', () => {
    expect(() => solveTargetVideoBitrate({ ...video, format: 'prores', targetBytes: 50 * DECIMAL_MB })).toThrow('unsupported')
    expect(() => solveTargetVideoBitrate({ ...video, browser: true, targetBytes: 5000 * DECIMAL_MB })).toThrow('bounds')
    expect(() => solveTargetVideoBitrate({ ...video, browser: false, targetBytes: 500 * DECIMAL_MB })).not.toThrow()
    expect(() => solveTargetVideoBitrate({ ...video, targetBytes: 10 * DECIMAL_MB })).not.toThrow()
    expect(() => solveTargetVideoBitrate({ ...video, durationSeconds: 1, muted: true, targetBytes: 128 * 1024 })).not.toThrow()
    expect(() => solveTargetVideoBitrate({ ...video, durationSeconds: 1, muted: true, targetBytes: 128 * 1024 - 1 })).toThrow('invalid')
  })

  it('models audio, still, GIF and ProRes with explicit honest bands', () => {
    for (const format of ['mp3', 'png', 'jpg', 'gif', 'prores']) {
      const estimate = estimateExportSize({ ...video, format })
      expect(estimate.lowBytes).toBeLessThanOrEqual(estimate.centerBytes)
      expect(estimate.highBytes).toBeGreaterThan(estimate.centerBytes)
      expect(estimate.assumptions.length).toBeGreaterThan(0)
    }
  })
})
