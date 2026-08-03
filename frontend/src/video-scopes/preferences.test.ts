import { describe, expect, it } from 'vitest'
import {
  defaultVideoScopesPreferences,
  loadVideoScopesPreferences,
  sanitizeVideoScopesPreferences,
  saveVideoScopesPreferences,
  VIDEO_SCOPES_PREFERENCES_KEY,
} from './preferences'

describe('video scope UI preferences', () => {
  it('sanitizes the versioned shape, preserves canonical order, and clamps trace intensity', () => {
    expect(sanitizeVideoScopesPreferences({
      schemaVersion: 1,
      stage: 'source',
      visibleScopes: ['histogram', 'histogram', 'unknown', 'waveform'],
      intensity: 99,
    })).toEqual({
      schemaVersion: 1,
      stage: 'source',
      visibleScopes: ['waveform', 'histogram'],
      intensity: 3,
    })
    expect(sanitizeVideoScopesPreferences({ schemaVersion: 99 }))
      .toEqual(defaultVideoScopesPreferences())
  })

  it('loads and saves optional UI state without leaking storage failures', () => {
    const values = new Map<string, string>()
    const storage = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => { values.set(key, value) },
    }
    const preferences = {
      ...defaultVideoScopesPreferences(),
      stage: 'source' as const,
      visibleScopes: ['vectorscope'] as const,
      intensity: 1.5,
    }
    saveVideoScopesPreferences({ ...preferences, visibleScopes: [...preferences.visibleScopes] }, storage)
    expect(loadVideoScopesPreferences(storage)).toEqual(preferences)
    expect(JSON.parse(values.get(VIDEO_SCOPES_PREFERENCES_KEY)!)).not.toHaveProperty('edit')

    expect(() => saveVideoScopesPreferences(defaultVideoScopesPreferences(), {
      setItem: () => { throw new DOMException('quota', 'QuotaExceededError') },
    })).not.toThrow()
    expect(loadVideoScopesPreferences({ getItem: () => '{broken' }))
      .toEqual(defaultVideoScopesPreferences())
  })
})
