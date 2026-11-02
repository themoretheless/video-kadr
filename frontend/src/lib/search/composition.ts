import type { Composition } from '../composition/types.js'
import { compositionMarkers } from '../composition/markers.js'

export interface SearchEntry { id: string; kind: 'command' | 'media' | 'clip' | 'caption' | 'marker'; label: string; context?: string; trackId?: string; tick?: number; enabled?: boolean; disabledReason?: string }

export function compositionSearchEntries(document: Composition, media: readonly { id: string; label: string }[], commands: readonly SearchEntry[]): SearchEntry[] {
  return [
    ...commands,
    ...media.map((item): SearchEntry => ({ id: item.id, kind: 'media', label: item.label })),
    ...document.tracks.flatMap((track) => track.clips.map((clip): SearchEntry => ({
      id: clip.id, kind: clip.kind === 'text' ? 'caption' : 'clip',
      label: clip.kind === 'text' ? clip.text : clip.kind === 'color' ? `Цвет ${clip.color}` : clip.kind === 'graphic' ? 'Графика' : media.find((item) => item.id === clip.sourceId)?.label ?? clip.sourceId,
      context: track.name, trackId: track.id, tick: clip.timelineStartTicks,
    }))),
    ...compositionMarkers(document).map((marker): SearchEntry => ({ id: marker.id, kind: 'marker', label: marker.label, tick: marker.tick })),
  ]
}

export function searchCompositionEntries(entries: readonly SearchEntry[], query: string, limit = 60): SearchEntry[] {
  const terms = query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean)
  return entries.map((entry, index) => {
    const haystack = `${entry.label} ${entry.context ?? ''}`.toLocaleLowerCase()
    let score = 0
    for (const term of terms) {
      const exact = haystack.indexOf(term)
      if (exact >= 0) { score += exact; continue }
      let position = -1
      for (const character of term) {
        position = haystack.indexOf(character, position + 1)
        if (position < 0) return { entry, index, score: Infinity }
      }
      score += 1000 + position
    }
    return { entry, index, score }
  }).filter((item) => Number.isFinite(item.score)).sort((a, b) => a.score - b.score || a.index - b.index).slice(0, limit).map((item) => item.entry)
}
