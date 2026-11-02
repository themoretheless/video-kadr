import { describe, expect, it } from 'vitest'
import { addClip, addTrack, createComposition, deleteClip, moveClip, moveClips, splitClip, trimClip, registerSource, duplicateClip, pasteClips } from './commands.js'
import { createCompositionLink, removeCompositionLink, compositionLinkToggleState } from './links.js'
import { assertValidComposition } from './validation.js'

function fixture() {
  let doc = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
  for (const [index, start] of [1_000_000, 2_000_000].entries()) {
    doc = addTrack(doc, { id: `track-${index}`, kind: 'text', name: 'Text', hidden: false, locked: false, clips: [] })
    doc = addClip(doc, `track-${index}`, { id: `clip-${index}`, kind: 'text', timelineStartTicks: start, durationTicks: 3_000_000, text: 'Linked', x: 0, y: 0, opacity: 1, style: { fontSizePx: 48, color: '#FFFFFFFF', align: 'center' } })
  }
  return createCompositionLink(doc, 'link', ['clip-0', 'clip-1'])
}

describe('flat clip links', () => {
  it('moves an exact set without expanding links and handles same-track subjects simultaneously', () => {
    const doc = fixture()
    expect(moveClips(doc, ['clip-0'], 'track-0', 6_000_000).tracks[1]!.clips[0]!.timelineStartTicks).toBe(2_000_000)
    let lane = removeCompositionLink(doc, 'link')
    lane = addClip(lane, 'track-0', { ...lane.tracks[0]!.clips[0]!, id: 'adjacent', timelineStartTicks: 4_000_000 })
    const moved = moveClips(lane, ['clip-0', 'adjacent'], 'track-0', 4_000_000)
    expect(moved.tracks[0]!.clips.map((clip) => clip.timelineStartTicks)).toEqual([4_000_000, 7_000_000])
    expect(lane.tracks[0]!.clips.map((clip) => clip.timelineStartTicks)).toEqual([1_000_000, 4_000_000])
    const occupied = addClip(lane, 'track-0', { ...lane.tracks[0]!.clips[0]!, id: 'blocker', timelineStartTicks: 7_000_000 })
    expect(() => moveClips(occupied, ['clip-0', 'adjacent'], 'track-0', 4_000_000)).toThrow()
    expect(occupied.tracks[0]!.clips.map((clip) => clip.timelineStartTicks)).toEqual([1_000_000, 4_000_000, 7_000_000])
    const locked = { ...doc, tracks: doc.tracks.map((track, index) => ({ ...track, locked: index === 1 })) }
    expect(() => moveClips(locked, ['clip-0', 'clip-1'], 'track-0', 6_000_000)).toThrow('locked')
  })
  it('pastes only explicit subjects, reconstructing only links with two or more copied members', () => {
    const doc = fixture()
    const single = pasteClips(doc, [{ clipId: 'clip-0', id: 'copy-a' }], { timelineStartTicks: 6_000_000 })
    expect(single.tracks.map((track) => track.clips.length)).toEqual([2, 1])
    expect(single.links).toEqual(doc.links)
    const pair = pasteClips(doc, [{ clipId: 'clip-0', id: 'copy-a' }, { clipId: 'clip-1', id: 'copy-b' }], { timelineStartTicks: 6_000_000, linkIds: ['new-link'] })
    expect(pair.tracks.map((track) => track.clips[1]!.timelineStartTicks)).toEqual([6_000_000, 7_000_000])
    expect(pair.links?.[1]).toEqual({ id: 'new-link', memberIds: ['copy-a', 'copy-b'] })
    const third = addClip(doc, 'track-0', { ...doc.tracks[0]!.clips[0]!, id: 'third', timelineStartTicks: 10_000_000 })
    const group = createCompositionLink(third, 'three', ['clip-0', 'clip-1', 'third'], true)
    const subset = pasteClips(group, [{ clipId: 'clip-0', id: 'copy-a' }, { clipId: 'clip-1', id: 'copy-b' }], { timelineStartTicks: 6_000_000 })
    expect(subset.links?.map((link) => link.memberIds.length)).toEqual([3, 2])
    expect(subset.tracks.map((track) => track.clips.length)).toEqual([3, 2])
    const locked = { ...doc, tracks: doc.tracks.map((track, index) => ({ ...track, locked: index === 1 })) }
    expect(pasteClips(locked, [{ clipId: 'clip-0', id: 'copy-a' }], { timelineStartTicks: 6_000_000 }).tracks[0]!.clips).toHaveLength(2)
    expect(() => pasteClips(locked, [{ clipId: 'clip-0', id: 'copy-a' }, { clipId: 'clip-1', id: 'copy-b' }], { timelineStartTicks: 6_000_000 })).toThrow('locked')
  })
  it('rejects a mixed toggle subject set without silently reassigning a linked member', () => {
    const original = fixture()
    const document = addClip(original, 'track-0', { ...original.tracks[0]!.clips[0]!, id: 'c', timelineStartTicks: 10_000_000 })
    expect(compositionLinkToggleState(document, ['clip-0', 'c'])).toMatchObject({ mode: 'unavailable' })
    expect(compositionLinkToggleState(document, ['clip-0'])).toMatchObject({ mode: 'remove', linkId: 'link' })
    expect(document.links).toEqual(original.links)
  })
  it('duplicates partners with their relative offsets into a separate link', () => {
    const doc = fixture()
    const result = duplicateClip(doc, 'clip-0', { id: 'copy', timelineStartTicks: 6_000_000 })
    expect(result.tracks.map((track) => track.clips[1]!.timelineStartTicks)).toEqual([6_000_000, 7_000_000])
    expect(result.links).toEqual([{ id: 'link', memberIds: ['clip-0', 'clip-1'] }, { id: 'copy-link', memberIds: ['copy', 'copy-linked-1'] }])
    expect(duplicateClip(doc, 'clip-0', { id: 'copy', timelineStartTicks: 6_000_000, escapeLink: true }).links).toEqual(doc.links)
    expect(doc.tracks[0]!.clips).toHaveLength(1)
  })
  it('rejects duplicate collisions and locked partners without producing any clones', () => {
    const doc = fixture()
    expect(() => duplicateClip(doc, 'clip-0', { id: 'copy', timelineStartTicks: 2_000_000 })).toThrow()
    const occupiedPartner = addClip(doc, 'track-1', { ...doc.tracks[1]!.clips[0]!, id: 'blocker', timelineStartTicks: 7_000_000 })
    expect(() => duplicateClip(occupiedPartner, 'clip-0', { id: 'copy', timelineStartTicks: 6_000_000 })).toThrow()
    expect(occupiedPartner.tracks.map((track) => track.clips.length)).toEqual([1, 2])
    const locked = { ...doc, tracks: doc.tracks.map((track, index) => ({ ...track, locked: index === 1 })) }
    expect(() => duplicateClip(locked, 'clip-0', { id: 'copy', timelineStartTicks: 6_000_000 })).toThrow('locked')
    expect(doc.tracks.map((track) => track.clips.length)).toEqual([1, 1])
    expect(doc.links).toHaveLength(1)
  })
  it('trims only exactly aligned edges and supports an explicit escape', () => {
    const doc = fixture()
    const slipped = trimClip(doc, 'clip-0', 1_500_000, 4_000_000)
    expect(slipped.tracks[1]!.clips[0]).toEqual(doc.tracks[1]!.clips[0])
    const aligned = moveClip(doc, 'clip-1', 'track-1', 1_000_000, true)
    expect(trimClip(aligned, 'clip-0', 1_500_000, 3_500_000).tracks.map((track) => track.clips[0]!.timelineStartTicks)).toEqual([1_500_000, 1_500_000])
    expect(trimClip(aligned, 'clip-0', 1_500_000, 3_500_000, true).tracks[1]!.clips[0]!.timelineStartTicks).toBe(1_000_000)
  })
  it('clamps source extension by the tightest aligned partner and rejects locked trims atomically', () => {
    let doc = createComposition({ width: 1280, height: 720, fps: 30, backgroundColor: '#000000' })
    for (const [index, sourceIn] of [2_000_000, 500_000].entries()) {
      doc = registerSource(doc, { id: `source-${index}`, kind: 'video', durationTicks: 10_000_000, width: 1280, height: 720, hasAudio: false })
      doc = addTrack(doc, { id: `video-${index}`, kind: 'video', name: 'Video', hidden: false, locked: false, muted: false, clips: [], transitions: [] })
      doc = addClip(doc, `video-${index}`, { id: `clip-${index}`, kind: 'video', timelineStartTicks: 2_000_000, sourceId: `source-${index}`, sourceInTicks: sourceIn, sourceOutTicks: sourceIn + 3_000_000, transform: { x: 0, y: 0, width: 1280, height: 720, fit: 'contain' }, opacity: 1, sourceAudioEnabled: false, audioGain: 1 })
    }
    doc = createCompositionLink(doc, 'link', ['clip-0', 'clip-1'])
    const trimmed = trimClip(doc, 'clip-0', 0, 5_000_000)
    expect(trimmed.tracks.map((track) => track.clips[0]!.timelineStartTicks)).toEqual([1_500_000, 1_500_000])
    const reversed = { ...doc, tracks: doc.tracks.map((track) => track.kind === 'video' ? { ...track, clips: track.clips.map((clip) => ({ ...clip, playbackMode: { mode: 'reverse' as const } })) } : track) }
    const extended = trimClip(reversed, 'clip-0', 2_000_000, 9_000_000)
    expect(extended.tracks.map((track) => {
      const clip = track.clips[0]!
      return clip.kind === 'video' ? [clip.sourceInTicks, clip.sourceOutTicks] : []
    })).toEqual([[1_500_000, 5_000_000], [0, 3_500_000]])
    const locked = { ...doc, tracks: doc.tracks.map((track, index) => ({ ...track, locked: index === 1 })) }
    expect(() => trimClip(locked, 'clip-0', 2_500_000, 5_000_000)).toThrow('locked')
    expect(doc.tracks[0]!.clips[0]!.timelineStartTicks).toBe(2_000_000)
  })
  it('validates unique membership, reassigns and dissolves after local deletion', () => {
    const doc = fixture()
    expect(() => createCompositionLink(doc, 'other', ['clip-0', 'clip-1'])).toThrow('already linked')
    expect(createCompositionLink(doc, 'other', ['clip-0', 'clip-1'], true).links?.map((link) => link.id)).toEqual(['other'])
    expect(deleteClip(doc, 'clip-0').links).toEqual([])
    expect(removeCompositionLink(doc, 'link').links).toEqual([])
    expect(() => assertValidComposition({ ...doc, links: [{ id: 'bad', memberIds: ['clip-0', 'missing'] }] })).toThrow()
  })
  it('moves with one shared delta, clamps as a set, and rejects locked partners atomically', () => {
    const doc = fixture()
    const moved = moveClip(doc, 'clip-1', 'track-1', 0)
    expect(moved.tracks.map((track) => track.clips[0]!.timelineStartTicks)).toEqual([0, 1_000_000])
    expect(moveClip(doc, 'clip-1', 'track-1', 0, true).tracks[0]!.clips[0]!.timelineStartTicks).toBe(1_000_000)
    const locked = { ...doc, tracks: doc.tracks.map((track, i) => ({ ...track, locked: i === 0 })) }
    expect(() => moveClip(locked, 'clip-1', 'track-1', 3_000_000)).toThrow('locked')
    expect(doc.tracks[1]!.clips[0]!.timelineStartTicks).toBe(2_000_000)
  })
  it('does not link a lone right piece to unsplit partners and refuses right-link collisions atomically', () => {
    const doc = fixture()
    const split = splitClip(doc, 'clip-0', 1_500_000, 'right')
    expect(split.links).toEqual(doc.links)
    expect(split.tracks.map(track => track.clips.length)).toEqual([2, 1])
    const extras = pasteClips(doc, [{ clipId: 'clip-0', id: 'copy-a' }, { clipId: 'clip-1', id: 'copy-b' }], { timelineStartTicks: 8_000_000, linkIds: ['right-link'] })
    const before = JSON.stringify(extras)
    expect(() => splitClip(extras, 'clip-0', 2_500_000, 'right')).toThrow('already exists')
    expect(JSON.stringify(extras)).toBe(before)
  })
  it('splits spanning partners into independent left and right links', () => {
    const doc = fixture()
    const split = splitClip(doc, 'clip-0', 2_500_000, 'right')
    expect(split.tracks.map((track) => track.clips.length)).toEqual([2, 2])
    expect(split.links).toEqual([
      { id: 'link', memberIds: ['clip-0', 'clip-1'] },
      { id: 'right-link', memberIds: ['right', 'right-linked-1'] },
    ])
    const moved = moveClip(split, 'right', 'track-0', 6_000_000)
    expect(moved.tracks.map(track => track.clips.find(clip => clip.id.startsWith('clip-'))!.timelineStartTicks)).toEqual([1_000_000, 2_000_000])
    expect(moved.tracks.map(track => track.clips.find(clip => clip.id.startsWith('right'))!.timelineStartTicks)).toEqual([6_000_000, 6_000_000])
    expect(doc.links).toEqual([{ id: 'link', memberIds: ['clip-0', 'clip-1'] }])
    expect(splitClip(doc, 'clip-0', 2_500_000, 'right', true).tracks[1]!.clips).toHaveLength(1)
    expect(() => splitClip(doc, 'clip-0', 5_000_000, 'right')).toThrow('strictly inside')
  })
})
