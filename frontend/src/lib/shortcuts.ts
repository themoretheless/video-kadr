export const SHORTCUT_SCHEMA_VERSION = 1 as const
export const SHORTCUT_STORAGE_KEY = 'video-kadr:keyboard-shortcuts:v1'

export type ShortcutMode = 'legacy' | 'composition'
export type ShortcutGroup = 'playback' | 'history' | 'timeline'

export type ShortcutCommandId =
  | 'legacy.playPause'
  | 'legacy.framePrevious'
  | 'legacy.frameNext'
  | 'legacy.seekPrevious'
  | 'legacy.seekNext'
  | 'legacy.seekPreviousLarge'
  | 'legacy.seekNextLarge'
  | 'legacy.trimStart'
  | 'legacy.trimEnd'
  | 'legacy.undo'
  | 'legacy.redo'
  | 'legacy.timelineSplit'
  | 'legacy.timelineDuplicate'
  | 'legacy.timelineDelete'
  | 'legacy.timelineMovePrevious'
  | 'legacy.timelineMoveNext'
  | 'composition.playPause'
  | 'composition.framePrevious'
  | 'composition.frameNext'
  | 'composition.seekPrevious'
  | 'composition.seekNext'
  | 'composition.seekPreviousLarge'
  | 'composition.seekNextLarge'
  | 'composition.undo'
  | 'composition.redo'
  | 'composition.split'
  | 'composition.duplicate'
  | 'composition.delete'
  | 'composition.toggleSnapping'
  | 'composition.nudgePrevious'
  | 'composition.nudgeNext'
  | 'composition.trimStart'
  | 'composition.trimEnd'
  | 'composition.trackUp'
  | 'composition.trackDown'

export type ShortcutChord = string
export type ShortcutBindings = Record<ShortcutCommandId, ShortcutChord | null>

export interface ShortcutDefinition {
  id: ShortcutCommandId
  mode: ShortcutMode
  group: ShortcutGroup
  label: string
  description: string
  defaultBinding: ShortcutChord
}

export interface ShortcutConflict {
  commandId: ShortcutCommandId
  conflictingCommandId: ShortcutCommandId
  chord: ShortcutChord
}

export interface StoredShortcutSettings {
  schemaVersion: typeof SHORTCUT_SCHEMA_VERSION
  bindings: Partial<Record<ShortcutCommandId, ShortcutChord | null>>
}

