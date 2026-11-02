import type {NativeNestedProject} from '$lib/api'
import {registerSource} from './commands'
import {linkedClipIds} from './links'
import {replaceEnhancedVideo} from './enhancedVideoReplace'
import {replaceNativeMedia} from './nativeMediaReplace'
import {expandNativeClipLinks} from './nativeClipLinks'
import {assertValidComposition} from './validation'
import {validateVideoAttachment} from '../video-mask-recipe'
import {MAX_COMPOSITION_SOURCES,type Composition,type CompositionSource,type VideoMaskAttachment} from './types'
export interface VideoCutoutSources{copy:CompositionSource;source:CompositionSource;mask:CompositionSource}
function check(s:VideoCutoutSources,snapshot:VideoMaskAttachment){validateVideoAttachment(snapshot);if(s.copy.kind!=='video'||s.source.kind!=='video'||s.mask.kind!=='video'||s.mask.hasAudio||s.source.id!==snapshot.sourceId||s.mask.id!==snapshot.maskSourceId||s.copy.id===s.source.id||s.copy.id===s.mask.id||s.mask.width!==snapshot.maskWidth||s.mask.height!==snapshot.maskHeight||s.copy.width!==snapshot.width||s.copy.height!==snapshot.height||!((s.source.width===snapshot.width&&s.source.height===snapshot.height)||(s.source.width===snapshot.height&&s.source.height===snapshot.width)))throw new Error('Ресурсы видеомаски не соответствуют снимку')}
function compatible(a:CompositionSource,b:CompositionSource){return (['id','kind','durationTicks','width','height','hasAudio','audioChannels'] as const).every(key=>a[key]===b[key])}
function pair(s:VideoCutoutSources,snapshot:VideoMaskAttachment){return{originalSourceId:s.source.id,copySourceId:s.copy.id,videoCutout:JSON.parse(JSON.stringify(snapshot)) as VideoMaskAttachment}}
export function replaceVideoCutoutMedia(document:Composition,clipId:string,s:VideoCutoutSources,snapshot:VideoMaskAttachment):Composition{
 check(s,snapshot);const ids=linkedClipIds(document,clipId);let staged=document
 for(const source of [s.source,s.mask,s.copy]){const old=staged.sources[source.id];if(old&&!compatible(old,source))throw new Error('Метаданные источника видеомаски изменились');if(!old)staged=registerSource(staged,source)}
 const next=replaceEnhancedVideo(staged,clipId,s.copy),versions={...next.metadata?.mediaVersions}
 for(const id of ids)versions[id]=pair(s,snapshot)
 const result={...next,metadata:{...next.metadata,mediaVersions:versions}};assertValidComposition(result);return result
}
export function replaceNativeVideoCutoutMedia(project:NativeNestedProject,id:string,ids:readonly string[],s:VideoCutoutSources,snapshot:VideoMaskAttachment):NativeNestedProject{
 check(s,snapshot);const node=project.compositions[id];if(!node)throw new Error('Таймлайн не найден');const selected=expandNativeClipLinks(project,id,ids)
 let sourceId:string|undefined,hasVideo=false
 for(const track of node.content.tracks)for(const clip of track.clips)if(selected.includes(clip.id)){if((track.kind!=='video'&&track.kind!=='audio')||!('sourceId'in clip))throw new Error('Выберите связанные видео и звук');if(sourceId!==undefined&&sourceId!==clip.sourceId)throw new Error('Связанные клипы используют разные исходники');sourceId=clip.sourceId;hasVideo ||= track.kind==='video'}
 if(!hasVideo)throw new Error('Выберите видеоклип')
 const registry={...node.content.sources}
 for(const source of [s.source,s.mask,s.copy]){const old=registry[source.id];if(old&&!compatible(old,source))throw new Error('Метаданные источника видеомаски изменились');if(!old){if(Object.keys(registry).length>=MAX_COMPOSITION_SOURCES)throw new Error('Достигнут предел источников');registry[source.id]={id:source.id,kind:source.kind,durationTicks:source.durationTicks,width:source.width,height:source.height,hasAudio:source.hasAudio,...(source.audioChannels!==undefined?{audioChannels:source.audioChannels}:{})}}}
 const staged={...project,compositions:{...project.compositions,[id]:{...node,content:{...node.content,sources:registry}}}}
 const next=replaceNativeMedia(staged,id,ids,s.copy,true),changed=next.compositions[id]!,versions={...changed.content.metadata?.mediaVersions}
 for(const clipId of selected)versions[clipId]=pair(s,snapshot)
 return {...next,compositions:{...next.compositions,[id]:{...changed,content:{...changed.content,metadata:{...changed.content.metadata,mediaVersions:versions}}}}}
}
