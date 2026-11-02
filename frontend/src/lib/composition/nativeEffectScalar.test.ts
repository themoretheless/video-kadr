import {expect,it} from 'vitest'
import {effectScalarProject,effectColorScalarProject,effectLumetriScalarProject} from './fixtures/nativeEffectScalarProject'
import {nativeEffectScalarBindings} from './nativeEffectScalar'
import {nativeScalarChannels,planNativeScalarKeyMove,planNativeScalarKeyValues,planNativeScalarKeyScale} from './nativeScalarTimeline'
import {applyNativeKeyframeOperation,readNativeKeyframes} from '$lib/agent/nativeKeyframes'
import {keyframeLocalTime} from './keyframeClock'
import {captureNativeScalarClipboard,planNativeScalarPaste,parseNativeScalarClipboard,serializeNativeScalarClipboard} from './nativeScalarClipboard'
const clip=(project:ReturnType<typeof effectScalarProject>)=>project.compositions.root!.content.tracks[0]!.clips[0]!
const keys=(project:ReturnType<typeof effectScalarProject>)=>nativeScalarChannels(project,'root')[0]!
it('addresses duplicate effect kinds by index and retains authored clocks through time/value edits',()=>{
 const source=effectScalarProject(),before=JSON.stringify(source),channel=keys(source),selection=channel.value.track.keyframes.map(key=>({channelId:channel.id,tick:key.tick}))
 expect(channel).toMatchObject({envelope:'effect',effectIndex:0,effectKind:'concat_exposure',property:'stops'});expect(nativeEffectScalarBindings(clip(source))).toHaveLength(2)
 const moved=planNativeScalarKeyMove(source,'root',selection,250000),changed=planNativeScalarKeyValues(moved.project,'root',moved.selection,'offset',-.5)
 expect(moved.operations.every(op=>op.effectIndex===0&&op.effectKind==='concat_exposure')).toBe(true);expect(keys(changed.project).value.track.keyframes.map(key=>[key.tick,key.value])).toEqual([[750,-2.5],[1250,-1.5],[1750,-.5]])
 expect(nativeEffectScalarBindings(clip(changed.project))[1]!.value).toEqual({mode:'constant',value:0});expect(keys(changed.project).value.track.sampleClock).toEqual(channel.value.track.sampleClock);expect(JSON.stringify(source)).toBe(before)
 expect(()=>applyNativeKeyframeOperation(source,'root',{op:'edit_keyframe',clipId:'shape',envelope:'effect',effectIndex:0,effectKind:'concat_contrast',property:'stops',tick:250,value:0})).toThrow('not found')
})
it('projects treatment absolute time and graphic trim offsets without rewriting the source clock',()=>{
 for(const [extra,origin] of [[{animationOffsetUs:250000},750000],[{concatTreatment:{strength:1,fadeIn:0,fadeOut:0,durationTicks:2000000}},0]] as const){
  const source=effectScalarProject();Object.assign(clip(source),extra);const channel=keys(source),selection=channel.value.track.keyframes.map(key=>({channelId:channel.id,tick:key.tick}))
  expect(channel.start+keyframeLocalTime(channel.value.track,250)).toBe(origin)
  const value=planNativeScalarKeyValues(source,'root',selection,'offset',-.25);expect(nativeEffectScalarBindings(clip(value.project))[0]!.value).toMatchObject({track:{sampleClock:{rateNum:2,rateDen:1,offsetNum:250000}}})
  const inspected=readNativeKeyframes(source,'root',{clipId:'shape',envelope:'effect',property:'stops',effectIndex:0,effectKind:'concat_exposure'});expect(inspected).toMatchObject({keyOriginUs:origin,keyframes:[{t_us:origin},{t_us:origin+250000},{t_us:origin+500000}]})
 }
})
it('edits named animation channels and refuses inactive original-source runs or locked tracks',()=>{
 const source=effectScalarProject();Object.assign(clip(source),{effects:[{kind:'concat_white_balance',parameters:{},animation:{temperature:keys(source).value,tint:{mode:'constant',value:0}}}]});expect(keys(source).property).toBe('temperature')
 Object.assign(clip(source),{effects:[{kind:'concat_exposure',parameters:{},animation:keys(source).value,concatKeys:[{at:0,value:1}]}]});expect(nativeScalarChannels(source,'root')).toEqual([])
 const locked=effectScalarProject();Object.assign(locked.compositions.root!.content.tracks[0]!,{locked:true});expect(()=>planNativeScalarKeyValues(locked,'root',[{channelId:keys(locked).id,tick:250}],'set',0)).toThrow('заблокирована')
})
it('copies and pastes exact effect addresses while preserving projected clocks and rejecting an incompatible target',()=>{
 const source=effectScalarProject();Object.assign(clip(source),{animationOffsetUs:250000});const channel=keys(source),clipboard=captureNativeScalarClipboard(source,'root',[{channelId:channel.id,tick:750}])
 expect(parseNativeScalarClipboard(serializeNativeScalarClipboard(clipboard))).toEqual(clipboard)
 const pasted=planNativeScalarPaste(source,'root',clipboard,'shape',1750000);expect(pasted.selection).toEqual([{channelId:channel.id,tick:2250}]);expect(nativeEffectScalarBindings(clip(pasted.project))[0]!.value).toMatchObject({track:{sampleClock:{rateNum:2,rateDen:1,offsetNum:250000}}})
 Object.assign(clip(source),{effects:[]});expect(()=>planNativeScalarPaste(source,'root',clipboard,'shape',1500000)).toThrow('соответствующего')
})
it('scales effect selections against the actual clip walls after clock projection',()=>{
 const source=effectScalarProject();Object.assign(clip(source),{animationOffsetUs:250000});const channel=keys(source),selected=[750,1250].map(tick=>({channelId:channel.id,tick}))
 const result=planNativeScalarKeyScale(source,'root',selected,1000000,2);expect(result.selection.map(key=>key.tick)).toEqual([750,1750]);expect(nativeEffectScalarBindings(clip(result.project))[0]!.value).toMatchObject({track:{sampleClock:{rateNum:2,rateDen:1,offsetNum:250000}}})
})

