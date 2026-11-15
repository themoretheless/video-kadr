const KEY = 'video-kadr.scene-description'

export interface SceneDescriptionPreferences {
  focus: 'general' | 'shot-type'
  language: 'ru' | 'en'
  fps: number
}

export const defaultSceneDescriptionPreferences: Readonly<SceneDescriptionPreferences> = {
  focus: 'general',
  language: 'ru',
  fps: 1,
}

export function readSceneDescriptionPreferences(): SceneDescriptionPreferences {
  try {
    const stored = typeof window === 'undefined' ? null : window.localStorage.getItem(KEY)
    const value = stored ? JSON.parse(stored) as Partial<SceneDescriptionPreferences> | null : null
    const fps = value?.fps
    return {
      focus: value?.focus === 'shot-type' ? 'shot-type' : 'general',
      language: value?.language === 'en' ? 'en' : 'ru',
      fps: typeof fps === 'number' && Number.isFinite(fps) && fps >= 0.001 && fps <= 30
        ? fps : defaultSceneDescriptionPreferences.fps,
    }
  } catch {
    return { ...defaultSceneDescriptionPreferences }
  }
}

export const sceneDescriptionPreferences = $state(readSceneDescriptionPreferences())

export function setSceneDescriptionPreferences(patch: Partial<SceneDescriptionPreferences>): void {
  Object.assign(sceneDescriptionPreferences, patch)
  try {
    window.localStorage.setItem(KEY, JSON.stringify(sceneDescriptionPreferences))
  } catch {
    // The preference still applies to this editor when storage is unavailable.
  }
}
