import type { PlayerSource, PlayerSourceKind } from '$lib/adapters/player/sources.js'

export interface PlaybackCapability {
  readonly supported: boolean
  readonly source: PlayerSource
  readonly fallback?: PlayerSourceKind
  readonly reason?: string
}

export interface MediaCapabilityProbe {
  canPlayType(type: string): CanPlayTypeResult
}

const MIME_BY_KIND: Partial<Record<PlayerSourceKind, string>> = {
  hls: 'application/vnd.apple.mpegurl',
  dash: 'application/dash+xml',
}

export function resolvePlaybackCapability(
  source: PlayerSource,
  probe: MediaCapabilityProbe,
): PlaybackCapability {
  if (source.kind === 'local' || source.kind === 'progressive') return { supported: true, source }
  const mime = source.mimeType ?? MIME_BY_KIND[source.kind]
  const native = mime ? probe.canPlayType(mime) : ''
  if (native === 'probably' || native === 'maybe') return { supported: true, source }
  return {
    supported: false,
    source,
    fallback: 'progressive',
    reason: `${source.kind.toUpperCase()} не поддерживается текущим player adapter`,
  }
}
