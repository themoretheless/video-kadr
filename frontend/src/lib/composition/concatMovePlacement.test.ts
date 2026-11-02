import {expect,it} from 'vitest'
import {nearestConcatGroupShift,type ConcatMoveSpan} from './concatMovePlacement'
import {MAX_COMPOSITION_DURATION_TICKS} from './types'
it('finds one shift that fits every lane and keeps staggered group timing',()=>{
 const moving=[{trackId:'v',start:2000000,end:4000000},{trackId:'a',start:2500000,end:3500000}],staying=[{trackId:'v',start:0,end:3000000},{trackId:'a',start:4000000,end:5000000}]
 expect(nearestConcatGroupShift(moving,staying)).toBe(2500000)
 expect(nearestConcatGroupShift([{trackId:'v',start:4000000,end:5000000}],[{trackId:'v',start:4000000,end:9000000}])).toBe(-1000000)
 expect(nearestConcatGroupShift(moving,[])).toBe(0)
})
it('refuses intrinsically overlapping destinations and timeline exhaustion',()=>{
 expect(()=>nearestConcatGroupShift([{trackId:'v',start:0,end:3},{trackId:'v',start:2,end:4}],[])).toThrow('перекрывают')
 expect(()=>nearestConcatGroupShift([{trackId:'v',start:0,end:1}],[{trackId:'v',start:0,end:MAX_COMPOSITION_DURATION_TICKS}])).toThrow('Нет свободного')
})
it('matches exhaustive legal common shifts across two occupied lanes',()=>{
 const moving=[{trackId:'v',start:3,end:5},{trackId:'a',start:4,end:7}]
 for(let mask=0;mask<64;mask++){
  const staying:ConcatMoveSpan[]=Array.from({length:6},(_,i)=>({trackId:i%2?'a':'v',start:i*2,end:i*2+2})).filter((_,i)=>(mask&(1<<i))!==0)
  const legal=Array.from({length:30},(_,i)=>i-3).filter(delta=>moving.every(span=>span.start+delta>=0&&!staying.some(other=>span.trackId===other.trackId&&span.start+delta<other.end&&other.start<span.end+delta)))
  expect(Math.abs(nearestConcatGroupShift(moving,staying))).toBe(Math.min(...legal.map(Math.abs)))
 }
})
it('keeps touching forbidden intervals open and handles a full 512-clip occupancy plan',()=>{
 const moving=[{trackId:'v',start:2,end:3}],staying=[{trackId:'v',start:0,end:2},{trackId:'v',start:3,end:5}]
 expect(nearestConcatGroupShift(moving,staying)).toBe(0)
 const group=Array.from({length:256},(_,i)=>({trackId:'v',start:2*i,end:2*i+1})),fixed=Array.from({length:256},(_,i)=>({trackId:'v',start:2*i,end:2*i+2}))
 expect(nearestConcatGroupShift(group,fixed)).toBe(512)
})
it('preserves authored same-lane overlaps but refuses collapsing different source lanes',()=>{
 const moving=[{trackId:'destination',overlapGroup:'original',start:1,end:4},{trackId:'destination',overlapGroup:'original',start:3,end:5}]
 expect(nearestConcatGroupShift(moving,[])).toBe(0)
 expect(nearestConcatGroupShift(moving,[{trackId:'destination',start:0,end:6}])).toBe(5)
 expect(()=>nearestConcatGroupShift([moving[0],{...moving[1],overlapGroup:'other'}],[])).toThrow('перекрывают')
})
