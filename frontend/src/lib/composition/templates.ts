import type {
  Composition,
  CompositionClip,
  CompositionSource,
  CompositionTrack,
} from './types'
import type { MulticamGroup } from './multicam'
import { assertValidComposition, isStableId, normalizeComposition, sourceSupportsClip } from './validation'

export const COMPOSITION_TEMPLATE_SCHEMA_VERSION = 1 as const
export const MAX_COMPOSITION_TEMPLATE_BYTES = 2 * 1024 * 1024

export type CompositionTemplateSlot =
  | {
      readonly id: string
      readonly kind: 'media'
      readonly label: string
      readonly clipId: string
    }
  | {
      readonly id: string
      readonly kind: 'text'
      readonly label: string
      readonly clipId: string
    }

export interface CompositionTemplate {
  readonly schemaVersion: typeof COMPOSITION_TEMPLATE_SCHEMA_VERSION
  readonly id: string
  readonly name: string
  readonly composition: Composition
  readonly slots: readonly CompositionTemplateSlot[]
}

export interface TemplateReplacement {
  readonly source?: CompositionSource
  readonly text?: string
}

export class CompositionTemplateError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'CompositionTemplateError'
  }
}

export function createCompositionTemplate(
  id: string,
  name: string,
  composition: Composition,
  slots: readonly CompositionTemplateSlot[],
): CompositionTemplate {
  composition = normalizeComposition(composition)
  if (!isStableId(id)) throw new CompositionTemplateError('Template id is invalid')
  if (!name.trim() || name.length > 128) throw new CompositionTemplateError('Template name is invalid')
  validateSlots(composition, slots)
  return {
    schemaVersion: COMPOSITION_TEMPLATE_SCHEMA_VERSION,
    id,
    name: name.trim(),
    composition: clone(composition),
    slots: clone(slots),
  }
}

export function instantiateCompositionTemplate(
  template: CompositionTemplate,
  replacements: Readonly<Record<string, TemplateReplacement>>,
): Composition {
  validateTemplate(template)
  const document = clone(template.composition) as MutableComposition
  const sources: Record<string, CompositionSource> = { ...document.sources }
  for (const slot of template.slots) {
    const replacement = replacements[slot.id]
    if (!replacement) throw new CompositionTemplateError(`Replacement is missing for slot ${slot.id}`)
    const located = findClip(document.tracks, slot.clipId)
    if (!located) throw new CompositionTemplateError(`Template clip ${slot.clipId} is missing`)

    if (slot.kind === 'text') {
      if (located.clip.kind !== 'text' || typeof replacement.text !== 'string') {
        throw new CompositionTemplateError(`Slot ${slot.id} requires text`)
      }
      located.track.clips[located.index] = { ...located.clip, text: replacement.text }
      continue
    }

    const source = replacement.source
    if (!source || located.clip.kind === 'text' || !sourceSupportsClip(source, located.clip)) {
      throw new CompositionTemplateError(`Slot ${slot.id} has an incompatible media source`)
    }
    located.track.clips[located.index] = { ...located.clip, sourceId: source.id }
    sources[source.id] = clone(source)
  }
  document.sources = removeUnusedSources(document.tracks, sources, document.multicamGroups)
  return normalizeComposition(document)
}

export function serializeCompositionTemplate(template: CompositionTemplate): string {
  validateTemplate(template)
  const serialized = JSON.stringify(template, null, 2)
  if (new TextEncoder().encode(serialized).byteLength > MAX_COMPOSITION_TEMPLATE_BYTES) {
    throw new CompositionTemplateError('Template exceeds the local package limit')
  }
  return serialized
}

export function parseCompositionTemplate(serialized: string): CompositionTemplate {
  if (new TextEncoder().encode(serialized).byteLength > MAX_COMPOSITION_TEMPLATE_BYTES) {
    throw new CompositionTemplateError('Template exceeds the local package limit')
  }
  let parsed: unknown
  try {
    parsed = JSON.parse(serialized)
  } catch {
    throw new CompositionTemplateError('Template is not valid JSON')
  }
  if (!isRecord(parsed)) throw new CompositionTemplateError('Unsupported template schema')
  const normalized = { ...parsed, composition: normalizeComposition(parsed.composition) }
  validateTemplate(normalized)
  return clone(normalized)
}

function validateTemplate(value: unknown): asserts value is CompositionTemplate {
  if (!isRecord(value) || value.schemaVersion !== COMPOSITION_TEMPLATE_SCHEMA_VERSION) {
    throw new CompositionTemplateError('Unsupported template schema')
  }
  if (!isStableId(value.id) || typeof value.name !== 'string' || !value.name.trim()) {
    throw new CompositionTemplateError('Template identity is invalid')
  }
  assertValidComposition(value.composition)
  if (!Array.isArray(value.slots)) throw new CompositionTemplateError('Template slots are invalid')
  validateSlots(value.composition, value.slots)
}

function validateSlots(composition: Composition, slots: readonly unknown[]): void {
  const ids = new Set<string>()
  const clipIds = new Set<string>()
  for (const candidate of slots) {
    if (!isRecord(candidate) || !isStableId(candidate.id) || ids.has(candidate.id)) {
      throw new CompositionTemplateError('Template slot ids must be unique and stable')
    }
    if (candidate.kind !== 'media' && candidate.kind !== 'text') {
      throw new CompositionTemplateError(`Template slot ${candidate.id} has an invalid kind`)
    }
    if (typeof candidate.label !== 'string' || !candidate.label.trim() || !isStableId(candidate.clipId)) {
      throw new CompositionTemplateError(`Template slot ${candidate.id} is invalid`)
    }
    const located = findClip(composition.tracks, candidate.clipId)
    if (!located || clipIds.has(candidate.clipId)) {
      throw new CompositionTemplateError(`Template slot clip ${candidate.clipId} is missing or repeated`)
    }
    if ((candidate.kind === 'text') !== (located.clip.kind === 'text')) {
      throw new CompositionTemplateError(`Template slot ${candidate.id} targets the wrong clip kind`)
    }
    ids.add(candidate.id)
    clipIds.add(candidate.clipId)
  }
}

type MutableTrack = CompositionTrack & { clips: CompositionClip[] }

interface MutableComposition extends Omit<Composition, 'sources' | 'tracks'> {
  sources: Record<string, CompositionSource>
  tracks: MutableTrack[]
}

function findClip(tracks: readonly { readonly clips: readonly CompositionClip[] }[], clipId: string) {
  for (const track of tracks) {
    const index = track.clips.findIndex((clip) => clip.id === clipId)
    if (index >= 0) return { track: track as MutableTrack, clip: track.clips[index]!, index }
  }
  return null
}

function removeUnusedSources(
  tracks: readonly { readonly clips: readonly CompositionClip[] }[],
  sources: Readonly<Record<string, CompositionSource>>,
  multicamGroups: readonly MulticamGroup[] | undefined,
): Record<string, CompositionSource> {
  const used = new Set<string>()
  for (const track of tracks) {
    for (const clip of track.clips) {
      if (clip.kind !== 'text') used.add(clip.sourceId)
    }
  }
  for (const group of multicamGroups ?? []) {
    for (const angle of group.angles) used.add(angle.sourceId)
  }
  return Object.fromEntries(
    Object.entries(sources)
      .filter(([id]) => used.has(id))
      .sort(([left], [right]) => left.localeCompare(right)),
  )
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
