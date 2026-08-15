import {
  createCompositionTemplate,
  instantiateCompositionTemplate,
  parseCompositionTemplate,
  serializeCompositionTemplate,
  type CompositionTemplate,
  type CompositionTemplateSlot,
  type TemplateReplacement,
} from '../composition/templates'
import {
  type Composition,
  type CompositionClip,
  type CompositionSource,
} from '../composition/types'
import { sourceSupportsClip } from '../composition/validation'
import {
  compositionState,
  openCompositionDocumentAsNew,
} from './composition.svelte'

const TEMPLATE_CATALOG_KEY = 'video-editor:composition-templates:v1'
export const MAX_TEMPLATE_CATALOG_ITEMS = 32
export const MAX_TEMPLATE_CATALOG_BYTES = 4 * 1024 * 1024

interface StoredTemplateCatalog {
  schemaVersion: 1
  templates: CompositionTemplate[]
}

let storageOverride: Storage | null | undefined

export const compositionTemplateState = $state({
  templates: loadCatalog(),
  selectedTemplateId: null as string | null,
  error: '',
  message: '',
})

export function createTemplateFromCurrentComposition(name: string): CompositionTemplate {
  const template = createCompositionTemplate(
    makeId('template'),
    name,
    compositionState.document,
    buildAutomaticSlots(compositionState.document),
  )
  saveTemplateToCatalog(template)
  compositionTemplateState.selectedTemplateId = template.id
  compositionTemplateState.message = `Шаблон «${template.name}» создан`
  return template
}

export function importTemplateToCatalog(serialized: string): CompositionTemplate {
  const template = parseCompositionTemplate(serialized)
  saveTemplateToCatalog(template)
  compositionTemplateState.selectedTemplateId = template.id
  compositionTemplateState.message = `Шаблон «${template.name}» импортирован`
  return template
}

export function exportTemplateFromCatalog(id: string): { filename: string; text: string } {
  const template = getTemplate(id)
  const base = template.name
    .replace(/[^\p{L}\p{N}._-]+/gu, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 80) || 'composition-template'
  return { filename: `${base}.composition-template.json`, text: serializeCompositionTemplate(template) }
}

export function instantiateCatalogTemplate(
  id: string,
  replacements: Readonly<Record<string, TemplateReplacement>>,
): Composition {
  const template = getTemplate(id)
  const document = instantiateCompositionTemplate(template, replacements)
  openCompositionDocumentAsNew(document, `${template.name} — копия`)
  compositionTemplateState.message = `Шаблон «${template.name}» применён как новая композиция`
  return document
}

export function deleteTemplateFromCatalog(id: string): void {
  const next = compositionTemplateState.templates.filter((template) => template.id !== id)
  if (next.length === compositionTemplateState.templates.length) return
  persistCatalog(next)
  compositionTemplateState.templates = next
  if (compositionTemplateState.selectedTemplateId === id) {
    compositionTemplateState.selectedTemplateId = next[0]?.id ?? null
  }
  compositionTemplateState.message = 'Шаблон удалён'
}

export function saveTemplateToCatalog(template: CompositionTemplate): void {
  const canonical = parseCompositionTemplate(serializeCompositionTemplate(template))
  const existing = compositionTemplateState.templates.findIndex((candidate) => candidate.id === canonical.id)
  const next = [...compositionTemplateState.templates]
  if (existing >= 0) next[existing] = canonical
  else next.push(canonical)
  next.sort((left, right) => left.name.localeCompare(right.name) || left.id.localeCompare(right.id))
  persistCatalog(next)
  compositionTemplateState.templates = next
  compositionTemplateState.error = ''
}

export function sourceCanReplaceTemplateSlot(
  template: CompositionTemplate,
  slot: CompositionTemplateSlot,
  source: CompositionSource,
): boolean {
  if (slot.kind !== 'media') return false
  const clip = findTemplateClip(template, slot.clipId)
  if (!clip || clip.kind === 'text' || !sourceSupportsClip(source, clip)) return false
  if (clip.kind === 'video' || clip.kind === 'audio') return source.durationTicks >= clip.sourceOutTicks
  return true
}

