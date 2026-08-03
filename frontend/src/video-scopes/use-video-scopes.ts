import { videoScopesController, type VideoScopeAnalyzerTransport, type VideoScopeKind } from './controller'
import type { ScopeTapId } from './frame-broker'
import { VideoScopesWorkerTransport } from './worker-transport'

let defaultTransportAttached = false

/** Component-facing singleton. Scope preferences are UI-only and never enter EditState. */
export function useVideoScopes() {
  if (!defaultTransportAttached && typeof Worker !== 'undefined') {
    try {
      const transport = new VideoScopesWorkerTransport()
      videoScopesController.attachTransport(transport)
      defaultTransportAttached = true
    } catch (error) {
      videoScopesController.setRuntimeUnavailable(
        error instanceof Error ? error.message : 'Video Scopes Worker недоступен.',
      )
    }
  } else if (!defaultTransportAttached) {
    videoScopesController.setRuntimeUnavailable('Web Worker недоступен для Video Scopes.')
  }
  return {
    state: videoScopesController.state,
    setEnabled: (value: boolean) => videoScopesController.setEnabled(value),
    setVisible: (value: boolean) => videoScopesController.setVisible(value),
    setTap: (value: ScopeTapId) => videoScopesController.setTap(value),
    setKinds: (value: VideoScopeKind[]) => videoScopesController.setKinds(value),
    attachTransport: (value: VideoScopeAnalyzerTransport) => videoScopesController.attachTransport(value),
  }
}

export type { ScopeAnalysisRequest, ScopeAnalysisResult, VideoScopeAnalyzerTransport, VideoScopeKind, VideoScopeStatus } from './controller'
export type { ScopeAccuracy, ScopeFrame, ScopeFrameIdentity, ScopeSourceMode, ScopeTapId } from './frame-broker'
