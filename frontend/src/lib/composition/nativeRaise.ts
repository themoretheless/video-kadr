import {moveNativeNestedClips,type NativeNestedProject,type NativeClipMove} from '../api'
import {nativeClipStart} from './nativeClipMove'
import {prepareNativeLinkEscape} from './nativeClipLinks'
import {rebindNativeLayerReferences} from './nativeLayerReferences'
import {MAX_COMPOSITION_TRACKS,type WireCompositionTrack} from './types'
import {isStableId} from './validation'
export async function raiseNativeLayers(project:NativeNestedProject,scope:string,ids:readonly string[],trackId:string,anchor?:{clipId:string;startTicks:number}):Promise<NativeNestedProject>{
 const node=Object.hasOwn(project.compositions,scope)?project.compositions[scope]:undefined
 if(!node)throw new Error('Composition not found')
 if(!Array.isArray(ids)||!ids.length||ids.length>512||new Set(ids).size!==ids.length||ids.some(id=>!isStableId(id)))throw new Error('Raise requires 1–512 distinct layers')
 if(!isStableId(trackId)||[...node.content.tracks,...node.content.captionTracks??[]].some(track=>track.id===trackId))throw new Error('New track ID exists or is invalid')
 if(node.content.tracks.length+(node.content.captionTracks?.length??0)>=MAX_COMPOSITION_TRACKS)throw new Error('Track limit reached')
 const rows=ids.map(id=>{const track=node.content.tracks.find(track=>track.clips.some(clip=>clip.id===id)),clip=track?.clips.find(clip=>clip.id===id);if(!track||!clip||track.locked)throw new Error('Raise source is missing or locked');return {track,clip}})
 if(rows.some(row=>row.track.kind!==rows[0]!.track.kind))throw new Error('Raise requires one layer kind')
 if(anchor&&(!ids.includes(anchor.clipId)||!Number.isSafeInteger(anchor.startTicks)||anchor.startTicks<0||Object.keys(anchor).some(field=>!['clipId','startTicks'].includes(field))))throw new Error('Invalid raise anchor')
 const seed=rows.find(row=>row.clip.id===(anchor?.clipId??ids[0]))!,origin=Math.min(...rows.map(row=>nativeClipStart(row.clip)))
 const delta=anchor?Math.max(-origin,anchor.startTicks-nativeClipStart(seed.clip)):0
 const source=rows[0]!.track
 const fresh={...source,id:trackId,name:'Новая дорожка',clips:[],...(source.kind==='video'?{transitions:[]}:{})} as WireCompositionTrack
 const staged={...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,tracks:[...node.content.tracks,fresh]}}}}
 const escape=prepareNativeLinkEscape(staged,scope,ids)
 const moves:NativeClipMove[]=rows.map(row=>{
  const startUs=nativeClipStart(row.clip)+delta
  if(!Number.isSafeInteger(startUs)||startUs<0)throw new Error('Raise exceeds the exact timeline range')
  return {fromComposition:scope,fromTrackId:row.track.id,clipId:row.clip.id,toComposition:scope,toTrackId:trackId,startUs,createTrack:false}
 })
 const moved=await moveNativeNestedClips(escape.project,moves)
 if(!moved.changed)throw new Error('Native raise did not move the selected layers')
 const result=escape.restore(moved.project),after=result.compositions[scope]!
 const sources=new Set(rows.map(row=>row.track.id)),removed=new Set(after.content.tracks.filter(track=>sources.has(track.id)&&!track.clips.length).map(track=>track.id))
 const destination=after.content.tracks.find(track=>track.id===trackId)!
 const ordered=[destination,...after.content.tracks.filter(track=>track.id!==trackId&&!removed.has(track.id))]
 const metadata=after.content.metadata,captionRoles=metadata?.captionTextTracks
 return {...project,compositions:{...project.compositions,[scope]:{...after,content:{...after.content,tracks:rebindNativeLayerReferences(after.content.tracks,ordered),...(metadata?{metadata:{...metadata,...(captionRoles?{captionTextTracks:[...captionRoles.filter(id=>!removed.has(id)),...(captionRoles.includes(source.id)?[trackId]:[])]}:{})}}:{})}}}}
}
