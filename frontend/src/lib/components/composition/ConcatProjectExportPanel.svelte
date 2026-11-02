<script lang="ts">
 import {exportConcatProject} from '$lib/api'
 import {nestedComposition,nestedHistory} from '$lib/state/nestedComposition.svelte'
 let {disabled=false}:{disabled?:boolean}=$props()
 let mediaPaths=$state<Record<string,string>>({})
 const sourceIds=$derived([...new Set(Object.values(nestedComposition.document?.compositions??{}).flatMap(node=>Object.keys(node.content.sources)))].sort())
 let busy=$state(false),error=$state(''),controller:AbortController|undefined
 $effect(()=>{const project=nestedComposition.document,scope=nestedComposition.activeCompositionId;void project;void scope;mediaPaths={};return()=>controller?.abort()})
 async function download(){
  const project=nestedComposition.document,scope=nestedComposition.activeCompositionId??project?.root
  if(disabled||busy||nestedHistory.busy||!project||!scope)return
  const request=new AbortController();controller=request;busy=true;error=''
  try{
   const result=await exportConcatProject({...project,root:scope},nestedComposition.name,request.signal,Object.fromEntries(sourceIds.map(id=>[id,mediaPaths[id]??''])))
   if(request.signal.aborted||nestedComposition.document!==project)return
   const url=URL.createObjectURL(new Blob([JSON.stringify(result.document)],{type:'application/json'})),link=document.createElement('a')
   try{link.href=url;link.download='concat.json';link.click()}finally{URL.revokeObjectURL(url)}
  }catch(cause){if(!request.signal.aborted)error=cause instanceof Error?cause.message:'Не удалось экспортировать Concat'}
  finally{if(controller===request){controller=undefined;busy=false}}
 }
</script>
{#if nestedComposition.document}
 <details><summary>Экспорт проекта Concat</summary>
  <p>Экспортирует клипы, звук, титры, фигуры и корректирующие слои в JSON Concat. Для медиа укажите путь к файлу на компьютере, где откроется Concat. Файлы не копируются. Вложенные композиции и встроенные ресурсы пока не поддерживаются. Несовместимые параметры будут указаны в ошибке экспорта.</p>
  {#each sourceIds as id (id)}<label>Путь медиа Concat: {id}<input aria-label={`Путь медиа Concat: ${id}`} value={mediaPaths[id]??''} disabled={busy||disabled} oninput={event=>mediaPaths[id]=event.currentTarget.value} /></label>{/each}
  <button class="btn ghost sm" disabled={disabled||busy||nestedHistory.busy} onclick={()=>void download()}>Скачать проект Concat JSON</button>
  {#if busy}<p role="status">Проверяю экспорт Concat…</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
 </details>
{/if}
