import { expect, test } from 'vitest'
import { createComposition } from './commands'
import { normalizeComposition, validateComposition, compositionDurationTicks, compositionClipCount } from './validation'
import { buildCompositionWireDocument, buildCompositionRenderRequest } from './payload'
import { assertCompositionLocksPreserved } from './locks'
import type { Composition } from './types'

test('caption-only composition survives save and render serialization with timing, style and markup', () => {
  const document: Composition = { ...createComposition({width:320,height:180,fps:24,backgroundColor:"#000000"}), captionTracks: [{ id:'captions',name:'Captions',style:{ color:'#abcdef80',size:42 },clips:[{id:'cue',timelineStartTick:3,timelineEndTick:2000003,text:'Привет <i>world</i>',speaker:'Narrator',settings:'line:90%'}] }] }
  const restored = normalizeComposition(JSON.parse(JSON.stringify(document)))
  expect(restored.captionTracks).toEqual(document.captionTracks)
  const wire = buildCompositionWireDocument(restored)
  expect(buildCompositionRenderRequest(restored).composition.captionTracks).toEqual(document.captionTracks)
  expect(wire.captionTracks).toEqual(document.captionTracks)
  expect(wire.captionTracks).not.toBe(restored.captionTracks)
  expect(compositionDurationTicks(restored)).toBe(2000003)
  expect(compositionClipCount(restored)).toBe(1)
  expect(validateComposition({...restored,captionTracks:[...restored.captionTracks!,...restored.captionTracks!]}).length).toBeGreaterThan(0)
  expect(validateComposition({...restored,captionTracks:[{...restored.captionTracks![0],style:{size:NaN}}]}).length).toBeGreaterThan(0)
})

test('locked caption track survives unrelated edits and requires unlocking before text changes', () => {
  const before:Composition={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),captionTracks:[{id:'captions',name:'Captions',locked:true,clips:[{id:'cue',timelineStartTick:0,timelineEndTick:2000000,text:'Keep'}]}]}
  expect(()=>assertCompositionLocksPreserved(before,{...before,metadata:{sequenceName:'Renamed'}})).not.toThrow()
  expect(()=>assertCompositionLocksPreserved(before,{...before,captionTracks:[{...before.captionTracks![0],clips:[]}]})).toThrow('locked')
  expect(()=>assertCompositionLocksPreserved(before,{...before,captionTracks:[{...before.captionTracks![0],locked:false}]})).not.toThrow()
})

test('caption tracks reject overlapping or reversed cue order', () => {
 const base=createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'})
 const cue={id:'one',timelineStartTick:0,timelineEndTick:2000000,text:'One'}
 for(const clips of [[cue,{...cue,id:'two',timelineStartTick:1000000,timelineEndTick:3000000}],[{...cue,id:'two',timelineStartTick:3000000,timelineEndTick:4000000},cue]]) {
  expect(validateComposition({...base,captionTracks:[{id:'captions',name:'Captions',clips}]}).some(issue=>issue.message.includes('non-overlapping'))).toBe(true)
 }
})

test('caption stream metadata and WebVTT identifiers survive save/render transport',()=>{
 const document:Composition={...createComposition({width:320,height:180,fps:25,backgroundColor:'#000000'}),captionTracks:[{id:'captions',name:'French',format:'CEA-608',language:'fr',syncLock:false,vttBlocks:['NOTE retained','STYLE\n::cue { color: yellow }'],clips:[{id:'cue',cueId:'file-cue-1',timelineStartTick:0,timelineEndTick:2000000,text:'Hello',speaker:'Ann',settings:'align:start'}]}]}
 const restored=normalizeComposition(JSON.parse(JSON.stringify(document)))
 expect(validateComposition(restored)).toEqual([])
 expect(buildCompositionWireDocument(restored).captionTracks).toEqual(document.captionTracks)
 expect(buildCompositionRenderRequest(restored).composition.captionTracks).toEqual(document.captionTracks)
 for(const patch of [{format:'invalid'},{language:42},{syncLock:'yes'},{vttBlocks:[42]},{clips:[{...document.captionTracks![0].clips[0],cueId:42}]}]) {
  const invalid={...document,captionTracks:[{...document.captionTracks![0],...patch}]} as unknown as Composition
  expect(validateComposition(invalid).length).toBeGreaterThan(0)
 }
})


test('caption visibility can change under lock while content remains protected', () => {
 const before:Composition={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),captionTracks:[{id:'captions',name:'Captions',locked:true,clips:[{id:'cue',timelineStartTick:0,timelineEndTick:2000000,text:'Keep'}]}]}
 const track=before.captionTracks![0]
 expect(()=>assertCompositionLocksPreserved(before,{...before,captionTracks:[{...track,hidden:true}]})).not.toThrow()
 expect(()=>assertCompositionLocksPreserved(before,{...before,captionTracks:[{...track,hidden:true,clips:[]}]})).toThrow('locked')
 expect(()=>assertCompositionLocksPreserved(before,{...before,captionTracks:[]})).toThrow('locked')
})

test('persists exact fractional source time and projects duration conservatively',()=>{
 const document:Composition={...createComposition({width:320,height:180,fps:24000/1001,fpsRate:{numerator:24000,denominator:1001},backgroundColor:'#000000'}),captionTracks:[{id:'captions',name:'Exact',clips:[{id:'cue',timelineStartTick:0,timelineEndTick:'137729592000',text:'Thirteen frames'}]}]}
 const restored=normalizeComposition(JSON.parse(JSON.stringify(document)))
 expect(restored.captionTracks![0].clips[0].timelineEndTick).toBe('137729592000')
 expect(buildCompositionWireDocument(restored).captionTracks).toEqual(document.captionTracks)
 expect(buildCompositionRenderRequest(restored).composition.captionTracks).toEqual(document.captionTracks)
 expect(compositionDurationTicks(restored)).toBe(542209)
 expect(validateComposition({...document,captionTracks:[{...document.captionTracks![0],clips:[{...document.captionTracks![0].clips[0],timelineStartTick:'-1'}]}]}).some(issue=>issue.code==='captions')).toBe(true)
})
