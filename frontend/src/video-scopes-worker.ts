/// <reference lib="webworker" />

import {
  analyzeVideoScopes,
  VIDEO_SCOPES_SCHEMA_VERSION,
  VideoScopesCancelledError,
  videoScopesTransferBuffers,
  type VideoScopeKind,
  type VideoScopesRate,
  type VideoScopesResult,
} from './domain/video-scopes'

export interface VideoScopesFrameIdentity {
  sourceFingerprint: string
  timelineTick: number
  graphVersion: string
  tapId: string
  colorDescriptorId: 'straight-rgba8-encoded-srgb'
  width: number
  height: number
}

export interface VideoScopesWorkerAnalyze {
  type: 'analyze'
  schemaVersion: typeof VIDEO_SCOPES_SCHEMA_VERSION
  requestId: string
  generation: number
  frameIdentity: VideoScopesFrameIdentity
  rate: VideoScopesRate
  scopes: VideoScopeKind[]
  rgba: ArrayBuffer
}

export interface VideoScopesWorkerCancel {
  type: 'cancel'
  schemaVersion: typeof VIDEO_SCOPES_SCHEMA_VERSION
  requestId: string
  generation: number
}

export type VideoScopesWorkerRequest = VideoScopesWorkerAnalyze | VideoScopesWorkerCancel
export type VideoScopesWorkerResponse =
  | { type: 'ready'; schemaVersion: typeof VIDEO_SCOPES_SCHEMA_VERSION }
  | { type: 'result'; schemaVersion: typeof VIDEO_SCOPES_SCHEMA_VERSION; requestId: string; generation: number; frameIdentity: VideoScopesFrameIdentity; result: VideoScopesResult }
  | { type: 'cancelled'; schemaVersion: typeof VIDEO_SCOPES_SCHEMA_VERSION; requestId: string; generation: number; frameIdentity: VideoScopesFrameIdentity }
  | { type: 'error'; schemaVersion: typeof VIDEO_SCOPES_SCHEMA_VERSION; requestId: string; generation: number; frameIdentity: VideoScopesFrameIdentity; message: string }

export interface VideoScopesWorkerPort {
  postMessage(message: VideoScopesWorkerResponse, transfer?: Transferable[]): void
}

/** Installable handler kept separate from worker bootstrapping for deterministic tests. */
export function createVideoScopesWorkerHandler(port: VideoScopesWorkerPort) {
  const cancelled = new Set<string>()
  const active = new Set<string>()
  const key = (requestId: string, generation: number) => `${requestId}\u0000${generation}`
  return async (message: VideoScopesWorkerRequest): Promise<void> => {
    if (message.type === 'cancel') {
      const requestKey = key(message.requestId, message.generation)
      if (message.schemaVersion === VIDEO_SCOPES_SCHEMA_VERSION && active.has(requestKey)) {
        cancelled.add(requestKey)
      }
      return
    }
    const generation = message.generation
    const requestKey = key(message.requestId, generation)
    const identity = message.frameIdentity
    try {
      if (message.schemaVersion !== VIDEO_SCOPES_SCHEMA_VERSION) throw new RangeError('unsupported video scopes schema')
      active.add(requestKey)
      if (identity.colorDescriptorId !== 'straight-rgba8-encoded-srgb') throw new RangeError('unsupported color descriptor')
      if (!Number.isSafeInteger(identity.width) || identity.width <= 0
        || !Number.isSafeInteger(identity.height) || identity.height <= 0) throw new RangeError('invalid frame identity dimensions')
      const result = await analyzeVideoScopes({
        rgba: new Uint8ClampedArray(message.rgba),
        width: identity.width,
        height: identity.height,
        rate: message.rate,
        scopes: message.scopes,
      }, {
        isCancelled: () => cancelled.has(requestKey),
        yieldEvery32Rows: () => new Promise(resolve => setTimeout(resolve, 0)),
      })
      if (cancelled.delete(requestKey)) {
        port.postMessage({ type: 'cancelled', schemaVersion: VIDEO_SCOPES_SCHEMA_VERSION, requestId: message.requestId, generation, frameIdentity: identity })
        return
      }
      port.postMessage(
        { type: 'result', schemaVersion: VIDEO_SCOPES_SCHEMA_VERSION, requestId: message.requestId, generation, frameIdentity: identity, result },
        videoScopesTransferBuffers(result),
      )
    } catch (error) {
      if (error instanceof VideoScopesCancelledError || cancelled.delete(requestKey)) {
        port.postMessage({ type: 'cancelled', schemaVersion: VIDEO_SCOPES_SCHEMA_VERSION, requestId: message.requestId, generation, frameIdentity: identity })
      } else {
        port.postMessage({
          type: 'error',
          schemaVersion: VIDEO_SCOPES_SCHEMA_VERSION,
          requestId: message.requestId,
          generation,
          frameIdentity: identity,
          message: error instanceof Error ? error.message : String(error),
        })
      }
    } finally {
      active.delete(requestKey)
      cancelled.delete(requestKey)
    }
  }
}

const workerScope = globalThis as typeof globalThis & Partial<DedicatedWorkerGlobalScope>
if (typeof workerScope.postMessage === 'function' && typeof workerScope.document === 'undefined') {
  const handler = createVideoScopesWorkerHandler(workerScope as DedicatedWorkerGlobalScope)
  workerScope.onmessage = event => { void handler(event.data as VideoScopesWorkerRequest) }
  workerScope.postMessage({ type: 'ready', schemaVersion: VIDEO_SCOPES_SCHEMA_VERSION } satisfies VideoScopesWorkerResponse)
}
