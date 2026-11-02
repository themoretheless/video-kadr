import {expect,it} from 'vitest'
import {createComposition,addTrack,addClip} from './commands'
import {buildCompositionWireDocument} from './payload'
import {nativeScalarChannels,planNativeScalarKeyMove,nativeScalarSelectionOperations} from './nativeScalarTimeline'
import {applyNativeKeyframeOperation} from '$lib/agent/nativeKeyframes'
import type {NativeNestedProject} from '$lib/api'
function fixture():NativeNestedProject{
 let doc=createComposition({width:64,height:48,fps:4,backgroundColor:'#000000FF'})
 doc=addTrack(doc,{id:'text',kind:'text',name:'Text',locked:false,hidden:false,clips:[]})
 doc=addClip(doc,'text',{id:'title',kind:'text',text:'Keys',timelineStartTicks:1000000,durationTicks:2000000,x:0,y:15,opacity:1,style:{fontSizePx:24,color:'#FFFFFFFF',align:'center'}})
 const content=buildCompositionWireDocument(doc),clip=content.tracks[0]!.clips[0]!
 if(!('transform' in clip))throw new Error('Visual clip required')
 Object.assign(clip.transform,{x:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:0},{tick:750,value:20},{tick:1250,value:40}]}}})
 Object.assign(clip,{opacity:{mode:'keyframes',track:{timeBase:24,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:12,value:0.5},{tick:24,value:1}]}}})
 return {schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}}
}
it('moves mixed-clock selections from their leading edge and preserves independent authored grids',()=>{
 const project=fixture(),before=JSON.stringify(project),channels=nativeScalarChannels(project,'root'),x=channels.find(channel=>channel.property==='x')!,opacity=channels.find(channel=>channel.property==='opacity')!
 const selection=[...x.value.track.keyframes.map(key=>({channelId:x.id,tick:key.tick})),{channelId:opacity.id,tick:0}]
 const moved=planNativeScalarKeyMove(project,'root',selection,250000)
 expect(moved.deltaUs).toBe(250000);expect(moved.operations.map(op=>[op.tick,op.newTick])).toEqual([[1250,1750],[750,1250],[250,750],[0,6]])
 const result=nativeScalarChannels(moved.project,'root')
 expect(result.find(channel=>channel.id===x.id)!.value.track.sampleClock).toEqual(x.value.track.sampleClock)
 expect(result.find(channel=>channel.id===x.id)!.value.track.keyframes.map(key=>key.tick)).toEqual([750,1250,1750])
 expect(result.find(channel=>channel.id===opacity.id)!.value.track.keyframes.map(key=>key.tick)).toEqual([6,12,24])
 expect(JSON.stringify(project)).toBe(before)
 const back=planNativeScalarKeyMove(moved.project,'root',moved.selection,-250000)
 expect(back.operations.slice(0,3).map(op=>[op.tick,op.newTick])).toEqual([[750,250],[1250,750],[1750,1250]])
 expect(nativeScalarChannels(back.project,'root').find(channel=>channel.id===x.id)!.value.track.keyframes.map(key=>key.tick)).toEqual([250,750,1250])
})
it('refuses collisions with unselected keys and locked lanes without mutating the source',()=>{
 const project=fixture(),x=nativeScalarChannels(project,'root')[0]!,before=JSON.stringify(project)
 expect(()=>planNativeScalarKeyMove(project,'root',[{channelId:x.id,tick:750}],250000)).toThrow('уже есть другой ключ')
 expect(JSON.stringify(project)).toBe(before)
 Object.assign(project.compositions.root!.content.tracks[0]!,{locked:true})
 expect(()=>planNativeScalarKeyMove(project,'root',[{channelId:x.id,tick:750}],100000)).toThrow('заблокирована')
})
it('clamps at clip walls, snaps one group delta and preserves the source clock phase',()=>{
 const project=fixture(),x=nativeScalarChannels(project,'root')[0]!,selection=x.value.track.keyframes.map(key=>({channelId:x.id,tick:key.tick}))
 expect(planNativeScalarKeyMove(project,'root',selection,-999999).deltaUs).toBe(0)
 const moved=planNativeScalarKeyMove(project,'root',selection,9999999)
 expect(moved.deltaUs).toBe(1500000)
 expect(nativeScalarChannels(moved.project,'root')[0]!.value.track.keyframes.at(-1)!.tick).toBe(4250)
})
it('plans atomic preset/deletion operations across channels and excludes inactive XY under a path',()=>{
 const project=fixture(),channels=nativeScalarChannels(project,'root'),selection=channels.flatMap(channel=>channel.value.track.keyframes.map(key=>({channelId:channel.id,tick:key.tick})))
 const operations=nativeScalarSelectionOperations(project,'root',selection,'delete')
 let removed=project;for(const op of operations)removed=applyNativeKeyframeOperation(removed,'root',op)
 expect(nativeScalarChannels(removed,'root')).toEqual([])
 const preset=nativeScalarSelectionOperations(project,'root',selection,'preset','hold')
 expect(preset).toHaveLength(2);expect(preset[0]).toMatchObject({op:'set_keyframe_preset',ticks:[250,750,1250],preset:'hold'})
 const clip=project.compositions.root!.content.tracks[0]!.clips[0]!;if(!('transform' in clip))throw new Error('Visual required')
 Object.assign(clip.transform,{position:{mode:'path',path:{nodes:[]},progress:channels[0]!.value}})
 expect(nativeScalarChannels(project,'root').map(channel=>channel.property)).toEqual(['progress','opacity'])
})

