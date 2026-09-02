import { normalizeShortcutChord, type ShortcutChord } from '../../shortcuts.js'

export type ToolId = 'select' | 'transform' | 'hand' | 'blade'

export interface ToolDescriptor {
  readonly id: ToolId
  readonly label: string
  readonly shortcut: ShortcutChord
  readonly cursor: string
  readonly supportsPointerCapture: boolean
}

export const TOOL_DESCRIPTORS = [
  { id: 'select', label: 'Выделение', shortcut: 'KeyV', cursor: 'default', supportsPointerCapture: true },
  { id: 'transform', label: 'Трансформация', shortcut: 'KeyT', cursor: 'move', supportsPointerCapture: true },
  { id: 'hand', label: 'Рука', shortcut: 'KeyH', cursor: 'grab', supportsPointerCapture: true },
  { id: 'blade', label: 'Лезвие', shortcut: 'KeyB', cursor: 'crosshair', supportsPointerCapture: false },
] as const satisfies readonly ToolDescriptor[]

export function validateToolRegistry(descriptors: readonly ToolDescriptor[] = TOOL_DESCRIPTORS): void {
  const ids = new Set<ToolId>()
  const shortcuts = new Set<string>()
  for (const descriptor of descriptors) {
    if (ids.has(descriptor.id)) throw new Error(`Duplicate tool id: ${descriptor.id}`)
    ids.add(descriptor.id)
    const shortcut = normalizeShortcutChord(descriptor.shortcut)
    if (!shortcut) throw new Error(`Invalid shortcut for tool ${descriptor.id}`)
    if (shortcuts.has(shortcut)) throw new Error(`Duplicate tool shortcut: ${shortcut}`)
    shortcuts.add(shortcut)
  }
}

validateToolRegistry()
