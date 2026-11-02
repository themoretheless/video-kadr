import type {WireVideoEffect,WireConcatParamKey} from './types'
export type GlowEffect=Extract<WireVideoEffect,{kind:'concat_glow'}>
export type SourceShiftKey=WireConcatParamKey
function validate(key:SourceShiftKey){
 if(!Number.isFinite(key.at)||key.at<0||key.at>1)throw new Error('Доля клипа должна быть от 0 до 1')
 if(!Number.isFinite(key.value)||key.value<10||key.value>100)throw new Error('Свечение должно быть от 10 до 100 %')
 if(Array.isArray(key.ease)&&(!key.ease.every(Number.isFinite)||key.ease[0]!<0||key.ease[0]!>1||key.ease[2]!<0||key.ease[2]!>1))throw new Error('Недопустимая кривая easing')
}
/** Indexed edits keep untouched key order, duplicates and easing spellings. */
export function patchSourceShiftKey(effect:GlowEffect,index:number,change:Partial<SourceShiftKey>):GlowEffect {
 const previous=effect.concatKeys?.[index];if(!previous)throw new Error('Ключ не найден')
 const next={...previous,...change};validate(next)
 return {...effect,concatKeys:effect.concatKeys!.map((key,i)=>i===index?next:key)}
}
/** Original Concat set-key rule: replace the nearest key within0.002, then stable-sort. */
export function setSourceShiftKey(effect:GlowEffect,key:SourceShiftKey):GlowEffect {
 validate(key);const keys=[...(effect.concatKeys??[])]
 let nearest=-1;let distance=Infinity
 keys.forEach((p,index)=>{const d=Math.abs(p.at-key.at);if(d<=0.002&&d<distance){nearest=index;distance=d}})
 if(nearest>=0)keys[nearest]={...keys[nearest],...key};else keys.push(key)
 if(keys.length>4096)throw new Error('Не более 4096 ключей Concat')
 keys.sort((a,b)=>a.at-b.at)
 return {...effect,concatKeys:keys}
}
export function removeSourceShiftKey(effect:GlowEffect,index:number):GlowEffect {
 if(!effect.concatKeys?.[index])throw new Error('Ключ не найден')
 const keys=effect.concatKeys.filter((_,i)=>i!==index)
 return {...effect,concatKeys:keys.length?keys:undefined}
}
