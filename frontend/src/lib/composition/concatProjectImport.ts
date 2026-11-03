import type {CompositionSource} from './types'
export interface ConcatImportMedia{id:string;name:string;kind:string;placeholder?:boolean}
export interface ConcatImportSource{label:string;source:CompositionSource}
export function concatImportMedia(document:unknown):readonly ConcatImportMedia[]{
 if(!document||typeof document!=='object'||Array.isArray(document))throw new Error('Ожидается документ Concat JSON')
 const media=(document as Record<string,unknown>).media??[]
 if(!Array.isArray(media)||media.length>32)throw new Error('Список медиа должен содержать не более 32 записей')
 const ids=new Set<string>()
 return media.map((entry:unknown)=>{if(!entry||typeof entry!=='object')throw new Error('Некорректное медиа');const item=entry as Record<string,unknown>;if(typeof item.id!=='string'||!item.id||ids.has(item.id))throw new Error('Повторный или пустой ID медиа');ids.add(item.id);return{id:item.id,name:typeof item.name==='string'?item.name:item.id,kind:typeof item.kind==='string'?item.kind:'',...(item.placeholder===true?{placeholder:true}:{})}})
}


export interface ConcatImportFont{family:string;path:string}
export function concatImportFonts(document:unknown):readonly ConcatImportFont[]{
 if(!document||typeof document!=='object'||Array.isArray(document))throw new Error('Ожидается документ Concat JSON')
 const fonts=(document as Record<string,unknown>).fonts??[]
 if(!Array.isArray(fonts)||fonts.length>32)throw new Error('Список шрифтов должен содержать не более 32 файлов')
 const paths=new Set<string>()
 return fonts.map((entry:unknown)=>{if(!entry||typeof entry!=='object')throw new Error('Некорректный шрифт');const font=entry as Record<string,unknown>;if(typeof font.family!=='string'||!font.family.trim()||font.family.length>256||typeof font.path!=='string'||!font.path||font.path.length>4096||paths.has(font.path))throw new Error('Некорректный или повторный путь шрифта');paths.add(font.path);return{family:font.family,path:font.path}})
}

export interface ConcatStillDestination{key:string;timelineId:string;clipId:string;mediaId:string;label:string}
export function concatImportStillDestinations(document:unknown):readonly ConcatStillDestination[]{return cutoutDestinations(document,'image')}
export function concatImportVideoDestinations(document:unknown):readonly ConcatStillDestination[]{return cutoutDestinations(document,'video')}
function cutoutDestinations(document:unknown,kind:string):readonly ConcatStillDestination[]{
 if(!document||typeof document!=='object'||Array.isArray(document))return []
 const doc=document as Record<string,unknown>,timelines=Array.isArray(doc.timelines)&&doc.timelines.length?doc.timelines:[{...doc,id:'TL1'}]
 const result:ConcatStillDestination[]=[]
 for(const raw of timelines){if(!raw||typeof raw!=='object')continue;const timeline=raw as Record<string,unknown>;if(typeof timeline.id!=='string'||!Array.isArray(timeline.clips))continue
  for(const value of timeline.clips){if(!value||typeof value!=='object')continue;const clip=value as Record<string,unknown>;if(clip.kind!==kind||clip.cutout==null||typeof clip.id!=='string'||typeof clip.mediaId!=='string')continue
   result.push({key:JSON.stringify([timeline.id,clip.id]),timelineId:timeline.id,clipId:clip.id,mediaId:clip.mediaId,label:`${typeof timeline.name==='string'?timeline.name:timeline.id} / ${typeof clip.name==='string'?clip.name:clip.id}`})
  }
 }
 return result
}
export interface SavedStillProject{id:string;name:string;revision:number;document:unknown}
export interface SavedStillChoice{key:string;label:string;sourceId:string;reference:Omit<import('../api').ConcatSavedStillReference,'timelineId'|'clipId'>}
export function savedStillChoices(projects:readonly SavedStillProject[]):readonly SavedStillChoice[]{
 const result:SavedStillChoice[]=[]
 for(const project of projects){
  if(!project.document||typeof project.document!=='object'||!Number.isSafeInteger(project.revision))continue
  const doc=project.document as import('../api').NativeNestedProject
  const nested=doc.compositions&&typeof doc.compositions==='object'
  const nodes=nested?Object.entries(doc.compositions).map(([id,node])=>({id,content:node?.content})):[{id:null,content:project.document as import('./types').Composition}]
  for(const node of nodes){const content=node.content;if(!content||!Array.isArray(content.tracks))continue
   for(const track of content.tracks){if(track.kind!=='image')continue
    for(const clip of track.clips){const pair=content.metadata?.mediaVersions?.[clip.id];if(!pair?.cutout||clip.sourceId!==pair.originalSourceId&&clip.sourceId!==pair.copySourceId)continue
     const original=content.sources?.[pair.originalSourceId];if(original?.kind!=='image')continue
     result.push({key:JSON.stringify([project.id,node.id,clip.id]),label:`${project.name} / ${content.metadata?.sequenceName??node.id??'Таймлайн'} / ${content.metadata?.clipNames?.[clip.id]??clip.id}`,sourceId:nested?doc.projectMedia?.[original.id]?.librarySourceId??original.id:original.id,reference:{projectId:project.id,compositionId:node.id,sourceClipId:clip.id,expectedRevision:project.revision}})
    }
   }
  }
 }
 return result
}

export function savedVideoChoices(projects:readonly SavedStillProject[]):readonly SavedStillChoice[]{
 const result:SavedStillChoice[]=[]
 for(const project of projects){
  if(!project.document||typeof project.document!=='object'||!Number.isSafeInteger(project.revision)||project.revision<1)continue
  const doc=project.document as import('../api').NativeNestedProject
  const nested=doc.compositions&&typeof doc.compositions==='object'
  const nodes=nested?Object.entries(doc.compositions).map(([id,node])=>({id,content:node?.content})):[{id:null,content:project.document as import('./types').Composition}]
  for(const node of nodes){const content=node.content;if(!content||!Array.isArray(content.tracks))continue
   for(const track of content.tracks){if(track.kind!=='video')continue
    for(const clip of track.clips){if(!clip.concatCutout||clip.concatCutout.origin)continue
     const source=content.sources?.[clip.sourceId];if(source?.kind!=='video')continue
     result.push({key:JSON.stringify([project.id,node.id,clip.id]),label:`${project.name} / ${content.metadata?.sequenceName??node.id??'Таймлайн'} / ${content.metadata?.clipNames?.[clip.id]??clip.id}`,sourceId:nested?doc.projectMedia?.[source.id]?.librarySourceId??source.id:source.id,reference:{projectId:project.id,compositionId:node.id,sourceClipId:clip.id,expectedRevision:project.revision}})
    }
   }
  }
 }
 return result
}
