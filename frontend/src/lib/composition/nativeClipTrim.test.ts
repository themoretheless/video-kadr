import { splitNativeClips } from './nativeClipSplit'
import { expect, it } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type { NativeNestedProject } from '$lib/api'
import { trimNativeClipEnds, trimNativeClipStarts } from './nativeClipTrim'
import { editNativeTrack } from './nativeTrackEdit'
import { insertNativeMedia } from './nativeClipInsert'
const project = () => structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as NativeNestedProject
it('trims and extends both linked AV members, preserving metadata, animation and input', () => {
  const input = project(), before = JSON.stringify(input)
  expect(trimNativeClipEnds(input, 'new-timeline', ['auto-first'], 0)).toBe(input)
  for (const delta of [-1000000, 1000000]) {
    const output = trimNativeClipEnds(input, 'new-timeline', ['auto-first'], delta)
    const tracks = output.compositions['new-timeline']!.content.tracks
    for (const index of [1, 3]) expect(tracks[index]!.clips[0]).toMatchObject({ placement: { timelineStartTick: 2000000, sourceInTick: 1000000, sourceOutTick: delta < 0 ? 4000000 : 4000000 + delta, timelineDurationTick: 3000000 + delta } })
    expect(output.compositions['new-timeline']!.content.metadata).toBe(input.compositions['new-timeline']!.content.metadata)
    expect(output.compositions.root).toBe(input.compositions.root)
  }
  expect(JSON.stringify(input)).toBe(before)
})
it('preserves reverse source-out and frozen frames, and atomically refuses locks, overlaps and source limits', () => {
  const input = project(), value = JSON.parse(JSON.stringify(input))
  const video = value.compositions['new-timeline'].content.tracks[1].clips[0], audio = value.compositions['new-timeline'].content.tracks[3].clips[0]
  video.playbackMode = { mode: 'reverse' }; audio.reversed = true
  const reversed = trimNativeClipEnds(value, 'new-timeline', ['auto-first'], -1000000)
  for (const index of [1, 3]) expect(reversed.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toMatchObject({ placement: { sourceInTick: 1000000, sourceOutTick: 4000000 } })
  video.playbackMode = { mode: 'freeze', sourceTick: 2000000 }
  const frozen = trimNativeClipEnds(value, 'new-timeline', ['auto-first'], -1000000)
  expect(frozen.compositions['new-timeline']!.content.tracks[1]!.clips[0]).toMatchObject({ playbackMode: video.playbackMode, placement: { sourceInTick: 1000000, sourceOutTick: 4000000, timelineDurationTick: 2000000 } })
  const locked = editNativeTrack(input, 'new-timeline', { kind: 'flag', id: 'audio-2', flag: 'locked', value: true })
  expect(() => trimNativeClipEnds(locked, 'new-timeline', ['auto-first'], -1000000)).toThrow('заблокирована')
  expect(() => trimNativeClipEnds(input, 'new-timeline', ['auto-first'], 7000000)).toThrow('исходного')
  const occupied = insertNativeMedia(input, 'new-timeline', input.compositions['new-timeline']!.content.sources.source!, { trackId: 'video-2', clipId: 'next', startUs: 5000000, sourceInUs: 0, durationUs: 1000000 })
  expect(() => trimNativeClipEnds(occupied, 'new-timeline', ['auto-first'], 1000000)).toThrow('соседний')
  expect(() => trimNativeClipEnds(input, 'new-timeline', ['auto-first'], -3000000)).toThrow('диапазон')
})
it('slices a linked source-progress ramp without dropping authored fields', () => {
  const input = JSON.parse(JSON.stringify(project()))
  for (const index of [1, 3]) input.compositions['new-timeline'].content.tracks[index].clips[0].placement.speedRamp = { interpolation: 'linear', audioPolicy: 'preserve_pitch', points: [{ sourceProgressTick: 0, speed: 1 }, { sourceProgressTick: 3000000, speed: 2 }] }
  const output = trimNativeClipEnds(input, 'new-timeline', ['auto-first'], -500000)
  const video = output.compositions['new-timeline']!.content.tracks[1]!.clips[0]!, audio = output.compositions['new-timeline']!.content.tracks[3]!.clips[0]!
  expect('placement' in video && video.placement).toEqual('placement' in audio && audio.placement)
  expect('placement' in video && video.placement.sourceOutTick).toBe(4000000)
  expect('placement' in video && video.placement.speedRamp).toEqual(input.compositions['new-timeline'].content.tracks[1].clips[0].placement.speedRamp)
  expect(video).toMatchObject({ sourceAudioEnabled: false, effects: [] })
})
it('extends a retained-phase fragment using the full source clock instead of holding the old last frame', () => {
  const input = JSON.parse(JSON.stringify(project()))
  for (const index of [1, 3]) {
    const p = input.compositions['new-timeline'].content.tracks[index].clips[0].placement
    p.timelineStartTick = 3000000; p.sourceTimelineOffsetTick = 1000000
  }
  const output = trimNativeClipEnds(input, 'new-timeline', ['auto-first'], 500000)
  for (const index of [1, 3]) expect(output.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toMatchObject({ placement: { timelineStartTick: 3000000, sourceTimelineOffsetTick: 1000000, sourceInTick: 1000000, sourceOutTick: 4500000, timelineDurationTick: 2500000 } })
})

it('crops linked heads and restores the retained source phase without sliding the remaining pixels', () => {
  const input = project(), before = JSON.stringify(input)
  const cropped = trimNativeClipStarts(input, 'new-timeline', ['auto-first'], 750000)
  for (const index of [1, 3]) expect(cropped.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toMatchObject({ placement: { timelineStartTick: 2750000, timelineDurationTick: 2250000, sourceTimelineOffsetTick: 750000, sourceInTick: 1000000, sourceOutTick: 4000000 } })
  const restored = trimNativeClipStarts(cropped, 'new-timeline', ['auto-first'], -750000)
  for (const index of [1, 3]) expect(restored.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toMatchObject({ placement: { timelineStartTick: 2000000, timelineDurationTick: 3000000, sourceTimelineOffsetTick: 0 } })
  expect(JSON.stringify(input)).toBe(before)
  expect(() => trimNativeClipStarts(input, 'new-timeline', ['auto-first'], -1000001)).toThrow('исходного материала')
  const locked = editNativeTrack(input, 'new-timeline', { kind: 'flag', id: 'audio-2', flag: 'locked', value: true })
  expect(() => trimNativeClipStarts(locked, 'new-timeline', ['auto-first'], 750000)).toThrow('заблокирована')
})

it('extends before the retained origin and keeps one shared head window when splitting inside the extension', () => {
  const input = project(), before = JSON.stringify(input)
  const extended = trimNativeClipStarts(input, 'new-timeline', ['auto-first'], -250000)
  for (const index of [1, 3]) expect(extended.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toMatchObject({ placement: { timelineStartTick: 1750000, timelineDurationTick: 3250000, sourceTimelineOffsetTick: -250000, sourceHeadTimelineTick: 250000, sourceInTick: 1000000, sourceOutTick: 4000000 } })
  let serial = 0
  const split = splitNativeClips(extended, 'new-timeline', ['auto-first'], 1900000, () => `extended-${serial++}`)
  for (const index of [1, 3]) {
    const clips = split.compositions['new-timeline']!.content.tracks[index]!.clips
    expect(clips[0]).toMatchObject({ placement: { sourceTimelineOffsetTick: -250000, sourceHeadTimelineTick: 250000 } })
    expect(clips[1]).toMatchObject({ placement: { sourceTimelineOffsetTick: -100000, sourceHeadTimelineTick: 250000 } })
  }
  expect(JSON.stringify(input)).toBe(before)
})

it('retains same-file cutout identities when trimming the selected source window',()=>{
 const input=JSON.parse(JSON.stringify(project())),binding={subject:'person',sourceHash:'a'.repeat(64),modelHash:'88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',runtimeHash:'b'.repeat(64)}
 input.compositions['new-timeline'].content.tracks[1].clips[0].concatCutout=binding
 const output=trimNativeClipEnds(input,'new-timeline',['auto-first'],-1000000)
 expect(output.compositions['new-timeline']!.content.tracks[1]!.clips[0]).toMatchObject({concatCutout:binding})
})

it('preserves Concat EOF admission across a split after EOF and a tail extension, while strict clips still refuse',()=>{
 const value=JSON.parse(JSON.stringify(project())),node=value.compositions['new-timeline']
 for(const source of Object.values(node.content.sources) as {kind:string;durationTicks:number}[])if(source.kind!=='image')source.durationTicks=1000000
 for(const index of [1,3])node.content.tracks[index].clips[0].placement.concatSourceEof=true
 const input=value as NativeNestedProject
 const extended=trimNativeClipEnds(input,'new-timeline',['auto-first'],1000000)
 for(const index of [1,3])expect(extended.compositions['new-timeline']!.content.tracks[index]!.clips[0]).toMatchObject({placement:{concatSourceEof:true,sourceOutTick:5000000,timelineDurationTick:4000000}})
 let next=0;const split=splitNativeClips(input,'new-timeline',['auto-first'],4500000,()=>`tail-eof-${++next}`)
 for(const index of [1,3])expect(split.compositions['new-timeline']!.content.tracks[index]!.clips[1]).toMatchObject({placement:{concatSourceEof:true,sourceOutTick:4000000,sourceTimelineOffsetTick:2500000,timelineDurationTick:500000}})
 for(const index of [1,3])delete node.content.tracks[index].clips[0].placement.concatSourceEof
 expect(()=>trimNativeClipEnds(value as NativeNestedProject,'new-timeline',['auto-first'],1000000)).toThrow('Не хватает исходного материала')
})
