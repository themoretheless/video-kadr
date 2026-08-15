import { describe, expect, it, vi } from 'vitest'
import type { Job } from '$lib/types'
import { PROXY_PREFERENCE_STORAGE_KEY } from './model'
import { createProxyController, type ProxyClient } from './state.svelte'
import type { ProxyList, ProxyProfile } from './types'

class MemoryStorage {
  readonly values = new Map<string, string>()
  getItem(key: string): string | null { return this.values.get(key) ?? null }
  setItem(key: string, value: string): void { this.values.set(key, value) }
  removeItem(key: string): void { this.values.delete(key) }
}

const key = 'a'.repeat(64)
const sourceFingerprint = 'b'.repeat(64)
const profile: ProxyProfile = { maxWidth: 720, codec: 'h264', quality: 28, includeAudio: true }

function list(status: 'none' | 'processing' | 'ready'): ProxyList {
  return {
    sourceId: 'source-one',
    sourceFingerprint,
    status,
    proxies: status === 'ready' ? [{
      key,
      profile,
      status: 'ready',
      url: `/files/proxies/${key}.mp4`,
      sizeBytes: 1024,
      sha256: 'c'.repeat(64),
    }] : [],
    jobs: status === 'processing' ? [{ jobId: 'job-1', key, profile, status: 'running', progress: 20 }] : [],
  }
}

describe('proxy controller', () => {
  it('persists selection, follows progress to ready and never changes source identity', async () => {
    const storage = new MemoryStorage()
    const client: ProxyClient = {
      createLibraryProxy: vi.fn().mockResolvedValue({ jobId: 'job-1', key }),
      getLibraryProxies: vi.fn()
        .mockResolvedValueOnce(list('processing'))
        .mockResolvedValue(list('ready')),
      deleteLibraryProxy: vi.fn(),
      pollJob: vi.fn(async (_jobId: string, onTick?: (job: Job) => void) => {
        onTick?.({ id: 'job-1', status: 'running', progress: 67, stage: 'processing' })
        return { id: 'job-1', status: 'done' } satisfies Job
      }),
    }
    const controller = createProxyController(client, storage)

    await expect(controller.generate('source-one', profile)).resolves.toEqual({ jobId: 'job-1', key })
    await vi.waitFor(() => expect(controller.state.sources['source-one']?.list?.status).toBe('ready'))
    expect(controller.state.preferences['source-one']).toEqual({ mode: 'proxy', key })
    expect(storage.getItem(PROXY_PREFERENCE_STORAGE_KEY)).toContain(key)
    expect(createProxyController(client, storage).state.preferences['source-one']).toEqual({
      mode: 'proxy',
      key,
    })
    expect(controller.playback('source-one', '/files/sources/original.mp4')).toMatchObject({
      kind: 'proxy',
      url: `/files/proxies/${key}.mp4`,
    })
  })

  it('deletes/cancels by content key and switches a selected proxy back to original', async () => {
    const client: ProxyClient = {
      createLibraryProxy: vi.fn(),
      getLibraryProxies: vi.fn().mockResolvedValueOnce(list('ready')).mockResolvedValue(list('none')),
      deleteLibraryProxy: vi.fn().mockResolvedValue(undefined),
      pollJob: vi.fn(),
    }
    const controller = createProxyController(client, new MemoryStorage())
    await controller.refresh('source-one')
    controller.setProxy('source-one', key)

    await expect(controller.remove('source-one', key)).resolves.toBe(true)
    expect(client.deleteLibraryProxy).toHaveBeenCalledWith('source-one', key)
    expect(controller.state.preferences['source-one']).toEqual({ mode: 'original' })
    expect(controller.playback('source-one', '/files/sources/original.mp4')).toMatchObject({
      kind: 'original',
      url: '/files/sources/original.mp4',
    })
  })
})
