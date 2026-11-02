import { describe, expect, it } from 'vitest'
import { compositionSearchEntries, searchCompositionEntries } from './composition.js'
import { createComposition, addTrack, addClip } from '../composition/commands.js'
import { upsertCompositionMarker } from '../composition/markers.js'

it('indexes commands, media, captions and markers with navigation targets', () => {
  let document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  document = addTrack(document, { id: 'captions', name: 'Subtitles', kind: 'text', locked: false, hidden: false, clips: [] })
  document = addClip(document, 'captions', { id: 'hello', kind: 'text', timelineStartTicks: 2_000_000, durationTicks: 1_000_000, text: 'Привет, мир!', x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } })
  document = upsertCompositionMarker(document, { id: 'cut', tick: 3_000_000, label: 'Opening cut' })
  const entries = compositionSearchEntries(document, [{ id: 'source', label: 'Interview.mov' }], [{ id: 'composition.undo', kind: 'command', label: 'Undo', enabled: false, disabledReason: 'No history' }])
  expect(entries.map((item) => item.kind)).toEqual(['command', 'media', 'caption', 'marker'])
  expect(searchCompositionEntries(entries, 'привет')[0]).toMatchObject({ trackId: 'captions', id: 'hello', tick: 2_000_000 })
  expect(searchCompositionEntries(entries, 'opening')[0]).toMatchObject({ kind: 'marker', tick: 3_000_000 })
  expect(searchCompositionEntries(entries, 'undo')[0]?.enabled).toBe(false)
})

describe('search ranking', () => {
  const entries = [
    { id: 'fuzzy', kind: 'media', label: 'I n t e r v i e w' },
    { id: 'exact', kind: 'media', label: 'Interview' },
    { id: 'other', kind: 'media', label: 'Music', context: 'Interview background' },
  ] as const
  it('ranks contiguous matches before subsequences and requires all terms', () => {
    expect(searchCompositionEntries(entries, 'interview').map((item) => item.id)).toEqual(['exact', 'other', 'fuzzy'])
    expect(searchCompositionEntries(entries, 'music interview').map((item) => item.id)).toEqual(['other'])
    expect(searchCompositionEntries(entries, 'missing')).toEqual([])
  })
  it('limits the result list and preserves stable ordering for an empty query', () => {
    expect(searchCompositionEntries(entries, '', 2).map((item) => item.id)).toEqual(['fuzzy', 'exact'])
  })
})
