import { beforeEach, describe, expect, it, vi } from 'vitest'
import { resolveBackendPreviewSource } from './proxy-preview'
import * as api from './api'
import type { DerivedTask } from './browser-derived-queue'

vi.mock('./api', () => ({ getProxyStatus: vi.fn(), invalidateProxy: vi.fn() }))
const video = { id: 'source', url: '/files/sources/original.mp4', filename: 'original.mp4', duration: 10, width: 1920, height: 1080, fingerprint: 'a'.repeat(64) }
const task = { id: 'task', projectId: 'media:source', consumerProjectIds: ['media:source'], kind: 'proxy', state: 'succeeded', idempotencyKey: 'b'.repeat(64) } as DerivedTask

describe('backend proxy preview resolver', () => {
  beforeEach(() => {
    vi.mocked(api.getProxyStatus).mockReset()
    vi.mocked(api.invalidateProxy).mockReset().mockResolvedValue(undefined)
  })

  it('selects only a verified ready proxy for the matching source consumer', async () => {
    vi.mocked(api.getProxyStatus).mockResolvedValue({ state: 'ready', artifactKey: task.idempotencyKey, sourceId: 'source', sourceFingerprint: video.fingerprint, previewUrl: '/api/proxies/key/preview', sourceMedia: {}, proxyMedia: { audioCodec: 'aac' } })
    await expect(resolveBackendPreviewSource(video, 'auto', [task])).resolves.toMatchObject({ usingProxy: true, url: '/api/proxies/key/preview' })
    expect(api.getProxyStatus).toHaveBeenCalledWith(task.idempotencyKey, video.id, video.fingerprint)
  })

  it('keeps the immutable original for off, missing, and stale proxy states', async () => {
    await expect(resolveBackendPreviewSource(video, 'original', [task])).resolves.toMatchObject({ usingProxy: false, url: video.url })
    await expect(resolveBackendPreviewSource(video, 'proxy', [])).resolves.toMatchObject({ status: 'missing', url: video.url })
    vi.mocked(api.getProxyStatus).mockResolvedValue({ state: 'stale', artifactKey: task.idempotencyKey, sourceId: 'source', sourceFingerprint: 'c'.repeat(64), previewUrl: null, sourceMedia: {}, proxyMedia: {} })
    await expect(resolveBackendPreviewSource(video, 'proxy', [task])).resolves.toMatchObject({ status: 'stale', url: video.url })
    expect(api.invalidateProxy).not.toHaveBeenCalled()
  })
})
