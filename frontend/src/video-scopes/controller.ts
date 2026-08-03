import { reactive, readonly } from 'vue'
import { videoScopeFrameBroker, type ScopeAccuracy, type ScopeFrame, type ScopeFrameIdentity, type ScopeTapId, type VideoScopeFrameBroker } from './frame-broker'

export type VideoScopeKind = 'histogram' | 'waveform' | 'parade' | 'vectorscope'
export type VideoScopeStatus = 'disabled' | 'waiting' | 'analyzing' | 'ready' | 'held' | 'unavailable' | 'error'

export interface ScopeAnalysisRequest {
  schemaVersion: 1
  requestId: string
  generation: number
  frameIdentity: ScopeFrameIdentity
  mode: 'paused' | 'live'
  maxSamples: 262144 | 65536
  scopes: VideoScopeKind[]
  rgba: Uint8ClampedArray
}

export interface ScopeAnalysisResult {
  schemaVersion: 1
  requestId: string
  generation: number
  frameIdentity: ScopeFrameIdentity
  stride: number
  sampleCount: number
  alphaWeight: number
  elapsedMs: number
  histogram?: Uint32Array
  waveform?: Uint32Array
  parade?: Uint32Array
  vectorscope?: Uint32Array
}

export interface VideoScopeAnalyzerTransport {
  ready?: Promise<void>
  analyze(request: ScopeAnalysisRequest, signal: AbortSignal): Promise<ScopeAnalysisResult>
  close?(): void
}

interface Pending { frame: ScopeFrame; generation: number }

const state = reactive({
  enabled: false,
  visible: true,
  tapId: 'post-grade' as ScopeTapId,
  kinds: ['histogram'] as VideoScopeKind[],
  status: 'disabled' as VideoScopeStatus,
  reason: '',
  result: null as ScopeAnalysisResult | null,
  frameIdentity: null as ScopeFrameIdentity | null,
  accuracy: null as ScopeAccuracy | null,
})

export class VideoScopesController {
  readonly state = readonly(state)
  private transport: VideoScopeAnalyzerTransport | null = null
  private unsubscribe: (() => void) | null = null
  private abort: AbortController | null = null
  private pending: Pending | null = null
  private generation = 0
  private requestSequence = 0
  private pausedTimer: ReturnType<typeof setTimeout> | null = null
  private lastLiveStartedAt = Number.NEGATIVE_INFINITY
  private runtimeUnavailableReason = ''
  private panelVisible = true
  private pageVisible = typeof document === 'undefined' || document.visibilityState !== 'hidden'
  private readonly onVisibilityChange = () => {
    this.pageVisible = document.visibilityState !== 'hidden'
    this.applyVisibility()
  }

  constructor(private broker: VideoScopeFrameBroker = videoScopeFrameBroker) {
    this.unsubscribe = broker.subscribe(event => {
      if (event.type === 'invalidate') {
        this.invalidate()
        this.clearAccepted()
        state.status = state.enabled ? 'waiting' : 'disabled'
        state.reason = ''
        return
      }
      if (event.type === 'unavailable') {
        if (event.unavailable.tapId === state.tapId) this.markUnavailable(event.unavailable.reason)
        return
      }
      if (event.frame.identity.tapId === state.tapId) this.acceptFrame(event.frame)
    })
    if (typeof document !== 'undefined') document.addEventListener('visibilitychange', this.onVisibilityChange)
  }

  attachTransport(transport: VideoScopeAnalyzerTransport): void {
    this.transport?.close?.()
    this.transport = transport
    this.runtimeUnavailableReason = ''
    this.invalidate()
    this.clearAccepted()
    state.status = state.enabled ? 'waiting' : 'disabled'
    void transport.ready?.catch(error => {
      if (this.transport !== transport) return
      this.setRuntimeUnavailable(error instanceof Error ? error.message : String(error))
    })
  }

  setEnabled(enabled: boolean): void {
    if (state.enabled === enabled) return
    state.enabled = enabled
    this.invalidate()
    this.clearAccepted()
    state.status = enabled ? (this.runtimeUnavailableReason ? 'unavailable' : 'waiting') : 'disabled'
    if (enabled && this.runtimeUnavailableReason) state.reason = this.runtimeUnavailableReason
    if (!enabled) state.reason = ''
  }

  setRuntimeUnavailable(reason: string): void {
    this.runtimeUnavailableReason = reason
    this.markUnavailable(reason)
  }

  setVisible(visible: boolean): void {
    this.panelVisible = visible
    this.applyVisibility()
  }

  setTap(tapId: ScopeTapId): void {
    if (state.tapId === tapId) return
    state.tapId = tapId
    this.invalidate()
    this.clearAccepted()
    if (state.enabled) state.status = 'waiting'
  }

