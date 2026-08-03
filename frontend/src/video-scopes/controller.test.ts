import { afterEach, describe, expect, it, vi } from 'vitest'
import { VideoScopeFrameBroker, type ScopeFrame, type ScopeTapId } from './frame-broker'
import { VideoScopesController, type ScopeAnalysisRequest, type ScopeAnalysisResult, type VideoScopeAnalyzerTransport } from './controller'

function frame(tapId: ScopeTapId, tick: number, accuracy: ScopeFrame['accuracy'] = 'exact-paused'): ScopeFrame {
  return {
    accuracy,
    identity: {
      sourceFingerprint: 'source', timelineTick: tick, graphVersion: `graph-${tick}`,
      tapId, colorDescriptorId: 'straight-rgba8-encoded-srgb', width: 1, height: 1,
      sourceMode: 'original', mappingIdentity: 'original',
    },
    rgba: new Uint8ClampedArray([tick, 0, 0, 255]),
  }
}

class DeferredTransport implements VideoScopeAnalyzerTransport {
  requests: Array<{ request: ScopeAnalysisRequest; signal: AbortSignal; resolve: (result: ScopeAnalysisResult) => void }> = []
  analyze(request: ScopeAnalysisRequest, signal: AbortSignal): Promise<ScopeAnalysisResult> {
    return new Promise(resolve => this.requests.push({ request, signal, resolve }))
  }
}

function result(request: ScopeAnalysisRequest): ScopeAnalysisResult {
  return {
    schemaVersion: 1, requestId: request.requestId, generation: request.generation,
    frameIdentity: request.frameIdentity, stride: 1, sampleCount: 1, alphaWeight: 255,
    elapsedMs: 1, histogram: new Uint32Array(1024),
  }
}

afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks() })

describe('VideoScopesController', () => {
  it('debounces paused frames, cancels in-flight work, and accepts only latest identity', async () => {
    vi.useFakeTimers()
    const broker = new VideoScopeFrameBroker()
    const transport = new DeferredTransport()
    const controller = new VideoScopesController(broker)
    controller.attachTransport(transport)
    controller.setEnabled(true)
    controller.setTap('pre-grade')

    broker.publish(frame('pre-grade', 1))
    await vi.advanceTimersByTimeAsync(49)
    expect(transport.requests).toHaveLength(0)
    await vi.advanceTimersByTimeAsync(1)
    expect(transport.requests).toHaveLength(1)

    broker.publish(frame('pre-grade', 2))
    expect(transport.requests[0]!.signal.aborted).toBe(true)
    await vi.advanceTimersByTimeAsync(50)
    expect(transport.requests).toHaveLength(1)
    transport.requests[0]!.resolve(result(transport.requests[0]!.request))
    await vi.advanceTimersByTimeAsync(0)
    expect(controller.state.frameIdentity).toBeNull()
    expect(transport.requests).toHaveLength(2)
    transport.requests[1]!.resolve(result(transport.requests[1]!.request))
    await Promise.resolve()
    expect(controller.state.frameIdentity?.timelineTick).toBe(2)
    controller.close()
  })

  it('throttles exact live frames to at most 8Hz and fails closed on unavailable taps', async () => {
    vi.useFakeTimers()
    vi.setSystemTime(1_000)
    const broker = new VideoScopeFrameBroker()
    const transport = new DeferredTransport()
    const controller = new VideoScopesController(broker)
    controller.attachTransport(transport)
    controller.setEnabled(true)
    controller.setTap('post-grade')
    broker.publish(frame('post-grade', 1, 'exact-live'))
    expect(transport.requests).toHaveLength(1)
    transport.requests[0]!.resolve(result(transport.requests[0]!.request))
    await Promise.resolve()
    vi.setSystemTime(1_050)
    broker.publish(frame('post-grade', 2, 'exact-live'))
    await vi.advanceTimersByTimeAsync(74)
    expect(transport.requests).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(1)
    expect(transport.requests).toHaveLength(2)
    broker.unavailable('post-grade', 'exact post unavailable')
    expect(controller.state.reason).toBe('exact post unavailable')
    expect(controller.state.status).toBe('unavailable')
    expect(controller.state.result).toBeNull()
    expect(controller.state.frameIdentity).toBeNull()
    expect(controller.state.accuracy).toBeNull()
    controller.close()
  })

  it('clears accepted provenance on graph invalidation and tap changes', async () => {
    vi.useFakeTimers()
    const broker = new VideoScopeFrameBroker()
    const transport = new DeferredTransport()
    const controller = new VideoScopesController(broker)
    controller.attachTransport(transport)
    controller.setEnabled(true)
    controller.setTap('pre-grade')
    broker.publish(frame('pre-grade', 3))
    await vi.advanceTimersByTimeAsync(50)
    transport.requests[0]!.resolve(result(transport.requests[0]!.request))
    await Promise.resolve()
    expect(controller.state.status).toBe('ready')
    expect(controller.state.accuracy).toBe('exact-paused')

    broker.invalidate()
    expect(controller.state.status).toBe('waiting')
    expect(controller.state.result).toBeNull()
    expect(controller.state.frameIdentity).toBeNull()
    expect(controller.state.accuracy).toBeNull()

    controller.setTap('post-grade')
    expect(controller.state.result).toBeNull()
    controller.close()
  })
})