export const SHORTCUT_DEFINITIONS = [
  shortcut('legacy.playPause', 'legacy', 'playback', 'Воспроизведение / пауза', 'Переключить воспроизведение', 'Space'),
  shortcut('legacy.framePrevious', 'legacy', 'playback', 'Предыдущий кадр', 'Шаг назад ровно на один кадр', 'Comma'),
  shortcut('legacy.frameNext', 'legacy', 'playback', 'Следующий кадр', 'Шаг вперёд ровно на один кадр', 'Period'),
  shortcut('legacy.seekPrevious', 'legacy', 'playback', 'Назад', 'Перейти на 1 секунду назад', 'ArrowLeft'),
  shortcut('legacy.seekNext', 'legacy', 'playback', 'Вперёд', 'Перейти на 1 секунду вперёд', 'ArrowRight'),
  shortcut('legacy.seekPreviousLarge', 'legacy', 'playback', 'Назад на 5 секунд', 'Большой шаг назад', 'Shift+ArrowLeft'),
  shortcut('legacy.seekNextLarge', 'legacy', 'playback', 'Вперёд на 5 секунд', 'Большой шаг вперёд', 'Shift+ArrowRight'),
  shortcut('legacy.trimStart', 'legacy', 'timeline', 'Начало от позиции', 'Поставить начало обрезки на плейхед', 'KeyI'),
  shortcut('legacy.trimEnd', 'legacy', 'timeline', 'Конец от позиции', 'Поставить конец обрезки на плейхед', 'KeyO'),
  shortcut('legacy.undo', 'legacy', 'history', 'Отменить', 'Отменить последнее изменение', 'Mod+KeyZ'),
  shortcut('legacy.redo', 'legacy', 'history', 'Повторить', 'Вернуть отменённое изменение', 'Mod+Shift+KeyZ'),
  shortcut('legacy.timelineSplit', 'legacy', 'timeline', 'Разделить фрагмент', 'Разделить выбранный фрагмент на плейхеде', 'KeyB'),
  shortcut('legacy.timelineDuplicate', 'legacy', 'timeline', 'Дублировать фрагмент', 'Дублировать выбранный фрагмент', 'Mod+KeyD'),
  shortcut('legacy.timelineDelete', 'legacy', 'timeline', 'Удалить фрагмент', 'Удалить выбранный фрагмент', 'Delete'),
  shortcut('legacy.timelineMovePrevious', 'legacy', 'timeline', 'Фрагмент левее', 'Переместить выбранный фрагмент раньше', 'Alt+ArrowLeft'),
  shortcut('legacy.timelineMoveNext', 'legacy', 'timeline', 'Фрагмент правее', 'Переместить выбранный фрагмент позже', 'Alt+ArrowRight'),
  shortcut('composition.playPause', 'composition', 'playback', 'Воспроизведение / пауза', 'Переключить воспроизведение', 'Space'),
  shortcut('composition.framePrevious', 'composition', 'playback', 'Предыдущий кадр', 'Шаг назад ровно на один кадр холста', 'Comma'),
  shortcut('composition.frameNext', 'composition', 'playback', 'Следующий кадр', 'Шаг вперёд ровно на один кадр холста', 'Period'),
  shortcut('composition.seekPrevious', 'composition', 'playback', 'Назад', 'Перейти на 0,1 секунды назад', 'ArrowLeft'),
  shortcut('composition.seekNext', 'composition', 'playback', 'Вперёд', 'Перейти на 0,1 секунды вперёд', 'ArrowRight'),
  shortcut('composition.seekPreviousLarge', 'composition', 'playback', 'Назад на 5 секунд', 'Большой шаг назад', 'Shift+ArrowLeft'),
  shortcut('composition.seekNextLarge', 'composition', 'playback', 'Вперёд на 5 секунд', 'Большой шаг вперёд', 'Shift+ArrowRight'),
  shortcut('composition.undo', 'composition', 'history', 'Отменить', 'Отменить последнее изменение', 'Mod+KeyZ'),
  shortcut('composition.redo', 'composition', 'history', 'Повторить', 'Вернуть отменённое изменение', 'Mod+Shift+KeyZ'),
  shortcut('composition.split', 'composition', 'timeline', 'Разрезать клип', 'Разрезать выбранный клип на плейхеде', 'KeyB'),
  shortcut('composition.duplicate', 'composition', 'timeline', 'Дублировать клип', 'Дублировать выбранный клип', 'Mod+KeyD'),
  shortcut('composition.delete', 'composition', 'timeline', 'Удалить клип', 'Удалить выбранный клип', 'Delete'),
  shortcut('composition.toggleSnapping', 'composition', 'timeline', 'Переключить магнит', 'Включить или выключить snapping', 'KeyN'),
  shortcut('composition.nudgePrevious', 'composition', 'timeline', 'Клип на кадр раньше', 'Сдвинуть выбранный клип на один кадр назад', 'Alt+Comma'),
  shortcut('composition.nudgeNext', 'composition', 'timeline', 'Клип на кадр позже', 'Сдвинуть выбранный клип на один кадр вперёд', 'Alt+Period'),
  shortcut('composition.trimStart', 'composition', 'timeline', 'Обрезать начало', 'Обрезать начало выбранного клипа по плейхеду', 'BracketLeft'),
  shortcut('composition.trimEnd', 'composition', 'timeline', 'Обрезать конец', 'Обрезать конец выбранного клипа по плейхеду', 'BracketRight'),
  shortcut('composition.trackUp', 'composition', 'timeline', 'Дорожка выше', 'Переместить выбранную дорожку выше', 'Alt+ArrowUp'),
  shortcut('composition.trackDown', 'composition', 'timeline', 'Дорожка ниже', 'Переместить выбранную дорожку ниже', 'Alt+ArrowDown'),
] as const satisfies readonly ShortcutDefinition[]

