export type PlayerSourceKind = 'local' | 'progressive' | 'hls' | 'dash'

export interface PlayerSource {
  readonly kind: PlayerSourceKind
  readonly url: string
  readonly mimeType?: string
  readonly immutable: boolean
}

const SAME_ORIGIN_FILE_PREFIXES = ['/files/', 'blob:', 'data:']

export function normalizePlayerSource(url: string, mimeType?: string): PlayerSource {
  const normalizedUrl = url.trim()
  const normalizedMime = mimeType?.trim().toLowerCase() || undefined
  const lowerPath = normalizedUrl.split(/[?#]/, 1)[0]?.toLowerCase() ?? ''
  let kind: PlayerSourceKind = 'progressive'
  if (normalizedMime === 'application/vnd.apple.mpegurl' || lowerPath.endsWith('.m3u8')) kind = 'hls'
  else if (normalizedMime === 'application/dash+xml' || lowerPath.endsWith('.mpd')) kind = 'dash'
  else if (SAME_ORIGIN_FILE_PREFIXES.some((prefix) => normalizedUrl.startsWith(prefix))) kind = 'local'
  return {
    kind,
    url: normalizedUrl,
    mimeType: normalizedMime,
    immutable: kind === 'local',
  }
}

export type PlayerEvent =
  | { readonly type: 'source-ready'; readonly source: PlayerSource }
  | { readonly type: 'time'; readonly seconds: number }
  | { readonly type: 'buffering' }
  | { readonly type: 'playing' }
  | { readonly type: 'paused' }
  | { readonly type: 'ended' }
  | { readonly type: 'failed'; readonly message: string }

export function playerEventKey(event: PlayerEvent): string {
  if (event.type === 'source-ready') return `${event.type}:${event.source.kind}:${event.source.url}`
  if (event.type === 'time') return `${event.type}:${event.seconds.toFixed(3)}`
  if (event.type === 'failed') return `${event.type}:${event.message}`
  return event.type
}
