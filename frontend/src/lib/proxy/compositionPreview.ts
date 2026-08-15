import {
  detectProxyPlatform,
  formatProxyProfile,
  resolveProxyPlayback,
  type ProxyPlatform,
  type ProxyPreviewPreference,
} from './model.js'
import { isSafeProxyMediaUrl, type ProxyArtifact, type ProxyList } from './types.js'

export type CompositionPreviewSourceKind = 'video' | 'audio' | 'image'

/**
 * One currently addressable composition media source. Callers explicitly mark
 * whether it is active so proxy discovery never expands to the whole project.
 */
export interface CompositionPreviewSourceRequest {
  sourceId: string
  kind: CompositionPreviewSourceKind
  active: boolean
  originalUrl: string
  audibleSourceAudio: boolean
}

export interface CompositionPreviewMediaSelection {
  kind: 'original' | 'proxy'
  url: string
  artifact: ProxyArtifact | null
  fallbackReason: string
}

export interface CompositionPreviewResolutionOption {
  value: 'original' | string
  label: string
}

const SAFE_SOURCE_ID = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/

export function isActiveCompositionVideoSource(
  request: CompositionPreviewSourceRequest,
): boolean {
  return request.active
    && request.kind === 'video'
    && SAFE_SOURCE_ID.test(request.sourceId)
    && !!request.originalUrl
}

function listForSource(
  request: CompositionPreviewSourceRequest,
  list: ProxyList | null,
): ProxyList | null {
  return list?.sourceId === request.sourceId ? list : null
}

function artifactCanPlay(
  request: CompositionPreviewSourceRequest,
  artifact: ProxyArtifact,
  failedKey: string | null,
  platform: ProxyPlatform,
): boolean {
  return artifact.key !== failedKey
    && isSafeProxyMediaUrl(artifact.url, artifact.key, artifact.profile.codec)
    && (artifact.profile.codec !== 'prores_proxy' || platform === 'mac')
    && (!request.audibleSourceAudio || artifact.profile.includeAudio)
}

export function compositionPreviewResolutionOptions(
  request: CompositionPreviewSourceRequest,
  list: ProxyList | null,
  failedKey: string | null = null,
  platform: ProxyPlatform = detectProxyPlatform(),
): CompositionPreviewResolutionOption[] {
  const options: CompositionPreviewResolutionOption[] = [{
    value: 'original',
    label: 'Original · исходное разрешение',
  }]
  if (!isActiveCompositionVideoSource(request)) return options

  const safeList = listForSource(request, list)
  if (!safeList) return options
  for (const artifact of safeList.proxies) {
    if (!artifactCanPlay(request, artifact, failedKey, platform)) continue
    options.push({
      value: artifact.key,
      label: `Proxy · ${formatProxyProfile(artifact.profile)}`,
    })
  }
  return options
}

export function resolveCompositionPreviewMedia(
  request: CompositionPreviewSourceRequest,
  list: ProxyList | null,
  preference: ProxyPreviewPreference | undefined,
  failedKey: string | null = null,
  platform: ProxyPlatform = detectProxyPlatform(),
): CompositionPreviewMediaSelection | null {
  if (!isActiveCompositionVideoSource(request)) return null

  const playback = resolveProxyPlayback(
    request.originalUrl,
    listForSource(request, list),
    preference,
    failedKey,
    platform,
  )
  if (playback.kind === 'proxy' && request.audibleSourceAudio && !playback.artifact?.profile.includeAudio) {
    return {
      kind: 'original',
      url: request.originalUrl,
      artifact: null,
      fallbackReason: 'Proxy без звука — используется Original.',
    }
  }
  return playback
}
