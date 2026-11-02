// SPDX-License-Identifier: AGPL-3.0-or-later
// Source behavior: pinned Concat AppliedFilter::set_key / KeyEase; UI data editing only.
import type {GaussianBlurEffect} from './concatGaussianBlurAuthoring'
export type SourceRadiusKey=NonNullable<GaussianBlurEffect['concatKeys']>[number]
export type EasePoints=readonly [number,number,number,number]
export const sourceEasePresets={linear:[0,0,1,1],in:[0.42,0,1,1],out:[0,0,0.58,1],inOut:[0.42,0,0.58,1]} as const
export function sourceEasePoints(key:SourceRadiusKey):EasePoints {
 const value=key.ease
 if(typeof value==='string')return sourceEasePresets[value as keyof typeof sourceEasePresets]??sourceEasePresets.linear
 if(Array.isArray(value)&&value.length===4&&value.every(Number.isFinite))return [Math.max(0,Math.min(1,value[0]!)),value[1]!,Math.max(0,Math.min(1,value[2]!)),value[3]!]
 return sourceEasePresets.linear
}
export function sourceEaseName(key:SourceRadiusKey):string {
 const value=sourceEasePoints(key)
 return Object.entries(sourceEasePresets).find(([,points])=>points.every((p,i)=>Math.abs(p-value[i]!)<0.005))?.[0]??'custom'
}
function check(effect:GaussianBlurEffect){if(effect.animation)throw new Error('Сначала удалите анимацию по шкале времени')}
function validate(key:SourceRadiusKey){
 if(!Number.isFinite(key.at)||key.at<0||key.at>1)throw new Error('Доля клипа должна быть от 0 до 1')
 if(!Number.isFinite(key.value)||key.value<1||key.value>50)throw new Error('Радиус должен быть от 1 до 50 px')
 if(Array.isArray(key.ease)&&(!key.ease.every(Number.isFinite)||key.ease[0]!<0||key.ease[0]!>1||key.ease[2]!<0||key.ease[2]!>1))throw new Error('Недопустимая кривая easing')
}
/** Indexed edits keep untouched key order, duplicates and easing spellings. */
export function patchSourceRadiusKey(effect:GaussianBlurEffect,index:number,change:Partial<SourceRadiusKey>):GaussianBlurEffect {
 check(effect);const previous=effect.concatKeys?.[index];if(!previous)throw new Error('Ключ не найден')
 const next={...previous,...change};validate(next)
 return {...effect,concatKeys:effect.concatKeys!.map((key,i)=>i===index?next:key)}
}
/** Original Concat set-key rule: replace the nearest key within0.002, then stable-sort. */
export function setSourceRadiusKey(effect:GaussianBlurEffect,key:SourceRadiusKey):GaussianBlurEffect {
 check(effect);validate(key);const keys=[...(effect.concatKeys??[])]
 let nearest=-1;let distance=Infinity
 keys.forEach((p,index)=>{const d=Math.abs(p.at-key.at);if(d<=0.002&&d<distance){nearest=index;distance=d}})
 if(nearest>=0)keys[nearest]={...keys[nearest],...key};else keys.push(key)
 keys.sort((a,b)=>a.at-b.at)
 return {...effect,concatKeys:keys}
}
export function removeSourceRadiusKey(effect:GaussianBlurEffect,index:number):GaussianBlurEffect {
 check(effect);if(!effect.concatKeys?.[index])throw new Error('Ключ не найден')
 const keys=effect.concatKeys.filter((_,i)=>i!==index)
 return {...effect,concatKeys:keys.length?keys:undefined}
}
