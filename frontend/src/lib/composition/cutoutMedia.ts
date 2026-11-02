import {cutoutMaskDimensions} from './types'
import type {NativeNestedProject} from '$lib/api'
import {registerSource,findClipLocation} from './commands'
import {replaceClipMedia} from './replaceMedia'
import {replaceNativeMedia} from './nativeMediaReplace'
import {expandNativeClipLinks} from './nativeClipLinks'
import {assertValidComposition} from './validation'
import {MAX_COMPOSITION_SOURCES,type Composition,type CompositionSource,type CutoutAttachment} from './types'
export interface CutoutSources {copy:CompositionSource;source:CompositionSource;mask:CompositionSource;regions?:readonly CompositionSource[]}
function check(sources:CutoutSources,snapshot:CutoutAttachment){const [maskWidth,maskHeight]=cutoutMaskDimensions(snapshot.modelHash,sources.source.width,sources.source.height);if(sources.source.id!==snapshot.sourceId||sources.mask.id!==snapshot.maskSourceId||sources.source.kind!=='image'||sources.mask.kind!=='image'||sources.copy.kind!=='image'||sources.mask.width!==maskWidth||sources.mask.height!==maskHeight)throw new Error('Данные маски не соответствуют сохранённым файлам')}
function compatible(a:CompositionSource,b:CompositionSource){return (['id','kind','durationTicks','width','height','hasAudio','audioChannels'] as const).every(key=>a[key]===b[key])}
export function replaceCutoutMedia(document:Composition,clipId:string,sources:CutoutSources,snapshot:CutoutAttachment):Composition{
 check(sources,snapshot);const clip=findClipLocation(document,clipId).clip;if(clip.kind!=='image')throw new Error('Маска доступна только для изображения')
 let staged=document
 for(const source of [sources.source,sources.mask,sources.copy,...(sources.regions??[])]){const existing=staged.sources[source.id];if(existing&&!compatible(existing,source))throw new Error('Метаданные источника маски изменились');if(!existing)staged=registerSource(staged,source)}
 const next=replaceClipMedia(staged,clipId,sources.copy.id,undefined,true)
 const result={...next,metadata:{...next.metadata,mediaVersions:{...next.metadata?.mediaVersions,[clipId]:{...next.metadata!.mediaVersions![clipId]!,cutout:JSON.parse(JSON.stringify(snapshot)) as CutoutAttachment}}}}
 assertValidComposition(result);return result
}
export function replaceNativeCutoutMedia(project:NativeNestedProject,compositionId:string,ids:readonly string[],sources:CutoutSources,snapshot:CutoutAttachment):NativeNestedProject{
 check(sources,snapshot);const node=project.compositions[compositionId];if(!node)throw new Error('Таймлайн не найден');const selected=expandNativeClipLinks(project,compositionId,ids)
 for(const track of node.content.tracks)for(const clip of track.clips)if(selected.includes(clip.id)&&track.kind!=='image')throw new Error('Для маски выберите только связанные изображения')
 const registry={...node.content.sources}
 for(const source of [sources.source,sources.mask,...(sources.regions??[])]){const existing=registry[source.id];if(existing&&!compatible(existing,source))throw new Error('Метаданные источника маски изменились');if(!existing){if(Object.keys(registry).length>=MAX_COMPOSITION_SOURCES)throw new Error('Достигнут предел источников');registry[source.id]={id:source.id,kind:source.kind,durationTicks:source.durationTicks,width:source.width,height:source.height,hasAudio:source.hasAudio}}}
 const staged={...project,compositions:{...project.compositions,[compositionId]:{...node,content:{...node.content,sources:registry}}}}
 const next=replaceNativeMedia(staged,compositionId,ids,sources.copy,true),changed=next.compositions[compositionId]!
 const versions={...changed.content.metadata?.mediaVersions}
 for(const id of selected)versions[id]={...versions[id]!,cutout:JSON.parse(JSON.stringify(snapshot)) as CutoutAttachment}
 return {...next,compositions:{...next.compositions,[compositionId]:{...changed,content:{...changed.content,metadata:{...changed.content.metadata,mediaVersions:versions}}}}}
}
