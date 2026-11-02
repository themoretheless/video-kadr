import {expect,test} from 'vitest'
import oracle from '../../../../backend/tests/fixtures/filmcraft-transition-split.json'
import {createComposition,splitClip} from './commands'
import type {Composition} from './types'
const second=1000000,ticks=254016000000n
for(const scenario of oracle.cases)test(`explicit transition source split: ${scenario.name}`,()=>{
 const endpoint=(value:number|null)=>value===null?null:String(value)
 const document:Composition={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),sources:{video:{id:'video',kind:'video',durationTicks:200*second,width:320,height:180,hasAudio:false}},tracks:[{id:'original',kind:'video',name:'Video',hidden:false,locked:false,muted:false,clips:scenario.before.items.map(item=>({id:String(item.id),kind:'video',sourceId:'video',timelineStartTicks:item.start*second,sourceInTicks:item.source_in*second,sourceOutTicks:(item.source_in+item.duration)*second,speed:1,opacity:1,sourceAudioEnabled:false,audioGain:1,transform:{x:0,y:0,width:320,height:180,fit:'contain'}})),transitions:scenario.before.transitions.map(item=>({id:String(item.id),kind:'dissolve',fromClipId:endpoint(item.from),toClipId:endpoint(item.to),durationTicks:0,timeline:{startTick:(BigInt(item.start)*ticks).toString(),durationTick:(BigInt(item.duration)*ticks).toString()}}))},{id:'destination',kind:'video',name:'Other',hidden:false,locked:false,muted:false,clips:[]}]}
 const snapshot=JSON.stringify(document)
 const crossing=scenario.before.items.find(item=>item.start<scenario.at&&scenario.at<item.start+item.duration)
 if(!crossing){
  const boundary=scenario.before.items.find(item=>item.start===scenario.at||item.start+item.duration===scenario.at)!
  expect(()=>splitClip(document,String(boundary.id),scenario.at*second,'2000',true)).toThrow()
 } else {
  const result=splitClip(document,String(crossing.id),scenario.at*second,String(scenario.right),true)
  const track=result.tracks[0]!
  if(track.kind!=='video')throw new Error('Video required')
  expect((track.transitions??[]).map(item=>({id:item.id,from:item.fromClipId,to:item.toClipId,timeline:item.timeline}))).toEqual(scenario.after.transitions.map(item=>({id:String(item.id),from:endpoint(item.from),to:endpoint(item.to),timeline:{startTick:(BigInt(item.start)*ticks).toString(),durationTick:(BigInt(item.duration)*ticks).toString()}})))
  expect(track.clips.map(item=>[Number(item.id),item.timelineStartTicks/second,item.sourceInTicks/second,item.sourceOutTicks/second])).toEqual(scenario.after.items.map(item=>[item.id,item.start,item.source_in,item.source_in+item.duration]))
 }
 expect(JSON.stringify(document)).toBe(snapshot)
})

import {splitTransitionWindow} from './transitionTimeline'
test('split uses exact source ticks at fade boundaries and preserves authored wire time',()=>{
 const fade={fromClipId:'a',toClipId:null,timeline:{startTick:'254016',durationTick:'2'},metadata:'preserved'}
 expect(splitTransitionWindow(fade,'254016','a','right')).toEqual({...fade,fromClipId:'right'})
 expect(splitTransitionWindow(fade,'254017','a','right')).toBeNull()
 expect(splitTransitionWindow(fade,'254018','a','right')).toBe(fade)
 const paired={...fade,toClipId:'b'}
 expect(splitTransitionWindow(paired,'254017','a','right')).toBe(paired)
 const incoming={...fade,fromClipId:null,toClipId:'a'}
 expect(splitTransitionWindow(incoming,'254016','a','right')).toBe(incoming)
 expect(splitTransitionWindow(incoming,'254017','a','right')).toBeNull()
})

import {resetCompositionForTests,addMediaInfoToComposition,compositionState,selectCompositionClip,setCompositionPlayhead,splitSelectedCompositionClip,undoComposition,redoComposition} from '../state/composition.svelte'
test('editor split removes a straddling fade in one undoable transaction',()=>{
 resetCompositionForTests()
 addMediaInfoToComposition({id:'split-source',url:'/files/source.mp4',filename:'source.mp4',mediaType:'video',duration:8,width:320,height:180,acodec:'aac'})
 const track=compositionState.document.tracks[0]!
 if(track.kind!=='video')throw new Error('Video required')
 const clip=track.clips[0]!
 compositionState.document={...compositionState.document,tracks:compositionState.document.tracks.map(lane=>lane.id===track.id?{...track,transitions:[{id:'fade',fromClipId:clip.id,toClipId:null,durationTicks:0,kind:'dissolve',timeline:{startTick:'508032000001',durationTick:'1016064000000'}}]}:lane)}
 const before=JSON.stringify(compositionState.document),depth=compositionState.history.past.length
 selectCompositionClip(track.id,clip.id);setCompositionPlayhead(3000000)
 expect(splitSelectedCompositionClip()).not.toBeNull()
 expect(compositionState.history.past).toHaveLength(depth+1)
 expect(compositionState.document.tracks[0]).toMatchObject({transitions:[]})
 const after=JSON.stringify(compositionState.document)
 undoComposition();expect(JSON.stringify(compositionState.document)).toBe(before)
 redoComposition();expect(JSON.stringify(compositionState.document)).toBe(after)
})
