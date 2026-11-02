import {expect,test} from 'vitest'
import oracle from '../../../../backend/tests/fixtures/filmcraft-transition-move.json'
import {createComposition,moveClip,moveClipPlacements,moveClipsToNewTrack} from './commands'
import type {Composition} from './types'
const second=1000000,ticks=254016000000n
for(const scenario of oracle.cases)test(`explicit transition source move: ${scenario.name}`,()=>{
 const endpoint=(value:number|null)=>value===null?null:String(value)
 const document:Composition={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),sources:{video:{id:'video',kind:'video',durationTicks:200*second,width:320,height:180,hasAudio:false}},tracks:[{id:'original',kind:'video',name:'Video',hidden:false,locked:false,muted:false,clips:scenario.before.items.map(item=>({id:String(item.id),kind:'video',sourceId:'video',timelineStartTicks:item.start*second,sourceInTicks:item.source_in*second,sourceOutTicks:(item.source_in+item.duration)*second,speed:1,opacity:1,sourceAudioEnabled:false,audioGain:1,transform:{x:0,y:0,width:320,height:180,fit:'contain'}})),transitions:scenario.before.transitions.map(item=>({id:String(item.id),kind:'dissolve',fromClipId:endpoint(item.from),toClipId:endpoint(item.to),durationTicks:0,timeline:{startTick:(BigInt(item.start)*ticks).toString(),durationTick:(BigInt(item.duration)*ticks).toString()}}))},{id:'destination',kind:'video',name:'Other',hidden:false,locked:false,muted:false,clips:[]}]}
 const snapshot=JSON.stringify(document)
 const cross=scenario.name==='move-pair-new-track'
 const placements=scenario.moves.map(([id])=>({clipId:String(id),trackId:cross?'destination':'original'}))
 const first=scenario.moves[0]!
 const result=moveClipPlacements(document,placements,String(first[0]),first[2]*second)
 const transitions=result.tracks.flatMap(track=>track.kind==='video'?track.transitions??[]:[])
 expect(transitions.map(item=>item.id)).toEqual(scenario.after.transitions.map(item=>String(item.id)))
 const actual=result.tracks.find(track=>track.id==='original')!.clips.map(item=>[Number(item.id),item.timelineStartTicks/second])
 expect(actual).toEqual(scenario.after.items.map(item=>[item.id,item.start]))
 if(placements.length===1){
  const single=moveClip(document,String(first[0]),'original',first[2]*second)
  expect(single.tracks.flatMap(track=>track.kind==='video'?track.transitions??[]:[])).toEqual(transitions)
 }
 const raised=moveClipsToNewTrack(document,placements.map(item=>item.clipId),'raised')
 expect(raised.tracks.flatMap(track=>track.kind==='video'?track.transitions??[]:[]).map(item=>item.id)).toEqual(scenario.after.transitions.map(item=>String(item.id)))
 expect(JSON.stringify(document)).toBe(snapshot)
})
