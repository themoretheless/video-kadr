import { beforeEach, describe, expect, it, vi } from 'vitest'
import { BrowserCaptureSink } from './browser-capture-sink'

describe('BrowserCaptureSink', () => {
  beforeEach(() => {
    Object.defineProperty(navigator, 'storage', { configurable: true, value: {} })
  })

  it('preserves fallback chunks in order and rejects reuse', async () => {
    const sink = await BrowserCaptureSink.create()
    sink.append(new Blob(['a']))
    sink.append(new Blob(['b']))
    const file = await sink.finish('capture.webm', 'video/webm')
    expect(await file.text()).toBe('ab')
    await expect(sink.finish('again.webm', 'video/webm')).rejects.toMatchObject({ code: 'storage_failed' })
  })

  it('serializes OPFS writes and removes staging on cleanup', async () => {
    const stored: Blob[] = []
    const writable = { write: vi.fn(async (blob: Blob) => { stored.push(blob) }), close: vi.fn(async () => undefined), abort: vi.fn(async () => undefined) }
    const staged = new File(['opfs'], 'part')
    const fileHandle = { createWritable: vi.fn(async () => writable), getFile: vi.fn(async () => staged) }
    const directory = { getFileHandle: vi.fn(async () => fileHandle), removeEntry: vi.fn(async () => undefined) }
    const root = { getDirectoryHandle: vi.fn(async () => directory) }
    Object.defineProperty(navigator, 'storage', { configurable: true, value: { getDirectory: vi.fn(async () => root) } })
    const sink = await BrowserCaptureSink.create()
    sink.append(new Blob(['1'])); sink.append(new Blob(['2']))
    const result = await sink.finish('capture.webm', 'video/webm')
    expect(writable.write).toHaveBeenCalledTimes(2)
    expect(result.name).toBe('capture.webm')
    await sink.removeStage()
    expect(directory.removeEntry).toHaveBeenCalledOnce()
  })

  it('reconciles only stale abandoned stages before creating a new owner', async () => {
    const stale = { kind: 'file', getFile: vi.fn(async () => new File(['x'], 'old', { lastModified: Date.now() - 25 * 60 * 60 * 1000 })) }
    const recent = { kind: 'file', getFile: vi.fn(async () => new File(['x'], 'recent', { lastModified: Date.now() })) }
    const writable = { write: vi.fn(async () => undefined), close: vi.fn(async () => undefined), abort: vi.fn(async () => undefined) }
    const created = { createWritable: vi.fn(async () => writable), getFile: vi.fn(async () => new File(['x'], 'new')) }
    const directory = {
      async *entries() { yield ['old.webm.part', stale] as const; yield ['recent.webm.part', recent] as const },
      getFileHandle: vi.fn(async () => created), removeEntry: vi.fn(async () => undefined),
    }
    const root = { getDirectoryHandle: vi.fn(async () => directory) }
    Object.defineProperty(navigator, 'storage', { configurable: true, value: { getDirectory: vi.fn(async () => root) } })
    const sink = await BrowserCaptureSink.create()
    expect(directory.removeEntry).toHaveBeenCalledWith('old.webm.part')
    expect(directory.removeEntry).not.toHaveBeenCalledWith('recent.webm.part')
    await sink.abort()
  })
})