const definitionById = new Map<ShortcutCommandId, ShortcutDefinition>(
  SHORTCUT_DEFINITIONS.map((definition) => [definition.id, definition]),
)

export const DEFAULT_SHORTCUT_BINDINGS = Object.freeze(Object.fromEntries(
  SHORTCUT_DEFINITIONS.map((definition) => [definition.id, definition.defaultBinding]),
) as ShortcutBindings)

export function shortcutDefinition(commandId: ShortcutCommandId): ShortcutDefinition {
  return definitionById.get(commandId)!
}

export function defaultShortcutBindings(): ShortcutBindings {
  return { ...DEFAULT_SHORTCUT_BINDINGS }
}

export function keyboardEventToChord(event: Pick<KeyboardEvent, 'altKey' | 'code' | 'ctrlKey' | 'isComposing' | 'key' | 'metaKey' | 'shiftKey'>): ShortcutChord | null {
  if (event.isComposing) return null
  const key = eventCode(event.code, event.key)
  if (!key) return null
  const parts: string[] = []
  if (event.metaKey || event.ctrlKey) parts.push('Mod')
  if (event.altKey) parts.push('Alt')
  if (event.shiftKey) parts.push('Shift')
  parts.push(key)
  return normalizeShortcutChord(parts.join('+'))
}

export function normalizeShortcutChord(value: string): ShortcutChord | null {
  if (typeof value !== 'string' || !value.trim() || value.length > 80) return null
  const tokens = value.split('+').map((token) => token.trim()).filter(Boolean)
  const modifiers = new Set<string>()
  let key = ''
  for (const token of tokens) {
    const modifier = canonicalModifier(token)
    if (modifier) {
      if (modifiers.has(modifier)) return null
      modifiers.add(modifier)
      continue
    }
    const candidate = canonicalKey(token)
    if (!candidate || key) return null
    key = candidate
  }
  if (!key) return null
  return [
    modifiers.has('Mod') ? 'Mod' : '',
    modifiers.has('Alt') ? 'Alt' : '',
    modifiers.has('Shift') ? 'Shift' : '',
    key,
  ].filter(Boolean).join('+')
}

export function findShortcutCommand(
  mode: ShortcutMode,
  event: Pick<KeyboardEvent, 'altKey' | 'code' | 'ctrlKey' | 'isComposing' | 'key' | 'metaKey' | 'shiftKey'>,
  bindings: ShortcutBindings,
): ShortcutCommandId | null {
  const chord = keyboardEventToChord(event)
  if (!chord) return null
  return SHORTCUT_DEFINITIONS.find((definition) =>
    definition.mode === mode && bindings[definition.id] === chord,
  )?.id ?? null
}

export function shortcutConflicts(
  commandId: ShortcutCommandId,
  chord: ShortcutChord | null,
  bindings: ShortcutBindings,
): ShortcutCommandId[] {
  if (!chord) return []
  const normalized = normalizeShortcutChord(chord)
  if (!normalized) return []
  const mode = shortcutDefinition(commandId).mode
  return SHORTCUT_DEFINITIONS
    .filter((definition) => definition.mode === mode && definition.id !== commandId && bindings[definition.id] === normalized)
    .map((definition) => definition.id)
}

