import type {Composition,WireComposition,WireVideoEffect,GraphicLayer,TextStyle} from './types'
import {cloneGraphicValue} from './graphicLayers'

function plainFamily(value:string):NonNullable<TextStyle['fontFamily']> {
 if(value==='Noto Sans'||value==='Arial Unicode MS'||value==='DejaVu Sans'||value==='Arial')return value
 throw new Error('Неподдерживаемое семейство обычного текста')
}
function effects(before:readonly WireVideoEffect[]|undefined,after:readonly WireVideoEffect[]|undefined):readonly WireVideoEffect[]|undefined {
 if(!before)return before
 if(before.length!==(after?.length??0))throw new Error('Изменился список эффектов')
 return before.map((effect,index)=>{
  const next=after![index]
  if(effect.kind!==next.kind)throw new Error('Изменился тип эффекта')
  return effect.kind==='graphic_text'&&next.kind==='graphic_text'
   ? {...effect,parameters:{...effect.parameters,font:next.parameters.font,fontStyle:next.parameters.fontStyle,runs:next.parameters.runs===undefined?undefined:cloneGraphicValue(next.parameters.runs)}}:effect
 })
}
function layers(before:readonly GraphicLayer[],after:readonly GraphicLayer[]):readonly GraphicLayer[] {
 if(before.length!==after.length)throw new Error('Изменился список слоёв')
 return before.map((layer,index)=>{
  const next=after[index]
  if(layer.uid!==next.uid||layer.content.kind!==next.content.kind)throw new Error('Изменилась структура графики')
  if(layer.content.kind!=='text'||next.content.kind!=='text')return layer
  return {...layer,originalParameters:next.originalParameters===undefined?undefined:cloneGraphicValue(next.originalParameters),content:{...layer.content,parameters:{...layer.content.parameters,font:next.content.parameters.font,fontStyle:next.content.parameters.fontStyle,runs:next.content.parameters.runs===undefined?undefined:cloneGraphicValue(next.content.parameters.runs)}}}
 })
}
/** Apply only native font fields; preserve editor timing, transforms and metadata. */
export function applyFontReplacement(before:Composition,after:WireComposition):Composition {
 if(before.tracks.length!==after.tracks.length)throw new Error('Изменился список дорожек')
 return {...before,tracks:before.tracks.map((track,index)=>{
  const next=after.tracks[index]
  if(track.id!==next.id||track.kind!==next.kind||track.clips.length!==next.clips.length)throw new Error('Изменилась структура композиции')
  if(track.kind==='graphic'&&next.kind==='graphic')return {...track,clips:track.clips.map((clip,i)=>{
   const changed=next.clips[i];if(clip.id!==changed.id)throw new Error('Изменился клип')
   return {...clip,graphic:{...clip.graphic,layers:layers(clip.graphic.layers,changed.graphic.layers)},effects:effects(clip.effects,changed.effects)}
  })}
  if(track.kind==='image'&&next.kind==='image')return {...track,clips:track.clips.map((clip,i)=>{
   const changed=next.clips[i];if(clip.id!==changed.id)throw new Error('Изменился клип')
   return {...clip,effects:effects(clip.effects,changed.effects)}
  })}
  if(track.kind==='text'&&next.kind==='text')return {...track,clips:track.clips.map((clip,i)=>{
   const changed=next.clips[i];if(clip.id!==changed.id)throw new Error('Изменился клип')
   return {...clip,style:{...clip.style,fontFamily:plainFamily(changed.style.fontFamily)}}
  })}
  return track
 })}
}
