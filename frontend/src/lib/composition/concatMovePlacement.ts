// SPDX-License-Identifier: AGPL-3.0-or-later
// Common-shift candidate search follows pinned Concat Timeline::resolve_moves.
import {MAX_COMPOSITION_DURATION_TICKS} from './types'
export interface ConcatMoveSpan {trackId:string;start:number;end:number;overlapGroup?:string}
export function nearestConcatGroupShift(moving:readonly ConcatMoveSpan[],staying:readonly ConcatMoveSpan[]):number{
 if(!moving.length)return 0
 const covers=(a:ConcatMoveSpan,b:ConcatMoveSpan)=>a.trackId===b.trackId&&a.start<b.end&&b.start<a.end
 for(let i=0;i<moving.length;i++)for(let j=i+1;j<moving.length;j++)if(covers(moving[i]!,moving[j]!)&&(!moving[i]!.overlapGroup||moving[i]!.overlapGroup!==moving[j]!.overlapGroup))throw new Error('Клипы группы перекрывают друг друга на целевой дорожке')
 const earliest=Math.min(...moving.map(span=>span.start)),latest=Math.max(...moving.map(span=>span.end)),candidates=[0,-earliest]
 // Each stationary overlap forbids an open interval of common shifts.
 // Merge those intervals once instead of rescanning every clip per candidate.
 const forbidden:{start:number;end:number}[]=[]
 for(const span of moving)for(const other of staying)if(span.trackId===other.trackId){
  const start=other.start-span.end,end=other.end-span.start
  candidates.push(end,start);forbidden.push({start,end})
 }
 forbidden.sort((a,b)=>a.start-b.start||a.end-b.end)
 const merged:{start:number;end:number}[]=[]
 for(const interval of forbidden){const last=merged.at(-1);if(last&&interval.start<last.end)last.end=Math.max(last.end,interval.end);else merged.push({...interval})}
 const blocked=(delta:number)=>{
  let lo=0,hi=merged.length
  while(lo<hi){const mid=(lo+hi)>>>1;if(merged[mid]!.start<delta)lo=mid+1;else hi=mid}
  return lo>0&&delta<merged[lo-1]!.end
 }
 let best:number|undefined
 for(const delta of candidates){
  if(!Number.isSafeInteger(delta)||!Number.isSafeInteger(earliest+delta)||!Number.isSafeInteger(latest+delta)||earliest+delta<0||latest+delta>MAX_COMPOSITION_DURATION_TICKS||blocked(delta))continue
  if(best===undefined||Math.abs(delta)<Math.abs(best))best=delta
 }
 if(best===undefined)throw new Error('Нет свободного промежутка для всей группы клипов')
 return best
}
