import type { VideoScopeKind } from '../domain/video-scopes'

export const VIDEO_SCOPES_PREFERENCES_VERSION = 1 as const
export const VIDEO_SCOPES_PREFERENCES_KEY = 've_video_scopes_preferences_v1'

export type VideoScopesStage = 'source' | 'post-effects'

export interface VideoScopesPreferences {
  schemaVersion: typeof VIDEO_SCOPES_PREFERENCES_VERSION
  stage: VideoScopesStage
  visibleScopes: VideoScopeKind[]
  /** Presentation-only trace gain. It never changes measured values. */
  intensity: number
}

export const VIDEO_SCOPE_KINDS: readonly VideoScopeKind[] = [
  'waveform',
  'parade',
  'vectorscope',
  'histogram',
]

export function defaultVideoScopesPreferences(): VideoScopesPreferences {
  return {
    schemaVersion: VIDEO_SCOPES_PREFERENCES_VERSION,
    stage: 'post-effects',
    visibleScopes: [...VIDEO_SCOPE_KINDS],
    intensity: 1,
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function sanitizeVideoScopesPreferences(value: unknown): VideoScopesPreferences {
  const defaults = defaultVideoScopesPreferences()
  if (!isRecord(value) || value.schemaVersion !== VIDEO_SCOPES_PREFERENCES_VERSION) return defaults

  const stage: VideoScopesStage = value.stage === 'source' || value.stage === 'post-effects'
    ? value.stage
    : defaults.stage
  const selected = Array.isArray(value.visibleScopes)
    ? value.visibleScopes.filter((scope): scope is VideoScopeKind =>
        typeof scope === 'string' && VIDEO_SCOPE_KINDS.includes(scope as VideoScopeKind),
      )
    : defaults.visibleScopes
  const visibleScopes = VIDEO_SCOPE_KINDS.filter(scope => selected.includes(scope))
  const intensity = typeof value.intensity === 'number' && Number.isFinite(value.intensity)
    ? Math.max(0.25, Math.min(3, value.intensity))
    : defaults.intensity

  return {
    schemaVersion: VIDEO_SCOPES_PREFERENCES_VERSION,
    stage,
    visibleScopes,
    intensity,
  }
}

export function loadVideoScopesPreferences(storage: Pick<Storage, 'getItem'> = localStorage): VideoScopesPreferences {
  try {
    const stored = storage.getItem(VIDEO_SCOPES_PREFERENCES_KEY)
    return stored ? sanitizeVideoScopesPreferences(JSON.parse(stored) as unknown) : defaultVideoScopesPreferences()
  } catch {
    return defaultVideoScopesPreferences()
  }
}

export function saveVideoScopesPreferences(
  value: VideoScopesPreferences,
  storage: Pick<Storage, 'setItem'> = localStorage,
): void {
  try {
    storage.setItem(VIDEO_SCOPES_PREFERENCES_KEY, JSON.stringify(sanitizeVideoScopesPreferences(value)))
  } catch {
    // Preferences are optional UI state; quota/private-mode failures are harmless.
  }
}
