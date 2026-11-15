import { expect, it } from 'vitest'
import {
  defaultSceneDescriptionPreferences,
  readSceneDescriptionPreferences,
  setSceneDescriptionPreferences,
} from './sceneDescriptionPreferences.svelte.js'

it('round-trips scene description defaults and recovers from invalid stored settings', () => {
  const descriptor = Object.getOwnPropertyDescriptor(window, 'localStorage')
  const entries = new Map<string, string>()
  Object.defineProperty(window, 'localStorage', { configurable: true, value: {
    getItem: (key: string) => entries.get(key) ?? null,
    setItem: (key: string, value: string) => { entries.set(key, value) },
  } })
  try {
    expect(readSceneDescriptionPreferences()).toEqual(defaultSceneDescriptionPreferences)
    setSceneDescriptionPreferences({ focus: 'shot-type', language: 'en', fps: 4 })
    expect(readSceneDescriptionPreferences()).toEqual({ focus: 'shot-type', language: 'en', fps: 4 })
    entries.set('video-kadr.scene-description', '{"language":"pt-BR"}')
    expect(readSceneDescriptionPreferences().language).toBe('pt')
    entries.set('video-kadr.scene-description', '{"focus":"shot-type","language":"broken","fps":99}')
    expect(readSceneDescriptionPreferences()).toEqual({ focus: 'shot-type', language: 'ru', fps: 1 })
    entries.set('video-kadr.scene-description', 'malformed')
    expect(readSceneDescriptionPreferences()).toEqual(defaultSceneDescriptionPreferences)
  } finally {
    setSceneDescriptionPreferences(defaultSceneDescriptionPreferences)
    if (descriptor) Object.defineProperty(window, 'localStorage', descriptor)
    else Reflect.deleteProperty(window, 'localStorage')
  }
})
