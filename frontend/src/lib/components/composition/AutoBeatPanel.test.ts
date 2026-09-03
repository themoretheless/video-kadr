// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Composition } from '$lib/composition/types.js'
import AutoBeatPanel from './AutoBeatPanel.svelte'

const second = 1_000_000

function fixture(): Composition {
  return {
    schemaVersion: 1,
    timeBase: second,
    canvas: { width: 1280, height: 720, fps: 30, backgroundColor: '#000000' },
    sources: { music: { id: 'music', kind: 'audio', durationTicks: 10 * second, width: 0, height: 0, hasAudio: true } },
    tracks: [{
      id: 'audio-track', kind: 'audio', name: 'Music', locked: false, muted: false, solo: false,
      clips: [{
        id: 'music-clip', kind: 'audio', sourceId: 'music', timelineStartTicks: 2 * second,
        sourceInTicks: 0, sourceOutTicks: 10 * second, speed: 1, gain: 1,
      }],
    }],
  }
}

let target: HTMLDivElement

beforeEach(() => {
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(() => target.remove())

describe('AutoBeatPanel', () => {
  it('offers reverse video source audio and excludes freeze clips', async () => {
    const base = fixture()
    const document: Composition = {
      ...base,
      sources: {
        ...base.sources,
        video: { id: 'video', kind: 'video', durationTicks: 4 * second, width: 640, height: 360, hasAudio: true },
      },
      tracks: [
        {
          id: 'video-track', kind: 'video', name: 'Video', locked: false, hidden: false, muted: false,
          transitions: [],
          clips: [
            {
              id: 'reverse-video', kind: 'video', sourceId: 'video', timelineStartTicks: 0,
              sourceInTicks: 0, sourceOutTicks: 2 * second, speed: 1,
              playbackMode: { mode: 'reverse' },
              transform: { x: 0, y: 0, width: 640, height: 360, fit: 'contain' }, opacity: 1,
              sourceAudioEnabled: true, audioGain: 1,
            },
            {
              id: 'freeze-video', kind: 'video', sourceId: 'video', timelineStartTicks: 2 * second,
              sourceInTicks: 2 * second, sourceOutTicks: 4 * second, speed: 1,
              playbackMode: { mode: 'freeze', sourceTick: 3 * second },
              transform: { x: 0, y: 0, width: 640, height: 360, fit: 'contain' }, opacity: 1,
              sourceAudioEnabled: true, audioGain: 1,
            },
          ],
        },
        ...base.tracks,
      ],
    }
    const component = mount(AutoBeatPanel, {
      target,
      props: { document, media: { video: { url: '/files/sources/video.mp4' } }, onapply: vi.fn() },
    })
    await tick()

    const options = [...target.querySelectorAll('option')].map((option) => option.value)
    expect(options).toContain('reverse-video')
    expect(options).not.toContain('freeze-video')
    await unmount(component)
  })

  it('offers preserved speed-ramp audio and excludes muted speed-ramp audio', async () => {
    const base = fixture()
    const track = base.tracks[0]!
    if (track.kind !== 'audio') throw new Error('fixture')
    const rampPoints = [
      { sourceProgressTick: 0, speed: 1 },
      { sourceProgressTick: 5 * second, speed: 2 },
      { sourceProgressTick: 10 * second, speed: 2 },
    ]
    const document: Composition = {
      ...base,
      tracks: [{
        ...track,
        clips: [
          { ...track.clips[0]!, id: 'ramp-audio', speedRamp: { interpolation: 'linear', points: rampPoints, audioPolicy: 'preserve_pitch' } },
          { ...track.clips[0]!, id: 'muted-ramp-audio', timelineStartTicks: 10 * second, speedRamp: { interpolation: 'linear', points: rampPoints, audioPolicy: 'mute' } },
        ],
      }],
    }
    const component = mount(AutoBeatPanel, {
      target,
      props: { document, media: { music: { url: '/files/sources/music.wav' } }, onapply: vi.fn() },
    })
    await tick()

    const options = [...target.querySelectorAll('option')].map((option) => option.value)
    expect(options).toContain('ramp-audio')
    expect(options).not.toContain('muted-ramp-audio')
    await unmount(component)
  })

  it('analyzes a local waveform and applies timeline markers', async () => {
    const buckets = Array.from({ length: 200 }, (_, index) => ({ min: -0.1, max: 0.1, rms: index % 10 === 0 ? 0.95 : 0.02 }))
    const waveformCache = { load: vi.fn().mockResolvedValue({ durationSeconds: 10, sampleRate: 48_000, buckets }) }
    const onapply = vi.fn()
    const component = mount(AutoBeatPanel, {
      target,
      props: {
        document: fixture(),
        media: { music: { url: '/files/sources/music.wav' } },
        waveformCache,
        onapply,
      },
    })
    await tick()
    const analyze = [...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Найти биты'))!
    analyze.click()
    await vi.waitFor(() => expect(target.textContent).toContain('BPM'))
    const apply = [...target.querySelectorAll('button')].find((button) => button.textContent?.includes('Добавить markers'))!
    expect(apply.disabled).toBe(false)
    apply.click()
    await vi.waitFor(() => expect(onapply).toHaveBeenCalledOnce())
    const [beats, bpm] = onapply.mock.calls[0]!
    expect(beats.length).toBeGreaterThan(10)
    expect(beats[0].tick).toBeGreaterThanOrEqual(2 * second)
    expect(bpm).toBeCloseTo(120, 0)
    await unmount(component)
  })

  it('shows an honest empty state without local audio', async () => {
    const component = mount(AutoBeatPanel, {
      target,
      props: { document: fixture(), media: {}, onapply: vi.fn() },
    })
    await tick()
    expect(target.textContent).toContain('Добавьте локальный audio clip')
    await unmount(component)
  })
})
