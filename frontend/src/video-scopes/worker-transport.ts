import type { VideoScopesResult } from '../domain/video-scopes'
import type { VideoScopesFrameIdentity, VideoScopesWorkerResponse } from '../video-scopes-worker'
import type { ScopeAnalysisRequest, ScopeAnalysisResult, VideoScopeAnalyzerTransport } from './controller'

interface Pending {
  request: ScopeAnalysisRequest
  resolve: (value: ScopeAnalysisResult) => void
  reject: (reason: unknown) => void
  removeAbort: () => void
  aborted: boolean
}

export class VideoScopesWorkerTransport implements VideoScopeAnalyzerTransport {
  readonly ready: Promise<void>
  private worker: Worker
  private pending = new Map<string, Pending>()
  private resolveReady!: () => void
  private rejectReady!: (reason: unknown) => void
  private readyTimer: ReturnType<typeof setTimeout>

  constructor() {
    // Construct first: if the browser rejects Worker synchronously, no startup
    // timer or orphaned rejecting promise has been created.
    this.worker = new Worker(new URL('../video-scopes-worker.ts', import.meta.url), { type: 'module' })
    this.ready = new Promise<void>((resolve, reject) => {
      this.resolveReady = resolve
      this.rejectReady = reject
    })
    this.readyTimer = setTimeout(() => this.rejectReady(new Error('Video Scopes worker startup timeout')), 5_000)
    this.worker.addEventListener('message', event => this.onMessage(event.data as VideoScopesWorkerResponse))
    this.worker.addEventListener('error', event => {
      const error = event.error ?? new Error(event.message)
      clearTimeout(this.readyTimer)
      this.rejectReady(error)
      this.failAll(error)
    })
  }

  analyze(request: ScopeAnalysisRequest, signal: AbortSignal): Promise<ScopeAnalysisResult> {
    return new Promise((resolve, reject) => {
      const cancel = () => {
        const pending = this.pending.get(request.requestId)
        if (pending) pending.aborted = true
        this.worker.postMessage({
          type: 'cancel', schemaVersion: 1, requestId: request.requestId, generation: request.generation,
        })
      }
      if (signal.aborted) { reject(new DOMException('Scope analysis cancelled', 'AbortError')); return }
      signal.addEventListener('abort', cancel, { once: true })
      this.pending.set(request.requestId, {
        request, resolve, reject,
        removeAbort: () => signal.removeEventListener('abort', cancel),
        aborted: false,
      })
      const rgba = request.rgba.buffer.slice(
        request.rgba.byteOffset,
        request.rgba.byteOffset + request.rgba.byteLength,
      )
      this.worker.postMessage({
        type: 'analyze', schemaVersion: 1, requestId: request.requestId,
        generation: request.generation, frameIdentity: request.frameIdentity,
        rate: request.mode, scopes: request.scopes, rgba,
      }, [rgba])
    })
  }

  close(): void {
    clearTimeout(this.readyTimer)
    this.worker.terminate()
    this.failAll(new DOMException('Scope worker closed', 'AbortError'))
  }

  private onMessage(message: VideoScopesWorkerResponse): void {
    if (message.type === 'ready') {
      if (message.schemaVersion !== 1) {
        const error = new Error('Unsupported Video Scopes worker schema')
        clearTimeout(this.readyTimer)
        this.rejectReady(error)
        this.failAll(error)
        return
      }
      clearTimeout(this.readyTimer)
      this.resolveReady()
      return
    }
    const pending = this.pending.get(message.requestId)
    if (!pending || message.generation !== pending.request.generation) return
    if (pending.aborted) {
      this.finish(message.requestId, () => pending.reject(new DOMException('Scope analysis cancelled', 'AbortError')))
      return
    }
    if (message.schemaVersion !== 1 || !sameWorkerIdentity(message.frameIdentity, pending.request.frameIdentity)) {
      this.finish(message.requestId, () => pending.reject(new Error('Video Scopes worker returned mismatched frame provenance')))
      return
    }
    if (message.type === 'result') {
      const resultValid = message.result.schemaVersion === 1
        && message.result.descriptor === 'straight-rgba8-encoded-srgb'
        && message.result.sourceWidth === message.frameIdentity.width
        && message.result.sourceHeight === message.frameIdentity.height
      if (!resultValid) {
        this.finish(message.requestId, () => pending.reject(new Error('Video Scopes worker returned invalid result metadata')))
        return
      }
      this.finish(message.requestId, () => pending.resolve(toControllerResult(pending.request, pending.request.frameIdentity, message.result)))
    } else if (message.type === 'error') {
      this.finish(message.requestId, () => pending.reject(new Error(message.message)))
    } else {
      this.finish(message.requestId, () => pending.reject(new DOMException('Scope analysis cancelled', 'AbortError')))
    }
  }

  private finish(requestId: string, settle: () => void): void {
    const pending = this.pending.get(requestId)
    if (!pending) return
    this.pending.delete(requestId)
    pending.removeAbort()
    settle()
  }

  private failAll(reason: unknown): void {
    for (const [requestId, pending] of this.pending) this.finish(requestId, () => pending.reject(reason))
  }
}

function sameWorkerIdentity(
  actual: VideoScopesFrameIdentity,
  expected: ScopeAnalysisRequest['frameIdentity'],
): boolean {
  const candidate = actual as VideoScopesFrameIdentity & Partial<ScopeAnalysisRequest['frameIdentity']>
  return candidate.sourceFingerprint === expected.sourceFingerprint
    && candidate.timelineTick === expected.timelineTick
    && candidate.graphVersion === expected.graphVersion
    && candidate.tapId === expected.tapId
    && candidate.colorDescriptorId === expected.colorDescriptorId
    && candidate.width === expected.width
    && candidate.height === expected.height
    && candidate.sourceMode === expected.sourceMode
    && candidate.mappingIdentity === expected.mappingIdentity
}

function toControllerResult(
  request: ScopeAnalysisRequest,
  identity: ScopeAnalysisRequest['frameIdentity'],
  result: VideoScopesResult,
): ScopeAnalysisResult {
  return {
    schemaVersion: 1,
    requestId: request.requestId,
    generation: request.generation,
    frameIdentity: identity,
    stride: result.stride,
    sampleCount: result.sampledPixels,
    alphaWeight: result.alphaWeight,
    elapsedMs: 0,
    histogram: result.histogram,
    waveform: result.waveform,
    parade: result.parade,
    vectorscope: result.vectorscope,
  }
}
