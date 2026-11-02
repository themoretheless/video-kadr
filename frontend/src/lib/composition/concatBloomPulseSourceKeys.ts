// SPDX-License-Identifier: AGPL-3.0-or-later
// Original Concat key authoring rules; playback remains in native Rust.
import type {WireVideoEffect,WireConcatParamKey} from './types'
export type BloomPulseEffect=Extract<WireVideoEffect,{kind:'concat_bloom_pulse'}>
export type BloomPulseChannel='amount'|'speed'
export const bloomPulseKeyChannels={amount:{label:'Сила, %',min:0,max:100,base:50},speed:{label:'Скорость',min:0.5,max:5,base:2}} as const
function validate(channel:BloomPulseChannel,key:WireConcatParamKey,position=true){
 const range=bloomPulseKeyChannels[channel]
 if(!Number.isFinite(key.at)||(position&&(key.at<0||key.at>1)))throw new Error('Доля клипа должна быть от 0 до 1')
 if(!Number.isFinite(key.value)||key.value<range.min||key.value>range.max)throw new Error('Значение ключа вне диапазона')
 if(Array.isArray(key.ease)&&(key.ease.length!==4||!key.ease.every(Number.isFinite)||key.ease[0]!<0||key.ease[0]!>1||key.ease[2]!<0||key.ease[2]!>1))throw new Error('Недопустимая кривая easing')
}
function withRun(effect:BloomPulseEffect,channel:BloomPulseChannel,run:readonly WireConcatParamKey[]):BloomPulseEffect {
 const keys={...effect.concatKeys}
 if(run.length)keys[channel]=run;else delete keys[channel]
 if(Object.values(keys).reduce((n,run)=>n+(run?.length??0),0)>4096)throw new Error('Слишком много ключей Concat')
 return {...effect,concatKeys:Object.keys(keys).length?keys:undefined}
}
export function patchBloomPulseSourceKey(effect:BloomPulseEffect,channel:BloomPulseChannel,index:number,change:Partial<WireConcatParamKey>):BloomPulseEffect {
 const keys=effect.concatKeys?.[channel];const old=keys?.[index];if(!keys||!old)throw new Error('Ключ не найден')
 const key={...old,...change};validate(channel,key,change.at!==undefined)
 return withRun(effect,channel,keys.map((p,i)=>i===index?key:p))
}
export function setBloomPulseSourceKey(effect:BloomPulseEffect,channel:BloomPulseChannel,key:WireConcatParamKey):BloomPulseEffect {
 validate(channel,key);const keys=[...(effect.concatKeys?.[channel]??[])];let nearest=-1;let distance=Infinity
 keys.forEach((p,i)=>{const d=Math.abs(p.at-key.at);if(d<=0.002&&d<distance){nearest=i;distance=d}})
 if(nearest>=0)keys[nearest]={...keys[nearest],...key};else keys.push(key)
 keys.sort((a,b)=>a.at-b.at);return withRun(effect,channel,keys)
}
export function removeBloomPulseSourceKey(effect:BloomPulseEffect,channel:BloomPulseChannel,index:number):BloomPulseEffect {
 const keys=effect.concatKeys?.[channel];if(!keys?.[index])throw new Error('Ключ не найден')
 return withRun(effect,channel,keys.filter((_,i)=>i!==index))
}
