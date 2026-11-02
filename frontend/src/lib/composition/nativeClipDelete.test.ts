import { expect, it } from 'vitest'
import type { NativeNestedProject } from '$lib/api'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import { removeNativeClips } from './nativeClipDelete'
const project = (): NativeNestedProject => structuredClone(fixture.createdTimeline) as unknown as NativeNestedProject
it('deletes selected AV clips without ripple or lane changes, preserving sources and input', () => {
  const input = project(), before = JSON.stringify(input)
  expect(removeNativeClips(input, 'root', [])).toBe(input)
  const output = removeNativeClips(input, 'root', ['picture', 'sound'])
  expect(output.compositions.root!.content.tracks.map(track => track.clips.map(clip => clip.id))).toEqual([['next'], []])
  expect(output.compositions.root!.content.sources).toBe(input.compositions.root!.content.sources)
  expect(output.compositions['new-timeline']).toBe(input.compositions['new-timeline'])
  expect(output.compositions.root!.durationUs).toBe(8000000)
  expect(JSON.stringify(input)).toBe(before)
})
it('prunes only removed clip versions/names and attached transitions', () => {
  let input = project()
  const content = fixture.cases[0]!.original as unknown as NativeNestedProject['compositions'][string]['content']
  input = { ...input, compositions: { ...input.compositions, root: { ...input.compositions.root!, content: { ...content, metadata: { clipNames: { picture: 'Picture', next: 'Next' }, mediaVersions: { picture: { originalSourceId: 'source', copySourceId: 'next-source' } } } } } } }
  const output = removeNativeClips(input, 'root', ['picture'])
  const video = output.compositions.root!.content.tracks[0]!
  expect(video.kind === 'video' && video.transitions).toEqual([])
  expect(output.compositions.root!.content.metadata?.clipNames).toEqual({ next: 'Next' })
  expect(output.compositions.root!.content.metadata?.mediaVersions).toEqual({})
})
it('refuses partial locked/unknown/duplicate requests without touching input', () => {
  const input = project()
  const locked: NativeNestedProject = { ...input, compositions: { ...input.compositions, root: { ...input.compositions.root!, content: { ...input.compositions.root!.content, tracks: input.compositions.root!.content.tracks.map(track => track.kind === 'audio' ? { ...track, locked: true } : track) } } } }
  const before = JSON.stringify(locked)
  expect(() => removeNativeClips(locked, 'root', ['picture', 'sound'])).toThrow('заблокирована')
  expect(() => removeNativeClips(input, 'root', ['picture', 'missing'])).toThrow('не найден')
  expect(() => removeNativeClips(input, 'root', ['picture', 'picture'])).toThrow()
  expect(JSON.stringify(locked)).toBe(before)
})
it('clears a surviving incoming audio crossfade when its predecessor is removed', () => {
  const input = project(), root = input.compositions.root!, audio = root.content.tracks.find(track => track.kind === 'audio')!
  if (audio.kind !== 'audio') throw new Error('audio fixture')
  const first = audio.clips[0]!
  const next = { ...first, id: 'next-sound', placement: { ...first.placement, timelineStartTick: 4000000, sourceInTick: 6000000, sourceOutTick: 10000000, speed: 1 }, crossfadeInTicks: 1000000 }
  const paired: NativeNestedProject = { ...input, compositions: { ...input.compositions, root: { ...root, content: { ...root.content, tracks: root.content.tracks.map(track => track.id === audio.id ? { ...audio, clips: [next, first] } : track) } } } }
  const output = removeNativeClips(paired, 'root', ['sound'])
  const remaining = output.compositions.root!.content.tracks.find(track => track.kind === 'audio')!
  expect(remaining.clips).toEqual([{ ...next, crossfadeInTicks: 0 }])
})