export function findShortcutConflicts(bindings: ShortcutBindings): ShortcutConflict[] {
  const conflicts: ShortcutConflict[] = []
  for (const definition of SHORTCUT_DEFINITIONS) {
    const chord = bindings[definition.id]
    if (!chord) continue
    for (const conflictingCommandId of shortcutConflicts(definition.id, chord, bindings)) {
      if (definition.id < conflictingCommandId) {
        conflicts.push({ commandId: definition.id, conflictingCommandId, chord })
      }
    }
  }
  return conflicts
}

export function assignShortcutBinding(
  bindings: ShortcutBindings,
  commandId: ShortcutCommandId,
  chord: ShortcutChord | null,
): { bindings: ShortcutBindings; conflicts: ShortcutCommandId[] } {
  const normalized = chord === null ? null : normalizeShortcutChord(chord)
  if (chord !== null && !normalized) throw new Error('Некорректное сочетание клавиш')
  const conflicts = shortcutConflicts(commandId, normalized, bindings)
  if (conflicts.length) return { bindings: { ...bindings }, conflicts }
  return { bindings: { ...bindings, [commandId]: normalized }, conflicts: [] }
}

export function serializeShortcutSettings(bindings: ShortcutBindings): string {
  const conflicts = findShortcutConflicts(bindings)
  if (conflicts.length) throw new Error('Нельзя сохранить конфликтующие сочетания клавиш')
  const safeBindings: StoredShortcutSettings['bindings'] = {}
  for (const definition of SHORTCUT_DEFINITIONS) {
    const chord = bindings[definition.id]
    if (chord === null) safeBindings[definition.id] = null
    else {
      const normalized = normalizeShortcutChord(chord)
      if (!normalized) throw new Error(`Некорректное сочетание для «${definition.label}»`)
      safeBindings[definition.id] = normalized
    }
  }
  return JSON.stringify({ schemaVersion: SHORTCUT_SCHEMA_VERSION, bindings: safeBindings } satisfies StoredShortcutSettings)
}

export function parseShortcutSettings(serialized: string): ShortcutBindings {
  let value: unknown
  try {
    value = JSON.parse(serialized)
  } catch {
    throw new Error('Настройки сочетаний должны быть корректным JSON')
  }
  if (!isRecord(value) || value.schemaVersion !== SHORTCUT_SCHEMA_VERSION || !isRecord(value.bindings)) {
    throw new Error(`Поддерживается схема сочетаний версии ${SHORTCUT_SCHEMA_VERSION}`)
  }
  const bindings = defaultShortcutBindings()
  for (const definition of SHORTCUT_DEFINITIONS) {
    const stored = value.bindings[definition.id]
    if (stored === undefined) continue
    if (stored === null) {
      bindings[definition.id] = null
      continue
    }
    if (typeof stored !== 'string') throw new Error(`Некорректное сочетание для «${definition.label}»`)
    const normalized = normalizeShortcutChord(stored)
    if (!normalized) throw new Error(`Некорректное сочетание для «${definition.label}»`)
    bindings[definition.id] = normalized
  }
  if (findShortcutConflicts(bindings).length) throw new Error('Сохранённые сочетания конфликтуют друг с другом')
  return bindings
}

export function formatShortcutChord(chord: ShortcutChord | null, platform = currentPlatform()): string {
  if (!chord) return 'Не назначено'
  const mac = platform === 'mac'
  return chord.split('+').map((token) => {
    if (token === 'Mod') return mac ? '⌘' : 'Ctrl'
    if (token === 'Alt') return mac ? '⌥' : 'Alt'
    if (token === 'Shift') return mac ? '⇧' : 'Shift'
    return displayKey(token)
  }).join(mac ? '' : '+')
}

export function shortcutChordToAria(chord: ShortcutChord | null, platform = currentPlatform()): string | undefined {
  if (!chord) return undefined
  return chord.split('+').map((token) => {
    if (token === 'Mod') return platform === 'mac' ? 'Meta' : 'Control'
    if (token.startsWith('Key')) return token.slice(3).toLowerCase()
    if (token.startsWith('Digit')) return token.slice(5)
    if (token === 'Comma') return ','
    if (token === 'Period') return '.'
    if (token === 'BracketLeft') return '['
    if (token === 'BracketRight') return ']'
    return token
  }).join('+')
}

