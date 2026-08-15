import { describe, expect, it, vi } from 'vitest'
import {
  createCompositionPreviewProxyAdapter,
  type CompositionPreviewProxyControllerPort,
} from './compositionPreviewController'
import type { CompositionPreviewSourceRequest } from './compositionPreview'
import type { ProxyControllerState, ProxySourceUiState } from './state.svelte'
import type { ProxyList } from './types'

const key = 'a'.repeat(64)
const list: ProxyList = {
  sourceId: 'video-one',
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
const source: CompositionPreviewSourceRequest = {
  sourceId: 'video-one',
  kind: 'video',
  active: true,
  originalUrl: '/files/sources/original.mp4',
  audibleSourceAudio: false,
}

function readyState(): ProxySourceUiState {
  return {
    phase: 'ready',
    list,
    error: '',
    playbackError: '',
    failedKey: null,
    submitting: false,
  }
}

function controllerFixture() {
  const state: ProxyControllerState = {
    sources: { [source.sourceId]: readyState() },
    preferences: { [source.sourceId]: { mode: 'proxy', key } },
  }
  const controller: CompositionPreviewProxyControllerPort = {
    state,
    ensure: vi.fn(),
    setOriginal: vi.fn((sourceId) => { state.preferences[sourceId] = { mode: 'original' } }),
    setProxy: vi.fn((sourceId, selectedKey) => {
      state.preferences[sourceId] = { mode: 'proxy', key: selectedKey }
      state.sources[sourceId]!.failedKey = null
    }),
    markPlaybackFailed: vi.fn((sourceId, failedKey) => {
      state.sources[sourceId]!.failedKey = failedKey
      state.sources[sourceId]!.playbackError = 'failed'
    }),
  }
  return { controller, state }
}

describe('composition proxy preview controller adapter', () => {
  it('ensures only active video sources and never the project-wide registry', () => {
    const { controller } = controllerFixture()
    const adapter = createCompositionPreviewProxyAdapter(controller, 'other')
    adapter.ensure(source)
    adapter.ensure({ ...source, sourceId: 'video-two' })
    adapter.ensure({ ...source, sourceId: 'audio-one', kind: 'audio' })
    adapter.ensure({ ...source, sourceId: 'hidden', active: false })
    expect(controller.ensure).toHaveBeenCalledTimes(2)
    expect(controller.ensure).toHaveBeenNthCalledWith(1, 'video-one')
    expect(controller.ensure).toHaveBeenNthCalledWith(2, 'video-two')
  })

  it('allows only a ready listed resolution and keeps original explicit', () => {
    const { controller, state } = controllerFixture()
    const adapter = createCompositionPreviewProxyAdapter(controller, 'other')
    adapter.selectResolution(source, 'd'.repeat(64))
    expect(controller.setProxy).not.toHaveBeenCalled()

    adapter.selectResolution(source, key)
    expect(controller.setProxy).toHaveBeenCalledWith(source.sourceId, key)
    expect(adapter.resolve(source)?.kind).toBe('proxy')

    adapter.selectResolution(source, 'original')
    expect(controller.setOriginal).toHaveBeenCalledWith(source.sourceId)
    expect(state.preferences[source.sourceId]).toEqual({ mode: 'original' })
    expect(adapter.resolve(source)).toMatchObject({ kind: 'original', url: source.originalUrl })
  })

  it('marks only the selected proxy failed and immediately resolves original fallback', () => {
    const { controller } = controllerFixture()
    const adapter = createCompositionPreviewProxyAdapter(controller, 'other')
    const selected = adapter.resolve(source)
    expect(selected?.kind).toBe('proxy')

    adapter.markMediaFailed(source, selected)
    expect(controller.markPlaybackFailed).toHaveBeenCalledWith(source.sourceId, key)
    expect(adapter.resolve(source)).toMatchObject({
      kind: 'original',
      url: source.originalUrl,
    })
    expect(adapter.resolve(source)?.fallbackReason).toContain('воспроизвести')

    adapter.markMediaFailed({ ...source, active: false }, selected)
    expect(controller.markPlaybackFailed).toHaveBeenCalledTimes(1)
  })
})
