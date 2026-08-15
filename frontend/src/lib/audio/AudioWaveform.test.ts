// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { afterEach, describe, expect, it, vi } from 'vitest'
import AudioWaveform from './AudioWaveform.svelte'
import type { AudioDecoderPort, DecodedAudioLike } from './waveform.js'
import { LocalWaveformCache } from './waveformCache.js'

afterEach(() => {
  document.body.innerHTML = ''
  vi.restoreAllMocks()
})

async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0))
  await tick()
}

function cacheWithSamples(samples: readonly number[]): LocalWaveformCache {
  const decoded: DecodedAudioLike = {
    duration: samples.length / 4,
    sampleRate: 4,
    numberOfChannels: 1,
    length: samples.length,
    getChannelData: () => Float32Array.from(samples),
  }
  const decoder: AudioDecoderPort = {
    decodeAudioData: async () => decoded,
    close: async () => undefined,
  }
  return new LocalWaveformCache(
    async () => new Response(new Uint8Array([1, 2, 3])),
    () => decoder,
    2,
    16,
  )
}

describe('AudioWaveform', () => {
  it('renders a locally decoded clip range as an accessible SVG', async () => {
    const target = document.createElement('div')
    document.body.append(target)
    const component = mount(AudioWaveform, {
      target,
      props: {
        url: '/files/sources/voiceover.wav',
        sourceInSeconds: 0.5,
        sourceOutSeconds: 1.5,
        label: 'Voiceover waveform',
        cache: cacheWithSamples([0, 0.2, -0.8, 1, -1, 0.4, 0.1, 0]),
      },
    })
    await settle()

    await vi.waitFor(() => {
      const svg = target.querySelector('svg')
      expect(svg?.getAttribute('aria-label')).toBe('Voiceover waveform')
      expect(svg?.querySelector('path')?.getAttribute('d')).toMatch(/^M/)
      expect(svg?.classList.contains('unavailable')).toBe(false)
    })

    await unmount(component)
  })

  it('fails closed for a non-local URL without breaking the timeline', async () => {
    const target = document.createElement('div')
    document.body.append(target)
    const component = mount(AudioWaveform, {
      target,
      props: {
        url: 'https://example.com/private.mp3',
        cache: cacheWithSamples([0, 1, 0, -1]),
      },
    })
    await settle()

    await vi.waitFor(() => {
      const svg = target.querySelector('svg')
      expect(svg?.classList.contains('unavailable')).toBe(true)
      expect(svg?.getAttribute('aria-label')).toContain('недоступна')
      expect(svg?.querySelector('path')).toBeNull()
    })

    await unmount(component)
  })
})
