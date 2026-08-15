import {
  MAX_COMPOSITION_DURATION_TICKS,
  type Composition,
} from './types'
import { assertValidComposition, isSafeTick, isStableId } from './validation'

export const MAX_COMPOSITION_MARKERS = 256
export const MAX_COMPOSITION_MARKER_LABEL = 128

export type CompositionMarkerOrigin = 'manual' | 'auto_beat'

export interface CompositionMarker {
  readonly id: string
  readonly tick: number
  readonly label: string
  readonly color?: string
  /** Omitted means a manual marker for backward-compatible v1 documents. */
  readonly origin?: CompositionMarkerOrigin
}

type CompositionWithMarkers = Composition & { readonly markers?: unknown }

/** Read and validate authoring-only markers embedded in a project document. */
export function compositionMarkers(composition: Composition): readonly CompositionMarker[] {
  const raw = (composition as CompositionWithMarkers).markers
  if (raw === undefined) return []
  if (!Array.isArray(raw) || raw.length > MAX_COMPOSITION_MARKERS) {
    throw new Error(`Markers must be an array with at most ${MAX_COMPOSITION_MARKERS} entries`)
  }
  const ids = new Set<string>()
  const markers = raw.map((value, index) => normalizeMarker(value, index))
  for (const marker of markers) {
    if (ids.has(marker.id)) throw new Error(`Duplicate marker id ${marker.id}`)
    ids.add(marker.id)
  }
  return markers.sort((left, right) => left.tick - right.tick || left.id.localeCompare(right.id))
}

export function upsertCompositionMarker(
  composition: Composition,
  marker: CompositionMarker,
): Composition {
  assertValidComposition(composition)
  const normalized = normalizeMarker(marker, 0)
  const existing = compositionMarkers(composition)
  if (!existing.some((candidate) => candidate.id === normalized.id) && existing.length >= MAX_COMPOSITION_MARKERS) {
    throw new Error(`Marker limit is ${MAX_COMPOSITION_MARKERS}`)
  }
  const markers = [...existing.filter((candidate) => candidate.id !== normalized.id), normalized]
    .sort((left, right) => left.tick - right.tick || left.id.localeCompare(right.id))
  return withMarkers(composition, markers)
}

export function deleteCompositionMarker(composition: Composition, markerId: string): Composition {
  assertValidComposition(composition)
  if (!isStableId(markerId)) throw new Error('Marker id is not stable')
  const existing = compositionMarkers(composition)
  if (!existing.some((marker) => marker.id === markerId)) throw new Error(`Marker ${markerId} does not exist`)
  return withMarkers(composition, existing.filter((marker) => marker.id !== markerId))
}

/** Atomically replace one generated marker family while preserving manual markers. */
export function replaceCompositionMarkersByOrigin(
  composition: Composition,
  origin: Exclude<CompositionMarkerOrigin, 'manual'>,
  replacements: readonly CompositionMarker[],
): Composition {
  assertValidComposition(composition)
  const preserved = compositionMarkers(composition).filter((marker) => marker.origin !== origin)
  const generated = replacements.map((marker, index) => normalizeMarker(marker, index))
  if (generated.some((marker) => marker.origin !== origin)) {
    throw new Error(`Replacement markers must use ${origin} origin`)
  }
  if (preserved.length + generated.length > MAX_COMPOSITION_MARKERS) {
    throw new Error(`Marker limit is ${MAX_COMPOSITION_MARKERS}`)
  }
  const ids = new Set<string>()
  const markers = [...preserved, ...generated]
    .sort((left, right) => left.tick - right.tick || left.id.localeCompare(right.id))
  for (const marker of markers) {
    if (ids.has(marker.id)) throw new Error(`Duplicate marker id ${marker.id}`)
    ids.add(marker.id)
  }
  return withMarkers(composition, markers)
}

function normalizeMarker(value: unknown, index: number): CompositionMarker {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`Marker ${index} must be an object`)
  }
  const marker = value as Record<string, unknown>
  if (typeof marker.id !== 'string' || !isStableId(marker.id)) {
    throw new Error(`Marker ${index} id is not stable`)
  }
  if (!isSafeTick(marker.tick) || marker.tick > MAX_COMPOSITION_DURATION_TICKS) {
    throw new Error(`Marker ${marker.id} tick is outside the composition limit`)
  }
  if (typeof marker.label !== 'string') throw new Error(`Marker ${marker.id} label is invalid`)
  const label = marker.label.trim()
  if (!label || [...label].length > MAX_COMPOSITION_MARKER_LABEL) {
    throw new Error(`Marker ${marker.id} label must be 1..${MAX_COMPOSITION_MARKER_LABEL} characters`)
  }
  const color = marker.color
  if (color !== undefined && (typeof color !== 'string' || !/^#[0-9a-f]{6}$/i.test(color))) {
    throw new Error(`Marker ${marker.id} color must be #rrggbb`)
  }
  const origin = marker.origin
  if (origin !== undefined && origin !== 'manual' && origin !== 'auto_beat') {
    throw new Error(`Marker ${marker.id} origin is invalid`)
  }
  return {
    id: marker.id,
    tick: marker.tick,
    label,
    ...(color === undefined ? {} : { color }),
    ...(origin === 'auto_beat' ? { origin } : {}),
  }
}

function withMarkers(composition: Composition, markers: readonly CompositionMarker[]): Composition {
  return { ...composition, markers } as Composition
}
