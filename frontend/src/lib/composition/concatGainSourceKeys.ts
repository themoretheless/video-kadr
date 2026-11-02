// Source key editing follows pinned Concat Clip::upsert_key (epsilon 0.002).
import type {WireConcatParamKey} from './types'
export function cloneGainSourceKeys(keys:readonly WireConcatParamKey[]):WireConcatParamKey[]{
 if(keys.length>4096)throw new Error('Не более 4096 ключей громкости')
 return keys.map(key=>{
  if(!Number.isFinite(key.at)||key.at<0||key.at>1||!Number.isFinite(key.value)||(!key.derived&&key.value<0))throw new Error('Недопустимый ключ громкости')
  if(Array.isArray(key.ease)&&(key.ease.length!==4||!key.ease.every(Number.isFinite)||key.ease[0]!<0||key.ease[0]!>1||key.ease[2]!<0||key.ease[2]!>1))throw new Error('Недопустимый easing')
  return {...key,...(Array.isArray(key.ease)?{ease:[...key.ease] as [number,number,number,number]}:{})}
 })
}
export function patchGainSourceKey(keys:readonly WireConcatParamKey[],index:number,patch:Partial<WireConcatParamKey>):WireConcatParamKey[]{
 if(!Number.isInteger(index)||!keys[index])throw new Error('Ключ громкости не найден')
 return cloneGainSourceKeys(keys.map((key,i)=>i===index?{...key,...patch,...(patch.value===undefined?{}:{derived:false})}:key)).sort((a,b)=>a.at-b.at)
}
export function upsertGainSourceKey(keys:readonly WireConcatParamKey[],at:number,value:number):WireConcatParamKey[]{
 let nearest=-1,distance=Infinity
 keys.forEach((key,i)=>{const d=Math.abs(key.at-at);if(d<=0.002&&d<distance){nearest=i;distance=d}})
 return nearest>=0?patchGainSourceKey(keys,nearest,{at,value}):cloneGainSourceKeys([...keys,{at,value}]).sort((a,b)=>a.at-b.at)
}
