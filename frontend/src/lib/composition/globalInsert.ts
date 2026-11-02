import {insertTransitionWindow} from './transitionTimeline'
import {addClip,CompositionCommandError,splitClip} from './commands'
import {insertCaptionGap} from './captionInsert'
import {assertCompositionLocksPreserved} from './locks'
import {clipEndTicks,type Composition,type CompositionClip,type CompositionTrack} from './types'
import {assertValidComposition,isSafeTick,isStableId} from './validation'

export interface InsertPlacement {readonly trackId:string;readonly clip:CompositionClip}
/** Filmcraft edit::insert: open one global span, then place all supplied items.
 * No live editor state is touched; the caller commits the returned document once.
 */
export function insertCompositionClips(document:Composition,placements:readonly InsertPlacement[],allocate:()=>string):Composition {
 assertValidComposition(document)
 if(!placements.length||placements.length>512)throw new CompositionCommandError('invalid-range','Insert requires 1–512 placements')
 const ids=new Set([...document.tracks.flatMap(track=>[track.id,...track.clips.map(clip=>clip.id)]),...(document.links??[]).map(link=>link.id),...(document.captionTracks??[]).flatMap(track=>[track.id,...track.clips.map(cue=>cue.id)])])
 const fresh=()=>{const id=allocate();if(!isStableId(id)||ids.has(id))throw new CompositionCommandError('duplicate-id','Insert ID must be unique and stable');ids.add(id);return id}
 const targets=new Set(placements.map(placement=>placement.trackId))
 for(const {trackId,clip} of placements){
  const track=document.tracks.find(track=>track.id===trackId)
  if(!track)throw new CompositionCommandError('missing-track',`Track ${trackId} does not exist`)
  if(track.locked)throw new CompositionCommandError('track-locked',`Track ${trackId} is locked`)
  if(track.kind!==clip.kind)throw new CompositionCommandError('track-kind','Insert placement does not match track')
  if(!isStableId(clip.id)||ids.has(clip.id))throw new CompositionCommandError('duplicate-id','Insert clip ID must be unique and stable')
  ids.add(clip.id)
  if(!isSafeTick(clip.timelineStartTicks)||!isSafeTick(clipEndTicks(clip))||clipEndTicks(clip)<=clip.timelineStartTicks)throw new CompositionCommandError('invalid-range','Invalid insertion span')
 }
 const at=Math.min(...placements.map(({clip})=>clip.timelineStartTicks))
 const end=Math.max(...placements.map(({clip})=>clipEndTicks(clip)))
 const duration=end-at
 let next=document
 const rights=new Map<string,string>()
 for(const track of document.tracks){
  if(track.locked||(!targets.has(track.id)&&track.syncLock===false))continue
  const crossing=track.clips.find(clip=>clip.timelineStartTicks<at&&clipEndTicks(clip)>at)
  if(crossing){const id=fresh();next=splitClip(next,crossing.id,at,id,true);rights.set(crossing.id,id)}
 }
 const links=[...(next.links??[])]
 for(const link of document.links??[]){
  const members=link.memberIds.flatMap(id=>rights.has(id)?[rights.get(id)!]:[])
  if(members.length>=2)links.push({id:fresh(),memberIds:members})
 }
 const tracks=next.tracks.map(track=>{
  if(track.locked||(!targets.has(track.id)&&track.syncLock===false))return track
  const original=document.tracks.find(candidate=>candidate.id===track.id)
  const transitions=track.kind==='video'&&original?.kind==='video'?(original.transitions??[]).flatMap(transition=>{
   if(!transition.timeline)return (track.transitions??[]).filter(candidate=>candidate.id===transition.id)
   const result=insertTransitionWindow({...transition,timeline:transition.timeline},at,duration,rights)
   return result?[result]:[]
  }):undefined
  return {...track,...(transitions?{transitions}:{}),clips:track.clips.map(clip=>{
   if(clip.timelineStartTicks<at)return clip
   if(clip.locked)throw new CompositionCommandError('clip-locked',`Clip ${clip.id} is locked`)
   const start=clip.timelineStartTicks+duration
   if(!isSafeTick(start)||!isSafeTick(clipEndTicks(clip)+duration))throw new CompositionCommandError('invalid-range','Insertion overflows timeline')
   return {...clip,timelineStartTicks:start}
  })} as CompositionTrack
 })
 next={...next,tracks,links,...(next.captionTracks?{captionTracks:insertCaptionGap(next.captionTracks,at,duration,fresh,new Set(tracks.flatMap(track=>track.clips.map(clip=>clip.id))))}:{})}
 for(const placement of placements)next=addClip(next,placement.trackId,placement.clip)
 assertValidComposition(next)
 assertCompositionLocksPreserved(document,next)
 return next
}
