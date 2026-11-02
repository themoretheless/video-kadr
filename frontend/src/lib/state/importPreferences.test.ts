import { beforeEach, afterEach, describe, expect, it } from 'vitest'
import { importPreferences, readAutoPairAudioOnImport, setAutoPairAudioOnImport } from './importPreferences.svelte.js'

const key = 'video-kadr.auto-pair-audio-on-import'
let originalStorage: PropertyDescriptor | undefined
beforeEach(() => {
  originalStorage = Object.getOwnPropertyDescriptor(window, 'localStorage')
  const values = new Map<string, string>()
  Object.defineProperty(window, 'localStorage', { configurable: true, value: {
    getItem: (name: string) => values.get(name) ?? null,
    setItem: (name: string, value: string) => { values.set(name, value) },
    removeItem: (name: string) => { values.delete(name) },
  } })
})
afterEach(() => { window.localStorage.removeItem(key); importPreferences.autoPairAudio = true; if (originalStorage) Object.defineProperty(window, 'localStorage', originalStorage); else Reflect.deleteProperty(window, 'localStorage') })

describe('import preference persistence', () => {
  it('defaults to enabled and persists an explicit opt-out independently of project history', () => {
    window.localStorage.removeItem(key)
    expect(readAutoPairAudioOnImport()).toBe(true)
    setAutoPairAudioOnImport(false)
    expect(importPreferences.autoPairAudio).toBe(false)
    expect(readAutoPairAudioOnImport()).toBe(false)
    setAutoPairAudioOnImport(true)
    expect(readAutoPairAudioOnImport()).toBe(true)
    window.localStorage.setItem(key, 'malformed')
    expect(readAutoPairAudioOnImport()).toBe(true)
  })
})