export function findTemplateClip(template: CompositionTemplate, clipId: string): CompositionClip | null {
  for (const track of template.composition.tracks) {
    const clip = track.clips.find((candidate) => candidate.id === clipId)
    if (clip) return clip
  }
  return null
}

export function resetCompositionTemplateCatalogForTests(templates: CompositionTemplate[] = []): void {
  const canonical = templates.map((template) => parseCompositionTemplate(serializeCompositionTemplate(template)))
  compositionTemplateState.templates = canonical
  compositionTemplateState.selectedTemplateId = canonical[0]?.id ?? null
  compositionTemplateState.error = ''
  compositionTemplateState.message = ''
}

export function reloadCompositionTemplateCatalog(): void {
  const templates = loadCatalog()
  compositionTemplateState.templates = templates
  compositionTemplateState.selectedTemplateId = templates[0]?.id ?? null
  compositionTemplateState.error = ''
}

export function setCompositionTemplateStorageForTests(storage: Storage | null | undefined): void {
  storageOverride = storage
}

function buildAutomaticSlots(document: Composition): CompositionTemplateSlot[] {
  const slots: CompositionTemplateSlot[] = []
  let index = 0
  for (const track of document.tracks) {
    for (const clip of track.clips) {
      index += 1
      slots.push({
        id: `slot-${String(index).padStart(3, '0')}`,
        kind: clip.kind === 'text' ? 'text' : 'media',
        label: clip.kind === 'text' ? `${track.name}: текст` : `${track.name}: медиа`,
        clipId: clip.id,
      })
    }
  }
  return slots
}

function getTemplate(id: string): CompositionTemplate {
  const template = compositionTemplateState.templates.find((candidate) => candidate.id === id)
  if (!template) throw new Error('Шаблон не найден в локальном каталоге')
  return template
}

function persistCatalog(templates: readonly CompositionTemplate[]): void {
  if (templates.length > MAX_TEMPLATE_CATALOG_ITEMS) {
    throw new Error(`Локальный каталог вмещает не более ${MAX_TEMPLATE_CATALOG_ITEMS} шаблонов`)
  }
  for (const template of templates) serializeCompositionTemplate(template)
  const catalog: StoredTemplateCatalog = { schemaVersion: 1, templates: [...templates] }
  const serialized = JSON.stringify(catalog)
  if (new TextEncoder().encode(serialized).byteLength > MAX_TEMPLATE_CATALOG_BYTES) {
    throw new Error('Локальный каталог шаблонов превышает лимит 4 МиБ')
  }
  const storage = getStorage()
  if (!storage) return
  try {
    storage.setItem(TEMPLATE_CATALOG_KEY, serialized)
  } catch {
    throw new Error('Не удалось сохранить шаблоны: локальное хранилище переполнено')
  }
}

function loadCatalog(): CompositionTemplate[] {
  const storage = getStorage()
  if (!storage) return []
  try {
    const serialized = storage.getItem(TEMPLATE_CATALOG_KEY)
    if (!serialized || new TextEncoder().encode(serialized).byteLength > MAX_TEMPLATE_CATALOG_BYTES) return []
    const parsed = JSON.parse(serialized) as Partial<StoredTemplateCatalog>
    if (parsed.schemaVersion !== 1 || !Array.isArray(parsed.templates) || parsed.templates.length > MAX_TEMPLATE_CATALOG_ITEMS) {
      return []
    }
    return parsed.templates
      .map((template) => parseCompositionTemplate(JSON.stringify(template)))
      .sort((left, right) => left.name.localeCompare(right.name) || left.id.localeCompare(right.id))
  } catch {
    return []
  }
}

function getStorage(): Storage | null {
  if (storageOverride !== undefined) return storageOverride
  if (typeof window === 'undefined') return null
  try { return window.localStorage } catch { return null }
}

function makeId(prefix: string): string {
  const uuid = globalThis.crypto?.randomUUID?.()
  return `${prefix}-${uuid ?? `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`}`
}