function shortcut(
  id: ShortcutCommandId,
  mode: ShortcutMode,
  group: ShortcutGroup,
  label: string,
  description: string,
  defaultBinding: ShortcutChord,
): ShortcutDefinition {
  return { id, mode, group, label, description, defaultBinding }
}

function canonicalModifier(value: string): 'Mod' | 'Alt' | 'Shift' | null {
  const token = value.toLowerCase()
  if (token === 'mod' || token === 'meta' || token === 'control' || token === 'ctrl' || token === 'cmd') return 'Mod'
  if (token === 'alt' || token === 'option') return 'Alt'
  if (token === 'shift') return 'Shift'
  return null
}

function canonicalKey(value: string): string | null {
  if (/^Key[A-Z]$/.test(value)) return value
  if (/^Digit[0-9]$/.test(value)) return value
  if (/^F(?:[1-9]|1[0-2])$/.test(value)) return value
  if (/^(?:ArrowLeft|ArrowRight|ArrowUp|ArrowDown|Space|Comma|Period|Delete|Backspace|BracketLeft|BracketRight|Slash|Semicolon|Quote|Minus|Equal|Home|End|PageUp|PageDown)$/.test(value)) return value
  if (/^[a-z]$/i.test(value)) return `Key${value.toUpperCase()}`
  if (/^[0-9]$/.test(value)) return `Digit${value}`
  return keyValueToCode(value)
}

function eventCode(code: string, key: string): string | null {
  if (/^(?:Key[A-Z]|Digit[0-9]|F(?:[1-9]|1[0-2])|ArrowLeft|ArrowRight|ArrowUp|ArrowDown|Space|Comma|Period|Delete|Backspace|BracketLeft|BracketRight|Slash|Semicolon|Quote|Minus|Equal|Home|End|PageUp|PageDown)$/.test(code)) {
    return code
  }
  return keyValueToCode(key)
}

function keyValueToCode(value: string): string | null {
  if (/^[a-z]$/i.test(value)) return `Key${value.toUpperCase()}`
  if (/^[0-9]$/.test(value)) return `Digit${value}`
  const aliases: Record<string, string> = {
    ' ': 'Space',
    Spacebar: 'Space',
    ',': 'Comma',
    '.': 'Period',
    '[': 'BracketLeft',
    ']': 'BracketRight',
    '/': 'Slash',
    ';': 'Semicolon',
    "'": 'Quote',
    '-': 'Minus',
    '=': 'Equal',
  }
  const candidate = aliases[value] ?? value
  return /^(?:F(?:[1-9]|1[0-2])|ArrowLeft|ArrowRight|ArrowUp|ArrowDown|Space|Comma|Period|Delete|Backspace|BracketLeft|BracketRight|Slash|Semicolon|Quote|Minus|Equal|Home|End|PageUp|PageDown)$/.test(candidate)
    ? candidate
    : null
}

function displayKey(value: string): string {
  if (value.startsWith('Key')) return value.slice(3)
  if (value.startsWith('Digit')) return value.slice(5)
  const labels: Record<string, string> = {
    Space: 'Space',
    Comma: ',',
    Period: '.',
    BracketLeft: '[',
    BracketRight: ']',
    ArrowLeft: '←',
    ArrowRight: '→',
    ArrowUp: '↑',
    ArrowDown: '↓',
    Delete: 'Delete',
    Backspace: 'Backspace',
  }
  return labels[value] ?? value
}

function currentPlatform(): 'mac' | 'other' {
  if (typeof navigator === 'undefined') return 'other'
  return /Mac|iPhone|iPad|iPod/i.test(navigator.platform) ? 'mac' : 'other'
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