it('scales mixed clocks around a composition anchor without overwriting selected old destinations',async()=>{
 const {planNativeScalarKeyScale}=await import('./nativeScalarTimeline')
 const project=fixture(),before=JSON.stringify(project),channels=nativeScalarChannels(project,'root'),selection=channels.flatMap(channel=>channel.value.track.keyframes.map(key=>({channelId:channel.id,tick:key.tick})))
 const scaled=planNativeScalarKeyScale(project,'root',selection,1000000,2)
 expect(scaled.factor).toBe(2)
 const result=nativeScalarChannels(scaled.project,'root')
 expect(result.find(channel=>channel.property==='x')!.value.track.keyframes.map(key=>key.tick)).toEqual([250,1250,2250])
 expect(result.find(channel=>channel.property==='opacity')!.value.track.keyframes.map(key=>key.tick)).toEqual([0,24,48])
 expect(result[0]!.value.track.sampleClock).toEqual(channels[0]!.value.track.sampleClock);expect(JSON.stringify(project)).toBe(before)
 expect(planNativeScalarKeyScale(project,'root',selection,1000000,100).factor).toBe(2)
 expect(()=>planNativeScalarKeyScale(project,'root',selection,1000000,-1)).toThrow('масштаб')
})
it('clamps compression around an anchor beyond the clip instead of moving keys outside the clip',async()=>{
 const {planNativeScalarKeyScale}=await import('./nativeScalarTimeline')
 const project=fixture();Object.assign(project.compositions.root!.content.canvas,{fpsMilli:30000})
 const x=nativeScalarChannels(project,'root')[0]!,selection=x.value.track.keyframes.map(key=>({channelId:x.id,tick:key.tick}))
 const result=planNativeScalarKeyScale(project,'root',selection,4000000,0.1)
 expect(result.factor).toBeCloseTo(0.4)
 const {keyframeLocalTime}=await import('./keyframeClock')
 for(const key of nativeScalarChannels(result.project,'root')[0]!.value.track.keyframes){const local=keyframeLocalTime(x.value.track,key.tick);expect(local).toBeGreaterThanOrEqual(0);expect(local).toBeLessThanOrEqual(2000000)}
})

it('refuses a scaled destination occupied by an unselected key atomically',async()=>{
 const {planNativeScalarKeyScale}=await import('./nativeScalarTimeline')
 const project=fixture(),before=JSON.stringify(project),x=nativeScalarChannels(project,'root')[0]!
 expect(()=>planNativeScalarKeyScale(project,'root',[{channelId:x.id,tick:250},{channelId:x.id,tick:750}],1000000,2)).toThrow('уже есть другой ключ')
 expect(JSON.stringify(project)).toBe(before)
})

