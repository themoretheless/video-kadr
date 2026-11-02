import type {WireConcatParamKey} from './types'
export interface SourceRunClipboard {format:'video-kadr/concat-source-keys';version:1;keys:readonly WireConcatParamKey[]}
const limit=2*1024*1024
const clone=(key:WireConcatParamKey):WireConcatParamKey=>({...key,...(Array.isArray(key.ease)?{ease:[...key.ease] as [number,number,number,number]}:{})})
function check(value:unknown):asserts value is SourceRunClipboard{
 if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('Некорректный буфер исходных ключей')
 const data=value as Record<string,unknown>
 if(Object.keys(data).some(key=>!['format','version','keys'].includes(key))||data.format!=='video-kadr/concat-source-keys'||data.version!==1||!Array.isArray(data.keys)||!data.keys.length||data.keys.length>4096)throw new Error('Некорректный буфер исходных ключей')
 for(const raw of data.keys){
  if(!raw||typeof raw!=='object'||Array.isArray(raw))throw new Error('Некорректный исходный ключ')
  const key=raw as Record<string,unknown>,ease=key.ease
  if(Object.keys(key).some(name=>!['at','value','ease','derived'].includes(name))||typeof key.at!=='number'||!Number.isFinite(key.at)||key.at<0||key.at>1||typeof key.value!=='number'||!Number.isFinite(key.value)||key.derived!==undefined&&typeof key.derived!=='boolean'||ease!==undefined&&!(typeof ease==='string'&&['linear','in','out','inOut'].includes(ease)||Array.isArray(ease)&&ease.length===4&&ease.every(value=>typeof value==='number'&&Number.isFinite(value))&&ease[0]>=0&&ease[0]<=1&&ease[2]>=0&&ease[2]<=1))throw new Error('Некорректный исходный ключ')
 }
}
export function serializeSourceRunClipboard(value:SourceRunClipboard):string{check(value);const text=JSON.stringify(value);if(new TextEncoder().encode(text).length>limit)throw new Error('Буфер слишком большой');return text}
export function parseSourceRunClipboard(text:string):SourceRunClipboard{if(new TextEncoder().encode(text).length>limit)throw new Error('Буфер слишком большой');let value:unknown;try{value=JSON.parse(text)}catch{throw new Error('Нет исходных ключей Concat в буфере')}check(value);return value}
export function captureSourceRunClipboard(keys:readonly WireConcatParamKey[],indices:readonly number[]):SourceRunClipboard{
 if(!indices.length||new Set(indices).size!==indices.length||indices.some(index=>!Number.isSafeInteger(index)||index<0||index>=keys.length))throw new Error('Выберите исходные ключи')
 const selected=keys.filter((_,index)=>indices.includes(index)),origin=Math.min(...selected.map(key=>key.at))
 return parseSourceRunClipboard(serializeSourceRunClipboard({format:'video-kadr/concat-source-keys',version:1,keys:selected.map(key=>clone({...key,at:key.at-origin}))}))
}
/** Match each old target at most once; duplicate source positions retain their indexed identity. */
export function pasteSourceRunClipboard(keys:readonly WireConcatParamKey[],clipboard:SourceRunClipboard,at:number):WireConcatParamKey[]{
 check(clipboard);if(!Number.isFinite(at)||at<0||at>1)throw new Error('Позиция вставки должна быть от 0 до 1')
 const incoming=clipboard.keys.map(key=>clone({...key,at:at+key.at}));if(incoming.some(key=>key.at>1))throw new Error('Вставка выходит за конец клипа')
 const result=keys.map(clone),used=new Set<number>()
 for(const key of incoming){let nearest=-1,distance=Infinity;keys.forEach((previous,index)=>{const delta=Math.abs(previous.at-key.at);if(!used.has(index)&&delta<=.002&&delta<distance){nearest=index;distance=delta}});if(nearest>=0){result[nearest]=key;used.add(nearest)}else result.push(key)}
 if(result.length>4096)throw new Error('Не более 4096 исходных ключей')
 return result.sort((a,b)=>a.at-b.at)
}
