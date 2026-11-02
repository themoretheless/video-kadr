import {expect,it} from 'vitest'
import {rewindowConcatSourceRun,sampleConcatSourceRun,rewindowConcatSourceEffects,rewindowNativeClipSourceEffects} from './concatSourceWindow'
const linear=[{at:0,value:0},{at:1,value:4}]
it('anchors boundary samples and positions for cut, trim and extension',()=>{
 expect(rewindowConcatSourceRun(linear,0,0.5)).toEqual([{at:0,value:0},{at:1,value:2,derived:true,ease:undefined}])
 expect(rewindowConcatSourceRun(linear,0.5,1)).toEqual([{at:0,value:2,derived:true,ease:[0,0,1,1]},{at:1,value:4}])
 const extended=rewindowConcatSourceRun(linear,-0.5,1.5);expect(extended).toEqual([{at:0.25,value:0},{at:0.75,value:4}]);expect(sampleConcatSourceRun(extended,0)).toBe(0);expect(sampleConcatSourceRun(extended,1)).toBe(4)
 for(let i=0;i<=100;i++){const x=i/100;expect(sampleConcatSourceRun(rewindowConcatSourceRun(linear,0.25,0.75),x)).toBeCloseTo(sampleConcatSourceRun(linear,0.25+x*0.5),12)}
})
it('matches original incoming easing and epsilon/duplicate boundary rules',()=>{
 const run=[{at:0,value:0},{at:0.5,value:2},{at:0.5,value:3},{at:1,value:4,ease:[0.42,-0.5,0.58,1] as const}]
 expect(sampleConcatSourceRun(run,0.5)).toBe(2);expect(rewindowConcatSourceRun(run,0.5,1)).toEqual([{at:0,value:2,derived:true,ease:[0,0,1,1]},{at:0,value:2},{at:0,value:3},{at:1,value:4,ease:[0.42,-0.5,0.58,1]}])
 expect(rewindowConcatSourceRun([{at:0.499,value:2},{at:0.501,value:3}],0.5,1)).toEqual([{at:0,value:2},{at:0.0020000000000000018,value:3}])
 expect(sampleConcatSourceRun([{at:0,value:0},{at:1,value:4,ease:[0.42,-0.5,0.58,1]}],0.5)).toBe(1.25)
})
it('does not commit nonfinite windows and retains finite derived overshoot',()=>{
 expect(()=>rewindowConcatSourceRun(linear,0.5,0.5)).toThrow();expect(()=>sampleConcatSourceRun(linear,NaN)).toThrow()
 expect(rewindowConcatSourceEffects([{kind:'concat_exposure',parameters:{},concatKeys:[{at:0,value:0},{at:1,value:4,ease:[0.42,3,0.58,3]}]}],1000000,500000,1000000)).toMatchObject([{concatKeys:[{at:0,value:9.5,derived:true},{at:1,value:4}]}])
})
it('windows map channels independently without changing static settings',()=>{
 const effect={kind:'concat_color_wheels' as const,parameters:{lift:[0.1,0.2,0.3] as const},concatKeys:{'lift.x':[{at:0,value:0},{at:1,value:1}],'gain.m':[{at:0,value:0.25}]}}
 const result=rewindowConcatSourceEffects([effect],1000000,250000,750000)![0]!
 expect(result).toMatchObject({parameters:effect.parameters,concatKeys:{'lift.x':[{at:0,value:0.25},{at:1,value:0.75}],'gain.m':[{at:0,value:0.25}]}})
 expect(effect.concatKeys['lift.x'][0]!.value).toBe(0)
})

it('rewindows inactive signed DSP metadata with original easing overshoot and static rest unchanged',async()=>{
 const {rewindowDspSourceKeys}=await import('./concatSourceWindow')
 const effects=[{id:'concat.pitch',params:{semitones:0},concatSourceKeys:{semitones:[{at:0,value:-12},{at:1,value:12,ease:[0.42,3,0.58,3] as const}]}}]
 const before=JSON.stringify(effects),out=rewindowDspSourceKeys(effects,1000000,500000,1000000)!
 expect(out[0]).toMatchObject({params:{semitones:0},concatSourceKeys:{semitones:[{at:0,value:45,derived:true},{at:1,value:12,ease:[0.42,3,0.58,3]}]}})
 expect(out[0]!.animation).toBeUndefined();expect(JSON.stringify(effects)).toBe(before);expect(out[0]!.concatSourceKeys!.semitones![1]!.ease).not.toBe(effects[0]!.concatSourceKeys.semitones[1]!.ease)
 expect(()=>rewindowDspSourceKeys(effects,0,0,1)).toThrow()
})

it('retains source-key timing inside native bypass when splitting or trimming',()=>{
 const active={kind:'concat_exposure' as const,parameters:{stops:1},concatKeys:linear}
 const wrapped={kind:'native_bypass' as const,effect:active},before=JSON.stringify(wrapped)
 for(const [from,to] of [[0,500000],[500000,1000000],[250000,750000]] as const){
  const actual=rewindowConcatSourceEffects([wrapped],1000000,from,to)![0]!
  expect(actual).toEqual({kind:'native_bypass',effect:rewindowConcatSourceEffects([active],1000000,from,to)![0]})
 }
 expect(JSON.stringify(wrapped)).toBe(before)
})

it('windows persisted clip source pose keys with an independent clock and without mutating the original',()=>{
 const clip={transform:{concatAnimation:{durationTicks:1000000,keys:{opacity:[{at:0,value:0},{at:1,value:1}]},rest:{scaleX:1,scaleY:1,x:0,y:0,rotationDegrees:0,opacity:0},stretch:[1,1],canvas:[1920,1080]}}}
 const before=JSON.stringify(clip)
 const right=rewindowNativeClipSourceEffects(clip,1000000,500000,1000000)
 expect(right.transform.concatAnimation.durationTicks).toBe(500000)
 expect(right.transform.concatAnimation.keys.opacity).toMatchObject([{at:0,value:0.5,derived:true},{at:1,value:1}])
 expect(JSON.stringify(clip)).toBe(before)
 expect(right.transform.concatAnimation.rest).not.toBe(clip.transform.concatAnimation.rest)
 expect(()=>rewindowNativeClipSourceEffects(clip,500000,0,250000)).toThrow('окно')
})
