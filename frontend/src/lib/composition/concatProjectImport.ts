import type {CompositionSource} from './types'
export interface ConcatImportMedia{id:string;name:string;kind:string;placeholder?:boolean}
export interface ConcatImportSource{label:string;source:CompositionSource}
export function concatImportMedia(document:unknown):readonly ConcatImportMedia[]{
 if(!document||typeof document!=='object'||Array.isArray(document))throw new Error('Ожидается документ Concat JSON')
 const media=(document as Record<string,unknown>).media??[]
 if(!Array.isArray(media)||media.length>32)throw new Error('Список медиа должен содержать не более 32 записей')
 const ids=new Set<string>()
 return media.map((entry:unknown)=>{if(!entry||typeof entry!=='object')throw new Error('Некорректное медиа');const item=entry as Record<string,unknown>;if(typeof item.id!=='string'||!item.id||ids.has(item.id))throw new Error('Повторный или пустой ID медиа');ids.add(item.id);return{id:item.id,name:typeof item.name==='string'?item.name:item.id,kind:typeof item.kind==='string'?item.kind:'',...(item.placeholder===true?{placeholder:true}:{})}})
}


export interface ConcatImportFont{family:string;path:string}
export function concatImportFonts(document:unknown):readonly ConcatImportFont[]{
 if(!document||typeof document!=='object'||Array.isArray(document))throw new Error('Ожидается документ Concat JSON')
 const fonts=(document as Record<string,unknown>).fonts??[]
 if(!Array.isArray(fonts)||fonts.length>32)throw new Error('Список шрифтов должен содержать не более 32 файлов')
 const paths=new Set<string>()
 return fonts.map((entry:unknown)=>{if(!entry||typeof entry!=='object')throw new Error('Некорректный шрифт');const font=entry as Record<string,unknown>;if(typeof font.family!=='string'||!font.family.trim()||font.family.length>256||typeof font.path!=='string'||!font.path||font.path.length>4096||paths.has(font.path))throw new Error('Некорректный или повторный путь шрифта');paths.add(font.path);return{family:font.family,path:font.path}})
}
