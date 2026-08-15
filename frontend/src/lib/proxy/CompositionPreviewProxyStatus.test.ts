// @ts-expect-error Vitest's SSR condition exposes server-only mount; this test needs Svelte's client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
import { afterEach, describe, expect, it, vi } from 'vitest'
import CompositionPreviewProxyStatus from './CompositionPreviewProxyStatus.svelte'
import { createCompositionPreviewProxyAdapter } from './compositionPreviewController'
import type { CompositionPreviewSourceRequest } from './compositionPreview'
import { createProxyController, type ProxyClient } from './state.svelte'
import type { ProxyList } from './types'

const key = 'a'.repeat(64)
const silentKey = 'b'.repeat(64)
const source: CompositionPreviewSourceRequest = {
  sourceId: 'video-one',
  kind: 'video',
  active: true,
  originalUrl: '/files/sources/original.mp4',
  audibleSourceAudio: false,
}

function list(includeAudio = true): ProxyList {
  const selectedKey = includeAudio ? key : silentKey
  return {
    sourceId: source.sourceId,
    sourceFingerprint: 'c'.repeat(64),
    status: 'ready',
    proxies: [{
      key: selectedKey,
      profile: { maxWidth: 720, codec: 'h264', quality: 28, includeAudio },
      status: 'ready',
      url: `/files/proxies/${selectedKey}.mp4`,
      sizeBytes: 1024,
      sha256: 'd'.repeat(64),
    }],
    jobs: [],
  }
}

function client(ready: ProxyList): ProxyClient {
  return {
    createLibraryProxy: vi.fn(),
    getLibraryProxies: vi.fn().mockResolvedValue(ready),
    deleteLibraryProxy: vi.fn(),
    pollJob: vi.fn(),
  }
}

let target: HTMLDivElement | null = null
let component: ReturnType<typeof mount> | null = null

afterEach(async () => {
  if (component) await unmount(component)
  component = null
  target?.remove()
  target = null
})

async function renderStatus(ready: ProxyList, request = source) {
  const proxyClient = client(ready)
  const controller = createProxyController(proxyClient, null)
  await controller.refresh(source.sourceId)
  controller.setProxy(source.sourceId, ready.proxies[0]!.key)
  const adapter = createCompositionPreviewProxyAdapter(controller, 'other')
  target = document.createElement('div')
  document.body.append(target)
  component = mount(CompositionPreviewProxyStatus, {
    target,
    props: { source: request, adapter },
  })
  await tick()
  return { proxyClient, controller }
}

describe('CompositionPreviewProxyStatus', () => {
  it('shows the truthful ready proxy, original export note and real resolution choices', async () => {
    const { controller } = await renderStatus(list())
    const indicator = target!.querySelector('.composition-proxy-indicator')
    expect(indicator?.classList.contains('proxy')).toBe(true)
    expect(indicator?.textContent).toContain('Proxy · 720px')
    expect(target!.textContent).toContain('Экспорт: Original')

    const select = target!.querySelector('select')
    if (!(select instanceof HTMLSelectElement)) throw new Error('Resolution selector not found')
    expect([...select.options].map(({ value, textContent }) => ({ value, textContent }))).toEqual([
      { value: 'original', textContent: 'Original · исходное разрешение' },
      { value: key, textContent: 'Proxy · 720px · H.264 · со звуком' },
    ])
    select.value = 'original'
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await tick()
    expect(controller.state.preferences[source.sourceId]).toEqual({ mode: 'original' })
    expect(target!.querySelector('.composition-proxy-indicator')?.textContent).toBe('Original')
  })

  it('shows an explicit original fallback and hides a silent proxy for audible source audio', async () => {
    await renderStatus(list(false), { ...source, audibleSourceAudio: true })
    const indicator = target!.querySelector('.composition-proxy-indicator')
    expect(indicator?.classList.contains('proxy')).toBe(false)
    expect(indicator?.textContent).toBe('Original · fallback')
    expect(target!.textContent).toContain('Proxy без звука')
    expect(target!.querySelectorAll('option')).toHaveLength(1)
    expect(target!.querySelector('video')).toBeNull()
    expect(target!.querySelector('audio')).toBeNull()
  })
})
