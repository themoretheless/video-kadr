import type {CompositionAnimatableValue} from './types'
import {shiftSampleClock} from './keyframeClock'
import {concatEffectNativeKeyOrigin} from './concatEffectInspectorContext'
import {nativeClipStart} from './nativeClipMove'
import type {WireCompositionTrack} from './types'
type Clip=WireCompositionTrack['clips'][number]
const single:Record<string,string>={concat_exposure:'stops',concat_contrast:'amount',concat_saturation:'amount',concat_gaussian_blur:'radius',gamma_correction:'gamma',posterize:'levels',color_invert:'blend',alpha_adjust:'opacity',compound_blur:'max',vr_denoise:'amount',vr_blur:'blurriness',vr_sharpen:'amount',reduce_interlace_flicker:'softness',sharpen:'amount',edge_feather:'amount',camera_blur:'percent',find_edges:'blend',replicate:'count',median:'radius',gaussian_blur:'blurriness',gaussian_blur_legacy:'blurriness',color_replace:'similarity',color_pass:'similarity',color_tint:'amount',color_sharpen:'amount'}
export const scalarObject=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value)
function animation(value:unknown):value is CompositionAnimatableValue{return scalarObject(value)&&(value.mode==='constant'&&typeof value.value==='number'||value.mode==='keyframes'&&scalarObject(value.track)&&Array.isArray(value.track.keyframes))}
function sourceKeys(value:unknown):boolean{return Array.isArray(value)?value.length>0:scalarObject(value)&&Object.values(value).some(sourceKeys)}
export function nativeEffectScalarBindings(clip:Clip){
 if('graphic' in clip&&!clip.concatShape&&!clip.concatTreatment||'text' in clip&&!clip.concatTitle)return []
 if(!('effects' in clip)||!clip.effects||'effectTiming' in clip&&clip.effectTiming||'placement' in clip&&clip.placement.concatRetime)return []
 const origin=concatEffectNativeKeyOrigin(clip),shift=origin-nativeClipStart(clip)
 return clip.effects.flatMap((effect,effectIndex)=>{
  const row=effect as unknown as Record<string,unknown>,value=row.animation
  // Original Concat source runs are sampled before native animation.
  if(sourceKeys(row.concatKeys))return []
  const fields: {property:string;path:string[];value:CompositionAnimatableValue}[]=animation(value)?single[effect.kind]?[{property:single[effect.kind]!,path:['animation'],value}]:[]:scalarObject(value)?Object.entries(value).filter((entry):entry is [string,CompositionAnimatableValue]=>animation(entry[1])).map(([property,value])=>({property,path:['animation',property],value})):[]
  if(scalarObject(row.colorAnimation))for(const [component,value] of Object.entries(row.colorAnimation))if(animation(value))fields.push({property:`colorAnimation.${component}`,path:['colorAnimation',component],value})
  if(effect.kind==='lumetri'&&scalarObject(row.parameters)&&scalarObject(row.parameters.animation)){
   const nested=row.parameters.animation
   for(const section of ['basic','creative','vignette','grade','secondary','sharpen']){
    const channels=nested[section]
    if(section==='sharpen'&&animation(channels))fields.push({property:'lumetri.sharpen',path:['parameters','animation','sharpen'],value:channels})
    else if(scalarObject(channels))for(const [channel,value] of Object.entries(channels))if(animation(value))fields.push({property:`lumetri.${section}.${channel}`,path:['parameters','animation',section,channel],value})
   }
  }
  return fields.map(field=>({...field,effectIndex,effectKind:effect.kind,shift}))
 })
}
export function shiftEffectScalarValue(value:CompositionAnimatableValue,shift:number):CompositionAnimatableValue{
 return !shift||value.mode==='constant'?value:{...value,track:{...value.track,sampleClock:shiftSampleClock(value.track.sampleClock??{rateNum:1,rateDen:1,offsetNum:0},shift,1000000)}}
}
