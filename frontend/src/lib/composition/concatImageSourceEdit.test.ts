import {setNativeSourceSpeedCurve} from './concatSourceCurve'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import * as api from '$lib/api'
import {createComposition} from './commands'
import {buildCompositionWireDocument} from './payload'
import {splitNativeClipsWithSourcePlans} from './concatSourceSplit'
import {trimNativeClipsWithSourcePlans} from './concatSourceTrim'
import {splitNativeClips} from './nativeClipSplit'
import {trimNativeClipStarts} from './nativeClipTrim'
import type {ConcatSourceDocumentClock,WireImageClip} from './types'
vi.mock('$lib/api',()=>({planConcatSourceEdit:vi.fn(),evaluateNestedFrame:vi.fn()}))
import {nestedComposition,nestedHistory,splitNestedClips,trimNestedClipStarts,undoNestedComposition,redoNestedComposition,closeNestedComposition} from '$lib/state/nestedComposition.svelte'
beforeEach(()=>{closeNestedComposition();vi.resetAllMocks();vi.mocked(api.evaluateNestedFrame).mockResolvedValue({entries:[]})})
afterEach(()=>closeNestedComposition())
function project():api.NativeNestedProject {
 const content=buildCompositionWireDocument(createComposition({width:16,height:16,fps:25,backgroundColor:'#000000'}))
 return {schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{...content,sources:{image:{id:'image',kind:'image',durationTicks:0,width:16,height:16,hasAudio:false}},tracks:[{id:'images',kind:'image',name:'Images',hidden:false,locked:false,clips:[{id:'still',sourceId:'image',timelineStartTick:2000000,durationTicks:3000000,concatSourceTiming:{sourceStart:4,speed:2},transform:{x:{mode:'constant',value:0},y:{mode:'constant',value:0},scaleX:{mode:'constant',value:1},scaleY:{mode:'constant',value:1},rotationDegrees:{mode:'constant',value:0},anchorX:0.5,anchorY:0.5},opacity:{mode:'constant',value:1},blendMode:'normal',enabled:true,effects:[{kind:'concat_contrast',parameters:{amount:0},concatKeys:[{at:0,value:0},{at:1,value:1}]}]}]}]}}}}
}
function part(clock:ConcatSourceDocumentClock,role:'head'|'tail'|'clip',tracks:api.ConcatSourceEditTracks):api.ConcatSourceEditPlan['parts'][number]{
 return {role,clock,tracks,fadeIn:0,fadeOut:0,placement:{timelineStartTick:Math.round(clock.start*1e6),timelineDurationTick:Math.round(clock.duration*1e6),sourceInTick:Math.round(clock.sourceStart*1e6),sourceOutTick:Math.round((clock.sourceStart+clock.duration*clock.speed)*1e6),speed:clock.speed,concatRetime:{documentClock:clock,points:[],durationTicks:Math.round(clock.duration*1e6),rateNum:25,rateDen:1}}}
}
it('applies native split clocks to stills without leaking moving-media fields or changing the input',async()=>{
 const input=project(),before=JSON.stringify(input)
 expect(()=>splitNativeClips(input,'root',['still'],3250000,()=> 'tail')).toThrow('нативного плана')
 vi.mocked(api.planConcatSourceEdit).mockImplementation(async request=>({changed:true,rippleBy:0,parts:[part({...request.clock,duration:1.25},'head',request.tracks!),part({...request.clock,start:3.25,duration:1.75,sourceStart:6.5},'tail',request.tracks!)]}))
 const output=await splitNativeClipsWithSourcePlans(input,'root',['still'],3250000,()=> 'tail')
 expect(api.planConcatSourceEdit).toHaveBeenCalledWith(expect.objectContaining({clock:{start:2,duration:3,sourceStart:4,speed:2},command:{kind:'split',time:3.25},tracks:expect.objectContaining({video:{'video/0/concatKeys':[{at:0,value:0},{at:1,value:1}]}})}))
 const clips=output.compositions.root!.content.tracks[0]!.clips as readonly WireImageClip[]
 expect(clips).toHaveLength(2);expect(clips[0]).toMatchObject({durationTicks:1250000,concatSourceTiming:{sourceStart:4,speed:2}});expect(clips[1]).toMatchObject({id:'tail',timelineStartTick:3250000,durationTicks:1750000,concatSourceTiming:{sourceStart:6.5,speed:2}})
 for(const clip of clips){expect('placement' in clip).toBe(false);expect('dspEffects' in clip).toBe(false);expect(clip.effects).toEqual((input.compositions.root!.content.tracks[0]!.clips[0] as WireImageClip).effects)}
 expect(JSON.stringify(input)).toBe(before)
})
it('uses the Rust trim plan for head movement and refuses synchronous or failed-plan edits atomically',async()=>{
 const input=project(),before=JSON.stringify(input)
 expect(()=>trimNativeClipStarts(input,'root',['still'],500000)).toThrow('нативного плана')
 vi.mocked(api.planConcatSourceEdit).mockImplementation(async request=>({changed:true,rippleBy:0,parts:[part({...request.clock,start:2.5,duration:2.5,sourceStart:5},'clip',request.tracks!)]}))
 const output=await trimNativeClipsWithSourcePlans(input,'root',['still'],500000,'start')
 expect(api.planConcatSourceEdit).toHaveBeenCalledWith(expect.objectContaining({command:{kind:'trimStart',delta:.5,floor:0,ripple:false}}))
 expect(output.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({timelineStartTick:2500000,durationTicks:2500000,concatSourceTiming:{sourceStart:5,speed:2}})
 vi.mocked(api.planConcatSourceEdit).mockRejectedValue(Error('plan refused'))
 await expect(trimNativeClipsWithSourcePlans(input,'root',['still'],500000,'start')).rejects.toThrow('plan refused');expect(JSON.stringify(input)).toBe(before)
})

it('records image split as one Undo/Redo edit and drops a late trim after close',async()=>{
 const input=project();nestedComposition.document=input;nestedComposition.activeCompositionId='root'
 vi.mocked(api.planConcatSourceEdit).mockImplementation(async request=>({changed:true,rippleBy:0,parts:[part({...request.clock,duration:1.25},'head',request.tracks!),part({...request.clock,start:3.25,duration:1.75,sourceStart:6.5},'tail',request.tracks!)]}))
 await splitNestedClips('root',['still'],3250000);expect(nestedHistory.past).toHaveLength(1)
 const split=JSON.stringify(nestedComposition.document);undoNestedComposition();expect(nestedComposition.document).toEqual(input);redoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(split)
 let resolve!:(value:api.ConcatSourceEditPlan)=>void;vi.mocked(api.planConcatSourceEdit).mockClear().mockReturnValue(new Promise(done=>{resolve=done}))
 const pending=trimNestedClipStarts('root',['still'],250000);await vi.waitFor(()=>expect(api.planConcatSourceEdit).toHaveBeenCalledTimes(1))
 const request=vi.mocked(api.planConcatSourceEdit).mock.calls[0]![0];closeNestedComposition();resolve({changed:true,rippleBy:0,parts:[part({...request.clock,start:2.25,duration:1,sourceStart:4.5},'clip',request.tracks!)]});await pending
 expect(nestedComposition.document).toBeNull();expect(nestedHistory.past).toHaveLength(0)
})

it('retains native curve results on images and supplies the curve to the following split',async()=>{
 const input=project(),points=[{at:0,speed:2},{at:1,speed:4}]
 vi.mocked(api.planConcatSourceEdit).mockImplementation(async request=>({changed:true,rippleBy:0,parts:[part({...request.clock,duration:2,speed:3,speedCurve:points},'clip',request.tracks!)]}))
 const curved=await setNativeSourceSpeedCurve(input,'root',['still'],points)
 expect(curved.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({durationTicks:2000000,concatSourceTiming:{sourceStart:4,speed:3,speedCurve:points}})
 expect(api.planConcatSourceEdit).toHaveBeenCalledWith(expect.objectContaining({command:{kind:'setCurve',points}}))
 vi.mocked(api.planConcatSourceEdit).mockClear().mockImplementation(async request=>({changed:true,rippleBy:0,parts:[part({...request.clock,duration:1,speedCurve:null},'head',request.tracks!),part({...request.clock,start:3,duration:1,sourceStart:7,speedCurve:null},'tail',request.tracks!)]}))
 const split=await splitNativeClipsWithSourcePlans(curved,'root',['still'],3000000,()=> 'tail')
 expect(api.planConcatSourceEdit).toHaveBeenCalledWith(expect.objectContaining({clock:expect.objectContaining({speedCurve:points})}))
 const clips=split.compositions.root!.content.tracks[0]!.clips as readonly WireImageClip[]
 expect(clips[1].concatSourceTiming).toEqual({sourceStart:7,speed:3});expect(clips[0].concatSourceTiming).toEqual({sourceStart:4,speed:3})
})
