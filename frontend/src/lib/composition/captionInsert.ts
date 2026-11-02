import {exactCaptionTicks,exactCaptionDeltaTicks,captionTimestampFromExact,type CaptionTimestamp} from './captionTime'
import {validateCaptionTracks} from './captionTracks'
import type {CaptionTrack} from './types'
/** Filmcraft global-insert caption phase: split straddling cues, then shift from the cut.
 * Source: edit/captions.rs insert_gap, MIT (docs/licenses/filmcraft-MIT.txt).
 * The caller still owns media placement and the single authored transaction.
 */
export function insertCaptionGap(tracks:readonly CaptionTrack[],at:CaptionTimestamp,duration:CaptionTimestamp,allocate:()=>string,reservedIds:ReadonlySet<string>=new Set()):readonly CaptionTrack[] {
 const point=exactCaptionTicks(at),delta=exactCaptionDeltaTicks(duration)
 if(point<0n||delta<=0n)throw new Error('Некорректный промежуток вставки субтитров')
 const ids=new Set(reservedIds)
 const validate=(value:readonly CaptionTrack[])=>{
  const errors:string[]=[]
  validateCaptionTracks(value,new Set(),new Set(reservedIds),(_code,_path,message)=>errors.push(message))
  if(errors.length)throw new Error(errors.join('; '))
 }
 validate(tracks)
 for(const track of tracks)for(const cue of track.clips)ids.add(cue.id)
 const next=tracks.map(track=>{
  if(track.locked||track.syncLock===false)return track
  let changed=false
  const clips=track.clips.flatMap(cue=>{
   const start=exactCaptionTicks(cue.timelineStartTick),end=exactCaptionTicks(cue.timelineEndTick)
   if(end<=point)return [cue]
   changed=true
   if(start>=point)return [{...cue,timelineStartTick:captionTimestampFromExact(start+delta,cue.timelineStartTick),timelineEndTick:captionTimestampFromExact(end+delta,cue.timelineEndTick)}]
   const id=allocate()
   if(ids.has(id))throw new Error('Повторяющийся идентификатор субтитра при вставке')
   ids.add(id)
   return [
    {...cue,timelineEndTick:captionTimestampFromExact(point,cue.timelineEndTick)},
    {...cue,id,cueId:null,timelineStartTick:captionTimestampFromExact(point+delta,cue.timelineStartTick),timelineEndTick:captionTimestampFromExact(end+delta,cue.timelineEndTick)},
   ]
  })
  return changed?{...track,clips}:track
 })
 validate(next)
 return next
}
