import type {Composition,WireComposition} from '../composition/types'
import type {NativeNestedProject} from '../api'
import {activityError} from './activityDetails'
export interface AffectedEntity {kind:'Layer'|'Track'|'Marker'|'Media'|'Transition'|'Composition';id:string;compositionId:string;label:string}
type Entity={ref:AffectedEntity;value:string}
type Row=Record<string,unknown>
const row=(value:unknown):Row=>value&&typeof value==='object'?value as Row:{}
const list=(value:unknown):Row[]=>Array.isArray(value)?value.map(row):[]
function entities(documents:Record<string,Composition|WireComposition>,instances?:NativeNestedProject['compositions'],bindings:Record<string,{filename:string}>={}):Map<string,Entity>{
 const result=new Map<string,Entity>()
 for(const [compositionId,document] of Object.entries(documents)){
  const doc=row(document),metadata=row(doc.metadata),clipNames=row(metadata.clipNames),sourceNames=row(metadata.sourceNames)
  const put=(kind:AffectedEntity['kind'],id:string,label:unknown,value:unknown)=>{
   const ref={kind,id,compositionId,label:activityError(typeof label==='string'&&label?label:id).slice(0,128)}
   result.set(JSON.stringify([compositionId,kind,id]),{ref,value:JSON.stringify({value,label:ref.label})})
  }
  const {tracks,sources,markers,...settings}=doc
  put('Composition',compositionId,metadata.sequenceName??compositionId,{settings,instances:instances?.[compositionId]?.instances})
  list(tracks).forEach((track,index)=>{
   const {clips,transitions,...settings}=track
   const trackId=String(track.id)
   put('Track',trackId,track.name,{settings,index})
   list(clips).forEach((clip,index)=>put('Layer',String(clip.id),clipNames[String(clip.id)]??clip.name??clip.text??clip.id,{clip,trackId,index}))
   list(transitions).forEach((transition,index)=>put('Transition',String(transition.id),transition.name??transition.kind??transition.id,{transition,trackId,index}))
  })
  for(const id of new Set([...Object.keys(row(sources)),...Object.keys(bindings)]))put('Media',id,sourceNames[id]??bindings[id]?.filename??id,{source:row(sources)[id],binding:bindings[id]})
  for(const marker of list(markers??metadata.markers))put('Marker',String(marker.id),marker.label??marker.id,marker)
 }
 return result
}
function changes(before:Map<string,Entity>,after:Map<string,Entity>):AffectedEntity[]{
 const keys=new Set([...before.keys(),...after.keys()]),result:AffectedEntity[]=[]
 for(const key of keys)if(before.get(key)?.value!==after.get(key)?.value)result.push((after.get(key)??before.get(key))!.ref)
 return result
}
/** Compare only documents admitted by a real commit. Deleted objects retain their pre-edit labels. */
export function ordinaryAffected(before:Composition,after:Composition,beforeBindings:Record<string,{filename:string}>={},afterBindings:Record<string,{filename:string}>={}):AffectedEntity[]{return changes(entities({ordinary:before},undefined,beforeBindings),entities({ordinary:after},undefined,afterBindings))}
export function nestedAffected(before:NativeNestedProject,after:NativeNestedProject):AffectedEntity[]{
 const documents=(project:NativeNestedProject)=>Object.fromEntries(Object.entries(project.compositions).map(([id,node])=>[id,node.content]))
 return changes(entities(documents(before),before.compositions),entities(documents(after),after.compositions))
}
