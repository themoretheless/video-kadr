import { expect, it } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type { NativeNestedProject } from '$lib/api'
import { replaceNativeMedia, setNativeMediaVersion } from './nativeMediaReplace'
import { editNativeTrack } from './nativeTrackEdit'
const project = () => structuredClone(fixture.nativeExtendedHeads[0]!.after) as unknown as NativeNestedProject
it('replaces a complete linked group while preserving clocks, effects, original sources and input', () => {
  const input = project(), before = JSON.stringify(input)
  const source = { ...input.compositions['new-timeline']!.content.sources.source!, id: 'replacement', vcodec: 'drop-wire-extra' }
  const output = replaceNativeMedia(input, 'new-timeline', ['auto-first'], source)
  for (const index of [1, 3]) {
    const old = input.compositions['new-timeline']!.content.tracks[index]!.clips[0]!
    expect(output.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toEqual({ ...old, sourceId: 'replacement' })
  }
  expect(output.compositions['new-timeline']!.content.sources.source).toBe(input.compositions['new-timeline']!.content.sources.source)
  expect(output.compositions['new-timeline']!.content.sources.replacement).not.toHaveProperty('vcodec')
  expect(output.compositions['new-timeline']!.content.metadata).toBe(input.compositions['new-timeline']!.content.metadata)
  expect(output.compositions.root).toBe(input.compositions.root)
  expect(JSON.stringify(input)).toBe(before)
  expect(replaceNativeMedia(input, 'new-timeline', ['auto-first'], input.compositions['new-timeline']!.content.sources.source!)).toBe(input)
})
it('rejects locked companions, wrong kinds, silent replacements and insufficient retained-head material', () => {
  const input = project(), source = { ...input.compositions['new-timeline']!.content.sources.source!, id: 'replacement' }
  const locked = editNativeTrack(input, 'new-timeline', { kind: 'flag', id: 'audio-2', flag: 'locked', value: true })
  expect(() => replaceNativeMedia(locked, 'new-timeline', ['auto-first'], source)).toThrow('заблокирована')
  expect(() => replaceNativeMedia(input, 'new-timeline', ['auto-first'], { ...source, hasAudio: false })).toThrow('несовместим')
  expect(() => replaceNativeMedia(input, 'new-timeline', ['auto-first'], { ...source, kind: 'image' })).toThrow('несовместим')
  expect(() => replaceNativeMedia(input, 'new-timeline', ['auto-first'], { ...source, durationTicks: 3999999 })).toThrow('короче')
  expect(() => replaceNativeMedia(input, 'new-timeline', ['missing'], source)).toThrow('не найден')
})

it('retains the first original across processed replacements and switches each linked member atomically', () => {
  const input = project(), source = input.compositions['new-timeline']!.content.sources.source!
  const first = replaceNativeMedia(input, 'new-timeline', ['auto-first'], { ...source, id: 'copy-one' }, true)
  const second = replaceNativeMedia(first, 'new-timeline', ['auto-first'], { ...source, id: 'copy-two' }, true)
  for (const id of ['auto-first', 'auto-first-audio']) expect(second.compositions['new-timeline']!.content.metadata!.mediaVersions![id]).toEqual({ originalSourceId: 'source', copySourceId: 'copy-two' })
  const restored = setNativeMediaVersion(second, 'new-timeline', ['auto-first'], false)
  for (const index of [1, 3]) expect(restored.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toEqual(input.compositions['new-timeline']!.content.tracks[index]!.clips[0])
  expect(restored.compositions['new-timeline']!.content.metadata).toBe(second.compositions['new-timeline']!.content.metadata)
  expect(setNativeMediaVersion(restored, 'new-timeline', ['auto-first'], false)).toBe(restored)
  const copied = setNativeMediaVersion(restored, 'new-timeline', ['auto-first'], true)
  expect(copied).toEqual(second)
  const locked = editNativeTrack(restored, 'new-timeline', { kind: 'flag', id: 'audio-2', flag: 'locked', value: true })
  expect(() => setNativeMediaVersion(locked, 'new-timeline', ['auto-first'], true)).toThrow('заблокирована')
  expect(() => setNativeMediaVersion(input, 'new-timeline', ['auto-first'], true)).toThrow('Не у всех')
})
it('restores distinct original sources for linked video and audio instead of assigning one source to both', () => {
  const input = JSON.parse(JSON.stringify(project()))
  const node = input.compositions['new-timeline']
  node.content.sources['audio-original'] = { ...node.content.sources.source, id: 'audio-original', kind: 'audio', width: 0, height: 0 }
  node.content.tracks[3].clips[0].sourceId = 'audio-original'
  const copied = replaceNativeMedia(input, 'new-timeline', ['auto-first'], { ...node.content.sources.source, id: 'new-copy' }, true)
  const restored = setNativeMediaVersion(copied, 'new-timeline', ['auto-first'], false)
  expect(restored.compositions['new-timeline']!.content.tracks[1]!.clips[0]).toMatchObject({ sourceId: 'source' })
  expect(restored.compositions['new-timeline']!.content.tracks[3]!.clips[0]).toMatchObject({ sourceId: 'audio-original' })
})
it('moves a linked source window by one delta and carries frozen source ticks with it', () => {
  const input = JSON.parse(JSON.stringify(project()))
  const node = input.compositions['new-timeline']
  node.content.tracks[1].clips[0].playbackMode = { mode: 'freeze', sourceTick: 2500000 }
  node.content.tracks[3].clips[0].placement.sourceInTick = 2000000
  node.content.tracks[3].clips[0].placement.sourceOutTick = 5000000
  const source = { ...node.content.sources.source, id: 'shifted-copy' }
  const before = JSON.stringify(input)
  const output = replaceNativeMedia(input, 'new-timeline', ['auto-first'], source, false, 2000000)
  const tracks = output.compositions['new-timeline']!.content.tracks
  expect(tracks[1]!.clips[0]).toMatchObject({ placement: { sourceInTick: 2000000, sourceOutTick: 5000000, sourceTimelineOffsetTick: -250000, sourceHeadTimelineTick: 250000 }, playbackMode: { mode: 'freeze', sourceTick: 3500000 } })
  expect(tracks[3]!.clips[0]).toMatchObject({ placement: { sourceInTick: 3000000, sourceOutTick: 6000000 } })
  expect(JSON.stringify(input)).toBe(before)
  expect(() => replaceNativeMedia(input, 'new-timeline', ['auto-first'], source, false, 9000000)).toThrow('короче')
  expect(() => replaceNativeMedia(input, 'new-timeline', ['auto-first'], source, false, NaN)).toThrow('начало')
})

it('drops source-owned video cutout when replacing a file and retains it for the same file',()=>{
 const input=JSON.parse(JSON.stringify(project())),node=input.compositions['new-timeline']
 const binding={subject:'person',sourceHash:'a'.repeat(64),modelHash:'88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',runtimeHash:'b'.repeat(64)}
 node.content.tracks[1].clips[0].concatCutout=binding;const before=JSON.stringify(input)
 expect(replaceNativeMedia(input,'new-timeline',['auto-first'],node.content.sources.source)).toBe(input)
 const changed=replaceNativeMedia(input,'new-timeline',['auto-first'],{...node.content.sources.source,id:'new-video'})
 expect(changed.compositions['new-timeline']!.content.tracks[1]!.clips[0]).not.toHaveProperty('concatCutout');expect(changed.compositions.root).toBe(input.compositions.root);expect(JSON.stringify(input)).toBe(before)
 const processed=replaceNativeMedia(input,'new-timeline',['auto-first'],{...node.content.sources.source,id:'processed'},true)
 expect(processed.compositions['new-timeline']!.content.tracks[1]!.clips[0]).not.toHaveProperty('concatCutout');expect(processed.compositions['new-timeline']!.content.metadata?.mediaVersions?.['auto-first']).toMatchObject({originalSourceId:'source',copySourceId:'processed'})
})
