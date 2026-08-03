import { afterEach, describe, expect, it, vi } from 'vitest'
import type { VideoScopesWorkerResponse } from '../video-scopes-worker'
import type { ScopeAnalysisRequest } from './controller'
import { VideoScopesWorkerTransport } from './worker-transport'

class FakeWorker {
  static latest: FakeWorker | null = null
  readonly sent: unknown[] = []
  private messages: Array<(event: MessageEvent) => void> = []
  private errors: Array<(event: ErrorEvent) => void> = []
  constructor() { FakeWorker.latest = this }
  addEventListener(type: string, listener: EventListener): void {
    if (type === 'message') this.messages.push(listener as (event: MessageEvent) => void)
    if (type === 'error') this.errors.push(listener as (event: ErrorEvent) => void)
  }
  postMessage(message: unknown): void { this.sent.push(message) }
  terminate(): void {}
  emit(message: VideoScopesWorkerResponse): void {
    this.messages.forEach(listener => listener(new MessageEvent('message', { data: message })))
  }
}

function request(): ScopeAnalysisRequest {
  return {
    schemaVersion: 1,
    requestId: 'request-1',
    generation: 1,
    frameIdentity: {
      sourceFingerprint: 'source', timelineTick: 1, graphVersion: 'graph', tapId: 'post-grade',
      colorDescriptorId: 'straight-rgba8-encoded-srgb', width: 1, height: 1,
      sourceMode: 'original', mappingIdentity: 'original',
    },
    mode: 'paused', maxSamples: 262144, scopes: ['histogram'],
    rgba: new Uint8ClampedArray([255, 0, 0, 255]),
  }
}

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  FakeWorker.latest = null
})

describe('VideoScopesWorkerTransport', () => {
  it('creates no startup timer when Worker construction fails synchronously', () => {
    vi.useFakeTimers()
    vi.stubGlobal('Worker', class { constructor() { throw new Error('worker blocked') } })
    expect(() => new VideoScopesWorkerTransport()).toThrow('worker blocked')
    expect(vi.getTimerCount()).toBe(0)
  })

  it('rejects a terminal response whose echoed frame provenance differs', async () => {
    vi.stubGlobal('Worker', FakeWorker)
    const transport = new VideoScopesWorkerTransport()
    const pending = transport.analyze(request(), new AbortController().signal)
    FakeWorker.latest!.emit({
      type: 'result', schemaVersion: 1, requestId: 'request-1', generation: 1,
      frameIdentity: { ...request().frameIdentity, graphVersion: 'wrong' },
      result: {
        schemaVersion: 1, descriptor: 'straight-rgba8-encoded-srgb', sourceWidth: 1, sourceHeight: 1,
        stride: 1, sampledColumns: 1, sampledRows: 1, sampledPixels: 1, alphaWeight: 255,
        histogram: new Uint32Array(1024),
      },
    })
    await expect(pending).rejects.toThrow('mismatched frame provenance')
    transport.close()
  })

  it('keeps cancellation pending until the worker acknowledges termination', async () => {
    vi.stubGlobal('Worker', FakeWorker)
    const transport = new VideoScopesWorkerTransport()
    const controller = new AbortController()
    let settled = false
    const pending = transport.analyze(request(), controller.signal).finally(() => { settled = true })
    controller.abort()
    await Promise.resolve()
    expect(settled).toBe(false)
    FakeWorker.latest!.emit({
      type: 'cancelled', schemaVersion: 1, requestId: 'request-1', generation: 1,
      frameIdentity: request().frameIdentity,
    })
    await expect(pending).rejects.toMatchObject({ name: 'AbortError' })
    transport.close()
  })

  it('rejects a result that crosses an already-sent cancellation', async () => {
    vi.stubGlobal('Worker', FakeWorker)
    const transport = new VideoScopesWorkerTransport()
    const controller = new AbortController()
    const pending = transport.analyze(request(), controller.signal)
    controller.abort()
    FakeWorker.latest!.emit({
      type: 'result', schemaVersion: 1, requestId: 'request-1', generation: 1,
      frameIdentity: request().frameIdentity,
      result: {
        schemaVersion: 1, descriptor: 'straight-rgba8-encoded-srgb', sourceWidth: 1, sourceHeight: 1,
        stride: 1, sampledColumns: 1, sampledRows: 1, sampledPixels: 1, alphaWeight: 255,
        histogram: new Uint32Array(1024),
      },
    })
    await expect(pending).rejects.toMatchObject({ name: 'AbortError' })
    transport.close()
  })
})
