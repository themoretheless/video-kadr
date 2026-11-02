import {findClipLocation,registerSource} from './commands'
import {linkedClipIds} from './links'
import {replaceClipMedia,setClipMediaVersion} from './replaceMedia'
import type {Composition,CompositionSource} from './types'

function members(document:Composition,clipId:string):readonly string[]{
 const seed=findClipLocation(document,clipId).clip
 if(seed.kind!=='video')throw new Error('Выберите видеоклип')
 const ids=linkedClipIds(document,clipId)
 for(const id of ids){const clip=findClipLocation(document,id).clip;if((clip.kind!=='video'&&clip.kind!=='audio')||clip.sourceId!==seed.sourceId)throw new Error('Связанные клипы используют разные исходники. Замените их отдельно из медиатеки.')}
 return ids
}
export function replaceEnhancedVideo(document:Composition,clipId:string,source:CompositionSource):Composition{
 if(source.kind!=='video')throw new Error('Обработанная копия должна быть видео')
 const ids=members(document,clipId),existing=document.sources[source.id]
 if(existing&&(['kind','durationTicks','width','height','hasAudio'] as const).some(key=>existing[key]!==source[key]))throw new Error('Метаданные копии отличаются от зарегистрированного источника')
 let next=existing?document:registerSource(document,source)
 for(const id of ids)next=replaceClipMedia(next,id,source.id,undefined,true)
 return next
}
export function hasEnhancedVideoGroup(document:Composition,clipId:string):boolean{
 try{const ids=members(document,clipId),pair=document.metadata?.mediaVersions?.[clipId];return Boolean(pair&&ids.every(id=>{const value=document.metadata?.mediaVersions?.[id];return value?.originalSourceId===pair.originalSourceId&&value?.copySourceId===pair.copySourceId}))}catch{return false}
}
export function switchEnhancedVideo(document:Composition,clipId:string,copy:boolean):Composition{
 const ids=members(document,clipId)
 if(!hasEnhancedVideoGroup(document,clipId))throw new Error('Не у всех связанных клипов сохранена обработанная копия')
 let next=document;for(const id of ids)next=setClipMediaVersion(next,id,copy)
 return next
}
