import {cloneGraphicValue} from './graphicLayers'
import {findClipLocation,CompositionCommandError} from './commands'
import {insertCompositionClips,type InsertPlacement} from './globalInsert'
import {clipEndTicks,mediaVersionSourceIds,type Composition} from './types'
import {assertValidComposition,isSafeTick} from './validation'
import {assertCompositionLocksPreserved} from './locks'

/** Paste Insert retains copied timing, same-kind track ordinal and copied links. */
export function pasteInsertComposition(document:Composition,snapshot:Composition,clipIds:readonly string[],at:number,allocate:()=>string):{document:Composition;placements:readonly InsertPlacement[];end:number}{
 assertValidComposition(document);assertValidComposition(snapshot)
 if(!clipIds.length||!isSafeTick(at))throw new CompositionCommandError('invalid-range','Empty or invalid Paste Insert')
 const copied=[...new Set(clipIds)].map(id=>findClipLocation(snapshot,id))
 const origin=Math.min(...copied.map(({clip})=>clip.timelineStartTicks))
 const placements:InsertPlacement[]=[]
 const copies=new Map<string,string>()
 const sources={...document.sources}
 for(const {track,clip} of copied){
  const sourceTracks=snapshot.tracks.filter(candidate=>candidate.kind===track.kind)
  const targets=document.tracks.filter(candidate=>candidate.kind===track.kind)
  const target=targets[sourceTracks.findIndex(candidate=>candidate.id===track.id)]??targets.at(-1)
  if(!target)continue
  for(const sourceId of 'sourceId' in clip ? [...new Set([clip.sourceId, ...mediaVersionSourceIds(snapshot.metadata?.mediaVersions?.[clip.id])])] : []) {
   const source=snapshot.sources[sourceId]!
   if(sources[source.id]&&JSON.stringify(sources[source.id])!==JSON.stringify(source))throw new CompositionCommandError('invalid-range','Copied source identity changed')
   sources[source.id]=cloneGraphicValue(source)
  }
  const id=allocate();copies.set(clip.id,id)
  placements.push({trackId:target.id,clip:{...cloneGraphicValue(clip),id,timelineStartTicks:at+clip.timelineStartTicks-origin}})
 }
 let next=insertCompositionClips({...document,sources},placements,allocate)
 const links=(snapshot.links??[]).flatMap(link=>{
  const memberIds=link.memberIds.flatMap(id=>copies.has(id)?[copies.get(id)!]:[])
  return memberIds.length>=2?[{id:allocate(),memberIds}]:[]
 })
 if(links.length)next={...next,links:[...(next.links??[]),...links]}
 const mediaVersions={...next.metadata?.mediaVersions}
 for(const [original,id] of copies){const pair=snapshot.metadata?.mediaVersions?.[original];if(pair)mediaVersions[id]={...pair}}
 if(Object.keys(mediaVersions).length)next={...next,metadata:{...next.metadata,mediaVersions}}
 const clipNames={...next.metadata?.clipNames}
 for(const [original,id] of copies){
  const name=snapshot.metadata?.clipNames?.[original]
  if(name!==undefined)clipNames[id]=name
 }
 const sourceNames={...next.metadata?.sourceNames}
 for(const {clip} of placements)if('sourceId' in clip&&sourceNames[clip.sourceId]===undefined){
  const name=snapshot.metadata?.sourceNames?.[clip.sourceId]
  if(name!==undefined)sourceNames[clip.sourceId]=name
 }
 if(Object.keys(clipNames).length||Object.keys(sourceNames).length)next={...next,metadata:{...next.metadata,...(Object.keys(clipNames).length?{clipNames}:{}),...(Object.keys(sourceNames).length?{sourceNames}:{})}}
 assertValidComposition(next);assertCompositionLocksPreserved(document,next)
 return {document:next,placements,end:Math.max(...placements.map(({clip})=>clipEndTicks(clip)))}
}
