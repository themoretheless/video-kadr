import { describe, expect, it } from 'vitest'
import {
  createVideoScopesWorkerHandler,
  type VideoScopesWorkerResponse,
} from './video-scopes-worker'

describe('video scopes worker protocol', () => {
  const frameIdentity = {
    sourceFingerprint: 'sha256:source', timelineTick: 42, graphVersion: 'graph-v1', tapId: 'post',
    colorDescriptorId: 'straight-rgba8-encoded-srgb' as const, width: 1, height: 1,
  }

  it('returns only selected typed-array buffers as transferables', async () => {
    const messages: VideoScopesWorkerResponse[] = []
    const transfers: Transferable[][] = []
    const handler = createVideoScopesWorkerHandler({
      postMessage(message, transfer = []) { messages.push(message); transfers.push(transfer) },
    })
    const rgba = new Uint8ClampedArray([255, 0, 0, 255])
    await handler({
      type: 'analyze', schemaVersion: 1, requestId: 'request-7', generation: 7, frameIdentity,
      rate: 'paused', scopes: ['histogram', 'vectorscope'], rgba: rgba.buffer,
    })
    expect(messages).toHaveLength(1)
    const response = messages[0]!
    expect(response.type).toBe('result')
    if (response.type !== 'result') throw new Error('expected result')
    expect(response.generation).toBe(7)
    expect(response.requestId).toBe('request-7')
    expect(response.frameIdentity).toEqual(frameIdentity)
    expect(response.result.histogram).toBeInstanceOf(Uint32Array)
    expect(response.result.waveform).toBeUndefined()
    expect(response.result.vectorscope).toBeInstanceOf(Uint32Array)
    expect(transfers[0]).toEqual([response.result.histogram!.buffer, response.result.vectorscope!.buffer])
  })

  it('cancels an in-flight generation at a 32-row yield', async () => {
    const messages: VideoScopesWorkerResponse[] = []
    const handler = createVideoScopesWorkerHandler({ postMessage(message) { messages.push(message) } })
    const analysis = handler({
      type: 'analyze', schemaVersion: 1, requestId: 'request-11', generation: 11,
      frameIdentity: { ...frameIdentity, height: 96 },
      rate: 'paused', scopes: ['histogram'], rgba: new Uint8ClampedArray(96 * 4).buffer,
    })
    await new Promise(resolve => setTimeout(resolve, 0))
    await handler({ type: 'cancel', schemaVersion: 1, requestId: 'request-11', generation: 11 })
    await analysis
    expect(messages).toEqual([{
      type: 'cancelled', schemaVersion: 1, requestId: 'request-11', generation: 11,
      frameIdentity: { ...frameIdentity, height: 96 },
    }])
  })

  it('reports validation errors with their generation', async () => {
    const messages: VideoScopesWorkerResponse[] = []
    const handler = createVideoScopesWorkerHandler({ postMessage(message) { messages.push(message) } })
    await handler({
      type: 'analyze', schemaVersion: 1, requestId: 'request-13', generation: 13,
      frameIdentity: { ...frameIdentity, width: 2, height: 2 },
      rate: 'live', scopes: ['waveform'], rgba: new ArrayBuffer(4),
    })
    expect(messages[0]).toMatchObject({ type: 'error', schemaVersion: 1, requestId: 'request-13', generation: 13 })
  })

  it('validates schema, descriptor, and identity dimensions', async () => {
    const messages: VideoScopesWorkerResponse[] = []
    const handler = createVideoScopesWorkerHandler({ postMessage(message) { messages.push(message) } })
    await handler({
      type: 'analyze', schemaVersion: 1, requestId: 'bad-descriptor', generation: 1,
      frameIdentity: { ...frameIdentity, colorDescriptorId: 'linear-srgb' as never },
      rate: 'live', scopes: [], rgba: new ArrayBuffer(4),
    })
    expect(messages[0]).toMatchObject({ type: 'error', requestId: 'bad-descriptor', frameIdentity: { width: 1, height: 1 } })
  })

  it('ignores late cancellation keys after a request has completed', async () => {
    const messages: VideoScopesWorkerResponse[] = []
    const handler = createVideoScopesWorkerHandler({ postMessage(message) { messages.push(message) } })
    const analyze = () => handler({
      type: 'analyze' as const, schemaVersion: 1 as const, requestId: 'reused', generation: 5,
      frameIdentity, rate: 'paused' as const, scopes: ['histogram' as const],
      rgba: new Uint8ClampedArray([0, 0, 0, 255]).buffer,
    })
    await analyze()
    await handler({ type: 'cancel', schemaVersion: 1, requestId: 'reused', generation: 5 })
    await analyze()
    expect(messages.map(message => message.type)).toEqual(['result', 'result'])
  })
})
