import {registerSource} from './commands'
import {linkedClipIds} from './links'
import {assertValidComposition} from './validation'
import type {Composition} from './types'
import type {NativeNestedProject} from '$lib/api'
import type {CompositionSource,VideoMaskAttachment} from './types'
import {MAX_COMPOSITION_SOURCES} from './types'
import {expandNativeClipLinks} from './nativeClipLinks'
import {validateVideoAttachment} from '../video-mask-recipe'
import {validateVideoRegion} from '../video-region'
import type {SamVideoRegionAttachment} from './types'

/** Add portable tracking resources without changing clip clocks or rendered pixels. */
export function attachNativeVideoRegion(project:NativeNestedProject,compositionId:string,clipId:string,expected:VideoMaskAttachment,source:CompositionSource,mask:CompositionSource,region:SamVideoRegionAttachment):NativeNestedProject {
 validateVideoAttachment(expected);validateVideoRegion(region)
 const node=project.compositions[compositionId];if(!node)throw new Error('Таймлайн не найден')
 if(region.sourceHash!==expected.sourceHash||region.width!==expected.width||region.height!==expected.height||region.frames!==expected.frames||region.frameRate.numerator!==expected.frameRate.numerator||region.frameRate.denominator!==expected.frameRate.denominator)throw new Error('Область не соответствует исходной видеомаске')
 if(source.id!==region.sourceId||mask.id!==region.maskSourceId||source.kind!=='video'||mask.kind!=='video'||mask.hasAudio||mask.width!==256||mask.height!==256||!((source.width===region.width&&source.height===region.height)||(source.width===region.height&&source.height===region.width)))throw new Error('Ресурсы области не соответствуют снимку')
 const ids=expandNativeClipLinks(project,compositionId,[clipId]),versions={...node.content.metadata?.mediaVersions}
 let found=0,hasVideo=false
 for(const track of node.content.tracks)for(const clip of track.clips)if(ids.includes(clip.id)){
  found++;hasVideo ||= track.kind==='video';if(!['video','audio'].includes(track.kind))throw new Error('Выберите видео и его связанный звук');if(track.locked)throw new Error('Дорожка связанного клипа заблокирована')
  const pair=versions[clip.id]
  if(!pair?.videoCutout||JSON.stringify(pair.videoCutout)!==JSON.stringify(expected)||!('sourceId'in clip)||![pair.originalSourceId,pair.copySourceId].includes(clip.sourceId))throw new Error('Сохранённая видеомаска изменилась')
  if([source.id,mask.id].includes(pair.copySourceId)||[expected.sourceId,expected.maskSourceId].includes(mask.id))throw new Error('Ресурс области совпадает с видеомаской')
 }
 if(found!==ids.length||!found||!hasVideo)throw new Error('Клип не найден')
 const retained=expected.trackedRegions??[],old=retained.find(r=>r.maskSourceId===region.maskSourceId)
 if(old&&JSON.stringify(old)!==JSON.stringify(region))throw new Error('Сохранённая область изменилась')
 const attachment={...expected,trackedRegions:old?retained:[...retained,JSON.parse(JSON.stringify(region)) as SamVideoRegionAttachment]};validateVideoAttachment(attachment)
 const sources={...node.content.sources};let changed=!old
 for(const input of [source,mask]){
  const wire={id:input.id,kind:input.kind,durationTicks:input.durationTicks,width:input.width,height:input.height,hasAudio:input.hasAudio,...(input.audioChannels!==undefined?{audioChannels:input.audioChannels}:{})}
  const existing=sources[input.id]
  if(existing&&!(['id','kind','durationTicks','width','height','hasAudio','audioChannels'] as const).every(key=>existing[key]===wire[key]))throw new Error('Метаданные источника области изменились')
  if(!existing){if(Object.keys(sources).length>=MAX_COMPOSITION_SOURCES)throw new Error('Достигнут предел источников');sources[input.id]=wire;changed=true}
 }
 if(!changed)return project
 for(const id of ids)versions[id]={...versions[id]!,videoCutout:JSON.parse(JSON.stringify(attachment)) as VideoMaskAttachment}
 return {...project,compositions:{...project.compositions,[compositionId]:{...node,content:{...node.content,sources,metadata:{...node.content.metadata,mediaVersions:versions}}}}}
}

/** Legacy flat documents retain their full source metadata and explicit link model. */
export function attachCompositionVideoRegion(document:Composition,clipId:string,expected:VideoMaskAttachment,source:CompositionSource,mask:CompositionSource,region:SamVideoRegionAttachment):Composition {
 validateVideoAttachment(expected);validateVideoRegion(region)
 if(region.sourceHash!==expected.sourceHash||region.width!==expected.width||region.height!==expected.height||region.frames!==expected.frames||region.frameRate.numerator!==expected.frameRate.numerator||region.frameRate.denominator!==expected.frameRate.denominator)throw new Error('Область не соответствует исходной видеомаске')
 if(source.id!==region.sourceId||mask.id!==region.maskSourceId||source.kind!=='video'||mask.kind!=='video'||mask.hasAudio||mask.width!==256||mask.height!==256||!((source.width===region.width&&source.height===region.height)||(source.width===region.height&&source.height===region.width)))throw new Error('Ресурсы области не соответствуют снимку')
 const ids=linkedClipIds(document,clipId),versions={...document.metadata?.mediaVersions};let found=0,hasVideo=false
 for(const track of document.tracks)for(const clip of track.clips)if(ids.includes(clip.id)){
  found++;hasVideo ||= clip.kind==='video'
  if(track.locked||clip.locked)throw new Error('Клип или дорожка связанного участника заблокированы')
  const pair=versions[clip.id]
  if(!['video','audio'].includes(clip.kind)||!pair?.videoCutout||JSON.stringify(pair.videoCutout)!==JSON.stringify(expected)||!('sourceId'in clip)||![pair.originalSourceId,pair.copySourceId].includes(clip.sourceId))throw new Error('Сохранённая видеомаска изменилась')
  if([source.id,mask.id].includes(pair.copySourceId)||[expected.sourceId,expected.maskSourceId].includes(mask.id))throw new Error('Ресурс области совпадает с видеомаской')
 }
 if(found!==ids.length||!hasVideo)throw new Error('Клип не найден')
 const retained=expected.trackedRegions??[],old=retained.find(r=>r.maskSourceId===region.maskSourceId)
 if(old&&JSON.stringify(old)!==JSON.stringify(region))throw new Error('Сохранённая область изменилась')
 const attachment={...expected,trackedRegions:old?retained:[...retained,JSON.parse(JSON.stringify(region)) as SamVideoRegionAttachment]};validateVideoAttachment(attachment)
 let staged=document
 for(const input of [source,mask]){
  const existing=staged.sources[input.id]
  if(existing&&!(['id','kind','durationTicks','width','height','hasAudio','audioChannels'] as const).every(key=>existing[key]===input[key]))throw new Error('Метаданные источника области изменились')
  if(!existing)staged=registerSource(staged,input)
 }
 if(old&&staged===document)return document
 for(const id of ids)versions[id]={...versions[id]!,videoCutout:JSON.parse(JSON.stringify(attachment)) as VideoMaskAttachment}
 const result={...staged,metadata:{...staged.metadata,mediaVersions:versions}};assertValidComposition(result);return result
}
