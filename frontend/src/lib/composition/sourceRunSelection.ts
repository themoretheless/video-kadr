import type {WireConcatParamKey} from './types'
export interface SourceRunEdit {index:number;change:{at?:number;value?:number}}
/** Original source addresses are array indices: duplicates and authored order are significant. */
export function sourceRunDelta(keys:readonly WireConcatParamKey[],indices:readonly number[],deltaAt:number,deltaValue:number):SourceRunEdit[]{
 if(!indices.length||new Set(indices).size!==indices.length||!Number.isFinite(deltaAt)||!Number.isFinite(deltaValue)||indices.some(index=>!Number.isSafeInteger(index)||index<0||index>=keys.length))throw new Error('Некорректное выделение исходных ключей')
 let minimum=-Infinity,maximum=Infinity
 for(const index of indices){minimum=Math.max(minimum,-keys[index]!.at);maximum=Math.min(maximum,1-keys[index]!.at)}
 const shift=deltaAt===0?0:minimum<=maximum?Math.max(minimum,Math.min(maximum,deltaAt)):0
 return indices.map(index=>{const key=keys[index]!,at=key.at+shift,value=key.value+deltaValue;if(!Number.isFinite(at)||!Number.isFinite(value))throw new Error('Значение исходного ключа вне диапазона');return {index,change:{...(at!==key.at?{at}:{}),...(value!==key.value?{value}:{})}}}).filter(edit=>Object.keys(edit.change).length)
}
