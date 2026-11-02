import {nearestConcatFreeStart,firstConcatFreeTrack,type ConcatTrackPlacement} from './concatPlacement'
import type {NativeNestedProject} from '$lib/api'
import {MAX_COMPOSITION_CLIPS,MAX_COMPOSITION_DURATION_TICKS,type WireGraphicClip} from './types'
import {isStableId} from './validation'
import {editNativeTrack} from './nativeTrackEdit'
import {rebindNativeLayerReferences} from './nativeLayerReferences'
export interface NativeTreatmentInsert {clipId:string;trackId:string;trackPlacement?:ConcatTrackPlacement;newTrackId:string;effects:readonly import('./types').WireVideoEffect[];startUs:number;durationUs:number}
export function insertNativeConcatTreatment(project:NativeNestedProject,compositionId:string,request:NativeTreatmentInsert):NativeNestedProject{
 const node=Object.hasOwn(project.compositions,compositionId)?project.compositions[compositionId]:undefined
 if(!node)throw new Error('Таймлайн не найден')
 let {startUs}=request
 const {durationUs}=request
 if(!Number.isSafeInteger(startUs)||!Number.isSafeInteger(durationUs)||startUs<0||durationUs<=0||startUs+durationUs>MAX_COMPOSITION_DURATION_TICKS)throw new Error('Некорректное время корректирующего слоя')
 if(!request.effects.length)throw new Error('Выберите эффект корректирующего слоя')
 if(!isStableId(request.clipId)||node.content.tracks.some(track=>track.clips.some(clip=>clip.id===request.clipId))||node.content.captionTracks?.some(track=>track.clips.some(clip=>clip.id===request.clipId)))throw new Error('Нужен уникальный идентификатор корректирующего слоя')
 if(node.content.tracks.reduce((count,track)=>count+track.clips.length,0)+(node.content.captionTracks?.reduce((count,track)=>count+track.clips.length,0)??0)>=MAX_COMPOSITION_CLIPS)throw new Error('Достигнут предел клипов')
 let document=project,trackId=request.trackId
 if(request.trackPlacement)trackId=firstConcatFreeTrack(node.content.tracks,'graphic',startUs,durationUs,request.trackPlacement,node.content.metadata?.captionTextTracks)??''
 if(!trackId){
  document=editNativeTrack(document,compositionId,{kind:'add',id:request.newTrackId,trackKind:'graphic',name:'Коррекция Concat'})
  trackId=request.newTrackId
  const created=document.compositions[compositionId].content.tracks.at(-1)!
  const tracks=rebindNativeLayerReferences(node.content.tracks,[created,...node.content.tracks])
  document={...document,compositions:{...document.compositions,[compositionId]:{...document.compositions[compositionId],content:{...node.content,tracks}}}}
 }
 const target=document.compositions[compositionId],track=target.content.tracks.find(track=>track.id===trackId)
 if(!track||track.kind!=='graphic')throw new Error('Выберите графическую дорожку')
 if(track.locked)throw new Error('Дорожка заблокирована')
 startUs=nearestConcatFreeStart(track.clips,startUs,durationUs)
 const constant=(value:number)=>({mode:'constant' as const,value})
 const clip:WireGraphicClip={id:request.clipId,timelineStartTick:startUs,timelineEndTick:startUs+durationUs,width:target.content.canvas.width,height:target.content.canvas.height,enabled:true,opacity:constant(1),graphic:{layers:[]},concatTreatment:{strength:1,fadeIn:0,fadeOut:0,durationTicks:durationUs},effects:JSON.parse(JSON.stringify(request.effects)),transform:{x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:0.5,anchorY:0.5}}
 return {...document,compositions:{...document.compositions,[compositionId]:{...target,...(target.durationUs!==undefined?{durationUs:Math.max(target.durationUs,startUs+durationUs)}:{}),content:{...target.content,tracks:target.content.tracks.map(lane=>lane.id===trackId?{...track,clips:[...track.clips,clip].sort((a,b)=>a.timelineStartTick-b.timelineStartTick)}:lane)}}}}
}
