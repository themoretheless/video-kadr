import { detectProxyPlatform, type ProxyPlatform } from './model.js'
import {
  compositionPreviewResolutionOptions,
  isActiveCompositionVideoSource,
  resolveCompositionPreviewMedia,
  type CompositionPreviewMediaSelection,
  type CompositionPreviewSourceRequest,
} from './compositionPreview.js'
import {
  proxyController,
  type ProxyController,
  type ProxyControllerState,
  type ProxySourceUiState,
} from './state.svelte.js'

export interface CompositionPreviewProxyControllerPort {
  readonly state: ProxyControllerState
  ensure(sourceId: string): void
  setOriginal(sourceId: string): void
  setProxy(sourceId: string, key: string): void
  markPlaybackFailed(sourceId: string, key: string): void
}

export function createCompositionPreviewProxyAdapter(
  controller: CompositionPreviewProxyControllerPort,
  platform: ProxyPlatform = detectProxyPlatform(),
) {
  function ensure(request: CompositionPreviewSourceRequest): void {
    if (isActiveCompositionVideoSource(request)) controller.ensure(request.sourceId)
  }

  function sourceState(request: CompositionPreviewSourceRequest): ProxySourceUiState | undefined {
    return controller.state.sources[request.sourceId]
  }

  function resolve(request: CompositionPreviewSourceRequest): CompositionPreviewMediaSelection | null {
    const current = sourceState(request)
    return resolveCompositionPreviewMedia(
      request,
      current?.list ?? null,
      controller.state.preferences[request.sourceId],
      current?.failedKey ?? null,
      platform,
    )
  }

  function resolutions(request: CompositionPreviewSourceRequest) {
    const current = sourceState(request)
    return compositionPreviewResolutionOptions(
      request,
      current?.list ?? null,
      current?.failedKey ?? null,
      platform,
    )
  }

  function selectResolution(request: CompositionPreviewSourceRequest, value: string): void {
    if (!isActiveCompositionVideoSource(request)) return
    if (value === 'original') {
      controller.setOriginal(request.sourceId)
      return
    }
    const selected = resolutions(request).find((option) => option.value === value)
    if (selected) controller.setProxy(request.sourceId, selected.value)
  }

  function markMediaFailed(
    request: CompositionPreviewSourceRequest,
    selection: CompositionPreviewMediaSelection | null,
  ): void {
    if (
      !isActiveCompositionVideoSource(request)
      || !selection
      || selection.kind !== 'proxy'
      || !selection.artifact
    ) return
    controller.markPlaybackFailed(request.sourceId, selection.artifact.key)
  }

  return {
    ensure,
    resolve,
    resolutions,
    selectResolution,
    markMediaFailed,
  }
}

export type CompositionPreviewProxyAdapter = ReturnType<typeof createCompositionPreviewProxyAdapter>

export const compositionPreviewProxyAdapter = createCompositionPreviewProxyAdapter(
  proxyController satisfies ProxyController,
)
