import type {WireTextClip,WireGraphicClip,WireCanvas,WireConcatClipAnimation,WireAnimatableValue} from './types'
/** Start source animation from the authored native constant pose without flattening native tracks. */
export function initializeConcatVisualAnimation(clip:WireTextClip|WireGraphicClip,canvas:WireCanvas):WireConcatClipAnimation{
 if(!('concatTitle' in clip&&clip.concatTitle)&&!('concatShape' in clip&&clip.concatShape))throw new Error('Нужен титр или фигура Concat')
 if(clip.transform.concatAnimation)return clip.transform.concatAnimation
 if(clip.transform.position)throw new Error('Сначала удалите нативную анимацию пути клипа')
 const constant=(value:WireAnimatableValue):number=>{
  if(value.mode!=='constant'||!Number.isFinite(value.value))throw new Error('Нативные ключи клипа нельзя заменить исходной анимацией Concat')
  return value.value
 }
 const {transform}=clip,x=constant(transform.x),y=constant(transform.y),scaleX=constant(transform.scaleX),scaleY=constant(transform.scaleY),rotationDegrees=constant(transform.rotationDegrees),opacity=constant(clip.opacity)
 const durationTicks=clip.timelineEndTick-clip.timelineStartTick
 if(!Number.isSafeInteger(durationTicks)||durationTicks<=0||canvas.width<=0||canvas.height<=0)throw new Error('Некорректная длительность или размер кадра')
 return {durationTicks,keys:{},rest:{scaleX:1,scaleY:1,x:x/canvas.width,y:y/canvas.height,rotationDegrees,opacity},stretch:[scaleX,scaleY],canvas:[canvas.width,canvas.height]}
}

export const initializeConcatTitleAnimation=initializeConcatVisualAnimation
