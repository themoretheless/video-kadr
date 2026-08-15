// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { proxyController } from '$lib/proxy/state.svelte.js'
import { defaultEdit, state } from '$lib/state/store.svelte.js'
import VideoPreview from './VideoPreview.svelte'

const key = 'a'.repeat(64)
const originalUrl = '/files/sources/source.mp4'

let target: HTMLDivElement
let component: ReturnType<typeof mount> | null = null

beforeEach(() => {
  proxyController.resetForTests()
  state.video = {
    id: 'source-one',
    url: originalUrl,
    filename: 'source.mp4',
    mediaType: 'video',
    duration: 10,
    width: 1280,
    height: 720,
  }
  state.edit = defaultEdit()
  state.edit.trimEnd = 10
  target = document.createElement('div')
  document.body.append(target)
})

afterEach(async () => {
  if (component) await unmount(component)
  component = null
  proxyController.resetForTests()
  state.video = null
  vi.unstubAllGlobals()
  target.remove()
})

describe('VideoPreview proxy playback', () => {
  it('uses a selected same-origin ready proxy and falls back to the unchanged original on media error', async () => {
    const response = {
      sourceId: 'source-one',
      sourceFingerprint: 'b'.repeat(64),
      status: 'ready',
      proxies: [{
        key,
        profile: { maxWidth: 720, codec: 'h264', quality: 28, includeAudio: true },
        status: 'ready',
        url: `/files/proxies/${key}.mp4`,
        sizeBytes: 1024,
        sha256: 'c'.repeat(64),
      }],
      jobs: [],
    }
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify(response), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)
    component = mount(VideoPreview, { target })

    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalledWith('/api/library/source-one/proxies', undefined))
    await tick()
    proxyController.setProxy('source-one', key)
    await tick()

    const video = target.querySelector('video')
    if (!(video instanceof HTMLVideoElement)) throw new Error('Video element not found')
    expect(video.getAttribute('src')).toBe(`/files/proxies/${key}.mp4`)
    expect(target.querySelector('[data-preview-source="proxy"]')?.textContent).toContain('Медиа: Proxy')

    video.dispatchEvent(new Event('error'))
    await tick()
    expect(video.getAttribute('src')).toBe(originalUrl)
    expect(target.querySelector('[data-preview-source="original"]')?.textContent).toContain('используется оригинал')
    expect(state.video?.id).toBe('source-one')
    expect(state.video?.url).toBe(originalUrl)
  })
})
