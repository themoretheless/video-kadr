// SPDX-License-Identifier: AGPL-3.0-or-later
// Candidate ordering follows pinned Concat Timeline::nearest_free_start.
import {MAX_COMPOSITION_DURATION_TICKS,type WireCompositionTrack} from './types'
import {nativeClipStart} from './nativeClipMove'
import {nativeClipEnd} from './nativeClipInsert'
export type ConcatTrackPlacement='first_free'|'first_free_above'
/** Native tracks paint top first; Concat searches from the bottom. */
export function firstConcatFreeTrack(tracks:readonly WireCompositionTrack[],kind:'text'|'graphic',start:number,duration:number,placement:ConcatTrackPlacement,captionTracks:readonly string[]=[]):string|undefined{
 const occupied=(track:WireCompositionTrack)=>track.clips.some(clip=>start<nativeClipEnd(clip)&&nativeClipStart(clip)<start+duration)
 const ceiling=placement==='first_free_above'?tracks.findIndex(occupied):-1
 for(let index=tracks.length-1;index>=0;index--){
  const track=tracks[index]!
  if(placement==='first_free_above'&&ceiling>=0&&index>=ceiling)continue
  if(track.kind===kind&&!track.locked&&!captionTracks.includes(track.id)&&!occupied(track))return track.id
 }
 return undefined
}
export function nearestConcatFreeStart(clips:readonly {timelineStartTick:number;timelineEndTick:number}[],start:number,duration:number):number{
 if(!Number.isSafeInteger(start)||start<0||!Number.isSafeInteger(duration)||duration<=0)throw new Error('Некорректное время размещения Concat')
 const candidates=[start,0,...clips.flatMap(clip=>[clip.timelineEndTick,clip.timelineStartTick-duration])]
 let best:number|undefined
 for(const at of candidates){
  if(!Number.isSafeInteger(at)||at<0||at+duration>MAX_COMPOSITION_DURATION_TICKS)continue
  if(clips.some(clip=>at<clip.timelineEndTick&&clip.timelineStartTick<at+duration))continue
  if(best===undefined||Math.abs(at-start)<Math.abs(best-start))best=at
 }
 if(best===undefined)throw new Error('Нет свободного места в допустимом времени композиции')
 return best
}
