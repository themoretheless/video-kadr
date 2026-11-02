import {expect,test} from 'vitest'
import oracle from '../../../../backend/tests/fixtures/filmcraft-media-insert.json'
import {createComposition} from './commands'
import {insertCompositionClips} from './globalInsert'
import {clipEndTicks,type Composition,type TextClip} from './types'
const second=1_000_000
const text=(id:string,start:number,duration:number):TextClip=>({id,kind:'text',timelineStartTicks:start*second,durationTicks:duration*second,text:'media',x:0,y:0,opacity:1,style:{fontSizePx:24,color:'#ffffffff',align:'center'}})
for(const scenario of oracle.cases)test(`global insert original placement/link policy: ${scenario.name}`,()=>{
 const before:Composition={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),tracks:scenario.before.map(track=>({id:String(track.id),kind:'text',name:track.name,locked:track.locked,syncLock:track.sync_lock,hidden:false,clips:track.items.map(item=>text(String(item.id),item.start,item.duration))})),links:[{id:'original-link',memberIds:scenario.before.flatMap(track=>track.items.map(item=>String(item.id)))}],captionTracks:[{id:'captions',name:'Subtitle',clips:[{id:'cue',timelineStartTick:second,timelineEndTick:6*second,text:'Caption',cueId:'file1'}]}]}
 const frozen=JSON.stringify(before)
 let id=0
 const result=insertCompositionClips(before,[{trackId:String(scenario.target),clip:text('inserted',3,2)}],()=>`new-${id++}`)
 expect(JSON.stringify(before)).toBe(frozen)
 const spans=(clips:readonly {timelineStartTicks:number}[])=>clips.map(clip=>[clip.timelineStartTicks,clipEndTicks(clip as TextClip)]).sort((a,b)=>a[0]!-b[0]!)
 for(const [index,track] of result.tracks.entries())expect(spans(track.clips)).toEqual(scenario.after[index]!.items.map(item=>[item.start*second,(item.start+item.duration)*second]).sort((a,b)=>a[0]!-b[0]!))
 const upstreamRight=scenario.after.flatMap(track=>track.items.filter(item=>item.id>=2000&&item.id<100000))
 const freshLinks=result.links!.filter(link=>link.id!=='original-link')
 expect(freshLinks.map(link=>link.memberIds.length)).toEqual(upstreamRight.length>=2?[upstreamRight.length]:[])
 expect(result.links![0]!.memberIds).toEqual(before.links![0]!.memberIds)
 expect(result.captionTracks![0]!.clips.map(cue=>[cue.timelineStartTick,cue.timelineEndTick,cue.cueId])).toEqual([[second,3*second,'file1'],[5*second,8*second,null]])
})
test('insertion rejects ID collisions and locked targets without touching input',()=>{
 const before:Composition={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),tracks:[{id:'track',kind:'text',name:'Text',hidden:false,locked:false,clips:[text('left',0,10)]}]}
 const frozen=JSON.stringify(before)
 expect(()=>insertCompositionClips(before,[{trackId:'track',clip:text('new',3,2)}],()=> 'left')).toThrow()
 expect(()=>insertCompositionClips({...before,tracks:[{...before.tracks[0]!,locked:true}]},[{trackId:'track',clip:text('new',3,2)}],()=> 'right')).toThrow('locked')
 expect(JSON.stringify(before)).toBe(frozen)
})

