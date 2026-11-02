import type {NativeNestedProject} from '$lib/api'
import type {AudioClip,Composition,WireAudioClip} from './types'
import {cloneDspEffect} from './dsp'
import {assertValidComposition} from './validation'
import {deleteClip} from './commands'
import {removeNativeClips} from './nativeClipDelete'
type Origin={id:string;detachedFrom?:string;concatDetachedOrder?:number}
function target<T extends Origin>(videos:readonly string[],sounds:readonly T[],selected:string){
 const video=videos.includes(selected)?selected:sounds.find(clip=>clip.id===selected)?.detachedFrom
 if(!video||!videos.includes(video))throw new Error('Исходное видео отделённого звука не найдено')
 const children=sounds.filter(clip=>clip.detachedFrom===video).sort((a,b)=>(a.concatDetachedOrder??Number.MAX_SAFE_INTEGER)-(b.concatDetachedOrder??Number.MAX_SAFE_INTEGER))
 if(!children.length)throw new Error('У видео нет отделённых звуковых фрагментов')
 return {video,children,ids:new Set(children.map(clip=>clip.id))}
}
export function hasConcatDetachedAudio(document:Composition,id:string):boolean {
 return document.tracks.some(track=>track.kind==='audio'&&track.clips.some(clip=>clip.id===id?!!clip.detachedFrom:clip.detachedFrom===id))
}
/** Source ReattachAudio restores only the first sound's filters, not its edited gain/window. */
export function reattachConcatAudio(document:Composition,id:string):{document:Composition;videoId:string}{
 assertValidComposition(document)
 const sounds=document.tracks.flatMap<AudioClip>(track=>track.kind==='audio'?[...track.clips]:[])
 const selected=target(document.tracks.flatMap(track=>track.kind==='video'?track.clips.map(clip=>clip.id):[]),sounds,id)
 if(document.links?.some(group=>group.memberIds.some(id=>selected.ids.has(id))))throw new Error('Сначала снимите явную связь звуковых клипов')
 for(const track of document.tracks)for(const clip of track.clips)if((selected.ids.has(clip.id)||clip.id===selected.video)&&(track.locked||clip.locked))throw new Error('Видео или звук заблокированы')
 let result=document;for(const child of selected.children)result=deleteClip(result,child.id)
 result={...result,tracks:result.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>clip.id===selected.video?{...clip,sourceAudioEnabled:true,independentAudio:true,dspEffects:selected.children[0]!.dspEffects?.map(cloneDspEffect)??[]}:clip)}:track)}
 assertValidComposition(result);return {document:result,videoId:selected.video}
}
export function reattachNativeConcatAudio(project:NativeNestedProject,compositionId:string,id:string):NativeNestedProject {
 const node=project.compositions[compositionId];if(!node)throw new Error('Композиция не найдена')
 const sounds=node.content.tracks.flatMap<WireAudioClip>(track=>track.kind==='audio'?[...track.clips]:[])
 const selected=target(node.content.tracks.flatMap(track=>track.kind==='video'?track.clips.map(clip=>clip.id):[]),sounds,id)
 if(Object.values(node.content.metadata?.clipLinks??{}).some(group=>group.some(id=>selected.ids.has(id))))throw new Error('Сначала снимите явную связь звуковых клипов')
 if(node.content.tracks.some(track=>track.locked&&track.clips.some(clip=>clip.id===selected.video||selected.ids.has(clip.id))))throw new Error('Видео или звук заблокированы')
 let clean=project;const ids=[...selected.ids]
 for(let start=0;start<ids.length;start+=512)clean=removeNativeClips(clean,compositionId,ids.slice(start,start+512))
 const current=clean.compositions[compositionId]!
 const tracks=current.content.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>clip.id===selected.video?{...clip,sourceAudioEnabled:true,independentAudio:true,dspEffects:selected.children[0]!.dspEffects?.map(cloneDspEffect)??[]}:clip)}:track)
 return {...clean,compositions:{...clean.compositions,[compositionId]:{...current,content:{...current.content,tracks}}}}
}
