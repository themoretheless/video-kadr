// SPDX-License-Identifier: AGPL-3.0-or-later
// Original Concat key authoring rules; playback remains in native Rust.
import type {WireVideoEffect,WireConcatParamKey} from './types'
export type ChromaEffect=Extract<WireVideoEffect,{kind:'concat_chroma_key'}>
export type ChromaChannel='color'|'similarity'|'softness'|'spill'
export const chromaKeyChannels={color:{label:'Цвет ключа',min:0,max:4294967295,base:0x00b140ff},similarity:{label:'Сходство, %',min:1,max:60,base:30},softness:{label:'Мягкость, %',min:0,max:50,base:10},spill:{label:'Подавление оттенка, %',min:0,max:100,base:50}} as const
function validate(channel:ChromaChannel,key:WireConcatParamKey,position=true){
 const range=chromaKeyChannels[channel]
 if(!Number.isFinite(key.at)||(position&&(key.at<0||key.at>1)))throw new Error('Доля клипа должна быть от 0 до 1')
 if(!Number.isFinite(key.value)||key.value<range.min||key.value>range.max)throw new Error('Значение ключа вне диапазона')
 if(Array.isArray(key.ease)&&(key.ease.length!==4||!key.ease.every(Number.isFinite)||key.ease[0]!<0||key.ease[0]!>1||key.ease[2]!<0||key.ease[2]!>1))throw new Error('Недопустимая кривая easing')
}
function withRun(effect:ChromaEffect,channel:ChromaChannel,run:readonly WireConcatParamKey[]):ChromaEffect {
 const keys={...effect.concatKeys}
 if(run.length)keys[channel]=run;else delete keys[channel]
 if(Object.values(keys).reduce((n,run)=>n+(run?.length??0),0)>4096)throw new Error('Слишком много ключей Concat')
 return {...effect,concatKeys:Object.keys(keys).length?keys:undefined}
}
export function patchChromaSourceKey(effect:ChromaEffect,channel:ChromaChannel,index:number,change:Partial<WireConcatParamKey>):ChromaEffect {
 const keys=effect.concatKeys?.[channel];const old=keys?.[index];if(!keys||!old)throw new Error('Ключ не найден')
 const key={...old,...change};validate(channel,key,change.at!==undefined)
 return withRun(effect,channel,keys.map((p,i)=>i===index?key:p))
}
export function setChromaSourceKey(effect:ChromaEffect,channel:ChromaChannel,key:WireConcatParamKey):ChromaEffect {
 validate(channel,key);const keys=[...(effect.concatKeys?.[channel]??[])];let nearest=-1;let distance=Infinity
 keys.forEach((p,i)=>{const d=Math.abs(p.at-key.at);if(d<=0.002&&d<distance){nearest=i;distance=d}})
 if(nearest>=0)keys[nearest]={...keys[nearest],...key};else keys.push(key)
 keys.sort((a,b)=>a.at-b.at);return withRun(effect,channel,keys)
}
export function removeChromaSourceKey(effect:ChromaEffect,channel:ChromaChannel,index:number):ChromaEffect {
 const keys=effect.concatKeys?.[channel];if(!keys?.[index])throw new Error('Ключ не найден')
 return withRun(effect,channel,keys.filter((_,i)=>i!==index))
}
function packed(value:number){return Math.trunc(Math.max(0,Math.min(4294967295,value)))}
export function chromaRgb(value:number):string{return '#'+Math.floor(packed(value)/256).toString(16).padStart(6,'0')}
export function chromaAlpha(value:number):number{return packed(value)%256}
export function chromaWithRgb(value:number,hex:string):number {
 if(!/^#[0-9a-f]{6}$/i.test(hex))throw new Error('Недопустимый цвет')
 return parseInt(hex.slice(1),16)*256+chromaAlpha(value)
}
export function chromaWithAlpha(value:number,alpha:number):number {
 if(!Number.isInteger(alpha)||alpha<0||alpha>255)throw new Error('Альфа должна быть от 0 до 255')
 return Math.floor(packed(value)/256)*256+alpha
}