  setKinds(kinds: VideoScopeKind[]): void {
    const canonical = [...new Set(kinds)].sort() as VideoScopeKind[]
    if (canonical.join('|') === state.kinds.join('|')) return
    state.kinds = canonical
    this.invalidate()
    this.clearAccepted()
    if (state.enabled) state.status = 'waiting'
  }

  invalidate(): void {
    this.generation++
    this.abort?.abort()
    this.pending = null
    if (this.pausedTimer) clearTimeout(this.pausedTimer)
    this.pausedTimer = null
  }

  close(): void {
    this.invalidate()
    this.unsubscribe?.()
    this.unsubscribe = null
    this.transport?.close?.()
    this.transport = null
    if (typeof document !== 'undefined') document.removeEventListener('visibilitychange', this.onVisibilityChange)
  }

  private clearAccepted(): void {
    state.result = null
    state.frameIdentity = null
    state.accuracy = null
  }

  private applyVisibility(): void {
    const visible = this.panelVisible && this.pageVisible
    if (state.visible === visible) return
    state.visible = visible
    if (!visible) this.invalidate()
    else if (state.enabled) state.status = 'waiting'
  }

  private markUnavailable(reason: string): void {
    if (!state.enabled) return
    this.invalidate()
    state.reason = reason
    if (state.result && state.accuracy === 'exact-paused') state.status = 'held'
    else {
      this.clearAccepted()
      state.status = 'unavailable'
    }
  }

  private acceptFrame(frame: ScopeFrame): void {
    if (!state.enabled || !state.visible || !this.transport) return
    const generation = ++this.generation
    this.abort?.abort()
    this.pending = { frame, generation }
    if (this.pausedTimer) clearTimeout(this.pausedTimer)
    this.pausedTimer = null
    if (frame.accuracy === 'exact-paused') {
      this.pausedTimer = setTimeout(() => { this.pausedTimer = null; this.startLatest() }, 50)
      return
    }
    this.scheduleLiveLatest()
  }

  private scheduleLiveLatest(): void {
    if (!this.pending || this.pending.frame.accuracy !== 'exact-live' || this.pausedTimer) return
    const remaining = 125 - (Date.now() - this.lastLiveStartedAt)
    if (remaining > 0) {
      this.pausedTimer = setTimeout(() => { this.pausedTimer = null; this.startLatest() }, remaining)
      return
    }
    this.startLatest()
  }

  private startLatest(): void {
    if (this.abort || !this.pending || !this.transport) return
    const pending = this.pending
    this.pending = null
    const controller = new AbortController()
    this.abort = controller
    if (pending.frame.accuracy === 'exact-live') this.lastLiveStartedAt = Date.now()
    state.status = 'analyzing'
    state.reason = ''
    const request: ScopeAnalysisRequest = {
      schemaVersion: 1,
      requestId: `scope-${++this.requestSequence}`,
      generation: pending.generation,
      frameIdentity: pending.frame.identity,
      mode: pending.frame.accuracy === 'exact-live' ? 'live' : 'paused',
      maxSamples: pending.frame.accuracy === 'exact-live' ? 65536 : 262144,
      scopes: [...state.kinds],
      rgba: pending.frame.rgba,
    }
    void this.transport.analyze(request, controller.signal).then(result => {
      if (controller.signal.aborted || request.generation !== this.generation || !sameIdentity(result.frameIdentity, request.frameIdentity)) return
      state.result = result
      state.frameIdentity = result.frameIdentity
      state.accuracy = request.mode === 'live' ? 'exact-live' : 'exact-paused'
      state.status = 'ready'
    }).catch(error => {
      if (!controller.signal.aborted && request.generation === this.generation) {
        state.reason = error instanceof Error ? error.message : String(error)
        state.status = 'error'
      }
    }).finally(() => {
      if (this.abort === controller) this.abort = null
      if (this.pending?.frame.accuracy === 'exact-live') this.scheduleLiveLatest()
      else if (this.pending && !this.pausedTimer) this.startLatest()
    })
  }
}

function sameIdentity(left: ScopeFrameIdentity, right: ScopeFrameIdentity): boolean {
  return left.sourceFingerprint === right.sourceFingerprint
    && left.timelineTick === right.timelineTick
    && left.graphVersion === right.graphVersion
    && left.tapId === right.tapId
    && left.colorDescriptorId === right.colorDescriptorId
    && left.width === right.width && left.height === right.height
    && left.sourceMode === right.sourceMode && left.mappingIdentity === right.mappingIdentity
}

export const videoScopesController = new VideoScopesController()
