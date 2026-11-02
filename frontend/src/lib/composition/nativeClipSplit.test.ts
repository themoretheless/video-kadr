import { expect, it } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type { NativeNestedProject } from '$lib/api'
import { splitNativeClips } from './nativeClipSplit'
import { nativeClipEnd } from './nativeClipInsert'
import { editNativeTrack } from './nativeTrackEdit'
it('splits the stored AV group atomically while retaining full source windows and source phase', () => {
  const input = structuredClone(fixture.nativeFirstFree[0]!.after) as unknown as NativeNestedProject
  const before = JSON.stringify(input); let serial = 0
  const output = splitNativeClips(input, 'new-timeline', ['auto-first'], 3250000, () => `tail-${serial++}`)
  for (const index of [1, 3]) {
    const clips = output.compositions['new-timeline']!.content.tracks[index]!.clips
    expect(clips).toHaveLength(2)
    expect(clips[0]).toMatchObject({ placement: { timelineStartTick: 2000000, sourceInTick: 1000000, sourceOutTick: 4000000, timelineDurationTick: 1250000 } })
    expect(clips[1]).toMatchObject({ placement: { timelineStartTick: 3250000, sourceInTick: 1000000, sourceOutTick: 4000000, timelineDurationTick: 1750000, sourceTimelineOffsetTick: 1250000 } })
    expect(nativeClipEnd(clips[1]!)).toBe(5000000)
  }
  expect(output.compositions['new-timeline']!.content.metadata!.clipLinks!['auto-first-link']).toHaveLength(4)
  expect(JSON.stringify(input)).toBe(before)
  expect(splitNativeClips(input, 'new-timeline', ['auto-first'], 2000000, () => 'unused')).toBe(input)
  const locked = editNativeTrack(input, 'new-timeline', { kind: 'flag', id: 'audio-2', flag: 'locked', value: true })
  expect(() => splitNativeClips(locked, 'new-timeline', ['auto-first'], 3250000, () => 'fresh')).toThrow('заблокирована')
  expect(() => splitNativeClips(input, 'new-timeline', ['auto-first'], 3250000, () => 'auto-first')).toThrow('идентификатор')
})
it('keeps incoming fades on the head, outgoing fades on the tail and transfers an outgoing transition', () => {
  const input = JSON.parse(JSON.stringify(fixture.nativeFirstFree[0]!.after))
  const node = input.compositions['new-timeline']
  node.content.metadata.clipNames = { 'auto-first': 'Camera A' }
  node.content.tracks[3].clips[0].fadeInTicks = 6000000
  node.content.tracks[3].clips[0].fadeOutTicks = 7000000
  node.content.tracks[1].transitions = [{ id: 'fade-away', fromClipId: 'auto-first', kind: 'dissolve', durationTicks: 0, reverse: false, timeline: { startTick: String(4750000n*254016n), durationTick: String(250000n*254016n) } }]
  const curve = { interpolation: 'linear', audioPolicy: 'preserve_pitch', points: [{ sourceProgressTick: 0, speed: 1 }, { sourceProgressTick: 3000000, speed: 2 }] }
  for (const index of [1, 3]) node.content.tracks[index].clips[0].placement.speedRamp = curve
  let serial = 0
  const output = splitNativeClips(input, 'new-timeline', ['auto-first'], 3250000, () => `part-${serial++}`)
  const tracks = output.compositions['new-timeline']!.content.tracks
  expect(tracks[3]!.clips[0]).toMatchObject({ fadeInTicks: 6000000, fadeOutTicks: 0 })
  expect(tracks[3]!.clips[1]).toMatchObject({ fadeInTicks: 0, fadeOutTicks: 7000000, crossfadeInTicks: 0 })
  expect(tracks[1]).toMatchObject({ transitions: [{ fromClipId: 'part-0', timeline: node.content.tracks[1].transitions[0].timeline }] })
  expect(output.compositions['new-timeline']!.content.metadata!.clipNames!['part-0']).toBe('Camera A')
  for (const index of [1, 3]) for (const clip of tracks[index]!.clips) expect(clip).toMatchObject({ placement: { speedRamp: curve } })
})

it('retains the same-file cutout binding on both split sides',()=>{
 const input=JSON.parse(JSON.stringify(fixture.nativeFirstFree[0]!.after)),binding={subject:'person',sourceHash:'a'.repeat(64),modelHash:'88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',runtimeHash:'b'.repeat(64)}
 input.compositions['new-timeline'].content.tracks[1].clips[0].concatCutout=binding;let serial=0
 const output=splitNativeClips(input,'new-timeline',['auto-first'],3250000,()=>`cut-${serial++}`)
 for(const clip of output.compositions['new-timeline']!.content.tracks[1]!.clips)expect(clip).toMatchObject({concatCutout:binding})
})
