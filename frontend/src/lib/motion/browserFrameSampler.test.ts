import { describe, expect, it } from 'vitest'
import {
  MAX_BROWSER_TRACKING_TOTAL_PIXELS,
  sampleBrowserVideoLumaFrames,
  trackingRasterSize,
  validateBrowserTrackingMediaUrl,
} from './browserFrameSampler'

describe('browser tracking frame sampler', () => {
  it('accepts only same-origin private media mounts', () => {
    expect(validateBrowserTrackingMediaUrl('/files/sources/a.mp4', 'https://editor.test')).toBe(
      'https://editor.test/files/sources/a.mp4',
    )
    expect(() => validateBrowserTrackingMediaUrl('https://evil.test/a.mp4', 'https://editor.test')).toThrow('same-origin')
    expect(() => validateBrowserTrackingMediaUrl('/api/library', 'https://editor.test')).toThrow('media mount')
    expect(() => validateBrowserTrackingMediaUrl('/files/sources/a.mp4?token=x', 'https://editor.test')).toThrow('query')
    expect(() => validateBrowserTrackingMediaUrl('/files/sources/nested/a.mp4', 'https://editor.test')).toThrow('media mount')
  })

  it('downscales proportionally into the bounded tracking raster', () => {
    expect(trackingRasterSize(1_920, 1_080)).toEqual({ width: 640, height: 360 })
    expect(trackingRasterSize(320, 240)).toEqual({ width: 320, height: 240 })
    expect(trackingRasterSize(1_080, 1_920)).toEqual({ width: 203, height: 360 })
  })

  it('samples deterministic luma frames and releases detached media resources', async () => {
    class FakeVideo extends EventTarget {
      preload = ''
      muted = false
      playsInline = false
      src = ''
      duration = 2
      videoWidth = 4
      videoHeight = 2
      readyState = HTMLMediaElement.HAVE_CURRENT_DATA
      paused = false
      removed = false
      loads = 0
      seeks: number[] = []
      private time = 0
      get currentTime(): number { return this.time }
      set currentTime(value: number) {
        this.time = value
        this.seeks.push(value)
        queueMicrotask(() => this.dispatchEvent(new Event('seeked')))
      }
      load(): void {
        this.loads += 1
        if (this.src) queueMicrotask(() => this.dispatchEvent(new Event('loadedmetadata')))
      }
      pause(): void { this.paused = true }
      removeAttribute(name: string): void { if (name === 'src') { this.src = ''; this.removed = true } }
    }
    const video = new FakeVideo()
    const context = {
      drawImage: () => undefined,
      getImageData: () => {
        const data = new Uint8ClampedArray([
          255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
          0, 0, 0, 255, 10, 10, 10, 255, 20, 20, 20, 255, 30, 30, 30, 255,
        ])
        data[0] = Math.round(video.currentTime * 100)
        data[1] = data[0]
        data[2] = data[0]
        return { data }
      },
    }
    const canvas = {
      width: 0,
      height: 0,
      getContext: () => context,
    }
    const result = await sampleBrowserVideoLumaFrames('/files/sources/source.mp4', [1.5, 0.5], {
      origin: 'https://editor.test',
      maxWidth: 4,
      maxHeight: 2,
      createVideo: () => video as unknown as HTMLVideoElement,
      createCanvas: () => canvas as unknown as HTMLCanvasElement,
    })
    expect(result.frames).toHaveLength(2)
    expect(result.frames.map((frame) => frame.data[0])).toEqual([150, 50])
    expect(video.seeks).toEqual([0.5, 1.5])
    expect(result.sampleToSourceScaleX).toBe(1)
    expect(video.paused).toBe(true)
    expect(video.removed).toBe(true)
    expect(canvas).toMatchObject({ width: 0, height: 0 })
  })

  it('fails before allocation when the luma budget would be exceeded', async () => {
    expect(MAX_BROWSER_TRACKING_TOTAL_PIXELS).toBeGreaterThan(0)
    await expect(sampleBrowserVideoLumaFrames('/files/sources/source.mp4', [0], {
      origin: 'https://editor.test',
    })).rejects.toThrow('2..=300')
  })
})
