import { describe, expect, it } from 'vitest'
import {
  SHORTCUT_SCHEMA_VERSION,
  assignShortcutBinding,
  defaultShortcutBindings,
  findShortcutCommand,
  keyboardEventToChord,
  parseShortcutSettings,
  serializeShortcutSettings,
} from './shortcuts'

function event(key: string, options: Partial<KeyboardEvent> = {}): KeyboardEvent {
  return { key, code: '', altKey: false, ctrlKey: false, metaKey: false, shiftKey: false, isComposing: false, ...options } as KeyboardEvent
}

describe('keyboard shortcut domain', () => {
  it('normalizes portable modifiers and dispatches only inside the active mode', () => {
    const bindings = defaultShortcutBindings()
    expect(keyboardEventToChord(event('z', { metaKey: true }))).toBe('Mod+KeyZ')
    expect(keyboardEventToChord(event('Z', { ctrlKey: true, shiftKey: true }))).toBe('Mod+Shift+KeyZ')
    expect(findShortcutCommand('composition', event('n'), bindings)).toBe('composition.toggleSnapping')
    expect(findShortcutCommand('legacy', event('n'), bindings)).toBeNull()
    expect(findShortcutCommand('legacy', event('ArrowLeft', { shiftKey: true }), bindings)).toBe('legacy.seekPreviousLarge')
  })

  it('detects same-mode conflicts without colliding across editor modes', () => {
    const bindings = defaultShortcutBindings()
    const conflict = assignShortcutBinding(bindings, 'composition.split', 'KeyN')
    expect(conflict.conflicts).toEqual(['composition.toggleSnapping'])
    expect(conflict.bindings).toEqual(bindings)

    const crossMode = assignShortcutBinding(bindings, 'legacy.timelineSplit', 'KeyN')
    expect(crossMode.conflicts).toEqual([])
    expect(crossMode.bindings['legacy.timelineSplit']).toBe('KeyN')
  })

  it('round-trips the versioned payload and rejects stale or conflicting data', () => {
    const changed = assignShortcutBinding(defaultShortcutBindings(), 'composition.toggleSnapping', 'KeyS').bindings
    const serialized = serializeShortcutSettings(changed)
    expect(JSON.parse(serialized)).toMatchObject({ schemaVersion: SHORTCUT_SCHEMA_VERSION })
    expect(parseShortcutSettings(serialized)).toEqual(changed)

    const partial = parseShortcutSettings(JSON.stringify({
      schemaVersion: SHORTCUT_SCHEMA_VERSION,
      bindings: { 'legacy.playPause': null },
    }))
    expect(partial['legacy.playPause']).toBeNull()
    expect(partial['composition.playPause']).toBe('Space')
    expect(() => parseShortcutSettings(JSON.stringify({ schemaVersion: 0, bindings: {} }))).toThrow('версии 1')
    expect(() => parseShortcutSettings(JSON.stringify({
      schemaVersion: SHORTCUT_SCHEMA_VERSION,
      bindings: { 'composition.split': 'KeyN' },
    }))).toThrow('конфликтуют')
  })
})