for(const scenario of oracle.cases.filter(scenario=>scenario.name!=='frame-hold'))test(`global insert original video/audio source ranges: ${scenario.name}`,()=>{
 const convert=(item:{id:number;start:number;duration:number;source_in:number;speed:number;reverse:boolean},audio:boolean)=>{
  const timing={id:String(item.id),sourceId:audio?'audio':'video',timelineStartTicks:item.start*second,sourceInTicks:item.source_in*second,sourceOutTicks:(item.source_in+item.duration*Math.abs(item.speed))*second,speed:Math.abs(item.speed)}
  return audio?{...timing,kind:'audio' as const,gain:1,pan:0,reversed:item.reverse}:{...timing,kind:'video' as const,transform:{x:0,y:0,width:320,height:180,fit:'contain' as const},opacity:1,sourceAudioEnabled:true,audioGain:1,playbackMode:{mode:item.reverse?'reverse' as const:'forward' as const}}
 }
 const before:Composition={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),sources:{video:{id:'video',kind:'video',durationTicks:200*second,width:320,height:180,hasAudio:true},audio:{id:'audio',kind:'audio',durationTicks:200*second,width:0,height:0,hasAudio:true}},tracks:scenario.before.map(track=>({id:String(track.id),kind:track.kind==='Audio'?'audio':'video',name:track.name,locked:track.locked,syncLock:track.sync_lock,hidden:false,muted:false,clips:track.items.map(item=>convert(item,track.kind==='Audio'))})) as Composition['tracks']}
 let id=0
 const placement=convert(scenario.after.flatMap(track=>track.items).find(item=>item.id===100000)!,false)
 const result=insertCompositionClips(before,[{trackId:String(scenario.target),clip:placement}],()=>`media-${id++}`)
 for(const [index,track] of result.tracks.entries()){
  const actual=track.clips.map(clip=>{if(clip.kind!=='audio'&&clip.kind!=='video')throw new Error('media required');return [clip.timelineStartTicks,clip.sourceInTicks,clip.sourceOutTicks]}).sort((a,b)=>a[0]!-b[0]!)
  const expected=scenario.after[index]!.items.map(item=>[item.start*second,item.source_in*second,(item.source_in+item.duration*Math.abs(item.speed))*second]).sort((a,b)=>a[0]!-b[0]!)
  expect(actual).toEqual(expected)
 }
})

import transitionOracle from '../../../../backend/tests/fixtures/filmcraft-transition-insert.json'
import {transitionSourceSpan} from './transitionTimeline'
import {buildCompositionWireDocument} from './payload'
import {assertValidComposition,normalizeComposition} from './validation'
for(const scenario of transitionOracle.cases)test(`global insert explicit native transition: ${scenario.name}`,()=>{
 const sourceTicks=254016000000n
 const clip=(id:string,start:number,duration:number)=>({id,kind:'video' as const,sourceId:'video',timelineStartTicks:start*second,sourceInTicks:100*second,sourceOutTicks:(100+duration)*second,speed:1,transform:{x:0,y:0,width:320,height:180,fit:'contain' as const},opacity:1,sourceAudioEnabled:true,audioGain:1})
 const endpoint=(value:number|null)=>value===null?null:String(value)
 const before:Composition={...createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}),sources:{video:{id:'video',kind:'video',durationTicks:200*second,width:320,height:180,hasAudio:true}},tracks:[{id:'video-track',kind:'video',name:'Video',hidden:false,muted:false,locked:false,clips:scenario.before.items.map(item=>clip(String(item.id),item.start,item.duration)),transitions:scenario.before.transitions.map(item=>({id:String(item.id),fromClipId:endpoint(item.from),toClipId:endpoint(item.to),durationTicks:0,kind:'dissolve',timeline:{startTick:(BigInt(item.start)*sourceTicks).toString(),durationTick:(BigInt(item.duration)*sourceTicks).toString()}}))}]}
 assertValidComposition(before)
 const raw=JSON.parse(JSON.stringify(before))
 for(const transition of raw.tracks[0].transitions)delete transition.durationTicks
 expect(normalizeComposition(raw)).toMatchObject({tracks:[{transitions:before.tracks[0]!.kind==='video'?before.tracks[0]!.transitions:[]}]})
 const malformed=JSON.parse(JSON.stringify(before));malformed.tracks[0].transitions[0].durationTicks=1
 expect(()=>assertValidComposition(malformed)).toThrow()
 expect(buildCompositionWireDocument(before).tracks[0]).toMatchObject({transitions:before.tracks[0]!.kind==='video'?before.tracks[0]!.transitions:[]})
 const frozen=JSON.stringify(before)
 let nextId=2000
 const after=insertCompositionClips(before,[{trackId:'video-track',clip:clip('inserted',scenario.at,scenario.duration)}],()=>String(nextId++))
 expect(JSON.stringify(before)).toBe(frozen)
 const track=after.tracks[0]!
 if(track.kind!=='video')throw new Error('Video required')
 expect((track.transitions??[]).map(item=>({from:item.fromClipId,to:item.toClipId,span:transitionSourceSpan(item.timeline!)}))).toEqual(scenario.after.transitions.map(item=>({from:endpoint(item.from),to:endpoint(item.to),span:[BigInt(item.start)*sourceTicks,BigInt(item.duration)*sourceTicks]})))
 assertValidComposition(JSON.parse(JSON.stringify(after)))
})
