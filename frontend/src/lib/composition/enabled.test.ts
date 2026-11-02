import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, setClipsEnabled } from './commands'
import { createCompositionLink } from './links'
import { normalizeComposition, assertValidComposition } from './validation'
import type { Composition } from './types'

function fixture(): Composition {
  let document = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  for (const id of ['a', 'b']) {
    document = addTrack(document, { id, kind: 'text', name: id, hidden: false, locked: false, clips: [] })
    document = addClip(document, id, { id: `${id}-clip`, kind: 'text', timelineStartTicks: 0, durationTicks: 2_000_000, text: id, x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } })
  }
  return createCompositionLink(document, 'pair', ['a-clip', 'b-clip'])
}

describe('exact clip enabled set', () => {
  it('changes only named members and preserves all media, timing and links', () => {
    const document = fixture()
    const next = setClipsEnabled(document, ['a-clip'], false)
    expect(next.tracks[0]!.clips[0]).toEqual({ ...document.tracks[0]!.clips[0], enabled: false })
    expect(next.tracks[1]!.clips[0]).toEqual(document.tracks[1]!.clips[0])
    expect(next.links).toEqual(document.links)
    expect(normalizeComposition(JSON.parse(JSON.stringify(next))).tracks[0]!.clips[0]!.enabled).toBe(false)
    expect(document.tracks[0]!.clips[0]!.enabled).toBeUndefined()
    expect(setClipsEnabled(next, ['a-clip', 'b-clip'], true).tracks.every((track) => track.clips[0]!.enabled)).toBe(true)
  })

  it('permits unselected locked siblings but rejects any selected locked member atomically', () => {
    const value = fixture()
    const document = { ...value, tracks: value.tracks.map((track) => track.id === 'b' ? { ...track, locked: true } : track) } as Composition
    expect(setClipsEnabled(document, ['a-clip'], false).tracks[0]!.clips[0]!.enabled).toBe(false)
    const before = JSON.stringify(document)
    expect(() => setClipsEnabled(document, ['a-clip', 'b-clip'], false)).toThrow('locked')
    expect(JSON.stringify(document)).toBe(before)
  })

  it('rejects malformed values, empty selections and missing identities', () => {
    const document = fixture()
    expect(() => setClipsEnabled(document, [], false)).toThrow()
    expect(() => setClipsEnabled(document, ['missing'], false)).toThrow()
    expect(() => setClipsEnabled(document, ['a-clip'], 'false' as unknown as boolean)).toThrow()
    const invalid = { ...document, tracks: document.tracks.map((track) => ({ ...track, clips: track.clips.map((clip) => ({ ...clip, enabled: 0 })) })) } as unknown as Composition
    expect(() => assertValidComposition(invalid)).toThrow('enabled')
  })
})
