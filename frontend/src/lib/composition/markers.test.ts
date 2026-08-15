import { describe, expect, it } from 'vitest'

import { createComposition } from './commands'
import {
  MAX_COMPOSITION_MARKERS,
  compositionMarkers,
  deleteCompositionMarker,
  replaceCompositionMarkersByOrigin,
  upsertCompositionMarker,
} from './markers'
import { COMPOSITION_TIME_BASE, type Composition } from './types'
import { normalizeComposition } from './validation'

function document(): Composition {
  return createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
}

describe('composition markers', () => {
  it('adds, sorts, moves and deletes stable project markers', () => {
    let value = upsertCompositionMarker(document(), {
      id: 'outro',
      tick: 8 * COMPOSITION_TIME_BASE,
      label: '  Outro  ',
      color: '#7c3aed',
    })
    value = upsertCompositionMarker(value, {
      id: 'intro',
      tick: COMPOSITION_TIME_BASE,
      label: 'Intro',
    })
    expect(compositionMarkers(value)).toEqual([
      { id: 'intro', tick: COMPOSITION_TIME_BASE, label: 'Intro' },
      { id: 'outro', tick: 8 * COMPOSITION_TIME_BASE, label: 'Outro', color: '#7c3aed' },
    ])

    value = upsertCompositionMarker(value, {
      id: 'outro',
      tick: 2 * COMPOSITION_TIME_BASE,
      label: 'Outro moved',
    })
    expect(compositionMarkers(value).map((marker) => [marker.id, marker.tick])).toEqual([
      ['intro', COMPOSITION_TIME_BASE],
      ['outro', 2 * COMPOSITION_TIME_BASE],
    ])
    expect(compositionMarkers(deleteCompositionMarker(value, 'intro')).map((marker) => marker.id)).toEqual(['outro'])
  })

  it('survives tolerant project normalization but remains outside render wire concerns', () => {
    let marked = upsertCompositionMarker(document(), {
      id: 'review-point',
      tick: 500_000,
      label: 'Check this cut',
      origin: 'manual',
    })
    marked = upsertCompositionMarker(marked, {
      id: 'auto-beat-1000000',
      tick: 1_000_000,
      label: 'Auto Beat 1 · ≈ 120 BPM',
      origin: 'auto_beat',
    })
    const restored = normalizeComposition(JSON.parse(JSON.stringify(marked)))
    expect(compositionMarkers(restored)).toEqual([
      { id: 'review-point', tick: 500_000, label: 'Check this cut' },
      { id: 'auto-beat-1000000', tick: 1_000_000, label: 'Auto Beat 1 · ≈ 120 BPM', origin: 'auto_beat' },
    ])
  })

  it('replaces only one generated origin while preserving manual markers', () => {
    let value = upsertCompositionMarker(document(), { id: 'manual-cut', tick: 10, label: 'Manual' })
    value = replaceCompositionMarkersByOrigin(value, 'auto_beat', [
      { id: 'auto-beat-20', tick: 20, label: 'Auto Beat 1', origin: 'auto_beat' },
    ])
    value = replaceCompositionMarkersByOrigin(value, 'auto_beat', [
      { id: 'auto-beat-30', tick: 30, label: 'Auto Beat 1', origin: 'auto_beat' },
    ])
    expect(compositionMarkers(value)).toEqual([
      { id: 'manual-cut', tick: 10, label: 'Manual' },
      { id: 'auto-beat-30', tick: 30, label: 'Auto Beat 1', origin: 'auto_beat' },
    ])
    expect(() => replaceCompositionMarkersByOrigin(value, 'auto_beat', [
      { id: 'wrong-origin', tick: 40, label: 'Wrong' },
    ])).toThrow('auto_beat')
  })

  it('rejects unsafe ids, ranges, colors and duplicate stored ids', () => {
    expect(() => upsertCompositionMarker(document(), { id: '../bad', tick: 0, label: 'Bad' })).toThrow('stable')
    expect(() => upsertCompositionMarker(document(), { id: 'bad-tick', tick: -1, label: 'Bad' })).toThrow('outside')
    expect(() => upsertCompositionMarker(document(), { id: 'bad-color', tick: 0, label: 'Bad', color: 'red' })).toThrow('#rrggbb')
    expect(() => upsertCompositionMarker(document(), {
      id: 'bad-origin', tick: 0, label: 'Bad', origin: 'generated' as 'auto_beat',
    })).toThrow('origin')
    const duplicate = {
      ...document(),
      markers: [
        { id: 'same', tick: 0, label: 'One' },
        { id: 'same', tick: 1, label: 'Two' },
      ],
    } as Composition
    expect(() => compositionMarkers(duplicate)).toThrow('Duplicate')

    const full = {
      ...document(),
      markers: Array.from({ length: MAX_COMPOSITION_MARKERS }, (_, index) => ({
        id: `marker-${index}`,
        tick: index,
        label: `Marker ${index}`,
      })),
    } as Composition
    expect(() => upsertCompositionMarker(full, { id: 'overflow-marker', tick: 0, label: 'Overflow' })).toThrow('limit')
  })
})
