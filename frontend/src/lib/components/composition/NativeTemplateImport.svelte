<script lang="ts">
 import {inspectNativeTemplatePackage,importNativeTemplate,type NativeTemplatePackageInfo,type NativeTemplateImportResponse} from '$lib/api'
 import type {ConcatImportSource} from '$lib/composition/concatProjectImport'
 let {token,spaceId,context,disabled=false,sources=[],oncreated}:{token:string|null;spaceId?:string;context:unknown;disabled?:boolean;sources?:readonly ConcatImportSource[];oncreated:(result:NativeTemplateImportResponse,guard:()=>void)=>Promise<void>}=$props()
 let file=$state<File|null>(null),info=$state<NativeTemplatePackageInfo|null>(null),choices=$state<Record<string,string>>({}),busy=$state(false),error=$state(''),message=$state(''),progress=$state('')
 let epoch=0,controller:AbortController|undefined
 $effect(()=>{void token;void spaceId;void context;epoch++;controller?.abort();file=null;info=null;choices={};error='';message='';busy=false;return()=>{epoch++;controller?.abort()}})
 function choose(event:Event){controller?.abort();epoch++;file=(event.currentTarget as HTMLInputElement).files?.[0]??null;(event.currentTarget as HTMLInputElement).value='';info=null;choices={};error='';message=''}
 async function inspect(){
  if(!token||!file||busy||disabled)return
  const serial=epoch,request=new AbortController();controller=request;busy=true;error='';progress='Проверяю шаблон…'
  try{const result=await inspectNativeTemplatePackage(file,token,request.signal);if(serial===epoch&&!request.signal.aborted)info=result}
  catch(reason){if(serial===epoch&&!request.signal.aborted)error=reason instanceof Error?reason.message:String(reason)}
  finally{if(serial===epoch)busy=false}
 }
 const ready=$derived(!!info&&info.template.slots.every(slot=>sources.some(entry=>entry.source.id===choices[slot.mediaId])))
 async function create(){
  if(!token||!file||!info||!ready||busy||disabled)return
  const serial=epoch,request=new AbortController();controller=request;busy=true;error='';message='';progress='Создаю проект…'
  const guard=()=>{if(serial!==epoch||request.signal.aborted)throw new Error('Контекст создания проекта изменился')}
  try{
   const fills=info.template.slots.map(slot=>({mediaId:slot.mediaId,sourceId:choices[slot.mediaId]!}))
   const result=await importNativeTemplate(file,fills,token,spaceId,request.signal);guard()
   // Do not offer another creation if opening the saved project subsequently fails.
   info=null;file=null;choices={};message=`Создан проект «${result.project.name}»`
   progress='Открываю проект…';await oncreated(result,guard)
  }catch(reason){if(serial===epoch&&!request.signal.aborted)error=reason instanceof Error?reason.message:String(reason)}
  finally{if(serial===epoch)busy=false}
 }
</script>
<details>
 <summary>Создать проект из шаблона</summary>
 <label>Пакет шаблона <input type="file" accept=".vktemplate" disabled={disabled||busy||!token} onchange={choose}/></label>
 {#if file}<p>{file.name}</p>{/if}
 <button class="btn ghost sm" disabled={disabled||busy||!token||!file} onclick={()=>void inspect()}>Проверить шаблон</button>
 {#if info}
  <p>{info.name} · {info.template.width} × {info.template.height}</p>
  {#each info.template.slots as slot(slot.mediaId)}
   <label>{slot.name} · {slot.kind==='image'?'Изображение':slot.kind==='video'?'Видео':'Аудио'} · {slot.seconds.toLocaleString('ru-RU',{maximumFractionDigits:3})} с
    <select aria-label={`Файл для слота ${slot.name}`} value={choices[slot.mediaId]??''} disabled={disabled||busy} onchange={event=>{choices={...choices,[slot.mediaId]:event.currentTarget.value}}}>
     <option value="">Выберите файл из библиотеки</option>
     {#each sources as entry(entry.source.id)}<option value={entry.source.id}>{entry.label}</option>{/each}
    </select>
   </label>
  {/each}
  <button class="btn primary sm" disabled={disabled||busy||!token||!ready} onclick={()=>void create()}>Создать проект из выбранного шаблона</button>
 {/if}
 {#if busy}<p role="status">{progress}</p>{/if}
 {#if message}<p role="status">{message}</p>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</details>
