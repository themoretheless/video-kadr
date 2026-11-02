const KEY = 'video-kadr.auto-pair-audio-on-import'
let sessionAutoPairAudio = true

export function readAutoPairAudioOnImport(): boolean {
  try { return typeof window === 'undefined' ? sessionAutoPairAudio : window.localStorage.getItem(KEY) !== 'false' } catch { return sessionAutoPairAudio }
}

export const importPreferences = $state({ autoPairAudio: readAutoPairAudioOnImport() })

export function setAutoPairAudioOnImport(enabled: boolean): void {
  sessionAutoPairAudio = enabled
  importPreferences.autoPairAudio = enabled
  try { window.localStorage.setItem(KEY, String(enabled)) } catch { /* Session preference still applies when storage is unavailable. */ }
}
