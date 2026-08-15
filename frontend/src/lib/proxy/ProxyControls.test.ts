// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Capabilities, MediaEntry } from '$lib/types'
import ProxyControls from './ProxyControls.svelte'
import { createProxyController, type ProxyClient } from './state.svelte'
import type { ProxyList } from './types'

const key = 'a'.repeat(64)
const entry: MediaEntry = {
  id: 'source-one',
  kind: 'source',
  filename: 'source.mp4',
  url: '/files/sources/source.mp4',
  mediaType: 'video',
  createdAt: 1,
}
const capabilities: Capabilities = {
  schemaVersion: 1,
  toolFingerprint: 'test',
  formats: [
    { id: 'mp4', label: 'MP4', available: true },
    { id: 'prores', label: 'ProRes', available: true },
  ],
  codecs: [{ id: 'h264', label: 'H.264', available: true }],
  filters: [],
  hardware: [],
}
const ready: ProxyList = {
  sourceId: entry.id,
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

let target: HTMLDivElement | null = null
let component: ReturnType<typeof mount> | null = null

afterEach(async () => {
  if (component) await unmount(component)
  component = null
  target?.remove()
  target = null
})

describe('ProxyControls', () => {
  it('exposes accessible profile controls and selects a ready proxy for preview', async () => {
    const client: ProxyClient = {
      createLibraryProxy: vi.fn(),
      getLibraryProxies: vi.fn().mockResolvedValue(ready),
      deleteLibraryProxy: vi.fn(),
      pollJob: vi.fn(),
    }
    const controller = createProxyController(client, null)
    target = document.createElement('div')
    document.body.append(target)
    component = mount(ProxyControls, { target, props: { entry, capabilities, controller } })

    await tick()
    await vi.waitFor(() => expect(client.getLibraryProxies).toHaveBeenCalledWith(entry.id))
    await Promise.resolve()
    await tick()
    expect(target.textContent).toContain('720px · H.264')
    const selects = target.querySelectorAll('select')
    expect(selects).toHaveLength(2)
    expect([...selects[0]!.options].map((option) => option.textContent)).toEqual(['480 px', '720 px', '960 px'])
    const prores = [...selects[1]!.options].find((option) => option.value === 'prores_proxy')
    expect(prores?.disabled).toBe(true)

    const selectProxy = [...target.querySelectorAll('button')].find((button) => button.textContent === 'Для просмотра')
    if (!(selectProxy instanceof HTMLButtonElement)) throw new Error('Proxy selection button not found')
    selectProxy.click()
    await tick()
    expect(selectProxy.getAttribute('aria-pressed')).toBe('true')
    expect(controller.playback(entry.id, entry.url).kind).toBe('proxy')
    expect(target.textContent).toContain('Экспорт всегда использует оригинальный source')
  })
})