it('keeps an off-frame authored anchor fixed while scaling the remaining clocked keys',async()=>{
 const {planNativeScalarKeyScale}=await import('./nativeScalarTimeline')
 const project=fixture();Object.assign(project.compositions.root!.content.canvas,{fpsMilli:30000})
 const x=nativeScalarChannels(project,'root')[0]!
 const scaled=planNativeScalarKeyScale(project,'root',[{channelId:x.id,tick:750},{channelId:x.id,tick:1250}],1250000,2)
 expect(scaled.selection.map(item=>item.tick)).toEqual([750,1783])
 expect(scaled.operations).toEqual([{op:'move_keyframe',clipId:'title',envelope:'visual',property:'x',tick:1250,newTick:1783}])
})

it('edits selected values as offset, multiply and set without changing authored clocks or unselected keys',async()=>{
 const {planNativeScalarKeyValues}=await import('./nativeScalarTimeline')
 const project=fixture(),before=JSON.stringify(project),channels=nativeScalarChannels(project,'root'),selection=[{channelId:channels[0]!.id,tick:750},{channelId:channels[1]!.id,tick:12}]
 const multiplied=planNativeScalarKeyValues(project,'root',selection,'multiply',0.5),result=nativeScalarChannels(multiplied.project,'root')
 expect(result[0]!.value.track.keyframes.map(key=>key.value)).toEqual([0,10,40]);expect(result[1]!.value.track.keyframes.map(key=>key.value)).toEqual([0,0.25,1]);expect(result[0]!.value.track.sampleClock).toEqual(channels[0]!.value.track.sampleClock)
 expect(planNativeScalarKeyValues(project,'root',selection,'offset',2).operations.map(op=>op.value)).toEqual([22,2.5]);expect(planNativeScalarKeyValues(project,'root',selection,'set',0).operations.map(op=>op.value)).toEqual([0,0])
 expect(planNativeScalarKeyValues(project,'root',selection,'offset',0).operations).toEqual([]);expect(()=>planNativeScalarKeyValues(project,'root',selection,'multiply',Number.MAX_VALUE)).toThrow('числовой диапазон');expect(JSON.stringify(project)).toBe(before)
})
it('plans a two-axis graph gesture as one complete candidate with collision-safe timing and new values',async()=>{
 const {planNativeScalarPointEdit}=await import('./nativeScalarTimeline')
 const project=fixture(),x=nativeScalarChannels(project,'root')[0]!,selection=x.value.track.keyframes.map(key=>({channelId:x.id,tick:key.tick})),plan=planNativeScalarPointEdit(project,'root',selection,250000,12)
 expect(plan.operations).toHaveLength(6);expect(nativeScalarChannels(plan.project,'root')[0]!.value.track.keyframes.map(key=>[key.tick,key.value])).toEqual([[750,12],[1250,32],[1750,52]])
 expect(()=>planNativeScalarPointEdit(project,'root',[{channelId:x.id,tick:750}],250000,12)).toThrow('уже есть другой ключ')
})

it('keeps negative authored times fixed during a vertical-only graph edit',async()=>{
 const {planNativeScalarPointEdit}=await import('./nativeScalarTimeline')
 const project=fixture(),clip=project.compositions.root!.content.tracks[0]!.clips[0]!;if(!('transform' in clip)||clip.transform.x.mode!=='keyframes')throw new Error('Curve required')
 Object.assign(clip.transform.x.track,{keyframes:[{tick:-250,value:0},{tick:750,value:20}]})
 const x=nativeScalarChannels(project,'root')[0]!,plan=planNativeScalarPointEdit(project,'root',[{channelId:x.id,tick:-250}],0,1)
 expect(plan.operations).toEqual([{op:'edit_keyframe',clipId:'title',envelope:'visual',property:'x',tick:-250,value:1}]);expect(plan.selection).toEqual([{channelId:x.id,tick:-250}])
})
