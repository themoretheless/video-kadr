import { MAX_COMPOSITION_DURATION_TICKS } from './types'
import { expect, it } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import { prepareNativeClipCopy, finishNativeClipCopy } from './nativeClipCopy'
import type { NativeNestedProject } from '$lib/api'
it('stages independent linked copies while retaining originals, versions and the source document', () => {
  const input=structuredClone(fixture.nativeCrossCopies[13]!.before) as unknown as NativeNestedProject
  const snapshot=JSON.stringify(input);let serial=0
  const plan=prepareNativeClipCopy(input,'new-timeline','copy-destination',['auto-first'],8000000,{},()=>`fresh-${serial++}`)
  expect(plan.staged.compositions['new-timeline']).toBe(input.compositions['new-timeline'])
  expect(plan.copies).toHaveLength(2)
  expect(plan.moves).toHaveLength(2)
  expect(plan.moves.every(move=>move.startUs===8000000 && move.createTrack)).toBe(true)
  const node=plan.staged.compositions[plan.temporary]!
  expect(node.content.tracks.flatMap(track=>track.clips.map(clip=>clip.id)).sort()).toEqual([...plan.copies].sort())
  expect(Object.keys(node.content.metadata!.mediaVersions!)).toEqual(plan.copies)
  expect(Object.values(node.content.metadata!.clipLinks!)).toEqual([plan.copies])
  expect(JSON.stringify(input)).toBe(snapshot)
  const finished=finishNativeClipCopy({project:plan.staged,changed:true,affectedCompositions:[],movedClipIds:plan.copies},plan.temporary,input,'copy-destination')
  expect(finished.compositions[plan.temporary]).toBeUndefined()
  expect(finished.compositions['new-timeline']).toBe(input.compositions['new-timeline'])
})
it('refuses invalid destinations and preserves selected control-layer dependencies in staging',()=>{
  const test=fixture.nativeCrossCopies[18]!,input=structuredClone(test.before) as unknown as NativeNestedProject
  let serial=0
  expect(()=>prepareNativeClipCopy(input,test.from,test.from,['picture'],0,{},()=>`refused-${serial++}`)).toThrow('другой')
  expect(()=>prepareNativeClipCopy(input,test.from,test.to,['picture'],0,{},()=>`refused-${serial++}`)).toThrow('дорожки')
  const plan=prepareNativeClipCopy(input,test.from,test.to,['picture','control-clip'],0,{},()=>`accepted-${serial++}`)
  expect(plan.copies).toHaveLength(2)
  expect(plan.moves).toHaveLength(2)
})
it('copies from the end of the allowed source timeline without needing room after the original',()=>{
  const test=fixture.nativeCrossCopies[0]!, input=structuredClone(test.before) as unknown as NativeNestedProject
  const node=input.compositions[test.from]!
  input.compositions[test.from]={...node,content:{...node.content,tracks:node.content.tracks.map(track=>({...track,clips:track.clips.map(clip=>'placement' in clip?{...clip,placement:{...clip.placement,timelineStartTick:MAX_COMPOSITION_DURATION_TICKS-3000000}}:clip)})) as typeof node.content.tracks}}
  let serial=0
  const plan=prepareNativeClipCopy(input,test.from,test.to,['auto-first'],0,{},()=>`late-${serial++}`)
  expect(plan.moves.every(move=>move.startUs===0)).toBe(true)
})
