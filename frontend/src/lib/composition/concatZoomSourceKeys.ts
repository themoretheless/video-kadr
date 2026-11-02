// SPDX-License-Identifier: AGPL-3.0-or-later
// Original Concat key authoring rules; playback remains in native Rust.
import type {WireVideoEffect,WireConcatParamKey} from './types'
export type ZoomEffect=Extract<WireVideoEffect,{kind:'concat_zoom_blur'}>
export type ZoomChannel='amount'|'x'|'y'
export const zoomKeyChannels={amount:{label:'Сила, %',min:0,max:100,base:25},x:{label:'Центр X, %',min:0,max:100,base:50},y:{label:'Центр Y, %',min:0,max:100,base:50}} as const
function validate(channel:ZoomChannel,key:WireConcatParamKey,position=true){
 const range=zoomKeyChannels[channel]
 if(!Number.isFinite(key.at)||(position&&(key.at<0||key.at>1)))throw new Error('Доля клипа должна быть от 0 до 1')
 if(!Number.isFinite(key.value)||key.value<range.min||key.value>range.max)throw new Error('Значение ключа вне диапазона')
 if(Array.isArray(key.ease)&&(key.ease.length!==4||!key.ease.every(Number.isFinite)||key.ease[0]!<0||key.ease[0]!>1||key.ease[2]!<0||key.ease[2]!>1))throw new Error('Недопустимая кривая easing')
}
function withRun(effect:ZoomEffect,channel:ZoomChannel,run:readonly WireConcatParamKey[]):ZoomEffect {
 const keys={...effect.concatKeys}
 if(run.length)keys[channel]=run;else delete keys[channel]
 if(Object.values(keys).reduce((n,run)=>n+(run?.length??0),0)>4096)throw new Error('Слишком много ключей Concat')
 return {...effect,concatKeys:Object.keys(keys).length?keys:undefined}
}
export function patchZoomSourceKey(effect:ZoomEffect,channel:ZoomChannel,index:number,change:Partial<WireConcatParamKey>):ZoomEffect {
 const keys=effect.concatKeys?.[channel];const old=keys?.[index];if(!keys||!old)throw new Error('Ключ не найден')
 const key={...old,...change};validate(channel,key,change.at!==undefined)
 return withRun(effect,channel,keys.map((p,i)=>i===index?key:p))
}
export function setZoomSourceKey(effect:ZoomEffect,channel:ZoomChannel,key:WireConcatParamKey):ZoomEffect {
 validate(channel,key);const keys=[...(effect.concatKeys?.[channel]??[])];let nearest=-1;let distance=Infinity
 keys.forEach((p,i)=>{const d=Math.abs(p.at-key.at);if(d<=0.002&&d<distance){nearest=i;distance=d}})
 if(nearest>=0)keys[nearest]={...keys[nearest],...key};else keys.push(key)
 keys.sort((a,b)=>a.at-b.at);return withRun(effect,channel,keys)
}
export function removeZoomSourceKey(effect:ZoomEffect,channel:ZoomChannel,index:number):ZoomEffect {
 const keys=effect.concatKeys?.[channel];if(!keys?.[index])throw new Error('Ключ не найден')
 return withRun(effect,channel,keys.filter((_,i)=>i!==index))
}
