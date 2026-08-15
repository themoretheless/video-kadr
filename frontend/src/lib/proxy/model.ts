import type { Capabilities } from '$lib/types.js'
import { isSafeProxyMediaUrl, type ProxyArtifact, type ProxyCodec, type ProxyList } from './types.js'

export type ProxyPlatform = 'mac' | 'other'

export interface ProxyCodecOption {
  codec: ProxyCodec
  label: string
  available: boolean
  reason?: string
}

export type ProxyPreviewPreference =
  | { mode: 'original' }
  | { mode: 'proxy'; key: string }

export type ProxyPreferences = Record<string, ProxyPreviewPreference>

export interface ProxyPlaybackSelection {
  kind: 'original' | 'proxy'
  url: string
  artifact: ProxyArtifact | null
  fallbackReason: string
}

export const PROXY_PREFERENCE_STORAGE_KEY = 'video-editor.proxy-preview.v1'
const MAX_PREFERENCES = 256
const MAX_SOURCE_ID_CHARS = 256
const KEY = /^[0-9a-f]{64}$/

export function detectProxyPlatform(): ProxyPlatform {
  if (typeof navigator === 'undefined') return 'other'
  return /Mac|iPhone|iPad|iPod/i.test(navigator.platform) ? 'mac' : 'other'
}

function availableCapability(
  capabilities: Capabilities | null,
  family: 'formats' | 'codecs',
  id: string,
): { available: boolean; reason?: string } {
  if (!capabilities) {
    return { available: false, reason: 'Поддержка не подтверждена сервером' }
  }
  const option = capabilities[family].find((candidate) => candidate.id === id)
  if (!option) return { available: false, reason: 'Сервер не объявил эту возможность' }
  return { available: option.available, ...(option.reason ? { reason: option.reason } : {}) }
}

export function proxyCodecOptions(
  capabilities: Capabilities | null,
  platform: ProxyPlatform = detectProxyPlatform(),
): ProxyCodecOption[] {
  const h264Codec = availableCapability(capabilities, 'codecs', 'h264')
  const mp4 = availableCapability(capabilities, 'formats', 'mp4')
  const h264Available = h264Codec.available && mp4.available
  const h264Reason = !h264Codec.available ? h264Codec.reason : !mp4.available ? mp4.reason : undefined

  const prores = availableCapability(capabilities, 'formats', 'prores')
  const proresAvailable = platform === 'mac' && prores.available
  const proresReason = platform !== 'mac'
    ? 'ProRes Proxy для браузерного просмотра доступен только на Apple-платформах'
    : prores.reason

  return [
    { codec: 'h264', label: 'H.264', available: h264Available, ...(h264Reason ? { reason: h264Reason } : {}) },
    {
      codec: 'prores_proxy',
      label: 'ProRes Proxy',
      available: proresAvailable,
      ...(proresReason ? { reason: proresReason } : {}),
    },
  ]
}

export function defaultProxyQuality(codec: ProxyCodec): number {
  return codec === 'h264' ? 28 : 0
}

export function formatProxyProfile(profile: { maxWidth: number; codec: ProxyCodec; includeAudio: boolean }): string {
  const codec = profile.codec === 'h264' ? 'H.264' : 'ProRes Proxy'
  return `${profile.maxWidth}px · ${codec}${profile.includeAudio ? ' · со звуком' : ' · без звука'}`
}

export function parseProxyPreferences(raw: string | null): ProxyPreferences {
  if (!raw || raw.length > 128 * 1024) return {}
  try {
    const value = JSON.parse(raw) as unknown
    if (!value || typeof value !== 'object' || Array.isArray(value)) return {}
    const result: ProxyPreferences = {}
    const entries = Object.entries(value as Record<string, unknown>)
    if (entries.length > MAX_PREFERENCES) return {}
    for (const [sourceId, preference] of entries) {
      if (!sourceId || sourceId.length > MAX_SOURCE_ID_CHARS || /[\0\r\n]/.test(sourceId)) return {}
      if (!preference || typeof preference !== 'object' || Array.isArray(preference)) return {}
      const body = preference as Record<string, unknown>
      const keys = Object.keys(body)
      if (body.mode === 'original' && keys.length === 1) result[sourceId] = { mode: 'original' }
      else if (body.mode === 'proxy' && keys.length === 2 && typeof body.key === 'string' && KEY.test(body.key)) {
        result[sourceId] = { mode: 'proxy', key: body.key }
      } else return {}
    }
    return result
  } catch {
    return {}
  }
}

export function serializeProxyPreferences(preferences: ProxyPreferences): string {
  const entries = Object.entries(preferences).slice(-MAX_PREFERENCES)
  return JSON.stringify(Object.fromEntries(entries))
}

export function resolveProxyPlayback(
  originalUrl: string,
  list: ProxyList | null,
  preference: ProxyPreviewPreference | undefined,
  failedKey: string | null = null,
  platform: ProxyPlatform = detectProxyPlatform(),
): ProxyPlaybackSelection {
  if (!preference || preference.mode === 'original') {
    return { kind: 'original', url: originalUrl, artifact: null, fallbackReason: '' }
  }
  const artifact = list?.proxies.find((candidate) => candidate.key === preference.key)
  if (!artifact) {
    return {
      kind: 'original',
      url: originalUrl,
      artifact: null,
      fallbackReason: list
        ? 'Выбранный proxy устарел, ещё не готов или удалён — используется оригинал.'
        : 'Проверяем выбранный proxy — пока используется оригинал.',
    }
  }
  if (artifact.profile.codec === 'prores_proxy' && platform !== 'mac') {
    return {
      kind: 'original',
      url: originalUrl,
      artifact: null,
      fallbackReason: 'ProRes Proxy не поддерживается для просмотра на этой платформе — используется оригинал.',
    }
  }
  if (artifact.key === failedKey || !isSafeProxyMediaUrl(artifact.url, artifact.key, artifact.profile.codec)) {
    return {
      kind: 'original',
      url: originalUrl,
      artifact: null,
      fallbackReason: 'Proxy не удалось воспроизвести — используется оригинал.',
    }
  }
  return { kind: 'proxy', url: artifact.url, artifact, fallbackReason: '' }
}
