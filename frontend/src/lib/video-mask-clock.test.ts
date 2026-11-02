import {describe,expect,it} from 'vitest'
import {isVideoMaskFrameRate,videoMaskFrameAtTime,videoMaskFrameRange,videoMaskTime} from './video-mask-clock'
import {copyVideoCorrectionRange} from './video-mask-recipe'
import type {VideoCorrectionRecipe} from './api'
const ntsc={numerator:30000,denominator:1001}
describe('source-relative mask frame clock',()=>{
 it('uses exact decimal comparisons at fractional frame boundaries',()=>{
  expect(videoMaskFrameAtTime('1.000999999999999999',ntsc,60)).toBe(29)
  expect(videoMaskFrameAtTime('1.001',ntsc,60)).toBe(30)
  expect(videoMaskFrameAtTime('1,001',ntsc,60)).toBe(30)
  expect(videoMaskFrameAtTime(' 0001.001 ',ntsc,60)).toBe(30)
  expect(videoMaskFrameRange('1.001','2.002',ntsc,60)).toEqual([30,60])
  expect(videoMaskFrameRange('0.01','0.25',{numerator:4,denominator:1},4)).toEqual([0,1])
  expect(videoMaskFrameRange('0.249','0.251',{numerator:4,denominator:1},4)).toEqual([0,2])
  expect(videoMaskFrameRange('0','',ntsc,61)).toEqual([0,61])
 })
 it('roundtrips displayed ordinals without floating-point boundary drift',()=>{
  for(const rate of [ntsc,{numerator:24000,denominator:1001},{numerator:60000,denominator:1001},{numerator:4294967295,denominator:1},{numerator:1,denominator:4294967295}]){
   for(const frame of [0,1,2,29,30,31,1001,10000,359999])expect(videoMaskFrameAtTime(videoMaskTime(frame,rate),rate,360000)).toBe(frame)
  }
 })
 it('rejects invalid clocks, unbounded input and time outside the half-open video',()=>{
  for(const rate of [null,[],{numerator:0,denominator:1},{numerator:1,denominator:0},{numerator:Infinity,denominator:1},{numerator:4294967296,denominator:1}])expect(isVideoMaskFrameRate(rate)).toBe(false)
  for(const seconds of ['','-1','NaN','Infinity','1e3','0.','0.0000000000000000001','0'.repeat(65),'2.002'])expect(()=>videoMaskFrameAtTime(seconds,ntsc,60)).toThrow()
  for(const [a,b] of [['1','1'],['1','0'],['0','2.003'],['2.002','']])expect(()=>videoMaskFrameRange(a!,b!,ntsc,60)).toThrow()
  expect(()=>videoMaskFrameAtTime('0',ntsc,0)).toThrow()
 })
})
const correction={custom:true,feather:0,strokes:[{tool:'brush' as const,size:.1,points:[[.2,.3] as [number,number]],at:null}]}
const recipe:VideoCorrectionRecipe={baseJobId:'root',maskHash:'a'.repeat(64),corrections:[{frame:0,correction},{frame:5,correction:{...correction,feather:.05}}]}
describe('range mask recipe authoring',()=>{
 it('replaces only the selected frames and owns separate stroke arrays',()=>{
  const before=JSON.stringify(recipe),next=copyVideoCorrectionRange(recipe,8,0,1,4)
  expect(next.map(e=>e.frame)).toEqual([0,1,2,3,5]);expect(next[4]!.correction.feather).toBe(.05)
  next[1]!.correction.strokes[0]!.points[0]![0]=.9
  expect(next[2]!.correction.strokes[0]!.points[0]![0]).toBe(.2);expect(JSON.stringify(recipe)).toBe(before)
 })
 it('restores automatic masks across large ranges without expanding sparse recipes',()=>{
  expect(copyVideoCorrectionRange(recipe,360000,1,0,360000)).toEqual([])
  expect(copyVideoCorrectionRange(recipe,8,1,0,4).map(e=>e.frame)).toEqual([5])
 })
 it('rejects frame, stroke and point budgets atomically before cloning a large range',()=>{
  const before=JSON.stringify(recipe)
  expect(()=>copyVideoCorrectionRange(recipe,360000,0,0,1025)).toThrow()
  const dense={...recipe,corrections:[{frame:0,correction:{...correction,strokes:[{...correction.strokes[0]!,points:Array.from({length:65536},()=>[.2,.3] as [number,number])}]}}]}
  expect(()=>copyVideoCorrectionRange(dense,4,0,0,2)).toThrow()
  expect(()=>copyVideoCorrectionRange(recipe,8,0,4,3)).toThrow();expect(JSON.stringify(recipe)).toBe(before)
 })
})

import {isVideoMaskNavigation} from './video-mask-clock'
it('validates ephemeral source ordinals and views without accepting unknown navigation fields',()=>{
 for(const view of ['mask','source','overlay'])for(const frame of [0,3])expect(isVideoMaskNavigation({frame,view},4)).toBe(true)
 for(const value of [null,[],{frame:-1,view:'mask'},{frame:4,view:'mask'},{frame:1.5,view:'mask'},{frame:NaN,view:'source'},{frame:0,view:'other'},{frame:0,view:'mask',time:0}])expect(isVideoMaskNavigation(value,4)).toBe(false)
 for(const frames of [0,4.5,360001,NaN])expect(isVideoMaskNavigation({frame:0,view:'mask'},frames)).toBe(false)
})
