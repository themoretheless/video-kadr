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

  it('exposes keyboard-labelled controls and seeks through the player adapter', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
      sourceId: 'source-one', sourceFingerprint: 'b'.repeat(64), status: 'ready', proxies: [], jobs: [],
    }), { status: 200 })))
    component = mount(VideoPreview, { target })
    await tick()

    const toolbar = target.querySelector<HTMLElement>('[role="toolbar"]')
    const video = target.querySelector<HTMLVideoElement>('video')
    const seek = target.querySelector<HTMLInputElement>('input[aria-label^="Позиция"]')
    expect(toolbar).not.toBeNull()
    expect(video).not.toBeNull()
    expect(seek?.getAttribute('aria-valuetext')).toBe('0:00 из 0:10')
    expect(target.querySelector('button[aria-label="Воспроизвести"]')).not.toBeNull()

    if (!toolbar || !video || !seek) throw new Error('Preview controls not found')
    seek.value = '4'
    seek.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(video.currentTime).toBe(4)
    expect(state.playerTime).toBe(4)

    toolbar.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
    await tick()
    expect(video.currentTime).toBe(9)
  })

  it('exposes an aria wipe slider that stacks original under edited', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
      sourceId: 'source-one', sourceFingerprint: 'b'.repeat(64), status: 'ready', proxies: [], jobs: [],
    }), { status: 200 })))
    component = mount(VideoPreview, { target })
    await tick()

    const wipeToggle = target.querySelector<HTMLButtonElement>('button[aria-label="Сравнение до/после"]')
    expect(wipeToggle).not.toBeNull()
    wipeToggle?.click()
    await tick()

    const slider = target.querySelector<HTMLInputElement>('input[aria-label="Разделитель до/после"]')
    const videos = target.querySelectorAll('video')
    expect(wipeToggle?.getAttribute('aria-pressed')).toBe('true')
    expect(slider).not.toBeNull()
    expect(slider?.getAttribute('aria-valuenow')).toBe('50')
    expect(videos).toHaveLength(2)
    expect(videos[0]?.hasAttribute('muted') || videos[0]?.muted).toBeTruthy()
    expect(videos[1]?.style.clipPath).toContain('inset(0 0 0 50%)')

    if (!slider) throw new Error('Wipe slider missing')
    slider.value = '25'
    slider.dispatchEvent(new Event('input', { bubbles: true }))
    await tick()
    expect(slider.getAttribute('aria-valuenow')).toBe('25')
    expect(videos[1]?.style.clipPath).toContain('inset(0 0 0 25%)')
  })
})
