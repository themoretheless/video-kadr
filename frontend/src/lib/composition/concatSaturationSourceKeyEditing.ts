// SPDX-License-Identifier: AGPL-3.0-or-later
import type {WireVideoEffect,WireConcatParamKey} from './types'
export type SaturationEffect=Extract<WireVideoEffect,{kind:'concat_saturation'}>
export type SourceSaturationKey=WireConcatParamKey
export {sourceEasePoints,sourceEaseName,sourceEasePresets,type EasePoints} from './concatRadiusSourceKeys'
function check(effect:SaturationEffect){if(effect.animation)throw new Error('Сначала удалите анимацию по шкале времени')}
function validate(key:SourceSaturationKey){
 if(!Number.isFinite(key.at)||key.at<0||key.at>1)throw new Error('Доля клипа должна быть от 0 до 1')
 if(!Number.isFinite(key.value)||(!key.derived&&(key.value< -100||key.value>100)))throw new Error('Насыщенность должна быть от -100 до 100')
 if(Array.isArray(key.ease)&&(!key.ease.every(Number.isFinite)||key.ease[0]!<0||key.ease[0]!>1||key.ease[2]!<0||key.ease[2]!>1))throw new Error('Недопустимая кривая easing')
}
/** Indexed edits keep untouched key order, duplicates and easing spellings. */
export function patchSourceSaturationKey(effect:SaturationEffect,index:number,change:Partial<SourceSaturationKey>):SaturationEffect {
 check(effect);const previous=effect.concatKeys?.[index];if(!previous)throw new Error('Ключ не найден')
 const next={...previous,...change,...(change.value===undefined?{}:{derived:undefined})};validate(next)
 return {...effect,concatKeys:effect.concatKeys!.map((key,i)=>i===index?next:key)}
}
/** Original Concat set-key rule: replace the nearest key within0.002, then stable-sort. */
export function setSourceSaturationKey(effect:SaturationEffect,key:SourceSaturationKey):SaturationEffect {
 key={...key,derived:undefined};check(effect);validate(key);const keys=[...(effect.concatKeys??[])]
 let nearest=-1;let distance=Infinity
 keys.forEach((p,index)=>{const d=Math.abs(p.at-key.at);if(d<=0.002&&d<distance){nearest=index;distance=d}})
 if(nearest>=0)keys[nearest]={...keys[nearest],...key,derived:undefined};else keys.push(key)
 if(keys.length>4096)throw new Error("Не более 4096 ключей Concat")
 keys.sort((a,b)=>a.at-b.at)
 return {...effect,concatKeys:keys}
}
export function removeSourceSaturationKey(effect:SaturationEffect,index:number):SaturationEffect {
 check(effect);if(!effect.concatKeys?.[index])throw new Error('Ключ не найден')
 const keys=effect.concatKeys.filter((_,i)=>i!==index)
 return {...effect,concatKeys:keys.length?keys:undefined}
}