it('keeps effect color components separate from amount and unrelated component envelopes',()=>{
 const source=effectColorScalarProject(),channel=keys(source),selection=channel.value.track.keyframes.map(key=>({channelId:channel.id,tick:key.tick})),before=JSON.stringify(source)
 expect(channel).toMatchObject({effectKind:'color_tint',property:'colorAnimation.whiteR'});expect(nativeEffectScalarBindings(clip(source)).map(field=>field.property)).toEqual(['amount','colorAnimation.whiteR','colorAnimation.whiteA','stops'])
 const changed=planNativeScalarKeyValues(source,'root',selection,'offset',-.25),bindings=nativeEffectScalarBindings(clip(changed.project))
 expect(bindings.find(field=>field.property==='colorAnimation.whiteR')!.value).toMatchObject({track:{sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{value:0},{value:.25},{value:.5}]}})
 expect(bindings.find(field=>field.property==='amount')!.value).toEqual({mode:'constant',value:100});expect(bindings.find(field=>field.property==='colorAnimation.whiteA')!.value).toEqual({mode:'constant',value:1});expect(JSON.stringify(source)).toBe(before)
})
it('round-trips color-component clipboard addresses and moves them on their original authored grid',()=>{
 const source=effectColorScalarProject(),channel=keys(source),selected=[{channelId:channel.id,tick:250}],clipboard=captureNativeScalarClipboard(source,'root',selected)
 expect(JSON.parse(clipboard.groups[0]!.parameter)).toEqual(['effect','colorAnimation.whiteR',0,'color_tint'])
 const pasted=planNativeScalarPaste(source,'root',parseNativeScalarClipboard(serializeNativeScalarClipboard(clipboard)),'shape',1750000);expect(pasted.selection).toEqual([{channelId:channel.id,tick:1750}])
 const moved=planNativeScalarKeyMove(source,'root',channel.value.track.keyframes.map(key=>({channelId:channel.id,tick:key.tick})),250000);expect(keys(moved.project).value.track.keyframes.map(key=>key.tick)).toEqual([750,1250,1750]);expect(keys(moved.project).value.track.sampleClock).toEqual(channel.value.track.sampleClock)
})

it('enumerates every authored Lumetri section with qualified paths and immutable sibling isolation',()=>{
 const source=effectLumetriScalarProject(),value=keys(source).value,copy=JSON.stringify(source)
 expect(keys(source)).toMatchObject({effectKind:'lumetri',property:'lumetri.basic.exposure'})
 const selected=value.track.keyframes.map(key=>({channelId:keys(source).id,tick:key.tick})),edited=planNativeScalarKeyValues(source,'root',selected,'offset',-1),bindings=nativeEffectScalarBindings(clip(edited.project))
 expect(bindings.find(binding=>binding.property==='lumetri.basic.exposure')!.value).toMatchObject({track:{sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{value:-3},{value:-2},{value:-1}]}});expect(bindings.find(binding=>binding.property==='lumetri.grade.shadowsX')!.value).toEqual({mode:'constant',value:0});expect(JSON.stringify(source)).toBe(copy)
 Object.assign(clip(source),{effects:[{kind:'lumetri',parameters:{animation:{basic:{exposure:value},creative:{fadedFilm:value},vignette:{amount:value},grade:{shadowsX:value},secondary:{temperature:value},sharpen:value}}}]})
 expect(nativeScalarChannels(source,'root').map(channel=>channel.property)).toEqual(['lumetri.basic.exposure','lumetri.creative.fadedFilm','lumetri.vignette.amount','lumetri.grade.shadowsX','lumetri.secondary.temperature','lumetri.sharpen'])
})
it('retimes and copies nested Lumetri channels without rewriting their authored clocks or section switches',()=>{
 const source=effectLumetriScalarProject(),channel=keys(source),selected=channel.value.track.keyframes.map(key=>({channelId:channel.id,tick:key.tick})),moved=planNativeScalarKeyMove(source,'root',selected,250000)
 expect(keys(moved.project).value.track.keyframes.map(key=>key.tick)).toEqual([750,1250,1750]);expect(keys(moved.project).value.track.sampleClock).toEqual(channel.value.track.sampleClock)
 const clipboard=captureNativeScalarClipboard(source,'root',[selected[0]!]),pasted=planNativeScalarPaste(source,'root',clipboard,'shape',1750000)
 expect(pasted.selection).toEqual([{channelId:channel.id,tick:1750}]);expect(clip(pasted.project)).toMatchObject({effects:[{kind:'lumetri',parameters:{enabled:true,curvesOn:true,wheelsOn:true}}]})
})
