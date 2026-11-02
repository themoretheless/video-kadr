import {expect,it} from 'vitest'
import {sphericalSemanticSamplePlan,sphericalCentresCameraKeys} from './sphericalSemanticTracking'
import type {SamVideoRegionAttachment} from '$lib/composition/types'
export function region():SamVideoRegionAttachment{return {sourceId:'source',maskSourceId:'mask',sourceHash:'a'.repeat(64),maskHash:'b'.repeat(64),maskFileHash:'c'.repeat(64),encoderHash:'d'.repeat(64),decoderHash:'e'.repeat(64),runtimeHash:'f'.repeat(64),promptHash:'1'.repeat(64),points:[[.5,.5]],width:64,height:32,frames:4,frameRate:{numerator:10,denominator:1},frameTimesUs:[0,100000,400000,600000,1000000],seedFrame:1,trackedFrames:4,forwardLostAt:null,backwardLostAt:null}}
it('maps VFR source presentation to deduplicated original mask ordinals',()=>{
 expect(sphericalSemanticSamplePlan(region(),0,1)).toEqual({times:[0,.2,.4,.6],ordinals:[0,1,2,3]})
 const lost={...region(),forwardLostAt:3,trackedFrames:3}
 expect(()=>sphericalSemanticSamplePlan(lost,0,1)).toThrow(/потеряна/)
})
it('unwraps native semantic centres without mutating measurements or serializing diagnostics',()=>{
 const centres=[{pan:179,tilt:3},{pan:-179,tilt:4}],original=structuredClone(centres)
 const result=sphericalCentresCameraKeys(centres,[0,.2])
 expect(centres).toEqual(original)
 expect(Object.keys(result)).toEqual(['pan','tilt'])
 if(result.pan.mode!=='keyframes')throw Error('keys')
 expect(result.pan.track.keyframes).toEqual([{tick:0,value:179},{tick:200000,value:181}])
 expect(()=>sphericalCentresCameraKeys([{pan:NaN,tilt:0},centres[0]!],[0,.2])).toThrow()
 expect(()=>sphericalCentresCameraKeys(centres,[0,0])).toThrow()
})
it('adds an optional complete bounded native FOV channel and rejects partial or invalid zoom responses',()=>{
 const result=sphericalCentresCameraKeys([{pan:0,tilt:0,fov:90},{pan:1,tilt:2,fov:96}],[0,.2])
 expect(Object.keys(result)).toEqual(['pan','tilt','fov'])
 expect(result.fov).toEqual({mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:90},{tick:200000,value:96}]}})
 expect(()=>sphericalCentresCameraKeys([{pan:0,tilt:0,fov:90},{pan:1,tilt:0}],[0,.2])).toThrow(/FOV/)
 expect(()=>sphericalCentresCameraKeys([{pan:0,tilt:0,fov:180},{pan:1,tilt:0,fov:90}],[0,.2])).toThrow(/FOV/)
})

it('allows previously lost source ordinals only for explicit neural recovery and bounds neural sampling',()=>{
 const lost={...region(),forwardLostAt:2,trackedFrames:2}
 expect(()=>sphericalSemanticSamplePlan(lost,0,1)).toThrow(/потеряна/)
 const plan=sphericalSemanticSamplePlan(lost,0,1,true)
 expect(plan.ordinals.some(i=>i>=2)).toBe(true)
 expect(plan.times.length).toBeLessThanOrEqual(60)
})

it('keeps intermediate poses in short neural clips and spans long clips within the inference budget',()=>{
 const source={...region(),frames:3000,trackedFrames:3000,frameRate:{numerator:30,denominator:1},frameTimesUs:undefined}
 const short=sphericalSemanticSamplePlan(source,0,10,true)
 expect(short.times).toHaveLength(50)
 expect(short.times.slice(0,4)).toEqual([0,.2,.4,.6])
 expect(short.ordinals.slice(0,4)).toEqual([0,6,12,18])
 const long=sphericalSemanticSamplePlan(source,0,60,true)
 expect(long.times).toHaveLength(60)
 expect(long.times.at(-1)).toBe(59)
 expect(sphericalSemanticSamplePlan(source,0,.2,true).times.length).toBeGreaterThanOrEqual(2)
})
