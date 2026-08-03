import { describe, expect, it } from 'vitest'
import type { VideoScopesResult } from '../domain/video-scopes'
import { videoScopeSummaryRows } from './presentation'

function result(): VideoScopesResult {
  const histogram = new Uint32Array(4 * 256)
  histogram[0] = 2
  histogram[128] = 4
  histogram[255] = 2
  histogram[256 + 20] = 5
  histogram[512 + 40] = 5
  histogram[768 + 60] = 5
  const waveform = new Uint32Array(2 * 256)
  waveform[255 - 10] = 3
  waveform[256 + 255 - 200] = 3
  const parade = new Uint32Array(3 * 2 * 256)
  parade[255 - 5] = 2
  parade[2 * 256 + 255 - 50] = 2
  parade[4 * 256 + 255 - 250] = 2
  const vectorscope = new Uint32Array(256 * 256)
  vectorscope[128 * 256 + 255] = 9
  return {
    schemaVersion: 1,
    descriptor: 'straight-rgba8-encoded-srgb',
    sourceWidth: 2,
    sourceHeight: 1,
    stride: 1,
    // Sample count metadata is intentionally independent of fixed trace width.
    sampledColumns: 1,
    sampledRows: 1,
    sampledPixels: 2,
    alphaWeight: 510,
    histogram,
    waveform,
    parade,
    vectorscope,
  }
}

describe('video scope presentation summaries', () => {
  it('reports stable code-value summaries for histogram, waveform, and RGB parade', () => {
    expect(videoScopeSummaryRows('histogram', result())).toEqual([
      { channel: 'Y′', minimum: '0', median: '128', maximum: '255' },
      { channel: 'R', minimum: '20', median: '20', maximum: '20' },
      { channel: 'G', minimum: '40', median: '40', maximum: '40' },
      { channel: 'B', minimum: '60', median: '60', maximum: '60' },
    ])
    expect(videoScopeSummaryRows('waveform', result()))
      .toEqual([{ channel: 'Y′', minimum: '10', median: '10', maximum: '200' }])
    expect(videoScopeSummaryRows('parade', result())).toEqual([
      { channel: 'R', minimum: '5', median: '5', maximum: '5' },
      { channel: 'G', minimum: '50', median: '50', maximum: '50' },
      { channel: 'B', minimum: '250', median: '250', maximum: '250' },
    ])
  })

  it('exposes a textual vectorscope peak and an empty fallback', () => {
    const vector = videoScopeSummaryRows('vectorscope', result())[0]!
    expect(vector.channel).toBe('Cb/Cr peak')
    expect(vector.minimum).toMatch(/°$/)
    expect(vector.median).toMatch(/%$/)
    expect(vector.maximum).toBe('9')
    expect(videoScopeSummaryRows('histogram', null))
      .toEqual([{ channel: '—', minimum: '—', median: '—', maximum: '—' }])
  })
})
