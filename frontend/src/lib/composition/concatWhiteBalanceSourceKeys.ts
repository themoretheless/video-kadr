// SPDX-License-Identifier: AGPL-3.0-or-later
// Original Concat key authoring rules; playback remains in native Rust.
import type {WireVideoEffect,WireConcatParamKey} from './types'
export type WhiteBalanceEffect=Extract<WireVideoEffect,{kind:'concat_white_balance'}>
export type WhiteBalanceChannel='temperature'|'tint'
export const whiteBalanceKeyChannels={temperature:{label:'Температура, K',min:2000,max:11000,base:6500},tint:{label:'Tint',min:-100,max:100,base:0}} as const
function validate(channel:WhiteBalanceChannel,key:WireConcatParamKey,position=true){
 const range=whiteBalanceKeyChannels[channel]
 if(!Number.isFinite(key.at)||(position&&(key.at<0||key.at>1)))throw new Error('Доля клипа должна быть от 0 до 1')
 if(!Number.isFinite(key.value)||(!key.derived&&(key.value<range.min||key.value>range.max)))throw new Error('Значение ключа вне диапазона')
 if(Array.isArray(key.ease)&&(key.ease.length!==4||!key.ease.every(Number.isFinite)||key.ease[0]!<0||key.ease[0]!>1||key.ease[2]!<0||key.ease[2]!>1))throw new Error('Недопустимая кривая easing')
}
function withRun(effect:WhiteBalanceEffect,channel:WhiteBalanceChannel,run:readonly WireConcatParamKey[]):WhiteBalanceEffect {
 if(effect.animation)throw new Error('Сначала удалите анимацию по шкале времени')
 const keys={...effect.concatKeys}
 if(run.length)keys[channel]=run;else delete keys[channel]
 if(Object.values(keys).reduce((n,run)=>n+(run?.length??0),0)>4096)throw new Error('Слишком много ключей Concat')
 return {...effect,concatKeys:Object.keys(keys).length?keys:undefined}
}
export function patchWhiteBalanceSourceKey(effect:WhiteBalanceEffect,channel:WhiteBalanceChannel,index:number,change:Partial<WireConcatParamKey>):WhiteBalanceEffect {
 const keys=effect.concatKeys?.[channel];const old=keys?.[index];if(!keys||!old)throw new Error('Ключ не найден')
 const key={...old,...change,...(change.value===undefined?{}:{derived:undefined})};validate(channel,key,change.at!==undefined)
 return withRun(effect,channel,keys.map((p,i)=>i===index?key:p))
}
export function setWhiteBalanceSourceKey(effect:WhiteBalanceEffect,channel:WhiteBalanceChannel,key:WireConcatParamKey):WhiteBalanceEffect {
 key={...key,derived:undefined};validate(channel,key);const keys=[...(effect.concatKeys?.[channel]??[])];let nearest=-1;let distance=Infinity
 keys.forEach((p,i)=>{const d=Math.abs(p.at-key.at);if(d<=0.002&&d<distance){nearest=i;distance=d}})
 if(nearest>=0)keys[nearest]={...keys[nearest],...key,derived:undefined};else keys.push(key)
 keys.sort((a,b)=>a.at-b.at);return withRun(effect,channel,keys)
}
export function removeWhiteBalanceSourceKey(effect:WhiteBalanceEffect,channel:WhiteBalanceChannel,index:number):WhiteBalanceEffect {
 const keys=effect.concatKeys?.[channel];if(!keys?.[index])throw new Error('Ключ не найден')
 return withRun(effect,channel,keys.filter((_,i)=>i!==index))
}
