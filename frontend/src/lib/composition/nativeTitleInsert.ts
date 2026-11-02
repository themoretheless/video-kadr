import {nearestConcatFreeStart,firstConcatFreeTrack,type ConcatTrackPlacement} from './concatPlacement'
import type {NativeNestedProject} from '$lib/api'
import {MAX_COMPOSITION_CLIPS,MAX_COMPOSITION_DURATION_TICKS,type WireTextClip} from './types'
import {isStableId} from './validation'
import {editNativeTrack} from './nativeTrackEdit'
import {rebindNativeLayerReferences} from './nativeLayerReferences'
export interface NativeTitleInsert {clipId:string;trackId:string;trackPlacement?:ConcatTrackPlacement;newTrackId:string;text:string;startUs:number;durationUs:number}
export function insertNativeConcatTitle(project:NativeNestedProject,compositionId:string,request:NativeTitleInsert):NativeNestedProject{
 const node=Object.hasOwn(project.compositions,compositionId)?project.compositions[compositionId]:undefined
 if(!node)throw new Error('Таймлайн не найден')
 let {startUs}=request
 const {durationUs}=request
 if(!Number.isSafeInteger(startUs)||!Number.isSafeInteger(durationUs)||startUs<0||durationUs<=0||startUs+durationUs>MAX_COMPOSITION_DURATION_TICKS)throw new Error('Некорректное время титра')
 if(!request.text.trim()||Array.from(request.text).length>512||request.text.includes('\0'))throw new Error('Текст титра: от 1 до 512 символов')
 if(!isStableId(request.clipId)||node.content.tracks.some(track=>track.clips.some(clip=>clip.id===request.clipId))||node.content.captionTracks?.some(track=>track.clips.some(clip=>clip.id===request.clipId)))throw new Error('Нужен уникальный идентификатор титра')
 if(node.content.tracks.reduce((count,track)=>count+track.clips.length,0)+(node.content.captionTracks?.reduce((count,track)=>count+track.clips.length,0)??0)>=MAX_COMPOSITION_CLIPS)throw new Error('Достигнут предел клипов')
 let document=project,trackId=request.trackId
 if(request.trackPlacement)trackId=firstConcatFreeTrack(node.content.tracks,'text',startUs,durationUs,request.trackPlacement,node.content.metadata?.captionTextTracks)??''
 if(!trackId){
  document=editNativeTrack(document,compositionId,{kind:'add',id:request.newTrackId,trackKind:'text',name:'Титры Concat'})
  trackId=request.newTrackId
  const created=document.compositions[compositionId].content.tracks.at(-1)!
  const tracks=rebindNativeLayerReferences(node.content.tracks,[created,...node.content.tracks])
  document={...document,compositions:{...document.compositions,[compositionId]:{...document.compositions[compositionId],content:{...node.content,tracks}}}}
 }
 const target=document.compositions[compositionId],track=target.content.tracks.find(track=>track.id===trackId)
 if(!track||track.kind!=='text')throw new Error('Выберите текстовую дорожку')
 if(track.locked)throw new Error('Дорожка заблокирована')
 if(target.content.metadata?.captionTextTracks?.includes(trackId))throw new Error('Дорожка субтитров не подходит для титра')
 startUs=nearestConcatFreeStart(track.clips,startUs,durationUs)
 const constant=(value:number)=>({mode:'constant' as const,value}),clear={red:0,green:0,blue:0,alpha:0}
 const clip:WireTextClip={id:request.clipId,text:request.text,timelineStartTick:startUs,timelineEndTick:startUs+durationUs,enabled:true,opacity:constant(1),concatTitle:{},transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5},style:{fontFamily:'Noto Sans',fontSize:24,color:{red:1,green:1,blue:1,alpha:1},background:clear,stroke:clear,strokeWidth:0,shadow:clear,shadowX:0,shadowY:0}}
 return {...document,compositions:{...document.compositions,[compositionId]:{...target,...(target.durationUs!==undefined?{durationUs:Math.max(target.durationUs,startUs+durationUs)}:{}),content:{...target.content,tracks:target.content.tracks.map(lane=>lane.id===trackId?{...track,clips:[...track.clips,clip].sort((a,b)=>a.timelineStartTick-b.timelineStartTick)}:lane)}}}}
}
