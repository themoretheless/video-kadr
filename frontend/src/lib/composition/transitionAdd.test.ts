import {expect,test} from 'vitest'
import oracle from '../../../../backend/tests/fixtures/filmcraft-transition-add.json'
import {createComposition,upsertTransition,setClipLocked} from './commands'
import type {Composition,CompositionTransition} from './types'
const ticks=254016000000n
const document=():Composition=>({...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),sources:{video:{id:'video',kind:'video',durationTicks:100000000,width:320,height:180,hasAudio:false}},tracks:[{id:'video-track',kind:'video',name:'Video',hidden:false,muted:false,locked:false,clips:[100,101,102,103].map((id,index)=>({id:String(id),kind:'video',sourceId:'video',timelineStartTicks:index*10000000,sourceInTicks:1000000,sourceOutTicks:6000000,speed:1,opacity:1,sourceAudioEnabled:false,audioGain:1,transform:{x:0,y:0,width:320,height:180,fit:'contain'}}))}]})
const convert=(item:{id:number;start:number;duration:number;from:number|null;to:number|null},id=item.id):CompositionTransition=>({id:String(id),kind:'dissolve',fromClipId:item.from===null?null:String(item.from),toClipId:item.to===null?null:String(item.to),durationTicks:0,timeline:{startTick:(BigInt(item.start)*ticks).toString(),durationTick:(BigInt(item.duration)*ticks).toString()}})
test('actual transition upsert matches 384 original overlap/nullable-side cases',()=>{
 expect(oracle.commit).toBe('5231852443363f001c3f6b396dd9b1e6461ae2be');expect(oracle.cases).toHaveLength(384)
 for(const scenario of oracle.cases){
  const before=upsertTransition(document(),'video-track',convert(scenario.before)),snapshot=JSON.stringify(before)
  const result=upsertTransition(before,'video-track',convert(scenario.added,scenario.allocated))
  const track=result.tracks[0]!
  expect(track.kind==='video'?track.transitions:[],scenario.name).toEqual(scenario.after.map(item=>convert(item)))
  expect(JSON.stringify(before)).toBe(snapshot)
 }
})
test('replacement cannot silently remove a transition attached to a locked clip',()=>{
 const original:CompositionTransition={id:'old',kind:'dissolve',fromClipId:'100',toClipId:'101',durationTicks:0,timeline:{startTick:'9',durationTick:'4'}}
 const before=setClipLocked(upsertTransition(document(),'video-track',original),'100',true),snapshot=JSON.stringify(before)
 expect(()=>upsertTransition(before,'video-track',{...original,id:'new',fromClipId:'102'})).toThrow('locked')
 expect(JSON.stringify(before)).toBe(snapshot)
})
test('one source tick overlap replaces while exact touching preserves both windows',()=>{
 const original:CompositionTransition={id:'old',kind:'dissolve',fromClipId:'100',toClipId:'101',durationTicks:0,timeline:{startTick:'9',durationTick:'4'}}
 const before=upsertTransition(document(),'video-track',original)
 for(const [start,count] of [['12',1],['13',2]] as const){
  const after=upsertTransition(before,'video-track',{...original,id:'new',timeline:{startTick:start,durationTick:'4'}})
  expect(after.tracks[0]!.kind==='video'&&after.tracks[0]!.transitions?.length).toBe(count)
 }
})

import {buildCompositionWireDocument} from './payload'
import {assertValidComposition,normalizeComposition} from './validation'
test('explicit reverse survives authoring persistence and native wire transport',()=>{
 const added:CompositionTransition={id:'reverse',fromClipId:'100',toClipId:null,durationTicks:0,kind:'dissolve',reverse:true,timeline:{startTick:'9',durationTick:'4'}}
 const after=upsertTransition(document(),'video-track',added)
 const restored=normalizeComposition(JSON.parse(JSON.stringify(after)))
 expect(restored.tracks[0]).toMatchObject({transitions:[added]})
 expect(buildCompositionWireDocument(restored).tracks[0]).toMatchObject({transitions:[added]})
 const invalid=JSON.parse(JSON.stringify(after));invalid.tracks[0].transitions[0].reverse='true'
 expect(()=>assertValidComposition(invalid)).toThrow()
})

test.each(['concat_dissolve','concat_fade_black','concat_fade_white','film_dissolve','additive_dissolve','fade_white'] as const)('%s survives authoring persistence and native wire with exact independent clock',kind=>{
 const added:CompositionTransition={id:'family',fromClipId:'100',toClipId:null,durationTicks:0,kind,reverse:true,timeline:{startTick:'9',durationTick:'4'}}
 const after=upsertTransition(document(),'video-track',added)
 const restored=normalizeComposition(JSON.parse(JSON.stringify(after)))
 expect(restored.tracks[0]).toMatchObject({transitions:[added]})
 expect(buildCompositionWireDocument(restored).tracks[0]).toMatchObject({transitions:[added]})
 const invalid=JSON.parse(JSON.stringify(after));delete invalid.tracks[0].transitions[0].timeline;invalid.tracks[0].transitions[0].durationTicks=100000
 expect(()=>assertValidComposition(invalid)).toThrow()
})
