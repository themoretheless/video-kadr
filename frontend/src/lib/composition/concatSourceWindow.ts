// SPDX-License-Identifier: AGPL-3.0-or-later
// Window semantics follow pinned Concat Clip::rewindow_keys / rewindow (KEY_EPSILON=0.002).
import type {CompositionDspEffect,WireConcatParamKey,WireVideoEffect,WireTransform} from './types'
import {sourceEasePoints} from './concatRadiusSourceKeys'
const clamp=(v:number)=>Math.max(0,Math.min(1,v))
function axis(a:number,b:number,t:number){const u=1-t;return 3*u*u*t*a+3*u*t*t*b+t*t*t}
function slope(a:number,b:number,t:number){const u=1-t;return 3*u*u*a+6*u*t*(b-a)+3*t*t*(1-b)}
function eased(key:WireConcatParamKey,x:number):number{
 const[a,b,c,d]=sourceEasePoints(key);x=clamp(x)
 if(Math.abs(a-b)<1e-6&&Math.abs(c-d)<1e-6)return x
 let t=x
 for(let i=0;i<8;i++){const error=axis(a,c,t)-x;if(Math.abs(error)<1e-7)return axis(b,d,t);const derivative=slope(a,c,t);if(Math.abs(derivative)<1e-9)break;t-=error/derivative}
 let lo=0,hi=1;t=x
 for(let i=0;i<32;i++){const at=axis(a,c,t);if(Math.abs(at-x)<1e-7)break;if(at>x)hi=t;else lo=t;t=(lo+hi)/2}
 return axis(b,d,t)
}
export function prepareConcatSourceRun(run:readonly WireConcatParamKey[]):(x:number)=>number{
 const keys=run.filter(k=>Number.isFinite(k.at)&&Number.isFinite(k.value)).map(k=>({...k,at:clamp(k.at)})).sort((a,b)=>a.at-b.at)
 if(!keys.length)throw new Error('Нет ключей Concat')
 return x=>{
 if(!Number.isFinite(x))throw new Error('Недопустимое время ключей Concat')
 const first=keys[0]!;if(x<=first.at)return first.value
 for(let i=1;i<keys.length;i++){const a=keys[i-1]!,b=keys[i]!;if(x<=b.at)return b.at<=a.at?b.value:a.value+(b.value-a.value)*eased(b,(x-a.at)/(b.at-a.at))}
 return keys[keys.length-1]!.value
 }
}
export function sampleConcatSourceRun(run:readonly WireConcatParamKey[],x:number):number{return prepareConcatSourceRun(run)(x)}
export function rewindowConcatSourceRun(run:readonly WireConcatParamKey[],a:number,b:number):readonly WireConcatParamKey[]{
 if(!Number.isFinite(a)||!Number.isFinite(b)||b<=a)throw new Error('Недопустимое окно ключей Concat')
 if(!run.length)return []
 const out:WireConcatParamKey[]=[]
 if(run.some(k=>k.at<a-0.002))out.push({at:0,value:sampleConcatSourceRun(run,a),derived:true,ease:[0,0,1,1]})
 let after:WireConcatParamKey|undefined
 for(const key of run){if(key.at<a-0.002)continue;if(key.at>b+0.002){after??=key;continue}out.push({...key,at:clamp((key.at-a)/(b-a))})}
 if(after)out.push({at:1,value:sampleConcatSourceRun(run,b),derived:true,ease:after.ease})
 return out.filter(k=>Number.isFinite(k.at)&&Number.isFinite(k.value)).sort((a,b)=>a.at-b.at)
}
export function rewindowConcatSourceEffects(effects:readonly WireVideoEffect[]|undefined,oldDuration:number,from:number,to:number):readonly WireVideoEffect[]|undefined{
 if(!Number.isFinite(oldDuration)||oldDuration<=0||!Number.isFinite(from)||!Number.isFinite(to)||to<=from)throw new Error('Недопустимая длительность ключей Concat')
 if(from===0&&to===oldDuration)return effects
 return effects?.map(effect=>{
  if(effect.kind==='native_bypass')return {...effect,effect:rewindowConcatSourceEffects([effect.effect],oldDuration,from,to)![0]!}
  if(effect.kind==='concat_bypass'){
   const keys=Object.fromEntries(Object.entries(effect.link.keys??{}).map(([id,run])=>[id,rewindowConcatSourceRun(run,from/oldDuration,to/oldDuration).map(key=>({...key,...(Array.isArray(key.ease)?{ease:[...key.ease] as [number,number,number,number]}:{})}))]))
   const runs=Object.values(keys);if(runs.some(run=>run.length>4096)||runs.reduce((n,run)=>n+run.length,0)>8192)throw new Error('Обрезка превысит предел ключей отключённого эффекта')
   return {...effect,link:{...effect.link,...(effect.link.params?{params:{...effect.link.params}}:{}),keys}}
  }
  if(!('concatKeys'in effect)||!effect.concatKeys)return effect
  const keys=effect.concatKeys
  const window=(run:readonly WireConcatParamKey[],channel:string)=>{
   const out=rewindowConcatSourceRun(run,from/oldDuration,to/oldDuration)
   // Admission bounds still apply to stored keys. Never commit a window the renderer will reject.
   const authored:Record<string,readonly[number,number]>={concat_exposure:[-4,4],concat_contrast:[-100,100],concat_saturation:[-100,100],concat_color_wheels:[-1,1],concat_adjust:[-1,1]}
   const bounds=effect.kind==='concat_white_balance'?(channel==='temperature'?[2000,11000]:[-100,100]):authored[effect.kind]??[Math.min(...run.map(k=>k.value)),Math.max(...run.map(k=>k.value))]
   const derivedSupported=['concat_exposure','concat_contrast','concat_saturation','concat_white_balance','concat_color_wheels','concat_adjust'].includes(effect.kind)
   if(out.some(k=>(!derivedSupported||!k.derived)&&(k.value<bounds[0]!||k.value>bounds[1]!)))throw new Error('Обрезка создаёт ключ Concat вне допустимого диапазона; измените easing перед обрезкой')
   return out
  }
  const concatKeys=Array.isArray(keys)?window(keys,''):Object.fromEntries(Object.entries(keys).map(([channel,run])=>[channel,window(run,channel)]).filter(([,run])=>(run as readonly WireConcatParamKey[]).length))
  const total=Array.isArray(concatKeys)?concatKeys.length:Object.values(concatKeys as Record<string,readonly WireConcatParamKey[]>).reduce<number>((n,run)=>n+run.length,0)
  if(total>4096)throw new Error('Обрезка превысит предел 4096 ключей Concat')
  return {...effect,concatKeys} as WireVideoEffect
 })
}
/** Original Clip::rewindow_keys also windows inactive audio filter parameter runs. */
export function rewindowDspSourceKeys(effects:readonly CompositionDspEffect[]|undefined,oldDuration:number,from:number,to:number):readonly CompositionDspEffect[]|undefined{
 if(!Number.isFinite(oldDuration)||oldDuration<=0||!Number.isFinite(from)||!Number.isFinite(to)||to<=from)throw new Error('Недопустимое окно исходных ключей DSP')
 if(from===0&&to===oldDuration)return effects
 return effects?.map(effect=>{
  if(!effect.concatSourceKeys)return effect
  const entries=Object.entries(effect.concatSourceKeys).map(([id,run])=>[id,rewindowConcatSourceRun(run,from/oldDuration,to/oldDuration).map(key=>({...key,...(Array.isArray(key.ease)?{ease:[...key.ease] as [number,number,number,number]}:{})}))]as const).filter(([,run])=>run.length)
  if(entries.some(([,run])=>run.length>4096)||entries.reduce((total,[,run])=>total+run.length,0)>8192)throw new Error('Обрезка превысит предел ключей DSP')
  return {...effect,concatSourceKeys:Object.fromEntries(entries)}
 })
}
/** Shared immutable adapter for wire video/image/graphic clips in nested editing. */
export function rewindowNativeClipSourceEffects<T extends object>(clip:T,oldDuration:number,from:number,to:number):T{
 let output=clip
 if('transform'in clip&&clip.transform&&typeof clip.transform==='object'){
  const transform=clip.transform as WireTransform,animation=transform.concatAnimation
  if(animation){
   if(!Number.isSafeInteger(to-from)||to<=from||animation.durationTicks!==oldDuration)throw new Error('Некорректное окно анимации клипа Concat')
   const keys=Object.fromEntries(Object.entries(animation.keys).map(([property,run])=>[property,rewindowConcatSourceRun(run!,from/oldDuration,to/oldDuration).map(key=>({...key,...(Array.isArray(key.ease)?{ease:[...key.ease] as [number,number,number,number]}:{})}))]))
   const runs=Object.values(keys);if(runs.some(run=>run.length>4096)||runs.reduce((n,run)=>n+run.length,0)>8192)throw new Error('Обрезка превысит предел ключей анимации Concat')
   output={...output,transform:{...transform,concatAnimation:{...animation,durationTicks:to-from,keys,rest:{...animation.rest},stretch:[...animation.stretch],canvas:[...animation.canvas]}}}
  }
 }
 if('concatGainKeys'in clip&&Array.isArray(clip.concatGainKeys)&&clip.concatGainKeys.length)output={...output,concatGainKeys:rewindowConcatSourceRun(clip.concatGainKeys,from/oldDuration,to/oldDuration)}
 if('dspEffects'in clip&&Array.isArray(clip.dspEffects)&&clip.dspEffects.length)output={...output,dspEffects:rewindowDspSourceKeys(clip.dspEffects as readonly CompositionDspEffect[],oldDuration,from,to)}
 if('effects'in clip&&Array.isArray(clip.effects)&&clip.effects.length)output={...output,effects:rewindowConcatSourceEffects(clip.effects as readonly WireVideoEffect[],oldDuration,from,to)}
 return output
}

export function rewindowConcatClipAnimation(animation:import('./types').WireConcatClipAnimation,oldDuration:number,from:number,to:number){
 return rewindowNativeClipSourceEffects({transform:{concatAnimation:animation}},oldDuration,from,to).transform.concatAnimation
}
