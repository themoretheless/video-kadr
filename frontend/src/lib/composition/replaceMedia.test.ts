import { expect, it } from 'vitest'
import { addClip, addTrack, createComposition, registerSource } from './commands'
import { replaceClipMedia } from './replaceMedia'
import type { VideoClip, VideoTrack } from './types'
function fixture() {
  let d = createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' })
  for (const [id, durationTicks, hasAudio] of [['old', 10_000_000, true], ['new', 12_000_000, true], ['short', 1_000_000, true], ['silent', 12_000_000, false]] as const) d = registerSource(d, { id, kind: 'video', durationTicks, width: 4, height: 4, hasAudio })
  d = addTrack(d, { id: 'v', kind: 'video', name: 'V', locked: false, hidden: false, muted: false, clips: [] })
  const clip: VideoClip = { id: 'a', kind: 'video', sourceId: 'old', timelineStartTicks: 0, sourceInTicks: 1_000_000, sourceOutTicks: 5_000_000, speed: 2, playbackMode: { mode: 'reverse' }, transform: { x: 2, y: 3, width: 16, height: 16, fit: 'contain' }, opacity: 0.7, sourceAudioEnabled: true, audioGain: 0.5, animation: { opacity: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', keyframes: [{ tick: 0, value: 0 }, { tick: 2_000_000, value: 1 }] } } } }
  return addClip(addClip(d, 'v', clip), 'v', { ...clip, id: 'b', timelineStartTicks: 2_000_000 })
}
it('replaces only one placement, retaining authoring and other uses of the source', () => {
  const d = fixture(), before = JSON.stringify(d)
  const result = replaceClipMedia(d, 'a', 'new')
  const lane = result.tracks[0] as VideoTrack
  expect(lane.clips[0]).toEqual({ ...d.tracks[0]!.clips[0], sourceId: 'new' })
  expect(lane.clips[1]).toEqual(d.tracks[0]!.clips[1])
  expect(result.sources.old).toEqual(d.sources.old)
  expect(JSON.stringify(d)).toBe(before)
})
it('moves a frozen source window and the frozen tick together', () => {
  const base = fixture()
  const d = { ...base, tracks: base.tracks.map(t => t.kind === 'video' ? { ...t, clips: t.clips.map(c => c.id === 'a' ? { ...c, playbackMode: { mode: 'freeze' as const, sourceTick: 3_000_000 }, sourceAudioEnabled: false } : c) } : t) }
  const result = replaceClipMedia(d, 'a', 'new', 2_000_000)
  expect(result.tracks[0]!.clips[0]).toMatchObject({ sourceInTicks: 2_000_000, sourceOutTicks: 6_000_000, playbackMode: { mode: 'freeze', sourceTick: 4_000_000 } })
})
it('refuses missing, short, silent and locked replacements atomically', () => {
  const d = fixture(), before = JSON.stringify(d)
  for (const id of ['absent', 'short', 'silent']) expect(() => replaceClipMedia(d, 'a', id)).toThrow()
  expect(() => replaceClipMedia(d, 'a', 'new', -1)).toThrow()
  const locked = { ...d, tracks: d.tracks.map(t => ({ ...t, locked: true })) }
  expect(() => replaceClipMedia(locked, 'a', 'new')).toThrow()
  expect(JSON.stringify(d)).toBe(before)
})
it('replaces images and audio independently and rejects cross-kind media', () => {
  let d = fixture()
  for (const id of ['image-a', 'image-b']) d = registerSource(d, { id, kind: 'image', durationTicks: 0, width: 4, height: 4, hasAudio: false })
  d = registerSource(d, { id: 'audio', kind: 'audio', durationTicks: 12_000_000, width: 0, height: 0, hasAudio: true })
  d = addTrack(d, { id: 'i', kind: 'image', name: 'I', locked: false, hidden: false, clips: [] })
  d = addClip(d, 'i', { id: 'image', kind: 'image', sourceId: 'image-a', timelineStartTicks: 0, durationTicks: 4_000_000, transform: { x: 0, y: 0, width: 16, height: 16, fit: 'contain' }, opacity: 1 })
  d = addTrack(d, { id: 'audio-track', kind: 'audio', name: 'A', locked: false, muted: false, clips: [] })
  d = addClip(d, 'audio-track', { id: 'sound', kind: 'audio', sourceId: 'old', timelineStartTicks: 0, sourceInTicks: 1_000_000, sourceOutTicks: 5_000_000, gain: 0.7, pan: -0.25, reversed: true })
  expect(replaceClipMedia(d, 'image', 'image-b').tracks[1]!.clips[0]).toEqual({ ...d.tracks[1]!.clips[0], sourceId: 'image-b' })
  expect(replaceClipMedia(d, 'sound', 'audio').tracks[2]!.clips[0]).toEqual({ ...d.tracks[2]!.clips[0], sourceId: 'audio' })
  expect(() => replaceClipMedia(d, 'a', 'image-b')).toThrow()
  expect(() => replaceClipMedia(d, 'sound', 'image-b')).toThrow()
  expect(() => replaceClipMedia(d, 'image', 'new')).toThrow()
})

import { setClipMediaVersion } from './replaceMedia'
import { splitClip, duplicateClip, deleteClip } from './commands'
import { mergeClips } from './mergeClips'
import { pasteInsertComposition } from './clipboardInsert'
import { relinkCompositionSource } from './relink'
import { buildCompositionWireDocument } from './payload'
it('retains the first original across repeated processed replacements and ordinary replacement clears the pair', () => {
  const d = fixture()
  const processed = replaceClipMedia(d, 'a', 'new', undefined, true)
  expect(processed.metadata?.mediaVersions?.a).toEqual({ originalSourceId: 'old', copySourceId: 'new' })
  const off = setClipMediaVersion(processed, 'a', false)
  expect(off.tracks[0]!.clips[0]).toEqual(d.tracks[0]!.clips[0])
  expect(setClipMediaVersion(off, 'a', true).tracks[0]!.clips[0]).toEqual(processed.tracks[0]!.clips[0])
  const third = registerSource(processed, { ...processed.sources.new!, id: 'third' })
  const again = replaceClipMedia(third, 'a', 'third', undefined, true)
  expect(again.metadata?.mediaVersions?.a?.originalSourceId).toBe('old')
  expect(replaceClipMedia(again, 'a', 'new').metadata?.mediaVersions?.a).toBeUndefined()
})
it('preserves versions through split, duplicate, join, delete, relink and wire serialization', () => {
  const d = replaceClipMedia(fixture(), 'a', 'new', undefined, true)
  const split = splitClip(d, 'a', 1_000_000, 'right')
  expect(split.metadata?.mediaVersions?.right).toEqual(d.metadata?.mediaVersions?.a)
  const joined = mergeClips(split, ['a', 'right']).document
  expect(joined.metadata?.mediaVersions?.right).toBeUndefined()
  expect(setClipMediaVersion(joined, 'a', false).tracks[0]!.clips[0]).toMatchObject({ sourceId: 'old' })
  const duplicate = duplicateClip(d, 'a', { id: 'duplicate', timelineStartTicks: 4_000_000 })
  expect(duplicate.metadata?.mediaVersions?.duplicate).toEqual(d.metadata?.mediaVersions?.a)
  expect(deleteClip(duplicate, 'duplicate').metadata?.mediaVersions?.duplicate).toBeUndefined()
  const replacement = { ...d.sources.old!, id: 'relinked-original' }
  const relinked = relinkCompositionSource(d, 'old', replacement)
  expect(relinked.metadata?.mediaVersions?.a?.originalSourceId).toBe('relinked-original')
  expect(setClipMediaVersion(relinked, 'a', false).tracks[0]!.clips[0]).toMatchObject({ sourceId: 'relinked-original' })
  expect(buildCompositionWireDocument(d).metadata?.mediaVersions).toEqual(d.metadata?.mediaVersions)
  const empty = { ...d, sources: {}, tracks: d.tracks.map(t => ({ ...t, clips: [] })), metadata: undefined }
  let n = 0
  const pasted = pasteInsertComposition(empty, d, ['a'], 0, () => `pasted-${n++}`).document
  expect(pasted.sources.old).toEqual(d.sources.old)
  expect(pasted.metadata?.mediaVersions?.['pasted-0']).toEqual(d.metadata?.mediaVersions?.a)
  expect(setClipMediaVersion(pasted, 'pasted-0', false).tracks[0]!.clips[0]).toMatchObject({ sourceId: 'old' })
})

it('invalidates only the replaced video binding and preserves it for same-file source shifts',()=>{
 const binding={subject:'person' as const,sourceHash:'a'.repeat(64),modelHash:'88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',runtimeHash:'b'.repeat(64)},base=fixture()
 const input={...base,tracks:base.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>({...clip,concatCutout:binding}))}:track)},before=JSON.stringify(input)
 const output=replaceClipMedia(input,'a','new'),lane=output.tracks[0] as VideoTrack
 expect(lane.clips[0]).not.toHaveProperty('concatCutout');expect(lane.clips[1]!.concatCutout).toEqual(binding);expect(JSON.stringify(input)).toBe(before)
 expect((replaceClipMedia(input,'a','old',2_000_000).tracks[0] as VideoTrack).clips[0]!.concatCutout).toEqual(binding)
})
