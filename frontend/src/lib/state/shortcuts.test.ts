import { beforeEach, describe, expect, it } from 'vitest'
import { SHORTCUT_SCHEMA_VERSION, SHORTCUT_STORAGE_KEY } from '../shortcuts'
import {
  reloadShortcutBindings,
  resetShortcutBindings,
  setShortcutBinding,
  setShortcutStorageForTests,
  shortcutState,
} from './shortcuts.svelte.js'

class MemoryStorage implements Storage {
  private readonly values = new Map<string, string>()
  get length(): number { return this.values.size }
  clear(): void { this.values.clear() }
  getItem(key: string): string | null { return this.values.get(key) ?? null }
  key(index: number): string | null { return [...this.values.keys()][index] ?? null }
  removeItem(key: string): void { this.values.delete(key) }
  setItem(key: string, value: string): void { this.values.set(key, value) }
}

let storage: MemoryStorage

beforeEach(() => {
  storage = new MemoryStorage()
  setShortcutStorageForTests(storage)
  resetShortcutBindings()
})

describe('shortcut settings state', () => {
  it('persists, reloads and resets versioned bindings', () => {
    expect(setShortcutBinding('composition.toggleSnapping', 'KeyS')).toEqual({ ok: true, conflicts: [] })
    const stored = JSON.parse(storage.getItem(SHORTCUT_STORAGE_KEY)!)
    expect(stored.schemaVersion).toBe(SHORTCUT_SCHEMA_VERSION)
    expect(stored.bindings['composition.toggleSnapping']).toBe('KeyS')

    shortcutState.bindings['composition.toggleSnapping'] = 'KeyX'
    reloadShortcutBindings()
    expect(shortcutState.bindings['composition.toggleSnapping']).toBe('KeyS')

    resetShortcutBindings()
    expect(shortcutState.bindings['composition.toggleSnapping']).toBe('KeyN')
    expect(JSON.parse(storage.getItem(SHORTCUT_STORAGE_KEY)!).bindings['composition.toggleSnapping']).toBe('KeyN')
  })

  it('keeps the old mapping and reports the conflicting command', () => {
    const before = storage.getItem(SHORTCUT_STORAGE_KEY)
    expect(setShortcutBinding('composition.split', 'KeyN')).toEqual({
      ok: false,
      conflicts: ['composition.toggleSnapping'],
    })
    expect(shortcutState.bindings['composition.split']).toBe('KeyB')
    expect(storage.getItem(SHORTCUT_STORAGE_KEY)).toBe(before)
    expect(shortcutState.error).toContain('Переключить магнит')
  })

  it('falls back to defaults when persisted schema is stale', () => {
    storage.setItem(SHORTCUT_STORAGE_KEY, JSON.stringify({ schemaVersion: 0, bindings: {} }))
    reloadShortcutBindings()
    expect(shortcutState.bindings['legacy.playPause']).toBe('Space')
    expect(shortcutState.error).toContain('сброшены в памяти')
  })
})
