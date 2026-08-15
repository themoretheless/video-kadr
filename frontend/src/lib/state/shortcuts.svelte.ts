import {
  SHORTCUT_STORAGE_KEY,
  assignShortcutBinding,
  defaultShortcutBindings,
  findShortcutCommand,
  formatShortcutChord,
  parseShortcutSettings,
  serializeShortcutSettings,
  shortcutChordToAria,
  shortcutDefinition,
  type ShortcutBindings,
  type ShortcutChord,
  type ShortcutCommandId,
  type ShortcutMode,
} from '../shortcuts'

let storageOverride: Storage | null | undefined
const restored = readShortcutBindings(getStorage())

export const shortcutState = $state({
  bindings: restored.bindings,
  dialogOpen: false,
  capturingCommandId: null as ShortcutCommandId | null,
  error: restored.error,
})

export interface ShortcutAssignment {
  ok: boolean
  conflicts: ShortcutCommandId[]
}

export function openShortcutSettings(): void {
  shortcutState.error = ''
  shortcutState.capturingCommandId = null
  shortcutState.dialogOpen = true
}

export function closeShortcutSettings(): void {
  shortcutState.capturingCommandId = null
  shortcutState.dialogOpen = false
}

export function beginShortcutCapture(commandId: ShortcutCommandId): void {
  shortcutState.error = ''
  shortcutState.capturingCommandId = commandId
}

export function cancelShortcutCapture(): void {
  shortcutState.capturingCommandId = null
}

export function setShortcutBinding(commandId: ShortcutCommandId, chord: ShortcutChord | null): ShortcutAssignment {
  const assigned = assignShortcutBinding(shortcutState.bindings, commandId, chord)
  if (assigned.conflicts.length) {
    const names = assigned.conflicts.map((id) => `«${shortcutDefinition(id).label}»`).join(', ')
    shortcutState.error = `Это сочетание уже назначено: ${names}`
    return { ok: false, conflicts: assigned.conflicts }
  }
  try {
    persistShortcutBindings(assigned.bindings)
    shortcutState.bindings = assigned.bindings
    shortcutState.capturingCommandId = null
    shortcutState.error = ''
    return { ok: true, conflicts: [] }
  } catch (error) {
    shortcutState.error = error instanceof Error ? error.message : String(error)
    return { ok: false, conflicts: [] }
  }
}

export function resetShortcutBindings(): void {
  const defaults = defaultShortcutBindings()
  persistShortcutBindings(defaults)
  shortcutState.bindings = defaults
  shortcutState.capturingCommandId = null
  shortcutState.error = ''
}

export function shortcutCommandForEvent(mode: ShortcutMode, event: KeyboardEvent): ShortcutCommandId | null {
  return findShortcutCommand(mode, event, shortcutState.bindings)
}

export function shortcutLabel(commandId: ShortcutCommandId): string {
  return formatShortcutChord(shortcutState.bindings[commandId])
}

export function shortcutAria(commandId: ShortcutCommandId): string | undefined {
  return shortcutChordToAria(shortcutState.bindings[commandId])
}

export function reloadShortcutBindings(): void {
  const restoredSettings = readShortcutBindings(getStorage())
  shortcutState.bindings = restoredSettings.bindings
  shortcutState.error = restoredSettings.error
  shortcutState.capturingCommandId = null
}

export function setShortcutStorageForTests(storage: Storage | null | undefined): void {
  storageOverride = storage
}

export function readShortcutBindings(storage: Storage | null): { bindings: ShortcutBindings; error: string } {
  const serialized = storage?.getItem(SHORTCUT_STORAGE_KEY)
  if (!serialized) return { bindings: defaultShortcutBindings(), error: '' }
  try {
    return { bindings: parseShortcutSettings(serialized), error: '' }
  } catch (error) {
    return {
      bindings: defaultShortcutBindings(),
      error: `Сохранённые сочетания сброшены в памяти: ${error instanceof Error ? error.message : String(error)}`,
    }
  }
}

function persistShortcutBindings(bindings: ShortcutBindings): void {
  const storage = getStorage()
  if (!storage) return
  try {
    storage.setItem(SHORTCUT_STORAGE_KEY, serializeShortcutSettings(bindings))
  } catch (error) {
    throw new Error(`Не удалось сохранить сочетания клавиш: ${error instanceof Error ? error.message : String(error)}`)
  }
}

function getStorage(): Storage | null {
  if (storageOverride !== undefined) return storageOverride
  if (typeof window === 'undefined') return null
  try {
    return window.localStorage
  } catch {
    return null
  }
}
